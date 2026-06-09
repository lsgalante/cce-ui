use crate::colors;
use crate::widget::*;

#[derive(Debug, Clone)]
pub struct ButtonStrip {
    pub base: Widget,
    pub buttons: Vec<String>,
    pub selected: Option<usize>,
    pub vertical: bool,
    pub just_clicked: Option<usize>,
    pub hovered_idx: Option<usize>,
    pub pressed_idx: Option<usize>,
}

impl ButtonStrip {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            buttons: Vec::new(),
            selected: None,
            vertical: false,
            just_clicked: None,
            hovered_idx: None,
            pressed_idx: None,
        }
    }

    pub fn with_buttons(mut self, buttons: Vec<String>) -> Self {
        self.buttons = buttons;
        self
    }

    pub fn with_selected(mut self, selected: Option<usize>) -> Self {
        self.selected = selected;
        self
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self
    }

    pub fn selected(&self) -> Option<usize> {
        self.selected
    }

    pub fn set_selected(&mut self, selected: Option<usize>) {
        self.selected = selected;
    }

    pub fn take_click(&mut self) -> Option<usize> {
        self.just_clicked.take()
    }

    pub fn add_button(&mut self, label: &str) {
        self.buttons.push(label.to_string());
    }

    pub fn item_rect(&self, idx: usize) -> (f32, f32, f32, f32) {
        if self.buttons.is_empty() || idx >= self.buttons.len() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let n = self.buttons.len() as f32;
        let (x, y, w, h) = self.rect();
        if self.vertical {
            let btn_h = h / n;
            (x, y + idx as f32 * btn_h, w, btn_h)
        } else {
            let btn_w = w / n;
            (x + idx as f32 * btn_w, y, btn_w, h)
        }
    }
}

impl Element for ButtonStrip {
    crate::impl_widget_base!(ButtonStrip);

    fn highlight_quad(&self, _ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        None
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left {
            return false;
        }
        let mut changed = false;
        match state {
            ElementState::Pressed => {
                for i in 0..self.buttons.len() {
                    let r = self.item_rect(i);
                    if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                        self.pressed_idx = Some(i);
                        changed = true;
                        break;
                    }
                }
            }
            ElementState::Released => {
                if let Some(pressed) = self.pressed_idx {
                    let r = self.item_rect(pressed);
                    if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                        if self.selected != Some(pressed) {
                            self.selected = Some(pressed);
                            self.just_clicked = Some(pressed);
                            changed = true;
                        }
                    }
                }
                self.pressed_idx = None;
            }
        }
        changed
    }

    fn cursor_moved(&mut self, px: f32, py: f32, _ctx: &mut UiContext) -> bool {
        let old_hovered = self.hovered_idx;
        self.hovered_idx = None;
        for i in 0..self.buttons.len() {
            let r = self.item_rect(i);
            if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                self.hovered_idx = Some(i);
                break;
            }
        }
        old_hovered != self.hovered_idx
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        for i in 0..self.buttons.len() {
            let r = self.item_rect(i);
            let mut bg_color = [0.0, 0.0, 0.0, 0.0];
            if Some(i) == self.selected {
                bg_color = colors::PANEL_MENU_FOCUSED;
            } else if Some(i) == self.pressed_idx {
                bg_color = colors::BUTTON_PRESS;
            } else if Some(i) == self.hovered_idx {
                bg_color = colors::PANEL_MENU_HOVER;
            }
            if bg_color != [0.0, 0.0, 0.0, 0.0] {
                quads.push((r.0, r.1, r.2, r.3, bg_color));
            }
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let font_size = 12.0;
        for (i, btn_label) in self.buttons.iter().enumerate() {
            let r = self.item_rect(i);
            let color = if Some(i) == self.selected {
                [0xf0, 0xf0, 0xf5]
            } else {
                [0xa8, 0xa8, 0xb3]
            };
            if self.vertical {
                let line_height = font_size * 1.2;
                let label_len = btn_label.chars().count() as f32;
                let total_h = label_len * line_height;
                let start_y = r.1 + (r.3 - total_h) / 2.0;
                let char_w = TextLabel::estimate_width("o", font_size);
                let x_pos = r.0 + (r.2 - char_w) / 2.0;
                for (char_idx, c) in btn_label.chars().enumerate() {
                    labels.push(TextLabel {
                        text: c.to_string(),
                        x: x_pos,
                        y: start_y + char_idx as f32 * line_height,
                        font_size,
                        color,
                    });
                }
            } else {
                let est_w = TextLabel::estimate_width(btn_label, font_size);
                labels.push(TextLabel {
                    text: btn_label.clone(),
                    x: r.0 + (r.2 - est_w) / 2.0,
                    y: r.1 + (r.3 - font_size) / 2.0 - 1.0,
                    font_size,
                    color,
                });
            }
        }
        labels
    }

    fn keyboard_input(&mut self, event: &KeyEvent, _ctx: &mut UiContext) -> bool {
        if event.state != ElementState::Pressed { return false; }
        if self.buttons.is_empty() { return false; }

        let current = self.selected.unwrap_or(0);
        let mut next = current;

        match event.logical_key {
            Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowUp) => {
                if current > 0 {
                    next = current - 1;
                } else {
                    next = self.buttons.len() - 1;
                }
            }
            Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowDown) => {
                if current + 1 < self.buttons.len() {
                    next = current + 1;
                } else {
                    next = 0;
                }
            }
            _ => return false,
        }

        if Some(next) != self.selected {
            self.selected = Some(next);
            self.just_clicked = Some(next);
            return true;
        }
        false
    }
}

impl Control for ButtonStrip {}
