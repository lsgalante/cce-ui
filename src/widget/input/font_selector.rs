use crate::widget::*;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub struct FontSelector {
    base: Widget,
    pub font_family: String,
    just_changed: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pressed: bool,
    child: Arc<Mutex<Option<std::process::Child>>>,
}

impl FontSelector {
    pub fn new(font_family: String) -> Self {
        Self {
            base: Widget::new(),
            font_family,
            just_changed: false,
            parent: None,
            children: Vec::new(),
            pressed: false,
            child: Arc::new(Mutex::new(None)),
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

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }
}

impl Element for FontSelector {
    crate::impl_widget_base!(FontSelector);

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::font_selector_height())
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::font_selector_font())
    }

    fn color(&self) -> [f32; 4] {
        [0.08, 0.08, 0.12, 1.0]
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.pressed = true;
                    return true;
                }
            }
            ElementState::Released => {
                if self.pressed && self.hit_test(px, py, ctx) {
                    self.pressed = false;
                    let mut child_guard = self.child.lock().unwrap();
                    if child_guard.is_none() {
                        let home = std::env::var("HOME").unwrap_or_default();
                        let local_fonts = std::path::Path::new(&home).join(".local/bin/cce-fonts");
                        let cmd_path = if local_fonts.exists() {
                            local_fonts.to_string_lossy().into_owned()
                        } else {
                            "cce-fonts".to_string()
                        };
                        if let Ok(child) = std::process::Command::new(&cmd_path)
                            .arg("--select")
                            .arg(&self.font_family)
                            .stdout(std::process::Stdio::piped())
                            .spawn()
                        {
                            *child_guard = Some(child);
                            ctx.register_tick_receiver(self.base.id());
                        }
                    }
                    return true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn tick(&mut self, _dt: f32, ctx: &mut UiContext) -> bool {
        let mut child_guard = self.child.lock().unwrap();
        if let Some(ref mut child) = *child_guard {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let child_val = child_guard.take().unwrap();
                    ctx.unregister_tick_receiver(self.base.id());
                    if status.success() {
                        if let Ok(output) = child_val.wait_with_output() {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            let trimmed = stdout.trim().to_string();
                            if !trimmed.is_empty() && trimmed != self.font_family {
                                self.font_family = trimmed;
                                self.just_changed = true;
                                return true;
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(_) => {
                    *child_guard = None;
                    ctx.unregister_tick_receiver(self.base.id());
                }
            }
        }
        false
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let bg_color = [0.08, 0.08, 0.12, 1.0];
        let border_color = if self.pressed {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, border_color));
        quads.push((self.base.x + 1.0, self.base.y + top + 1.0, self.base.w - 2.0, visual_h - 2.0, bg_color));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.base.label_offset();
        let _visual_h = self.base.h - top;
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }

        labels.push(TextLabel {
            text: self.font_family.clone(),
            x: self.base.x + 8.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top),
            font_size: 12.0,
            color: [0xdd, 0xdd, 0xe2],
        });

        labels.push(TextLabel {
            text: "🔤".to_string(),
            x: self.base.x + self.base.w - 20.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 11.0, top),
            font_size: 11.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::font_selector_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::font_selector_corner_radius()
    }
}

impl Drop for FontSelector {
    fn drop(&mut self) {
        clear_widget_references(self);
    }
}

unsafe impl Send for FontSelector {}
unsafe impl Sync for FontSelector {}

impl Control for FontSelector {}
