//! `Slider` and `RangeSlider`: detached-label widgets (the adapter draws the control label in the
//! strip above the content rect the geometry here works in) whose value is a swelling band in a
//! track. Drags are host-driven through the `Input` drag hooks; the readout's edit mode takes the
//! keyboard with `EventCtx::request_focus`, and the wheel is gated by the context's scroll-gesture
//! state through `EventCtx::ui`.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | `Slider`: its geometry, construction and setters, value stepping, the readout's commit, `impl Layout`, the label strip |
//! | `band` | the band's profile and its shape, public for app-owned scrubbers |
//! | `paint` | `impl Paint for Slider` |
//! | `input` | `impl Input for Slider`: presses, drags, the wheel, keys, the readout, the tick |
//! | `range` | `RangeSlider` whole |

mod band;
mod input;
mod paint;
mod range;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod tests;

pub use band::*;
pub use range::{ActiveThumb, RangeSlider};

use crate::color;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton,
    NamedKey, Paint, TextEditorState,
};

/// The track/readout/thumb geometry shared by the paint and input paths, derived from the
/// content rect (the legacy code re-derived this in five places from the base rect).
struct SliderGeom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    track_x: f32,
    track_w: f32,
}

#[derive(Debug, Clone)]
pub struct Slider {
    dragging: bool,
    pub(crate) value: f32,
    drag_offset: f32,
    pub(crate) scroll_enabled: bool,
    show_readout: bool,
    pub(crate) editing: bool,
    edit_buffer: String,
    min: f32,
    max: f32,
    /// A SOFT range: a value typed into the readout past either end widens
    /// the range to hold it, where a hard range clamps it to the end. For
    /// a value with no natural bounds, whose range is only a scale to drag
    /// over — the host re-chooses it around the value. Off by default; a
    /// drag and the wheel stop at the ends either way.
    soft: bool,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    label: Option<String>,
    /// Wheel-scroll glide velocity (normalized value units/sec) and the last
    /// wheel-event instant — the Ramp hover-scroll idiom: when the event
    /// stream stops (fingers lifted), `tick` keeps the value coasting with
    /// exponential decay instead of stopping dead.
    scroll_vel: f32,
    last_wheel: Option<web_time::Instant>,
    /// Keyboard focus (FocusIn / FocusOut): the band lights in the highlight;
    /// the arrows adjust, Home / End go to the ends, Enter opens the readout.
    focused: bool,
    /// Readout / edit-buffer display precision (decimal places).
    decimals: usize,
    /// The pointer is over the row (`MouseEnter` / `MouseLeave`, synthesized
    /// by the adapter's hover bookkeeping): the band lifts, the way a well's
    /// frame or a dropdown's border does. A slider had no hover at all until
    /// 2026-09-28, so a params pane answered the pointer on every row but
    /// its sliders.
    hovered: bool,
}

impl Slider {
    pub fn new() -> Adapted<Slider> {
        Adapted::new(Slider {
            dragging: false,
            value: 0.5,
            drag_offset: 0.0,
            scroll_enabled: true,
            show_readout: false,
            editing: false,
            edit_buffer: String::new(),
            min: 0.0,
            max: 1.0,
            soft: false,
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            label: None,
            hovered: false,
            scroll_vel: 0.0,
            last_wheel: None,
            focused: false,
            decimals: 2,
        })
    }

    pub fn set_range(&mut self, min: f32, max: f32) {
        self.min = min;
        self.max = max;
    }

    /// See the `soft` field.
    pub fn set_soft(&mut self, soft: bool) {
        self.soft = soft;
    }

    pub fn set_scroll(&mut self, enabled: bool) {
        self.scroll_enabled = enabled;
    }

    pub fn set_value(&mut self, val: f32) {
        self.value = val.clamp(0.0, 1.0);
    }

    pub fn set_readout(&mut self, enabled: bool) {
        self.show_readout = enabled;
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn get_scaled_value(&self) -> f32 {
        self.min + self.value * (self.max - self.min)
    }

    pub fn range(&self) -> (f32, f32) {
        (self.min, self.max)
    }

    /// Readout precision, for a host that changes it after construction
    /// (a float3 group gaining a trackball reads to a third decimal).
    pub fn set_decimals(&mut self, decimals: usize) {
        self.decimals = decimals;
    }

    /// The widest range a notch steps a flat 2% of. Wider than this, the
    /// step follows the value's magnitude instead ([`Self::notch_step`]).
    pub const FINE_SPAN: f32 = 20.0;

    /// What one wheel notch, or one arrow press, moves the value by, as a
    /// fraction of the range.
    ///
    /// 2% of the range — for a range up to [`Self::FINE_SPAN`] wide, which
    /// is every slider the toolkit had until a host asked for one over
    /// -1000..1000 (the designer's pull vector, 2026-09-28): there 2% is 40
    /// units a notch, and a value of 0.06 cannot be reached by scrolling at
    /// all. A wide range steps 2% of a SPAN that grows with the value —
    /// `FINE_SPAN` times its magnitude, never less than `FINE_SPAN` itself
    /// and never more than the range — so near zero it moves as a 20-wide
    /// slider does (0.4 a notch, under a hundredth per pixel of trackpad
    /// travel) and far from zero by the full 2%, and the whole range is
    /// still a few dozen notches end to end. By magnitude rather than by a
    /// finer flat step, because a flat step fine enough for 0.06 is one
    /// that takes thousands of notches to reach 1000.
    pub fn notch_step(&self) -> f32 {
        let range = (self.max - self.min).abs();
        if range <= Self::FINE_SPAN {
            return 0.02;
        }
        let span = (Self::FINE_SPAN * self.get_scaled_value().abs().max(1.0)).min(range);
        0.02 * span / range
    }

    pub fn set_scaled_value(&mut self, val: f32) {
        let range = self.max - self.min;
        if range != 0.0 {
            self.value = ((val - self.min) / range).clamp(0.0, 1.0);
        } else {
            self.value = 0.0;
        }
    }

    /// The width the value maps over: the whole track — the band has no thumb
    /// to keep inside the ends.
    fn value_span(&self, g: &SliderGeom) -> f32 {
        g.track_w
    }


    /// Width of the value readout at the slider's right end.
    pub const READOUT_W: f32 = 60.0;
    /// Gap between the track and the readout.
    pub const READOUT_GAP: f32 = 8.0;

    /// What a slider's rect spends on everything but its TRACK: the readout
    /// and its gap when it shows one. `rect width - chrome` is the track a
    /// host gets for a rect — what `ParametersBg` measures to decide whether
    /// a row's label can sit beside the control.
    pub const fn readout_chrome() -> f32 {
        Self::READOUT_W + Self::READOUT_GAP
    }

    fn geom(&self, rect: Rect) -> SliderGeom {
        let x = rect.x;
        let w = rect.width;
        let (track_x, track_w) = if self.show_readout {
            let readout_w = Self::READOUT_W;
            let gap = Self::READOUT_GAP;
            ((x), (w - readout_w - gap).max(10.0))
        } else {
            (x, w)
        };
        SliderGeom { x, y: rect.y, w, h: rect.height, track_x, track_w }
    }

    /// The band's height profile at `x` (`band_profile`, one swell at the value).
    fn band_height_at(&self, g: &SliderGeom, x: f32) -> f32 {
        let vx = g.track_x + self.value * g.track_w;
        band_profile(g.track_x, g.track_w, g.h, x, &[vx], None)
    }

    /// The wheel-capture zone. Band style: an inset halo around the DRAWN
    /// shape — the thin band and the bulge, which travels with the value — so
    /// a scroll near the visible slider adjusts it while the rest of the row
    /// stays the host pane's to scroll. Otherwise: plain rect containment.
    pub fn scroll_hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        const SCROLL_INSET: f32 = 14.0;
        let g = self.geom(rect);
        if px < g.track_x - SCROLL_INSET || px > g.track_x + g.track_w + SCROLL_INSET {
            return false;
        }
        let cy = g.y + g.h * 0.5;
        let x = px.clamp(g.track_x, g.track_x + g.track_w);
        (py - cy).abs() <= self.band_height_at(&g, x) * 0.5 + SCROLL_INSET
    }

    /// The slider: the band spanning the whole track, swelling at the value
    /// (`paint_band_shape`).
    fn paint_band(&self, g: &SliderGeom, ctx: &mut PaintCtx) {
        // A band has no rim to light: focused, the band itself is the
        // highlight; hovered, it lifts by the dropdown border's step.
        let color = if self.dragging {
            color::slider_thumb_drag()
        } else if self.focused {
            crate::color::highlight_primary_color()
        } else if self.hovered {
            let c = color::slider_thumb();
            [(c[0] + 0.15).min(1.0), (c[1] + 0.15).min(1.0), (c[2] + 0.15).min(1.0), c[3]]
        } else {
            color::slider_thumb()
        };
        paint_band_shape(ctx, g.track_x, g.track_w, g.y + g.h * 0.5, color, &|x| self.band_height_at(g, x));
    }

    /// The pointer is over the row.
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    /// Set the hover directly, for a host that paints one slider as a STAMP
    /// over several rows and does its own hit-testing — the designer's
    /// dialog — rather than routing `MouseEnter` / `MouseLeave` to it.
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn scaled_string(&self) -> String {
        format!("{:.*}", self.decimals, self.min + self.value * (self.max - self.min))
    }

    fn set_value_marking(&mut self, new_val: f32) -> bool {
        if (new_val - self.value).abs() > 0.0001 {
            self.value = new_val;
            self.just_changed = true;
            if self.editing {
                self.edit_buffer = self.scaled_string();
            }
            true
        } else {
            false
        }
    }

    fn commit_edit(&mut self) {
        if self.editing {
            self.editing = false;
            let old_val = self.value;
            if let Ok(new_val) = self.edit_buffer.parse::<f32>() {
                // A soft range widens to hold what was typed; the value is
                // the number, not the end it would clamp to.
                if self.soft && new_val.is_finite() && (new_val < self.min || new_val > self.max) {
                    self.min = self.min.min(new_val);
                    self.max = self.max.max(new_val);
                    self.just_changed = true;
                }
                let range = self.max - self.min;
                if range != 0.0 {
                    self.value = ((new_val - self.min) / range).clamp(0.0, 1.0);
                } else {
                    self.value = 0.0;
                }
            }
            if (self.value - old_val).abs() > 0.0001 {
                self.just_changed = true;
            }
        }
    }
}

impl Adapted<Slider> {
    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.set_range(min, max);
        self
    }

    pub fn with_scroll(mut self, enabled: bool) -> Self {
        self.scroll_enabled = enabled;
        self
    }


    pub fn with_value(mut self, val: f32) -> Self {
        self.set_value(val);
        self
    }

    pub fn with_readout(mut self, enabled: bool) -> Self {
        self.show_readout = enabled;
        self
    }

    /// Readout display precision in decimal places (default 2).
    pub fn with_decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

    /// See the `soft` field.
    pub fn with_soft(mut self, soft: bool) -> Self {
        self.set_soft(soft);
        self
    }

}

impl Layout for Slider {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::slider_height()))
    }
}

/// The detached-label strip height above a content rect (zero unlabeled) — the
/// adapter's `Widget::label_offset` over a model's synced label, for models whose
/// cached rect is the whole block.
pub(crate) fn detached_strip(label: &Option<String>) -> f32 {
    // An EMPTY label is no label: `Adapted::clear_label` syncs one to take a
    // label off a widget that stores whatever it is handed.
    if label.as_deref().is_some_and(|l| !l.is_empty()) { crate::layout::control_label_strip() } else { 0.0 }
}
