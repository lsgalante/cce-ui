//! `Spinbox`: a number in a field with a -/+ run at its right end — one field, the well and
//! the run sharing an outline (`relief_parts`). The value is an integer stepped within a range,
//! shown with an optional unit and number of decimals. The -/+ halves step it, the wheel steps it
//! by notches (accumulating a trackpad's fractions), and so do Up / Down while it has the
//! keyboard; a click on the value opens it for typing and takes the focus through
//! `EventCtx::request_focus`. The adapter's detached label sits above the content rect the
//! geometry here works in.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, its relief and geometry, construction, the caret, the value's text, parsing and stepping, the builders, `impl Layout` |
//! | `paint` | `impl Paint`: the field and its run, the value and caret, the -/+ glyphs |
//! | `input` | `impl Input`: the -/+ zones, the wheel, keys, typing, the reader's value |

mod input;
mod paint;
#[cfg(test)]
mod tests;

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, NamedKey,
    Paint, TextEditorState,
};

#[derive(Debug, Clone)]
pub struct Spinbox {
    pub value: i32,
    pub(crate) min: i32,
    pub(crate) max: i32,
    pub(crate) step: i32,
    pub editing: bool,
    pub edit_buffer: String,
    pub cursor_idx: usize,
    hover_dec: bool,
    hover_inc: bool,
    hovered: bool,
    unit: Option<String>,
    pub decimals: u32,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    label: Option<String>,
    /// Wheel notches carried between events: a trackpad's fractional notches
    /// add up to whole steps instead of being dropped.
    wheel_accum: f32,
    /// Whether it has the keyboard. Enter ends editing but not focus, and a focused box that
    /// is not editing goes back into it on the keys that edit or step it.
    focused: bool,
    /// Char-index → x offsets of the value text, recorded by [`Paint::prepare_text`]
    /// from the same shaped buffer the renderer draws (`ctx.text`, size 14, default
    /// family). The caret and click→index math read these; the `8.4` px/char guess
    /// they used before drifted off the glyphs. Empty until the first shape.
    glyph_offsets: Vec<f32>,
}

/// The zone geometry shared by paint and input, derived from the content rect.
/// A spinbox's relief: one field, its well and its -/+ run — see
/// [`Spinbox::relief_parts`].
#[derive(Clone, Copy, Debug)]
pub struct SpinRelief {
    /// The field's outline, as the host was handed it.
    pub rect: Rect,
    pub radius: f32,
    pub depth: f32,
    /// Where the -/+ run begins (an x), and the engraved seam dividing the minus from the
    /// plus as `(top, bottom, width, host)` — `None` when the button zone has no area, and
    /// the field is the well alone.
    pub run: Option<(f32, ((f32, f32), (f32, f32), f32, Rect))>,
}

struct SpinGeom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    split_dec: f32,
    btn_y: f32,
    btn_h: f32,
    btn_w: f32,
    pad: f32,
    /// The -/+ run: where it begins (under the relief, the field's seam,
    /// where the well's floor ends and its wall meets the run's lip), the
    /// line between - and +, and where it ends. Hit zones, hover washes,
    /// glyphs and the relief all read these, so they cannot disagree.
    run_x: f32,
    seam_x: f32,
    run_end: f32,
    /// The middles of the - and + halves, where the glyphs stand.
    dec_c: f32,
    inc_c: f32,
    /// Where the value's text is clipped: the end of the well's floor.
    text_end: f32,
}

impl Spinbox {
    pub fn new(value: i32, min: i32, max: i32, step: i32) -> Adapted<Spinbox> {
        Adapted::new(Spinbox {
            value,
            min,
            max,
            step,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            hover_dec: false,
            hover_inc: false,
            hovered: false,
            unit: None,
            decimals: 0,
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            label: None,
            wheel_accum: 0.0,
            focused: false,
            glyph_offsets: Vec::new(),
        })
    }

    /// The caret x offset for a char index, from the shaped offsets when present
    /// (falling back to the legacy estimate only if nothing shaped yet).
    fn caret_offset(&self, idx: usize) -> f32 {
        self.glyph_offsets
            .get(idx)
            .copied()
            .unwrap_or(idx as f32 * 8.4)
    }

    /// Click x (relative to the text origin) → char index, nearest shaped offset.
    fn x_to_idx(&self, relative_x: f32) -> usize {
        if self.glyph_offsets.is_empty() {
            return ((relative_x / 8.4).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize;
        }
        let mut closest = 0;
        let mut min_diff = f32::MAX;
        for (i, &pos) in self.glyph_offsets.iter().enumerate() {
            let diff = (pos - relative_x).abs();
            if diff < min_diff {
                min_diff = diff;
                closest = i;
            }
        }
        closest
    }

    pub fn set_unit(&mut self, unit: &str) {
        self.unit = Some(unit.to_string());
    }

    pub fn range(&self) -> (i32, i32) {
        (self.min, self.max)
    }

    fn geom(&self, rect: Rect) -> SpinGeom {
        let x = rect.x;
        let w = rect.width;
        let pad = crate::layout::spinbox_button_padding();
        let split_dec = x + w * 0.55;
        let btn_w = ((w * 0.45 - 2.0 * pad).max(0.0)) / 2.0;
        let flat_run = split_dec + pad;
        let (run_x, seam_x, run_end, dec_c, inc_c, text_end) = if crate::layout::control_relief() && btn_w > 0.0 {
            // The run's face is inset half a wall from its outline on every
            // side (`Prim::Field`), so it is halved at its own middle and
            // each glyph stands in the middle of its half: the padding about
            // - and + is even. The run begins a wall before the flat
            // layout's buttons, taking the wall a ridge stood on for a few
            // hours on 2026-10-02 (the well's floor ends where it did then).
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let hw = 0.5 * depth;
            let run_x = flat_run - depth;
            let end = x + w;
            let seam = 0.5 * (run_x + end);
            (run_x, seam, end, 0.5 * (run_x + hw + seam), 0.5 * (seam + end - hw), (run_x - hw).min(split_dec))
        } else {
            let seam = flat_run + btn_w;
            (flat_run, seam, seam + btn_w, flat_run + 0.5 * btn_w, seam + 0.5 * btn_w, split_dec)
        };
        SpinGeom {
            x,
            y: rect.y,
            w,
            h: rect.height,
            split_dec,
            btn_y: rect.y + pad,
            btn_h: (rect.height - 2.0 * pad).max(0.0),
            btn_w,
            pad,
            run_x,
            seam_x,
            run_end,
            dec_c,
            inc_c,
            text_end,
        }
    }

    /// The control's relief set under `control_relief`, shared by the widget's
    /// own `paint` and flat hosts (`ParametersBg`) that must re-emit carves
    /// (their rounded-quad bridge keeps only flat prims — the slider's
    /// `track_relief` precedent). Faces are transparent in this style; the
    /// relief IS the chrome, and it is ONE field ([`crate::scene::paint::Prim::Field`]):
    /// - the value sits in a sunken well (the TextBox language),
    /// - the -/+ pair is a flush run, the field's right end — out to the
    ///   control's edge on the top, right and bottom as a dropdown trigger's
    ///   plate is — and the field's one outline runs round both, its wall
    ///   blending from the well's step to the run's valley at the seam, so
    ///   the edge does not break there. Until 2026-10-01 the run was nested
    ///   inside a full-width well (its face stopping at the base of the
    ///   well's wall), and for a few hours after that the well and the run
    ///   were two carves side by side, each turning its own corner at the
    ///   seam,
    /// - the two buttons divide by an engraved seam, not a wall pair — the
    ///   breadcrumb run's segment language at miniature scale.
    ///
    /// The field's outline is `rect` as given — a host carves it inside or
    /// not by its own rule, as it does every well. `None` when the control
    /// has no area. The caller gates on `control_relief`.
    pub fn relief_parts(&self, rect: Rect) -> Option<SpinRelief> {
        let g = self.geom(rect);
        if g.w <= 0.0 || g.h <= 0.0 {
            return None;
        }
        let radius = crate::layout::spinbox_corner_radius();
        let depth = crate::layout::bevel_width().min(g.h * 0.2);
        let whole = Rect { x: g.x, y: g.y, width: g.w, height: g.h };
        if g.btn_w <= 0.0 || g.btn_h <= 0.0 {
            return Some(SpinRelief { rect: whole, radius, depth, run: None });
        }
        // The run begins at the button hit zone and spans the whole band to
        // the control's right edge.
        let split = g.run_x;
        // Seam floor width: the breadcrumb's SEAM_WIDTH — a hair of flat
        // floor so the crease doesn't alias into a dotted line. It sits on
        // the -/+ hit boundary, not the painted run's midpoint, and crosses
        // the run's face between its outline's walls.
        let sx = g.seam_x;
        let host = Rect { x: split, y: g.y, width: g.x + g.w - split, height: g.h };
        let seam = ((sx, g.y + depth), (sx, g.y + g.h - depth), 0.75, host);
        Some(SpinRelief { rect: whole, radius, depth, run: Some((split, seam)) })
    }

    fn value_text(&self) -> String {
        if self.editing {
            self.edit_buffer.clone()
        } else if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize)
        } else {
            self.value.to_string()
        }
    }

    fn formatted_value(&self) -> String {
        if self.decimals > 0 {
            let divisor = 10.0f32.powi(self.decimals as i32);
            format!("{:.width$}", self.value as f32 / divisor, width = self.decimals as usize)
        } else {
            self.value.to_string()
        }
    }

    fn parse_into_value(&mut self, text: &str) {
        let old_val = self.value;
        if self.decimals > 0 {
            if let Ok(val_f) = text.parse::<f32>() {
                let divisor = 10.0f32.powi(self.decimals as i32);
                self.value = ((val_f * divisor).round() as i32).clamp(self.min, self.max);
            }
        } else if let Ok(val) = text.parse::<i32>() {
            self.value = val.clamp(self.min, self.max);
        }
        if self.value != old_val {
            self.just_changed = true;
        }
    }

    fn begin_edit(&mut self, cursor_at_end: bool) {
        self.editing = true;
        self.edit_buffer = self.formatted_value();
        if cursor_at_end {
            self.cursor_idx = self.edit_buffer.chars().count();
        }
    }

    /// Step the value by `delta` steps. While editing (the display shows
    /// `edit_buffer`), commit the typed text first and refresh the buffer
    /// after — otherwise the value moves invisibly behind a frozen buffer,
    /// and the next commit (FocusOut/Enter) resets it to the stale text.
    fn step_by(&mut self, delta: i32) {
        if self.editing {
            let text = self.edit_buffer.clone();
            self.parse_into_value(&text);
        }
        let old_val = self.value;
        self.value = (self.value + delta * self.step).clamp(self.min, self.max);
        if self.value != old_val {
            self.just_changed = true;
        }
        if self.editing {
            self.edit_buffer = self.formatted_value();
            self.cursor_idx = self.edit_buffer.chars().count();
        }
    }
}

impl Adapted<Spinbox> {
    pub fn with_unit(mut self, unit: &str) -> Self {
        self.set_unit(unit);
        self
    }

    pub fn with_decimals(mut self, decimals: u32) -> Self {
        self.decimals = decimals;
        self
    }
}

impl Layout for Spinbox {


    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::spinbox_height()))
    }
}
