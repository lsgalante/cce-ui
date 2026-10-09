//! The surface objects a painter asks for by form: a root or pane plate ([`PlateSpec`]), a
//! control plate's stance and face ([`ControlPlate`], [`PlateStance`]), a well with its run ([`Field`]).

use super::*;

/// RFC Phase 7b: the ONE description of a lit base surface — a window's root
/// plate or a nested pane plate — distinguished only by ROLE data, never by
/// type. A window root is a plate whose four corners are all window corners;
/// detaching a pane into its own window is a role flip, nothing more.
///
/// The material's tint always carries POSITIVE alpha; the frost encoding is
/// applied by [`Self::fill`] per the role (see the Phase 7b blur-regime note
/// in `docs/rfc-core-rebuild.md`): a root plate stays positive-alpha (the
/// COMPOSITOR frosts behind the window), a nested plate whose material is
/// [`Frost::Frosted`] encodes the in-app frost pass's negative-alpha sentinel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateSpec {
    pub rect: Rect,
    /// What the plate is made of: tint, frost and finish
    /// (`docs/rfc-material.md`). `Material::root()` / `Material::pane()` are
    /// the rung defaults; `Material::opaque(c)` an app's own colour.
    pub material: Material,
    /// Which corners lie ON the window silhouette (TL, TR, BR, BL).
    pub window_corners: (bool, bool, bool, bool),
    /// Transition-band width of the rolled perimeter. Negative = the fill-less
    /// roll-overlay sentinel (see [`PaintCtx::plate`]).
    pub depth: f32,
}

impl PlateSpec {
    /// THE standard root plate of a `width` x `height` window — the base
    /// surface every cce app stands its panes and controls on: the whole
    /// window, the root rung's material ([`Material::root`], which is the
    /// DE's `style.surface.plate.root.color` at its configured opacity
    /// unless a `material=` is bound), all four corners on the silhouette,
    /// and the perimeter rolled over [`crate::layout::bevel_width`].
    ///
    /// This is the spec every app used to hand-copy as an eight-line block
    /// (page-low colour, opacity override, four window corners, the DE roll)
    /// — the copies are gone, and a window whose base is anything else is
    /// off the standard on purpose, which its code should say. Emit it with
    /// [`PaintCtx::root_plate`]; deviate with [`Self::with_material`] /
    /// [`Self::with_depth`] (cce-system-interface's own tint, an overlay's
    /// shallower roll).
    pub fn window(width: f32, height: f32) -> Self {
        Self::root_at(Rect { x: 0.0, y: 0.0, width, height })
    }

    /// [`Self::window`] for a root plate that is not the whole surface — a
    /// layer-shell overlay drawing the window silhouette itself inside a
    /// larger transparent surface (cce-cloud). Same material, corners and
    /// roll; `rect` is where the "window" is.
    pub fn root_at(rect: Rect) -> Self {
        Self {
            rect,
            material: Material::root(),
            window_corners: (true, true, true, true),
            depth: crate::layout::bevel_width(),
        }
    }

    /// This plate made of `material` instead of its rung's default.
    pub fn with_material(mut self, material: Material) -> Self {
        self.material = material;
        self
    }

    /// This plate with a `depth` roll instead of the DE's `bevel_width`.
    pub fn with_depth(mut self, depth: f32) -> Self {
        self.depth = depth;
        self
    }

    /// All four corners on the silhouette: this plate IS the window's base
    /// surface.
    pub fn is_root(&self) -> bool {
        let (tl, tr, br, bl) = self.window_corners;
        tl && tr && br && bl
    }

    /// Which of `rect`'s corners lie on a `win_w` x `win_h` window's
    /// silhouette (edge tolerance 1.5px) — the designer's `pane_plate_radii`
    /// derivation, toolkit-side.
    pub fn window_corner_flags(rect: Rect, win_w: f32, win_h: f32) -> (bool, bool, bool, bool) {
        let e = 1.5;
        let left = rect.x <= e;
        let top = rect.y <= e;
        let right = rect.x + rect.width >= win_w - e;
        let bottom = rect.y + rect.height >= win_h - e;
        (top && left, top && right, bottom && right, bottom && left)
    }

    /// Per-corner radii for `flags`: a window corner wears the SHARED
    /// silhouette curve (`window_corner_radius * corner_span_factor` — the
    /// compositor clips the window and the desktop grid draws its cells from
    /// the same value, so window-corner arcs must follow it, never a per-app
    /// plate override); an interior corner wears the nominal
    /// `plate_corner_radius`.
    pub fn radii_for(flags: (bool, bool, bool, bool)) -> Radii {
        let nominal = crate::layout::plate_corner_radius();
        let window_r = crate::layout::window_silhouette_radius();
        let (tl, tr, br, bl) = flags;
        let pick = |on: bool| if on { window_r } else { nominal };
        (pick(tl), pick(tr), pick(br), pick(bl))
    }

    /// [`Self::radii_for`] over this spec's flags.
    pub fn radii(&self) -> Radii {
        Self::radii_for(self.window_corners)
    }

    /// This plate detached into its own window (RFC Phase 7c): every corner
    /// becomes a window corner, and with the role the radii snap to the
    /// silhouette curve and [`Self::fill`] flips frost regimes (the
    /// compositor's blur-behind takes over from the in-app sentinel). The
    /// reverse — reattaching — is the host assigning its computed
    /// `window_corner_flags` back.
    pub fn detached(mut self) -> Self {
        self.window_corners = (true, true, true, true);
        self
    }

    /// The frost regime this plate is under: [`PlateRole::Root`] when it IS
    /// the window's base surface, [`PlateRole::Nested`] otherwise.
    pub fn role(&self) -> PlateRole {
        if self.is_root() { PlateRole::Root } else { PlateRole::Nested }
    }

    /// The fill with the role-correct frost encoding: root → alpha forced
    /// non-negative (the compositor's frost, not ours), nested + frosted →
    /// the in-app frost pass's negative-alpha sentinel. The rule itself is
    /// [`Material::fill_tint`], the one place a negative alpha is written.
    pub fn fill(&self) -> [f32; 4] {
        self.material.fill(self.role())
    }
}

/// The **relief primitives** are the members of this enum that describe a lit
/// surface rather than a flat fill: [`Prim::Bevel`], [`Prim::Plate`],
/// [`Prim::Recess`], [`Prim::Boss`], [`Prim::Ridge`], [`Prim::ConcaveFillet`],
/// [`Prim::Groove`], [`Prim::Lattice`], [`Prim::CarveUnion`] and [`Prim::Sphere`]. They share one lighting model — the
/// DE's light vector, roll width and profile, per-pixel through shader2d's
/// SDF branch (see `crate::layout::bevel_shader`) — and split in two:
///
/// - **plates** carry their own fill: `Bevel`, `Plate`. Shader mode 1.
/// - **carves** emit shading ONLY, no fill, over whatever is already painted
///   beneath: `Recess`, `Boss`, `Ridge`, `ConcaveFillet`, `Groove`, `Lattice`,
///   `CarveUnion`. Modes 2-4, 6-8, 13 and 14. (`Sphere`, mode 5, is neither —
///   a lit ball under the same model.)
///
/// That split is load-bearing for flat-path hosts, which need one list for the
/// faces and another for the edges drawn over them (cce-files' `rects` vs
/// `reliefs`).
///
/// How a control plate sits on the surface beneath it — see "Plates, wells
/// and seams" in `CLAUDE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlateStance {
    /// Floats above the surface: a [`Prim::Bevel`] when it has a face of its
    /// own, an edges-only [`Prim::Boss`] carved inside its footprint when the
    /// face is transparent (the surface below shows through as the face).
    Raised,
    /// Level with the surface inside a groove ring: a [`Prim::Trough`] carved
    /// inside its footprint, with the face as a flat fill when it has one
    /// ([`PaintCtx::inset_plate`]).
    Flush,
    /// No relief at all — the face alone, filling the footprint as a flat
    /// rounded rect. This is the PANE rung's material brought down to the
    /// control rung, and it exists because the other two stances cannot give
    /// a control two things a pane has:
    ///
    /// - **Its silhouette IS its rect.** `Raised` and `Flush` both carve
    ///   inside the footprint, so their visible edge sits half the carve depth
    ///   in and a control laid out on the same numbers as a pane does not line
    ///   up with it. Nothing is inset here, so it does.
    /// - **It can be frosted.** The blur-behind sentinel (a negative alpha)
    ///   only reaches quads, and the relief stances lay their face through
    ///   `Border`/`Trough` strokes. This one fills with a quad, so a control
    ///   can be made of the same frosted material as the pane behind it.
    ///
    /// The cost is that a flat fill carries ONE radius, not four: the
    /// per-corner silhouette a nested relief control computes (Dropdown's
    /// concentric corner adjustment) has no equivalent here, and `radii.0` is
    /// used for all four corners. A focus `tint` is drawn as a ring, since
    /// there is no rim to light.
    Flat,
}

/// How far past a [`Prim::Field`]'s end its run's end is put to say the run
/// REACHES that end and there is no well on that side: far enough that the
/// blend across the seam and the seam's own wall land nowhere near the
/// field. An encoding of the prim's; a [`Field`] says it by its form.
pub const FIELD_RUN_ONLY: f32 = 1.0e4;

/// How near a run's end may come to the field's end and still be taken as
/// reaching it — the shader's own tolerance (half a pixel, `MODE_FIELD`).
pub(super) const FIELD_REACH: f32 = 0.5;

/// A FIELD: a well cut into a plate with a flush plate, the RUN, standing
/// in it, one outline round both. One object in every form a control takes
/// — they differ only in where the run is:
///
/// | form | constructor | run | well |
/// | --- | --- | --- | --- |
/// | text box | [`Field::well`] | none | the whole field |
/// | flush control plate (dropdown, button, …) | [`Field::run`] | the whole field | none |
/// | text row with its picker, spinbox | [`Field::ending_in_run`] | the right end | left of it |
/// | toggle | [`Field::sliding_run`] | part of the field, anywhere | either side of it |
///
/// Painted by [`PaintCtx::field`], as a [`Prim::Field`] — except a field
/// with no run, which is a [`Prim::Recess`]: that is what a plain well has
/// always been, and a recess groups into the plate under it where a field
/// prim never does. The rect is the field's OUTLINE (the carve's boundary;
/// a widget takes it through [`crate::layout::carve_inside`] from its
/// footprint), and `depth` the wall width.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Field {
    pub rect: Rect,
    pub radii: Radii,
    pub depth: f32,
    /// The run's span in x, clamped to the outline; `None` for a well.
    run: Option<(f32, f32)>,
    /// Lights the rim — the focus and hover treatment.
    pub tint: Option<[f32; 3]>,
}

impl Field {
    /// All well, no run: a text box.
    pub fn well(rect: Rect, radii: Radii, depth: f32) -> Self {
        Field { rect, radii, depth, run: None, tint: None }
    }

    /// All run, no well: the edge of a flush control plate — a dropdown
    /// trigger, a button ([`PaintCtx::inset_plate`]).
    pub fn run(rect: Rect, radii: Radii, depth: f32) -> Self {
        Self::spanning(rect, radii, depth, rect.x, rect.x + rect.width)
    }

    /// A well ending in a run from `split` to the field's right end: a text
    /// row with its completion picker, a spinbox with its -/+ run.
    pub fn ending_in_run(rect: Rect, radii: Radii, depth: f32, split: f32) -> Self {
        Self::spanning(rect, radii, depth, split, rect.x + rect.width)
    }

    /// A run `width` wide, at `t` of its travel along the field: 0 is the
    /// left end, 1 the right, a well either side between — a toggle, whose
    /// run glides from off to on.
    pub fn sliding_run(rect: Rect, radii: Radii, depth: f32, width: f32, t: f32) -> Self {
        let width = width.clamp(0.0, rect.width);
        let x = rect.x + t.clamp(0.0, 1.0) * (rect.width - width);
        Self::spanning(rect, radii, depth, x, x + width)
    }

    /// A run from `a` to `b`, anywhere in the field — the general form the
    /// others are; clamped to the outline.
    pub fn spanning(rect: Rect, radii: Radii, depth: f32, a: f32, b: f32) -> Self {
        let (l, r) = (rect.x, rect.x + rect.width);
        let a = a.clamp(l, r);
        let b = b.clamp(a, r);
        Field { rect, radii, depth, run: Some((a, b)), tint: None }
    }

    /// The rim lit in `tint` (focus, hover), or not.
    pub fn with_tint(mut self, tint: Option<[f32; 3]>) -> Self {
        self.tint = tint;
        self
    }

    /// Where the run is, `(left, right)` within the outline; `None` for a
    /// field that is all well.
    pub fn run_span(&self) -> Option<(f32, f32)> {
        self.run
    }

    /// Whether any of the field is well — false for a field that is all run.
    pub fn has_well(&self) -> bool {
        match self.run {
            None => true,
            Some((a, b)) => a > self.rect.x + FIELD_REACH || b < self.rect.x + self.rect.width - FIELD_REACH,
        }
    }

    /// The run as [`Prim::Field`] encodes it, `(split, end)`: an end that
    /// reaches the field's is put [`FIELD_RUN_ONLY`] past it.
    pub(super) fn prim_span(&self) -> Option<(f32, f32)> {
        let (a, b) = self.run?;
        let (l, r) = (self.rect.x, self.rect.x + self.rect.width);
        let split = if a <= l + FIELD_REACH { l - FIELD_RUN_ONLY } else { a };
        let end = if b >= r - FIELD_REACH { r + FIELD_RUN_ONLY } else { b };
        Some((split, end))
    }
}

/// A control plate: the thing you press, at the control rung of the plate
/// ladder. One description for every control face — Button, Dropdown,
/// FontSelector, Breadcrumb, a ButtonStrip's selected plateau — so their
/// carve-inside, radius, depth and transparent-face rules cannot drift.
/// Painted by [`PaintCtx::control_plate`]. The root and pane rungs of the
/// ladder are [`PlateSpec`]; this is the same idea one rung down.
///
/// `rect` is the plate's footprint, the OUTER edge of its silhouette; the
/// carve is taken inside it ([`crate::layout::carve_inside`]), so the gap
/// beside the plate is the gap. `radii` is the silhouette, per corner (a
/// Dropdown nested concentrically in a frame corner adjusts each). `face`
/// is the plate's own material; `None` means the surface below IS the face
/// (edges only), and a frosted material is a real face — the frost carried
/// where the stance can (`Flat`; see [`PlateStance`]). `depth` is the
/// relief's wall width — [`ControlPlate::control`] takes the DE relief width
/// capped at a fifth of the height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlPlate {
    pub rect: Rect,
    pub radii: Radii,
    pub stance: PlateStance,
    pub face: Option<Material>,
    pub depth: f32,
    /// The rim's light and shadow tinted this colour: the keyboard-focus
    /// ring, drawn on the plate's own relief rather than as extra geometry.
    /// `None` untinted.
    pub tint: Option<[f32; 3]>,
}

impl ControlPlate {
    /// A control plate at `rect` with a uniform corner `radius`: depth from
    /// the DE relief width, capped at a fifth of the plate's height.
    pub fn control(rect: Rect, radius: f32, stance: PlateStance, face: Option<Material>) -> Self {
        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
        Self { rect, radii: (radius, radius, radius, radius), stance, face, depth, tint: None }
    }


    /// Light the rim — the focus ring on the plate's silhouette. Pass the
    /// highlight colour while the control holds keyboard focus, `None` otherwise.
    pub fn with_tint(mut self, tint: Option<[f32; 3]>) -> Self {
        self.tint = tint;
        self
    }

    /// The DE's focus-ring colour for a plate rim: the highlight accent, the
    /// same the wells light their rims with while editing.
    pub fn focus_tint() -> [f32; 3] {
        let c = crate::color::highlight_primary_color();
        [c[0], c[1], c[2]]
    }

    /// Per-corner silhouette (a concentric corner-frame adjustment).
    pub fn with_radii(mut self, radii: Radii) -> Self {
        self.radii = radii;
        self
    }

    /// An explicit wall width — a plate that shares its depth with the well
    /// it stands in, or one capped by its short side rather than its height.
    pub fn with_depth(mut self, depth: f32) -> Self {
        self.depth = depth;
        self
    }

    /// The face a stance draws: `Some` only for a material with a visible
    /// tint — a transparent one is the surface below showing through, the
    /// same as `None`.
    pub fn faced(&self) -> Option<&Material> {
        self.face.as_ref().filter(|m| m.tint[3] > 0.001)
    }

    /// The face as the encoded fill the flat-path bridges consume
    /// (`Button::inset_face`, cce-system-interface's `ControlCarve`):
    /// transparent for no face, else the material's nested fill.
    pub fn face_fill(&self) -> [f32; 4] {
        self.face.map_or([0.0; 4], |m| m.fill(PlateRole::Nested))
    }
}
