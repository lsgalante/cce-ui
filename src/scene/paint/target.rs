//! The flat-host target: [`RenderTarget`], what a host that does not build a display list
//! paints into, and the carve and section-frame types a widget hands it. `PaintCtx`
//! implements it (`render_target.rs`), so a widget's flat-path drawing lands as real prims.
//! The walk that replays a widget onto one is `compose::render_widget`. Legacy: new code
//! paints through `PaintCtx` directly.

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
    /// A flush inset control plate ([`PaintCtx::inset_plate`](crate::scene::paint::PaintCtx::inset_plate)) — the raised
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
