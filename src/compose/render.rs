//! Placing a widget in a host's block ([`place`] / [`place_h`]), and replaying it onto a
//! flat-host [`RenderTarget`] ([`render_widget`] / [`render_widget_h`], [`render_popovers`]).
//! The target and its carve types are `scene::paint`'s. Legacy: new code places with [`place`]
//! and paints through `scene::paint::PaintCtx` (`widget::painter::paint_root_into`).

use crate::context::UiContext;
use crate::scene::paint::{CarveKind, ReliefCarve, RenderTarget};
use crate::widget::WidgetHost;
use crate::widget::WidgetHostExt;

/// [`place`] for a widget the context owns, named by its handle: lent for the call.
/// Places nothing if the widget is gone or already out on loan.
pub fn place_h<T: WidgetHost + 'static>(h: crate::widget::Handle<T>, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    ctx.lend_h(h, |w, ctx| place(w, x, y, ww, wh, ctx));
}

/// The layout half of [`render_widget`]: everything it does to the widget, and no painting.
/// The block's contract is the flat host's — `(x, y)` is the top of the detached label and
/// `wh` the block's height, label strip included, `ww` its width — and the widget is laid out
/// at exactly that size, shaped (`prepare_text`), and its open popover, if it has one,
/// registered with the context. A display-list host calls this where a flat host called
/// `render_widget` (the same arguments, less the target) and paints the widget in its frame
/// with `widget::painter::paint_root_into`. [`render_widget`] runs it first, so the two place
/// a widget identically.
pub fn place<T: WidgetHost + 'static>(w: &mut T, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    // The flat-host contract, the same block `set_rect` takes: `(x, y)` is the top of
    // the detached label and `wh` the block height, label strip included. `layout`
    // takes the CONTENT origin and height, so step down by the strip.
    let strip = w.label_strip();
    let content_h = (wh - strip).max(0.0);
    w.layout(crate::widget::Point { x, y: y + strip }, crate::widget::LayoutConstraints::new(ww, ww, content_h, content_h), ctx);

    // Shape: `build_frame` does it for the registered widgets of a display-list frame, and
    // nothing does on the flat-host path. A flat host consumes `all_quads`, so `prepare_text`
    // — where a TextBox records the per-glyph x offsets its selection highlight, caret and
    // click->index mapping all read — was never called for the widgets it draws. Those three then fell back to
    // `measure_text_width("M")`, an SVG-rasterized INKED extent rather than an advance, so
    // the highlight under-ran the glyphs by a few px per character (a full glyph by the end
    // of "example.com"). Hosts that shape for themselves (cce-files, the TreeList) just
    // re-read the shared buffer cache here.
    if let Ok(mut fs) = crate::geometry_font_system().lock() {
        w.prepare_text(&mut fs);
    }
    // Painting reads no popover registration, so registering before the paint (where
    // `render_widget` paints) changes nothing.
    if w.popover_rect().is_some() {
        ctx.register_popover_id(w.base().id());
    }
}

/// [`render_widget`] for a widget the context owns, named by its handle: lent for the call.
/// Draws nothing if the widget is gone or already out on loan.
pub fn render_widget_h<T: WidgetHost + 'static>(
    pc: &mut dyn RenderTarget,
    h: crate::widget::Handle<T>,
    x: f32,
    y: f32,
    ww: f32,
    wh: f32,
    ctx: &mut UiContext,
) {
    ctx.lend_h(h, |w, ctx| render_widget(pc, w, x, y, ww, wh, ctx));
}

/// Place `w` in the block at `(x, y)`, `ww` × `wh` ([`place`]), then replay its paint walk
/// onto the flat host `pc`, every prim in order, mapped onto the `RenderTarget` calls.
pub fn render_widget<T: WidgetHost + 'static>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    place(w, x, y, ww, wh, ctx);
    let (style_r, corners) =
        w.paint_model().corner_style(w.content_rect()).unwrap_or((0.0, (false, false, false, false)));
    let r = if corners != (false, false, false, false) { style_r } else { 0.0 };
    let (wx, mut wy, www, mut whh) = w.rect();
    let top_room = w.label_strip();
    wy += top_room;
    whh -= top_room;

    // ONE ordered replay of the paint walk — the same walk the live display-
    // list render runs (`scene::painter`), every prim in the order the widget
    // painted it, each mapped onto the flat host's RenderTarget surface.
    //
    // This used to be three passes over three typed views of the same paint:
    // the relief prims (downcast per widget type and re-derived from the
    // widget's accessors — the Dropdown's inset plate, the TextBox's well, the
    // Button's face, the Toggle's faces and steps), then every plain quad
    // (`all_quads`), then every rounded quad (`all_rounded_quads`). Splitting
    // one paint into typed streams loses the order between them, and the
    // order is the picture: a TextBox draws its rounded background, carves
    // its well, THEN lays the selection highlight and caret on top — the
    // three-pass replay put the well under the highlight and the background
    // over both. A Dropdown's hovered row is drawn after its menu plate; a
    // host that replays plates after rects buries the highlight. The prim
    // walk keeps the widget's order, covers every widget instead of the four
    // that had a special case, and carries the relief prims' own per-corner
    // radii and depth (the re-derivations rounded those off).
    //
    // Mapping onto the tuple surface: Quad and RoundedRect are the two
    // native fills (the root's plain background keeps its solid-border
    // expansion and window-corner resolution); a zero-stroke Border is an
    // inset plate's FACE and is held until the Trough that follows it, so the
    // pair reaches the host as ONE `inset_plate` call (a relief host carves
    // it for real; the default degrades to the flat fill); Recess/Boss go to
    // `relief_carve`; a Bevel degrades to its fill — what a flat host can
    // draw of a raised plate. Ridges, circles, arcs, vectors and images have
    // no flat-surface counterpart and are skipped, as they always were.
    let solid_border = w.solid_border();
    let mut text_scratch = crate::scene::paint::PaintCtx::new();
    crate::widget::painter::paint_root_into(&*ctx, &*w, &mut text_scratch);

    // A zero-stroke Border waiting for its Trough: (rect, radii, fill).
    let mut pending_face: Option<(crate::scene::layout::Rect, crate::scene::paint::Radii, [f32; 4])> = None;
    fn same_rect(a: crate::scene::layout::Rect, b: crate::scene::layout::Rect) -> bool {
        (a.x - b.x).abs() < 0.1 && (a.y - b.y).abs() < 0.1 && (a.width - b.width).abs() < 0.1 && (a.height - b.height).abs() < 0.1
    }
    fn emit_rounded(pc: &mut dyn RenderTarget, rect: crate::scene::layout::Rect, radii: crate::scene::paint::Radii, color: [f32; 4]) {
        let (r1, r2, r3, r4) = radii;
        let radius = r1.max(r2).max(r3).max(r4);
        let mask = (r1 > 0.0, r2 > 0.0, r3 > 0.0, r4 > 0.0);
        pc.rect_with_radius_corners(color, rect.x, rect.y, rect.width, rect.height, radius, mask);
    }

    for item in text_scratch.finish().items {
        use crate::scene::paint::Prim;
        let trough_for_face = match (&item.prim, &pending_face) {
            (Prim::Trough { rect, .. }, Some((face_rect, _, _))) => same_rect(*rect, *face_rect),
            // `inset_plate`'s edge: a field that is all run.
            (Prim::Field { rect, split, end, .. }, Some((face_rect, _, _))) => {
                *split <= rect.x && *end >= rect.x + rect.width && same_rect(*rect, *face_rect)
            }
            _ => false,
        };
        if !trough_for_face {
            if let Some((frect, fradii, fill)) = pending_face.take() {
                emit_rounded(pc, frect, fradii, fill);
            }
        }
        match item.prim {
            Prim::Quad { rect, color: qc } => {
                let (qx, qy, qw, qh) = (rect.x, rect.y, rect.width, rect.height);
                let extra_corners = (
                    corners.0 && qx <= wx + 1.5 && qy <= wy + 1.5,
                    corners.1 && qx + qw >= wx + www - 1.5 && qy <= wy + 1.5,
                    corners.2 && qx + qw >= wx + www - 1.5 && qy + qh >= wy + whh - 1.5,
                    corners.3 && qx <= wx + 1.5 && qy + qh >= wy + whh - 1.5,
                );

                let (resolved_r, resolved_corners) = if r <= 0.1 || corners == (false, false, false, false) || extra_corners == (false, false, false, false) {
                    (0.0, (false, false, false, false))
                } else {
                    (r, extra_corners)
                };

                // The widget's own background quad with a solid border: full-size
                // border quad, then the inset background over it.
                let mut border_drawn = false;
                let is_bg_quad = (qx - wx).abs() < 0.1 && (qy - wy).abs() < 0.1 && (qw - www).abs() < 0.1 && (qh - whh).abs() < 0.1;
                if is_bg_quad {
                    if let Some((border_color, thickness)) = solid_border {
                        if thickness > 0.0 {
                            pc.rect_with_radius_corners(border_color, qx, qy, qw, qh, resolved_r, resolved_corners);
                            pc.rect_with_radius_corners(
                                qc,
                                qx + thickness,
                                qy + thickness,
                                (qw - 2.0 * thickness).max(0.0),
                                (qh - 2.0 * thickness).max(0.0),
                                (resolved_r - thickness).max(0.0),
                                resolved_corners,
                            );
                            border_drawn = true;
                        }
                    }
                }

                if !border_drawn {
                    pc.rect_with_radius_corners(qc, qx, qy, qw, qh, resolved_r, resolved_corners);
                }
            }
            Prim::RoundedRect { rect, radius, corners: qcorners, color: qc } => {
                pc.rect_with_radius_corners(qc, rect.x, rect.y, rect.width, rect.height, radius, qcorners);
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                if thickness > 0.0 && border[3].abs() > 0.001 {
                    emit_rounded(pc, rect, radii, border);
                    if fill[3].abs() > 0.001 {
                        let inner = crate::scene::layout::Rect {
                            x: rect.x + thickness,
                            y: rect.y + thickness,
                            width: (rect.width - 2.0 * thickness).max(0.0),
                            height: (rect.height - 2.0 * thickness).max(0.0),
                        };
                        let (r1, r2, r3, r4) = radii;
                        let shrink = |v: f32| if v > 0.0 { (v - thickness).max(0.0) } else { 0.0 };
                        emit_rounded(pc, inner, (shrink(r1), shrink(r2), shrink(r3), shrink(r4)), fill);
                    }
                } else if fill[3].abs() > 0.001 {
                    // abs(): a negative alpha is the frost sentinel, a real face.
                    pending_face = Some((rect, radii, fill));
                }
            }
            // A flat host has no blended outline: the two-box form
            // `Prim::Field` replaced — the well to the seam, the run's flush
            // plate past it.
            Prim::Field { rect, radii, depth, split, end, tint } => {
                let (fl, fr) = (rect.x, rect.x + rect.width);
                let (well_l, well_r) = (split > fl, end < fr);
                // All run and no well (`PaintCtx::inset_plate`'s edge): the
                // plate alone, with the face that came before it.
                let face = if !well_l && !well_r { pending_face.take().map(|(_, _, fill)| fill) } else { None }.unwrap_or([0.0; 4]);
                let rx = split.max(fl);
                let rw = (end.min(fr) - rx).max(0.0);
                let mut well = |x: f32, w: f32, radii: (f32, f32, f32, f32)| {
                    pc.relief_carve(&ReliefCarve {
                        kind: CarveKind::Recess { tint },
                        x,
                        y: rect.y,
                        w,
                        h: rect.height,
                        radii,
                        depth,
                        edges: (true, true, true, true),
                    });
                };
                if well_l {
                    well(fl, rx - fl, (radii.0, 0.0, 0.0, radii.3));
                }
                if well_r {
                    well(rx + rw, fr - rx - rw, (0.0, radii.1, radii.2, 0.0));
                }
                let r = if well_r { radii.0.max(radii.3) } else { radii.1.max(radii.2) };
                match tint {
                    Some(t) => pc.inset_plate_tinted(face, rx, rect.y, rw, rect.height, r, depth, t),
                    None => pc.inset_plate(face, rx, rect.y, rw, rect.height, r, depth),
                }
            }
            Prim::Trough { rect, radii, depth, tint, .. } => {
                let face = pending_face.take().map(|(_, _, fill)| fill).unwrap_or([0.0; 4]);
                let (r1, r2, r3, r4) = radii;
                let r = r1.max(r2).max(r3).max(r4);
                match tint {
                    Some(t) => pc.inset_plate_tinted(face, rect.x, rect.y, rect.width, rect.height, r, depth, t),
                    None => pc.inset_plate(face, rect.x, rect.y, rect.width, rect.height, r, depth),
                }
            }
            Prim::Bevel { rect, radii, material, .. } => {
                let color = material.fill(crate::scene::material::PlateRole::Nested);
                if color[3].abs() > 0.001 {
                    emit_rounded(pc, rect, radii, color);
                }
            }
            Prim::Recess { rect, radii, depth, edges, tint } => {
                pc.relief_carve(&ReliefCarve {
                    kind: CarveKind::Recess { tint },
                    x: rect.x,
                    y: rect.y,
                    w: rect.width,
                    h: rect.height,
                    radii,
                    depth,
                    edges,
                });
            }
            Prim::Boss { rect, radii, depth, edges, tint } => {
                pc.relief_carve(&ReliefCarve {
                    kind: CarveKind::Boss { tint },
                    x: rect.x,
                    y: rect.y,
                    w: rect.width,
                    h: rect.height,
                    radii,
                    depth,
                    edges,
                });
            }
            // Text via the paint walk: each widget's Text prims (content font + scroll-ancestor
            // clip) exactly as the live display-list render does; the prim already carries the
            // per-widget font + bounds.
            Prim::Text { text, x, y, font_size, color, font, bounds, .. } => {
                let color_f32 = [
                    color[0] as f32 / 255.0,
                    color[1] as f32 / 255.0,
                    color[2] as f32 / 255.0,
                    1.0,
                ];
                // Compose the walk's container clip with the prim's own bounds (the engine's dl-text
                // merge), so a clipping ancestor still bounds the text.
                let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
                let merged = match (clip, bounds) {
                    (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                    (Some(a), None) => Some(a),
                    (None, b) => b,
                };
                match font {
                    Some(ref f) => pc.text_with_font_and_bounds(&text, x, y, font_size, color_f32, f, merged),
                    None => pc.text_with_bounds(&text, x, y, font_size, color_f32, merged),
                }
            }
            // Strokes, arcs and discs — a graph's wires and ports, chiefly.
            // Until 2026-10-06 these were dropped too, so a graph in a flat
            // host (cce-files' Graph page) drew its nodes and no wires.
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => {
                if let Some(c) = item.clip {
                    pc.push_clip_rect(c.x, c.y, c.width, c.height);
                }
                pc.line(x1, y1, x2, y2, thickness, color, cap);
                if item.clip.is_some() {
                    pc.pop_clip_rect();
                }
            }
            Prim::Arc { cx, cy, radius, thickness, start, end, color } => {
                if let Some(c) = item.clip {
                    pc.push_clip_rect(c.x, c.y, c.width, c.height);
                }
                pc.arc(cx, cy, radius, thickness, start, end, color);
                if item.clip.is_some() {
                    pc.pop_clip_rect();
                }
            }
            Prim::Circle { cx, cy, radius, color } => {
                if let Some(c) = item.clip {
                    pc.push_clip_rect(c.x, c.y, c.width, c.height);
                }
                pc.circle(cx, cy, radius, color);
                if item.clip.is_some() {
                    pc.pop_clip_rect();
                }
            }
            // A widget's GLYPH (a dropdown's arrow, a spinbox's −/+, a menu
            // mark): drawn by name through the host's `RenderTarget::icon`,
            // since an image id means nothing to a flat host. Until this arm
            // every widget glyph vanished from a flat host — the toolkit drew
            // its symbols as text before 2026-10-05, and this replay dropped
            // images. An image that is not a bundled glyph is still dropped.
            Prim::Image { image, rect, alpha } => {
                if let Some((name, _, tint)) = crate::icon_source(image) {
                    let [r, g, b] = tint.unwrap_or([255, 255, 255]);
                    let color = [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, alpha];
                    if let Some(c) = item.clip {
                        pc.push_clip_rect(c.x, c.y, c.width, c.height);
                    }
                    pc.icon(&name, rect, color);
                    if item.clip.is_some() {
                        pc.pop_clip_rect();
                    }
                }
            }
            _ => {}
        }
    }
    if let Some((frect, fradii, fill)) = pending_face.take() {
        emit_rounded(pc, frect, fradii, fill);
    }
}

pub fn render_popovers(pc: &mut dyn RenderTarget, ctx: &UiContext) {
    for &pop_id in &ctx.active_popovers {
        if let Some(ptr) = ctx.tree.get_ptr(pop_id) {
            unsafe {
                (*ptr).render_popover(pc);
            }
        }
    }
    // Registry sweep for open popovers the app never registered (popover
    // registration is optional and spotty) — the same fallback the coverage
    // check and the engine's outside-press close use.
    for (id, ptr) in ctx.tree.iter_registered() {
        if ctx.active_popovers.contains(&id) {
            continue;
        }
        unsafe {
            if let Some(w) = ptr.as_ref() {
                if w.visible() && w.popover_rect().is_some() {
                    w.render_popover(pc);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::{DisplayList, PaintCtx};
    use crate::widget::painter::paint_root_into;
    use crate::widget::{Button, Dropdown, TextBox};

    fn painted(ctx: &UiContext, w: &dyn WidgetHost) -> DisplayList {
        let mut pc = PaintCtx::new();
        paint_root_into(ctx, w, &mut pc);
        pc.finish()
    }

    /// Two copies of a widget in the same block, one placed and one rendered through the
    /// bridge into a `PaintCtx`: the same rect, the same paint, and the prims the bridge drew.
    fn place_matches_render_widget<W: WidgetHost + 'static>(make: impl Fn() -> W, (x, y, ww, wh): (f32, f32, f32, f32)) {
        let mut ctx = UiContext::new();
        let placed = ctx.insert(make());
        let rendered = ctx.insert(make());
        place_h(placed, x, y, ww, wh, &mut ctx);
        let mut bridge = PaintCtx::new();
        render_widget_h(&mut bridge, rendered, x, y, ww, wh, &mut ctx);
        assert_eq!(ctx[placed].rect(), (x, y, ww, wh), "the block it was given");
        assert_eq!(ctx[placed].rect(), ctx[rendered].rect());
        let native = painted(&ctx, &ctx[placed]);
        assert!(native.items.iter().any(|i| matches!(i.prim, crate::scene::paint::Prim::Text { .. })), "{native:?}");
        assert_eq!(native, painted(&ctx, &ctx[rendered]), "placed as render_widget places it");
        assert_eq!(native, bridge.finish(), "painted as render_widget drew it");
    }

    #[test]
    fn a_placed_button_paints_what_render_widget_drew() {
        place_matches_render_widget(|| Button::new(0.0, 0.0, 0.0, 0.0).with_label("OK"), (10.0, 20.0, 80.0, 28.0));
    }

    /// The label strip heads the block: the field is laid out below it, the label in it.
    #[test]
    fn a_placed_labelled_text_box_paints_what_render_widget_drew() {
        let make = || TextBox::new("hello".to_string()).with_label("Name");
        assert!(make().label_strip() > 0.0, "a labelled box carries a strip");
        place_matches_render_widget(make, (10.0, 20.0, 160.0, 48.0));
    }

    /// An open popover is registered with the context as `render_widget` registers it; a
    /// closed one is not.
    #[test]
    fn placing_an_open_dropdown_registers_its_popover() {
        let mut ctx = UiContext::new();
        let mut open = Dropdown::new(vec!["a".to_string(), "b".to_string()], 0);
        open.inner_mut().open = true;
        let open = ctx.insert(open);
        let closed = ctx.insert(Dropdown::new(vec!["a".to_string()], 0));
        place_h(closed, 0.0, 0.0, 120.0, 24.0, &mut ctx);
        assert!(ctx.active_popovers.is_empty());
        place_h(open, 0.0, 40.0, 120.0, 24.0, &mut ctx);
        assert_eq!(ctx.active_popovers, vec![ctx[open].id()]);
    }
}
