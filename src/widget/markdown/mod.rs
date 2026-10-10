//! `MarkdownView`: a note's blocks (`cce_vault::markdown`) laid out at a
//! width into draw items and click targets — the reading view of
//! cce-notes, the note cards on cce-grid's desktop, and any other surface
//! that shows a vault note. Behind the `markdown` feature, which brings in
//! cce-vault; clients that draw no Markdown do not build it.
//!
//! cce-ui's text prim draws one run in one style, so a paragraph is laid
//! out word by word: each word is measured through the same shaping entry
//! the renderer draws with ([`ShapingMeasure`]), wrapped greedily (a word
//! running across styles — `` `code`, `` — wraps as one), and consecutive
//! words of one style on one line merge back into a single prim. Lay out
//! once per note and width; [`Layout::paint`] only translates and culls,
//! [`Layout::paint_scaled`] also scales.
//!
//! Colours for links, tags, highlights and callouts are written in sRGB
//! and converted where used; bullets are discs (`PaintCtx::circle`), since
//! the squircle corner shape draws a few-px rounded rect as a square.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the theme and colours, what a layout is (`Draw`, `Hit`, `Layout`) and painting it, the `layout` entry points, the layouter's state |
//! | `blocks` | laying out each block: headings, paragraphs, properties, lists and tasks, quotes and callouts, code, tables |
//! | `inline` | laying out a run of spans word by word: measuring, wrapping, styling, inline images, merging runs |

mod blocks;
mod inline;
#[cfg(test)]
mod tests;

use crate::scene::layout::Rect;
use crate::scene::paint::{Cap, PaintCtx, TextAttrs};
pub use cce_vault::markdown::{blocks, Block, Callout, EmbedSize, ListItem, Span, SpanLink, Style};
pub use crate::widget::EmbedImage;

pub use crate::widget::shaping::{Measure, ShapingMeasure};

/// Fonts and the body size the layout works from.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub body_font: String,
    pub mono_font: String,
    pub size: f32,
}

impl Theme {
    fn line_h(&self, size: f32) -> f32 {
        (size * 1.5).ceil()
    }
}

// Colours. Links, tags, the quote bar, the highlight and the code wash read the
// theme (`style.text.*`, through `color::text_link_color` and its kin); FG, DIM
// and the rule are linear constants.
const FG: [f32; 4] = crate::color::TEXT_FG;

const DIM: [f32; 4] = crate::color::TEXT_DIM;

const RULE: [f32; 4] = [1.0, 1.0, 1.0, 0.12];


/// What an inline image's link text is swapped for while a paragraph is
/// tokenized: one non-space char, so the image is one word.
const IMAGE_MARK: &str = "\u{FFFC}";

/// Space an inline image keeps above and below it on its line.
const INLINE_IMAGE_PAD: f32 = 4.0;

/// Indent of a list level, and of a quote's body past its bar.
const INDENT: f32 = 24.0;

const BLOCK_GAP: f32 = 10.0;

const CODE_PAD: f32 = 10.0;

#[derive(Clone, Debug, PartialEq)]
pub enum Hit {
    Link(SpanLink),
    /// A task's checkbox: its source line and current status.
    Task { line: usize, status: char },
}

#[derive(Clone, Debug)]
pub enum Draw {
    Text { text: String, x: f32, y: f32, size: f32, color: [f32; 4], font: String, attrs: TextAttrs },
    Quad { rect: Rect, color: [f32; 4] },
    Round { rect: Rect, radius: f32, color: [f32; 4] },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: [f32; 4] },
    Check { cx: f32, cy: f32, r: f32, checked: bool },
    /// A filled disc: a list bullet. Not a `Round` — at a few px the
    /// squircle corner shape draws a small radius as a square.
    Dot { cx: f32, cy: f32, r: f32, color: [f32; 4] },
    /// An embedded image, by its link text; its id is asked for at paint.
    Image { target: String, rect: Rect },
    /// A bundled cce-icons glyph, tinted `color` (sRGB, as a text colour).
    Icon { name: &'static str, rect: Rect, color: [f32; 4] },
}

impl Draw {
    fn top_bottom(&self) -> (f32, f32) {
        match self {
            Draw::Text { y, size, .. } => (*y, y + size * 1.3),
            Draw::Quad { rect, .. } | Draw::Round { rect, .. } | Draw::Image { rect, .. } | Draw::Icon { rect, .. } => (rect.y, rect.y + rect.height),
            Draw::Line { y1, y2, width, .. } => (y1.min(*y2) - width, y1.max(*y2) + width),
            Draw::Check { cy, r, .. } | Draw::Dot { cy, r, .. } => (cy - r, cy + r),
        }
    }
}

#[derive(Default, Debug)]
pub struct Layout {
    pub draws: Vec<Draw>,
    pub hits: Vec<(Rect, Hit)>,
    /// Each block's (and list item's) first source line and the y it
    /// starts at, in source order.
    pub lines: Vec<(usize, f32)>,
    pub height: f32,
}

impl Layout {
    /// The y of the last block starting at or before source `line`.
    pub fn y_of_line(&self, line: usize) -> f32 {
        self.lines.iter().take_while(|(l, _)| *l <= line).last().map(|(_, y)| *y).unwrap_or(0.0)
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<&Hit> {
        self.hits
            .iter()
            .find(|(r, _)| x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height)
            .map(|(_, h)| h)
    }

    /// Paint at `origin` scrolled by `scroll`, culled to `viewport`.
    pub fn paint(&self, pc: &mut PaintCtx, origin: (f32, f32), scroll: f32, viewport: Rect) {
        self.paint_scaled(pc, (origin.0, origin.1 - scroll), 1.0, viewport);
    }

    /// Paint with every length multiplied by `k`, the layout's top-left at
    /// `origin` — for a host that lays out in its own units (cce-grid's
    /// virtual desktop) and draws at another resolution. Culled to
    /// `viewport`, in the painted space.
    pub fn paint_scaled(&self, pc: &mut PaintCtx, origin: (f32, f32), k: f32, viewport: Rect) {
        self.paint_scaled_with(pc, origin, k, viewport, &|_| None);
    }

    /// [`Layout::paint`], drawing embedded images through `image` — the
    /// same lookup the layout was made with ([`layout_with`]).
    pub fn paint_with(&self, pc: &mut PaintCtx, origin: (f32, f32), scroll: f32, viewport: Rect, image: &dyn Fn(&str) -> Option<EmbedImage>) {
        self.paint_scaled_with(pc, (origin.0, origin.1 - scroll), 1.0, viewport, image);
    }

    /// [`Layout::paint_scaled`] with embedded images. An image the host no
    /// longer has (gone since layout) leaves a faint placeholder.
    pub fn paint_scaled_with(&self, pc: &mut PaintCtx, origin: (f32, f32), k: f32, viewport: Rect, image: &dyn Fn(&str) -> Option<EmbedImage>) {
        let (ox, oy) = origin;
        let (top, bottom) = ((viewport.y - oy) / k, (viewport.y + viewport.height - oy) / k);
        let at = |x: f32, y: f32| (ox + x * k, oy + y * k);
        let rect = |r: &Rect| {
            let (x, y) = at(r.x, r.y);
            Rect { x, y, width: r.width * k, height: r.height * k }
        };
        for d in &self.draws {
            let (t, b) = d.top_bottom();
            if b < top || t > bottom {
                continue;
            }
            match d {
                Draw::Text { text, x, y, size, color, font, attrs } => {
                    let (x, y) = at(*x, *y);
                    pc.text_attrs(text.clone(), x, y, size * k, srgb_u8(*color), Some(font.clone()), None, *attrs)
                }
                Draw::Quad { rect: r, color } => pc.quad(rect(r), *color),
                Draw::Round { rect: r, radius, color } => {
                    pc.rounded_rect(rect(r), radius * k, (true, true, true, true), *color)
                }
                Draw::Line { x1, y1, x2, y2, width, color } => {
                    let ((ax, ay), (bx, by)) = (at(*x1, *y1), at(*x2, *y2));
                    pc.vector(ax, ay, bx, by, (width * k).max(0.5), *color, Cap::Flat)
                }
                Draw::Check { cx, cy, r, checked } => {
                    let (x, y) = at(*cx, *cy);
                    crate::widget::Checkbox::paint_inline(pc, x, y, r * k, *checked)
                }
                Draw::Dot { cx, cy, r, color } => {
                    let (x, y) = at(*cx, *cy);
                    pc.circle(x, y, r * k, *color)
                }
                Draw::Image { target, rect: r } => match image(target) {
                    Some(img) => pc.image(img.id, rect(r), 1.0),
                    None => pc.rounded_rect(rect(r), 4.0 * k, (true, true, true, true), crate::color::text_code_background_color()),
                },
                Draw::Icon { name, rect: r, color } => {
                    pc.icon(name, rect(r), *color);
                }
            }
        }
    }
}

/// An sRGB colour (alpha untouched) in the linear space prims take.
fn lin(srgb: [f32; 4]) -> [f32; 4] {
    crate::color::to_linear(srgb)
}

pub fn srgb_u8(linear: [f32; 4]) -> [u8; 3] {
    let s = crate::color::to_srgb(linear);
    [(s[0] * 255.0) as u8, (s[1] * 255.0) as u8, (s[2] * 255.0) as u8]
}

/// Lay out `blocks` at `width`. `resolved` says whether a note link has a
/// target, so unresolved ones draw faded as in Obsidian.
pub fn layout(
    blocks: &[Block],
    width: f32,
    theme: &Theme,
    m: &mut dyn Measure,
    resolved: &dyn Fn(&SpanLink) -> bool,
) -> Layout {
    layout_with(blocks, width, theme, m, resolved, &|_| None)
}

/// [`layout`], with `image` answering for an embed's link text: an image
/// it has draws in place of the embed's link, sized by [`EmbedImage::fit`]
/// (Obsidian's `|300` / `|300x200` honoured). One it does not have — not
/// an image, or still loading — keeps the link; lay out again when it
/// arrives.
pub fn layout_with(
    blocks: &[Block],
    width: f32,
    theme: &Theme,
    m: &mut dyn Measure,
    resolved: &dyn Fn(&SpanLink) -> bool,
    image: &dyn Fn(&str) -> Option<EmbedImage>,
) -> Layout {
    let mut l = Layouter { theme, m, resolved, image, out: Layout::default(), pending: Vec::new(), pending_images: Vec::new() };
    let y = l.blocks(blocks, 0.0, width.max(80.0), 0.0, true);
    l.out.height = y;
    l.out
}

struct Layouter<'a> {
    theme: &'a Theme,
    m: &'a mut dyn Measure,
    resolved: &'a dyn Fn(&SpanLink) -> bool,
    image: &'a dyn Fn(&str) -> Option<EmbedImage>,
    out: Layout,
    /// Runs finished on the current line, emitted when the line ends.
    pending: Vec<Run>,
    /// Inline images placed on the current line: (link text, x, w, h).
    pending_images: Vec<(String, f32, f32, f32)>,
}

/// One inline run's look, before it is placed.
#[derive(Clone, Copy)]
struct RunStyle {
    size: f32,
    bold: bool,
    color: Option<[f32; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
struct Look {
    size: f32,
    font: String,
    attrs: TextAttrs,
    color: [f32; 4],
    bg: Option<[f32; 4]>,
    strike: bool,
}

struct Run {
    text: String,
    x: f32,
    w: f32,
    look: Look,
    link: Option<SpanLink>,
}
