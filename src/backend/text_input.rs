// text-input-v3 (zwp_text_input_v3): the seat's text input follows the
// frame's claim (`crate::text_input`).
//
// While some widget claims a field, the text input is enabled and carries
// the caret's rectangle; the first frame nobody claims, it is disabled. The
// compositor only listens while this client holds keyboard focus (between
// `enter` and `leave`), so the last frame's claim is kept and replayed on
// `enter`, and `leave` disables: wlroots keeps a text input's enabled state
// across a leave, and a stale "enabled" would turn the next enable into a
// plain commit the compositor ignores.
//
// Nothing here types. Keys arrive over wl_keyboard as before (the on-screen
// keyboard is a virtual keyboard); a `commit_string` from an input method is
// delivered to the app as one typed key (`EngineState::type_text`).

use smithay_client_toolkit::reexports::client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};

use super::window_runner::{Application, EngineState};

/// A request the sync wants sent, in order. `Commit` closes each batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Enable,
    Disable,
    CursorRectangle([i32; 4]),
    Commit,
}

/// What the compositor has been told, and what the app wants. Pure, so the
/// enter/leave/claim interplay is tested without a compositor.
#[derive(Debug, Default)]
pub struct Sync {
    entered: bool,
    enabled: bool,
    sent_rect: Option<[i32; 4]>,
    wanted: Option<[i32; 4]>,
}

impl Sync {
    /// A frame was built; `claim` is what it claimed.
    pub fn frame(&mut self, claim: Option<[f32; 4]>) -> Vec<Request> {
        self.wanted = claim.map(|[x, y, w, h]| [x.round() as i32, y.round() as i32, w.round().max(1.0) as i32, h.round().max(1.0) as i32]);
        self.apply()
    }

    pub fn enter(&mut self) -> Vec<Request> {
        self.entered = true;
        self.apply()
    }

    pub fn leave(&mut self) -> Vec<Request> {
        let out = if self.enabled { vec![Request::Disable, Request::Commit] } else { Vec::new() };
        self.entered = false;
        self.enabled = false;
        self.sent_rect = None;
        out
    }

    fn apply(&mut self) -> Vec<Request> {
        if !self.entered {
            return Vec::new();
        }
        let mut out = Vec::new();
        match self.wanted {
            Some(rect) => {
                if !self.enabled {
                    // `enable` resets the state, so the rectangle follows it.
                    out.push(Request::Enable);
                    self.enabled = true;
                    self.sent_rect = None;
                }
                if self.sent_rect != Some(rect) {
                    out.push(Request::CursorRectangle(rect));
                    self.sent_rect = Some(rect);
                }
            }
            None => {
                if self.enabled {
                    out.push(Request::Disable);
                    self.enabled = false;
                    self.sent_rect = None;
                }
            }
        }
        if !out.is_empty() {
            out.push(Request::Commit);
        }
        out
    }
}

/// The seat's text input and its sync.
pub struct TextInput {
    proxy: ZwpTextInputV3,
    sync: Sync,
    /// An input method's `commit_string`, held until its `done`.
    pending_commit: Option<String>,
}

impl TextInput {
    pub fn new<A: Application>(
        manager: &ZwpTextInputManagerV3,
        seat: &smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat,
        qh: &QueueHandle<EngineState<A>>,
    ) -> Self {
        TextInput { proxy: manager.get_text_input(seat, qh, ()), sync: Sync::default(), pending_commit: None }
    }

    pub fn frame(&mut self, claim: Option<[f32; 4]>) {
        let requests = self.sync.frame(claim);
        self.send(&requests);
    }

    fn send(&self, requests: &[Request]) {
        for r in requests {
            match *r {
                Request::Enable => {
                    self.proxy.enable();
                    self.proxy.set_content_type(
                        zwp_text_input_v3::ContentHint::empty(),
                        zwp_text_input_v3::ContentPurpose::Normal,
                    );
                }
                Request::Disable => self.proxy.disable(),
                Request::CursorRectangle([x, y, w, h]) => self.proxy.set_cursor_rectangle(x, y, w, h),
                Request::Commit => self.proxy.commit(),
            }
        }
    }
}

impl Drop for TextInput {
    fn drop(&mut self) {
        self.proxy.destroy();
    }
}

impl<A: Application> Dispatch<ZwpTextInputManagerV3, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpTextInputManagerV3,
        _event: <ZwpTextInputManagerV3 as smithay_client_toolkit::reexports::client::Proxy>::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
    }
}

impl<A: Application> Dispatch<ZwpTextInputV3, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &ZwpTextInputV3,
        event: zwp_text_input_v3::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        let Some(ti) = state.text_input.as_mut() else { return };
        match event {
            zwp_text_input_v3::Event::Enter { .. } => {
                let requests = ti.sync.enter();
                ti.send(&requests);
            }
            zwp_text_input_v3::Event::Leave { .. } => {
                let requests = ti.sync.leave();
                ti.send(&requests);
                ti.pending_commit = None;
            }
            zwp_text_input_v3::Event::CommitString { text } => {
                ti.pending_commit = text;
            }
            zwp_text_input_v3::Event::Done { .. } => {
                if let Some(text) = ti.pending_commit.take().filter(|t| !t.is_empty()) {
                    state.type_text(text);
                }
            }
            // No preedit display and no surrounding text: an input method
            // composing in place is not supported, only its committed text.
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Request::*;
    use super::*;

    const R: [f32; 4] = [10.0, 20.0, 100.0, 30.0];
    const RI: [i32; 4] = [10, 20, 100, 30];

    #[test]
    fn nothing_is_sent_before_enter() {
        let mut s = Sync::default();
        assert!(s.frame(Some(R)).is_empty());
        assert_eq!(s.enter(), vec![Enable, CursorRectangle(RI), Commit], "the claim waits for enter");
    }

    #[test]
    fn a_claim_enables_once_and_follows_the_caret() {
        let mut s = Sync::default();
        s.enter();
        assert_eq!(s.frame(Some(R)), vec![Enable, CursorRectangle(RI), Commit]);
        assert!(s.frame(Some(R)).is_empty(), "an unchanged frame sends nothing");
        let moved = [12.0, 20.0, 100.0, 30.0];
        assert_eq!(s.frame(Some(moved)), vec![CursorRectangle([12, 20, 100, 30]), Commit]);
        assert_eq!(s.frame(None), vec![Disable, Commit]);
        assert!(s.frame(None).is_empty());
    }

    #[test]
    fn leave_disables_and_enter_restores() {
        let mut s = Sync::default();
        s.enter();
        s.frame(Some(R));
        assert_eq!(s.leave(), vec![Disable, Commit]);
        assert!(s.frame(Some(R)).is_empty(), "unfocused: the compositor is not listening");
        assert_eq!(s.enter(), vec![Enable, CursorRectangle(RI), Commit]);
    }

    #[test]
    fn leave_without_a_field_sends_nothing() {
        let mut s = Sync::default();
        s.enter();
        assert!(s.leave().is_empty());
    }
}
