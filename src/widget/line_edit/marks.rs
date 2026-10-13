//! A [`LineEdit`] measured as it is drawn, and drawn: what an app that keeps one and paints
//! it itself needs — the caret's x, the byte under the pointer, the run's width, the scroll
//! that keeps the caret in view ([`LineMarks`], [`keep_caret`]) — and, if it wants them, the
//! text, selection, composition and caret painted in one call ([`paint_line`]). The well the
//! text sits in is the app's own (`PaintCtx::field`, a recess); these draw what is in it.
//!
//! Every x is read off the run shaped exactly as the renderer draws it, through the shared
//! buffer cache `shaped_cluster_offsets` reads too, and right in text of either direction
//! (`text::ShapedRun`). Never place a caret with `measure_text_width`: its inked extent
//! disagrees with the drawn advances, and the caret drifts off the glyphs a little more
//! with every character.

use super::LineEdit;
use cosmic_text::FontSystem;
use crate::scene::layout::Rect;
use crate::scene::paint::{PaintCtx, TextAttrs};
use crate::text::ShapedRun;

/// The caret's width, a `TextBox`'s.
pub const CARET_W: f32 = 1.5;

/// What a [`LineEdit`] draws and where its marks fall on it, in logical px from the text's
/// origin.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LineMarks {
    /// What is drawn: [`LineEdit::display`] — the text with an input method's composition
    /// at the caret, bullets for a masked one. Every byte offset a `LineMarks` takes or
    /// gives is into this, not the edit's text.
    pub shown: String,
    /// `shown` as shaped: the caret's place at every char boundary, the clusters and the
    /// width.
    pub run: ShapedRun,
    /// The caret's x.
    pub caret: f32,
    /// The selection's spans, left to right: one in text of one direction, two where it
    /// crosses a change of direction, none with nothing selected.
    pub selection: Vec<(f32, f32)>,
    /// The spans of an input method's composition, to underline.
    pub composition: Vec<(f32, f32)>,
}

impl LineMarks {
    /// Measure what `edit` shows as it will be drawn: at `size` in `family` (`None` for the
    /// default face; a configured string with a size, `"Berkeley Mono 14"`, wears its own),
    /// through `fs` — the font system the text is drawn with: the app's renderer's, or
    /// `geometry_font_system()` for an app on the cce-ui runner. An edit with the keyboard
    /// calls [`LineEdit::sync_ime`] first, so the composition is the current one.
    pub fn of(fs: &mut FontSystem, edit: &LineEdit, size: f32, family: Option<&str>) -> LineMarks {
        let shown = edit.display();
        let scale = crate::scale::scale_factor();
        let buffer = crate::text::shared_text_buffer_at(fs, &shown, size, family, TextAttrs::default(), scale);
        let run = crate::text::shaped_run(&buffer, &shown, scale);
        let caret = run.x_of(edit.display_index(edit.cursor));
        let selection = edit
            .selection
            .filter(|&(a, b)| a < b)
            .map_or_else(Vec::new, |(a, b)| run.spans(edit.display_index(a), edit.display_index(b)));
        let composition = edit.composition_range().map_or_else(Vec::new, |(a, b)| run.spans(a, b));
        LineMarks { shown, run, caret, selection, composition }
    }

    /// How wide the text is drawn.
    pub fn width(&self) -> f32 {
        self.run.width
    }

    /// The x of byte `byte` of [`shown`](Self::shown): the caret's place before it (a byte
    /// inside a character or a cluster takes its start's).
    pub fn x_of(&self, byte: usize) -> f32 {
        self.run.x_of(byte)
    }

    /// The char boundary of [`shown`](Self::shown) nearest `x`: a click's caret.
    pub fn byte_at(&self, x: f32) -> usize {
        self.run.index_at(x)
    }

    /// The boundary of `edit`'s TEXT nearest `x` — what [`LineEdit::press`] and
    /// [`LineEdit::drag_to`] take: [`byte_at`](Self::byte_at) carried back across a
    /// composition or a mask ([`LineEdit::text_index`]). `edit` is the one these marks were
    /// measured from.
    pub fn index_at(&self, edit: &LineEdit, x: f32) -> usize {
        edit.text_index(self.byte_at(x))
    }

    /// The scroll that keeps the caret inside a text area `avail` wide — see [`keep_caret`].
    pub fn keep_caret(&self, shift: f32, avail: f32) -> f32 {
        keep_caret(shift, self.caret, self.width(), avail)
    }
}

/// The scroll that keeps `caret` inside a text area `avail` wide, starting from `shift`:
/// unchanged while the caret is in view, else just enough to bring it back; never past the
/// text's end (`text_w`), never negative. The app draws the text at the area's left less
/// the scroll, and keeps the scroll from frame to frame.
pub fn keep_caret(shift: f32, caret: f32, text_w: f32, avail: f32) -> f32 {
    let avail = avail.max(1.0);
    let mut s = shift;
    if caret - s > avail {
        s = caret - avail;
    }
    if caret < s {
        s = caret;
    }
    s.min((text_w - avail).max(0.0)).max(0.0)
}

/// How [`paint_line`] draws: the face the marks were measured in, and the colours.
#[derive(Debug, Clone, PartialEq)]
pub struct LineLook {
    pub size: f32,
    pub family: Option<String>,
    pub text: [u8; 3],
    pub placeholder: [u8; 3],
    /// The selection's fill.
    pub selection: [f32; 4],
    /// The caret, and the composition's underline.
    pub caret: [f32; 4],
    /// The bounds the text carries (`[left, top, right, bottom]`), besides the area's clip.
    /// A line painted inside an app's own occluding popover or dialog sets them to that
    /// popover's rect: the engine's text clamp keeps page text out from under an open
    /// popover, and it is the text's own bounds that say it belongs to this one. `None` (the
    /// default) is page text.
    pub bounds: Option<[f32; 4]>,
}

impl LineLook {
    /// A `TextBox`'s look at `size` in `family`: its text and placeholder colours, its
    /// selection fill and its caret; no bounds of its own.
    pub fn new(size: f32, family: Option<String>) -> LineLook {
        LineLook {
            size,
            family,
            text: crate::icon_tint(crate::color::TEXT_FG),
            placeholder: crate::icon_tint(crate::color::TEXT_DIM),
            selection: [0.20, 0.50, 0.85, 0.3],
            caret: crate::color::TEXT_FG,
            bounds: None,
        }
    }
}

/// Draw a [`LineEdit`]'s text and marks inside `area` — the text area of the app's well,
/// which clips them — the text starting at `origin_x`, the area's left less its scroll
/// ([`LineMarks::keep_caret`]): the selection under the text, `placeholder` in its dim
/// colour in place of empty text, an input method's composition underlined, and the caret
/// when `caret` is set. The text is centred in the area's height, the selection and the
/// caret a little taller than it, as a `TextBox`'s are. The text carries `look.bounds`
/// (see [`LineLook::bounds`]) as well as the area's clip.
///
/// Returns the caret's rect, kept inside the area, whether or not it was drawn: the edit
/// with the keyboard claims it (`text_input::claim`), which is what places an input
/// method's candidates and tells the shell text is wanted.
pub fn paint_line(pc: &mut PaintCtx, area: Rect, origin_x: f32, marks: &LineMarks, look: &LineLook, placeholder: &str, caret: bool) -> Rect {
    let size = look.size;
    let ty = crate::layout::center_text_y(area.y, area.height, size);
    let mark_h = (size * 1.15).min(area.height);
    let mark_y = ty + (crate::layout::line_height(size) - mark_h) / 2.0;
    let x = (origin_x + marks.caret).clamp(area.x, (area.x + area.width - CARET_W).max(area.x));
    let caret_rect = Rect { x, y: mark_y, width: CARET_W, height: mark_h };
    pc.clip(area, |pc| {
        for &(a, b) in &marks.selection {
            pc.quad(Rect { x: origin_x + a, y: mark_y, width: b - a, height: mark_h }, look.selection);
        }
        if !marks.shown.is_empty() {
            pc.text_with(marks.shown.clone(), origin_x, ty, size, look.text, look.family.clone(), look.bounds);
        } else if !placeholder.is_empty() {
            pc.text_with(placeholder.to_string(), origin_x, ty, size, look.placeholder, look.family.clone(), look.bounds);
        }
        for &(a, b) in &marks.composition {
            pc.quad(Rect { x: origin_x + a, y: ty + size + 1.0, width: b - a, height: 1.5 }, look.caret);
        }
        if caret {
            pc.quad(caret_rect, look.caret);
        }
    });
    caret_rect
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ime::{self, Preedit};
    use crate::scene::paint::Prim;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    fn close_spans(got: &[(f32, f32)], want: &[(f32, f32)]) -> bool {
        got.len() == want.len() && got.iter().zip(want).all(|(g, w)| close(g.0, w.0) && close(g.1, w.1))
    }

    fn marks(edit: &LineEdit) -> LineMarks {
        let mut fs = crate::geometry_font_system().lock().unwrap();
        LineMarks::of(&mut fs, edit, 14.0, None)
    }

    /// In text of one-, two- and three-byte characters the marks stand on char boundaries:
    /// x rises boundary by boundary from 0 to the width, a byte inside a character takes its
    /// start's x, the boundary nearest a point is found from its x and past either end, and
    /// the caret and the selection are the boundaries' xs.
    #[test]
    fn marks_stand_on_char_boundaries_in_multibyte_text() {
        let text = "aé日b"; // a, é (2 bytes), 日 (3 bytes), b
        let mut e = LineEdit::with_text(text);
        e.selection = Some((1, 6));
        let m = marks(&e);
        assert_eq!(m.shown, text);
        let bounds = [0, 1, 3, 6, 7];
        let xs: Vec<f32> = bounds.iter().map(|&b| m.x_of(b)).collect();
        assert_eq!(xs[0], 0.0);
        assert!(xs.windows(2).all(|w| w[1] > w[0]), "x rises across every character: {xs:?}");
        assert!(close(xs[4], m.width()), "the end is the width");
        assert_eq!(m.x_of(2), m.x_of(1), "inside é is é's start");
        assert_eq!((m.x_of(4), m.x_of(5)), (m.x_of(3), m.x_of(3)), "inside 日 is 日's start");
        for &b in &bounds {
            assert_eq!(m.byte_at(m.x_of(b)), b);
        }
        let near_end_of_ri = m.x_of(6) - 0.2 * (m.x_of(6) - m.x_of(3));
        assert_eq!(m.byte_at(near_end_of_ri), 6, "the nearer boundary");
        assert_eq!((m.byte_at(-5.0), m.byte_at(m.width() + 5.0)), (0, 7), "past either end");
        assert_eq!(m.caret, m.width(), "the caret after the text");
        assert!(close_spans(&m.selection, &[(m.x_of(1), m.x_of(6))]), "é and 日: {:?}", m.selection);
        assert!(m.composition.is_empty());
    }

    /// A masked edit is measured as its bullets, and a point on them is carried back to the
    /// text's own boundary: a bullet is three bytes whatever character it hides.
    #[test]
    fn a_masked_edit_is_measured_as_drawn_and_hit_back_into_its_text() {
        let mut e = LineEdit::masked();
        e.text = "é本".to_string(); // boundaries 0, 2, 5
        e.cursor = 2;
        let m = marks(&e);
        assert_eq!(m.shown, "\u{2022}\u{2022}");
        assert_eq!(m.caret, m.x_of(3), "the caret after the first bullet");
        assert_eq!(m.index_at(&e, m.x_of(3)), 2, "after é in the text");
        assert_eq!(m.index_at(&e, m.width() + 1.0), 5);
    }

    /// An input method's composition is measured where it is drawn — spliced in at the
    /// caret — and spans what it covers; the caret stands where the input method has it.
    #[test]
    fn a_composition_is_measured_where_it_is_drawn() {
        let mut e = LineEdit::with_text("ab");
        e.cursor = 1;
        let _ = ime::take_reset();
        ime::set_preedit(Some(Preedit::new("にほ", Some((3, 3)))));
        e.sync_ime();
        let m = marks(&e);
        ime::set_preedit(None);
        assert_eq!(m.shown, "aにほb");
        assert!(close_spans(&m.composition, &[(m.x_of(1), m.x_of(7))]), "{:?}", m.composition);
        assert_eq!(m.caret, m.x_of(4), "after に");
        assert_eq!(m.index_at(&e, m.x_of(5)), 1, "inside the composition is the caret");
        assert_eq!(m.index_at(&e, m.width()), 2);
    }

    #[test]
    fn the_caret_stays_in_view() {
        assert_eq!(keep_caret(0.0, 50.0, 80.0, 100.0), 0.0, "in view: no scroll");
        assert_eq!(keep_caret(0.0, 150.0, 200.0, 100.0), 50.0, "past the right edge: just enough");
        assert_eq!(keep_caret(50.0, 20.0, 200.0, 100.0), 20.0, "left of the view: back to it");
        assert_eq!(keep_caret(80.0, 60.0, 60.0, 100.0), 0.0, "the text shrank: never past its end");
        let m = LineMarks { caret: 150.0, run: ShapedRun { width: 200.0, ..ShapedRun::default() }, ..LineMarks::default() };
        assert_eq!(m.keep_caret(0.0, 100.0), 50.0);
    }

    /// The selection is painted under the text, the text in the look's face, and the caret
    /// on request, all clipped to the area, and the caret is returned inside it; empty text
    /// shows the placeholder in the dim colour instead.
    #[test]
    fn a_line_paints_its_text_and_marks_and_returns_the_caret() {
        let area = Rect { x: 10.0, y: 20.0, width: 80.0, height: 24.0 };
        let look = LineLook::new(14.0, Some("Sans".to_string()));
        let mut e = LineEdit::with_text("日本語");
        e.selection = Some((0, 3));
        e.cursor = 3;
        let m = marks(&e);
        let mut pc = PaintCtx::new();
        let caret = paint_line(&mut pc, area, area.x + 4.0, &m, &look, "Search", true);
        let items = pc.finish().items;
        assert!(items.iter().all(|i| i.clip == Some(area)), "everything is cut at the area");
        let quads: Vec<Rect> = items.iter().filter_map(|i| match i.prim {
            Prim::Quad { rect, .. } => Some(rect),
            _ => None,
        }).collect();
        assert_eq!(quads.len(), 2, "the selection and the caret");
        assert!(close(quads[0].x, area.x + 4.0) && close(quads[0].width, m.x_of(3)), "the selection under 日: {:?}", quads[0]);
        assert_eq!(quads[1], caret);
        assert_eq!(caret.x, area.x + 4.0 + m.caret);
        assert!(caret.y >= area.y && caret.y + caret.height <= area.y + area.height);
        let text = items.iter().find_map(|i| match &i.prim {
            Prim::Text { text, color, font, .. } => Some((text.clone(), *color, font.clone())),
            _ => None,
        });
        assert_eq!(text, Some(("日本語".to_string(), look.text, look.family.clone())));

        // Empty: the placeholder, dim; no caret asked for, none drawn — but its rect, kept
        // inside the area even when the origin is scrolled off it.
        let m = marks(&LineEdit::default());
        let mut pc = PaintCtx::new();
        let caret = paint_line(&mut pc, area, area.x - 30.0, &m, &look, "Search", false);
        let items = pc.finish().items;
        assert!(matches!(&items[..], [i] if matches!(&i.prim, Prim::Text { text, color, .. } if text == "Search" && *color == look.placeholder)));
        assert_eq!(caret.x, area.x);
    }
}
