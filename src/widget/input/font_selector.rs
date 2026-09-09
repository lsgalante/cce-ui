use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::*;
use std::sync::{Arc, Mutex};

/// Font-family picker field (narrow-trait model, Phase 6as leaf sweep): click spawns
/// `cce-fonts --select` and the tick reaps the child, committing its stdout as the new
/// family. The detached control label rides the adapter; the model paints the field,
/// the (possibly fade-truncated) family name, and the picker glyph.
#[derive(Debug, Clone)]
pub struct FontSelector {
    pub font_family: String,
    just_changed: bool,
    pressed: bool,
    hovered: bool,
    child: Arc<Mutex<Option<std::process::Child>>>,
    /// Raised style, the closed Dropdown's: the field is a flush inset trough
    /// with a transparent face (the plate shows through), the hover and press
    /// states a wash inside it. Defaults to `control_relief()`; the flat style
    /// keeps the framed dark field.
    raised: bool,
    /// Keyboard focus (FocusIn / FocusOut): lights the plate's rim and arms
    /// Enter / Space to open the picker.
    focused: bool,
}

impl FontSelector {
    pub fn new(font_family: String) -> Adapted<FontSelector> {
        Adapted::new(FontSelector {
            font_family,
            just_changed: false,
            pressed: false,
            hovered: false,
            child: Arc::new(Mutex::new(None)),
            raised: crate::layout::control_relief(),
            focused: false,
        })
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    /// The field's own labels at the laid-out rect: the family name (fade-truncated to
    /// fit before the picker glyph when too wide) and the glyph itself.
    fn field_labels(&self, rect: Rect) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let max_w = rect.width - 33.0;
        let full_w = TextLabel::estimate_width(&self.font_family, 12.0);
        let text_y = crate::layout::align_text_y(rect.y, rect.height, 12.0, 0.0);

        if full_w <= max_w {
            labels.push(TextLabel {
                text: self.font_family.clone(),
                x: rect.x + 8.0,
                y: text_y,
                font_size: 12.0,
                color: [0xdd, 0xdd, 0xe2],
            });
        } else {
            // Find the prefix that fits in max_w - 30.0
            let target_prefix_w = max_w - 30.0;
            let mut prefix = String::new();
            for c in self.font_family.chars() {
                let mut test_prefix = prefix.clone();
                test_prefix.push(c);
                if TextLabel::estimate_width(&test_prefix, 12.0) > target_prefix_w {
                    break;
                }
                prefix.push(c);
            }

            let prefix_w = TextLabel::estimate_width(&prefix, 12.0);
            labels.push(TextLabel {
                text: prefix.clone(),
                x: rect.x + 8.0,
                y: text_y,
                font_size: 12.0,
                color: [0xdd, 0xdd, 0xe2],
            });

            // The next 5 characters fade out
            let remaining: Vec<char> = self.font_family.chars().skip(prefix.chars().count()).collect();
            let fade_colors = [
                [187, 187, 193],
                [154, 154, 160],
                [120, 120, 128],
                [87, 87, 95],
                [53, 53, 62],
            ];
            let mut cur_x = rect.x + 8.0 + prefix_w;
            for i in 0..5 {
                if i < remaining.len() {
                    let c_str = remaining[i].to_string();
                    let c_w = TextLabel::estimate_width(&c_str, 12.0);
                    labels.push(TextLabel {
                        text: c_str,
                        x: cur_x,
                        y: text_y,
                        font_size: 12.0,
                        color: fade_colors[i],
                    });
                    cur_x += c_w;
                }
            }
        }

        // The picker glyph: "Aa", the font-picker convention, in the dropdown arrow's
        // grey — a text glyph every face has (the emoji this drew rendered as tofu
        // wherever no emoji font was installed).
        labels.push(TextLabel {
            text: "Aa".to_string(),
            x: rect.x + rect.width - 24.0,
            y: crate::layout::align_text_y(rect.y, rect.height, 11.0, 0.0),
            font_size: 11.0,
            color: [0x83, 0x83, 0x8a],
        });

        labels
    }
}

impl Adapted<FontSelector> {
    /// Raised style: see the `raised` field.
    pub fn with_raised(mut self, raised: bool) -> Self {
        self.raised = raised;
        self
    }
}

impl Layout for FontSelector {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::font_selector_height()))
    }
}

impl Paint for FontSelector {
    fn color(&self) -> [f32; 4] {
        // The plate shows through in both styles: a transparent-faced flush
        // plate raised, the shared well frame flat.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::font_selector_font())
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::font_selector_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let r = crate::layout::font_selector_corner_radius();
        if self.raised {
            // The closed-dropdown chrome: a flush control plate with a
            // transparent face, the state fill rounded to sit inside it.
            ctx.control_plate(
                &crate::widget::ControlPlate::control(rect, r, crate::widget::PlateStance::Flush, [0.0; 4])
                    .with_tint(self.focused.then(crate::widget::ControlPlate::focus_tint)),
            );
            let wash = if self.pressed {
                Some(colors::button_press_color())
            } else if self.hovered {
                Some(colors::button_hover_color())
            } else {
                None
            };
            if let Some(c) = wash {
                ctx.rounded_rect(rect, r, (true, true, true, true), c);
            }
            self.paint_labels(rect, ctx);
            return;
        }
        // The flat style: the one well frame (`colors::well_frame_color`) over
        // the plate, lit while pressed, rounded at the selector's radius.
        ctx.border(rect, (r, r, r, r), [0.0; 4], colors::well_frame_color(self.hovered, self.pressed), 1.0);

        self.paint_labels(rect, ctx);
    }
}

impl FontSelector {
    /// Labels: family text clipped short of the picker glyph (the legacy per-label
    /// bounds), glyph unclipped.
    fn paint_labels(&self, rect: Rect, ctx: &mut PaintCtx) {
        let font = self.widget_font();
        let clip_right = rect.x + rect.width - 24.0;
        let bounds = Some([rect.x, rect.y, clip_right, rect.y + rect.height]);
        let labels = self.field_labels(rect);
        let count = labels.len();
        for (idx, l) in labels.into_iter().enumerate() {
            let b = if idx < count - 1 { bounds } else { None };
            ctx.text_with(l.text, l.x, l.y, l.font_size, l.color, font.clone(), b);
        }
    }
}

impl FontSelector {
    /// Spawn `cce-fonts --select` (once — a picker already open keeps it); the
    /// tick reaps it. The press of this plate, by pointer or by key.
    fn open_picker(&mut self) {
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
            }
        }
    }
}

impl Input for FontSelector {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, _dt: f32, _rect: Rect) -> bool {
        let mut child_guard = self.child.lock().unwrap();
        if let Some(ref mut child) = *child_guard {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let child_val = child_guard.take().unwrap();
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
                }
            }
        }
        false
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            Event::KeyInput(key_event) => {
                // A focused plate is pressed by Enter / Space, as a Button is.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.open_picker();
                        true
                    }
                    _ => false,
                }
            }
            Event::MouseButton { button, state, x, y, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                match state {
                    ElementState::Pressed => {
                        self.pressed = true;
                        true
                    }
                    ElementState::Released => {
                        let r = ectx.rect;
                        let inside = *x >= r.x && *x <= r.x + r.width && *y >= r.y && *y <= r.y + r.height;
                        if self.pressed && inside {
                            self.pressed = false;
                            self.open_picker();
                            return true;
                        }
                        let was = self.pressed;
                        self.pressed = false;
                        was
                    }
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
            _ => false,
        }
    }
}
