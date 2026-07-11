use crate::colors;
use crate::widget::*;

#[derive(Debug, Clone)]
pub struct KeybindRecorder {
    pub base: Widget,
    pub value: String,
    pub recording: bool,
    pub just_changed: bool,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    pub pressed: bool,
}

impl KeybindRecorder {
    pub fn new(value: String) -> Self {
        Self {
            base: Widget::new(),
            value,
            recording: false,
            just_changed: false,
            parent: None,
            children: Vec::new(),
            pressed: false,
        }
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

impl Element for KeybindRecorder {
    crate::impl_widget_base!(KeybindRecorder);

    // Leaf legacy widget: own fonted labels via paint_self (the default no longer
    // drains the text getters).
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) {
        crate::scene::painter::paint_legacy_leaf(
            self, ui, ctx,
            crate::scene::painter::fonted_leaf_labels(self, ui, self.own_labels()),
        );
    }

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::textbox_height())
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
                    self.recording = true;
                    ctx.set_focused(self);
                    self.mark_dirty(ctx);
                    return true;
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !self.recording {
            return false;
        }

        match event.state {
            ElementState::Pressed => {
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        // Cancel recording
                        self.recording = false;
                        self.mark_dirty(ctx);
                        return true;
                    }
                    Key::Named(NamedKey::Control) |
                    Key::Named(NamedKey::Shift) |
                    Key::Named(NamedKey::Alt) |
                    Key::Named(NamedKey::Super) => {
                        // Modifier key pressed. Build current active modifiers list.
                        let mut parts = Vec::new();
                        if ctx.logo_pressed { parts.push("super"); }
                        if ctx.ctrl_pressed { parts.push("ctrl"); }
                        if ctx.alt_pressed { parts.push("alt"); }
                        if ctx.shift_pressed { parts.push("shift"); }
                        
                        if !parts.is_empty() {
                            self.value = parts.join("+");
                        }
                        self.mark_dirty(ctx);
                        return true;
                    }
                    Key::Named(key) => {
                        // Named key pressed (e.g. Enter, Space, Backspace, Arrow keys)
                        let mut parts = Vec::new();
                        if ctx.logo_pressed { parts.push("super"); }
                        if ctx.ctrl_pressed { parts.push("ctrl"); }
                        if ctx.alt_pressed { parts.push("alt"); }
                        if ctx.shift_pressed { parts.push("shift"); }

                        let key_str = match key {
                            NamedKey::Backspace => "backspace",
                            NamedKey::Tab => "tab",
                            NamedKey::Enter => "enter",
                            NamedKey::Space => "space",
                            NamedKey::ArrowDown => "down",
                            NamedKey::ArrowLeft => "left",
                            NamedKey::ArrowRight => "right",
                            NamedKey::ArrowUp => "up",
                            NamedKey::End => "end",
                            NamedKey::Home => "home",
                            NamedKey::PageDown => "pagedown",
                            NamedKey::PageUp => "pageup",
                            NamedKey::Delete => "delete",
                            _ => "",
                        };

                        if !key_str.is_empty() {
                            parts.push(key_str);
                            self.value = parts.join("+");
                            self.just_changed = true;
                            self.recording = false;
                            self.mark_dirty(ctx);
                        }
                        return true;
                    }
                    Key::Character(ch) => {
                        // Character key pressed (e.g. "a", "q", "1", etc.)
                        let mut parts = Vec::new();
                        if ctx.logo_pressed { parts.push("super"); }
                        if ctx.ctrl_pressed { parts.push("ctrl"); }
                        if ctx.alt_pressed { parts.push("alt"); }
                        if ctx.shift_pressed { parts.push("shift"); }

                        parts.push(ch.as_str());
                        self.value = parts.join("+");
                        self.just_changed = true;
                        self.recording = false;
                        self.mark_dirty(ctx);
                        return true;
                    }
                }
            }
            ElementState::Released => {
                match &event.logical_key {
                    Key::Named(NamedKey::Control) |
                    Key::Named(NamedKey::Shift) |
                    Key::Named(NamedKey::Alt) |
                    Key::Named(NamedKey::Super) => {
                        // When a modifier key is released, if we have a non-empty value and no keys are pressed anymore,
                        // we commit the recorded modifier keybinding!
                        if !self.value.is_empty() && !ctx.ctrl_pressed && !ctx.shift_pressed && !ctx.alt_pressed && !ctx.logo_pressed {
                            self.just_changed = true;
                            self.recording = false;
                            self.mark_dirty(ctx);
                        }
                        return true;
                    }
                    _ => {
                        return true;
                    }
                }
            }
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let bg_color = [0.08, 0.08, 0.12, 1.0];
        
        let border_color = if self.recording {
            colors::HIGHLIGHT_PRIMARY
        } else if self.pressed {
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



    fn unfocus(&mut self) {
        self.recording = false;
    }
}

impl Control for KeybindRecorder {}

impl KeybindRecorder {
    pub(crate) fn own_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let top = self.base.label_offset();
        
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }

        let text_y = crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top);
        
        let (display_text, color) = if self.recording {
            ("[ Press Keys... ]".to_string(), [135, 135, 153])
        } else if self.value.is_empty() {
            ("None".to_string(), [127, 127, 127])
        } else {
            (self.value.clone(), [221, 221, 226])
        };

        labels.push(TextLabel {
            text: display_text,
            x: self.base.x + 8.0,
            y: text_y,
            font_size: 12.0,
            color,
        });

        labels
    }

}
