//! `DocEditor`: a document editor with Markdown live preview — cce-notes'
//! editing mode (Obsidian-on-cce milestone 6), and a fast plain editor with
//! preview off.
//!
//! - [`buffer`]: the text as lines, caret and selection, undoable edits.
//! - [`preview`]: how a line shows — markup hidden except on the caret's
//!   line, where it shows dimmed (Obsidian's live preview).
//! - [`layout`]: one styled line wrapped into runs, with the x of every
//!   byte, so drawing, caret and clicks agree.
//!
//! The editor itself is split by concern: `incremental` (which lines show raw, estimated and
//! laid-out heights, the line at a y), `composition` (the input method's composition, laid out in
//! place on the caret's line), `caret` (its geometry, the position at a point, vertical moves),
//! `keys` (the keymap and the edits it makes, undo and redo), `pointer` (presses, drags, links,
//! the wheel and the scroll's tick) and `paint`.
//!
//! **Incremental.** A line is laid out (shaped) only when it is drawn and
//! has changed: an edit invalidates the lines it replaced
//! ([`buffer::Change`]), a moved caret the lines it left and entered (their
//! markers show or hide), a fence the block contexts after it. Lines never
//! drawn keep an estimated height, so opening a long file shapes one
//! screen of it.
//!
//! **Driven by its host, not a registered widget.** The app forwards keys,
//! pointer events and the wheel, and paints it into its display list
//! ([`DocEditor::paint`]); answers come back as [`Response`] — a link to
//! follow is the host's to resolve. The caret does not blink: a blinking
//! caret is a frame every half second for as long as the window is open.

pub mod buffer;
pub mod layout;
pub mod preview;

mod caret;
mod composition;
mod incremental;
mod keys;
mod paint;
mod pointer;
#[cfg(test)]
mod tests;

use std::time::Duration;
use web_time::Instant;

pub use buffer::{Buffer, EditKind, Pos};
pub use layout::EditorTheme;
pub use preview::Target;

use crate::scene::layout::Rect;
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::shaping::ShapingMeasure;
use crate::widget::{Bounds, EmbedImage, Key, KeyEvent, MouseScrollDelta, NamedKey, ScrollMotion};
use layout::{Deco, LineLayout};
use preview::{Context, Kind, Marker, Prop};


/// What a key or click did, for the host.
#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    None,
    /// The caret or selection moved; the text did not change.
    Moved,
    /// The text changed.
    Changed,
    /// A link was clicked (rendered) or Ctrl+clicked (raw).
    Follow(Target),
}

const DOUBLE_CLICK: Duration = Duration::from_millis(400);

pub struct DocEditor {
    pub buf: Buffer,
    /// Live preview: markup hidden except on the caret's lines. Off: every
    /// line shows as written (with its syntax still dimmed).
    pub preview: bool,
    pub theme: EditorTheme,
    /// The widest the text column grows (Obsidian's readable line length);
    /// 0 lets it fill the rect.
    pub max_width: f32,
    pub pad: f32,
    measure: ShapingMeasure,
    ctx: Vec<Context>,
    /// The frontmatter lines' Properties roles (`None` elsewhere).
    props: Vec<Option<Prop>>,
    layouts: Vec<Option<LineLayout>>,
    heights: Vec<f32>,
    tops: Vec<f32>,
    tops_dirty: bool,
    /// Lines laid out as active (raw), so a caret move relays out the lines
    /// it leaves and the ones it enters.
    shown_active: (usize, usize),
    width: f32,
    pub scroll: f32,
    motion: ScrollMotion,
    /// The x a vertical move keeps to.
    want_x: Option<f32>,
    dragging: bool,
    clicks: Option<(Instant, Pos, u8)>,
    /// Where the text column was last painted: its origin and the rect.
    origin: (f32, f32),
    viewport: Rect,
    follow_caret: bool,
    /// The host's embedded images, by link text (see [`DocEditor::set_images`]).
    images: Option<Box<dyn Fn(&str) -> Option<EmbedImage>>>,
    /// An input method's composition (see `crate::ime`), at the source
    /// position it began, and the `ime::generation` it was taken at. It is
    /// in the caret line's LAYOUT, never in the buffer — the buffer is what
    /// the host saves — so every column read off that line's layout is
    /// mapped across it (`laid_col`, `source_col`).
    composition: Option<(Pos, crate::ime::Preedit)>,
    ime_seen: u64,
}

impl DocEditor {
    /// An editor over `text`. `system_fonts` must match the app's
    /// `Application::load_system_fonts`, so measured widths are drawn ones.
    pub fn new(text: &str, theme: EditorTheme, system_fonts: bool) -> DocEditor {
        let mut e = DocEditor {
            buf: Buffer::new(text),
            preview: true,
            theme,
            max_width: 0.0,
            pad: 24.0,
            measure: ShapingMeasure::new(system_fonts),
            ctx: Vec::new(),
            props: Vec::new(),
            layouts: Vec::new(),
            heights: Vec::new(),
            tops: Vec::new(),
            tops_dirty: true,
            shown_active: (usize::MAX, usize::MAX),
            width: 0.0,
            scroll: 0.0,
            motion: ScrollMotion::new(),
            want_x: None,
            dragging: false,
            clicks: None,
            origin: (0.0, 0.0),
            viewport: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            follow_caret: false,
            images: None,
            composition: None,
            ime_seen: 0,
        };
        e.sync();
        e.caret_to_body();
        e
    }

    pub fn set_text(&mut self, text: &str) {
        self.buf.set_text(text);
        self.scroll = 0.0;
        self.motion = ScrollMotion::new();
        self.want_x = None;
        self.sync();
        self.caret_to_body();
    }

    /// Start the caret on the body, past a frontmatter block, so the
    /// Properties table shows rather than its raw YAML.
    fn caret_to_body(&mut self) {
        if let Some(close) = self.props.iter().position(|p| *p == Some(Prop::Close)) {
            self.buf.caret = Pos::new((close + 1).min(self.buf.line_count() - 1), 0);
            self.sync();
        }
    }

    pub fn text(&self) -> String {
        self.buf.text()
    }

    /// Where embedded images come from: `images` answers an embed's link
    /// text with the host's uploaded image, or `None` (not an image, or
    /// not loaded yet — the line shows as text). In live preview a line
    /// that is one embed shows as its image, and with the caret on it as
    /// the raw text with the image below. Asked at layout for the size and
    /// again at paint for the id; call [`DocEditor::invalidate`] when an
    /// image arrives or changes size.
    pub fn set_images(&mut self, images: Box<dyn Fn(&str) -> Option<EmbedImage>>) {
        self.images = Some(images);
        self.invalidate();
    }

    /// Drop every layout (fonts or the theme changed).
    pub fn invalidate(&mut self) {
        self.layouts.fill(None);
        self.tops_dirty = true;
    }

    pub fn set_preview(&mut self, on: bool) {
        if self.preview != on {
            self.preview = on;
            self.invalidate();
        }
    }
}
