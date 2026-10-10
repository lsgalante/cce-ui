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
    pub(crate) bounds: Option<[f32; 4]>,
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
        let mut item_bounds = if let Some([l, t, r, b]) = ti.bounds {
            TextBounds {
                left: ((l * scale_f32).round() as i32).clamp(0, bounds.right),
                top: ((t * scale_f32).round() as i32).clamp(0, bounds.bottom),
                right: ((r * scale_f32).round() as i32).clamp(0, bounds.right),
                bottom: ((b * scale_f32).round() as i32).clamp(0, bounds.bottom),
            }
        } else {
            bounds
        };
        popover_occlusion_clamp(overlays, ti, scale_f32, &mut item_bounds);
        spans.push(TextSpan {
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
        });
    }
    spans
}

/// The popover-occlusion clamp shared by the default [`Application::text_areas`] mapping and
/// the display-list text path: clip a text item's bounds so it does not bleed through an open
/// popover's plate. A text item whose own bounds coincide with a popover rect IS that popover's
/// text and is left alone; anything else that intersects gets clamped horizontally toward
/// whichever side of the popover it starts on.
/// Clamp a text item's bounds away from the registered popover rects it
/// runs under, so page text does not bleed through a floating plate.
///
/// A text item BELONGS to a popover when it carries exactly that popover's
/// rect as its bounds (the convention every popover's own labels follow),
/// and it is then clamped only against the popovers registered AFTER its
/// own — `overlay_rects` is in stacking order, the shared context menu
/// last. Before 2026-09-22 a popover's text was exempt from its own rect
/// alone and clamped against every other, so a context menu opened over a
/// modal dialog had its labels clipped by the dialog it was drawn on top
/// of, and showed as a plate with no legible entries.
pub(super) fn popover_occlusion_clamp(
    overlay_rects: &[(f32, f32, f32, f32)],
    ti: &DlText,
    scale_f32: f32,
    item_bounds: &mut TextBounds,
) {
    let owner = ti.bounds.and_then(|[l, t, r, b]| {
        overlay_rects.iter().position(|&(ox, oy, ow, oh)| {
            (l - ox).abs() < 1.0
                && (t - oy).abs() < 1.0
                && (r - (ox + ow)).abs() < 1.0
                && (b - (oy + oh)).abs() < 1.0
        })
    });
    let first_above = owner.map_or(0, |k| k + 1);
    for &(ox, oy, ow, oh) in &overlay_rects[first_above..] {
        let ol = (ox * scale_f32).round() as i32;
        let ot = (oy * scale_f32).round() as i32;
        let or = ((ox + ow) * scale_f32).round() as i32;
        let ob = ((oy + oh) * scale_f32).round() as i32;

        let tx_pixel = ti.x * scale_f32;
        let ty_pixel = ti.y * scale_f32;

        let mut text_w = 0.0f32;
        let mut run_count = 0;
        for run in ti.buffer.layout_runs() {
            text_w = text_w.max(run.line_w);
            run_count += 1;
        }
        let text_h = run_count as f32 * ti.buffer.metrics().line_height;

        let actual_left = tx_pixel;
        let actual_right = tx_pixel + text_w;
        let actual_top = ty_pixel;
        let actual_bottom = ty_pixel + text_h;

        if actual_left < or as f32
            && actual_right > ol as f32
            && actual_top < ob as f32
            && actual_bottom > ot as f32
        {
            if tx_pixel < ol as f32 {
                item_bounds.right = item_bounds.right.min(ol);
            } else {
                item_bounds.left = item_bounds.left.max(or);
            }
        }
    }
}
