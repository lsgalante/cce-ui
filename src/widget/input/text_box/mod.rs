//! `TextBox`: a one- or multi-line text field — the toolkit's editing widget. It edits through
//! a `TextEditorState` (`widget::editor`) while focused and commits on Enter, a click elsewhere
//! or focus loss (`committed_buffer`, `take_change`); it shapes its own glyphs in
//! [`Paint::prepare_text`], so caret, selection and click-to-index read the measured advances;
//! it wraps a multi-line value by shaped width; it shows an input-method composition as a
//! provisional run that is never committed or recorded (`docs/runtime.md`, "Input methods");
//! it carries its own undo history of typing and a clipboard through `widget::clipboard`; and
//! it is a well, or half of a field when joined to a picker (`joined_right`; `docs/surfaces.md`).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, construction and setters, `impl Layout`, the shared font database |
//! | `shaping` | measuring and wrapping: per-glyph advances, x ↔ column and index mapping |
//! | `geometry` | the well's border, padding and clip, content width, scrolling to the caret |
//! | `editing` | editor state, undo and redo, the clipboard, selection, keys |
//! | `composition` | the input-method composition: shown in place, never held |
//! | `paint` | selection quads, the value's text, `impl Paint` |
//! | `input` | the wheel and `impl Input` |

mod composition;
mod editing;
mod geometry;
mod input;
mod paint;
mod shaping;
#[cfg(test)]
mod tests;


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

/// Legacy `Default` (an empty box) — settings' accounts page derives `Default` over fields of
/// this type.
impl Default for Adapted<TextBox> {
    fn default() -> Self {
        TextBox::new(String::new())
    }
}

unsafe impl Send for TextBox {}

unsafe impl Sync for TextBox {}
