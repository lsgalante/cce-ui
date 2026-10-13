//! The display list's text: its prims gathered and shaped for the glyph pass, and the clamp that
//! keeps text under a popover from drawing through it.

use super::*;

/// A text item's clip rect in physical pixels. This was `glyphon::TextBounds` — the one
/// glyphon-owned type cce-ui ever used, everything else being a cosmic-text re-export — so
/// it is defined here now that the dependency is cosmic-text directly. Same plain
/// four-`i32` layout; it is only an intermediate on the way to `TextSpan::bounds`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// A display-list text prim ready for the glyph pass: a [`TextItem`](crate::widget::TextItem) whose
/// buffer is the cache's own, shared rather than copied. Public so a shell
/// outside the crate can hold what [`build_frame`](crate::backend::frame::build_frame)
/// fills; its fields are the frame's own.
pub struct DlText {
    pub(crate) buffer: Rc<Buffer>,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) color: cosmic_text::Color,
    /// Where the glyphs may draw: the paint walk's clip ∩ the prim's own bounds.
    pub(crate) bounds: Option<[f32; 4]>,
    /// The prim's own bounds, before the clip is merged in: what says a text belongs to a
    /// popover ([`popover_occlusion_clamp`]).
    pub(crate) own_bounds: Option<[f32; 4]>,
    pub(crate) clip_circle: Option<[f32; 3]>,
    pub(crate) clip_rrect: Option<[f32; 5]>,
}

/// The display list's Text prims, shaped through the shared buffer cache and
/// held for the glyph pass (the [`TextSpan`]s built by [`dl_text_spans`] borrow
/// these). Clip = the paint walk's item clip ∩ the prim's own bounds, in
/// logical space. Shared by the window's frame and the context-menu popup's.
pub(crate) fn collect_dl_text(fs: &mut FontSystem, dl: &crate::scene::paint::DisplayList, out: &mut Vec<DlText>) {
    // A cached buffer may have been shaped by another `FontSystem` (the app's
    // measuring one); the glyph pass rasterizes its face-alias IDs with this one.
    sync_font_ops(fs);
    for item in &dl.items {
        if let crate::scene::paint::Prim::Text { text, x, y, font_size, color, alpha, font, bounds, attrs, layout } = &item.prim {
            let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
            let merged = match (clip, *bounds) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            // Boxed text (wrap/align) is laid out in its box and shifts down by the
            // vertical offset; ordinary labels are a single run. Both cached.
            let (buffer, y_off) = match layout {
                Some(l) => shared_laid_out_buffer(fs, text, *font_size, font.as_deref(), *attrs, *l),
                None => (shared_text_buffer(fs, text, *font_size, font.as_deref(), *attrs), 0.0),
            };
            out.push(DlText {
                buffer,
                x: *x,
                y: *y + y_off,
                color: cosmic_text::Color::rgba(
                    color[0],
                    color[1],
                    color[2],
                    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                ),
                bounds: merged,
                own_bounds: *bounds,
                clip_circle: item.clip_circle,
                clip_rrect: item.clip_rrect,
            });
        }
    }
}

/// The glyph pass's spans for `items`: each clamped to the surface and its
/// own bounds, then by the popover-occlusion clamp against `overlays`.
pub(crate) fn dl_text_spans<'a>(
    items: &'a [DlText],
    scale_f32: f32,
    bounds: TextBounds,
    overlays: &[(f32, f32, f32, f32)],
) -> Vec<TextSpan<'a>> {
    let mut spans: Vec<TextSpan<'a>> = Vec::new();
    for ti in items {
        let item_bounds = if let Some([l, t, r, b]) = ti.bounds {
            TextBounds {
                left: ((l * scale_f32).round() as i32).clamp(0, bounds.right),
                top: ((t * scale_f32).round() as i32).clamp(0, bounds.bottom),
                right: ((r * scale_f32).round() as i32).clamp(0, bounds.right),
                bottom: ((b * scale_f32).round() as i32).clamp(0, bounds.bottom),
            }
        } else {
            bounds
        };
        for piece in popover_occlusion_clamp(overlays, ti, scale_f32, item_bounds) {
            spans.push(dl_text_span(ti, scale_f32, piece));
        }
    }
    spans
}

/// One glyph-pass span of `ti`, cut at `item_bounds` (physical px).
fn dl_text_span(ti: &DlText, scale_f32: f32, item_bounds: TextBounds) -> TextSpan<'_> {
    TextSpan {
        buffer: &ti.buffer,
        left: (ti.x * scale_f32).round(),
        top: (ti.y * scale_f32).round(),
        // Buffers are shaped at physical size (get_text_buffer_attrs).
        scale: 1.0,
        bounds: Some([
            item_bounds.left,
            item_bounds.top,
            item_bounds.right,
            item_bounds.bottom,
        ]),
        default_color: [
            ti.color.r() as f32 / 255.0,
            ti.color.g() as f32 / 255.0,
            ti.color.b() as f32 / 255.0,
            ti.color.a() as f32 / 255.0,
        ],
        rotation: None,
        // Circle wins when both are set (the circular pane's innermost clip);
        // otherwise a rounded-rect clip rides as center+radius with extents.
        clip_circle: match (ti.clip_circle, ti.clip_rrect) {
            (Some(c), _) => [c[0] * scale_f32, c[1] * scale_f32, c[2] * scale_f32],
            (None, Some(rr)) => [rr[0] * scale_f32, rr[1] * scale_f32, rr[4] * scale_f32],
            (None, None) => [0.0; 3],
        },
        clip_extents: match (ti.clip_circle, ti.clip_rrect) {
            (None, Some(rr)) => [rr[2] * scale_f32, rr[3] * scale_f32],
            _ => [0.0; 2],
        },
    }
}

/// Cut a text item's bounds around the registered popover rects it runs
/// under, so page text does not bleed through a floating plate: the pieces of
/// `item_bounds` (physical px) that lie outside every covering popover, one
/// glyph-pass span each. Text drawn after all geometry cannot be covered by a
/// plate, so this is what hides it.
///
/// Only the popover's own rect is taken out. Before 2026-10-13 the item's
/// bounds were narrowed to one side of the popover, so a paragraph the list
/// overlapped lost everything right of the list's left edge, on every one of
/// its lines, not just the ones the list covered.
///
/// A text item BELONGS to a popover when it carries exactly that popover's
/// rect as its bounds (the convention every popover's own labels follow),
/// and it is then clamped only against the popovers registered AFTER its
/// own — `overlay_rects` is in stacking order, the shared context menu
/// last. Before 2026-09-22 a popover's text was exempt from its own rect
/// alone and clamped against every other, so a context menu opened over a
/// modal dialog had its labels clipped by the dialog it was drawn on top
/// of, and showed as a plate with no legible entries.
///
/// The prim's OWN bounds say whose text it is, before a clip narrows them:
/// a line of text inside a popover, clipped to its own well (`paint_line`
/// with `LineLook::bounds`), still belongs to the popover. Text with no
/// bounds of its own that a clip of exactly the popover's rect bounds
/// belongs to it as well.
pub(super) fn popover_occlusion_clamp(
    overlay_rects: &[(f32, f32, f32, f32)],
    ti: &DlText,
    scale_f32: f32,
    item_bounds: TextBounds,
) -> Vec<TextBounds> {
    let popover_of = |[l, t, r, b]: [f32; 4]| {
        overlay_rects.iter().position(|&(ox, oy, ow, oh)| {
            (l - ox).abs() < 1.0
                && (t - oy).abs() < 1.0
                && (r - (ox + ow)).abs() < 1.0
                && (b - (oy + oh)).abs() < 1.0
        })
    };
    let owner = ti.own_bounds.and_then(popover_of).or_else(|| ti.bounds.and_then(popover_of));
    let first_above = owner.map_or(0, |k| k + 1);
    let mut pieces = vec![item_bounds];
    if overlay_rects.len() <= first_above {
        return pieces;
    }

    // The text's own extent: a popover it does not reach leaves it whole,
    // however far its bounds run (unbounded text has the whole surface).
    let mut text_w = 0.0f32;
    let mut run_count = 0;
    for run in ti.buffer.layout_runs() {
        text_w = text_w.max(run.line_w);
        run_count += 1;
    }
    let text_h = run_count as f32 * ti.buffer.metrics().line_height;
    let (tl, tt) = (ti.x * scale_f32, ti.y * scale_f32);
    let (tr, tb) = (tl + text_w, tt + text_h);

    for &(ox, oy, ow, oh) in &overlay_rects[first_above..] {
        let ol = (ox * scale_f32).round() as i32;
        let ot = (oy * scale_f32).round() as i32;
        let or = ((ox + ow) * scale_f32).round() as i32;
        let ob = ((oy + oh) * scale_f32).round() as i32;
        if !(tl < or as f32 && tr > ol as f32 && tt < ob as f32 && tb > ot as f32) {
            continue;
        }
        pieces = pieces
            .into_iter()
            .flat_map(|p| subtract(p, TextBounds { left: ol, top: ot, right: or, bottom: ob }))
            .collect();
    }
    pieces
}

/// `a` less `b`: at most four disjoint rects (the full-width bands above and
/// below `b`, the strips left and right of it), empty ones left out.
fn subtract(a: TextBounds, b: TextBounds) -> Vec<TextBounds> {
    let empty = |r: &TextBounds| r.left >= r.right || r.top >= r.bottom;
    if empty(&a) {
        return Vec::new();
    }
    if a.left >= b.right || a.right <= b.left || a.top >= b.bottom || a.bottom <= b.top {
        return vec![a];
    }
    let (mid_top, mid_bottom) = (a.top.max(b.top), a.bottom.min(b.bottom));
    [
        TextBounds { bottom: b.top.min(a.bottom), ..a },
        TextBounds { top: b.bottom.max(a.top), ..a },
        TextBounds { right: b.left.min(a.right), top: mid_top, bottom: mid_bottom, ..a },
        TextBounds { left: b.right.max(a.left), top: mid_top, bottom: mid_bottom, ..a },
    ]
    .into_iter()
    .filter(|r| !empty(r))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::layout::Rect;
    use crate::scene::paint::{DisplayList, PaintCtx};
    use crate::widget::line_edit::paint_line;
    use crate::widget::{LineEdit, LineLook, LineMarks};

    const SURFACE: TextBounds = TextBounds { left: 0, top: 0, right: 400, bottom: 300 };
    /// An app's own occluding popover (a dialog), and the well of a line inside it.
    const POPOVER: (f32, f32, f32, f32) = (100.0, 50.0, 200.0, 120.0);
    const AREA: Rect = Rect { x: 120.0, y: 80.0, width: 100.0, height: 24.0 };

    /// The glyph pass's bounds for each text of `dl`, at scale 1, with `POPOVER` open.
    fn span_bounds(dl: &DisplayList) -> Vec<[i32; 4]> {
        let mut fs = crate::geometry_font_system().lock().unwrap();
        let mut items = Vec::new();
        collect_dl_text(&mut fs, dl, &mut items);
        dl_text_spans(&items, 1.0, SURFACE, &[POPOVER]).iter().map(|s| s.bounds.unwrap()).collect()
    }

    fn line(bounds: Option<[f32; 4]>) -> DisplayList {
        let marks = {
            let mut fs = crate::geometry_font_system().lock().unwrap();
            LineMarks::of(&mut fs, &LineEdit::with_text("query"), 14.0, None)
        };
        let look = LineLook { bounds, ..LineLook::new(14.0, None) };
        let mut pc = PaintCtx::new();
        paint_line(&mut pc, AREA, AREA.x, &marks, &look, "", true);
        pc.finish()
    }

    /// A line inside a popover, clipped to its own well, keeps its text when it carries the
    /// popover's rect as `LineLook::bounds`: the clamp reads the prim's own bounds, before
    /// the clip narrows them to the well, and the glyphs stay cut at the well. Without them
    /// it is page text, which the popover covers.
    #[test]
    fn a_line_inside_its_popover_keeps_its_text_when_it_says_so() {
        let (x, y, w, h) = POPOVER;
        let own = line(Some([x, y, x + w, y + h]));
        let well = [AREA.x as i32, AREA.y as i32, (AREA.x + AREA.width) as i32, (AREA.y + AREA.height) as i32];
        assert_eq!(span_bounds(&own), vec![well], "the popover's own text, cut at its well");

        let page = span_bounds(&line(None));
        assert!(page.is_empty(), "page text wholly under the popover is clamped away: {page:?}");
    }

    /// Page text the popover only partly covers keeps everything outside the popover's rect:
    /// the lines above and below it whole, and on the covered lines what runs left and right
    /// of it. It used to be cut at the popover's left edge on every line.
    #[test]
    fn page_text_keeps_what_the_popover_does_not_cover() {
        let (x, y, w, h) = POPOVER;
        let para = "a long paragraph line that runs under the popover and out the other side\n".repeat(12);
        let mut pc = PaintCtx::new();
        pc.text_with(para, 10.0, 10.0, 14.0, [255; 3], None, None);
        let (l, t, r, b) = (x as i32, y as i32, (x + w) as i32, (y + h) as i32);
        let s = SURFACE;
        assert_eq!(
            span_bounds(&pc.finish()),
            vec![
                [s.left, s.top, s.right, t],
                [s.left, b, s.right, s.bottom],
                [s.left, t, l, b],
                [r, t, s.right, b],
            ]
        );
    }

    /// A label whose bounds are the popover's rect and that no clip narrows is the popover's,
    /// as before; so is one with no bounds that a clip of exactly the rect bounds.
    #[test]
    fn a_popovers_label_is_known_by_its_bounds_or_an_exact_clip() {
        let (x, y, w, h) = POPOVER;
        let rect = Rect { x, y, width: w, height: h };
        let mut pc = PaintCtx::new();
        pc.text_with("Open".to_string(), 120.0, 60.0, 14.0, [255; 3], None, Some([x, y, x + w, y + h]));
        pc.clip(rect, |pc| pc.text_with("Save".to_string(), 120.0, 90.0, 14.0, [255; 3], None, None));
        let all = [x as i32, y as i32, (x + w) as i32, (y + h) as i32];
        assert_eq!(span_bounds(&pc.finish()), vec![all, all]);
    }
}
