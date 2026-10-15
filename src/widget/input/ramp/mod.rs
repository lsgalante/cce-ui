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
//! | `mod.rs` | the float `Ramp`: its key, construction, presets, the spec in and out, `impl Layout`; and what both ramps share: resettling a moved key, peg order and the topmost-peg hit test, the closed point test, parking a field |
//! | `geometry` | the plot, the key pegs (size, order, hit test), where a dragged key lands, the controls' places |
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

/// A key drag's start ([`Ramp::drag_grab`](Ramp)).
#[derive(Debug, Clone, Copy)]
pub(super) struct DragGrab {
    /// The key's position when the drag began.
    pub origin: f32,
    /// The key's centre less the pointer, in plot units (position, value).
    pub offset: (f32, f32),
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
    /// The dragged key as the drag found it: its position when grabbed (the soft wall's
    /// zone never starts behind it — `resisted_pos`), and where on its peg the pointer
    /// took it, in plot units (the key keeps that offset instead of snapping its centre
    /// to the pointer). `None` outside a key drag.
    drag_grab: Option<DragGrab>,

    // Child controls for key editing & deletion. The key pad is a 2-axis
    // slider driving the selected key's position (x) and value (y).
    pub key_pad: Adapted<Slider2D>,
    pub del_button: Adapted<Button>,
    pub preset_dropdown: Adapted<Dropdown>,
    pub line_type_dropdown: Adapted<Dropdown>,
}

/// How many fields the float ramp has: the two dropdowns, the key pad, the delete button.
const FIELD_TOTAL: usize = 4;

/// Restore sort order after `keys[i]` changed position (`pos` reads a key's), by adjacent
/// swaps, and return the key's new index — both ramps' `resettle_key`. The moved key is
/// tracked by identity, not found again by its position, so a key within ε of the one it is
/// passing is never mistaken for it, and no float comparison can fail the order.
fn resettle<K>(keys: &mut [K], mut i: usize, pos: impl Fn(&K) -> f32) -> usize {
    while i + 1 < keys.len() && pos(&keys[i]) > pos(&keys[i + 1]) {
        keys.swap(i, i + 1);
        i += 1;
    }
    while i > 0 && pos(&keys[i]) < pos(&keys[i - 1]) {
        keys.swap(i, i - 1);
        i -= 1;
    }
    i
}

/// The pegs of `n` keys in paint order, bottom first: by index, the selected one (`sel`)
/// last, on top.
fn peg_order(n: usize, sel: Option<usize>) -> impl DoubleEndedIterator<Item = usize> {
    let sel = sel.filter(|&i| i < n);
    (0..n).filter(move |&i| Some(i) != sel).chain(sel)
}

/// The key whose peg is under `(px, py)`, topmost first — the reverse of [`peg_order`], so
/// where pegs overlap the press takes the one drawn on top. `peg(i)` is key `i`'s peg centre
/// and the radius it is drawn to; a press past that radius is not on it.
fn topmost_peg(n: usize, sel: Option<usize>, px: f32, py: f32, peg: impl Fn(usize) -> ((f32, f32), f32)) -> Option<usize> {
    peg_order(n, sel).rev().find(|&i| {
        let ((cx, cy), r) = peg(i);
        let (dx, dy) = (px - cx, py - cy);
        dx * dx + dy * dy <= r * r
    })
}

/// Whether `(x, y)` lies in `r`, its edges included: every ramp press test is closed
/// (`Rect::contains` is half-open, and a press on the right or bottom edge would miss).
fn within(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

/// Park a field off-screen at no size: how a ramp hides a control it is not showing.
fn park(w: &mut dyn WidgetHost) {
    w.set_rect(-1000.0, -1000.0, 0.0, 0.0);
}

impl Ramp {
    /// How many fields take the keyboard, in order: the two dropdowns, then the key pad and
    /// the delete button while a key is selected.
    fn field_count(&self) -> usize {
        if self.selected_key_idx.is_some() { FIELD_TOTAL } else { 2 }
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
        (0..FIELD_TOTAL).find(|&i| self.field_id(i) == focused)
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
        // widget outlives the renderer (see `Button::icon_name`). Its size is
        // `arrange_fields`'.
        let del_button =
            Button::new(0.0, 0.0, 0.0, 0.0).with_icon_name("x", &crate::l10n::tr("ramp-delete-key"));
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
            drag_grab: None,
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
            self.keys = keys_from(keys);
        }
        self.selected_key_idx = None;
        self.sync_preset();
        self.just_changed = true;
    }

    /// Delete the selected key — what the delete button does, clicked or pressed from the
    /// keyboard. A ramp keeps two keys at least: with two, nothing happens.
    pub fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_key_idx {
            if self.keys.len() > 2 {
                self.keys.remove(idx);
                self.selected_key_idx = None;
                self.sync_preset();
                self.just_changed = true;
                self.arrange_fields();
            }
        }
    }

    /// The height the ramp needs: a graph [`GRAPH_H`](Ramp::GRAPH_H) tall over the
    /// control strip ([`strip_reserve`](Ramp::strip_reserve)), or with its controls
    /// collapsed the graph alone in its 10 px margins. What it declares
    /// (`intrinsic_size`), and what a host that sizes its rows itself asks for.
    pub fn natural_height(&self) -> f32 {
        if self.controls_collapsed {
            Self::GRAPH_H + 20.0
        } else {
            Self::expanded_height()
        }
    }

    /// [`natural_height`](Self::natural_height) with the controls shown, as every ramp
    /// starts: for a host sizing a row before its ramp exists.
    pub fn expanded_height() -> f32 {
        Self::GRAPH_H + Self::strip_reserve()
    }

    /// Show on the preset trigger the preset the curve IS, or nothing when
    /// it is none of them — after a hand edit, or a spec that is no preset.
    /// Blank rather than a stale name, and the dropdown keeps a pick of the
    /// preset it last showed live, so choosing it again puts it back.
    pub fn sync_preset(&mut self) {
        match RAMP_PRESETS.iter().position(|(_, keys)| self.keys_match(keys)) {
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
    /// To sample many points, take [`key_pairs`](Self::key_pairs) once and sample them
    /// (`paint` does).
    pub fn get_interpolated_value(&self, t: f32) -> f32 {
        crate::layout::sample_ramp_keys(&self.key_pairs(), self.smooth(), t)
    }

    /// The keys as `(pos, value)` pairs, the form the DE's ramp functions take.
    pub fn key_pairs(&self) -> Vec<(f32, f32)> {
        self.keys.iter().map(|k| (k.pos, k.value)).collect()
    }

    /// Whether the keys are `pairs`, each within 0.0005 — a preset the curve is, or a spec
    /// that changes nothing.
    fn keys_match(&self, pairs: &[(f32, f32)]) -> bool {
        self.keys.len() == pairs.len()
            && self.keys.iter().zip(pairs).all(|(k, &(pos, value))| {
                (k.pos - pos).abs() <= 0.0005 && (k.value - value).abs() <= 0.0005
            })
    }

    /// Whether the curve is the smooth (monotone cubic) line type vs straight
    /// segments — see [`crate::layout::sample_ramp_keys`].
    pub fn smooth(&self) -> bool {
        self.line_type_dropdown.selected == 1
    }

    /// This ramp's state as the DE's ramp spec string ([`format_ramp_spec`]).
    pub fn spec_string(&self) -> String {
        format_ramp_spec(&self.key_pairs(), self.smooth())
    }

    /// Apply a spec string ([`parse_ramp_spec`]); returns whether anything changed.
    /// Unparsable specs are ignored (keeps the current curve).
    pub fn set_spec(&mut self, spec: &str) -> bool {
        let Some((keys, smooth)) = parse_ramp_spec(spec) else {
            return false;
        };
        let new_line = if smooth { 1 } else { 0 };
        let changed = self.line_type_dropdown.selected != new_line || !self.keys_match(&keys);
        if changed {
            self.keys = keys_from(&keys);
            self.line_type_dropdown.selected = new_line;
            self.selected_key_idx = None;
            self.sync_preset();
            self.arrange_fields();
        }
        changed
    }
}

/// Keys from `(pos, value)` pairs.
fn keys_from(pairs: &[(f32, f32)]) -> Vec<RampKey> {
    pairs.iter().map(|&(pos, value)| RampKey { pos, value }).collect()
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
    /// [`natural_height`](Ramp::natural_height): the graph whole over the control strip.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, self.natural_height()))
    }

    /// Take the CONTENT rect and arrange the fields in it — the TreeList's rule. Not
    /// `rect_assigned`'s block: the adapter's block holds the detached label strip above
    /// the content, and from its top the graph sat under a label, which is why the gallery
    /// left the ramps unlabelled.
    fn arrange_children(&mut self, rect: Rect) {
        self.base.x = rect.x;
        self.base.y = rect.y;
        self.base.w = rect.width;
        self.base.h = rect.height;
        self.arrange_fields();
    }

    // No `register_embedded_children`: the fields are never in the registry. The ramp
    // decides which field has the keyboard (`focus_field`) and covers the spatial grid
    // itself, and a registered child dropdown's open popover would occlude the ramp's own
    // hit gate (`is_coordinate_covered` excludes by exact id only).
}
