//! Narrow-trait `Spinbox` (Phase 5i). The adapter's detached label sits above the content rect
//! the geometry here works in. Sub-zone
//! hover (the -/+ buttons) is tracked from `PointerMove` against the content rect; a click on
//! the display area enters edit mode and takes focus via `EventCtx::request_focus`.

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
    /// Where the -/+ run begins (an x), and the engraved seam dividing - from
    /// + as `(top, bottom, width, host)` — `None` when the button zone has no
    /// area, and the field is the well alone.
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

impl Paint for Spinbox {
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        // Shape the displayed value exactly as `ctx.text` draws it (size 14,
        // default family) and record char-index → x. Cluster offsets arrive
        // keyed by byte; the editor state is char-indexed.
        let text = self.value_text();
        let clusters =
            crate::backend::window_runner::shaped_cluster_offsets(fs, &text, 14.0, None);
        let mut offsets = vec![0.0f32; text.chars().count() + 1];
        for (byte, x) in clusters {
            let ci = text[..byte.min(text.len())].chars().count();
            if ci < offsets.len() {
                offsets[ci] = x;
            }
        }
        let mut current = 0.0;
        for off in offsets.iter_mut() {
            if *off == 0.0 {
                *off = current;
            } else {
                current = *off;
            }
        }
        self.glyph_offsets = offsets;
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::spinbox_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let g = self.geom(rect);
        let radius = crate::layout::spinbox_corner_radius();
        let rounded = radius > 0.0;
        let display_bg = if self.editing { [0.06, 0.10, 0.18, 1.0] } else { colors::spinbox_display() };
        let inc_col = if self.hover_inc { colors::spinbox_button_hover() } else { colors::spinbox_button() };
        let dec_col = if self.hover_dec { colors::spinbox_button_hover() } else { colors::spinbox_button() };

        if crate::layout::control_relief() {
            // The DE relief style: transparent faces, the relief is the
            // chrome (see [`Self::relief_parts`]). Flat prims first — hover
            // washes and the editing cue survive a flat host's rounded-quad
            // bridge, the carves are re-emitted host-side.
            if let Some(rel) = self.relief_parts(rect) {
                // Carved inside the control as every well is.
                let r = rel.radius;
                let (field, radii) = crate::layout::carve_inside(rel.rect, (r, r, r, r), rel.depth);
                if let Some((split, _)) = rel.run {
                    let wash = [1.0, 1.0, 1.0, 0.06];
                    let sx = g.seam_x;
                    let (top, h) = (field.y, field.height);
                    if self.hover_dec {
                        ctx.rounded_rect(Rect { x: split, y: top, width: sx - split, height: h }, 0.0, (false, false, false, false), wash);
                    }
                    if self.hover_inc {
                        ctx.rounded_rect(
                            Rect { x: sx, y: top, width: field.x + field.width - sx, height: h },
                            radii.1,
                            (false, true, true, false),
                            wash,
                        );
                    }
                }
                if self.editing {
                    // Editing cue: an accent hairline on the well floor under
                    // the value, plus the caret — a flat stand-in for the
                    // tinted-recess focus treatment the pane's relief tuple
                    // cannot carry.
                    let accent = colors::highlight_primary_color();
                    ctx.quad(
                        Rect { x: g.x + crate::layout::CONTROL_TEXT_INSET, y: g.y + g.h - 4.0, width: g.w * 0.55 - 8.0, height: 1.5 },
                        accent,
                    );
                    let cursor_x = (g.x + crate::layout::CONTROL_TEXT_INSET + self.caret_offset(self.cursor_idx)).min(g.x + g.w * 0.55 - 4.0);
                    let cursor_y = g.y + (g.h - 14.0) / 2.0;
                    ctx.quad(Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 }, [0.80, 0.80, 0.85, 1.0]);
                }
                // A field in its two forms: the value's well ending in the
                // -/+ run, or (no room for the buttons) all well.
                use crate::scene::paint::Field;
                match rel.run {
                    Some((split, (sa, sb, sw, host))) => {
                        ctx.field(&Field::ending_in_run(field, radii, rel.depth, split));
                        ctx.groove(sa, sb, sw, rel.depth, host);
                    }
                    None => ctx.field(&Field::well(field, radii, rel.depth)),
                }
            }
        } else if rounded {
            let rc = (true, true, true, true);
            let border_color = if self.editing {
                [0.20, 0.50, 0.85, 1.0]
            } else if self.hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            ctx.rounded_rect(Rect { x: g.x, y: g.y, width: g.w, height: g.h }, radius, rc, border_color);
            ctx.rounded_rect(
                Rect { x: g.x + 1.0, y: g.y + 1.0, width: g.w - 2.0, height: g.h - 2.0 },
                radius - 1.0,
                rc,
                display_bg,
            );
            if g.btn_h > 0.0 && g.btn_w > 0.0 {
                ctx.rounded_rect(
                    Rect { x: g.split_dec + g.pad, y: g.btn_y, width: g.btn_w, height: g.btn_h },
                    0.0,
                    (false, false, false, false),
                    dec_col,
                );
                ctx.rounded_rect(
                    Rect { x: g.split_dec + g.pad + g.btn_w, y: g.btn_y, width: g.btn_w, height: g.btn_h },
                    radius,
                    (false, true, true, false),
                    inc_col,
                );
            }
            if self.editing {
                let cursor_x = (g.x + crate::layout::CONTROL_TEXT_INSET + self.caret_offset(self.cursor_idx)).min(g.x + g.w * 0.55 - 4.0);
                let cursor_y = g.y + (g.h - 14.0) / 2.0;
                ctx.rounded_rect(
                    Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 },
                    0.0,
                    (false, false, false, false),
                    [0.80, 0.80, 0.85, 1.0],
                );
            }
        } else {
            ctx.quad(Rect { x: g.x, y: g.y, width: g.w, height: g.h }, display_bg);
            if g.btn_h > 0.0 && g.btn_w > 0.0 {
                ctx.quad(Rect { x: g.split_dec + g.pad, y: g.btn_y, width: g.btn_w, height: g.btn_h }, dec_col);
                ctx.quad(Rect { x: g.split_dec + g.pad + g.btn_w, y: g.btn_y, width: g.btn_w, height: g.btn_h }, inc_col);
            }
            if self.editing {
                let border_color = [0.20, 0.50, 0.85, 1.0];
                ctx.quad(Rect { x: g.x, y: g.y, width: g.w, height: 1.0 }, border_color);
                ctx.quad(Rect { x: g.x, y: g.y + g.h - 1.0, width: g.w, height: 1.0 }, border_color);
                ctx.quad(Rect { x: g.x, y: g.y, width: 1.0, height: g.h }, border_color);
                ctx.quad(Rect { x: g.x + g.w - 1.0, y: g.y, width: 1.0, height: g.h }, border_color);

                let cursor_x = (g.x + crate::layout::CONTROL_TEXT_INSET + self.caret_offset(self.cursor_idx)).min(g.x + g.w * 0.55 - 4.0);
                let cursor_y = g.y + (g.h - 14.0) / 2.0;
                ctx.quad(Rect { x: cursor_x, y: cursor_y, width: 1.5, height: 14.0 }, [0.80, 0.80, 0.85, 1.0]);
            }
        }

        // Value, unit, and -/+ glyphs.
        let tc = colors::spinbox_text_color();
        let text_color = [(tc[0] * 255.0) as u8, (tc[1] * 255.0) as u8, (tc[2] * 255.0) as u8];
        // The value and its unit live in the FIELD, which ends where the -/+
        // buttons begin (`split_dec`). The caret above is already clamped to
        // that field; the text it belongs to was not, so a long value ran
        // under the buttons and out of the control.
        let field = Some([g.x, g.y, g.text_end, g.y + g.h]);
        ctx.text_with(self.value_text(), g.x + crate::layout::CONTROL_TEXT_INSET, crate::layout::align_text_y(g.y, g.h, 14.0, 0.0), 14.0, text_color, None, field);
        if let Some(ref unit) = self.unit {
            ctx.text_with(unit.clone(), g.x + crate::layout::CONTROL_TEXT_INSET + 36.0, crate::layout::align_text_y(g.y, g.h, 11.0, 0.0), 11.0, [0x73, 0x73, 0x7a], None, field);
        }
        if g.btn_w > 0.0 {
            let dec_center_x = g.dec_c;
            let inc_center_x = g.inc_c;
            let ty = crate::layout::align_text_y(g.y, g.h, 12.0, 0.0);
            let dec_box = Some([g.run_x, g.y, g.seam_x, g.y + g.h]);
            let inc_box = Some([g.seam_x, g.y, g.run_end, g.y + g.h]);
            ctx.text_with("-".to_string(), dec_center_x - 4.0, ty, 12.0, text_color, None, dec_box);
            ctx.text_with("+".to_string(), inc_center_x - 4.0, ty, 12.0, text_color, None, inc_box);
        }
    }
}

impl Input for Spinbox {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was = self.hovered;
                self.hovered = *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                if !self.hovered {
                    let changed = self.hover_dec || self.hover_inc;
                    self.hover_dec = false;
                    self.hover_inc = false;
                    return changed || was != self.hovered;
                }
                let g = self.geom(r);
                let in_y = *py >= g.btn_y && *py < g.btn_y + g.btn_h;
                let hd = in_y && *px >= g.run_x && *px < g.seam_x;
                let hi = in_y && *px >= g.seam_x && *px < g.run_end;
                let changed = hd != self.hover_dec || hi != self.hover_inc;
                self.hover_dec = hd;
                self.hover_inc = hi;
                changed || was != self.hovered
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, .. } => {
                let g = self.geom(ectx.rect);
                let in_y = *py >= g.btn_y && *py < g.btn_y + g.btn_h;
                if in_y && *px >= g.run_x && *px < g.seam_x {
                    self.step_by(-1);
                    true
                } else if in_y && *px >= g.seam_x && *px < g.run_end {
                    self.step_by(1);
                    true
                } else if *px < g.split_dec {
                    self.begin_edit(false);
                    self.cursor_idx = self
                        .x_to_idx(px - (g.x + crate::layout::CONTROL_TEXT_INSET))
                        .min(self.edit_buffer.chars().count());
                    ectx.request_focus();
                    true
                } else {
                    false
                }
            }
            Event::MouseWheel { delta, .. } => {
                // Wheel up steps up, wheel down steps down, one step per notch;
                // fractional (trackpad) notches accumulate. Always consumed, so
                // a host's page never scrolls under a spinbox mid-gesture.
                self.wheel_accum += delta.value_notches_y();
                while self.wheel_accum >= 1.0 {
                    self.wheel_accum -= 1.0;
                    self.step_by(1);
                }
                while self.wheel_accum <= -1.0 {
                    self.wheel_accum += 1.0;
                    self.step_by(-1);
                }
                true
            }
            Event::KeyInput(key_event) => {
                if !self.editing || key_event.state != ElementState::Pressed {
                    return false;
                }
                let mut state = TextEditorState {
                    buffer: self.edit_buffer.clone(),
                    cursor_idx: self.cursor_idx,
                    select_anchor: None,
                    all_selected: false,
                };
                let mut handled = false;
                match &key_event.logical_key {
                    Key::Named(NamedKey::Backspace) => handled = state.delete_backwards(),
                    Key::Named(NamedKey::Delete) => handled = state.delete_forwards(),
                    Key::Named(NamedKey::ArrowLeft) => handled = state.move_cursor_left(false),
                    Key::Named(NamedKey::ArrowRight) => handled = state.move_cursor_right(false),
                    Key::Named(NamedKey::Enter) => {
                        let text = state.buffer.clone();
                        self.parse_into_value(&text);
                        self.editing = false;
                        handled = true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.editing = false;
                        handled = true;
                    }
                    _ => {
                        if let Some(text) = &key_event.text {
                            for ch in text.chars() {
                                match ch {
                                    '-' if state.cursor_idx == 0 && !state.buffer.starts_with('-') => {
                                        state.insert_text("-");
                                        handled = true;
                                    }
                                    '.' if self.decimals > 0 && !state.buffer.contains('.') => {
                                        state.insert_text(".");
                                        handled = true;
                                    }
                                    '0'..='9' => {
                                        state.insert_text(&ch.to_string());
                                        handled = true;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                if self.editing {
                    self.edit_buffer = state.buffer;
                    self.cursor_idx = state.cursor_idx;
                }
                handled
            }
            // Focus gained programmatically enters edit mode (legacy `focus()` override);
            // focus loss commits (legacy `unfocus`).
            Event::FocusIn => {
                self.begin_edit(true);
                false
            }
            Event::FocusOut => {
                if self.editing {
                    self.editing = false;
                    let text = self.edit_buffer.clone();
                    self.parse_into_value(&text);
                }
                false
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.formatted_value())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let old_val = self.value;
        self.parse_into_value(val.trim());
        if self.value != old_val {
            if self.editing {
                self.edit_buffer = self.formatted_value();
                self.cursor_idx = self.edit_buffer.chars().count();
            }
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        self.value
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, UiContext};

    /// The value's well and the -/+ run are ONE field: one outline round
    /// both, its seam where the run begins — not a well and a trough side by
    /// side, each turning its own corner there — with the engraved seam
    /// between - and + on top.
    #[test]
    fn a_spinbox_is_one_field_with_its_run_at_the_right_end() {
        use crate::scene::paint::{PaintCtx, Prim};
        if !crate::layout::control_relief() {
            return;
        }
        let sb = Spinbox::new(0, -100, 100, 1);
        let rect = Rect { x: 10.0, y: 20.0, width: 200.0, height: 26.0 };
        let rel = sb.inner().relief_parts(rect).expect("a relief");
        assert_eq!(rel.rect, rect, "the outline is the control as handed over");
        let (split, (sa, sb2, _, _)) = rel.run.expect("a -/+ run");
        assert!(split > rect.x + rect.width * 0.5 && split < rect.x + rect.width, "the run is the right end");
        assert!(sa.0 > split && sa.0 == sb2.0, "the -/+ seam crosses the run");
        let mut pc = PaintCtx::new();
        Paint::paint(sb.inner(), rect, &mut pc);
        let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        let fields: Vec<&Prim> = prims.iter().filter(|p| matches!(p, Prim::Field { .. })).collect();
        assert_eq!(fields.len(), 1, "{prims:?}");
        if let Prim::Field { split: s, .. } = fields[0] {
            assert_eq!(*s, split);
        }
        assert!(
            !prims.iter().any(|p| matches!(p, Prim::Recess { .. } | Prim::Trough { .. })),
            "no well or ring of its own beside the field"
        );
        assert!(prims.iter().any(|p| matches!(p, Prim::Groove { .. })), "the -/+ seam");
    }

    #[test]
    fn spinbox_button_zones_step_the_value() {
        let mut ctx = UiContext::new();
        let mut sb = Spinbox::new(0, -100, 100, 1);
        let (id, ptr) = (sb.id(), sb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut sb, 10.0, 20.0, 100.0, 26.0);

        // Legacy test: click at (75, 33) lands in the decrement zone.
        assert!(sb.mouse_input(MouseButton::Left, ElementState::Pressed, 75.0, 33.0, &mut ctx));
        assert_eq!(sb.value, -1);
        assert!(sb.take_change());

        // Increment zone (past 77.5% of the width).
        assert!(sb.mouse_input(MouseButton::Left, ElementState::Pressed, 92.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 0);
    }

    #[test]
    fn spinbox_buttons_step_visibly_while_editing() {
        // Row-selection focus puts the spinbox in edit mode (FocusIn →
        // begin_edit): the display shows edit_buffer. Stepping must commit
        // and refresh the buffer, or the value moves invisibly and the next
        // FocusOut commit resets it to the stale text.
        let mut ctx = UiContext::new();
        let mut sb = Spinbox::new(6, 0, 100, 1);
        let (id, ptr) = (sb.id(), sb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut sb, 10.0, 20.0, 100.0, 26.0);
        sb.begin_edit(true);
        assert_eq!(sb.edit_buffer, "6");

        assert!(sb.mouse_input(MouseButton::Left, ElementState::Pressed, 92.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 7);
        assert_eq!(sb.edit_buffer, "7");
        assert!(sb.take_change());

        // FocusOut now commits the refreshed buffer — the step survives.
        sb.handle_event(&Event::FocusOut, &mut ctx);
        assert_eq!(sb.value, 7);
    }

    #[test]
    fn spinbox_wheel_steps_by_notch_and_accumulates_fractions() {
        use crate::widget::MouseScrollDelta;
        let mut ctx = UiContext::new();
        let mut sb = Spinbox::new(10, 0, 100, 5);
        let (id, ptr) = (sb.id(), sb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut sb, 10.0, 20.0, 100.0, 26.0);

        // One notch up steps up, one notch down steps down — and the wheel is consumed.
        assert!(sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 50.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 15);
        assert!(sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 10);
        assert!(sb.take_change());

        // Fractional (trackpad) notches accumulate to a whole step, consumed all the while.
        assert!(sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 0.5), 50.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 10, "half a notch: no step yet");
        assert!(sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 0.5), 50.0, 33.0, &mut ctx));
        assert_eq!(sb.value, 15, "the second half completes the notch");

        // Outside the rect the wheel is not the spinbox's (hit-gated by the adapter).
        assert!(!sb.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 200.0, 200.0, &mut ctx));
        assert_eq!(sb.value, 15);
    }

    #[test]
    fn spinbox_value_string_decimals_round_trip() {
        let mut sb = Spinbox::new(150, 0, 1000, 5).with_decimals(2);
        assert_eq!(sb.get_value_string(), Some("1.50".to_string()));
        assert!(sb.set_value_string("2.75"));
        assert_eq!(sb.value, 275);
        assert_eq!(sb.value(), 275);
    }
}
