use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::*;

/// Keybinding capture field (narrow-trait model, Phase 6as leaf sweep): click to arm,
/// then the next chord/key commits into `value`. The detached control label rides the
/// adapter's base-label machinery; the model paints only the field itself.
#[derive(Debug, Clone)]
pub struct KeybindRecorder {
    pub value: String,
    pub recording: bool,
    pub just_changed: bool,
    pressed: bool,
    hovered: bool,
    /// Recessed style, the TextBox's: the field is a well carved into the
    /// plate below with no fill of its own, its rim lit in the highlight
    /// accent while recording (the TextBox's editing treatment). Defaults to
    /// `control_relief()`; the flat style is the shared well frame.
    recessed: bool,
    /// Keyboard focus (FocusIn / FocusOut): Enter / Space arm recording.
    focused: bool,
}

impl KeybindRecorder {
    pub fn new(value: String) -> Adapted<KeybindRecorder> {
        Adapted::new(KeybindRecorder {
            value,
            recording: false,
            just_changed: false,
            pressed: false,
            hovered: false,
            recessed: crate::layout::control_relief(),
            focused: false,
        })
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    fn commit(&mut self, parts: Vec<&str>) {
        self.value = parts.join("+");
        self.just_changed = true;
        self.recording = false;
    }
}

impl Adapted<KeybindRecorder> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
        self
    }
}

impl Layout for KeybindRecorder {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::textbox_height()))
    }
}

impl Paint for KeybindRecorder {
    fn color(&self) -> [f32; 4] {
        // A well: the plate is its floor, in both styles.
        [0.0, 0.0, 0.0, 0.0]
    }

    /// The field text in the TextBox's font: both are wells you type into.
    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if self.recessed {
            let radius = crate::layout::textbox_corner_radius();
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let (well, radii) = crate::layout::carve_inside(rect, (radius, radius, radius, radius), depth);
            if self.recording {
                let hc = crate::color::highlight_primary_color();
                ctx.recess_tinted(well, radii, depth, [hc[0], hc[1], hc[2]]);
            } else {
                ctx.recess(well, radii, depth);
            }
            self.paint_text(rect, ctx);
            return;
        }
        // The flat style: the one well frame (`colors::well_frame_color`), lit
        // while recording, over the plate — no floor of its own, like the
        // TextBox it stands beside. Rounded like the text wells.
        let radius = crate::layout::textbox_corner_radius();
        let frame = colors::well_frame_color(self.hovered, self.recording || self.pressed);
        ctx.border(rect, (radius, radius, radius, radius), [0.0; 4], frame, 1.0);

        self.paint_text(rect, ctx);
    }
}

impl KeybindRecorder {
    fn paint_text(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (display_text, color) = if self.recording {
            ("[ Press Keys... ]".to_string(), [135, 135, 153])
        } else if self.value.is_empty() {
            ("None".to_string(), [127, 127, 127])
        } else {
            (self.value.clone(), [221, 221, 226])
        };
        let (_, font_size) = crate::layout::control_label_font_detached_parsed();
        let text_y = crate::layout::align_text_y(rect.y, rect.height, font_size, 0.0);
        ctx.text(display_text, rect.x + 8.0, text_y, font_size, color);
    }
}

impl Input for KeybindRecorder {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                match state {
                    // Presses are adapter hit-gated; releases arrive regardless (the
                    // legacy commit/cancel contract).
                    ElementState::Pressed => {
                        self.pressed = true;
                        true
                    }
                    ElementState::Released => {
                        let r = ectx.rect;
                        let inside = *x >= r.x && *x <= r.x + r.width && *y >= r.y && *y <= r.y + r.height;
                        if self.pressed && inside {
                            self.pressed = false;
                            self.recording = true;
                            ectx.request_focus();
                            return true;
                        }
                        let was = self.pressed;
                        self.pressed = false;
                        was
                    }
                }
            }
            Event::KeyInput(key_event) if !self.recording => {
                // A focused well not yet recording: Enter / Space arm it (the
                // click's job, by key). Anything else is not this field's.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.recording = true;
                        true
                    }
                    _ => false,
                }
            }
            Event::KeyInput(key_event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                let mut parts = Vec::new();
                if ui.logo_pressed {
                    parts.push("super");
                }
                if ui.ctrl_pressed {
                    parts.push("ctrl");
                }
                if ui.alt_pressed {
                    parts.push("alt");
                }
                if ui.shift_pressed {
                    parts.push("shift");
                }

                match key_event.state {
                    ElementState::Pressed => match &key_event.logical_key {
                        Key::Named(NamedKey::Escape) => {
                            self.recording = false;
                            true
                        }
                        Key::Named(NamedKey::Control)
                        | Key::Named(NamedKey::Shift)
                        | Key::Named(NamedKey::Alt)
                        | Key::Named(NamedKey::Super) => {
                            // Modifier pressed: track the active-modifier chord so far.
                            if !parts.is_empty() {
                                self.value = parts.join("+");
                            }
                            true
                        }
                        Key::Named(key) => {
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
                                self.commit(parts);
                            }
                            true
                        }
                        Key::Character(ch) => {
                            let ch = ch.clone();
                            parts.push(ch.as_str());
                            self.commit(parts);
                            true
                        }
                    },
                    ElementState::Released => match &key_event.logical_key {
                        Key::Named(NamedKey::Control)
                        | Key::Named(NamedKey::Shift)
                        | Key::Named(NamedKey::Alt)
                        | Key::Named(NamedKey::Super) => {
                            // Modifier released with nothing else held: commit the
                            // recorded modifier-only binding.
                            if !self.value.is_empty()
                                && !ui.ctrl_pressed
                                && !ui.shift_pressed
                                && !ui.alt_pressed
                                && !ui.logo_pressed
                            {
                                self.just_changed = true;
                                self.recording = false;
                            }
                            true
                        }
                        _ => true,
                    },
                }
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                self.recording = false;
                false
            }
            _ => false,
        }
    }
}
