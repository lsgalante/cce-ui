//! The flat-host render bridge: [`RenderTarget`], the target a host that does not build a
//! display list paints into, [`render_widget`] / [`render_popovers`] which replay a widget
//! onto one, and the carve and popover types that cross it. Legacy: new code paints
//! through `scene::paint::PaintCtx`.

use crate::widget::WidgetHostExt;
use crate::widget::WidgetHost;
use crate::context::UiContext;

/// One step carve a widget's `paint` draws, handed to a flat-path host through
/// [`RenderTarget::relief_carve`] so it can re-emit it as a real prim.
///
/// Geometry always comes from the WIDGET (`TextBox::well`, `Toggle::well` /
/// `Toggle::slide_plate`), never re-derived here — a second copy of that math
/// in the bridge is exactly how the flat host's carve and the drawn one drift
/// apart.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReliefCarve {
    pub kind: CarveKind,
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Per-corner radii, clockwise from top-left.
    pub radii: (f32, f32, f32, f32),
    /// Full width of the step's transition band.
    pub depth: f32,
    /// Which walls the carve has (top, right, bottom, left). A suppressed wall
    /// means the step runs flush to its neighbour there — a Spinbox's field
    /// running into its button column, where the two meet in ONE step rather
    /// than two facing walls.
    pub edges: (bool, bool, bool, bool),
}

/// Which way a [`ReliefCarve`] steps.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CarveKind {
    /// Interior one step DOWN ([`crate::scene::paint::PaintCtx::recess_edges`]).
    /// `tint` lights the rim in the focus accent (`recess_tinted`).
    Recess { tint: Option<[f32; 3]> },
    /// Interior one step UP ([`crate::scene::paint::PaintCtx::boss_edges`]).
    Boss { tint: Option<[f32; 3]> },
    /// A FLUSH inset ([`crate::scene::paint::PaintCtx::trough_edges`]): the
    /// interior stays level with the surface and a valley seam runs the
    /// boundary — the closed dropdown's chrome, for a control that is part
    /// of the plate rather than a step up or down from it.
    Trough,
}

impl ReliefCarve {
    /// This carve shifted vertically — the page-scroll adjustment a host
    /// applies when it re-emits collected carves.
    pub fn shifted_y(self, dy: f32) -> Self {
        Self { y: self.y + dy, ..self }
    }
}

pub trait RenderTarget {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32);
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, _radius: f32) {
        self.rect(color, x, y, w, h);
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, _corners: (bool, bool, bool, bool)) {
        self.rect_with_radius(color, x, y, w, h, radius);
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]);
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _font: &str) {
        self.text(content, x, y, size, color);
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _bounds: Option<[f32; 4]>) {
        self.text(content, x, y, size, color);
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, _bounds: Option<[f32; 4]>) {
        self.text_with_font(content, x, y, size, color, font);
    }
    fn push_clip_rect(&mut self, _x: f32, _y: f32, _w: f32, _h: f32) {}
    fn pop_clip_rect(&mut self) {}
    /// A bundled cce-icons glyph — see `PaintCtx::icon`. A target that
    /// cannot draw images (the legacy `PopoverCollector`) draws nothing.
    fn icon(&mut self, _name: &str, _rect: crate::scene::layout::Rect, _color: [f32; 4]) {}
    /// A straight stroke — see `PaintCtx::vector`. What a graph's wires are
    /// made of; a target that draws none (the default) shows no wires.
    fn line(&mut self, _x1: f32, _y1: f32, _x2: f32, _y2: f32, _thickness: f32, _color: [f32; 4], _cap: crate::scene::paint::Cap) {}
    /// A stroked arc, `radius` its OUTER edge — see `PaintCtx::arc`. A
    /// rounded wire's bends.
    fn arc(&mut self, _cx: f32, _cy: f32, _radius: f32, _thickness: f32, _start: f32, _end: f32, _color: [f32; 4]) {}
    /// A filled disc — see `PaintCtx::circle`. A graph's ports.
    fn circle(&mut self, _cx: f32, _cy: f32, _radius: f32, _color: [f32; 4]) {}
    /// A flush inset control plate ([`PaintCtx::inset_plate`]) — the raised
    /// control surface (groove ring down, beveled lip back up). Lets a popover
    /// draw the ACTUAL widget surface expanded (the Dropdown's grown trigger).
    /// Hosts without relief prims degrade to a flat rounded fill.
    fn inset_plate(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, _depth: f32) {
        self.rect_with_radius(color, x, y, w, h, radius);
    }
    /// [`inset_plate`](Self::inset_plate) with the rim lit — the focused
    /// control plate's ring (`ControlPlate::with_tint`). Hosts without relief
    /// prims draw the plain plate.
    fn inset_plate_tinted(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32, _tint: [f32; 3]) {
        self.inset_plate(color, x, y, w, h, radius, depth);
    }
    /// One step carve from a widget's `paint` ([`ReliefCarve`]) — offered here
    /// for the same reason as `inset_plate`: the legacy `all_quads` stream
    /// carries no relief prims, so a flat-path host never sees them.
    ///
    /// The default is deliberately a NO-OP, not a fill. These controls have
    /// transparent faces by design (the host surface IS the well floor / the
    /// plate's face), so the carve is their entire decoration — a host that
    /// can't carve has nothing truthful to draw, and a solid box here would
    /// paint every text field a flat slab it never had.
    fn relief_carve(&mut self, _carve: &ReliefCarve) {}
    /// Whether this host renders sections as sunken wells (the designer idiom).
    /// `SectionContext` then lays the title out left-aligned over its tab box
    /// instead of centered on the top border.
    fn section_relief_style(&self) -> bool {
        false
    }
    /// The section frame hatch: `SectionContext::finish` offers the frame here
    /// before falling back to the legacy 1px outline. A relief-capable host
    /// returns true and carves the section into its plate instead (the
    /// designer sunken-well idiom); the tuple hosts keep the default.
    fn section_relief(&mut self, _frame: &SectionFrame) -> bool {
        false
    }
}

/// A section frame offered to [`RenderTarget::section_relief`]: the content
/// body box plus, under [`RenderTarget::section_relief_style`], the title tab
/// box the label was laid out in — the tab sits flush on the body's top edge
/// (the designer union-carve shape).
pub struct SectionFrame {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub tab: Option<(f32, f32, f32, f32)>,
    pub focused: bool,
    pub is_child: bool,
}

pub struct PopoverCollector {
    pub rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    pub texts: Vec<(String, f32, f32, f32, [f32; 4], Option<String>, Option<[f32; 4]>)>,
}

impl PopoverCollector {
    pub fn new() -> Self {
        Self { rects: Vec::new(), texts: Vec::new() }
    }
}

impl RenderTarget for PopoverCollector {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.rects.push((color, x, y, w, h));
    }

    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        self.texts.push((content.to_string(), size, x, y, color, None, None));
    }

    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string()), None));
    }

    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        self.texts.push((content.to_string(), size, x, y, color, None, bounds));
    }

    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string()), bounds));
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

pub fn render_widget<T: WidgetHost + 'static>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    ctx.register_host(w);
    // The flat-host contract, the same block `set_rect` takes: `(x, y)` is the top of
    // the detached label and `wh` the block height, label strip included. `layout`
    // takes the CONTENT origin and height, so step down by the strip.
    let strip = w.label_strip();
    let content_h = (wh - strip).max(0.0);
    w.layout(crate::widget::Point { x, y: y + strip }, crate::widget::LayoutConstraints::new(ww, ww, content_h, content_h), ctx);

    // Shape, which on this path nobody else does. A flat host consumes
    // `all_quads`, so `prepare_text` — where a TextBox records the per-glyph x
    // offsets its selection highlight, caret and click->index mapping all read
    // — was never called for the widgets it draws. Those three then fell back
    // to `measure_text_width("M")`, an SVG-rasterized INKED extent rather than
    // an advance, so the highlight under-ran the glyphs by a few px per
    // character (a full glyph by the end of "example.com"). Hosts that shape
    // for themselves (cce-files, the TreeList) just re-read the shared buffer
    // cache here.
    if let Ok(mut fs) = crate::geometry_font_system().lock() {
        w.prepare_text(&mut fs);
    }
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
    crate::scene::painter::paint_root_into(&*ctx, &*w, &mut text_scratch);

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
    if w.popover_rect().is_some() {
        ctx.register_popover(w);
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

pub fn partition_concentric_corners(
    x: f32, y: f32, w: f32, h: f32,
    _r_std: f32,
    r_adjust: [f32; 4],
    color: [f32; 4],
) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
    let mut quads = Vec::new();

    let r0 = r_adjust[0];
    let r1 = r_adjust[1];
    let r2 = r_adjust[2];
    let r3 = r_adjust[3];

    let max_left = r0.max(r3);
    let max_right = r1.max(r2);

    let mut push_valid_quad = |qx: f32, qy: f32, qw: f32, qh: f32, qr: f32, qcorners: (bool, bool, bool, bool)| {
        if qw > 0.001 && qh > 0.001 {
            quads.push((qx, qy, qw, qh, qr, color, qcorners));
        }
    };

    // 1. Center vertical block
    push_valid_quad(x + max_left, y, w - max_left - max_right, h, 0.0, (false, false, false, false));

    // 2. Left block
    push_valid_quad(x, y + r0, max_left, h - r0 - r3, 0.0, (false, false, false, false));

    // 3. Top-left transition
    push_valid_quad(x + r0, y, max_left - r0, r0, 0.0, (false, false, false, false));

    // 4. Bottom-left transition
    push_valid_quad(x + r3, y + h - r3, max_left - r3, r3, 0.0, (false, false, false, false));

    // 5. Right block
    push_valid_quad(x + w - max_right, y + r1, max_right, h - r1 - r2, 0.0, (false, false, false, false));

    // 6. Top-right transition
    push_valid_quad(x + w - max_right, y, max_right - r1, r1, 0.0, (false, false, false, false));

    // 7. Bottom-right transition
    push_valid_quad(x + w - max_right, y + h - r2, max_right - r2, r2, 0.0, (false, false, false, false));

    // 8. Corner 0 (top-left)
    push_valid_quad(x, y, r0, r0, r0, (true, false, false, false));

    // 9. Corner 1 (top-right)
    push_valid_quad(x + w - r1, y, r1, r1, r1, (false, true, false, false));

    // 10. Corner 2 (bottom-right)
    push_valid_quad(x + w - r2, y + h - r2, r2, r2, r2, (false, false, true, false));

    // 11. Corner 3 (bottom-left)
    push_valid_quad(x, y + h - r3, r3, r3, r3, (false, false, false, true));

    quads
}
