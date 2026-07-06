use crate::widget::*;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct Finger {
    pub slot: usize,
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Clone)]
pub struct Trackpad {
    base: Widget,
    pub fingers: Vec<Finger>,
}

impl Trackpad {
    pub fn new() -> Self {
        Self {
            base: Widget::new(),
            fingers: Vec::new(),
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn set_fingers(&mut self, fingers: Vec<Finger>) {
        self.fingers = fingers;
    }

    pub fn label_offset(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        if self.base.label.is_some() {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        }
    }
}

impl Element for Trackpad {
    crate::impl_widget_base!(Trackpad);

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
    }

    fn color(&self) -> [f32; 4] {
        [0.11, 0.11, 0.16, 0.85]
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (rx_rect, y, rw_rect, h) = self.rect();
        let label_x = self.label_x_offset();
        let x = rx_rect + label_x;
        let w = rw_rect - label_x;
        let top = self.label_offset();
        let visual_h = h - top;
        let mut quads = Vec::new();

        // 1. Background
        quads.push((x, y + top, w, visual_h, [0.11, 0.11, 0.16, 0.85]));

        // 2. Borders
        let border_color = [0.28, 0.28, 0.38, 1.0];
        quads.push((x, y + top, w, 1.0, border_color));             // Top
        quads.push((x, y + top + visual_h - 1.0, w, 1.0, border_color));     // Bottom
        quads.push((x, y + top, 1.0, visual_h, border_color));             // Left
        quads.push((x + w - 1.0, y + top, 1.0, visual_h, border_color));     // Right

        // 3. Fingers
        for finger in &self.fingers {
            let rx = finger.x.clamp(0.0, 1.0);
            let ry = finger.y.clamp(0.0, 1.0);
            let fx = x + rx * w;
            let fy = y + top + ry * visual_h;
            let dot_size = 12.0;

            // Render glow (outer light blue rectangle)
            quads.push((
                fx - (dot_size + 6.0) / 2.0,
                fy - (dot_size + 6.0) / 2.0,
                dot_size + 6.0,
                dot_size + 6.0,
                [0.35, 0.55, 0.95, 0.4],
            ));
            // Render core (solid blue/purple rectangle)
            quads.push((
                fx - dot_size / 2.0,
                fy - dot_size / 2.0,
                dot_size,
                dot_size,
                [0.45, 0.65, 1.0, 1.0],
            ));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (rx_rect, y, rw_rect, h) = self.rect();
        let label_x = self.label_x_offset();
        let x = rx_rect + label_x;
        let _w = rw_rect - label_x;
        let top = self.label_offset();
        let visual_h = h - top;
        let mut labels = Vec::new();

        // Render "Touchpad Area" label
        labels.push(TextLabel {
            text: "Touchpad Area".to_string(),
            x: x + 12.0,
            y: y + top + visual_h - 22.0,
            font_size: 11.0,
            color: [0x73, 0x73, 0x8c],
        });

        // Optional widget-base label on top
        if let Some(ref label) = self.base.label {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            labels.push(TextLabel {
                text: label.clone(),
                x: rx_rect,
                y,
                font_size,
                color: colors::control_label_color_detached_for_state(self.base.hovered, self.base.focused),
            });
        }

        labels
    }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { !self.fingers.is_empty() }

    fn drag_begin(&mut self, px: f32, py: f32) {
        let (rx_rect, y, rw_rect, h) = self.rect();
        let label_x = self.label_x_offset();
        let x = rx_rect + label_x;
        let w = rw_rect - label_x;
        let top = self.label_offset();
        let visual_h = h - top;
        if w > 0.0 && visual_h > 0.0 {
            let rx = ((px - x) / w).clamp(0.0, 1.0);
            let ry = ((py - (y + top)) / visual_h).clamp(0.0, 1.0);
            self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let (rx_rect, y, rw_rect, h) = self.rect();
        let label_x = self.label_x_offset();
        let x = rx_rect + label_x;
        let w = rw_rect - label_x;
        let top = self.label_offset();
        let visual_h = h - top;
        if w > 0.0 && visual_h > 0.0 {
            let rx = ((px - x) / w).clamp(0.0, 1.0);
            let ry = ((py - (y + top)) / visual_h).clamp(0.0, 1.0);
            self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
            true
        } else {
            false
        }
    }

    fn drag_end(&mut self) {
        self.fingers.clear();
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, _ctx: &mut UiContext) -> bool {
        if button == MouseButton::Left {
            let (rx_rect, y, rw_rect, h) = self.rect();
            let label_x = self.label_x_offset();
            let x = rx_rect + label_x;
            let w = rw_rect - label_x;
            let top = self.label_offset();
            let visual_h = h - top;
            if px >= x && px <= x + w && py >= y + top && py <= y + top + visual_h {
                if state == ElementState::Pressed {
                    let rx = ((px - x) / w).clamp(0.0, 1.0);
                    let ry = ((py - (y + top)) / visual_h).clamp(0.0, 1.0);
                    self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
                    return true;
                } else {
                    self.fingers.clear();
                    return true;
                }
            }
        }
        false
    }
}
