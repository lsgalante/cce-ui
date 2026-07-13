//! Narrow-trait `Float3` (Phase 5t) — three labeled slider rows (X/Y/Z) with click-to-edit
//! numeric readouts, embedded by value inside `ParametersBg` (its only consumer), which drives
//! it through direct `WidgetHost` calls and reads the pub value/edit fields through `Deref`. The
//! model caches its laid-out rect ([`Layout::rect_assigned`] — `get_row_rects` is pub API with
//! no rect parameter), draws everything in [`Paint::paint`], and keeps the legacy drag surface
//! on the `Input` drag hooks. The readout click's legacy `focus::set_focused(self)` rides
//! `EventCtx::request_focus`.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, NamedKey, Paint,
    TextEditorState,
};

pub struct Float3 {
    rect: Rect,
    pub values: [f32; 3],
    pub(crate) mins: [f32; 3],
    pub(crate) maxs: [f32; 3],
    labels: [String; 3],
    label: Option<String>,
    dragging_idx: Option<usize>,
    drag_offset: f32,
    pub editing_idx: Option<usize>,
    pub edit_buffer: String,
}

impl Float3 {
    pub fn new() -> Adapted<Float3> {
        Adapted::new(Float3 {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            values: [0.5, 0.5, 0.5],
            mins: [0.0, 0.0, 0.0],
            maxs: [1.0, 1.0, 1.0],
            labels: ["X".to_string(), "Y".to_string(), "Z".to_string()],
            label: None,
            dragging_idx: None,
            drag_offset: 0.0,
            editing_idx: None,
            edit_buffer: String::new(),
        })
    }

    pub fn set_values(&mut self, values: [f32; 3]) {
        self.values = values;
    }

    pub fn get_row_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let mut rects = Vec::new();
        let by = self.rect.y + 20.0;
        for i in 0..3 {
            rects.push((self.rect.x + 8.0, by + 6.0 + i as f32 * 26.0, self.rect.width - 16.0, 20.0));
        }
        rects
    }

    /// Commit the in-flight readout edit back into the value — the legacy `unfocus` body.
    fn commit_edit(&mut self) {
        if let Some(i) = self.editing_idx.take() {
            if let Ok(new_val) = self.edit_buffer.parse::<f32>() {
                let range = self.maxs[i] - self.mins[i];
                if range != 0.0 {
                    self.values[i] = ((new_val - self.mins[i]) / range).clamp(0.0, 1.0);
                } else {
                    self.values[i] = 0.0;
                }
            }
        }
    }
}

impl Adapted<Float3> {
    pub fn with_values(mut self, values: [f32; 3]) -> Self {
        self.values = values;
        self
    }

    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.mins = [min, min, min];
        self.maxs = [max, max, max];
        self
    }
}

impl Layout for Float3 {
    /// The control label draws inside the rect (row one's header), like legacy.
    fn inline_label(&self) -> bool {
        true
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
    }
}

impl Paint for Float3 {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, _rect: Rect, ctx: &mut PaintCtx) {
        // Outline border box
        let bx = self.rect.x + 4.0;
        let bw = self.rect.width - 8.0;
        let by = self.rect.y + 20.0;
        let bh = 84.0;
        let border_color = [0.18, 0.18, 0.27, 1.0];
        let border_t = 1.0;

        ctx.quad(Rect { x: bx, y: by, width: bw, height: border_t }, border_color);
        ctx.quad(Rect { x: bx, y: by + bh - border_t, width: bw, height: border_t }, border_color);
        ctx.quad(Rect { x: bx, y: by, width: border_t, height: bh }, border_color);
        ctx.quad(Rect { x: bx + bw - border_t, y: by, width: border_t, height: bh }, border_color);

        if let Some(ref l) = self.label {
            ctx.text(l.clone(), self.rect.x + 8.0, self.rect.y + 2.0, 13.0, [0xee, 0xee, 0xf0]);
        }

        let rects = self.get_row_rects();
        for i in 0..3 {
            let r = rects[i];
            let track_x = self.rect.x + 100.0;
            let track_w = self.rect.width - 188.0;
            let track_y = r.1 + 4.0;
            let track_h = 12.0;

            ctx.quad(Rect { x: track_x, y: track_y, width: track_w, height: track_h }, colors::slider_track());

            let thumb_size = track_h * 0.9;
            let range = track_w - thumb_size;
            let thumb_x = track_x + self.values[i] * range;
            let thumb_color = if self.dragging_idx == Some(i) {
                colors::SLIDER_THUMB_DRAG
            } else {
                colors::SLIDER_THUMB
            };
            ctx.quad(
                Rect { x: thumb_x, y: track_y + (track_h - thumb_size) / 2.0, width: thumb_size, height: thumb_size },
                thumb_color,
            );

            let rx = self.rect.x + self.rect.width - 68.0;
            let bg_color = if self.editing_idx == Some(i) {
                [0.06, 0.10, 0.18, 1.0]
            } else {
                [0.10, 0.10, 0.13, 1.0]
            };
            ctx.quad(Rect { x: rx, y: track_y, width: 60.0, height: track_h }, bg_color);

            if self.editing_idx == Some(i) {
                let border_color = [0.20, 0.50, 0.85, 1.0];
                ctx.quad(Rect { x: rx, y: track_y, width: 60.0, height: border_t }, border_color);
                ctx.quad(Rect { x: rx, y: track_y + track_h - border_t, width: 60.0, height: border_t }, border_color);
                ctx.quad(Rect { x: rx, y: track_y, width: border_t, height: track_h }, border_color);
                ctx.quad(Rect { x: rx + 60.0 - border_t, y: track_y, width: border_t, height: track_h }, border_color);
            }

            // Row label + readout text (the legacy `text_labels` body).
            let ry = track_y;
            ctx.text(self.labels[i].clone(), self.rect.x + 16.0, ry - 2.0, 12.0, [0xaa, 0xaa, 0xbb]);
            let text = if self.editing_idx == Some(i) {
                self.edit_buffer.clone()
            } else {
                let scaled_val = self.mins[i] + self.values[i] * (self.maxs[i] - self.mins[i]);
                format!("{:.2}", scaled_val)
            };
            ctx.text(text, rx + 8.0, ry - 2.0, 12.0, [0xee, 0xee, 0xf0]);
        }
    }
}

impl Input for Float3 {
    fn draggable(&self, _rect: Rect) -> bool {
        self.dragging_idx.is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_idx.is_some()
    }

    fn drag_begin(&mut self, _px: f32, _py: f32, _rect: Rect) {}

    fn drag_update(&mut self, px: f32, _py: f32, _rect: Rect) -> bool {
        if let Some(i) = self.dragging_idx {
            let track_x = self.rect.x + 100.0;
            let track_w = self.rect.width - 188.0;
            let thumb_size = 12.0 * 0.9;
            let range = track_w - thumb_size;
            if range > 0.0 {
                let raw = (px - self.drag_offset - track_x) / range;
                let new_val = raw.clamp(0.0, 1.0);
                if (new_val - self.values[i]).abs() > 0.001 {
                    self.values[i] = new_val;
                    if self.editing_idx == Some(i) {
                        let scaled_val = self.mins[i] + self.values[i] * (self.maxs[i] - self.mins[i]);
                        self.edit_buffer = format!("{:.2}", scaled_val);
                    }
                    return true;
                }
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_idx = None;
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                let (state, px, py) = (*state, *px, *py);
                let rects = self.get_row_rects();

                for i in 0..3 {
                    let r = rects[i];
                    let rx = self.rect.x + self.rect.width - 68.0;
                    let ry = r.1 + 4.0;
                    let rh = 12.0;
                    let readout_w = 60.0;

                    if px >= rx && px <= rx + readout_w && py >= ry && py <= ry + rh {
                        if state == ElementState::Pressed {
                            if self.editing_idx != Some(i) {
                                self.commit_edit();
                                self.editing_idx = Some(i);
                                let scaled_val = self.mins[i] + self.values[i] * (self.maxs[i] - self.mins[i]);
                                self.edit_buffer = format!("{:.2}", scaled_val);
                                ectx.request_focus();
                            }
                        }
                        return true;
                    }
                }

                if state == ElementState::Pressed {
                    for i in 0..3 {
                        let r = rects[i];
                        let track_x = self.rect.x + 100.0;
                        let track_w = self.rect.width - 188.0;
                        let track_y = r.1 + 4.0;
                        let track_h = 12.0;
                        let thumb_size = track_h * 0.9;
                        let range = track_w - thumb_size;
                        let thumb_x = track_x + self.values[i] * range;

                        if px >= track_x && px <= track_x + track_w && py >= track_y && py <= track_y + track_h {
                            self.dragging_idx = Some(i);
                            self.drag_offset = px - thumb_x;
                            return true;
                        }
                    }
                } else if state == ElementState::Released {
                    if self.dragging_idx.is_some() {
                        self.dragging_idx = None;
                        return true;
                    }
                }
                false
            }
            Event::KeyInput(event) => {
                if self.editing_idx.is_none() {
                    return false;
                }
                if event.state != ElementState::Pressed {
                    return false;
                }

                let mut state = TextEditorState {
                    buffer: self.edit_buffer.clone(),
                    cursor_idx: self.edit_buffer.chars().count(),
                    select_anchor: None,
                    all_selected: false,
                };

                let mut handled = false;
                match &event.logical_key {
                    Key::Named(NamedKey::Backspace) => {
                        state.delete_backwards();
                        handled = true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        self.commit_edit();
                        handled = true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.editing_idx = None;
                        handled = true;
                    }
                    Key::Character(s) => {
                        for ch in s.chars() {
                            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && state.buffer.is_empty()) {
                                state.insert_text(&ch.to_string());
                            }
                        }
                        handled = true;
                    }
                    _ => {}
                }

                if self.editing_idx.is_some() {
                    self.edit_buffer = state.buffer;
                }
                handled
            }
            Event::FocusOut => {
                self.commit_edit();
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::WidgetHost;

    /// The ParametersBg drive pattern: readout click opens the edit, Enter/unfocus commits
    /// back into the normalized value, track press starts a drag.
    #[test]
    fn readout_edit_commits_on_unfocus() {
        let mut ctx = UiContext::new();
        let mut f = Float3::new().with_values([0.5, 0.5, 0.5]).with_range(0.0, 10.0);
        WidgetHost::set_rect(&mut f, 0.0, 0.0, 300.0, 108.0);

        let rows = f.get_row_rects();
        assert_eq!(rows.len(), 3);
        // Click row 1's readout (x within [w-68, w-8], y within row top+4..+16).
        let rx = 300.0 - 68.0 + 5.0;
        let ry = rows[1].1 + 8.0;
        assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, rx, ry, &mut ctx));
        assert_eq!(f.editing_idx, Some(1));
        assert_eq!(f.edit_buffer, "5.00");

        f.edit_buffer = "7.5".to_string();
        WidgetHost::unfocus(&mut f);
        assert_eq!(f.editing_idx, None);
        assert!((f.values[1] - 0.75).abs() < 1e-4, "7.5 of 0..10 normalizes to 0.75");

        // Track press starts a drag; drag_update moves the value; release ends it.
        let track_y = rows[0].1 + 8.0;
        assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, 150.0, track_y, &mut ctx));
        assert!(WidgetHost::is_dragging(&f));
        f.drag_update(260.0, track_y);
        assert!(f.values[0] > 0.5, "drag right raises the value");
        f.drag_end();
        assert!(!WidgetHost::is_dragging(&f));
    }
}
