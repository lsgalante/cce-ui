use crate::colors;
use crate::widget::*;

pub struct Checkbox {
    base: Widget,
    checked: bool,
    just_clicked: bool,
    pub just_changed: bool,
}

impl Checkbox {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            checked: false,
            just_clicked: false,
            just_changed: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }
}

impl Element for Checkbox {
    crate::impl_widget_base!(Checkbox);

    fn get_value_string(&self) -> Option<String> {
        Some(self.checked.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_trimmed = val.trim().to_lowercase();
        let new_checked = if val_trimmed == "true" || val_trimmed == "1" || val_trimmed == "yes" || val_trimmed == "on" {
            true
        } else if val_trimmed == "false" || val_trimmed == "0" || val_trimmed == "no" || val_trimmed == "off" {
            false
        } else {
            return false;
        };
        if self.checked != new_checked {
            self.checked = new_checked;
            self.just_changed = true;
            return true;
        }
        false
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn color(&self) -> [f32; 4] {
        let (_, _, w, _) = self.rect();
        if w > 30.0 {
            // Wide mode (with label) -> transparent widget background
            [0.0, 0.0, 0.0, 0.0]
        } else {
            // Standalone mode -> colored widget background
            if self.checked {
                colors::CHECKBOX_CHECKED
            } else if self.base.hovered {
                colors::CHECKBOX_HOVER
            } else {
                colors::CHECKBOX_BG
            }
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                ctx.handle_right_click(self.as_ptr(), px, py);
                return true;
            }
        }
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.checked = !self.checked;
                    self.just_clicked = true;
                    self.just_changed = true;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let (x, y, w, h) = self.rect();
        if w > 30.0 {
            // Draw box on the right
            let box_size = 18.0f32;
            let box_x = x + w - box_size - 8.0;
            let box_y = y + (h - box_size) / 2.0;

            // Box background
            let bg_color = if self.checked {
                colors::CHECKBOX_CHECKED
            } else if self.base.hovered {
                colors::CHECKBOX_HOVER
            } else {
                colors::CHECKBOX_BG
            };
            quads.push((box_x, box_y, box_size, box_size, bg_color));

            // Box border
            let border_color = if self.base.hovered {
                [0.35, 0.35, 0.40, 1.0]
            } else {
                [0.25, 0.25, 0.30, 1.0]
            };
            quads.push((box_x, box_y, box_size, 1.0, border_color));
            quads.push((box_x, box_y + box_size - 1.0, box_size, 1.0, border_color));
            quads.push((box_x, box_y, 1.0, box_size, border_color));
            quads.push((box_x + box_size - 1.0, box_y, 1.0, box_size, border_color));

            // Checked indicator
            if self.checked {
                let pad = 5.0f32;
                quads.push((box_x + pad, box_y + pad, box_size - 2.0 * pad, box_size - 2.0 * pad, [1.0, 1.0, 1.0, 0.9]));
            }
        } else {
            // Standalone mode -> centered checkmark inside the widget
            if self.checked {
                let pad_x = w * 0.25;
                let pad_y = h * 0.25;
                quads.push((x + pad_x, y + pad_y, w - 2.0 * pad_x, h - 2.0 * pad_y, [1.0, 1.0, 1.0, 0.9]));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            let font_size = 12.0;
            let y = crate::layout::align_text_y(self.base.y, self.base.h, font_size, 0.0);
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + 8.0,
                y,
                font_size,
                color: [0xcc, 0xcc, 0xd4],
            });
        }
        labels
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }
    fn value(&self) -> i32 { if self.checked { 1 } else { 0 } }
}

#[derive(Debug, Clone)]
pub struct Toggle {
    base: Widget,
    toggled: bool,
    just_toggled: bool,
}

impl Toggle {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            toggled: false,
            just_toggled: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
    }

    pub fn set_toggled(&mut self, v: bool) {
        self.toggled = v;
    }

    pub fn toggled(&self) -> bool {
        self.toggled
    }
}

impl Element for Toggle {
    crate::impl_widget_base!(Toggle);

    fn get_value_string(&self) -> Option<String> {
        Some(self.toggled.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_trimmed = val.trim().to_lowercase();
        let new_toggled = if val_trimmed == "true" || val_trimmed == "1" || val_trimmed == "yes" || val_trimmed == "on" {
            true
        } else if val_trimmed == "false" || val_trimmed == "0" || val_trimmed == "no" || val_trimmed == "off" {
            false
        } else {
            return false;
        };
        if self.toggled != new_toggled {
            self.toggled = new_toggled;
            self.just_toggled = true;
            return true;
        }
        false
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_toggled;
        self.just_toggled = false;
        ret
    }

    fn color(&self) -> [f32; 4] {
        if self.toggled {
            colors::TOGGLE_ON
        } else if self.base.hovered {
            colors::TOGGLE_HOVER
        } else {
            colors::TOGGLE_OFF
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.toggled = !self.toggled;
                    self.just_toggled = true;
                    return true;
                }
            }
            _ => {}
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        vec![(self.base.x, self.base.y, self.base.w, self.base.h, self.color())]
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            let font_size = 12.0;
            let est_w = TextLabel::estimate_width(label, font_size);
            labels.push(TextLabel {
                text: label.clone(),
                x: self.base.x + (self.base.w - est_w) / 2.0,
                y: crate::layout::align_text_y(self.base.y, self.base.h, font_size, 0.0),
                font_size,
                color: [0xcc, 0xcc, 0xd4],
            });
        }
        labels
    }

    fn take_click(&mut self) -> bool {
        if self.just_toggled { self.just_toggled = false; true } else { false }
    }
}

impl Control for Checkbox {}
impl Control for Toggle {}
