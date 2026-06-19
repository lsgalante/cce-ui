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
        if self.base.label.is_some() {
            12.0 + crate::layout::label_margin()
        } else {
            0.0
        }
    }
}

impl Element for Trackpad {
    crate::impl_widget_base!(Trackpad);

    fn color(&self) -> [f32; 4] {
        [0.11, 0.11, 0.16, 0.85]
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let mut quads = Vec::new();

        // 1. Background
        quads.push((x, y, w, h, [0.11, 0.11, 0.16, 0.85]));

        // 2. Borders
        let border_color = [0.28, 0.28, 0.38, 1.0];
        quads.push((x, y, w, 1.0, border_color));             // Top
        quads.push((x, y + h - 1.0, w, 1.0, border_color));     // Bottom
        quads.push((x, y, 1.0, h, border_color));             // Left
        quads.push((x + w - 1.0, y, 1.0, h, border_color));     // Right

        // 3. Fingers
        for finger in &self.fingers {
            let rx = finger.x.clamp(0.0, 1.0);
            let ry = finger.y.clamp(0.0, 1.0);
            let fx = x + rx * w;
            let fy = y + ry * h;
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
        let (x, y, _w, h) = self.rect();
        let mut labels = Vec::new();

        // Render "Touchpad Area" label
        labels.push(TextLabel {
            text: "Touchpad Area".to_string(),
            x: x + 12.0,
            y: y + h - 22.0,
            font_size: 11.0,
            color: [0x73, 0x73, 0x8c],
        });

        // Optional widget-base label on top
        if let Some(ref label) = self.base.label {
            labels.push(TextLabel {
                text: label.clone(),
                x,
                y: y - (12.0 + crate::layout::label_margin()),
                font_size: 12.0,
                color: [0x83, 0x83, 0x8a],
            });
        }

        labels
    }

    fn draggable(&self) -> bool { true }
    fn is_dragging(&self) -> bool { !self.fingers.is_empty() }

    fn drag_begin(&mut self, px: f32, py: f32) {
        let (x, y, w, h) = self.rect();
        if w > 0.0 && h > 0.0 {
            let rx = ((px - x) / w).clamp(0.0, 1.0);
            let ry = ((py - y) / h).clamp(0.0, 1.0);
            self.fingers = vec![Finger { slot: 0, x: rx, y: ry }];
        }
    }

    fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let (x, y, w, h) = self.rect();
        if w > 0.0 && h > 0.0 {
            let rx = ((px - x) / w).clamp(0.0, 1.0);
            let ry = ((py - y) / h).clamp(0.0, 1.0);
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
            let (x, y, w, h) = self.rect();
            if px >= x && px <= x + w && py >= y && py <= y + h {
                if state == ElementState::Pressed {
                    let rx = ((px - x) / w).clamp(0.0, 1.0);
                    let ry = ((py - y) / h).clamp(0.0, 1.0);
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
