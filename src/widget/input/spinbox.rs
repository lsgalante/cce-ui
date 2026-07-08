//! Narrow-trait `Spinbox` (Phase 5i). Slider-style label convention (no rect inflation; label
//! eats into the assigned rect, side-label inset computed from the synced label). Sub-zone
//! hover (the -/+ buttons) is tracked from `PointerMove` against the content rect; a click on
//! the display area enters edit mode and takes focus via `EventCtx::request_focus`.

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, Control, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, NamedKey,
    Paint, TextEditorState,
};

fn side_offset(label: &Option<String>) -> f32 {
    if crate::layout::control_label_layout() == "side" && label.is_some() {
        90.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct Spinbox {
    pub value: i32,
    pub(crate) min: i32,
    pub(crate) max: i32,
    pub(crate) step: i32,
    pub editing: bool,
    pub edit_buffer: String,
    pub cursor_idx: usize,
    hover_dec: bool,
    hover_inc: bool,
    hovered: bool,
    unit: Option<String>,
    pub decimals: u32,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    label: Option<String>,
}

/// The zone geometry shared by paint and input, derived from the content rect.
struct SpinGeom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    split_dec: f32,
    btn_y: f32,
    btn_h: f32,
    btn_w: f32,
    pad: f32,
}

impl Spinbox {
    pub fn new(value: i32, min: i32, max: i32, step: i32) -> Adapted<Spinbox> {
        Adapted::new(Spinbox {
            value,
            min,
            max,
            step,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            hover_dec: false,
            hover_inc: false,
            hovered: false,
            unit: None,
            decimals: 0,
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            label: None,
        })
    }

    pub fn set_unit(&mut self, unit: &str) {
        self.unit = Some(unit.to_string());
    }

    pub fn range(&self) -> (i32, i32) {
        (self.min, self.max)
    }

    fn geom(&self, rect: Rect) -> SpinGeom {
        let side = side_offset(&self.label);
        let x = rect.x + side;
        let w = rect.width - side;
        let pad = crate::layout::spinbox_button_padding();
        SpinGeom {
            x,
            y: rect.y,
            w,
            h: rect.height,
            split_dec: x + w * 0.55,
            btn_y: rect.y + pad,
            btn_h: (rect.height - 2.0 * pad).max(0.0),
            btn_w: ((w * 0.45 - 2.0 * pad).max(0.0)) / 2.0,
            pad,
        }
    }

    fn value_text(&self) -> String {
        if self.editing {
            self.edit_buffer.clone()
        } else if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize)
        } else {
            self.value.to_string()
        }
    }

    fn formatted_value(&self) -> String {
        if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize)
        } else {
            self.value.to_string()
        }
    }

    fn parse_into_value(&mut self, text: &str) {
        let old_val = self.value;
        if self.decimals > 0 {
            if let Ok(val_f) = text.parse::<f32>() {
                let divisor = 10.0f32.powi(self.decimals as i32);
                self.value = ((val_f * divisor).round() as i32).clamp(self.min, self.max);
            }
        } else if let Ok(val) = text.parse::<i32>() {
            self.value = val.clamp(self.min, self.max);
        }
        if self.value != old_val {
            self.just_changed = true;
        }
    }

    fn begin_edit(&mut self, cursor_at_end: bool) {
        self.editing = true;
        self.edit_buffer = self.formatted_value();
        if cursor_at_end {
            self.cursor_idx = self.edit_buffer.chars().count();
        }
    }
}

impl Adapted<Spinbox> {
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.set_unit(unit);
        self
    }

    pub fn with_decimals(mut self, decimals: u32) -> Self {
        self.decimals = decimals;
        self
    }
}

impl Layout for Spinbox {
    fn inflates_label_rect(&self) -> bool {
        false
    }

    fn layout_ignore(&self) -> bool {
        true
    }

    fn detached_label_inset(&self) -> f32 {
        4.0 // legacy Control::control_label x offset
    }

    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::spinbox_height()))
    }
}

impl Paint for Spinbox {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::spinbox_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let g = self.geom(rect);
        let radius = crate::layout::spinbox_corner_radius();
        let rounded = radius > 0.0;
        let display_bg = if self.editing { [0.06, 0.10, 0.18, 1.0] } else { colors::spinbox_display() };
        let inc_col = if self.hover_inc { colors::spinbox_button_hover() } else { colors::spinbox_button() };
        let dec_col = if self.hover_dec { colors::spinbox_button_hover() } else { colors::spinbox_button() };

        if rounded {
            let rc = (true, true, true, true);
            let border_color = if self.editing {
                [0.20, 0.50, 0.85, 1.0]
            } else if self.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            ctx.rounded_rect(Rect { x: g.x, y: g.y, width: g.w, height: g.h }, radius, rc, border_color);
            ctx.rounded_rect(
                Rect { x: g.x + 1.0, y: g.y + 1.0, width: g.w - 2.0, height: g.h - 2.0 },
                radius - 1.0,
                rc,
                display_bg,
            );
            if g.btn_h > 0.0 && g.btn_w > 0.0 {
                ctx.rounded_rect(
                    Rect { x: g.split_dec + g.pad, y: g.btn_y, width: g.btn_w, height: g.btn_h },
                    0.0,
                    (false, false, false, false),
                    dec_col,
                );
                ctx.rounded_rect(
                    Rect { x: g.split_dec + g.pad + g.btn_w, y: g.btn_y, width: g.btn_w, height: g.btn_h },
                    radius,
                    (false, true, true, false),
                    inc_col,
                );
            }
            if self.editing {
                let char_width = 8.4;
                let cursor_x = (g.x + 4.0 + self.cursor_idx as f32 * char_width).min(g.x + g.w * 0.55 - 4.0);
                let cursor_y = g.y + (g.h - 14.0) / 2.0;
                ctx.rounded_rect(
                    Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 },
                    0.0,
                    (false, false, false, false),
                    [0.80, 0.80, 0.85, 1.0],
                );
            }
        } else {
            ctx.quad(Rect { x: g.x, y: g.y, width: g.w, height: g.h }, display_bg);
            if g.btn_h > 0.0 && g.btn_w > 0.0 {
                ctx.quad(Rect { x: g.split_dec + g.pad, y: g.btn_y, width: g.btn_w, height: g.btn_h }, dec_col);
                ctx.quad(Rect { x: g.split_dec + g.pad + g.btn_w, y: g.btn_y, width: g.btn_w, height: g.btn_h }, inc_col);
            }
            if self.editing {
                let border_color = [0.20, 0.50, 0.85, 1.0];
                ctx.quad(Rect { x: g.x, y: g.y, width: g.w, height: 1.0 }, border_color);
                ctx.quad(Rect { x: g.x, y: g.y + g.h - 1.0, width: g.w, height: 1.0 }, border_color);
                ctx.quad(Rect { x: g.x, y: g.y, width: 1.0, height: g.h }, border_color);
                ctx.quad(Rect { x: g.x + g.w - 1.0, y: g.y, width: 1.0, height: g.h }, border_color);

                let char_width = 8.4;
                let cursor_x = (g.x + 4.0 + self.cursor_idx as f32 * char_width).min(g.x + g.w * 0.55 - 4.0);
                let cursor_y = g.y + (g.h - 14.0) / 2.0;
                ctx.quad(Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 }, [0.80, 0.80, 0.85, 1.0]);
            }
        }

        // Value, unit, and -/+ glyphs.
        let tc = colors::spinbox_text_color();
        let text_color = [(tc[0] * 255.0) as u8, (tc[1] * 255.0) as u8, (tc[2] * 255.0) as u8];
        ctx.text(self.value_text(), g.x + 4.0, crate::layout::align_text_y(g.y, g.h, 14.0, 0.0), 14.0, text_color);
        if let Some(ref unit) = self.unit {
            ctx.text(unit.clone(), g.x + 4.0 + 36.0, crate::layout::align_text_y(g.y, g.h, 11.0, 0.0), 11.0, [0x73, 0x73, 0x7a]);
        }
        if g.btn_w > 0.0 {
            let dec_center_x = g.split_dec + g.pad + g.btn_w * 0.5;
            let inc_center_x = g.split_dec + g.pad + g.btn_w * 1.5;
            let ty = crate::layout::align_text_y(g.y, g.h, 12.0, 0.0);
            ctx.text("-".to_string(), dec_center_x - 4.0, ty, 12.0, text_color);
            ctx.text("+".to_string(), inc_center_x - 4.0, ty, 12.0, text_color);
        }
    }
}

impl Input for Spinbox {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was = self.hovered;
                self.hovered = *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                if !self.hovered {
                    let changed = self.hover_dec || self.hover_inc;
                    self.hover_dec = false;
                    self.hover_inc = false;
                    return changed || was != self.hovered;
                }
                let g = self.geom(r);
                let in_y = *py >= g.btn_y && *py < g.btn_y + g.btn_h;
                let hd = in_y && *px >= g.split_dec + g.pad && *px < g.x + g.w * 0.775;
                let hi = in_y && *px >= g.x + g.w * 0.775 && *px < g.x + g.w - g.pad;
                let changed = hd != self.hover_dec || hi != self.hover_inc;
                self.hover_dec = hd;
                self.hover_inc = hi;
                changed || was != self.hovered
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, .. } => {
                let g = self.geom(ectx.rect);
                let in_y = *py >= g.btn_y && *py < g.btn_y + g.btn_h;
                if in_y && *px >= g.split_dec + g.pad && *px < g.x + g.w * 0.775 {
                    let old_val = self.value;
                    self.value = (self.value - self.step).max(self.min);
                    if self.value != old_val {
                        self.just_changed = true;
                    }
                    true
                } else if in_y && *px >= g.x + g.w * 0.775 && *px < g.x + g.w - g.pad {
                    let old_val = self.value;
                    self.value = (self.value + self.step).min(self.max);
                    if self.value != old_val {
                        self.just_changed = true;
                    }
                    true
                } else if *px < g.split_dec {
                    self.begin_edit(false);
                    let char_width = 8.4;
                    self.cursor_idx = (((px - (g.x + 4.0)) / char_width).round() as isize)
                        .max(0)
                        .min(self.edit_buffer.chars().count() as isize) as usize;
                    ectx.request_focus();
                    true
                } else {
                    false
                }
            }
            Event::KeyInput(key_event) => {
                if !self.editing || key_event.state != ElementState::Pressed {
                    return false;
                }
                let mut state = TextEditorState {
                    buffer: self.edit_buffer.clone(),
                    cursor_idx: self.cursor_idx,
                    select_anchor: None,
                    all_selected: false,
                };
                let mut handled = false;
                match &key_event.logical_key {
                    Key::Named(NamedKey::Backspace) => handled = state.delete_backwards(),
                    Key::Named(NamedKey::Delete) => handled = state.delete_forwards(),
                    Key::Named(NamedKey::ArrowLeft) => handled = state.move_cursor_left(false),
                    Key::Named(NamedKey::ArrowRight) => handled = state.move_cursor_right(false),
                    Key::Named(NamedKey::Enter) => {
                        let text = state.buffer.clone();
                        self.parse_into_value(&text);
                        self.editing = false;
                        handled = true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.editing = false;
                        handled = true;
                    }
                    _ => {
                        if let Some(text) = &key_event.text {
                            for ch in text.chars() {
                                match ch {
                                    '-' if state.cursor_idx == 0 && !state.buffer.starts_with('-') => {
                                        state.insert_text("-");
                                        handled = true;
                                    }
                                    '.' if self.decimals > 0 && !state.buffer.contains('.') => {
                                        state.insert_text(".");
                                        handled = true;
                                    }
                                    '0'..='9' => {
                                        state.insert_text(&ch.to_string());
                                        handled = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                if self.editing {
                    self.edit_buffer = state.buffer;
                    self.cursor_idx = state.cursor_idx;
                }
                handled
            }
            // Focus gained programmatically enters edit mode (legacy `focus()` override);
            // focus loss commits (legacy `unfocus`).
            Event::FocusIn => {
                self.begin_edit(true);
                false
            }
            Event::FocusOut => {
                if self.editing {
                    self.editing = false;
                    let text = self.edit_buffer.clone();
                    self.parse_into_value(&text);
                }
                false
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.formatted_value())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let old_val = self.value;
        self.parse_into_value(val.trim());
        if self.value != old_val {
            if self.editing {
                self.edit_buffer = self.formatted_value();
                self.cursor_idx = self.edit_buffer.chars().count();
            }
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        self.value
    }
}

impl Control for Adapted<Spinbox> {
    fn set_label(&mut self, label: &str) {
        Adapted::set_label(self, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Element, UiContext};

    #[test]
    fn spinbox_button_zones_step_the_value() {
        let mut ctx = UiContext::new();
        let mut sb = Spinbox::new(0, -100, 100, 1);
        let (id, ptr) = (sb.id(), sb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        Element::set_rect(&mut sb, 10.0, 20.0, 100.0, 26.0);

        // Legacy test: click at (75, 33) lands in the decrement zone.
        assert!(Element::mouse_input(&mut sb, MouseButton::Left, ElementState::Pressed, 75.0, 33.0, &mut ctx));
        assert_eq!(sb.value, -1);
        assert!(Element::take_change(&mut sb));

        // Increment zone (past 77.5% of the width).
        assert!(Element::mouse_input(&mut sb, MouseButton::Left, ElementState::Pressed, 92.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 0);
    }

    #[test]
    fn spinbox_value_string_decimals_round_trip() {
        let mut sb = Spinbox::new(150, 0, 1000, 5).with_decimals(2);
        assert_eq!(Element::get_value_string(&sb), Some("1.50".to_string()));
        assert!(Element::set_value_string(&mut sb, "2.75"));
        assert_eq!(sb.value, 275);
        assert_eq!(Element::value(&sb), 275);
    }
}
