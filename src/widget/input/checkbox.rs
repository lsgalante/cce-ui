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

    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
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
                ctx.handle_right_click(self.as_ptr_mut(), px, py);
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

impl Drop for Checkbox {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
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

    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
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

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::toggle_height())
    }

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
        colors::toggle_bg_color()
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        let border_color = if self.toggled {
            colors::toggle_on_color()
        } else {
            colors::toggle_off_color()
        };
        let border_w = crate::layout::toggle_border_width();
        if border_w > 0.0 {
            Some((border_color, border_w))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::toggle_font())
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
        let mut quads = Vec::new();
        if self.corner_radius() <= 0.0 {
            quads.push((self.base.x, self.base.y, self.base.w, self.base.h, self.color()));
            
            let border_w = crate::layout::toggle_border_width();
            if border_w > 0.0 {
                let x = self.base.x;
                let y = self.base.y;
                let w = self.base.w;
                let h = self.base.h;
                let r = crate::layout::toggle_corner_radius();
                let t = border_w;
                
                let border_color = if self.toggled {
                    colors::toggle_on_color()
                } else {
                    colors::toggle_off_color()
                };
                
                let edge_h = ((h / 2.0) - r).max(0.0);
                
                if self.toggled {
                    // Top edge
                    quads.push((x + r, y, w - 2.0 * r, t, border_color));
                    // Top half of left edge
                    if edge_h > 0.0 {
                        quads.push((x, y + r, t, edge_h, border_color));
                    }
                    // Top half of right edge
                    if edge_h > 0.0 {
                        quads.push((x + w - t, y + r, t, edge_h, border_color));
                    }
                } else {
                    // Bottom edge
                    quads.push((x + r, y + h - t, w - 2.0 * r, t, border_color));
                    // Bottom half of left edge
                    if edge_h > 0.0 {
                        quads.push((x, y + h / 2.0, t, edge_h, border_color));
                    }
                    // Bottom half of right edge
                    if edge_h > 0.0 {
                        quads.push((x + w - t, y + h / 2.0, t, edge_h, border_color));
                    }
                }
            }
        }
        quads
    }

    fn extra_arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        let mut arcs = Vec::new();
        let border_w = crate::layout::toggle_border_width();
        let r = crate::layout::toggle_corner_radius();
        
        if border_w > 0.0 && r > 0.1 {
            let x = self.base.x;
            let y = self.base.y;
            let w = self.base.w;
            let h = self.base.h;
            let t = border_w;
            
            let border_color = if self.toggled {
                colors::toggle_on_color()
            } else {
                colors::toggle_off_color()
            };
            
            if self.toggled {
                // Top-Left corner arc
                arcs.push((
                    x + r, y + r, r, t,
                    std::f32::consts::PI, 1.5 * std::f32::consts::PI,
                    border_color
                ));
                // Top-Right corner arc
                arcs.push((
                    x + w - r, y + r, r, t,
                    1.5 * std::f32::consts::PI, 2.0 * std::f32::consts::PI,
                    border_color
                ));
            } else {
                // Bottom-Left corner arc
                arcs.push((
                    x + r, y + h - r, r, t,
                    0.5 * std::f32::consts::PI, std::f32::consts::PI,
                    border_color
                ));
                // Bottom-Right corner arc
                arcs.push((
                    x + w - r, y + h - r, r, t,
                    0.0, 0.5 * std::f32::consts::PI,
                    border_color
                ));
            }
        }
        arcs
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            let font_size = 12.0;
            let font_fam = crate::layout::toggle_font_parsed().0;
            let est_w = crate::widget::display::measure_text_width(label, &font_fam, font_size);
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

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::toggle_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::toggle_corner_radius()
    }

    fn all_rounded_quads(&self, _ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible() {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        let x = self.base.x;
        let y = self.base.y;
        let w = self.base.w;
        let h = self.base.h;
        let radius = self.corner_radius();
        let bg_color = self.color();
        
        if r1 || r2 || r3 || r4 {
            let border_w = crate::layout::toggle_border_width();
            let border_color = if self.toggled {
                colors::toggle_on_color()
            } else {
                colors::toggle_off_color()
            };
            
            if self.toggled {
                // Bottom half background
                quads.push((x, y + h / 2.0, w, h / 2.0, radius, bg_color, (false, false, true, true)));
                
                if border_w > 0.0 {
                    // Top half border
                    quads.push((x, y, w, h / 2.0, radius, border_color, (true, true, false, false)));
                    // Top half inset background
                    let inner_radius = (radius - border_w).max(0.0);
                    quads.push((x + border_w, y + border_w, w - 2.0 * border_w, h / 2.0 - border_w, inner_radius, bg_color, (true, true, false, false)));
                } else {
                    // Top half background (no border)
                    quads.push((x, y, w, h / 2.0, radius, bg_color, (true, true, false, false)));
                }
            } else {
                // Top half background
                quads.push((x, y, w, h / 2.0, radius, bg_color, (true, true, false, false)));
                
                if border_w > 0.0 {
                    // Bottom half border
                    quads.push((x, y + h / 2.0, w, h / 2.0, radius, border_color, (false, false, true, true)));
                    // Bottom half inset background
                    let inner_radius = (radius - border_w).max(0.0);
                    quads.push((x + border_w, y + h / 2.0, w - 2.0 * border_w, h / 2.0 - border_w, inner_radius, bg_color, (false, false, true, true)));
                } else {
                    // Bottom half background (no border)
                    quads.push((x, y + h / 2.0, w, h / 2.0, radius, bg_color, (false, false, true, true)));
                }
            }
        }
        quads
    }
}


impl Control for Checkbox {}
impl Control for Toggle {}
