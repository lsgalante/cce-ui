//! The ramp editors. [`Ramp`] is a Houdini-style float ramp: keys on a curve over a plot, a
//! preset and line-type dropdown, and while a key is selected a 2-axis key pad (its position
//! and value) and a delete button. Those controls are the ramp's own values, never registered:
//! the window's focus record names the one with the keyboard and the ramp hands it the keys
//! (`focus_field`). A hover-scroll over a key steers it and coasts. [`ColorRamp`] is the colour
//! gradient: keys of colour along a bar, the selected one edited by R, G and B sliders. A
//! ramp's spec string is `cce_core::ramp`'s (`format_ramp_spec` / `parse_ramp_spec`, re-exported
//! here).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the float `Ramp`: its key, construction, presets, the spec in and out, `impl Layout` |
//! | `geometry` | the plot, key rings, the rolled rim, where a dragged key lands, the controls' places |
//! | `paint` | `impl Paint for Ramp` |
//! | `input` | `impl Input for Ramp`: key drags, the hover-scroll, the controls and their focus |
//! | `color` | the `ColorRamp` widget whole |

mod color;
mod geometry;
mod input;
mod paint;
#[cfg(test)]
mod tests;

pub use color::{ColorRamp, ColorRampKey};

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::model::{EventCtx, Input, Layout, Paint};
use crate::widget::*;
use crate::widget::input::{Slider, Slider2D, Button};

#[derive(Debug, Clone)]
pub struct RampKey {
    pub pos: f32,
    pub value: f32,
}

pub struct Ramp {
    pub base: Widget,
    pub keys: Vec<RampKey>,
    pub selected_key_idx: Option<usize>,
    pub is_dragging_key: bool,
    pub just_changed: bool,
    /// The key latched by the current hover-scroll gesture: a trackpad
    /// scroll starting over a key steers that key until the fingers lift
    /// (a >250ms pause reads as a new gesture and re-latches by hover).
    scroll_key_idx: Option<usize>,
    /// Context-menu toggle: hide the bottom control strip and let the graph
    /// claim its space.
    pub controls_collapsed: bool,
    /// Hover-scroll glide velocity (plot units/sec, applied-delta signs) and
    /// the last scroll-event instant: when the event stream stops, the tick
    /// keeps the latched key coasting with exponential decay.
    scroll_vel: (f32, f32),
    last_key_scroll: Option<web_time::Instant>,

    // Child controls for key editing & deletion. The key pad is a 2-axis
    // slider driving the selected key's position (x) and value (y).
    pub key_pad: Adapted<Slider2D>,
    pub del_button: Adapted<Button>,
    pub preset_dropdown: Adapted<Dropdown>,
    pub line_type_dropdown: Adapted<Dropdown>,
}

impl Ramp {
    /// How many fields take the keyboard, in order: the two dropdowns, then the key pad and
    /// the delete button while a key is selected.
    fn field_count(&self) -> usize {
        if self.selected_key_idx.is_some() { 4 } else { 2 }
    }

    fn field(&mut self, i: usize) -> &mut dyn WidgetHost {
        match i {
            0 => &mut self.preset_dropdown,
            1 => &mut self.line_type_dropdown,
            2 => &mut self.key_pad,
            _ => &mut self.del_button,
        }
    }

    fn field_id(&self, i: usize) -> WidgetId {
        match i {
            0 => self.preset_dropdown.base().id(),
            1 => self.line_type_dropdown.base().id(),
            2 => self.key_pad.base().id(),
            _ => self.del_button.base().id(),
        }
    }

    /// The field the window's focus is on, if it is one of the ramp's.
    fn focused_field(&self, ui: &UiContext) -> Option<usize> {
        let focused = ui.focused_widget?;
        (0..4).find(|&i| self.field_id(i) == focused)
    }

    /// Give field `i` the keyboard: the window's focus record names it (a field checks the
    /// record before it takes a key), and it is told. The field is the ramp's own value and
    /// never enters the registry; keys reach the ramp, which hands them to the field the
    /// record names (`on_event`).
    fn focus_field(&mut self, i: usize, ui: &mut UiContext) {
        if let Some(old) = self.focused_field(ui).filter(|&o| o != i) {
            self.field(old).unfocus();
        }
        ui.claim_focus(self.field_id(i));
        self.field(i).focus();
    }

    pub fn new() -> Adapted<Ramp> {
        let keys = vec![
            RampKey { pos: 0.0, value: 0.5 },
            RampKey { pos: 0.2, value: 1.0 },
            RampKey { pos: 0.8, value: 1.0 },
            RampKey { pos: 1.0, value: 0.5 },
        ];
        
        // The key pad: a 2-axis slider driving the selected key's position
        // (x) and value (y), labeled like the dropdowns.
        let key_pad = Slider2D::new().with_label("Key");
        // A square x-icon button (cce-icons); label fallback if the icon set
        // is missing on this machine. By NAME, not by a captured id: an id
        // does not survive the renderer rebuild a reconnect performs, and the
        // widget outlives the renderer (see `Button::icon_name`).
        let del_button =
            Button::new(0.0, 0.0, 22.0, 22.0).with_icon_name("x", &crate::l10n::tr("ramp-delete-key"));
        // Short names on purpose: the strip's columns are narrow, and these
        // render inside param rows too ("Bevel (Raised)" used to clip).
        // Labeled: the dropdowns draw their own detached labels, sitting on
        // the expanded top wall of their inset (the labeled-relief style).
        // The presets, and nothing else: a curve edited by hand is no
        // preset, and the trigger says so by going blank (`sync_preset`)
        // rather than by a "Custom" entry that, picked, did nothing.
        let preset_dropdown = Dropdown::new(
            RAMP_PRESETS.iter().map(|(name, _)| name.to_string()).collect(),
            1,
        ).with_open_upward(true).with_label("Preset");
        let line_type_dropdown = Dropdown::new(
            vec![
                "Linear".to_string(),
                "Bezier".to_string(),
            ],
            0,
        ).with_open_upward(true).with_label("Line");
        
        Adapted::new(Ramp {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            scroll_key_idx: None,
            controls_collapsed: false,
            scroll_vel: (0.0, 0.0),
            last_key_scroll: None,
            key_pad,
            del_button,
            preset_dropdown,
            line_type_dropdown,
        })
    }
    
    /// Replace the curve with preset `idx` of [`RAMP_PRESETS`] (an index
    /// past the end changes nothing) and show it on the trigger.
    pub fn apply_preset(&mut self, idx: usize) {
        if let Some((_, keys)) = RAMP_PRESETS.get(idx) {
            self.keys = keys.iter().map(|&(pos, value)| RampKey { pos, value }).collect();
        }
        self.selected_key_idx = None;
        self.sync_preset();
        self.just_changed = true;
    }

    /// Show on the preset trigger the preset the curve IS, or nothing when
    /// it is none of them — after a hand edit, or a spec that is no preset.
    /// Blank rather than a stale name, and the dropdown keeps a pick of the
    /// preset it last showed live, so choosing it again puts it back.
    pub fn sync_preset(&mut self) {
        let matches = |keys: &[(f32, f32)]| {
            self.keys.len() == keys.len()
                && self.keys.iter().zip(keys).all(|(k, &(pos, value))| {
                    (k.pos - pos).abs() <= 0.0005 && (k.value - value).abs() <= 0.0005
                })
        };
        match RAMP_PRESETS.iter().position(|(_, keys)| matches(keys)) {
            Some(idx) => {
                self.preset_dropdown.selected = idx;
                self.preset_dropdown.custom_display_text = None;
            }
            None => self.preset_dropdown.custom_display_text = Some(String::new()),
        }
    }

    /// The curve's value at `t` — [`crate::layout::sample_ramp_keys`], the
    /// DE's one ramp interpolation, so what this widget draws is exactly
    /// what every consumer of its spec string evaluates.
    pub fn get_interpolated_value(&self, t: f32) -> f32 {
        let keys: Vec<(f32, f32)> = self.keys.iter().map(|k| (k.pos, k.value)).collect();
        crate::layout::sample_ramp_keys(&keys, self.smooth(), t)
    }

    /// Whether the curve is the smooth (monotone cubic) line type vs straight
    /// segments — see [`crate::layout::sample_ramp_keys`].
    pub fn smooth(&self) -> bool {
        self.line_type_dropdown.selected == 1
    }

    /// This ramp's state as the DE's ramp spec string ([`format_ramp_spec`]).
    pub fn spec_string(&self) -> String {
        let keys: Vec<(f32, f32)> = self.keys.iter().map(|k| (k.pos, k.value)).collect();
        format_ramp_spec(&keys, self.smooth())
    }

    /// Apply a spec string ([`parse_ramp_spec`]); returns whether anything changed.
    /// Unparsable specs are ignored (keeps the current curve).
    pub fn set_spec(&mut self, spec: &str) -> bool {
        let Some((keys, smooth)) = parse_ramp_spec(spec) else {
            return false;
        };
        let new_keys: Vec<RampKey> =
            keys.into_iter().map(|(pos, value)| RampKey { pos, value }).collect();
        let new_line = if smooth { 1 } else { 0 };
        let changed = self.line_type_dropdown.selected != new_line
            || self.keys.len() != new_keys.len()
            || self
                .keys
                .iter()
                .zip(new_keys.iter())
                .any(|(a, b)| (a.pos - b.pos).abs() > 0.0005 || (a.value - b.value).abs() > 0.0005);
        if changed {
            self.keys = new_keys;
            self.line_type_dropdown.selected = new_line;
            self.selected_key_idx = None;
            self.sync_preset();
            self.arrange_fields();
        }
        changed
    }
}

/// The ramp editor's presets, in the order its Preset dropdown lists them:
/// a name and the keys, `(pos, value)`, the curve is set to.
pub const RAMP_PRESETS: &[(&str, &[(f32, f32)])] = &[
    ("Linear", &[(0.0, 0.0), (1.0, 1.0)]),
    ("Raised", &[(0.0, 0.5), (0.2, 1.0), (0.8, 1.0), (1.0, 0.5)]),
    ("Sunken", &[(0.0, 0.5), (0.2, 0.0), (0.8, 0.0), (1.0, 0.5)]),
    ("Peak", &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]),
    ("Valley", &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)]),
];

pub use cce_core::ramp::{format_ramp_spec, parse_ramp_spec};

impl Layout for Ramp {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, 150.0))
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.base.x = rect.x;
        self.base.y = rect.y;
        self.base.w = rect.width;
        self.base.h = rect.height;
        self.arrange_fields();
    }

    // register_embedded_children: gone entirely (6bd self-routing): the fields are never
    // in the registry — the ramp decides which field has the keyboard (`focus_field`), the composite
    // itself covers the spatial grid, and an eagerly-registered child DROPDOWN's open
    // popover made `is_coordinate_covered` occlude the composite's own hit gate (the
    // exclusion is exact-id only), which is why preset-item clicks never landed.
}
