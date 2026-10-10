//! The routing, driven with no window system: a mock app records what
//! reaches it.
use super::*;
use crate::backend::app::{AppSender, WindowSettings};

#[derive(Debug, Clone, PartialEq)]
enum Seen {
    Move(f32, f32),
    Button(MouseButton, ElementState, f32, f32),
    Wheel(MouseScrollDelta),
    Key(Key, bool),
    Focus(bool),
    Undo,
    Update(u32),
}

struct Mock {
    seen: Vec<Seen>,
    csd: bool,
    takes_undo: bool,
    /// A press makes this message, which `update` records.
    press_msg: Option<u32>,
    /// A widget tree, for the Tab walk; none by default, as most apps here.
    ctx: Option<crate::context::UiContext>,
}

impl Application for Mock {
    type Message = u32;
    fn create(_: AppSender<u32>) -> Self {
        unreachable!("built directly")
    }
    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: String::new(),
            app_id: "mock".into(),
            width: 400,
            height: 300,
            fullscreen: false,
            min_size: None,
        }
    }
    fn update(&mut self, msg: u32, needs_rebuild: &mut bool, _exit: &mut bool) {
        self.seen.push(Seen::Update(msg));
        *needs_rebuild = true;
    }
    fn tick(&mut self, _dt: f32, _needs_rebuild: &mut bool) {}
    fn handle_pointer_move(&mut self, pos: LogicalPosition, _: &mut bool) {
        self.seen.push(Seen::Move(pos.x, pos.y));
    }
    fn handle_mouse_input(
        &mut self,
        button: MouseButton,
        state: ElementState,
        pos: LogicalPosition,
        _: &mut bool,
    ) -> Option<u32> {
        self.seen.push(Seen::Button(button, state, pos.x, pos.y));
        if state == ElementState::Pressed { self.press_msg } else { None }
    }
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta, _: LogicalPosition, _: &mut bool) {
        self.seen.push(Seen::Wheel(*delta));
    }
    fn handle_key_input(&mut self, event: &KeyEvent, _: &mut bool) -> Option<u32> {
        self.seen.push(Seen::Key(event.logical_key.clone(), event.repeat));
        None
    }
    fn handle_focus_change(&mut self, focused: bool, _: &mut bool) {
        self.seen.push(Seen::Focus(focused));
    }
    fn undo(&mut self, _: &mut bool) -> bool {
        self.seen.push(Seen::Undo);
        self.takes_undo
    }
    fn csd_resize_borders(&self) -> bool {
        self.csd
    }
    fn csd_titlebar_move(&self) -> bool {
        self.csd
    }
    fn ui_context_mut(&mut self) -> Option<&mut crate::context::UiContext> {
        self.ctx.as_mut()
    }
}

fn mock() -> Mock {
    Mock { seen: Vec::new(), csd: false, takes_undo: false, press_msg: None, ctx: None }
}

/// The Tab walk is on unless an app says otherwise. Tab leaves a one-line field for the
/// next stop and never reaches the app; a multi-line box that is editing keeps it (the
/// app's key handling, which forwards it to the box, sees it) until Ctrl+Tab moves on.
#[test]
fn tab_walks_the_stops_but_a_multi_line_box_keeps_it() {
    use crate::widget::{TextBox, WidgetHost, WidgetHostExt};
    let tab = Key::Named(NamedKey::Tab);
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    let mut ctx = crate::context::UiContext::new();
    let name = ctx.insert(TextBox::new(String::new()));
    ctx[name].set_rect(10.0, 10.0, 200.0, 24.0);
    let notes = ctx.insert(TextBox::new(String::new()).with_multiline(true));
    ctx[notes].set_rect(10.0, 50.0, 200.0, 120.0);
    let save = ctx.insert(crate::widget::Button::new(10.0, 200.0, 80.0, 24.0).with_label("Save"));
    app.ctx = Some(ctx);
    assert!(app.plate_navigation(), "on by default");
    let focused = |app: &mut Mock| app.ctx.as_ref().unwrap().focused_widget;

    d.key(turn(&mut app, &mut f), tab.clone(), Some("\t".into()), ElementState::Pressed);
    assert_eq!(focused(&mut app), Some(name.id()), "the first stop");
    d.key(turn(&mut app, &mut f), tab.clone(), Some("\t".into()), ElementState::Pressed);
    assert_eq!(focused(&mut app), Some(notes.id()), "Tab leaves a one-line field");
    assert!(!app.seen.iter().any(|s| matches!(s, Seen::Key(k, _) if *k == tab)), "the walk took both");
    assert!(app.ctx.as_ref().unwrap()[notes].keeps_tab(), "focused, the multi-line box is editing");

    d.key(turn(&mut app, &mut f), tab.clone(), Some("\t".into()), ElementState::Pressed);
    assert_eq!(focused(&mut app), Some(notes.id()), "the box keeps Tab");
    assert_eq!(app.seen.last(), Some(&Seen::Key(tab.clone(), false)), "and the app hands it on");

    d.set_modifiers(&mut app, Modifiers { ctrl: true, shift: false, alt: false, logo: false });
    d.key(turn(&mut app, &mut f), tab.clone(), None, ElementState::Pressed);
    assert_eq!(focused(&mut app), Some(save.id()), "Ctrl+Tab leaves it");
}

/// A driver with known chords, whatever the machine's input.kdl says.
fn driver() -> Driver {
    Driver {
        undo_chord: "ctrl+z".into(),
        redo_chord: "ctrl+shift+z".into(),
        group_next_chord: "ctrl+tab".into(),
        group_prev_chord: "ctrl+shift+tab".into(),
        ..Driver::new()
    }
}

struct Flags {
    redraw: bool,
    exit: bool,
}

fn turn<'a>(app: &'a mut Mock, f: &'a mut Flags) -> Turn<'a, Mock> {
    Turn { app, redraw: &mut f.redraw, exit: &mut f.exit }
}

const SIZE: LogicalSize = LogicalSize { width: 400.0, height: 300.0 };

fn site(can_grab: bool) -> PressSite {
    PressSite { size: SIZE, on_popup: false, can_grab, own_edges: false }
}

#[test]
fn a_lost_pointer_releases_what_was_held_then_clears_hover() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    let at = LogicalPosition::new(50.0, 60.0);
    d.cursor_pos = (50.0, 60.0);
    d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, at, site(false));
    d.pointer_press(turn(&mut app, &mut f), MouseButton::Middle, at, site(false));
    assert_eq!(d.buttons_down, 1 | 4);
    app.seen.clear();

    d.pointer_leave(turn(&mut app, &mut f));
    assert_eq!(
        app.seen,
        vec![
            Seen::Button(MouseButton::Left, ElementState::Released, 50.0, 60.0),
            Seen::Button(MouseButton::Middle, ElementState::Released, 50.0, 60.0),
            Seen::Move(-10000.0, -10000.0),
        ]
    );
    assert_eq!(d.buttons_down, 0);
}

#[test]
fn a_press_on_the_border_is_the_windows_only_when_the_shell_can_grab() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    app.csd = true;
    let corner = LogicalPosition::new(2.0, 2.0);
    let r = d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, site(true));
    assert_eq!(r, Press::Resize(ResizeEdge::TopLeft));
    assert!(app.seen.is_empty(), "a grab never reaches the app: {:?}", app.seen);

    let band = LogicalPosition::new(100.0, 20.0);
    let r = d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, band, site(true));
    assert_eq!(r, Press::Move);

    // No grab to start (a layer surface, a shell without grabs): the app's.
    let r = d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, site(false));
    assert_eq!(r, Press::Dispatched);
    assert_eq!(app.seen, vec![Seen::Button(MouseButton::Left, ElementState::Pressed, 2.0, 2.0)]);

    // A window system that resizes from its own edges: the band is no
    // grab, and the press is the app's.
    app.seen.clear();
    let own = PressSite { own_edges: true, ..site(true) };
    assert_eq!(d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, own), Press::Dispatched);
    assert_eq!(app.seen, vec![Seen::Button(MouseButton::Left, ElementState::Pressed, 2.0, 2.0)]);
    // But the titlebar band still moves the window.
    assert_eq!(d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, band, own), Press::Move);

    // Nor is a press through the menu popup ever a grab.
    app.seen.clear();
    let popup = PressSite { on_popup: true, ..site(true) };
    assert_eq!(d.pointer_press(turn(&mut app, &mut f), MouseButton::Left, corner, popup), Press::Dispatched);
    assert_eq!(app.seen.len(), 1);
}

#[test]
fn a_pressed_message_reaches_update_and_asks_for_a_frame() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    app.press_msg = Some(7);
    d.pointer_press(turn(&mut app, &mut f), MouseButton::Right, LogicalPosition::new(9.0, 9.0), site(true));
    assert_eq!(app.seen.last(), Some(&Seen::Update(7)));
    assert!(f.redraw);
}

#[test]
fn a_commit_is_typed_text_and_never_a_shortcut_or_a_held_key() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    app.takes_undo = true;
    // Ctrl held as the input method commits "z": typed, not an undo.
    d.set_modifiers(&mut app, Modifiers { ctrl: true, ..Modifiers::default() });
    d.commit_text(turn(&mut app, &mut f), "z".into());
    d.commit_text(turn(&mut app, &mut f), "日本".into());
    d.commit_text(turn(&mut app, &mut f), String::new());
    let typed = |t: &str| Seen::Key(Key::Character(t.into()), false);
    assert_eq!(app.seen, vec![typed("z"), typed("z"), typed("日本"), typed("日本")]);
    assert!(d.pressed_key.is_none(), "nothing left held to repeat");
    assert!(f.redraw);
}

#[test]
fn the_undo_chord_goes_to_the_app_hook_before_key_dispatch() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    d.set_modifiers(&mut app, Modifiers { ctrl: true, ..Modifiers::default() });
    app.takes_undo = true;
    d.key(turn(&mut app, &mut f), Key::Character("z".into()), None, ElementState::Pressed);
    assert_eq!(app.seen, vec![Seen::Undo]);
    assert!(f.redraw);

    // Declined by the app, the key is dispatched as usual.
    app.seen.clear();
    app.takes_undo = false;
    d.key(turn(&mut app, &mut f), Key::Character("z".into()), None, ElementState::Pressed);
    assert_eq!(app.seen, vec![Seen::Undo, Seen::Key(Key::Character("z".into()), false)]);
}

#[test]
fn a_held_key_repeats_after_the_delay_and_stops_on_release() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    let bs = Key::Named(NamedKey::Backspace);
    d.key(turn(&mut app, &mut f), bs.clone(), None, ElementState::Pressed);
    app.seen.clear();

    // Inside the delay: nothing.
    d.repeat_keys(turn(&mut app, &mut f));
    assert!(app.seen.is_empty());

    // Past it: one repeat per turn that is an interval on.
    let pk = d.pressed_key.as_mut().unwrap();
    pk.first_pressed -= KEY_REPEAT_DELAY;
    pk.last_repeated -= KEY_REPEAT_INTERVAL;
    d.repeat_keys(turn(&mut app, &mut f));
    d.repeat_keys(turn(&mut app, &mut f));
    assert_eq!(app.seen, vec![Seen::Key(bs.clone(), true)]);

    d.key(turn(&mut app, &mut f), bs, None, ElementState::Released);
    assert!(d.pressed_key.is_none());
}

#[test]
fn losing_focus_drops_the_held_key_and_modifiers() {
    let (mut d, mut app, mut f) = (driver(), mock(), Flags { redraw: false, exit: false });
    d.set_modifiers(&mut app, Modifiers { ctrl: true, shift: true, alt: true, logo: true });
    d.key(turn(&mut app, &mut f), Key::Character("a".into()), Some("a".into()), ElementState::Pressed);
    assert!(d.pressed_key.is_some());

    d.keyboard_focus(turn(&mut app, &mut f), false);
    assert!(d.pressed_key.is_none());
    // Logo is left as it was, as the runner always left it.
    assert_eq!(d.mods, Modifiers { ctrl: false, shift: false, alt: false, logo: true });
    assert_eq!(app.seen.last(), Some(&Seen::Focus(false)));
}

#[test]
fn a_finger_is_the_left_button_and_never_the_windows() {
    // Only the tap and the hold: a touch scroll publishes the scroll
    // phase process-wide, which a parallel suite must not race.
    use crate::backend::touch::TouchAction::*;
    let (mut app, mut d) = (mock(), driver());
    app.csd = true;
    app.press_msg = Some(7);
    let mut f = Flags { redraw: false, exit: false };
    // A tap on the resize border: a click there, not a resize.
    d.touch(turn(&mut app, &mut f), vec![Hover(2.0, 2.0), Press(2.0, 2.0), Release(2.0, 2.0), Leave], None);
    assert_eq!(
        app.seen,
        vec![
            Seen::Move(2.0, 2.0),
            Seen::Button(MouseButton::Left, ElementState::Pressed, 2.0, 2.0),
            Seen::Update(7),
            Seen::Button(MouseButton::Left, ElementState::Released, 2.0, 2.0),
            Seen::Move(-10000.0, -10000.0),
        ]
    );
    assert!(f.redraw, "the press's message asked for a frame");
    // A hold, then a drag: the held left button's motion.
    app.seen.clear();
    d.touch(turn(&mut app, &mut f), vec![Press(10.0, 10.0), Drag(40.0, 10.0), Release(40.0, 10.0)], None);
    assert_eq!(app.seen[2], Seen::Move(40.0, 10.0));
    assert_eq!(d.buttons_down, 0, "a finger holds no pointer button");
}

#[test]
fn a_scroll_frame_is_its_phase_and_delta() {
    // The pure half of `scroll`: the dispatch itself publishes the phase
    // process-wide, which a parallel suite must not race.
    use crate::input::ScrollFactors;
    use crate::widget::ScrollPhase;
    let unit = ScrollFactors { mouse: 1.0, trackpad: 1.0 };
    let notch = ScrollFrame { v: 15.0, discrete_v: 1, source: Some(ScrollSource::Wheel), ..ScrollFrame::default() };
    assert_eq!(scroll_delta(&notch, unit), (ScrollPhase::Wheel, MouseScrollDelta::LineDelta(-0.0, -1.0)));

    let finger = ScrollFrame { v: 4.0, source: Some(ScrollSource::Finger), ..ScrollFrame::default() };
    assert_eq!(
        scroll_delta(&finger, ScrollFactors { mouse: 1.0, trackpad: 2.0 }),
        (ScrollPhase::Finger, MouseScrollDelta::PixelDelta(Position { x: -0.0, y: -8.0 }))
    );
    // No source named is a finger too; a lift with no delta ends it.
    let unnamed = ScrollFrame { h: 3.0, ..ScrollFrame::default() };
    assert_eq!(scroll_delta(&unnamed, unit).0, ScrollPhase::Finger);
    let lift = ScrollFrame { stop: true, source: Some(ScrollSource::Finger), ..ScrollFrame::default() };
    assert_eq!(scroll_delta(&lift, unit).0, ScrollPhase::FingerEnd);
    // A tilt wheel, or anything newer, glides like a wheel.
    let tilt = ScrollFrame { h: 5.0, source: Some(ScrollSource::WheelTilt), ..ScrollFrame::default() };
    assert_eq!(scroll_delta(&tilt, unit).0, ScrollPhase::Wheel);
}
