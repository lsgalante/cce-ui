//! The flat-host target: [`RenderTarget`], what a host that does not build a display list
//! paints into, and the carve types a widget hands it. `PaintCtx`
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
    /// cannot draw images draws nothing.
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
}
