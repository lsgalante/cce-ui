//! Narrow-trait `TextBox` (Phase 5q). The widest-surface leaf so far: real selection-aware
//! clipboard (the new `Input` cut/copy/paste/select-all/clear hooks — their defaults replicate
//! the whole-value `WidgetHost` defaults for everyone else), load-bearing glyph shaping through
//! `Paint::prepare_text` (cursor↔pixel mapping reads the measured advances), the row-hit
//! restoration (`Layout::hit_row_rect` — cce-files' save-name box relies on row hits), a
//! width/max-width clamp on both rect paths (`Layout::adjust_rect` + `adjust_row_rect`), the
//! ungated `Layout::rect_assigned` (scroll re-clamp on every `set_rect`, hidden or not), and
//! `Input::tracks_base_focus = false` (legacy `focus()` never set the base flag — the detached
//! label must not color as focused).
//!
//! Parity notes:
//! - The legacy render split is asymmetric and preserved faithfully: the non-rounded path
//!   (`extra_quads`) draws at the full base x/width with a disabled special-case; the rounded
//!   path (`all_rounded_quads`) has NO disabled branch.
//! - Releases: legacy `mouse_input` hit-gated releases too (out-of-rect releases were dropped).
//!   The adapter delivers releases ungated, so the model re-checks containment itself against
//!   the plain rect (the row-substituted release geometry is approximated — flagged).
//! - Wheel scrolling is now hit-gated by the adapter (legacy hosts called `mouse_wheel`
//!   directly on the hovered widget, so the gate should be a no-op in practice — flagged).

use crate::widget::*;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::history::History;
use std::sync::OnceLock;

static FONT_DB: OnceLock<resvg::usvg::fontdb::Database> = OnceLock::new();

pub fn get_font_db() -> &'static resvg::usvg::fontdb::Database {
    FONT_DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        #[cfg(not(target_arch = "wasm32"))]
        {
            db.load_system_fonts();
            db.load_fonts_dir(crate::fonts_dir());
        }
        // A page has neither: the fonts it handed the browser shell.
        #[cfg(target_arch = "wasm32")]
        crate::page_fonts::load_into(&mut db);
        db
    })
}

/// Everything [`TextBox`]'s `prepare_text` output depends on.
#[derive(Debug, Clone, PartialEq)]
struct PrepKey {
    text: String,
    placeholder: bool,
    password: bool,
    font_size_bits: u32,
    font: Option<String>,
    attrs: crate::scene::paint::TextAttrs,
    scale_bits: u32,
    vertical: bool,
    /// `Some(the wrap width's bits)` for a multiline box ([`TextBox::wrap_width`]).
    wrap: Option<u32>,
    /// The width a right-to-left paragraph is set against the right of (the box less its
    /// padding), as bits.
    room_bits: u32,
}

/// What a [`TextBox`]'s cached advances were measured for.
#[derive(Debug, Clone, PartialEq)]
struct AdvanceKey {
    text: String,
    font_size_bits: u32,
    font: Option<String>,
    attrs: crate::scene::paint::TextAttrs,
    scale_bits: u32,
}

#[derive(Debug, Clone)]
pub struct TextBox {
    pub text: String,
    pub editing: bool,
    pub edit_buffer: String,
    pub(crate) just_changed: bool,
    pub disabled: bool,
    pub all_selected: bool,
    pub cursor_idx: usize,
    pub select_anchor: Option<usize>,
    pub dragging: bool,
    pub just_focused: bool,
    pub drag_start_idx: Option<usize>,
    pub max_width: Option<f32>,
    pub width: Option<f32>,
    pub is_password: bool,
    pub multiline: bool,
    /// Per-widget wrap override; `None` defers to the global `textbox_line_wrap()`
    /// style. Flowed-text consumers (an email body) set this to keep wrapping even
    /// when the user's config turns multiline wrap off DE-wide.
    pub line_wrap_override: Option<bool>,
    pub draw_bg_border: bool,
    /// The box's right end meets another control (a parameter pane's
    /// completion picker): its well's right corners are square, so the two
    /// read as one field with a seam between them.
    pub joined_right: bool,
    pub text_color: Option<[u8; 3]>,
    pub font_size: f32,
    pub font_family: String,
    /// Italic / weight the value text is drawn AND measured with (see
    /// `value_font`) — default for every box but one showing a particular
    /// face of `font_family`.
    pub font_attrs: crate::scene::paint::TextAttrs,
    pub placeholder: Option<String>,
    /// A search box (`with_search`): its menu offers Clear. Known by this flag, never by
    /// its placeholder's text, which is translated.
    pub is_search: bool,
    pub editor_state: TextEditorState,
    /// Edit history for the current editing session (cleared by
    /// `begin_editing`): a typed run is one step, a deleted run one, a
    /// paste/cut/selection-replacement one. Stepped by `ContextAction::Undo`
    /// / `Redo`, which the runner routes here on the `undo` / `redo` chords
    /// while the box is focused and editing.
    pub history: History<TextEditorState>,
    pub scroll_y: f32,
    pub scroll_x: f32,
    /// Smooth-scroll driver behind `scroll_x`/`scroll_y` (see `ScrollRegion::motion`).
    scroll_motion: ScrollMotion,
    /// `(max_x, max_y)` as of the last wheel — the glide's bounds, so `tick`
    /// never re-wraps the text just to re-derive them.
    scroll_max: (f32, f32),
    default_font_size: f32,
    default_font_family: String,
    pub cursor_x_offset: f32,
    pub glyph_positions: Vec<f32>,
    pub total_text_width: f32,
    /// The shaped advance of one column, cached by [`Paint::prepare_text`] from the same
    /// cosmic-text path that draws the value text. `char_width()` prefers this over the
    /// SVG-rasterized `measure_text_width`, which reports inked extent (and resolves generic
    /// families through fontdb, not cosmic-text) — a per-column error that made the multiline
    /// selection highlight drift off the glyphs. 0.0 until the first `prepare_text`.
    shaped_char_advance: f32,
    /// Each char's shaped advance in the text being wrapped (logical px; a cluster's
    /// whole width on its first char, 0 on the rest and on a newline), for the key it was
    /// measured for. Wrapping sums these against the width, so a proportional face and
    /// two-column CJK wrap where they are drawn. Filled from the paint's font system in
    /// `prepare_text`, else from `geometry_font_system` when an edit outruns the frame.
    advances: std::cell::RefCell<Option<(AdvanceKey, Vec<f32>)>>,
    /// How far right a right-to-left paragraph is set so its right edge meets the box's
    /// (0 for left-to-right text, and for a line too wide to fit): the single line's, and
    /// each wrapped line's. Already in `glyph_positions` / `line_glyph_positions`, so
    /// carets, clicks and selections follow; the drawing adds it to the text's x.
    glyph_shift: f32,
    line_shift: Vec<f32>,
    /// The shaped runs those positions came from, for selections drawn as the boxes of
    /// the clusters they cover (two pieces where one crosses a change of direction).
    glyph_run: Option<crate::backend::text::ShapedRun>,
    line_runs: Vec<crate::backend::text::ShapedRun>,
    /// Multiline counterpart of `glyph_positions`: per WRAPPED line, per-column x
    /// offsets of that line as drawn (`[line][col]`, one extra entry per line = its
    /// total advance), recorded by [`Paint::prepare_text`] over the same wrap the
    /// paint uses. The multiline caret, selection, click→column, and
    /// scroll-to-cursor read these; the uniform `col * char_width()` grid they used
    /// before is exact only for monospace. Empty for single-line boxes or until the
    /// first shape (readers fall back to the grid).
    line_glyph_positions: Vec<Vec<f32>>,
    /// What the last `prepare_text` shaped. The runner calls `prepare_text` on
    /// every registered box before every frame; when none of these inputs moved,
    /// the offsets above are still right and only the caret is re-read.
    prep_key: Option<PrepKey>,
    pub update_on_type: bool,
    /// Synced control label ([`Paint::sync_label`]) — drives the detached strip offset.
    label: Option<String>,
    /// Own hover flag, maintained from `MouseEnter`/`MouseLeave` (adapter bookkeeping).
    hovered: bool,
    /// The laid-out base rect, cached from [`Layout::rect_assigned`] — the cursor/scroll math
    /// reads geometry between events, which the narrow traits don't otherwise carry.
    rect: Rect,
    /// An input method's composition, shown in `edit_buffer` as a PROVISIONAL run:
    /// its start and length in chars. Everything that draws the buffer — wrap,
    /// scroll, caret, the glyph advances — draws it as it draws typed text, and
    /// the selection quads underline it. It is never committed, recorded in the
    /// history or reported as a change: `committed_buffer` is the buffer without
    /// it. See `crate::ime`.
    composing: Option<(usize, usize)>,
    /// The `ime::generation` the run shows.
    ime_seen: u64,
    /// Recessed style: a `Recess` overlay is carved over the box's own fill —
    /// an inset well, the input-direction counterpart of the raised controls.
    recessed: Option<bool>,
}

impl TextBox {
    /// The style in force: the per-widget override (`with_recessed`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new(text: String) -> Adapted<TextBox> {
        let (style_family, style_size) = crate::layout::control_label_font_detached_parsed();
        let editor_state = TextEditorState::new(text.clone());
        Adapted::new(TextBox {
            text,
            editing: false,
            edit_buffer: String::new(),
            just_changed: false,
            disabled: false,
            all_selected: false,
            cursor_idx: 0,
            select_anchor: None,
            dragging: false,
            just_focused: false,
            drag_start_idx: None,
            max_width: None,
            width: None,
            is_password: false,
            multiline: false,
            line_wrap_override: None,
            draw_bg_border: true,
            joined_right: false,
            text_color: None,
            font_size: style_size,
            font_family: style_family.clone(),
            font_attrs: crate::scene::paint::TextAttrs::default(),
            placeholder: None,
            is_search: false,
            editor_state,
            history: History::new(),
            scroll_y: 0.0,
            scroll_x: 0.0,
            scroll_motion: ScrollMotion::new(),
            scroll_max: (0.0, 0.0),
            default_font_size: style_size,
            default_font_family: style_family,
            cursor_x_offset: 0.0,
            glyph_positions: Vec::new(),
            total_text_width: 0.0,
            shaped_char_advance: 0.0,
            advances: std::cell::RefCell::new(None),
            glyph_shift: 0.0,
            line_shift: Vec::new(),
            glyph_run: None,
            line_runs: Vec::new(),
            line_glyph_positions: Vec::new(),
            prep_key: None,
            update_on_type: false,
            label: None,
            hovered: false,
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            recessed: None,
            composing: None,
            ime_seen: 0,
        })
    }

    /// The detached-label strip height above the content (zero unlabeled) — the
    /// adapter's `Widget::label_offset` over the synced label.
    fn label_top(&self) -> f32 {
        crate::widget::input::slider::detached_strip(&self.label)
    }

    fn map_x_to_idx(&self, click_x: f32) -> usize {
        let pad = self.pad();
        let relative_x = click_x - (self.rect.x + pad) + self.scroll_x;
        if self.glyph_positions.is_empty() {
            let char_width = self.char_width();
            return ((relative_x / char_width).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize;
        }

        let mut closest_idx = 0;
        let mut min_diff = f32::MAX;
        for (i, &pos) in self.glyph_positions.iter().enumerate() {
            let diff = (pos - relative_x).abs();
            if diff < min_diff {
                min_diff = diff;
                closest_idx = i;
            }
        }
        // When the box is empty, `prepare_text` shapes the PLACEHOLDER into
        // `glyph_positions`, so the nearest-glyph snap above can land on a
        // placeholder column. Clamp to the real text: the placeholder is
        // painted, not caret-addressable.
        let text_len = if self.editing { self.edit_buffer.chars().count() } else { self.text.chars().count() };
        closest_idx.min(text_len)
    }

    /// The x offset of `col` on wrapped line `line`, from the shaped per-line
    /// offsets when recorded, else the uniform-grid estimate.
    fn line_col_x(&self, line: usize, col: usize) -> f32 {
        self.line_glyph_positions
            .get(line)
            .and_then(|l| l.get(col).copied())
            .unwrap_or_else(|| col as f32 * self.char_width())
    }

    /// An x offset (relative to the text origin) → nearest column on wrapped
    /// line `line`, from the shaped offsets when recorded.
    fn line_x_to_col(&self, line: usize, relative_x: f32) -> usize {
        let Some(offsets) = self.line_glyph_positions.get(line).filter(|l| !l.is_empty()) else {
            return ((relative_x / self.char_width()).round() as isize).max(0) as usize;
        };
        let mut closest = 0;
        let mut min_diff = f32::MAX;
        for (i, &pos) in offsets.iter().enumerate() {
            let diff = (pos - relative_x).abs();
            if diff < min_diff {
                min_diff = diff;
                closest = i;
            }
        }
        closest
    }

    /// The font the value text is drawn in — AND measured in: `prepare_text`
    /// (whose per-glyph positions place the caret, the selection and
    /// click-to-column) shapes with exactly this, and `paint` emits it. It is
    /// [`Paint::text_font`], because that is what actually draws the text:
    /// `Adapted::paint_self` drops the Text prims `paint` emits and redraws
    /// the value through its own-labels bridge in `text_font()` — the box's
    /// family when customized (cce-text-editor's monospace editor), else the
    /// configured control font string.
    ///
    /// Until 2026-09-25 the measurement shaped `font_family` instead (or
    /// `monospace` for a password), a different face from the drawn one, so
    /// the caret drifted off the text a little more with every character —
    /// two dots short after twelve in the login greeter's password box.
    pub fn value_font(&self) -> Option<String> {
        <Self as Paint>::text_font(self)
    }

    /// The shaping half of `prepare_text`: `glyph_positions` and
    /// `total_text_width` for the whole display text, and, multiline,
    /// `line_glyph_positions` per wrapped line as the paint draws them.
    fn shape_columns(&mut self, fs: &mut cosmic_text::FontSystem, key: &PrepKey) {
        let scale = f32::from_bits(key.scale_bits);
        let font_fam = key.font.as_deref();
        let attrs = key.attrs;
        let shared = crate::backend::text::shared_text_buffer;

        let render_text = if key.password {
            "•".repeat(key.text.chars().count())
        } else {
            key.text.clone()
        };

        // A multiline box reads only the per-line offsets; shaping the whole
        // document as one buffer would be its most expensive and least used step.
        self.line_glyph_positions.clear();
        if let Some(bits) = key.wrap {
            // Measure the wrapped text's advances with the paint's font system first, so
            // the wrap below never reaches for the shared one (which may be this one).
            let src = if self.editing { self.edit_buffer.clone() } else { self.text.clone() };
            self.cache_advances(fs, &src);
            let (lines, map) = self.wrap_text(f32::from_bits(bits));
            // Each wrapped line's paragraph, and whether that paragraph is right to left: a
            // paragraph's direction is its first strong character's, for all its lines.
            let para_rtl: Vec<bool> = src.split('\n').map(crate::backend::text::paragraph_rtl).collect();
            let mut para_of_line = vec![0usize; lines.len()];
            let mut para = 0usize;
            let mut seen = vec![false; lines.len()];
            for (ci, ch) in src.chars().enumerate() {
                let line = map[ci].0.min(lines.len() - 1);
                if !seen[line] {
                    seen[line] = true;
                    para_of_line[line] = para;
                }
                if ch == '\n' {
                    para += 1;
                }
            }
            for (line, s) in seen.iter().enumerate() {
                if !s {
                    para_of_line[line] = para;
                }
            }
            let room = f32::from_bits(key.room_bits);
            self.line_shift.clear();
            self.line_runs.clear();
            for (li, line) in lines.iter().enumerate() {
                let line_buffer = shared(fs, line, self.font_size, font_fam, attrs);
                let run = crate::backend::text::shaped_run(&line_buffer, line, scale);
                let rtl = para_rtl.get(para_of_line[li]).copied().unwrap_or(false);
                let shift = if rtl { (room - run.width).max(0.0) } else { 0.0 };
                self.line_glyph_positions.push(run.stops.iter().map(|s| s.1 + shift).collect());
                self.line_shift.push(shift);
                self.line_runs.push(run);
            }
            self.glyph_positions = vec![0.0; render_text.chars().count() + 1];
            self.total_text_width = 0.0;
            return;
        }

        let buffer = shared(fs, &render_text, self.font_size, font_fam, attrs);
        let run = crate::backend::text::shaped_run(&buffer, &render_text, scale);
        let total_w = run.width;
        // A right-to-left line that fits is set against the right. One that does not
        // overflows to the LEFT, its start being at the right: shown and not being edited,
        // it is scrolled to its right end, where a left-to-right line shows its left (until
        // 2026-10-08 it showed its left end too, which is the END of a right-to-left line).
        // While editing, the caret is followed (`scroll_to_cursor`) as for any line.
        let room = f32::from_bits(key.room_bits);
        let rtl = crate::backend::text::paragraph_rtl(&render_text);
        let shift = if rtl && total_w < room { room - total_w } else { 0.0 };
        self.glyph_positions = run.stops.iter().map(|s| s.1 + shift).collect();
        self.glyph_shift = shift;
        self.glyph_run = Some(run);
        self.total_text_width = total_w;
        if rtl && total_w > room && !self.editing && key.wrap.is_none() && !self.multiline {
            self.scroll_x = total_w - room;
        }
    }

    pub fn char_width(&self) -> f32 {
        if self.shaped_char_advance > 0.0 {
            self.shaped_char_advance
        } else {
            crate::widget::display::measure_text_width("M", &self.font_family, self.font_size)
        }
    }

    pub fn line_height(&self) -> f32 {
        self.font_size * 1.333
    }

    /// The width a multiline box wraps its lines at, for a box `outer_w` wide: its width
    /// less the padding, or no limit with wrapping off.
    pub fn wrap_width(&self, outer_w: f32) -> f32 {
        if self.line_wrap_enabled() {
            (outer_w - 2.0 * self.pad()).max(1.0)
        } else {
            f32::INFINITY
        }
    }

    fn advance_key(&self, text: &str) -> AdvanceKey {
        AdvanceKey {
            text: text.to_string(),
            font_size_bits: self.font_size.to_bits(),
            font: self.value_font(),
            attrs: self.font_attrs,
            scale_bits: crate::scale::scale_factor().to_bits(),
        }
    }

    /// Each char's shaped advance in `text` (see the `advances` field), shaped with `fs`.
    fn measure_advances(&self, fs: &mut cosmic_text::FontSystem, text: &str) -> Vec<f32> {
        let scale = crate::scale::scale_factor().max(0.01);
        let font = self.value_font();
        let mut out = Vec::with_capacity(text.chars().count());
        for (pi, para) in text.split('\n').enumerate() {
            if pi > 0 {
                out.push(0.0); // the newline
            }
            let shown = if self.is_password { "•".repeat(para.chars().count()) } else { para.to_string() };
            let buf = crate::backend::text::shared_text_buffer(fs, &shown, self.font_size, font.as_deref(), self.font_attrs);
            let run = crate::backend::text::shaped_run(&buf, &shown, scale);
            let mut adv = vec![0.0f32; shown.chars().count()];
            let char_at: std::collections::HashMap<usize, usize> =
                shown.char_indices().enumerate().map(|(ci, (b, _))| (b, ci)).collect();
            for c in &run.clusters {
                if let Some(&ci) = char_at.get(&c.start) {
                    adv[ci] += c.x1 - c.x0;
                }
            }
            out.extend(adv);
        }
        out
    }

    /// Measure `text`'s advances with `fs` into the cache, unless it already holds them.
    fn cache_advances(&self, fs: &mut cosmic_text::FontSystem, text: &str) {
        let key = self.advance_key(text);
        if self.advances.borrow().as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let adv = self.measure_advances(fs, text);
        *self.advances.borrow_mut() = Some((key, adv));
    }

    /// `text`'s char advances: the cache, else measured with the shared geometry font
    /// system, else — that one held, by this thread further up the stack or by another —
    /// with a font system of this thread's own, from the same font set. Always measured,
    /// never a grid: two wraps of one text must agree.
    fn char_advances(&self, text: &str) -> Vec<f32> {
        thread_local! {
            static OWN_FS: std::cell::RefCell<Option<cosmic_text::FontSystem>> = const { std::cell::RefCell::new(None) };
        }
        let key = self.advance_key(text);
        if let Some((k, adv)) = self.advances.borrow().as_ref() {
            if *k == key {
                return adv.clone();
            }
        }
        let adv = match crate::geometry_font_system().try_lock() {
            Ok(mut fs) => self.measure_advances(&mut fs, text),
            Err(_) => OWN_FS.with(|own| {
                let mut own = own.borrow_mut();
                let fs = own.get_or_insert_with(crate::create_font_system);
                self.measure_advances(fs, text)
            }),
        };
        *self.advances.borrow_mut() = Some((key, adv.clone()));
        adv
    }

    /// The box's text wrapped at `max_width` px ([`TextBox::wrap_width`]): its lines, and
    /// where each char lands as (line, column). Words move whole to the next line; a word
    /// wider than the line breaks where it overflows. Widths are the shaped advances, so a
    /// proportional face and two-column CJK wrap where they are drawn (until 2026-10-08 it
    /// counted chars against one monospace advance).
    pub fn wrap_text(&self, max_width: f32) -> (Vec<String>, Vec<(usize, usize)>) {
        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        self.wrap_str(text_src, max_width)
    }

    /// [`TextBox::wrap_text`] for any text in the box's face (its placeholder).
    pub fn wrap_str(&self, text_src: &str, max_width: f32) -> (Vec<String>, Vec<(usize, usize)>) {
        let chars: Vec<char> = text_src.chars().collect();
        let mut lines = Vec::new();
        let mut current_line = Vec::new();
        let mut index_map = vec![(0, 0); chars.len() + 1];

        if !self.line_wrap_enabled() || !max_width.is_finite() {
            let mut i = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\n' {
                    index_map[i] = (lines.len(), current_line.len());
                    lines.push(current_line.iter().collect::<String>());
                    current_line.clear();
                } else {
                    current_line.push(ch);
                    index_map[i] = (lines.len(), current_line.len() - 1);
                }
                i += 1;
            }
            index_map[chars.len()] = (lines.len(), current_line.len());
            lines.push(current_line.iter().collect::<String>());
            return (lines, index_map);
        }

        let adv = self.char_advances(text_src);
        let max_width = max_width.max(1.0);
        // The current line's width: the advances of the chars it holds.
        let mut line_w = 0.0f32;

        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];

            if ch == '\n' {
                index_map[i] = (lines.len(), current_line.len());
                lines.push(current_line.iter().collect::<String>());
                current_line.clear();
                line_w = 0.0;
                i += 1;
                continue;
            }

            current_line.push(ch);
            line_w += adv.get(i).copied().unwrap_or(0.0);
            index_map[i] = (lines.len(), current_line.len() - 1);

            // A space may hang past the edge, as it is drawn there; anything else that
            // overflows breaks the line after its last space, else before itself.
            if line_w > max_width + 0.5 && current_line.len() > 1 && !ch.is_whitespace() {
                let split = match current_line.iter().rposition(|c| c.is_whitespace()) {
                    Some(s_idx) => s_idx + 1,
                    None => current_line.len() - 1,
                };
                let line_to_push: Vec<char> = current_line[..split].to_vec();
                let remaining: Vec<char> = current_line[split..].to_vec();

                let line_idx = lines.len();
                lines.push(line_to_push.iter().collect::<String>());

                current_line = remaining;
                let start_orig = i + 1 - current_line.len();
                line_w = 0.0;
                for c_idx in 0..current_line.len() {
                    index_map[start_orig + c_idx] = (line_idx + 1, c_idx);
                    line_w += adv.get(start_orig + c_idx).copied().unwrap_or(0.0);
                }
            }
            i += 1;
        }

        index_map[chars.len()] = (lines.len(), current_line.len());
        lines.push(current_line.iter().collect::<String>());

        (lines, index_map)
    }

    pub fn map_2d_to_1d(&self, index_map: &[(usize, usize)], target_line: usize, target_col: usize, max_line_idx: usize) -> usize {
        let line = target_line.min(max_line_idx);
        let mut best_idx = 0;
        let mut best_dist = usize::MAX;

        for (i, &(l, c)) in index_map.iter().enumerate() {
            if l == line {
                let dist = (c as isize - target_col as isize).unsigned_abs();
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = i;
                }
            }
        }
        best_idx
    }

    fn border_width(&self) -> f32 {
        if self.multiline {
            crate::layout::textbox_multiline_border_width()
        } else {
            1.0
        }
    }

    /// The inset from the box's edge to its text — the caret, selection,
    /// hit-testing, wrap and scroll range all measure from it. 8px, or the
    /// well's floor when the relief wall reaches further in: the wall is
    /// `bevel_width` deep once the box is tall enough (`well`'s 20% cap), so a
    /// fixed 8px sat a multiline box's text on its wall.
    fn pad(&self) -> f32 {
        8.0f32.max(self.wall_inset())
    }

    /// How far in from the box's edge its relief wall ends (the outline's own
    /// inset from the carve plus the wall's depth); zero with no relief.
    fn wall_inset(&self) -> f32 {
        self.well().map_or(0.0, |f| f.rect.x - self.rect.x + f.depth)
    }

    /// The value text's clip, `[x1, y1, x2, y2]`, from the content rect (the
    /// box below its label strip): the well's floor, so scrolled text slides
    /// under the relief wall rather than over it. Only a multiline box is
    /// clipped in y: a single line is centred, never scrolls vertically, and a
    /// short box's floor is shallower than its line.
    fn text_clip(&self, content: Rect) -> [f32; 4] {
        let inset = self.wall_inset();
        let inset_y = if self.multiline { inset } else { 0.0 };
        [
            content.x + inset,
            content.y + inset_y,
            content.x + content.width - inset,
            content.y + content.height - inset_y,
        ]
    }

    pub fn set_placeholder(&mut self, placeholder: &str) {
        self.placeholder = Some(placeholder.to_string());
    }

    pub fn take_change(&mut self) -> bool {
        if self.update_on_type {
            let held = self.committed_buffer();
            if self.text != held {
                self.text = held;
                self.just_changed = true;
            }
        }
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn line_wrap_enabled(&self) -> bool {
        self.multiline && self.line_wrap_override.unwrap_or_else(crate::layout::textbox_line_wrap)
    }

    pub fn set_max_width(&mut self, max_w: Option<f32>) {
        self.max_width = max_w;
    }

    pub fn set_width(&mut self, w: f32) {
        self.width = Some(w);
    }

    pub fn sync_editor_state(&mut self) {
        self.editor_state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
    }

    /// The editing fields as one value — what the history stores.
    fn snapshot(&self) -> TextEditorState {
        TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        }
    }

    fn restore(&mut self, snap: TextEditorState) {
        self.edit_buffer = snap.buffer;
        self.cursor_idx = snap.cursor_idx;
        self.select_anchor = snap.select_anchor;
        self.all_selected = snap.all_selected;
        self.sync_editor_state();
        self.scroll_to_cursor();
    }

    /// Step the edit buffer back one recorded step. Only while editing —
    /// a committed value is the app's to undo, not the box's.
    pub fn undo_edit(&mut self) -> bool {
        if !self.editing || self.disabled {
            return false;
        }
        let current = self.snapshot();
        match self.history.undo(current) {
            Some(prev) => {
                self.restore(prev);
                self.just_changed = true;
                true
            }
            None => false,
        }
    }

    /// Step forward again — see [`undo_edit`](Self::undo_edit).
    pub fn redo_edit(&mut self) -> bool {
        if !self.editing || self.disabled {
            return false;
        }
        let current = self.snapshot();
        match self.history.redo(current) {
            Some(next) => {
                self.restore(next);
                self.just_changed = true;
                true
            }
            None => false,
        }
    }

    /// The selection as it may go to the clipboard: never a password box's.
    /// Every copy and cut goes through here -- Ctrl+C / Ctrl+X, the context
    /// menu's rows, [`copy_selection`](Self::copy_selection) and
    /// [`cut_selection`](Self::cut_selection) -- because each of them used to
    /// put a password on the clipboard in plain text, readable by any client,
    /// from the login greeter's and the polkit dialog's boxes alike.
    fn clipboard_text(&self, state: &TextEditorState) -> Option<String> {
        if self.is_password {
            return None;
        }
        state.selected_text()
    }

    pub fn copy_selection(&self) {
        let state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        if let Some(text) = self.clipboard_text(&state) {
            clipboard::copy_to_clipboard(&text);
        }
    }

    /// Cut the selection to the clipboard. A password box's selection can
    /// not go there (`clipboard_text`), so it is left in place, not deleted.
    pub fn cut_selection(&mut self) -> bool {
        if self.is_password {
            return false;
        }
        let before = self.snapshot();
        let mut state = TextEditorState {
            buffer: std::mem::take(&mut self.edit_buffer),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        if let Some(text) = state.selected_text() {
            clipboard::copy_to_clipboard(&text);
            state.insert_text("");
            self.history.record(before);
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            true
        } else {
            self.edit_buffer = state.buffer;
            self.sync_editor_state();
            false
        }
    }

    pub fn paste_from_clipboard(&mut self) -> bool {
        if let Some(text) = clipboard::read_from_clipboard() {
            let before = self.snapshot();
            let mut state = TextEditorState {
                buffer: std::mem::take(&mut self.edit_buffer),
                cursor_idx: self.cursor_idx,
                select_anchor: self.select_anchor,
                all_selected: self.all_selected,
            };
            let mut cleaned = String::new();
            for ch in text.chars() {
                if !ch.is_control() && ch != '\n' && ch != '\r' {
                    cleaned.push(ch);
                }
            }
            state.insert_text(&cleaned);
            self.history.record(before);
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            true
        } else {
            false
        }
    }

    pub fn select_all(&mut self) {
        let mut state = TextEditorState {
            buffer: std::mem::take(&mut self.edit_buffer),
            cursor_idx: self.cursor_idx,
            select_anchor: self.select_anchor,
            all_selected: self.all_selected,
        };
        state.select_all();
        self.edit_buffer = state.buffer;
        self.cursor_idx = state.cursor_idx;
        self.select_anchor = state.select_anchor;
        self.all_selected = state.all_selected;
        self.sync_editor_state();
    }

    pub fn set_value(&mut self, val: &str) -> bool {
        let val_str = val.to_string();
        if self.text != val_str {
            if self.editing {
                let before = self.snapshot();
                self.history.record(before);
            } else {
                self.history.clear();
            }
            self.text = val_str.clone();
            self.edit_buffer = val_str;
            self.just_changed = true;
            let len = self.edit_buffer.chars().count();
            self.cursor_idx = self.cursor_idx.min(len);
            if let Some(anchor) = self.select_anchor {
                self.select_anchor = Some(anchor.min(len));
            }
            if self.cursor_idx == 0 && self.select_anchor == Some(0) {
                self.all_selected = false;
            }
            self.sync_editor_state();
            self.clamp_scroll();
            true
        } else {
            false
        }
    }

    /// The widest line's drawn advance — shaped when recorded, else the
    /// chars × char_width estimate (the pre-shaping formula).
    fn content_width(&self, lines: &[String]) -> f32 {
        let shaped = if self.multiline {
            self.line_glyph_positions
                .iter()
                // A line's width is its widest stop: the last in left-to-right text, the
                // first in right-to-left.
                .map(|l| l.iter().copied().fold(0.0f32, f32::max))
                .fold(0.0f32, f32::max)
        } else {
            self.total_text_width
        };
        if shaped > 0.0 {
            shaped
        } else {
            let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            max_line_len as f32 * self.char_width()
        }
    }

    pub fn clamp_scroll(&mut self) {
        let pad = self.pad();
        let line_height = self.line_height();
        let max_w = self.wrap_width(self.rect.width);
        let (lines, _) = if self.multiline {
            self.wrap_text(max_w)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            (vec![buffer.clone()], vec![(0, 0); buffer.chars().count() + 1])
        };

        if self.multiline {
            let content_h = lines.len() as f32 * line_height;
            let max_scroll = (content_h - (self.rect.height - 2.0 * pad)).max(0.0);
            self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        } else {
            self.scroll_y = 0.0;
        }

        if !self.line_wrap_enabled() {
            let content_w = self.content_width(&lines);
            let max_scroll_x = (content_w - (self.rect.width - 2.0 * pad)).max(0.0);
            self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);
        } else {
            self.scroll_x = 0.0;
        }
    }

    pub fn scroll_to_cursor(&mut self) {
        let pad = self.pad();
        let char_width = self.char_width();
        let line_height = self.line_height();
        let max_w = self.wrap_width(self.rect.width);
        let (_lines, index_map) = if self.multiline {
            self.wrap_text(max_w)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            let mut m = Vec::new();
            for i in 0..=buffer.chars().count() {
                m.push((0, i));
            }
            (vec![buffer.clone()], m)
        };
        if index_map.is_empty() { return; }

        let cursor_idx = self.cursor_idx.min(index_map.len() - 1);
        let (line_idx, col_idx) = index_map[cursor_idx];

        let top = self.label_top();
        let viewport_w = self.rect.width - 2.0 * pad;
        let viewport_h = self.rect.height - top - 2.0 * pad;

        if self.multiline {
            let line_y = top + pad + (line_idx as f32 * line_height);
            if line_y < self.scroll_y + 10.0 {
                self.scroll_y = (line_y - 20.0).max(0.0);
            } else if line_y + line_height > self.scroll_y + viewport_h - 10.0 {
                self.scroll_y = (line_y + line_height - viewport_h + 20.0).max(0.0);
            }
        }

        if !self.line_wrap_enabled() {
            let cursor_x = if self.multiline {
                self.line_col_x(line_idx, col_idx)
            } else {
                self.glyph_positions
                    .get(col_idx)
                    .copied()
                    .unwrap_or(col_idx as f32 * char_width)
            };
            if cursor_x < self.scroll_x + 10.0 {
                self.scroll_x = (cursor_x - 20.0).max(0.0);
            } else if cursor_x + char_width > self.scroll_x + viewport_w - 10.0 {
                self.scroll_x = (cursor_x + char_width - viewport_w + 20.0).max(0.0);
            }
        }
        self.clamp_scroll();
    }

    /// `edit_buffer` without an input method's provisional run: what the box
    /// holds, as opposed to what it shows.
    pub fn committed_buffer(&self) -> String {
        match self.composing {
            Some((start, len)) => self.edit_buffer.chars().enumerate().filter(|(i, _)| *i < start || *i >= start + len).map(|(_, c)| c).collect(),
            None => self.edit_buffer.clone(),
        }
    }

    /// Take the provisional run out of the buffer, the caret back where it
    /// began. Whether there was one.
    fn strip_composition(&mut self) -> bool {
        let Some((start, _)) = self.composing else { return false };
        self.edit_buffer = self.committed_buffer();
        self.composing = None;
        self.cursor_idx = start.min(self.edit_buffer.chars().count());
        self.select_anchor = None;
        self.all_selected = false;
        true
    }

    /// Drop a composition this box is showing, and have the input method
    /// cancel it: the box is no longer where it is going.
    fn abandon_composition(&mut self) {
        if self.strip_composition() {
            crate::ime::request_reset();
            self.sync_editor_state();
        }
        self.ime_seen = crate::ime::generation();
    }

    /// Show the input method's current composition, if it has moved since
    /// this box last showed one: the old run out, the new one in at the
    /// caret, the caret where the input method has its cursor. A composition
    /// begun over a selection replaces it, as typing would.
    fn sync_preedit(&mut self) {
        if !self.editing {
            return;
        }
        let generation = crate::ime::generation();
        if generation == self.ime_seen {
            return;
        }
        self.ime_seen = generation;
        self.strip_composition();
        if let Some(p) = crate::ime::preedit() {
            let before = self.snapshot();
            if before.all_selected || before.selected_range().is_some() {
                let mut state = before.clone();
                state.insert_text("");
                self.history.record(before);
                self.edit_buffer = state.buffer;
                self.cursor_idx = state.cursor_idx;
            }
            let start = self.cursor_idx.min(self.edit_buffer.chars().count());
            let at = self.edit_buffer.char_indices().nth(start).map_or(self.edit_buffer.len(), |(b, _)| b);
            self.edit_buffer.insert_str(at, &p.text);
            self.composing = Some((start, p.text.chars().count()));
            self.cursor_idx = start + p.caret_chars();
            self.select_anchor = None;
            self.all_selected = false;
        }
        self.sync_editor_state();
        self.scroll_to_cursor();
    }

    /// The legacy `focus()` body minus the global-focus claim (the caller's, via
    /// `EventCtx::request_focus`).
    fn begin_editing(&mut self) {
        if self.disabled { return; }
        // A composition still going is the input method's for wherever the
        // caret was; this box starts with none.
        self.composing = None;
        self.ime_seen = crate::ime::generation();
        self.editing = true;
        self.edit_buffer = self.text.clone();
        let len = self.edit_buffer.chars().count();
        self.cursor_idx = len;
        self.select_anchor = Some(0);
        self.all_selected = len > 0;
        self.just_focused = true;
        self.history.clear();
        self.sync_editor_state();
    }

    /// The legacy `unfocus()` body: leave edit mode and commit the buffer.
    fn commit_editing(&mut self) {
        if self.editing {
            self.abandon_composition();
            self.editing = false;
            if self.text != self.edit_buffer {
                self.text = self.edit_buffer.clone();
                self.just_changed = true;
            }
            self.select_anchor = None;
            self.all_selected = false;
            self.sync_editor_state();
        }
    }

    /// Map a press/drag position to a buffer index — the shared body of the legacy
    /// `mouse_input` press arm and `drag_update`.
    fn position_to_idx(&self, px: f32, py: f32) -> usize {
        let pad = self.pad();
        let top = self.label_top();
        if self.multiline {
            let line_height = self.line_height();
            let max_w = self.wrap_width(self.rect.width);
            let (lines, index_map) = self.wrap_text(max_w);
            let click_line = (((py - (self.rect.y + top + pad) + self.scroll_y) / line_height).floor() as isize).max(0) as usize;
            let rel_x = px - (self.rect.x + pad) + self.scroll_x;
            let click_col = self.line_x_to_col(click_line.min(lines.len() - 1), rel_x);
            self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
        } else {
            self.map_x_to_idx(px)
        }
    }

    /// Extend the selection to a drag position — the shared body of the legacy
    /// `on_cursor_moved` drag arm and `drag_update` (which used no label inset).
    fn extend_selection_to(&mut self, px: f32, py: f32) -> bool {
        let drag_idx = self.position_to_idx(px, py);
        if self.cursor_idx != drag_idx {
            self.cursor_idx = drag_idx;
            self.just_focused = false;
            let len = self.edit_buffer.chars().count();
            let start = self.select_anchor.unwrap_or(0).min(self.cursor_idx);
            let end = self.select_anchor.unwrap_or(0).max(self.cursor_idx);
            self.all_selected = start == 0 && end == len && len > 0;
            true
        } else {
            false
        }
    }

    /// Port of the legacy `keyboard_input` body.
    fn handle_key(&mut self, event: &KeyEvent) -> bool {
        if !self.editing || self.disabled { return false; }
        if event.state != ElementState::Pressed { return false; }
        // While an input method composes, its keys are its own: a shell does
        // not deliver them, and one that does is not editing this text.
        self.sync_preedit();
        if self.composing.is_some() {
            return true;
        }

        let control = event.ctrl;

        // The undo/redo chords, for apps that hand keys to widgets without
        // exposing a `UiContext` (the runner's routing reaches the box
        // through `ContextAction` first when they do). Before the working
        // copy below, since a step replaces the whole editing state.
        if control {
            if match_key_shortcut(event, &crate::input::widget_chord("undo", "", "ctrl+z")) {
                return self.undo_edit();
            }
            if match_key_shortcut(event, &crate::input::widget_chord("redo", "", "ctrl+shift+z")) {
                return self.redo_edit();
            }
        }

        let before = self.snapshot();
        let mut state = before.clone();

        let handled = match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                state.delete_backwards()
            }
            Key::Named(NamedKey::Delete) => {
                state.delete_forwards()
            }
            Key::Named(NamedKey::ArrowLeft) => {
                state.move_cursor_left(event.shift)
            }
            Key::Named(NamedKey::ArrowRight) => {
                state.move_cursor_right(event.shift)
            }
            Key::Named(NamedKey::ArrowUp) => {
                if state.all_selected {
                    state.clear_selection();
                } else if event.shift {
                    if state.select_anchor.is_none() {
                        state.select_anchor = Some(state.cursor_idx);
                    }
                } else {
                    state.clear_selection();
                }
                if self.multiline {
                    let max_w = self.wrap_width(self.rect.width);
                    let (lines, index_map) = self.wrap_text(max_w);
                    let (cursor_l, cursor_c) = index_map[state.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l > 0 {
                        state.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l - 1, cursor_c, lines.len() - 1);
                    } else {
                        state.cursor_idx = 0;
                    }
                } else {
                    state.cursor_idx = 0;
                }
                true
            }
            Key::Named(NamedKey::ArrowDown) => {
                if state.all_selected {
                    state.clear_selection();
                } else if event.shift {
                    if state.select_anchor.is_none() {
                        state.select_anchor = Some(state.cursor_idx);
                    }
                } else {
                    state.clear_selection();
                }
                if self.multiline {
                    let max_w = self.wrap_width(self.rect.width);
                    let (lines, index_map) = self.wrap_text(max_w);
                    let (cursor_l, cursor_c) = index_map[state.cursor_idx.min(index_map.len() - 1)];
                    if cursor_l < lines.len() - 1 {
                        state.cursor_idx = self.map_2d_to_1d(&index_map, cursor_l + 1, cursor_c, lines.len() - 1);
                    } else {
                        state.cursor_idx = state.buffer.chars().count();
                    }
                } else {
                    state.cursor_idx = state.buffer.chars().count();
                }
                true
            }
            Key::Named(NamedKey::Home) => {
                state.move_cursor_to_start(event.shift)
            }
            Key::Named(NamedKey::End) => {
                state.move_cursor_to_end(event.shift)
            }
            Key::Named(NamedKey::Enter) => {
                if self.multiline {
                    state.insert_text("\n");
                    true
                } else {
                    self.commit_editing();
                    true
                }
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                state.buffer = self.text.clone();
                state.clear_selection();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "a" || ch_str == "A") => {
                state.select_all();
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "c" || ch_str == "C") => {
                if let Some(text) = self.clipboard_text(&state) {
                    clipboard::copy_to_clipboard(&text);
                }
                true
            }
            // A password box's selection stays put: it can not be cut to the
            // clipboard, and deleting it alone would not be a cut.
            Key::Character(ref ch_str) if control && (ch_str == "x" || ch_str == "X") => {
                if let Some(text) = self.clipboard_text(&state) {
                    clipboard::copy_to_clipboard(&text);
                    state.insert_text("");
                }
                true
            }
            Key::Character(ref ch_str) if control && (ch_str == "v" || ch_str == "V") => {
                if let Some(pasted) = clipboard::read_from_clipboard() {
                    let mut cleaned = String::new();
                    for ch in pasted.chars() {
                        if !ch.is_control() && ch != '\n' && ch != '\r' {
                            cleaned.push(ch);
                        }
                    }
                    state.insert_text(&cleaned);
                } else {
                    state.clear_selection();
                }
                true
            }
            _ => {
                if let Some(text) = &event.text {
                    if !control {
                        state.insert_text(text);
                        true
                    } else {
                        false
                    }
                } else {
                    false
                }
            }
        };

        if self.editing {
            if state.buffer != before.buffer {
                // One step per typed run, per deleted run; whitespace
                // starts a new run so undo walks back a word at a time.
                // Replacing a selection is always its own step.
                let group = match &event.logical_key {
                    _ if before.selected_range().is_some() => None,
                    Key::Named(NamedKey::Backspace) => Some(2),
                    Key::Named(NamedKey::Delete) => Some(3),
                    Key::Character(_) if !control => {
                        let ws = event.text.as_deref().is_some_and(|t| t.chars().all(char::is_whitespace));
                        Some(if ws { 4 } else { 1 })
                    }
                    _ => None,
                };
                match group {
                    Some(g) => self.history.record_grouped(before, g),
                    None => self.history.record(before),
                }
            } else if handled {
                // A cursor or selection move between keystrokes splits the
                // run: "abc", move, "def" undoes as two steps.
                self.history.break_group();
            }
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
            self.select_anchor = state.select_anchor;
            self.all_selected = state.all_selected;
            self.sync_editor_state();
            self.scroll_to_cursor();
        }

        handled
    }

    /// Port of the legacy `mouse_wheel` body (scroll the multiline/no-wrap viewports).
    fn handle_wheel(&mut self, delta: &MouseScrollDelta) -> bool {
        let pad = self.pad();
        if self.disabled { return false; }
        let char_width = self.char_width();
        let line_height = self.line_height();

        let max_w = self.wrap_width(self.rect.width);

        let (lines, _) = if self.multiline {
            self.wrap_text(max_w)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            (vec![buffer.clone()], vec![(0, 0); buffer.chars().count() + 1])
        };

        let mut dy_px = 0.0;
        let mut max_scroll_y = 0.0;
        if self.multiline {
            let content_h = lines.len() as f32 * line_height;
            max_scroll_y = (content_h - (self.rect.height - 2.0 * pad)).max(0.0);
            dy_px = match *delta {
                MouseScrollDelta::LineDelta(_, dy) => -dy * line_height * 2.0,
                MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
            };
        }

        let mut dx_px = 0.0;
        let mut max_scroll_x = 0.0;
        if !self.line_wrap_enabled() {
            let content_w = self.content_width(&lines);
            max_scroll_x = (content_w - (self.rect.width - 2.0 * pad)).max(0.0);
            let natural = crate::layout::touchpad_natural_scroll();
            let scroll_amt_x = match *delta {
                MouseScrollDelta::LineDelta(dx, dy) => {
                    if !self.multiline {
                        let scroll_val = if dy != 0.0 { -dy } else { if natural { -dx } else { dx } };
                        scroll_val * char_width * 3.0
                    } else {
                        let scroll_val = if natural { -dx } else { dx };
                        scroll_val * char_width * 3.0
                    }
                }
                MouseScrollDelta::PixelDelta(pos) => {
                    if !self.multiline {
                        
                        if pos.y != 0.0 { -pos.y as f32 } else { if natural { -pos.x as f32 } else { pos.x as f32 } }
                    } else {
                        if natural { -pos.x as f32 } else { pos.x as f32 }
                    }
                }
            };
            dx_px = scroll_amt_x;
        }

        // Both axes through the shared motion: notches glide, finger tracks
        // 1:1, a flick coasts. The pub offsets are the drawn values.
        self.scroll_max = (max_scroll_x, max_scroll_y);
        self.scroll_motion.reconcile(self.scroll_x, self.scroll_y);
        let discrete = matches!(delta, MouseScrollDelta::LineDelta(..));
        let changed = self.scroll_motion.apply_px(
            dx_px,
            dy_px,
            discrete,
            Bounds::max(max_scroll_x),
            Bounds::max(max_scroll_y),
        );
        self.scroll_x = self.scroll_motion.x.pos();
        self.scroll_y = self.scroll_motion.y.pos();
        changed
    }

    /// Selection highlight + caret quads, shared by both render branches. `x`/`w` are the
    /// (possibly label-inset) horizontal span the branch draws in — the legacy paths differed
    /// (non-rounded and rounded alike use the full base span).
    /// The selection highlight, an input method's composition underlined, and
    /// the caret, as quads. Returns the caret's rect while editing (unclipped,
    /// in the box's coordinates), which the painter reports to the input method.
    fn selection_quads(&self, x: f32, w: f32, out: &mut Vec<(f32, f32, f32, f32, [f32; 4])>) -> Option<[f32; 4]> {
        let pad = self.pad();
        if !(self.editing || self.select_anchor.is_some()) {
            return None;
        }
        let mut caret = None;
        let top = self.label_top();
        let char_width = self.char_width();
        let line_height = self.line_height();

        let highlight_color = [0.20, 0.50, 0.85, 0.3];
        let cursor_color = if self.draw_bg_border {
            [0.80, 0.80, 0.85, 1.0]
        } else {
            [0.10, 0.10, 0.15, 1.0]
        };

        let start = self.select_anchor.unwrap_or(self.cursor_idx).min(self.cursor_idx);
        let end = self.select_anchor.unwrap_or(self.cursor_idx).max(self.cursor_idx);

        if self.multiline {
            let max_w = self.wrap_width(w);
            let (_lines, index_map) = self.wrap_text(max_w);

            let view_top = self.rect.y + top;
            let view_bottom = self.rect.y + self.rect.height;

            if start != end {
                let start_pos = index_map[start.min(index_map.len() - 1)];
                let end_pos = index_map[end.min(index_map.len() - 1)];

                for line_idx in start_pos.0..=end_pos.0 {
                    let mut line_start_col = None;
                    let mut line_end_col = None;
                    for idx in start..end {
                        if idx < index_map.len() {
                            let (l, c) = index_map[idx];
                            if l == line_idx {
                                if line_start_col.is_none() || c < line_start_col.unwrap() {
                                    line_start_col = Some(c);
                                }
                                if line_end_col.is_none() || c > line_end_col.unwrap() {
                                    line_end_col = Some(c);
                                }
                            }
                        }
                    }
                    if let (Some(sc), Some(ec)) = (line_start_col, line_end_col) {
                        let highlight_y = self.rect.y + top + pad + (line_idx as f32 * line_height) - self.scroll_y;
                        let clipped_y = highlight_y.max(view_top);
                        let clipped_bottom = (highlight_y + line_height).min(view_bottom);
                        let shift = self.line_shift.get(line_idx).copied().unwrap_or(0.0);
                        // The boxes of the selected clusters: two pieces where the selection
                        // crosses a change of direction. Without a shaped run, the column span.
                        let spans = match (self.line_runs.get(line_idx), _lines.get(line_idx)) {
                            (Some(run), Some(line)) => {
                                let byte = |col: usize| line.char_indices().nth(col).map_or(line.len(), |(b, _)| b);
                                run.spans(byte(sc), byte(ec + 1)).into_iter().map(|(a, b)| (a + shift, b + shift)).collect()
                            }
                            _ => {
                                let (a, b) = (self.line_col_x(line_idx, sc), self.line_col_x(line_idx, ec + 1));
                                vec![(a.min(b), a.max(b))]
                            }
                        };
                        for (a, b) in spans {
                            let h_left = (x + pad + a - self.scroll_x).max(x + pad);
                            let h_right = (x + pad + b - self.scroll_x).min(x + w - pad);
                            if h_left < h_right && clipped_y < clipped_bottom {
                                out.push((h_left, clipped_y, h_right - h_left, clipped_bottom - clipped_y, highlight_color));
                            }
                        }
                    }
                }
            }

            if let Some((cs, cl)) = self.composing {
                // The composition's underline, a line at a time, under the
                // glyphs it covers.
                let last_line = index_map.get((cs + cl).saturating_sub(1).min(index_map.len() - 1)).map_or(0, |p| p.0);
                let first_line = index_map.get(cs.min(index_map.len() - 1)).map_or(0, |p| p.0);
                for line_idx in first_line..=last_line {
                    let cols: Vec<usize> = (cs..cs + cl)
                        .filter_map(|i| index_map.get(i).filter(|p| p.0 == line_idx).map(|p| p.1))
                        .collect();
                    if let (Some(&a), Some(&b)) = (cols.iter().min(), cols.iter().max()) {
                        let (xa, xb) = (self.line_col_x(line_idx, a), self.line_col_x(line_idx, b + 1));
                        let ux = x + pad + xa.min(xb) - self.scroll_x;
                        let uw = (xb - xa).abs();
                        let uy = self.rect.y + top + pad + ((line_idx + 1) as f32 * line_height) - 2.0 - self.scroll_y;
                        let left = ux.max(x + pad);
                        let right = (ux + uw).min(x + w - pad);
                        if left < right && uy >= view_top && uy + 1.5 <= view_bottom {
                            out.push((left, uy, right - left, 1.5, cursor_color));
                        }
                    }
                }
            }

            if self.editing {
                let caret_h = self.font_size * 1.15;
                let (cursor_l, cursor_c) = index_map[self.cursor_idx.min(index_map.len() - 1)];
                let cursor_x = x + pad + self.line_col_x(cursor_l, cursor_c) - self.scroll_x;
                let cursor_y = self.rect.y + top + pad + (cursor_l as f32 * line_height) + (line_height - caret_h) / 2.0 - self.scroll_y;
                caret = Some([cursor_x, cursor_y, 1.5, caret_h]);
                let clipped_y = cursor_y.max(view_top);
                let clipped_bottom = (cursor_y + caret_h).min(view_bottom);
                if cursor_x >= x + pad && cursor_x <= x + w - pad
                    && clipped_y < clipped_bottom {
                        out.push((cursor_x, clipped_y, 1.5, clipped_bottom - clipped_y, cursor_color));
                    }
            }
        } else {
            let caret_h = self.font_size * 1.15;
            if start != end {
                // The boxes of the selected clusters: two pieces where the selection crosses
                // a change of direction. Without a shaped run, the column span.
                let shown = if self.is_password { "•".repeat(self.edit_buffer.chars().count()) } else { self.edit_buffer.clone() };
                let spans: Vec<(f32, f32)> = match &self.glyph_run {
                    Some(run) => {
                        let byte = |col: usize| shown.char_indices().nth(col).map_or(shown.len(), |(b, _)| b);
                        run.spans(byte(start), byte(end)).into_iter().map(|(a, b)| (a + self.glyph_shift, b + self.glyph_shift)).collect()
                    }
                    None => {
                        let at = |i: usize| self.glyph_positions.get(i).copied().unwrap_or(i as f32 * char_width);
                        vec![(at(start).min(at(end)), at(start).max(at(end)))]
                    }
                };
                for (a, b) in spans {
                    let h_left = (x + pad + a - self.scroll_x).max(x + pad);
                    let h_right = (x + pad + b - self.scroll_x).min(x + w - pad);
                    if h_left < h_right {
                        out.push((
                            h_left,
                            crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top),
                            h_right - h_left,
                            crate::layout::line_height(self.font_size),
                            highlight_color,
                        ));
                    }
                }
            }

            if let Some((cs, cl)) = self.composing {
                let at = |i: usize| self.glyph_positions.get(i).copied().unwrap_or(i as f32 * char_width);
                let (ca, cb) = (at(cs).min(at(cs + cl)), at(cs).max(at(cs + cl)));
                let left = (x + pad + ca - self.scroll_x).max(x + pad);
                let right = (x + pad + cb - self.scroll_x).min(x + w - pad);
                if left < right {
                    let text_y = crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top);
                    out.push((left, text_y + self.font_size + 1.0, right - left, 1.5, cursor_color));
                }
            }

            if self.editing {
                let offset = if self.glyph_positions.is_empty() {
                    self.cursor_idx as f32 * char_width
                } else {
                    self.cursor_x_offset
                };
                let cursor_x = x + pad + offset - self.scroll_x;
                let text_y = crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top);
                let cursor_y = text_y + (self.font_size - caret_h) / 2.0;
                caret = Some([cursor_x, cursor_y, 1.5, caret_h]);
                if cursor_x >= x + pad && cursor_x <= x + w - pad {
                    out.push((cursor_x, cursor_y, 1.5, caret_h, cursor_color));
                }
            }
        }
        caret
    }

    /// The value/placeholder text lines — the legacy `text_labels` body minus the control
    /// label (the adapter's base-label machinery draws that).
    fn value_labels(&self) -> Vec<TextLabel> {
        let pad = self.pad();
        let mut labels = Vec::new();
        let top = self.label_top();
        let mut val_text = if self.editing {
            self.edit_buffer.clone()
        } else {
            self.text.clone()
        };
        if self.is_password {
            val_text = "•".repeat(val_text.chars().count());
        }

        let is_placeholder = val_text.is_empty() && self.placeholder.is_some();
        let display_text = if is_placeholder {
            self.placeholder.as_ref().unwrap().clone()
        } else {
            val_text
        };

        let label_color = if is_placeholder {
            crate::colors::textbox_placeholder_text_color()
        } else if let Some(custom_color) = self.text_color {
            custom_color
        } else if self.disabled {
            [0x53, 0x53, 0x5a]
        } else if self.all_selected {
            [0xff, 0xff, 0xff]
        } else if self.editing {
            [0xee, 0xee, 0xf5]
        } else {
            [0xcc, 0xcc, 0xd4]
        };

        let x = self.rect.x;
        let w = self.rect.width;

        if self.multiline {
            let line_height = self.line_height();
            let max_w = self.wrap_width(w);
            let (lines, _) = self.wrap_text(max_w);
            let lines_to_draw = if is_placeholder {
                self.wrap_str(self.placeholder.as_deref().unwrap_or(""), max_w).0
            } else {
                lines
            };
            for (line_idx, line_text) in lines_to_draw.iter().enumerate() {
                let shift = if is_placeholder { 0.0 } else { self.line_shift.get(line_idx).copied().unwrap_or(0.0) };
                labels.push(TextLabel {
                    text: line_text.clone(),
                    x: x + pad + shift - self.scroll_x,
                    y: self.rect.y + top + pad + (line_idx as f32 * line_height) + (line_height - self.font_size) / 2.0 - self.scroll_y,
                    font_size: self.font_size,
                    color: label_color,
                });
            }
        } else {
            labels.push(TextLabel {
                text: display_text,
                x: x + pad + self.glyph_shift - self.scroll_x,
                y: crate::layout::align_text_y(self.rect.y, self.rect.height, self.font_size, top),
                font_size: self.font_size,
                color: label_color,
            });
        }
        labels
    }

    /// The well this TextBox carves: a [`crate::scene::paint::Field`] that is
    /// all well, lit while editing — its radii top-left, top-right,
    /// bottom-right, bottom-left, the right two square when the box is
    /// [`Self::joined_right`] (its host draws the field that joins it to a
    /// run, `ParametersBg::fields`). `None` when it draws no relief at all
    /// (square-cornered legacy geometry, `control_relief` off, or a box that
    /// draws no background).
    ///
    /// The SINGLE source for that geometry: `paint` carves it here, and the
    /// flat-path bridge in `layout::render_widget` re-offers the same rect
    /// through [`crate::layout::RenderTarget::recess`] for hosts that consume
    /// `all_quads` and so never see the carve. A second copy of this math in
    /// the bridge is exactly how the two would drift apart.
    pub fn well(&self) -> Option<crate::scene::paint::Field> {
        let radius = crate::layout::textbox_corner_radius();
        if radius <= 0.0 || !self.recessed() || !self.draw_bg_border {
            return None;
        }
        let top = self.label_top();
        let well = Rect {
            x: self.rect.x,
            y: self.rect.y + top,
            width: self.rect.width,
            height: self.rect.height - top,
        };
        let depth = crate::layout::bevel_width().min(well.height * 0.2);
        let right = if self.joined_right { 0.0 } else { radius };
        let (well, radii) = crate::layout::carve_inside(well, (radius, right, right, radius), depth);
        let tint = self.editing.then(|| {
            let hc = crate::color::highlight_primary_color();
            [hc[0], hc[1], hc[2]]
        });
        Some(crate::scene::paint::Field::well(well, radii, depth).with_tint(tint))
    }
}

impl Adapted<TextBox> {
    /// Recessed style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
        self
    }

    pub fn with_update_on_type(mut self, update: bool) -> Self {
        self.update_on_type = update;
        self
    }

    pub fn with_multiline(mut self, multiline: bool) -> Self {
        self.multiline = multiline;
        self
    }

    pub fn with_line_wrap(mut self, wrap: bool) -> Self {
        self.line_wrap_override = Some(wrap);
        self
    }

    pub fn with_draw_bg_border(mut self, draw: bool) -> Self {
        self.draw_bg_border = draw;
        self
    }

    pub fn with_text_color(mut self, color: Option<[u8; 3]>) -> Self {
        self.text_color = color;
        self
    }

    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_font_family(mut self, family: String) -> Self {
        self.font_family = family;
        self
    }

    pub fn with_password(mut self, is_password: bool) -> Self {
        self.is_password = is_password;
        self
    }

    /// A search box: the toolkit's "Search..." placeholder, in the user's language, and a
    /// menu with Clear.
    pub fn with_search(mut self) -> Self {
        self.is_search = true;
        self.placeholder = Some(crate::l10n::tr("search-placeholder"));
        self
    }

    pub fn with_placeholder(mut self, placeholder: &str) -> Self {
        self.placeholder = Some(placeholder.to_string());
        self
    }

    pub fn with_max_width(mut self, max_w: Option<f32>) -> Self {
        self.max_width = max_w;
        self
    }

    pub fn with_width(mut self, w: f32) -> Self {
        self.width = Some(w);
        self
    }
}

impl Layout for TextBox {

    /// One row for a single-line box; a multiline box has no natural height of its own —
    /// the host sizes it, and a layout strategy leaves its assigned rect alone.
    fn intrinsic_size(&self) -> Option<Size> {
        if self.multiline {
            return None;
        }
        Some(Size::new(0.0, crate::layout::textbox_height()))
    }

    fn hit_row_rect(&self) -> bool {
        true
    }

    /// The legacy `set_rect` width clamp: an explicit `width` wins, else cap at `max_width`.
    fn adjust_rect(&self, requested: Rect) -> Rect {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            requested.width.min(max_w)
        } else {
            requested.width
        };
        Rect { width: final_w, ..requested }
    }

    /// The legacy `set_row_rect` applied the same clamp to the row span.
    fn adjust_row_rect(&self, x: f32, w: f32) -> (f32, f32) {
        let final_w = if let Some(explicit_w) = self.width {
            explicit_w
        } else if let Some(max_w) = self.max_width {
            w.min(max_w)
        } else {
            w
        };
        (x, final_w)
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
        self.clamp_scroll();
    }
}

impl Paint for TextBox {
    fn color(&self) -> [f32; 4] {
        [0.10, 0.10, 0.16, 1.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::textbox_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            Some((r, (false, false, false, false)))
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    /// Content text font for the paint walk: a TextBox whose `font_family`/`font_size` was
    /// deliberately customized (cce-text-editor's monospace editor) draws its value text in
    /// that family at the label's own size — a bare family name, so the control-font string's
    /// size suffix doesn't override `font_size`. Default boxes keep the `widget_font` string
    /// verbatim (the legacy convention, size suffix included).
    fn text_font(&self) -> Option<String> {
        if self.font_family != self.default_font_family || self.font_size != self.default_font_size {
            Some(self.font_family.clone())
        } else {
            self.widget_font()
        }
    }

    fn text_attrs(&self) -> crate::scene::paint::TextAttrs {
        self.font_attrs
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
        Some(self.text_clip(rect))
    }

    /// The legacy `prepare_text`: sync font family/size with the live config defaults, then
    /// shape the display text and record per-glyph advances (`map_x_to_idx` reads them).
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        // The input method's composition, shown before the buffer is shaped.
        self.sync_preedit();
        let (style_family, style_size) = crate::layout::control_label_font_detached_parsed();
        if self.font_size == self.default_font_size {
            self.font_size = style_size;
        }
        self.default_font_size = style_size;

        if self.font_family == self.default_font_family {
            self.font_family = style_family.clone();
        }
        self.default_font_family = style_family;

        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        let showing_placeholder = text_src.is_empty() && self.placeholder.is_some();
        let display_text = if showing_placeholder {
            self.placeholder.as_ref().unwrap().as_str()
        } else {
            text_src.as_str()
        };

        // Measured in the family the value text is DRAWN in — see `value_font`.
        let font_fam = self.value_font();
        let scale = crate::scale::scale_factor().max(1.0);

        // One column's advance, from the same shaping path as the labels (buffer-cached,
        // so this is a lookup after the first frame per family/size).
        let probe = crate::backend::text::shared_text_buffer(
            fs,
            "MMMMMMMM",
            self.font_size,
            font_fam.as_deref(),
            self.font_attrs,
        );
        self.shaped_char_advance = probe
            .layout_runs()
            .next()
            .and_then(|run| run.glyphs.last().map(|g| (g.x + g.w) / scale / 8.0))
            .unwrap_or(0.0);

        // `char_width()` returns this frame's shaped advance from here on, so the
        // wrap below matches the one `selection_quads`/`value_labels` compute at
        // paint time.
        let wrap = self.multiline.then(|| self.wrap_width(self.rect.width).to_bits());

        let key = PrepKey {
            text: display_text.to_string(),
            placeholder: showing_placeholder,
            password: self.is_password,
            font_size_bits: self.font_size.to_bits(),
            font: font_fam.clone(),
            attrs: self.font_attrs,
            scale_bits: scale.to_bits(),
            vertical: crate::backend::text::vertical_text().is_some(),
            wrap,
            room_bits: (self.rect.width - 2.0 * self.pad()).max(0.0).to_bits(),
        };
        if self.prep_key.as_ref() != Some(&key) {
            self.shape_columns(fs, &key);
            self.prep_key = Some(key);
        }

        let cursor_pos = self.cursor_idx.min(self.glyph_positions.len().saturating_sub(1));
        self.cursor_x_offset = self.glyph_positions.get(cursor_pos).copied().unwrap_or(0.0);
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let pad = self.pad();
        let top = self.label_top();
        let base_y = rect.y - top;
        let base_h = rect.height + top;
        let visual_h = rect.height;
        let radius = crate::layout::textbox_corner_radius();
        let border_w = self.border_width();

        // Open for typing: say so to the compositor this frame (the
        // on-screen keyboard follows it). The field stands in for the caret
        // until the caret is drawn below, which reports itself.
        if self.editing && !self.disabled {
            let (ox, oy) = ctx.offset();
            crate::text_input::claim(self.rect.x + ox, self.rect.y + top + oy, self.rect.width, visual_h);
        }

        // Keep the model's cached rect and the paint rect consistent: paint receives the
        // content rect derived from the same base the cache holds, so the bodies below read
        // `self.rect` (the legacy `self.base`) exactly as legacy did. `rect` is used only to
        // localize this frame's geometry.
        let _ = (base_y, base_h);

        if radius <= 0.0 {
            // Legacy `extra_quads`: full base span, disabled
            // special-case with early return.
            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
            if self.disabled {
                if self.draw_bg_border {
                    quads.push((self.rect.x, self.rect.y + top, self.rect.width, visual_h, [0.12, 0.12, 0.16, 1.0]));
                    quads.push((self.rect.x + border_w, self.rect.y + top + border_w, self.rect.width - 2.0 * border_w, visual_h - 2.0 * border_w, [0.06, 0.06, 0.08, 1.0]));
                }
            } else {
                if self.draw_bg_border {
                    // One background regardless of focus — the focus treatment is
                    // the tinted recess rim (rounded path) / editing border, not a
                    // surface swap.
                    let bg_color = crate::colors::textbox_background_color();
                    let border_color = crate::colors::well_frame_color(self.hovered, self.editing);
                    quads.push((self.rect.x, self.rect.y + top, self.rect.width, visual_h, border_color));
                    quads.push((self.rect.x + border_w, self.rect.y + top + border_w, self.rect.width - 2.0 * border_w, visual_h - 2.0 * border_w, bg_color));
                }
                if let Some([cx, cy, cw, ch]) = self.selection_quads(self.rect.x, self.rect.width, &mut quads) {
                    let (ox, oy) = ctx.offset();
                    crate::ime::report_caret(cx + ox, cy + oy, cw, ch);
                }
            }
            for (qx, qy, qw, qh, qc) in quads {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
        } else {
            // Legacy `all_rounded_quads`: no disabled special-case.
            let x = self.rect.x;
            let w = self.rect.width;

            // One background regardless of focus (see the flat path above).
            let bg_color = crate::colors::textbox_background_color();
            let border_color = crate::colors::well_frame_color(self.hovered, self.editing);

            if self.draw_bg_border {
                let corners = (true, true, true, true);
                // Recessed + transparent fill: the carve alone defines the
                // well — the plate below is its floor, so the flat border and
                // bg rects are skipped entirely. An opaque fill (e.g. the edit
                // color while editing) draws as usual and gets carved.
                let bare = self.recessed() && bg_color[3] <= 0.001;
                if !bare {
                    ctx.rounded_rect(Rect { x, y: self.rect.y + top, width: w, height: visual_h }, radius, corners, border_color);
                    ctx.rounded_rect(
                        Rect { x: x + border_w, y: self.rect.y + top + border_w, width: w - 2.0 * border_w, height: visual_h - 2.0 * border_w },
                        (radius - border_w).max(0.0),
                        corners,
                        bg_color,
                    );
                }
                if let Some(field) = self.well() {
                    // Focus lights the well's rim in the highlight accent (with
                    // the shader's complementary shadow) — the TreeList treatment.
                    ctx.field(&field);
                }
            }

            let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
            if let Some([cx, cy, cw, ch]) = self.selection_quads(x, w, &mut quads) {
                let (ox, oy) = ctx.offset();
                crate::ime::report_caret(cx + ox, cy + oy, cw, ch);
            }
            for (qx, qy, qw, qh, qc) in quads {
                ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }

            // Relief scrollbar for overflowing multiline content — the shared
            // groove + raised-pill painter (the TreeList treatment), persistent
            // rather than activity-faded: an editor keeps its position
            // indicator. Content height mirrors clamp_scroll's math (`pad`
            // top and bottom), so the thumb tracks the scroll range exactly.
            if self.multiline {
                let line_height = self.line_height();
                let content_h = self.wrap_text(self.wrap_width(self.rect.width)).0.len() as f32 * line_height;
                crate::widget::container::scroll_box::paint_relief_scrollbar(
                    ctx,
                    Rect { x, y: self.rect.y + top, width: w, height: visual_h },
                    content_h + 2.0 * pad,
                    self.scroll_y,
                );
            }
        }

        // The content is whatever has been typed, so a line longer than the
        // well is routine rather than exceptional; the well scrolls, but
        // nothing stopped the glyphs drawing outside it. (The adapter's label
        // bridge swaps this for `text_bounds` — the same clip.)
        let well = Some(self.text_clip(Rect { x: self.rect.x, y: self.rect.y + top, width: self.rect.width, height: visual_h }));
        let font = self.value_font();
        for tl in self.value_labels() {
            ctx.text_with(tl.text, tl.x, tl.y, tl.font_size, tl.color, font.clone(), well);
        }
    }
}

impl Input for TextBox {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    /// A multi-line box that is editing types Tab, as an editor does; a one-line field lets
    /// the Tab walk take it (Tab leaves a field).
    fn keeps_tab(&self) -> bool {
        self.multiline && self.editing
    }
    /// Advances the wheel glide / trackpad coast behind the scroll offsets.
    /// Cheap when idle (the common case); `wants_tick` is unconditional
    /// because it is sampled once at registration.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        self.scroll_motion.reconcile(self.scroll_x, self.scroll_y);
        if !self.scroll_motion.is_animating() {
            return false;
        }
        let (mx, my) = self.scroll_max;
        let moved = self.scroll_motion.tick(dt, Bounds::max(mx), Bounds::max(my));
        self.scroll_x = self.scroll_motion.x.pos();
        self.scroll_y = self.scroll_motion.y.pos();
        moved || self.scroll_motion.is_animating()
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn tracks_base_focus(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        // A press moves the caret: a composition in progress is left where
        // it was, cancelled.
        if let Event::MouseButton { state: ElementState::Pressed, .. } = event {
            self.abandon_composition();
        }
        match event {
            Event::MouseButton { button: MouseButton::Right, state: ElementState::Pressed, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // Adapter hit-gates presses; legacy focused an un-editing box before opening
                // the menu (work-before-menu, so `opens_context_menu` can't express it).
                if !self.editing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // A click aims into the field: it places the caret where it landed,
                // whether or not it is the click that focuses. Only KEYBOARD focus
                // (`Event::FocusIn` -> `begin_editing`) arms select-all. That split is
                // the convention everywhere — tabbing selects a field, clicking points
                // into it — and it is what a prefilled box needs: select-all on the
                // focusing click meant the first keystroke wiped the whole value, which
                // is wrong for the ~26 prefilled single-line boxes across the fleet
                // (login username, reply subject, a unit's ExecStart, a config value).
                // This used to be carved out for multiline only; both arms now agree.
                let focusing = !self.editing;
                if focusing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                let idx = self.position_to_idx(*px, *py);
                self.cursor_idx = idx;
                self.select_anchor = Some(idx);
                self.all_selected = false;
                if focusing {
                    self.sync_editor_state();
                }
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x: px, y: py, .. } => {
                if self.disabled { return false; }
                // Legacy gated releases on the hit test; the adapter delivers them ungated, so
                // re-check containment (the plain block rect — row spans approximated).
                let (bx, by, bw, bh) = (self.rect.x, self.rect.y, self.rect.width, self.rect.height);
                if !(*px >= bx && *px <= bx + bw && *py >= by && *py <= by + bh) {
                    return false;
                }
                if self.dragging {
                    self.dragging = false;
                }
                if self.select_anchor == Some(self.cursor_idx) {
                    self.select_anchor = None;
                }
                true
            }
            Event::PointerMove { x: px, y: py, .. } => {
                // The drag-selection half of the legacy `on_cursor_moved`; hover bookkeeping
                // is the adapter's (Enter/Leave below).
                if self.disabled {
                    return false;
                }
                if self.dragging && self.editing {
                    return self.extend_selection_to(*px, *py);
                }
                false
            }
            Event::MouseEnter => {
                self.hovered = !self.disabled;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::MouseWheel { delta, .. } => self.handle_wheel(delta),
            Event::KeyInput(key_event) => self.handle_key(key_event),
            Event::FocusIn => {
                // The legacy `focus()`: enter editing and claim the global slot (unless
                // disabled — legacy early-returned before `set_focused`). Skipped when
                // already editing: a press-then-set_focused sequence must not re-arm
                // select-all over the caret the press just placed.
                if !self.disabled && !self.editing {
                    self.begin_editing();
                    ectx.request_focus();
                }
                false
            }
            Event::FocusOut => {
                self.commit_editing();
                false
            }
            _ => false,
        }
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn value_string(&self) -> Option<String> {
        Some(self.text.clone())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        self.set_value(val)
    }

    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        use crate::widget::ContextAction as CA;
        match action {
            CA::Cut => {
                let res = self.cut_selection();
                if res {
                    self.just_changed = true;
                }
                res
            }
            CA::Copy => {
                self.copy_selection();
                true
            }
            CA::Paste => {
                let res = self.paste_from_clipboard();
                if res {
                    self.just_changed = true;
                }
                res
            }
            CA::SelectAll => {
                self.select_all();
                true
            }
            CA::ClearText => {
                self.set_value("");
                true
            }
            CA::Undo => self.undo_edit(),
            CA::Redo => self.redo_edit(),
            _ => false,
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        !self.disabled
    }

    fn is_dragging(&self) -> bool {
        self.dragging
    }

    fn drag_begin(&mut self, _px: f32, _py: f32, _rect: Rect) {
        if self.disabled || !self.editing { return; }
        self.dragging = true;
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.disabled || !self.editing { return false; }
        self.extend_selection_to(px, py)
    }

    fn drag_end(&mut self) {
        self.dragging = false;
    }
}

/// Legacy `Default` (an empty box) — settings' accounts page derives `Default` over fields of
/// this type.
impl Default for Adapted<TextBox> {
    fn default() -> Self {
        TextBox::new(String::new())
    }
}

unsafe impl Send for TextBox {}
unsafe impl Sync for TextBox {}


#[cfg(test)]
mod tests {
    use super::*;

    /// The multiline caret/click math reads shaped per-line offsets; a caret
    /// must land exactly where it is drawn, on every column of every line.
    /// A right-to-left word in a one-line box is set against the box's right edge, and
    /// edited where it is drawn: the caret before each letter stands at its right edge, so
    /// the offsets fall from the right end to the word's left; a click at the right end is
    /// the start, at the word's left the end; the drawn text starts where the offsets do.
    /// A selection from inside English into Hebrew is two pieces. The bidi levels come from
    /// the text, so this holds whatever face draws the letters.
    #[test]
    fn a_right_to_left_word_is_edited_where_it_is_drawn() {
        let mut fs = crate::create_font_system();
        let mut tb = TextBox::new("שלום".to_string());
        tb.set_rect(10.0, 10.0, 300.0, 30.0);
        tb.prepare_text(&mut fs);
        let offs = tb.glyph_positions.clone();
        assert_eq!(offs.len(), 5, "four letters and the end");
        if tb.total_text_width == 0.0 {
            return; // nothing shaped (no fonts at all): nothing to check
        }
        let pad = tb.inner().pad();
        let (left, room) = (10.0 + pad, 300.0 - 2.0 * pad);
        assert!(offs.windows(2).all(|w| w[1] < w[0]), "carets fall right to left: {offs:?}");
        assert!((offs[0] - room).abs() < 0.01, "set against the right edge: {offs:?}");
        assert!((offs[4] - (room - tb.total_text_width)).abs() < 0.01, "{offs:?}");
        assert_eq!(tb.inner().map_x_to_idx(left + room), 0, "the right end is the start");
        assert_eq!(tb.inner().map_x_to_idx(left + offs[4]), 4, "the word's left is the end");
        let label = &tb.inner().value_labels()[0];
        assert!((label.x - (left + offs[4])).abs() < 0.01, "drawn where the offsets are: {} vs {}", label.x, left + offs[4]);

        // English then Hebrew: a left-to-right line, as it was; a selection from the "b" to
        // the Hebrew word's first letter is the b and that letter at the word's far right.
        let mut tb = TextBox::new("ab שלום".to_string());
        tb.set_rect(10.0, 10.0, 300.0, 30.0);
        tb.inner_mut().editing = true;
        tb.inner_mut().edit_buffer = "ab שלום".to_string();
        tb.prepare_text(&mut fs);
        assert!(tb.glyph_positions[0].abs() < 0.01, "a left-to-right line starts at the left");
        tb.inner_mut().select_anchor = Some(1);
        tb.inner_mut().cursor_idx = 4;
        let mut quads = Vec::new();
        tb.inner().selection_quads(10.0, 300.0, &mut quads);
        let sel: Vec<_> = quads.iter().filter(|q| q.2 > 2.0).collect();
        assert_eq!(sel.len(), 2, "two pieces: {sel:?}");
    }

    /// In a multiline box a Hebrew paragraph's lines are set against the right edge and an
    /// English paragraph's are not; the drawn lines carry the same shift as the offsets.
    #[test]
    fn a_right_to_left_paragraph_is_set_against_the_right() {
        let mut fs = crate::create_font_system();
        let mut tb = TextBox::new("hello there\nשלום עולם".to_string()).with_multiline(true).with_line_wrap(true);
        tb.set_rect(10.0, 10.0, 300.0, 200.0);
        tb.prepare_text(&mut fs);
        let room = 300.0 - 2.0 * tb.inner().pad();
        let lines = tb.line_glyph_positions.clone();
        assert_eq!(lines.len(), 2);
        if lines[1].iter().all(|x| *x == 0.0) {
            return; // nothing shaped
        }
        assert!(lines[0][0].abs() < 0.01, "English starts at the left");
        let widest = lines[1].iter().copied().fold(0.0f32, f32::max);
        assert!((widest - room).abs() < 0.01, "Hebrew ends at the right: {:?}", lines[1]);
        let labels = tb.inner().value_labels();
        assert!(labels[1].x > labels[0].x + 50.0, "the Hebrew line is drawn shifted: {} vs {}", labels[1].x, labels[0].x);
    }

    /// A one-line box shows the START of an overflowing line: the left end of English, the
    /// right end of Hebrew, whose characters run from the right. Editing follows the caret.
    #[test]
    fn an_overflowing_right_to_left_line_shows_its_start() {
        let mut fs = crate::create_font_system();
        let long = "שלום עולם ".repeat(8);
        let mut tb = TextBox::new(long).with_multiline(false);
        tb.set_rect(10.0, 10.0, 120.0, 28.0);
        tb.prepare_text(&mut fs);
        let room = 120.0 - 2.0 * tb.inner().pad();
        let total = tb.inner().total_text_width;
        if total <= room {
            return; // nothing shaped
        }
        assert!((tb.inner().scroll_x - (total - room)).abs() < 0.01, "scrolled to the right end: {} of {}", tb.inner().scroll_x, total - room);
        // The first character's stop lies in view.
        let first = tb.inner().glyph_positions[0];
        let x = first - tb.inner().scroll_x;
        assert!((0.0..=room + 0.5).contains(&x), "the line's first character is shown: {x}");

        let mut english = TextBox::new("hello there ".repeat(8)).with_multiline(false);
        english.set_rect(10.0, 10.0, 120.0, 28.0);
        english.prepare_text(&mut fs);
        assert_eq!(english.inner().scroll_x, 0.0, "English shows its left end");
    }

    /// A multiline box wraps by the shaped width of what it holds, not a count of chars:
    /// every line fits the box (a hanging space aside) and none breaks early — the next
    /// line's first word would not have fitted on it. Text of mixed widths (narrow, wide,
    /// CJK) is where a column count and the drawn width disagree.
    #[test]
    fn a_multiline_box_wraps_where_its_text_is_wide() {
        let text = "iiii WWWW lll MMM 漢字漢字 iii WW 漢字漢字漢字 il WM 漢字".repeat(3);
        let mut tb = TextBox::new(text.clone()).with_multiline(true).with_line_wrap(true);
        crate::widget::WidgetHost::set_rect(&mut tb, 0.0, 0.0, 160.0, 400.0);
        let inner = tb.inner();
        let max_w = inner.wrap_width(160.0);
        let (lines, map) = inner.wrap_text(max_w);
        assert!(lines.len() > 2, "{lines:?}");
        let width = |s: &str| inner.char_advances(s).iter().sum::<f32>();
        for (i, line) in lines.iter().enumerate() {
            let shown = line.trim_end();
            assert!(width(shown) <= max_w + 0.5 || shown.chars().count() == 1, "line {i} {shown:?} is {} wide, past {max_w}", width(shown));
            if let Some(next) = lines.get(i + 1) {
                let word: String = next.chars().take_while(|c| !c.is_whitespace()).collect();
                if !word.is_empty() && line.ends_with(' ') {
                    assert!(width(&format!("{line}{word}")) > max_w + 0.5, "line {i} {line:?} broke before {word:?}, which fitted");
                }
            }
        }
        let rejoined: String = lines.concat();
        assert_eq!(rejoined, text, "the lines are the text, cut");
        assert_eq!(map.len(), text.chars().count() + 1);
    }

    #[test]
    fn multiline_shaped_offsets_round_trip() {
        let mut fs = cosmic_text::FontSystem::new();
        let mut tb = TextBox::new("wim wim wim\niiii WWWW mm ii".to_string())
            .with_multiline(true)
            .with_line_wrap(false);
        tb.set_rect(10.0, 10.0, 300.0, 100.0);
        tb.prepare_text(&mut fs);

        assert_eq!(tb.line_glyph_positions.len(), 2, "one offset row per line");
        for line in &tb.line_glyph_positions {
            for w in line.windows(2) {
                assert!(w[1] >= w[0], "offsets must be non-decreasing: {:?}", line);
            }
        }

        // Round-trip caret→pixel→column. Skipped in a font-less environment
        // (no glyphs shape, offsets stay zero — nothing to verify).
        for (l, line) in tb.line_glyph_positions.clone().iter().enumerate() {
            if line.last().copied().unwrap_or(0.0) == 0.0 {
                continue;
            }
            for c in 0..line.len() {
                assert_eq!(
                    tb.line_x_to_col(l, tb.line_col_x(l, c)),
                    c,
                    "line {l} col {c} must round-trip"
                );
            }
        }
    }

    /// Single-line offsets must be non-decreasing for MULTI-WORD text. Guards
    /// the `normalized_glyph_starts` workaround for cosmic-text 0.12's
    /// `Shaping::Basic` bug (span-relative `LayoutGlyph::start`, resetting at
    /// every word): without it, each word's glyphs overwrite the low columns
    /// and a mid-text caret lands inside the wrong word.
    #[test]
    fn single_line_multi_word_offsets_monotonic() {
        let mut fs = cosmic_text::FontSystem::new();
        let mut tb = TextBox::new("wim wim wim".to_string());
        tb.set_rect(10.0, 10.0, 300.0, 30.0);
        tb.prepare_text(&mut fs);

        for w in tb.glyph_positions.windows(2) {
            assert!(w[1] >= w[0], "offsets must be non-decreasing: {:?}", tb.glyph_positions);
        }
        // In a font-bearing environment the mid-text columns are real advances:
        // strictly inside (0, total). Skipped font-less (all zeros).
        if tb.glyph_positions.last().copied().unwrap_or(0.0) > 0.0 {
            let total = *tb.glyph_positions.last().unwrap();
            for (i, &x) in tb.glyph_positions.iter().enumerate().skip(1).take(tb.glyph_positions.len() - 2) {
                assert!(x > 0.0 && x < total, "col {i} offset {x} must sit inside the text run");
            }
        }
    }

    /// Multi-byte text gets one offset per CHAR (not per byte), and the
    /// per-frame memo notices a change of text: the second shape must not
    /// serve the first one's offsets.
    #[test]
    fn offsets_count_chars_and_reshape_on_change() {
        let mut fs = cosmic_text::FontSystem::new();
        let mut tb = TextBox::new("héllo wörld".to_string());
        tb.set_rect(10.0, 10.0, 300.0, 30.0);
        tb.prepare_text(&mut fs);
        assert_eq!(tb.glyph_positions.len(), "héllo wörld".chars().count() + 1);
        for w in tb.glyph_positions.windows(2) {
            assert!(w[1] >= w[0], "offsets must be non-decreasing: {:?}", tb.glyph_positions);
        }

        let first = tb.glyph_positions.clone();
        tb.prepare_text(&mut fs);
        assert_eq!(tb.glyph_positions, first, "an unchanged box keeps its offsets");

        tb.text = "hé".to_string();
        tb.prepare_text(&mut fs);
        assert_eq!(tb.glyph_positions.len(), 3, "a changed text is reshaped");
    }

    #[test]
    fn test_textbox_selection_highlight() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Initial Text".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial state
        assert!(!tb.editing);
        assert!(!tb.all_selected);

        // 2. Keyboard focus triggers highlighting. (A CLICK deliberately does not
        //    — it places the caret; see `prefilled_single_line_click_does_not_wipe_the_value`.)
        tb.focus();
        assert!(tb.editing);
        assert!(tb.all_selected);
        assert_eq!(tb.edit_buffer, "Initial Text");

        // 3. Typing a key replaces all text
        let key_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("A".to_string()),
            text: Some("A".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let handled = tb.keyboard_input(&key_ev, &mut dummy);
        assert!(handled);
        assert!(!tb.all_selected);
        assert_eq!(tb.edit_buffer, "A");

        // 4. Pressing Enter commits change
        let enter_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let handled_enter = tb.keyboard_input(&enter_ev, &mut dummy);
        assert!(handled_enter);
        assert!(!tb.editing);
        assert_eq!(tb.text, "A");
        assert!(tb.take_change());
    }

    /// The other half of the click change: keyboard focus must still arm
    /// select-all, so tabbing into a field and typing replaces it. Losing this
    /// would make every form field tedious to retype.
    #[test]
    fn keyboard_focus_still_selects_all() {
        let mut tb = TextBox::new("imap.example.org:993".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        tb.focus();

        assert!(tb.editing);
        assert!(tb.all_selected, "tabbing in selects the whole value");
        assert_eq!(tb.select_anchor, Some(0));
        assert_eq!(tb.cursor_idx, "imap.example.org:993".chars().count());
    }

    /// A prefilled single-line box is the case that motivated the change: the
    /// focusing click must leave the value intact so the first keystroke edits
    /// rather than erases. Same guarantee the multiline test below asserts.
    #[test]
    fn prefilled_single_line_click_does_not_wipe_the_value() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("imap.example.org:993".to_string());
        tb.set_rect(10.0, 10.0, 300.0, 30.0);

        let click_x = 10.0 + 8.0 + 4.0 * tb.char_width();
        assert!(tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x, 20.0, &mut dummy));
        assert!(tb.editing);
        assert!(!tb.all_selected);

        let key_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("X".to_string()),
            text: Some("X".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(tb.keyboard_input(&key_ev, &mut dummy));
        assert_eq!(
            tb.edit_buffer.chars().count(),
            21,
            "typing must insert into the prefilled value, not replace it"
        );
        assert!(tb.edit_buffer.starts_with("imap"), "the existing value survives the first keystroke");
    }

    /// An input method's composition is a provisional run in the buffer: shown
    /// at the caret with the caret where the input method has it, never held
    /// (`committed_buffer`, `take_change`), replaced by the commit, which is
    /// typed — and dropped, the input method told, when editing ends under it.
    #[test]
    fn a_composition_is_shown_in_place_and_the_commit_is_typed() {
        use crate::ime::{self, Preedit};
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("ab".to_string()).with_update_on_type(true);
        tb.set_rect(10.0, 10.0, 300.0, 30.0);
        tb.focus();
        tb.cursor_idx = 1;
        tb.select_anchor = None;
        tb.all_selected = false;
        let _ = ime::take_reset();

        // "にほ", the input method's cursor after "に".
        ime::set_preedit(Some(Preedit::new("にほ", Some((0, 3)))));
        tb.sync_preedit();
        assert_eq!(tb.edit_buffer, "aにほb");
        assert_eq!(tb.composing, Some((1, 2)));
        assert_eq!(tb.cursor_idx, 2);
        assert_eq!(tb.committed_buffer(), "ab");
        assert!(!tb.take_change(), "a composition is not a change");
        // A key the input method lets through mid-composition is not typed.
        let typed = |t: &str| KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character(t.to_string()),
            text: Some(t.to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        assert!(tb.keyboard_input(&typed("x"), &mut dummy));
        assert_eq!(tb.edit_buffer, "aにほb");

        // The commit: the composition ends, then its text is typed.
        ime::set_preedit(None);
        assert!(tb.keyboard_input(&typed("日本"), &mut dummy));
        assert_eq!(tb.edit_buffer, "a日本b");
        assert_eq!((tb.composing, tb.cursor_idx), (None, 3));
        assert!(tb.take_change());
        assert_eq!(tb.text, "a日本b");

        // Editing ends mid-composition: the run goes, the input method is
        // told, and the text is what was held.
        ime::set_preedit(Some(Preedit::new("か", None)));
        tb.sync_preedit();
        assert_eq!(tb.edit_buffer, "a日本かb");
        tb.commit_editing();
        assert_eq!(tb.text, "a日本b");
        assert!(ime::take_reset());
        assert_eq!(ime::preedit(), None);
    }

    /// Undo/redo over an editing session: a typed word is one step, a
    /// space starts the next, a cursor move splits a run, Backspace runs
    /// coalesce, redo walks forward, a fresh keystroke after an undo forks,
    /// and the chord reaches the box both as a `ContextAction` (the runner's
    /// route) and as a raw key (the no-context fallback).
    #[test]
    fn typing_undoes_by_run() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new(String::new());
        tb.set_rect(10.0, 10.0, 300.0, 30.0);
        tb.focus();
        assert!(tb.editing);
        assert!(!tb.undo_edit(), "a fresh session has nothing to undo");

        let key = |k: &str, ctrl: bool, shift: bool| KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character(k.to_string()),
            text: if ctrl { None } else { Some(k.to_string()) },
            repeat: false,
            ctrl,
            shift,
            alt: false,
        };
        let named = |n: NamedKey| KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(n),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let type_str = |tb: &mut Adapted<TextBox>, dummy: &mut crate::context::UiContext, s: &str| {
            for ch in s.chars() {
                assert!(tb.keyboard_input(&key(&ch.to_string(), false, false), dummy));
            }
        };

        type_str(&mut tb, &mut dummy, "hello world");
        assert_eq!(tb.edit_buffer, "hello world");
        assert_eq!(tb.history.undo_len(), 3, "'hello', ' ', 'world'");

        // A cursor move splits the next run off.
        assert!(tb.keyboard_input(&named(NamedKey::ArrowLeft), &mut dummy));
        type_str(&mut tb, &mut dummy, "XY");
        assert_eq!(tb.edit_buffer, "hello worlXYd");
        assert_eq!(tb.history.undo_len(), 4);

        // Backspaces coalesce into one step.
        assert!(tb.keyboard_input(&named(NamedKey::Backspace), &mut dummy));
        assert!(tb.keyboard_input(&named(NamedKey::Backspace), &mut dummy));
        assert_eq!(tb.edit_buffer, "hello world");
        assert_eq!(tb.history.undo_len(), 5);

        // Undo through the ContextAction route, then the raw-chord route.
        assert!(crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Undo));
        assert_eq!(tb.edit_buffer, "hello worlXYd");
        assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
        assert_eq!(tb.edit_buffer, "hello world");
        assert!(tb.keyboard_input(&key("Z", true, true), &mut dummy), "redo via ctrl+shift+z");
        assert_eq!(tb.edit_buffer, "hello worlXYd");
        assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
        assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
        assert_eq!(tb.edit_buffer, "hello ");
        assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
        assert_eq!(tb.edit_buffer, "hello");
        assert!(tb.keyboard_input(&key("z", true, false), &mut dummy));
        assert_eq!(tb.edit_buffer, "");
        assert!(!tb.undo_edit(), "history exhausted");

        // Redo forward one, then a fresh keystroke forks the branch.
        assert!(crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Redo));
        assert_eq!(tb.edit_buffer, "hello");
        type_str(&mut tb, &mut dummy, "!");
        assert_eq!(tb.edit_buffer, "hello!");
        assert!(!tb.redo_edit(), "a new edit after an undo drops the redo branch");

        // A committed value is not the box's to undo.
        tb.unfocus();
        assert!(!tb.editing);
        assert!(!crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Undo));
    }

    #[test]
    fn empty_box_click_ignores_placeholder_glyphs() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new(String::new()).with_placeholder("Search...");
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // `prepare_text` shapes the placeholder when the value is empty; the
        // caret math must still treat the box as zero-length. Simulate the
        // shaped placeholder ("Search...", 9 cols) directly so the test does
        // not depend on a font being present.
        tb.glyph_positions = (0..=9).map(|i| i as f32 * 7.0).collect();

        // Click deep into the painted placeholder.
        let click_x = 10.0 + 8.0 + 8.0 * 7.0;
        assert!(tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x, 20.0, &mut dummy));
        assert!(tb.editing);
        assert_eq!(tb.cursor_idx, 0, "empty box: the caret lands at the start, not on a placeholder column");
    }

    #[test]
    fn multiline_focus_click_places_caret_instead_of_select_all() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("abcdef".to_string()).with_multiline(true);
        tb.set_rect(10.0, 10.0, 200.0, 100.0);

        // The focusing click must NOT arm select-all (a first keystroke would wipe
        // prefilled content, e.g. a reply quote) — it places the caret like any click.
        let clicked = tb.mouse_input(MouseButton::Left, ElementState::Pressed, 12.0, 20.0, &mut dummy);
        assert!(clicked);
        assert!(tb.editing);
        assert!(!tb.all_selected);
        let caret_after_click = tb.cursor_idx;
        let key_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("X".to_string()),
            text: Some("X".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let handled = tb.keyboard_input(&key_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.edit_buffer.chars().count(), 7, "typing must insert, not replace the buffer");
        assert!(tb.edit_buffer.contains("X"));
        assert_eq!(tb.cursor_idx, caret_after_click + 1);
    }

    #[test]
    fn test_textbox_drag_and_modifier_selection() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Hello World".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // 1. Initial click focuses and places the caret where it landed — it does
        //    NOT select all. Selecting all belongs to keyboard focus (see
        //    `keyboard_focus_still_selects_all`).
        let click_x3 = 10.0 + 8.0 + 3.0 * tb.char_width();
        let pressed = tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x3, 20.0, &mut dummy);
        assert!(pressed);
        let released = tb.mouse_input(MouseButton::Left, ElementState::Released, click_x3, 20.0, &mut dummy);
        assert!(released);
        assert!(tb.editing);
        assert!(!tb.all_selected);
        assert_eq!(tb.cursor_idx, 3);
        // The release collapses the zero-width selection the press opened.
        assert_eq!(tb.select_anchor, None);

        // 2. Click inside placed caret at index 5
        let click_x5 = 10.0 + 8.0 + 5.0 * tb.char_width();
        let pressed_inside = tb.mouse_input(MouseButton::Left, ElementState::Pressed, click_x5, 20.0, &mut dummy);
        assert!(pressed_inside);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, Some(5));
        assert!(!tb.all_selected);

        // 3. Drag to index 11
        let drag_x11 = 10.0 + 8.0 + 11.0 * tb.char_width();
        tb.drag_begin(click_x5, 20.0);
        let updated = tb.drag_update(drag_x11, 20.0);
        assert!(updated);
        assert_eq!(tb.cursor_idx, 11);
        assert_eq!(tb.select_anchor, Some(5));
        tb.drag_end();

        // 4. Keyboard ArrowLeft with Shift shrinks selection from 11 to 10
        let left_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
            alt: false,
        };
        let handled = tb.keyboard_input(&left_shift_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 10);
        assert_eq!(tb.select_anchor, Some(5));

        // 5. Keyboard ArrowLeft without Shift collapses selection to start (index 5)
        let left_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let handled = tb.keyboard_input(&left_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 5);
        assert_eq!(tb.select_anchor, None);

        // 6. Keyboard Shift+Up highlights to beginning (cursor 0, anchor 5)
        let up_shift_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: true,
            alt: false,
        };
        let handled = tb.keyboard_input(&up_shift_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.cursor_idx, 0);
        assert_eq!(tb.select_anchor, Some(5));

        // 7. Typing a key replaces selected range "Hello" with "Rust"
        let rust_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("Rust".to_string()),
            text: Some("Rust".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        };
        let handled = tb.keyboard_input(&rust_ev, &mut dummy);
        assert!(handled);
        assert_eq!(tb.edit_buffer, "Rust World");
        assert_eq!(tb.cursor_idx, 4);
        assert_eq!(tb.select_anchor, None);
    }

    #[test]
    fn test_textbox_right_click_context_menu() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Context Menu Text".to_string());
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // Hide context menu initially
        dummy.hide_context_menu();
        assert!(!dummy.is_context_menu_visible());

        // Right click on textbox
        let clicked = tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(clicked);
        assert!(dummy.is_context_menu_visible());
        assert!(tb.editing);
     }

    #[test]
    fn test_textbox_multiline_selection_highlight() {
        let dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Line 1\nLine 2\nLine 3".to_string()).with_multiline(true);
        tb.set_rect(10.0, 10.0, 200.0, 100.0);
        tb.select_anchor = Some(7); // starts at "Line 2"
        tb.cursor_idx = 13;        // ends at end of "Line 2"

        let has_rounded = WidgetHost::corner_style(&tb).1 != (false, false, false, false);
        let has_highlight = if has_rounded {
            let rounded = tb.all_rounded_quads(&dummy);
            println!("Rounded quads: {:?}", rounded);
            let quads = tb.all_quads(&dummy);
            rounded.iter().any(|q| q.5 == [0.20, 0.50, 0.85, 0.3])
                || quads.iter().any(|q| q.4 == [0.20, 0.50, 0.85, 0.3])
        } else {
            let extra = tb.extra_quads();
            println!("Extra quads: {:?}", extra);
            extra.iter().any(|q| q.4 == [0.20, 0.50, 0.85, 0.3])
        };
        assert!(has_highlight, "Should have a highlight quad!");
    }

    #[test]
    fn test_textbox_line_wrap_disabled_horizontal_scrolling() {
        let _dummy = crate::context::UiContext::new();

        // Wrap is stated on the widget (`line_wrap_override`), not on the
        // process-global `textbox_line_wrap` — this box is single-line, which
        // is already unwrapped, and the override says so whatever the config
        // does. The global would be visible to every other test in parallel.
        let mut tb = TextBox::new("Very long text that should not wrap and instead scroll horizontally".to_string())
            .with_line_wrap(false);
        tb.set_rect(10.0, 10.0, 100.0, 30.0);
        assert!(!tb.line_wrap_enabled());

        assert_eq!(tb.scroll_x, 0.0);

        tb.focus();
        tb.cursor_idx = tb.edit_buffer.chars().count();
        tb.scroll_to_cursor();

        assert!(tb.scroll_x > 0.0, "scroll_x should be scrolled horizontally to keep the cursor visible");
    }

    #[test]
    fn test_search_textbox_clear_option() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("Some Search query".to_string()).with_placeholder("Search...");
        tb.set_rect(10.0, 10.0, 200.0, 30.0);

        // Right click on textbox
        let clicked = tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(clicked);
        assert!(dummy.is_context_menu_visible());

        // Verify "Clear" option is in options
        let opts = crate::widget::context_menu::options();
        assert!(opts.contains(&"Clear".to_string()));

        // Simulate choosing the "Clear" option
        crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::ClearText);
        assert_eq!(tb.text, "");
        assert_eq!(tb.edit_buffer, "");
    }

    /// A single-line box's border is always the hairline; a multiline box's
    /// is whatever `textbox_multiline_border_width` resolves to. Read, never
    /// written: that width is a process global and the suite runs in
    /// parallel, so setting it here would widen every other test's borders.
    #[test]
    fn test_multiline_textbox_border_width() {
        let _dummy = crate::context::UiContext::new();
        let tb_single = TextBox::new("Singleline".to_string()).with_multiline(false);
        let tb_multi = TextBox::new("Multiline".to_string()).with_multiline(true);

        let configured = crate::layout::textbox_multiline_border_width();
        assert_eq!(tb_single.border_width(), 1.0);
        assert_eq!(tb_multi.border_width(), configured);
    }
    /// A tall recessed box's wall is the full `bevel_width`, deeper than the
    /// old fixed 8px inset: its text starts on the well's floor, past the
    /// wall, not on it (cce-fonts' preview box drew its sample over its relief).
    #[test]
    fn tall_box_text_starts_past_its_relief_wall() {
        let _dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new("The quick brown fox".to_string())
            .with_multiline(true)
            .with_recessed(true);
        tb.set_rect(10.0, 10.0, 400.0, 180.0);
        let field = tb.well().expect("a recessed box with rounded corners carves a well");
        let floor_x = field.rect.x + field.depth;
        let floor_y = field.rect.y + field.depth;
        let first = &tb.value_labels()[0];
        assert!(first.x >= floor_x, "text x {} is on the wall (floor at {})", first.x, floor_x);
        assert!(first.y >= floor_y, "text y {} is on the wall (floor at {})", first.y, floor_y);
    }
    /// A password box never hands its text to the clipboard -- not from a
    /// selection, not by Ctrl+X, not through the menu -- and its menu offers
    /// no Cut or Copy. Its Ctrl+X leaves the text in place.
    #[test]
    fn a_password_box_never_copies_or_cuts() {
        let mut dummy = crate::context::UiContext::new();
        let mut tb = TextBox::new(String::new()).with_password(true);
        tb.set_rect(10.0, 10.0, 200.0, 30.0);
        tb.focus();
        tb.edit_buffer = "hunter2".to_string();
        tb.cursor_idx = 7;
        tb.select_all();
        let state = TextEditorState {
            buffer: tb.edit_buffer.clone(),
            cursor_idx: tb.cursor_idx,
            select_anchor: tb.select_anchor,
            all_selected: tb.all_selected,
        };
        assert_eq!(state.selected_text().as_deref(), Some("hunter2"), "the selection is there");
        assert_eq!(tb.clipboard_text(&state), None, "but never for the clipboard");

        let ctrl_x = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("x".to_string()),
            text: None,
            repeat: false,
            ctrl: true,
            shift: false,
            alt: false,
        };
        tb.keyboard_input(&ctrl_x, &mut dummy);
        assert_eq!(tb.edit_buffer, "hunter2", "Ctrl+X cuts nothing");
        assert!(!crate::widget::WidgetHostExt::context_action(&mut tb, crate::widget::ContextAction::Cut));
        assert_eq!(tb.edit_buffer, "hunter2", "nor does the menu's Cut");

        dummy.hide_context_menu();
        assert!(tb.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy));
        let opts = crate::widget::context_menu::options();
        assert!(!opts.iter().any(|o| o == "Cut" || o == "Copy"), "no Cut / Copy rows: {opts:?}");
        assert!(opts.iter().any(|o| o == "Paste"), "Paste stays: {opts:?}");
        dummy.hide_context_menu();
    }

    /// The gate is the password flag alone: an ordinary box's selection goes.
    #[test]
    fn an_ordinary_box_selection_is_clipboard_text() {
        let tb = TextBox::new("hello".to_string());
        let state = TextEditorState {
            buffer: "hello".to_string(),
            cursor_idx: 5,
            select_anchor: Some(0),
            all_selected: false,
        };
        assert_eq!(tb.clipboard_text(&state).as_deref(), Some("hello"));
    }
}
