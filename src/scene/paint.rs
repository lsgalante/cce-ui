//! Display list + paint context — Phase 3 of the core rebuild (single paint path).
//!
//! Today the toolkit paints through **three** uncoordinated routes — the app's top-level
//! `view*` methods, each container's recursive `all_quads`/`all_rounded_quads` (with clipping
//! hand-copied into every container), and the immediate-mode `render_widget`/`SectionContext`.
//! Nothing arbitrates z-order (hence the `overlay_quads` escape hatch) and every clip is CPU
//! rect-intersection math duplicated per container (there is no GPU scissor).
//!
//! This module is the foundation for collapsing those into **one** ordered pass: a paint walk
//! emits primitives into a single [`DisplayList`] through a [`PaintCtx`] that carries a **clip
//! stack** (each pushed clip is intersected with the current one, so a primitive records the exact
//! scissor rect it should be drawn under) and a **translate stack** (local coordinates compose to
//! absolute — the seam Phase 4 animation slides/scales through). The backend then tessellates the
//! one ordered list, using the recorded clip as a GPU `set_scissor_rect`.
//!
//! This first cut is pure data + bookkeeping, fully unit-tested without a GPU. Wiring the widget
//! tree's paint into it, and routing the backend through the result, are the runtime-gated
//! follow-ups.

use crate::scene::layout::Rect;

/// End-cap style for a [`Prim::Vector`], mirroring the toolkit's line caps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cap {
    Flat,
    Round,
    Arrow,
}

/// A single paint primitive in logical pixels (absolute coordinates once emitted). These mirror
/// the toolkit's existing tessellators so a `DisplayList` maps directly onto them at draw time.
/// Per-corner radii `(top_left, top_right, bottom_right, bottom_left)`, matching the toolkit's
/// `CornerRadii` order.
pub type Radii = (f32, f32, f32, f32);

/// RFC Phase 7b: the ONE description of a lit base surface — a window's root
/// plate or a nested pane plate — distinguished only by ROLE data, never by
/// type. A window root is a plate whose four corners are all window corners;
/// detaching a pane into its own window is a role flip, nothing more.
///
/// `color` always carries POSITIVE alpha; the frost encoding is applied by
/// [`Self::fill`] per the role (see the Phase 7b blur-regime note in
/// `docs/rfc-core-rebuild.md`): a root plate stays positive-alpha (the
/// COMPOSITOR frosts behind the window), a nested plate with `blur` encodes
/// the in-app frost pass's negative-alpha sentinel.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlateSpec {
    pub rect: Rect,
    /// Fill, linear RGBA, alpha positive — the role encoding is `fill()`'s.
    pub color: [f32; 4],
    /// Frost the surface (encoding per role; see `fill`).
    pub blur: bool,
    /// Which corners lie ON the window silhouette (TL, TR, BR, BL).
    pub window_corners: (bool, bool, bool, bool),
    /// Transition-band width of the rolled perimeter. Negative = the fill-less
    /// roll-overlay sentinel (see [`PaintCtx::plate`]).
    pub depth: f32,
}

impl PlateSpec {
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

    /// The fill with the role-correct frost encoding: root → alpha forced
    /// non-negative (the compositor's frost, not ours), nested + `blur` →
    /// the in-app frost pass's negative-alpha sentinel.
    pub fn fill(&self) -> [f32; 4] {
        let mut c = self.color;
        if self.is_root() {
            c[3] = c[3].abs();
        } else if self.blur {
            c[3] = -c[3].abs();
        }
        c
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
/// is the plate's own fill; transparent means the surface below IS the face
/// (a negative alpha is the blur-behind frost, a real face). `depth` is the
/// relief's wall width — [`ControlPlate::control`] takes the DE relief width
/// capped at a fifth of the height.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ControlPlate {
    pub rect: Rect,
    pub radii: Radii,
    pub stance: PlateStance,
    pub face: [f32; 4],
    pub depth: f32,
    /// The rim lit in this colour: the keyboard-focus ring, drawn on the
    /// plate's own silhouette rather than as extra geometry. `None` unlit.
    pub tint: Option<[f32; 3]>,
}

impl ControlPlate {
    /// A control plate at `rect` with a uniform corner `radius`: depth from
    /// the DE relief width, capped at a fifth of the plate's height.
    pub fn control(rect: Rect, radius: f32, stance: PlateStance, face: [f32; 4]) -> Self {
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

    /// A control plate's face from a configured fill: an opaque one is the
    /// face (alpha forced to 1 — a translucent face would blend into the
    /// relief's shading and read as a second material); a transparent one
    /// leaves the surface below as the face (edges only).
    pub fn face_from_fill(raw: [f32; 4]) -> [f32; 4] {
        if raw[3] > 0.001 {
            [raw[0], raw[1], raw[2], 1.0]
        } else {
            [0.0; 4]
        }
    }
}

/// Call the family **relief primitives**, not "bevel primitives": `Bevel` is one
/// specific member — a filled rounded rect plus a lit roll on its lip — and a
/// groove, a fillet or a sphere is not a bevel in any sense. "Relief" is also
/// what the rest of the stack already says: `layout::control_relief` gates the
/// whole family, and the config node is `relief`. The name **bevel** is reserved
/// for two things: the `Bevel` prim, and the shared *edge treatment* every
/// relief primitive is shaded with (`bevel_width`, `bevel_depth`,
/// `bevel_shader`, `bevel_profile` — the lit roll, not the shape).
/// Shape and material knobs for [`Prim::Droplet`]. Fractions are of the
/// droplet rect's height unless said otherwise, so a spec is resolution- and
/// module-size-independent; the tessellator resolves and clamps them against
/// the concrete rect.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DropletSpec {
    /// How far the sheet's bottom lifts above the rect bottom (the waist the
    /// sides pull up into), fraction of height. 0 = no waist (a capsule).
    pub sag: f32,
    /// Belly capsule radius, fraction of height. **≤ 0 disables the belly**:
    /// the drop is the sheet alone — with `attach` and `sheet_r` rounding its
    /// top and bottom this is the oval dewdrop, and the default.
    pub belly: f32,
    /// Belly half-width, fraction of the half-width left after the belly
    /// radius (1 = the belly spans the whole bottom).
    pub belly_w: f32,
    /// Smooth-union blend distance, fraction of height — bigger = softer neck
    /// between sheet and belly.
    pub blend: f32,
    /// Sheet bottom-corner radius, fraction of height.
    pub sheet_r: f32,
    /// Sheet TOP-corner radius (the meniscus taper at the attach line),
    /// fraction of height. 0 = the sides meet the attach edge square (the
    /// clinging-pool look); larger values narrow the contact span so the
    /// silhouette curves into the edge like a dewdrop. When `attach + sheet_r`
    /// exceeds the sheet height the pair scales down proportionally, so 0.5 +
    /// 0.5 is the fully continuous egg curve with no straight side segment.
    pub attach: f32,
    /// Tint opacity at the deep interior relative to the color's own alpha;
    /// the rim falls toward `clarity` × that (thin water is clearer). 1 = flat.
    pub clarity: f32,
    /// Dome slope amplitude: scales the surface tilt the shading sees.
    pub dome: f32,
    /// Shaded band width (the dome's curved skirt), fraction of height.
    pub band: f32,
    /// Specular (gleam) strength — replaces the DE material's slot.
    pub gleam: f32,
    /// Wet-surface shininess exponent.
    pub shine: f32,
    /// Fresnel rim crest amplitude (the glass-edge brightening).
    pub rim: f32,
    /// Bottom bow: the drop's bottom boundary becomes ONE continuous circular
    /// arc — lowest at center, rising by `bow` (fraction of height) at the
    /// drop's side extents. The arc's radius is derived per drop from that
    /// fixed edge rise, so a wide drop gets a huge radius and the curvature
    /// stays subtle at the middle while a narrow drop curves visibly. 0
    /// disables it (flat bottom run between the corner arcs).
    pub bow: f32,
    /// Corner-curve exponent for the silhouette (and the dome profile riding
    /// it): 2 = circular arcs, above 2 = superellipse quadrants whose
    /// curvature ramps to ZERO at both ends of each arc — every junction
    /// (attach↔side, side↔bottom, curve↔flat top) becomes curvature-
    /// continuous, so unequal attach/sheet_r radii read as ONE flowing curve
    /// instead of two arcs meeting, and the contact eases out of the flat
    /// top like a meniscus. Clamped to [2, 6].
    pub curve: f32,
    /// Extra tint density at the drop's deep interior: the body opacity ramps
    /// from `clarity` at the rim up to `1 + core` (× the color's own alpha,
    /// clamped to opaque) inside — the water reads thickest in the middle,
    /// which is also where a module's text sits, so glyphs get a calmer
    /// field without giving up the watery rim. 0 = the original flat
    /// interior falloff.
    pub core: f32,
    /// Refraction strength in logical px — how far the COMPOSITOR's droplet
    /// backdrop pass bends the image behind the drop at the rim. Client-side
    /// rendering ignores it (a Wayland client cannot see behind its own
    /// window); the compositor reads the same spec and drives its scenefx
    /// droplet node with it. 0 disables the backdrop pass.
    pub refr: f32,
    /// Strength (0-1) of the compositor pass's inverted lens ghost — the
    /// faint upside-down image of the scene a real hanging drop shows in its
    /// belly. Client-side ignored, like `refr`.
    pub ghost: f32,
    /// Contact-shadow strength (0-1): a soft dark falloff cast below the
    /// drop's lower arc, outside the silhouette — the volume cue of a bead
    /// sitting proud of the surface. The host must leave room beneath the
    /// drop box for it (the status bar insets the box by
    /// [`DropletSpec::shadow_gap`]). 0 disables it.
    pub shadow: f32,
}

impl DropletSpec {
    /// Parse the DE's droplet spec string — whitespace-separated `k=v` pairs
    /// onto the defaults (an empty string is all defaults). Unknown keys and
    /// non-numeric values `log::warn!` and are skipped, so a typo surfaces in
    /// the log instead of silently reverting one knob. Shared by the status
    /// bar (which draws the drop) and the compositor (whose scenefx droplet
    /// node refracts the backdrop behind it) so the two sides can never
    /// disagree about a spec's meaning.
    pub fn parse(raw: &str) -> Self {
        let mut spec = Self::default();
        for tok in raw.split_whitespace() {
            let Some((key, val)) = tok.split_once('=') else {
                log::warn!("droplet spec: token '{}' is not k=v — skipped", tok);
                continue;
            };
            let Ok(v) = val.parse::<f32>() else {
                log::warn!("droplet spec: '{}' has a non-numeric value — skipped", tok);
                continue;
            };
            match key {
                "sag" => spec.sag = v,
                "belly" => spec.belly = v,
                "belly_w" => spec.belly_w = v,
                "blend" => spec.blend = v,
                "sheet_r" => spec.sheet_r = v,
                "attach" => spec.attach = v,
                "clarity" => spec.clarity = v,
                "dome" => spec.dome = v,
                "band" => spec.band = v,
                "gleam" => spec.gleam = v,
                "shine" => spec.shine = v,
                "rim" => spec.rim = v,
                "bow" => spec.bow = v,
                "curve" => spec.curve = v,
                "core" => spec.core = v,
                "refr" => spec.refr = v,
                "ghost" => spec.ghost = v,
                "shadow" => spec.shadow = v,
                _ => log::warn!("droplet spec: unknown key '{}' — skipped", key),
            }
        }
        spec
    }

    /// Resolve the silhouette's height-fraction knobs against a concrete rect
    /// (logical px) with the SAME clamps the tessellator applies: returns
    /// `(sheet_r, attach_r, bow_rise)` in logical px, the attach/sheet pair
    /// proportionally scaled down when it overfills the height. The
    /// compositor's droplet backdrop node uses this so its refracting
    /// silhouette and the client-drawn drop are the same shape.
    pub fn resolve_silhouette(&self, w: f32, h: f32) -> (f32, f32, f32) {
        let hx = w * 0.5;
        let hy = h * 0.5;
        let mut sr = (self.sheet_r.clamp(0.0, 1.0) * h).min(hx);
        let mut ar = (self.attach.clamp(0.0, 1.0) * h).min(hx);
        let sheet_h = 2.0 * hy;
        if sr + ar > sheet_h && sr + ar > 0.0 {
            let f = sheet_h / (sr + ar);
            sr *= f;
            ar *= f;
        }
        let bow = (self.bow.clamp(0.0, 0.5) * h).min(hy * 0.9);
        (sr, ar, bow)
    }

    /// Vertical room (logical px) a host should leave BELOW the drop box for
    /// the contact shadow, given the full slot height. One place, so the
    /// bar's reserved gap and the shader's falloff reach stay proportioned.
    pub fn shadow_gap(&self, slot_h: f32) -> f32 {
        if self.shadow > 0.0 {
            (0.16 * slot_h).ceil()
        } else {
            0.0
        }
    }
}

impl Default for DropletSpec {
    fn default() -> Self {
        // The oval dewdrop: no belly, no sag — one continuous curve from a
        // tapered attach line to a fully round bottom. attach + sheet_r fill
        // the whole height (no straight side segment), biased bottom-heavy,
        // and the superellipse curve exponent keeps the unequal pair
        // curvature-continuous. The pendant-pool look is reachable by
        // setting `belly` > 0 (and usually some `sag`).
        Self {
            sag: 0.0,
            belly: 0.0,
            belly_w: 0.5,
            blend: 0.35,
            sheet_r: 0.58,
            attach: 0.42,
            clarity: 0.5,
            dome: 0.9,
            band: 0.9,
            gleam: 1.4,
            shine: 32.0,
            rim: 0.5,
            bow: 0.12,
            curve: 2.6,
            core: 0.35,
            refr: 0.0,
            ghost: 0.0,
            shadow: 0.35,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Prim {
    Quad { rect: Rect, color: [f32; 4] },
    RoundedRect { rect: Rect, radius: f32, corners: (bool, bool, bool, bool), color: [f32; 4] },
    /// A rounded fill plus a solid border stroke — a widget's own "plate" (mirrors
    /// `push_widget_vertices`' non-bevel branch: rounded bg + `push_plate_solid_border_vertices`).
    Border { rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32 },
    /// A beveled plate: a rounded fill at full size plus a light/shadow overlay lip
    /// (mirrors `push_widget_vertices`' bevel branch). `tint` multiplies the lit
    /// roll's specular color — neutral white normally; a host sets it to a
    /// highlight color to mark the plate (the focused-pane treatment) without a
    /// separate border ring. Shader-plates path only; the legacy banded
    /// tessellation ignores it.
    Bevel { rect: Rect, radii: Radii, color: [f32; 4], depth: f32, tint: [f32; 3] },
    /// A recess carved into whatever is already painted underneath — the inverse of
    /// `Bevel`. Emits ONLY the shaded edges, never a fill, so the surface below shows
    /// through the middle: a relief cut into the root plate rather than a plate laid on
    /// top of it. The light vector is negated relative to `Bevel`, so the edges facing
    /// `light_source_position` fall into shadow and the far edges catch the light —
    /// which is what reads as "lower" instead of "raised".
    ///
    /// The shading is a translucent light/shadow overlay, so the carve needs no knowledge
    /// of what it carves: fills, gradients, and translucency below all show through
    /// modulated rather than repainted.
    /// `edges` is (top, right, bottom, left): which walls of the carve actually exist.
    /// A region flush with the plate's own edge is a step, not a trough — see
    /// `push_bevel_edge_vertices_banded`.
    /// `tint` colors the wall's lit rim — the same focused-pane treatment as
    /// [`Prim::Bevel`]'s tint, for carved wells instead of raised plates. A tinted
    /// recess never groups into a host plate's CSG features (a feature carries no
    /// color), so it always renders as the free-carve overlay. Shader-plates path
    /// only; the legacy banded tessellation ignores it.
    Recess { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// The inverse of [`Prim::Recess`]: a plateau RAISED out of the surface below.
    /// Like `Recess` it emits only the shaded edges, never a fill — the face is the
    /// untouched surface underneath — so a region outlined by raised rolled bumps
    /// keeps the root plate's own color and translucency. Same wall semantics as
    /// `Recess` (`edges` = top/right/bottom/left); the lighting is the raised sign,
    /// so the edges facing `light_source_position` catch the light. `tint` colors
    /// the lit rim like [`Prim::Recess`]'s — the focused-pane treatment for a
    /// rim-only pane (a fill-less surface can't carry [`Prim::Bevel`]'s tint).
    /// Like a tinted recess it never groups into a host plate's CSG features.
    Boss { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// A raised RIM riding the rect's boundary: a bump profile straddling the
    /// outline (span ±depth/2), rising from the surrounding surface to a crest on
    /// the boundary and falling back to the same level inside — an elevated border
    /// around a channel, both faces at the underlying surface's own level. One
    /// primitive, ONE lighting evaluation per pixel: building the same shape from
    /// a Boss plus an inset Recess stacks two shading passes (double specular /
    /// shoulder terms at the crest) and reads far hotter than a plate edge.
    Ridge { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool) },
    /// The sunken twin of [`Prim::Ridge`]: a VALLEY riding the rect's boundary —
    /// a bump profile straddling the outline (span ±depth/2), falling from the
    /// surrounding surface to a trough on the boundary and rising back to the
    /// same level inside, so both faces sit at the underlying surface's own
    /// level. This is the seam a flush inset control leaves ([`PaintCtx::inset_plate`]).
    ///
    /// Same reason to exist as `Ridge`, measured: building this from a `Recess`
    /// on an outset rect plus a `Boss` on the rect (what `inset_plate` used to
    /// emit) stacks two independent shading passes. At depth 4.8 that read as a
    /// band 15px wide instead of 8 with THREE lobes — bright, dark, brighter —
    /// because the recess ring's own lit rim lands ~depth outside the control
    /// instead of merging into one wall, and the highlight peaked 22% hotter
    /// than a single evaluation of the same depth. It looked like two concentric
    /// rings, which is what it was.
    ///
    /// `edges` and the host-box fade behave exactly as [`Prim::Recess`]'s.
    /// SDF path only; the legacy banded tessellation approximates it with the
    /// old two-step stack (like `Ridge`, which approximates itself there).
    ///
    /// `tint` lights the rim like [`Prim::Recess`]'s — the focus treatment of a
    /// flush control plate (`PaintCtx::control_plate`).
    Trough { rect: Rect, radii: Radii, depth: f32, edges: (bool, bool, bool, bool), tint: Option<[f32; 3]> },
    /// The window's glass slab: a rounded fill plus a rolled, lit edge around its whole
    /// perimeter, drawn at full size. Distinct from `Bevel`, which insets its fill by
    /// `depth` — a plate must fill the window exactly, or the compositor's rounded window
    /// corners would show a gap. `depth` is the width of the roll-off in px, not a color
    /// offset (the shading amplitude is the DE-wide `bevel_depth`).
    ///
    /// `shape` overrides the DE-wide corner exponent (`layout::corner_shape`)
    /// for this one plate — `Some(2.0)` is circular arcs, so a plate whose
    /// radii reach its half-extent is a true circle regardless of the
    /// squircle the rest of the DE wears. `None` follows the DE.
    Plate { rect: Rect, radii: Radii, color: [f32; 4], depth: f32, shape: Option<f32> },
    Arc { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4] },
    /// A ring band with radial color interpolation — inner rim → crest
    /// (centerline) → outer rim — for rounded rim bevels (the Ramp's key
    /// rings). `radius` is the stroke's outer edge, like `Arc`.
    ArcShaded { cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, inner: [f32; 4], crest: [f32; 4], outer: [f32; 4] },
    Vector { x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap },
    Circle { cx: f32, cy: f32, radius: f32, color: [f32; 4] },
    /// A feathered aura around (and over) a rounded rect: the interior fills
    /// at the color's full alpha, and outside the boundary the alpha falls
    /// off smoothly to zero across `reach` px. Tessellated as concentric
    /// per-vertex-alpha rings the GPU interpolates, so the gradient is
    /// per-pixel smooth — no stacked-layer banding. Highlights and soft
    /// focus auras (the designer's drop-target glow) are the intended use;
    /// no relief shading, no light involvement.
    Glow { rect: Rect, radius: f32, reach: f32, color: [f32; 4] },
    /// A `Circle` lit as a ball: the disc is shaded per pixel as a hemisphere
    /// under the DE's plate light (same ambient/diffuse/specular model), so it
    /// reads as a sphere sitting on the surface — the slider thumb's look. The
    /// color is the sphere's face color exactly at the lit center, like a
    /// plate's face keeps the app's color. Falls back to a flat circle on the
    /// legacy (`bevel_shader 0`) path.
    Sphere { cx: f32, cy: f32, radius: f32, color: [f32; 4] },
    /// A hanging water droplet clinging to the TOP edge of `rect`, lit per pixel
    /// by shader mode 10: the silhouette is a smooth union of a film "sheet"
    /// attached to the top edge (square top corners — the attach line) and a
    /// belly capsule resting on the rect's bottom, blended metaball-style so a
    /// waist forms where the sides pull up. Shaded as a glass dome under the
    /// DE's plate light — same ambient/diffuse and decoupled specular as the
    /// plates, plus a fresnel rim crest and a thin-edge clarity falloff (tint
    /// opacity drops toward the silhouette, so the frosted backdrop shows
    /// through clearer at the rim, which is what reads as water rather than
    /// plastic). Shape knobs in [`DropletSpec`]. On the legacy (`bevel_shader
    /// 0`) path it degrades to the flat hanging capsule — square top, round
    /// bottom — rather than vanishing.
    Droplet { rect: Rect, color: [f32; 4], spec: DropletSpec },
    /// The same silhouette as [`Prim::Droplet`] under the same [`DropletSpec`],
    /// filled FLAT and feathered inward: opaque through the interior, fading
    /// to nothing over `feather` px as it approaches the drop's edge. A
    /// vignette shaped exactly like the drop, for grounding text drawn on top
    /// of one — not a second lit body, so it carries no dome, rim, gleam or
    /// contact shadow.
    ///
    /// It shares the droplet's shader path rather than approximating the
    /// outline with a rounded rect, so the two can never disagree about where
    /// the drop's edge is. On the legacy (`bevel_shader 0`) path it degrades
    /// to the same flat rounded-rect outline `Prim::Droplet` falls back to.
    DropletScrim { rect: Rect, color: [f32; 4], spec: DropletSpec, feather: f32 },
    /// A concave inside-corner fillet for composed carves: a quarter-arc wall
    /// whose centre `(cx, cy)` sits out in the corner's pocket, shaded with the
    /// same step profile as a `Recess`/`Boss` wall (`raised` flips the sign).
    /// `start` is the wedge's start angle (quarter span, hard-cut at the
    /// tangent lines — the neighboring straight walls continue the profile
    /// exactly there). Box radii can only round convex corners; this is the
    /// missing concave piece. SDF path only (no legacy fallback).
    ConcaveFillet { cx: f32, cy: f32, radius: f32, depth: f32, start: f32, raised: bool },
    /// An engraved line: a groove of half-width `width / 2` running along the
    /// segment `a`–`b`, cut into whatever is painted beneath. Like [`Prim::Recess`]
    /// it emits only shading, never a fill — but its shape is a SLAB (a band about
    /// an arbitrary line) rather than a box, which is what lets it run at an angle.
    /// A box SDF can only carve axis-aligned walls; this is the diagonal case.
    ///
    /// Both walls come from ONE profile evaluation on `|distance to the line|`, so
    /// the groove carries a single specular/shoulder term — the same reason
    /// [`Prim::Ridge`] exists instead of stacking a boss on a recess.
    /// `width` 0 makes the two walls meet in a V.
    ///
    /// `depth` is the transition width in px (the wall's run), matching
    /// [`Prim::Recess`]. `host` is the surface the groove is engraved into: the
    /// shading fades out across that box's perimeter roll, so a seam cut across a
    /// plate dies into the plate's own rolled edge instead of ending on a hard line.
    /// SDF path only — the legacy banded tessellation draws nothing (like `Ridge`).
    Groove { a: (f32, f32), b: (f32, f32), width: f32, depth: f32, host: Rect },
    /// A periodic field of identical rounded-box wells — every cell of a grid
    /// carved into whatever is painted beneath, as ONE surface. The wells
    /// repeat every `period` (x, y) with one cell centred at `origin`, each
    /// `cell` (w, h) big with `radius` corners; the wall runs from the cell
    /// edge OUTWARD over `depth` px (floor at the edge, plateau one run out),
    /// so a rail between two cells carries one wall from each side and the
    /// rail face is whatever the runs leave. Shading lands only inside `rect`.
    /// The wall's outer edge is MITRED, not offset: it is the cell grown by
    /// the run at the same `radius`, so a crossing keeps the cell's corner
    /// rounding instead of sweeping at `radius + depth`, and the four walls
    /// meet on the diagonals.
    ///
    /// This exists because a lattice drawn as one [`Prim::Recess`] per cell is
    /// N independent overlays: where four rounded rings meet at a crossing
    /// their shadings stack in colour space and read as overlapping effects,
    /// not a junction. Here the pixel is folded into the period and the
    /// distance is to the NEAREST cell — the union of every well — evaluated
    /// once, so the rail centre lines and the diagonals at each crossing are
    /// true mitres, and the cost is one draw regardless of how many cells the
    /// surface holds (a free carve per cell also runs into the per-frame
    /// feature budget long before a zoomed-out grid does). SDF path only.
    Lattice { rect: Rect, period: (f32, f32), origin: (f32, f32), cell: (f32, f32), radius: f32, depth: f32 },
    /// Several rounded boxes carved (`raised` false) or raised (`raised`
    /// true) as ONE shape: the union of the boxes is the well, and its wall
    /// follows the union's outline — straddling it by ±`depth`/2 like every
    /// carve boundary — through a single profile evaluation per pixel. An L,
    /// a T, a plus, a slot with a round end: any outline boxes can compose.
    ///
    /// The alternative, one [`Prim::Recess`] per box, is N overlays that
    /// each shade their own full outline: where two boxes overlap, each
    /// draws a wall straight through the other's interior, and where their
    /// walls cross the shadings stack in colour space — the junction reads
    /// as two effects laid over each other, not one shape. Here the pixel's
    /// distance is to the NEAREST box (the union SDF), so a box's wall
    /// vanishes wherever it runs inside another, and an inside corner is a
    /// sharp mitre (round it with [`Prim::ConcaveFillet`] if it must be
    /// concave-rounded — the union has no radius there by construction).
    /// Outer corners are mitred like [`Prim::Lattice`]'s: the wall band runs
    /// between each box shrunk and grown by half the run at the box's own
    /// radius, so a corner keeps its radius instead of sweeping wider.
    ///
    /// The boxes ride the frame's plate-feature buffer (the same slots CSG
    /// carves use, 64 per frame), so a union costs one draw plus one slot
    /// per box. When the budget cannot hold all of a union's boxes the
    /// tessellator keeps as many as fit — a degraded shape rather than none —
    /// and says so under `CCE_PLATE_DEBUG`. SDF path only.
    CarveUnion { boxes: Vec<(Rect, Radii)>, depth: f32, raised: bool },
    /// Text in sRGB u8 (the `TextLabel` convention). `font` is a font string for
    /// `get_text_buffer` (family, or "family:size"); `bounds` is a logical `[l, t, r, b]` clip
    /// for the glyph pass (Phase 6: the backend renders these through the glyph pass when the app
    /// opts in via `Application::display_list_text`; the paint walk's clip additionally
    /// applies through the item's `clip`). `attrs` carries the optional shaping attributes
    /// beyond family+size (the font picker's italic/weight preview variants). `layout`, when
    /// `Some`, requests box layout — word-wrap at a width and horizontal/vertical alignment
    /// within a box (the placed-text-box case, e.g. cce-layout-interface's canvas elements);
    /// `None` is the ordinary single-run label. `alpha` fades the glyphs (1.0 = opaque) —
    /// the color stays sRGB u8, so translucent text doesn't need a color-type change.
    Text { text: String, x: f32, y: f32, font_size: f32, color: [u8; 3], alpha: f32, font: Option<String>, bounds: Option<[f32; 4]>, attrs: TextAttrs, layout: Option<TextLayout> },
    /// A user image (id from `cce_ui::vk::upload_rgba`) drawn as a quad, in
    /// display-list order like any other primitive. The paint walk's clip
    /// applies through the item's `clip` as usual.
    Image { image: u32, rect: Rect, alpha: f32 },
}

/// Horizontal alignment of laid-out (boxed) text — the toolkit-plain mirror of
/// `cosmic_text::Align`, mapped at shape time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignH {
    #[default]
    Left,
    Center,
    Right,
}

/// Vertical alignment of laid-out text within its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum AlignV {
    #[default]
    Top,
    Middle,
    Bottom,
}

/// Box layout for a [`Prim::Text`]: word-wrap width (`Some` ⇒ multiline wrap; `None` ⇒ single
/// run) and horizontal/vertical alignment within a box of `box_height`. All lengths are logical.
/// The backend shapes an uncached buffer (`get_text_buffer_laid_out`) so the wrap/align do not
/// pollute the shared single-run cache, and applies the vertical offset from the shaped height.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLayout {
    pub wrap_width: Option<f32>,
    pub box_height: f32,
    pub align_h: AlignH,
    pub align_v: AlignV,
}

/// Optional shaping attributes for a [`Prim::Text`] — the subset a widget can request beyond
/// family + size. `weight` is the OpenType weight (400 regular, 700 bold); `None` leaves the
/// family default. Kept toolkit-plain (no cosmic-text types) like the rest of the scene layer;
/// the backend maps them onto `cosmic_text::Style`/`Weight` at shape time.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct TextAttrs {
    pub italic: bool,
    pub weight: Option<u16>,
}

/// A primitive plus the scissor rect it must be clipped to (`None` = unclipped), an
/// optional circular clip `[cx, cy, r]` in logical pixels (`None` = unclipped) — the
/// per-vertex circle clip the tessellators already support, for round panes (the designer's
/// circular network pane) — and an optional rounded-rect clip `[cx, cy, bx, by, r]`
/// (center, SDF half-extents = half-size minus radius, corner radius; logical px) so a
/// plate's children cut off at its rounded corners. The clips compose: the scissor is GPU
/// state, the circle rides the vertices, the rounded rect is per-draw-batch state.
#[derive(Clone, Debug, PartialEq)]
pub struct PaintItem {
    pub prim: Prim,
    pub clip: Option<Rect>,
    pub clip_circle: Option<[f32; 3]>,
    pub clip_rrect: Option<[f32; 5]>,
}

/// An ordered list of clipped primitives — the single source of truth for a frame's geometry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DisplayList {
    pub items: Vec<PaintItem>,
}

impl DisplayList {
    pub fn new() -> Self {
        DisplayList { items: Vec::new() }
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// Intersection of two rects, clamped so width/height never go negative (an empty clip is a
/// zero-size rect — nothing draws under it).
fn intersect(a: Rect, b: Rect) -> Rect {
    let x0 = a.x.max(b.x);
    let y0 = a.y.max(b.y);
    let x1 = (a.x + a.width).min(b.x + b.width);
    let y1 = (a.y + a.height).min(b.y + b.height);
    Rect { x: x0, y: y0, width: (x1 - x0).max(0.0), height: (y1 - y0).max(0.0) }
}

/// Accumulates a [`DisplayList`] while a paint walk pushes/pops clips and translations.
///
/// Coordinates passed to the emit methods (and to [`push_clip`](PaintCtx::push_clip)) are in the
/// **current** local space; the active translation is applied so everything recorded is absolute.
pub struct PaintCtx {
    list: DisplayList,
    /// Each entry is the effective (already-intersected, absolute) clip at that depth.
    clip_stack: Vec<Rect>,
    /// Active circular clips; primitives record the innermost (`last`). Circles don't
    /// intersect analytically like rects, so nesting keeps the innermost only.
    clip_circle_stack: Vec<[f32; 3]>,
    /// Active rounded-rect clips `[cx, cy, bx, by, r]`; innermost wins, like circles.
    clip_rrect_stack: Vec<[f32; 5]>,
    /// Saved offsets for nesting; `offset` is the current cumulative translation.
    offset_stack: Vec<(f32, f32)>,
    offset: (f32, f32),
}

impl Default for PaintCtx {
    fn default() -> Self {
        Self::new()
    }
}

impl PaintCtx {
    pub fn new() -> Self {
        PaintCtx {
            list: DisplayList::new(),
            clip_stack: Vec::new(),
            clip_circle_stack: Vec::new(),
            clip_rrect_stack: Vec::new(),
            offset_stack: Vec::new(),
            offset: (0.0, 0.0),
        }
    }

    /// The scissor rect primitives are currently recorded under.
    pub fn current_clip(&self) -> Option<Rect> {
        self.clip_stack.last().copied()
    }

    /// Push a clip (in current local space); it is translated to absolute and intersected with the
    /// enclosing clip. Pair with [`pop_clip`](PaintCtx::pop_clip), or prefer [`clip`](PaintCtx::clip).
    pub fn push_clip(&mut self, rect: Rect) {
        let r = self.apply_offset(rect);
        let effective = match self.clip_stack.last() {
            Some(cur) => intersect(*cur, r),
            None => r,
        };
        self.clip_stack.push(effective);
    }

    pub fn pop_clip(&mut self) {
        self.clip_stack.pop();
    }

    /// Run `f` with `rect` pushed as a clip, popping it afterward.
    pub fn clip<R>(&mut self, rect: Rect, f: impl FnOnce(&mut Self) -> R) -> R {
        self.push_clip(rect);
        let out = f(self);
        self.pop_clip();
        out
    }

    /// Push a circular clip `[cx, cy, r]` (current local space, translated to absolute).
    /// Primitives emitted while it is active record it and tessellate with the per-vertex
    /// circle clip. Pair with [`pop_clip_circle`](PaintCtx::pop_clip_circle), or prefer
    /// [`clip_circle`](PaintCtx::clip_circle).
    pub fn push_clip_circle(&mut self, c: [f32; 3]) {
        self.clip_circle_stack.push([c[0] + self.offset.0, c[1] + self.offset.1, c[2]]);
    }

    pub fn pop_clip_circle(&mut self) {
        self.clip_circle_stack.pop();
    }

    /// Run `f` with `[cx, cy, r]` pushed as a circular clip, popping it afterward.
    pub fn clip_circle<R>(&mut self, c: [f32; 3], f: impl FnOnce(&mut Self) -> R) -> R {
        self.push_clip_circle(c);
        let out = f(self);
        self.pop_clip_circle();
        out
    }

    /// Push a rounded-rect clip: `rect` (current local space) with corner radius `radius`,
    /// so children of a rounded plate cut off at its corners. Pushes the rect as a scissor
    /// too — the scissor handles the straight edges (and keeps batching), the SDF trims the
    /// corners. A radius of zero degenerates to the plain rect clip. Pair with
    /// [`pop_clip_rounded`](PaintCtx::pop_clip_rounded), or prefer
    /// [`clip_rounded`](PaintCtx::clip_rounded).
    pub fn push_clip_rounded(&mut self, rect: Rect, radius: f32) {
        self.push_clip(rect);
        let r = radius.max(0.0);
        if r > 0.0 {
            let abs = self.apply_offset(rect);
            self.clip_rrect_stack.push([
                abs.x + abs.width / 2.0,
                abs.y + abs.height / 2.0,
                (abs.width / 2.0 - r).max(0.0),
                (abs.height / 2.0 - r).max(0.0),
                r,
            ]);
        } else {
            // Keep push/pop balanced regardless of radius.
            self.clip_rrect_stack.push([0.0; 5]);
        }
    }

    pub fn pop_clip_rounded(&mut self) {
        self.clip_rrect_stack.pop();
        self.pop_clip();
    }

    /// Run `f` with `rect` (radius `radius`) pushed as a rounded clip, popping it afterward.
    pub fn clip_rounded<R>(&mut self, rect: Rect, radius: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.push_clip_rounded(rect, radius);
        let out = f(self);
        self.pop_clip_rounded();
        out
    }

    /// Run `f` with an additional translation applied to all emitted coordinates.
    /// Imperative translate pair for spans too large to wrap in
    /// [`translate`](Self::translate)'s closure (an app bracketing its whole
    /// frame in the overflow-margin shift). Must balance before `finish`.
    pub fn push_translate(&mut self, dx: f32, dy: f32) {
        self.offset_stack.push(self.offset);
        self.offset.0 += dx;
        self.offset.1 += dy;
    }

    /// See [`push_translate`](Self::push_translate).
    pub fn pop_translate(&mut self) {
        self.offset = self.offset_stack.pop().expect("translate stack underflow");
    }

    pub fn translate<R>(&mut self, dx: f32, dy: f32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.offset_stack.push(self.offset);
        self.offset = (self.offset.0 + dx, self.offset.1 + dy);
        let out = f(self);
        self.offset = self.offset_stack.pop().expect("translate stack underflow");
        out
    }

    fn apply_offset(&self, r: Rect) -> Rect {
        Rect { x: r.x + self.offset.0, y: r.y + self.offset.1, width: r.width, height: r.height }
    }

    fn push(&mut self, prim: Prim) {
        let clip = self.current_clip();
        let clip_circle = self.clip_circle_stack.last().copied();
        // r == 0 entries are balance placeholders (a zero-radius rounded clip is just its
        // scissor rect) — record no rounded clip so batches keep merging.
        let clip_rrect = self.clip_rrect_stack.last().copied().filter(|c| c[4] > 0.0);
        self.list.items.push(PaintItem { prim, clip, clip_circle, clip_rrect });
    }

    pub fn quad(&mut self, rect: Rect, color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Quad { rect, color });
    }

    /// A user image (id from `cce_ui::vk::upload_rgba`) drawn at `rect`.
    pub fn image(&mut self, image: u32, rect: Rect, alpha: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Image { image, rect, alpha });
    }

    pub fn rounded_rect(&mut self, rect: Rect, radius: f32, corners: (bool, bool, bool, bool), color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::RoundedRect { rect, radius, corners, color });
    }

    /// Feathered aura over a rounded rect (see [`Prim::Glow`]): interior at
    /// the color's alpha, smooth per-pixel falloff to zero across `reach` px
    /// outside the boundary.
    pub fn glow(&mut self, rect: Rect, radius: f32, reach: f32, color: [f32; 4]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Glow { rect, radius, reach, color });
    }

    pub fn vector(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, thickness: f32, color: [f32; 4], cap: Cap) {
        let (ox, oy) = self.offset;
        self.push(Prim::Vector { x1: x1 + ox, y1: y1 + oy, x2: x2 + ox, y2: y2 + oy, thickness, color, cap });
    }

    pub fn circle(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Circle { cx: cx + ox, cy: cy + oy, radius, color });
    }

    /// A sphere-lit circle — see `Prim::Sphere`.
    pub fn sphere(&mut self, cx: f32, cy: f32, radius: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Sphere { cx: cx + ox, cy: cy + oy, radius, color });
    }

    /// A hanging water droplet clinging to `rect`'s top edge — see
    /// [`Prim::Droplet`] and [`DropletSpec`].
    /// See [`Prim::DropletScrim`]. `feather` is how far in from the drop's
    /// edge the fill reaches full opacity, in logical px.
    pub fn droplet_scrim(&mut self, rect: Rect, color: [f32; 4], spec: DropletSpec, feather: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::DropletScrim { rect, color, spec, feather });
    }

    pub fn droplet(&mut self, rect: Rect, color: [f32; 4], spec: DropletSpec) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Droplet { rect, color, spec });
    }

    /// A concave inside-corner fillet — see `Prim::ConcaveFillet`. `start` is
    /// the quarter wedge's start angle; the arc's centre sits in the corner's
    /// pocket and the wall descends (or rises, `raised`) away from it.
    pub fn concave_fillet(&mut self, cx: f32, cy: f32, radius: f32, depth: f32, start: f32, raised: bool) {
        let (ox, oy) = self.offset;
        self.push(Prim::ConcaveFillet { cx: cx + ox, cy: cy + oy, radius, depth, start, raised });
    }

    /// Re-emit an already-built [`Prim`] through this context, so it re-records the
    /// current clip and translate state. This is the **single** place that has to
    /// learn a new `Prim` variant: a nested paint walk builds a scratch list and
    /// replays it into the real one, and that forwarding match used to exist
    /// verbatim in two crates ([`crate::widget::model`] and cce-cloud's
    /// `json_layout`) — adding `Prim::Groove` compiled against one and broke the
    /// other, caught only by a full workspace build.
    ///
    /// [`Prim::Text`] is NOT emitted: it is returned untouched, because the two
    /// callers disagree about it (a subtree painter authors its own text and wants
    /// it forwarded; everyone else drops it in favour of the widget's own label
    /// bridge). Every other variant is emitted and `None` comes back.
    #[must_use = "a returned Text prim was not emitted — drop or forward it explicitly"]
    pub fn replay(&mut self, prim: Prim) -> Option<Prim> {
        match prim {
            Prim::Text { .. } => return Some(prim),
            Prim::Quad { rect, color } => self.quad(rect, color),
            Prim::RoundedRect { rect, radius, corners, color } => {
                self.rounded_rect(rect, radius, corners, color)
            }
            Prim::Border { rect, radii, fill, border, thickness } => {
                self.border(rect, radii, fill, border, thickness)
            }
            Prim::Bevel { rect, radii, color, depth, tint } => {
                self.bevel_tinted(rect, radii, color, depth, tint)
            }
            Prim::Recess { rect, radii, depth, edges, tint } => match tint {
                Some(t) => self.recess_tinted(rect, radii, depth, t),
                None => self.recess_edges(rect, radii, depth, edges),
            },
            Prim::Boss { rect, radii, depth, edges, tint } => match tint {
                Some(t) => self.boss_edges_tinted(rect, radii, depth, edges, t),
                None => self.boss_edges(rect, radii, depth, edges),
            },
            Prim::Ridge { rect, radii, depth, edges } => self.ridge_edges(rect, radii, depth, edges),
            Prim::Trough { rect, radii, depth, edges, tint } => match tint {
                Some(t) => self.trough_tinted(rect, radii, depth, t),
                None => self.trough_edges(rect, radii, depth, edges),
            },
            Prim::Plate { rect, radii, color, depth, shape } => {
                self.plate_shaped(rect, radii, color, depth, shape)
            }
            Prim::Arc { cx, cy, radius, thickness, start, end, color } => {
                self.arc(cx, cy, radius, thickness, start, end, color)
            }
            Prim::ArcShaded { cx, cy, radius, thickness, start, end, inner, crest, outer } => {
                self.arc_shaded(cx, cy, radius, thickness, start, end, inner, crest, outer)
            }
            Prim::Vector { x1, y1, x2, y2, thickness, color, cap } => {
                self.vector(x1, y1, x2, y2, thickness, color, cap)
            }
            Prim::Circle { cx, cy, radius, color } => self.circle(cx, cy, radius, color),
            Prim::Sphere { cx, cy, radius, color } => self.sphere(cx, cy, radius, color),
            Prim::Glow { rect, radius, reach, color } => self.glow(rect, radius, reach, color),
            Prim::Droplet { rect, color, spec } => self.droplet(rect, color, spec),
            Prim::DropletScrim { rect, color, spec, feather } => {
                self.droplet_scrim(rect, color, spec, feather)
            }
            Prim::ConcaveFillet { cx, cy, radius, depth, start, raised } => {
                self.concave_fillet(cx, cy, radius, depth, start, raised)
            }
            Prim::Groove { a, b, width, depth, host } => self.groove(a, b, width, depth, host),
            Prim::Lattice { rect, period, origin, cell, radius, depth } => {
                self.lattice(rect, period, origin, cell, radius, depth)
            }
            Prim::CarveUnion { boxes, depth, raised } => self.carve_union(boxes, depth, raised),
            Prim::Image { image, rect, alpha } => self.image(image, rect, alpha),
        }
        None
    }

    /// An engraved line from `a` to `b` cut into `host` — see [`Prim::Groove`].
    pub fn groove(&mut self, a: (f32, f32), b: (f32, f32), width: f32, depth: f32, host: Rect) {
        let (ox, oy) = self.offset;
        let host = self.apply_offset(host);
        self.push(Prim::Groove {
            a: (a.0 + ox, a.1 + oy),
            b: (b.0 + ox, b.1 + oy),
            width,
            depth,
            host,
        });
    }

    /// A periodic field of rounded wells carved as one surface — see
    /// [`Prim::Lattice`]. `origin` is any one cell's centre; `rect` bounds the
    /// shading. All logical px, like every other carve.
    pub fn lattice(
        &mut self,
        rect: Rect,
        period: (f32, f32),
        origin: (f32, f32),
        cell: (f32, f32),
        radius: f32,
        depth: f32,
    ) {
        let (ox, oy) = self.offset;
        let rect = self.apply_offset(rect);
        self.push(Prim::Lattice { rect, period, origin: (origin.0 + ox, origin.1 + oy), cell, radius, depth });
    }

    /// Carve (or raise, with `raised`) the union of `boxes` as one shape with
    /// one wall — see [`Prim::CarveUnion`]. `depth` is the wall's run in px,
    /// as for [`PaintCtx::recess`].
    pub fn carve_union(&mut self, boxes: Vec<(Rect, Radii)>, depth: f32, raised: bool) {
        let boxes: Vec<(Rect, Radii)> = boxes.into_iter().map(|(r, radii)| (self.apply_offset(r), radii)).collect();
        if boxes.is_empty() {
            return;
        }
        self.push(Prim::CarveUnion { boxes, depth, raised });
    }

    pub fn border(&mut self, rect: Rect, radii: Radii, fill: [f32; 4], border: [f32; 4], thickness: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Border { rect, radii, fill, border, thickness });
    }

    pub fn bevel(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        self.bevel_tinted(rect, radii, color, depth, [1.0, 1.0, 1.0]);
    }

    /// `bevel` with a specular tint — see `Prim::Bevel::tint`.
    pub fn bevel_tinted(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32, tint: [f32; 3]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Bevel { rect, radii, color, depth, tint });
    }

    /// Carve a recess into the already-painted surface below. Unlike `bevel`, this fills
    /// nothing — the shading is an overlay, so it composes over whatever was painted.
    pub fn recess(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.recess_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::recess`] with the lit rim tinted — see `Prim::Recess::tint`
    /// (the focused-well treatment).
    pub fn recess_tinted(&mut self, rect: Rect, radii: Radii, depth: f32, tint: [f32; 3]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Recess { rect, radii, depth, edges: (true, true, true, true), tint: Some(tint) });
    }

    /// Raise a plateau out of the already-painted surface below — the inverse of
    /// [`PaintCtx::recess`]. Only the edges are shaded; the face stays the surface
    /// beneath, so the raised region inherits the root plate's color. `depth` is the
    /// roll width in px (pass [`crate::layout::bevel_width`] unless the widget
    /// needs a tighter lip).
    pub fn boss(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.boss_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::boss`] with only some of the walls — see `Prim::Boss`.
    pub fn boss_edges(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Boss { rect, radii, depth, edges, tint: None });
    }

    /// [`PaintCtx::boss_edges`] with a specular tint on the lit rim — see
    /// `Prim::Boss::tint`.
    pub fn boss_edges_tinted(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
        tint: [f32; 3],
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Boss { rect, radii, depth, edges, tint: Some(tint) });
    }

    /// Paint a control plate — see [`ControlPlate`]. The ONE place a control face's
    /// relief is composed: raised with a face is a `bevel` on the footprint;
    /// raised without one carves inside and raises a `boss`; flush carves
    /// inside and lays an `inset_plate` (trough plus face).
    pub fn control_plate(&mut self, plate: &ControlPlate) {
        match plate.stance {
            PlateStance::Raised => {
                // abs(): a negative alpha is the frost sentinel, a real face.
                if plate.face[3].abs() > 0.001 {
                    match plate.tint {
                        Some(t) => self.bevel_tinted(plate.rect, plate.radii, plate.face, plate.depth, t),
                        None => self.bevel(plate.rect, plate.radii, plate.face, plate.depth),
                    }
                } else {
                    let (plateau, radii) = crate::layout::carve_inside(plate.rect, plate.radii, plate.depth);
                    match plate.tint {
                        Some(t) => self.boss_edges_tinted(plateau, radii, plate.depth, (true, true, true, true), t),
                        None => self.boss(plateau, radii, plate.depth),
                    }
                }
            }
            PlateStance::Flush => {
                let (trough, radii) = crate::layout::carve_inside(plate.rect, plate.radii, plate.depth);
                match plate.tint {
                    Some(t) => self.inset_plate_tinted(trough, radii, plate.face, plate.depth, t),
                    None => self.inset_plate(trough, radii, plate.face, plate.depth),
                }
            }
        }
    }

    /// A section's well — the settings app's union carve, the ONE shape a
    /// section or a [`crate::widget::Group`] is cut into the plate with: the
    /// `body` carved as a recess with `radii` (TL, TR, BR, BL), and when there
    /// is a title `tab` (flush on the body's top edge, at its left), the tab
    /// carved WITH it as one shape — the tab bottom-open, one piece owning the
    /// body's whole right run so its corners are real turns, a left piece
    /// carrying the left wall, the pieces extending past their interior seam by
    /// `depth` so the walls crossfade there instead of notching — and the
    /// throat's inside corner rounded by a concave fillet.
    pub fn section_well(&mut self, body: Rect, tab: Option<Rect>, radii: Radii, depth: f32) {
        let (cx, cy, cw, ch) = (body.x, body.y, body.width, body.height);
        let (tl, tr, br, bl) = radii;
        let Some(t) = tab else {
            self.recess_edges(body, radii, depth, (true, true, true, true));
            return;
        };
        let (tx, ty, tw, th) = (t.x, t.y, t.width, t.height);
        let rt = tl.max(tr).min(th * 0.45);
        let throat_r = tx + tw;
        // The designer's SECTION_FILLET_R.
        let rho = 10.0f32;
        let body_lr = |x_run: f32, pc: &mut Self| {
            pc.recess_edges(
                Rect { x: x_run, y: cy, width: cx + cw - x_run, height: ch },
                (0.0, tr, br, 0.0),
                depth,
                (true, true, true, false),
            );
            pc.recess_edges(
                Rect { x: cx, y: cy, width: x_run + depth - cx, height: ch },
                (0.0, 0.0, 0.0, bl),
                depth,
                (false, false, true, true),
            );
        };
        if cx + cw > throat_r + 2.0 * rho {
            // Filleted throat: the tab's right wall ends at the fillet's vertical
            // tangent, a left-only bridge carries the left wall across the span.
            self.recess_edges(
                Rect { x: tx, y: ty, width: tw, height: (cy - rho) - ty + depth },
                (rt, rt, 0.0, 0.0),
                depth,
                (true, true, false, true),
            );
            self.recess_edges(
                Rect { x: tx, y: cy - rho, width: tw, height: rho + depth },
                (0.0, 0.0, 0.0, 0.0),
                depth,
                (false, false, false, true),
            );
            body_lr(throat_r + rho - depth, self);
            self.concave_fillet(throat_r + rho, cy - rho, rho, depth, std::f32::consts::FRAC_PI_2, false);
        } else if cx + cw > throat_r + 0.5 {
            // Too narrow for the fillet: the plain square throat.
            self.recess_edges(
                Rect { x: tx, y: ty, width: tw, height: (cy - ty) + depth },
                (rt, rt, 0.0, 0.0),
                depth,
                (true, true, false, true),
            );
            body_lr(throat_r - depth, self);
        } else {
            // The tab spans the body: no top wall at all.
            self.recess_edges(
                Rect { x: tx, y: ty, width: tw, height: (cy - ty) + depth },
                (rt, rt, 0.0, 0.0),
                depth,
                (true, true, false, true),
            );
            self.recess_edges(
                Rect { x: cx, y: cy, width: cw, height: ch },
                (0.0, 0.0, br, bl),
                depth,
                (false, true, true, true),
            );
        }
    }

    /// A flush inset control: `rect`'s plate sits SUNKEN into the surface with
    /// its face level with it — a valley seam runs the boundary, the surface
    /// falling into it on the way out and the control's own face rising back
    /// out of it inside. The face never leaves the surface plane; the seam is
    /// the only thing saying it is a separate part. `depth` is the full width
    /// of that valley, which straddles the boundary by ±depth/2.
    ///
    /// One [`Prim::Trough`] — ONE lighting evaluation. This used to emit a
    /// `Recess` on a rect outset by depth/2 plus a `Boss` on the rect, whose
    /// walls overlapped over half their width and shaded twice; see
    /// `Prim::Trough` for what that measured as. Do not re-expand this into its
    /// parts.
    ///
    /// An opaque `color` fills the face; transparent leaves the surface below
    /// showing through as the face.
    pub fn inset_plate(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        // abs(): negative alpha is the blur-behind frost sentinel, a real
        // face — only a genuinely transparent color skips the fill.
        if color[3].abs() > 0.001 {
            // Flat fill only — the relief is the trough's, so the face must not
            // carry a lip of its own (that lip WAS the second wall).
            //
            // Deliberately a zero-stroke `Border` and NOT `rounded_rect`: this
            // fill used to be a `Bevel`, and the legacy reverse bridges
            // (`all_rounded_quads` and friends in `widget/model.rs`) extract
            // `Prim::RoundedRect` but neither `Bevel` nor `Border`. Emitting a
            // RoundedRect here would newly leak every raised control's face into
            // those getters — a change to the legacy surface that has nothing to
            // do with the relief. Border also keeps all four radii, which
            // `Prim::RoundedRect`'s single radius cannot.
            self.border(rect, radii, color, [0.0; 4], 0.0);
        }
        self.trough(rect, radii, depth);
    }

    /// [`inset_plate`](Self::inset_plate) with the rim lit — the focused flush
    /// control plate's ring (`ControlPlate::with_tint`); the face fill as
    /// there, the trough tinted.
    pub fn inset_plate_tinted(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32, tint: [f32; 3]) {
        if color[3].abs() > 0.001 {
            self.border(rect, radii, color, [0.0; 4], 0.0);
        }
        self.trough_tinted(rect, radii, depth, tint);
    }

    /// A canvas well's floor — the opening you look into or draw in (a
    /// Trackpad, a Slider2D pad, a bevel or ramp preview). It darkens the plate
    /// it is cut from ([`crate::colors::WELL_FLOOR`]) rather than painting a
    /// floor of its own, so every canvas sits in the one material. `lifted` is
    /// a clickable canvas's hover cue: the floor rises toward the plate.
    pub fn well_floor(&mut self, rect: Rect, radius: f32, lifted: bool) {
        let tint = if lifted { crate::colors::WELL_FLOOR_LIFTED } else { crate::colors::WELL_FLOOR };
        self.rounded_rect(rect, radius, (true, true, true, true), tint);
    }

    /// A canvas well's rim, drawn AFTER the content so the wall's shading falls
    /// over whatever runs to the edge. Under `relief` it is the recess carved
    /// inside `rect` ([`crate::layout::carve_inside`], the wall the DE width
    /// capped at a fifth of the height — every well's rule); flat, the
    /// hairline frame every well shares ([`crate::colors::well_frame_color`]).
    pub fn well_rim(&mut self, rect: Rect, radius: f32, relief: bool) {
        let radii = (radius, radius, radius, radius);
        if relief {
            let depth = crate::layout::bevel_width().min(rect.height * 0.2);
            let (well, radii) = crate::layout::carve_inside(rect, radii, depth);
            self.recess(well, radii, depth);
        } else {
            self.border(rect, radii, [0.0; 4], crate::colors::well_frame_color(false, false), 1.0);
        }
    }

    /// [`well_floor`](Self::well_floor) then [`well_rim`](Self::well_rim) in
    /// one call — a canvas whose content is drawn over the rim (a Trackpad's
    /// fingers). Content that should slide under the wall draws between the two.
    pub fn canvas_well(&mut self, rect: Rect, radius: f32, relief: bool, lifted: bool) {
        self.well_floor(rect, radius, lifted);
        self.well_rim(rect, radius, relief);
    }

    /// Emit one [`crate::layout::ReliefCarve`]. The shared application point:
    /// a widget's `paint` carves through here, and a flat host re-emits the
    /// carves it collected through here too, so the two can only ever draw the
    /// same prim.
    ///
    /// A tinted recess takes `recess_tinted`, which lights the whole rim — it
    /// is the focus treatment, and every tinted carve the toolkit emits is a
    /// full ring. A partial ring falls back to the untinted walls rather than
    /// silently tinting walls the caller suppressed.
    pub fn carve(&mut self, c: &crate::layout::ReliefCarve) {
        let rect = Rect { x: c.x, y: c.y, width: c.w, height: c.h };
        match c.kind {
            crate::layout::CarveKind::Boss { tint: Some(t) } if c.edges == (true, true, true, true) => {
                self.boss_edges_tinted(rect, c.radii, c.depth, c.edges, t)
            }
            crate::layout::CarveKind::Boss { .. } => self.boss_edges(rect, c.radii, c.depth, c.edges),
            crate::layout::CarveKind::Recess { tint: Some(t) }
                if c.edges == (true, true, true, true) =>
            {
                self.recess_tinted(rect, c.radii, c.depth, t)
            }
            crate::layout::CarveKind::Recess { .. } => {
                self.recess_edges(rect, c.radii, c.depth, c.edges)
            }
            crate::layout::CarveKind::Trough => self.trough_edges(rect, c.radii, c.depth, c.edges),
        }
    }

    /// Sink a valley along `rect`'s boundary — see [`Prim::Trough`]. `depth` is
    /// the full width of the seam (it straddles the outline by ±depth/2).
    pub fn trough(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.trough_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::trough`] with only some of the walls — see [`Prim::Trough`].
    pub fn trough_edges(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Trough { rect, radii, depth, edges, tint: None });
    }

    /// [`PaintCtx::trough`] with the rim lit — see `Prim::Trough::tint` (the
    /// focused flush control plate).
    pub fn trough_tinted(&mut self, rect: Rect, radii: Radii, depth: f32, tint: [f32; 3]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Trough { rect, radii, depth, edges: (true, true, true, true), tint: Some(tint) });
    }

    /// Raise a rim along `rect`'s boundary — see `Prim::Ridge`. `depth` is the
    /// full width of the bump (it straddles the outline by ±depth/2).
    pub fn ridge(&mut self, rect: Rect, radii: Radii, depth: f32) {
        self.ridge_edges(rect, radii, depth, (true, true, true, true));
    }

    /// [`PaintCtx::ridge`] with only some of the walls — see `Prim::Ridge`.
    pub fn ridge_edges(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Ridge { rect, radii, depth, edges });
    }

    /// [`PaintCtx::recess`] with only some of the walls — see `Prim::Recess`.
    pub fn recess_edges(
        &mut self, rect: Rect, radii: Radii, depth: f32,
        edges: (bool, bool, bool, bool),
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Recess { rect, radii, depth, edges, tint: None });
    }

    /// The window's glass slab: rounded fill at full size plus a rolled, lit perimeter.
    /// `depth` is the roll-off width in px — pass [`crate::layout::bevel_width`] unless the
    /// window wants a shallower edge than the DE default.
    ///
    /// A NEGATIVE `depth` is the fill-less sentinel: no fill is drawn, and the
    /// rolled perimeter (width `-depth`) renders as an overlay — translucent
    /// white screen / black multiply — over whatever is beneath, for a root
    /// plate whose face is not a fill (the designer's full-bleed 3D canvas).
    /// `color` is ignored; the roll profile, crest and specular are exactly the
    /// positive-depth plate's.
    pub fn plate(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32) {
        self.plate_shaped(rect, radii, color, depth, None);
    }

    /// [`plate`](Self::plate) with an explicit corner exponent — see
    /// [`Prim::Plate`]'s `shape`. `Some(2.0)` on a plate whose radii are its
    /// half-extent draws a circle; `None` is exactly `plate`.
    pub fn plate_shaped(&mut self, rect: Rect, radii: Radii, color: [f32; 4], depth: f32, shape: Option<f32>) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Plate { rect, radii, color, depth, shape });
    }

    /// Emit the plate a [`PlateSpec`] describes: role-resolved per-corner
    /// radii and role-encoded frost (RFC Phase 7b).
    ///
    /// The spec's radii are FINAL on-screen values (a window corner already
    /// wears the full silhouette span), but `Prim::Plate` speaks the older
    /// convention — NOMINAL radii, span applied downstream by
    /// `plate_push_raised(scale_corners = true)`, which the unmigrated
    /// hand-rolled plates (cce-cloud, the test-interface gallery shim) still
    /// rely on. So divide the span back out here and let the push multiply
    /// reconstruct the spec's exact values.
    ///
    /// Feeding the final radii straight through double-spanned every window
    /// corner (12 → ~100 logical at corner_shape 4.5): the plate arc pulled
    /// away from the compositor's clip, the black window background showed
    /// through as a corner crescent, and the corners stopped matching the
    /// desktop grid — the original 7b-2 report of this looking like "the arc
    /// correction" was the regression itself.
    pub fn plate_spec(&mut self, spec: &PlateSpec) {
        let f = crate::layout::corner_span_factor();
        let (tl, tr, br, bl) = spec.radii();
        self.plate(spec.rect, (tl / f, tr / f, br / f, bl / f), spec.fill(), spec.depth);
    }

    pub fn arc(&mut self, cx: f32, cy: f32, radius: f32, thickness: f32, start: f32, end: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        self.push(Prim::Arc { cx: cx + ox, cy: cy + oy, radius, thickness, start, end, color });
    }

    /// A radially-shaded ring band — see [`Prim::ArcShaded`].
    #[allow(clippy::too_many_arguments)]
    pub fn arc_shaded(
        &mut self,
        cx: f32,
        cy: f32,
        radius: f32,
        thickness: f32,
        start: f32,
        end: f32,
        inner: [f32; 4],
        crest: [f32; 4],
        outer: [f32; 4],
    ) {
        let (ox, oy) = self.offset;
        self.push(Prim::ArcShaded {
            cx: cx + ox,
            cy: cy + oy,
            radius,
            thickness,
            start,
            end,
            inner,
            crest,
            outer,
        });
    }

    pub fn text(&mut self, text: impl Into<String>, x: f32, y: f32, font_size: f32, color: [u8; 3]) {
        self.text_with(text, x, y, font_size, color, None, None);
    }

    /// Text with a per-label font and clip rect (`[l, t, r, b]`, local space) — what the
    /// legacy `text_labels_with_font_and_bounds` tuples carry, expressible in the display
    /// list since Phase 6.
    pub fn text_with(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
    ) {
        self.text_attrs(text, x, y, font_size, color, font, bounds, TextAttrs::default());
    }

    /// [`text_with`](PaintCtx::text_with) plus shaping attributes (italic / weight) — what the
    /// font picker's style-variant previews need beyond family + size.
    #[allow(clippy::too_many_arguments)]
    pub fn text_attrs(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
        attrs: TextAttrs,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text { text: text.into(), x: x + ox, y: y + oy, font_size, color, alpha: 1.0, font, bounds, attrs, layout: None });
    }

    /// [`text_with`](PaintCtx::text_with) plus a glyph alpha (1.0 = opaque) — translucent
    /// labels (a pane fading out) without changing the sRGB u8 color convention.
    #[allow(clippy::too_many_arguments)]
    pub fn text_faded(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        alpha: f32,
        font: Option<String>,
        bounds: Option<[f32; 4]>,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text {
            text: text.into(),
            x: x + ox,
            y: y + oy,
            font_size,
            color,
            alpha,
            font,
            bounds,
            attrs: TextAttrs::default(),
            layout: None,
        });
    }

    /// Boxed text: word-wrap + horizontal/vertical alignment within a box (a placed text box).
    /// Unlike [`text_with`](PaintCtx::text_with), the backend shapes this uncached with the box
    /// layout applied. `x, y` are the box's top-left; the backend applies the vertical offset.
    #[allow(clippy::too_many_arguments)]
    pub fn text_boxed(
        &mut self,
        text: impl Into<String>,
        x: f32,
        y: f32,
        font_size: f32,
        color: [u8; 3],
        font: Option<String>,
        bounds: Option<[f32; 4]>,
        attrs: TextAttrs,
        layout: TextLayout,
    ) {
        let (ox, oy) = self.offset;
        let bounds = bounds.map(|[l, t, r, b]| [l + ox, t + oy, r + ox, b + oy]);
        self.push(Prim::Text {
            text: text.into(),
            x: x + ox,
            y: y + oy,
            font_size,
            color,
            alpha: 1.0,
            font,
            bounds,
            attrs,
            layout: Some(layout),
        });
    }

    /// Consume the context and return the accumulated display list.
    pub fn finish(self) -> DisplayList {
        debug_assert!(self.clip_stack.is_empty(), "unbalanced push_clip/pop_clip");
        debug_assert!(self.offset_stack.is_empty(), "unbalanced translate");
        self.list
    }
}

/// `PaintCtx` as a popover render target: display-list hosts pass their frame
/// ctx straight into `render_popover`, so popovers draw REAL prims — relief
/// plates, rounded rects, bounded text — instead of the flattened
/// `PopoverCollector` view (which stays for legacy tuple hosts).
impl crate::layout::RenderTarget for PaintCtx {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.quad(Rect { x, y, width: w, height: h }, color);
    }
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32) {
        self.rounded_rect(Rect { x, y, width: w, height: h }, radius, (true, true, true, true), color);
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, corners: (bool, bool, bool, bool)) {
        self.rounded_rect(Rect { x, y, width: w, height: h }, radius, corners, color);
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        PaintCtx::text(self, content, x, y, size, c);
    }
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        crate::layout::RenderTarget::text_with_font_and_bounds(self, content, x, y, size, color, font, None);
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        self.text_with(content, x, y, size, c, None, bounds);
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        let c = [
            (color[0] * 255.0).clamp(0.0, 255.0) as u8,
            (color[1] * 255.0).clamp(0.0, 255.0) as u8,
            (color[2] * 255.0).clamp(0.0, 255.0) as u8,
        ];
        self.text_with(content, x, y, size, c, Some(font.to_string()), bounds);
    }
    fn push_clip_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.push_clip(Rect { x, y, width: w, height: h });
    }
    fn pop_clip_rect(&mut self) {
        self.pop_clip();
    }
    fn inset_plate(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32) {
        PaintCtx::inset_plate(self, Rect { x, y, width: w, height: h }, (radius, radius, radius, radius), color, depth);
    }
    fn inset_plate_tinted(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, depth: f32, tint: [f32; 3]) {
        PaintCtx::inset_plate_tinted(self, Rect { x, y, width: w, height: h }, (radius, radius, radius, radius), color, depth, tint);
    }
    fn relief_carve(&mut self, carve: &crate::layout::ReliefCarve) {
        PaintCtx::carve(self, carve);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC Phase 7b: PlateSpec role mechanics — flag derivation from window
    /// geometry, silhouette-vs-nominal radii selection, and the role-encoded
    /// frost (root positive-alpha, nested negative-alpha sentinel).
    #[test]
    fn plate_spec_roles() {
        // Flags: a full-window rect is root; an inset pane has none; a pane
        // flush to the window's right edge owns the two right corners.
        let root_flags = PlateSpec::window_corner_flags(
            Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 }, 800.0, 600.0);
        assert_eq!(root_flags, (true, true, true, true));
        let inset = PlateSpec::window_corner_flags(
            Rect { x: 20.0, y: 20.0, width: 100.0, height: 100.0 }, 800.0, 600.0);
        assert_eq!(inset, (false, false, false, false));
        let right_pane = PlateSpec::window_corner_flags(
            Rect { x: 500.0, y: 0.0, width: 300.0, height: 600.0 }, 800.0, 600.0);
        assert_eq!(right_pane, (false, true, true, false));

        // Radii: flagged corners wear the shared silhouette curve, interior
        // ones the nominal plate radius (compared against the same getters,
        // so the assertion holds for any configured values).
        let window_r =
            crate::layout::window_corner_radius() * crate::layout::corner_span_factor();
        let nominal = crate::layout::plate_corner_radius();
        let r = PlateSpec::radii_for((false, true, true, false));
        assert_eq!(r, (nominal, window_r, window_r, nominal));

        // Frost encoding by role.
        let mut spec = PlateSpec {
            rect: Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 },
            color: [0.1, 0.2, 0.3, 0.8],
            blur: true,
            window_corners: (true, true, true, true),
            depth: 3.0,
        };
        assert!(spec.is_root());
        assert!(spec.fill()[3] > 0.0, "root frost is the compositor's; alpha stays positive");
        spec.window_corners = (false, true, true, false);
        assert!(!spec.is_root());
        assert!(spec.fill()[3] < 0.0, "nested frost = negative-alpha sentinel");
        spec.blur = false;
        assert_eq!(spec.fill()[3], 0.8, "no frost, no encoding");

        // The detach role flip (RFC 7c): a frosted nested pane becomes a
        // root — silhouette corners, and the frost regime flips from the
        // in-app sentinel to the compositor's (alpha back to positive).
        spec.blur = true;
        assert!(spec.fill()[3] < 0.0);
        let det = spec.detached();
        assert!(det.is_root());
        assert!(det.fill()[3] > 0.0, "root frost is the compositor's again");
        let wr = crate::layout::window_silhouette_radius();
        assert_eq!(det.radii(), (wr, wr, wr, wr));
    }

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    /// The emission round-trip: `plate_spec` pre-divides by the span factor so
    /// `plate_push_raised(scale_corners = true)` lands each corner at exactly
    /// the spec's final radius. Guards the double-span regression (7b-2), and
    /// holds for any configured corner_shape because both sides use the same
    /// factor.
    #[test]
    fn plate_spec_emission_round_trips_the_span() {
        let spec = PlateSpec {
            rect: r(0.0, 0.0, 400.0, 300.0),
            color: [0.1, 0.2, 0.3, 0.8],
            blur: false,
            window_corners: (true, true, false, false),
            depth: 4.0,
        };
        let mut pc = PaintCtx::new();
        pc.plate_spec(&spec);
        let f = crate::layout::corner_span_factor();
        let emitted = pc
            .finish()
            .items
            .iter()
            .find_map(|it| match &it.prim {
                Prim::Plate { radii, .. } => Some(radii.clone()),
                _ => None,
            })
            .expect("plate_spec emits a Prim::Plate");
        let want = spec.radii();
        let got = (emitted.0 * f, emitted.1 * f, emitted.2 * f, emitted.3 * f);
        for (g, w) in [(got.0, want.0), (got.1, want.1), (got.2, want.2), (got.3, want.3)] {
            assert!((g - w).abs() < 1e-3, "span round-trip drifted: {g} vs {w}");
        }
    }

    #[test]
    fn emits_in_order_unclipped() {
        let mut ctx = PaintCtx::new();
        ctx.quad(r(0.0, 0.0, 10.0, 10.0), [1.0, 0.0, 0.0, 1.0]);
        ctx.quad(r(5.0, 5.0, 10.0, 10.0), [0.0, 1.0, 0.0, 1.0]);
        let list = ctx.finish();
        assert_eq!(list.len(), 2);
        assert_eq!(list.items[0].clip, None);
        assert!(matches!(list.items[0].prim, Prim::Quad { color, .. } if color[0] == 1.0));
        assert!(matches!(list.items[1].prim, Prim::Quad { color, .. } if color[1] == 1.0));
    }

    #[test]
    fn clip_is_recorded_and_popped() {
        let mut ctx = PaintCtx::new();
        ctx.clip(r(0.0, 0.0, 50.0, 50.0), |ctx| {
            ctx.quad(r(10.0, 10.0, 5.0, 5.0), [0.0; 4]);
        });
        ctx.quad(r(60.0, 60.0, 5.0, 5.0), [0.0; 4]); // outside any clip now
        let list = ctx.finish();
        assert_eq!(list.items[0].clip, Some(r(0.0, 0.0, 50.0, 50.0)));
        assert_eq!(list.items[1].clip, None, "clip popped after the closure");
    }

    #[test]
    fn nested_clips_intersect() {
        let mut ctx = PaintCtx::new();
        ctx.clip(r(0.0, 0.0, 100.0, 100.0), |ctx| {
            ctx.clip(r(50.0, 50.0, 100.0, 100.0), |ctx| {
                ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
            });
        });
        // Intersection of (0,0,100,100) and (50,50,100,100) = (50,50,50,50).
        assert_eq!(ctx.finish().items[0].clip, Some(r(50.0, 50.0, 50.0, 50.0)));
    }

    #[test]
    fn non_overlapping_clips_produce_empty_scissor() {
        let mut ctx = PaintCtx::new();
        ctx.clip(r(0.0, 0.0, 10.0, 10.0), |ctx| {
            ctx.clip(r(100.0, 100.0, 10.0, 10.0), |ctx| {
                ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
            });
        });
        let clip = ctx.finish().items[0].clip.unwrap();
        assert_eq!((clip.width, clip.height), (0.0, 0.0), "empty intersection");
    }

    #[test]
    fn translate_applies_to_coordinates_and_restores() {
        let mut ctx = PaintCtx::new();
        ctx.translate(100.0, 200.0, |ctx| {
            ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]);
        });
        ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]); // back at origin
        let list = ctx.finish();
        assert!(matches!(list.items[0].prim, Prim::Quad { rect, .. } if rect.x == 100.0 && rect.y == 200.0));
        assert!(matches!(list.items[1].prim, Prim::Quad { rect, .. } if rect.x == 0.0 && rect.y == 0.0));
    }

    #[test]
    fn nested_translate_is_cumulative() {
        let mut ctx = PaintCtx::new();
        ctx.translate(10.0, 10.0, |ctx| {
            ctx.translate(5.0, 5.0, |ctx| {
                ctx.circle(0.0, 0.0, 3.0, [0.0; 4]);
            });
        });
        assert!(matches!(ctx.finish().items[0].prim, Prim::Circle { cx, cy, .. } if cx == 15.0 && cy == 15.0));
    }

    #[test]
    fn clip_pushed_under_translation_is_absolute() {
        let mut ctx = PaintCtx::new();
        ctx.translate(20.0, 20.0, |ctx| {
            ctx.clip(r(0.0, 0.0, 30.0, 30.0), |ctx| {
                ctx.quad(r(0.0, 0.0, 5.0, 5.0), [0.0; 4]);
            });
        });
        let item = &ctx.finish().items[0];
        // Clip translated to absolute (20,20,30,30); prim likewise at (20,20).
        assert_eq!(item.clip, Some(r(20.0, 20.0, 30.0, 30.0)));
        assert!(matches!(item.prim, Prim::Quad { rect, .. } if rect.x == 20.0 && rect.y == 20.0));
    }

    #[test]
    fn all_primitive_kinds_emit() {
        let mut ctx = PaintCtx::new();
        ctx.quad(r(0.0, 0.0, 1.0, 1.0), [0.0; 4]);
        ctx.rounded_rect(r(0.0, 0.0, 1.0, 1.0), 2.0, (true, false, true, false), [0.0; 4]);
        ctx.border(r(0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 2.0, 2.0), [0.1; 4], [0.9; 4], 1.5);
        ctx.bevel(r(0.0, 0.0, 10.0, 10.0), (2.0, 2.0, 2.0, 2.0), [0.3; 4], 2.0);
        ctx.arc(5.0, 5.0, 4.0, 1.0, 0.0, 3.14, [0.0; 4]);
        ctx.vector(0.0, 0.0, 10.0, 0.0, 1.0, [0.0; 4], Cap::Arrow);
        ctx.circle(5.0, 5.0, 3.0, [0.0; 4]);
        ctx.text("hi", 1.0, 2.0, 12.0, [255, 255, 255]);
        assert_eq!(ctx.finish().len(), 8);
    }

    #[test]
    fn border_and_bevel_are_offset() {
        let mut ctx = PaintCtx::new();
        ctx.translate(10.0, 20.0, |ctx| {
            ctx.border(r(0.0, 0.0, 5.0, 5.0), (1.0, 1.0, 1.0, 1.0), [0.0; 4], [1.0; 4], 1.0);
            ctx.bevel(r(0.0, 0.0, 5.0, 5.0), (1.0, 1.0, 1.0, 1.0), [0.0; 4], 1.0);
        });
        let list = ctx.finish();
        assert!(matches!(list.items[0].prim, Prim::Border { rect, .. } if rect.x == 10.0 && rect.y == 20.0));
        assert!(matches!(list.items[1].prim, Prim::Bevel { rect, .. } if rect.x == 10.0 && rect.y == 20.0));
    }
    #[test]
    fn text_with_translates_position_and_bounds() {
        let mut ctx = PaintCtx::new();
        ctx.translate(10.0, 20.0, |ctx| {
            ctx.text_with("hi", 1.0, 2.0, 12.0, [1, 2, 3], Some("Mono".into()), Some([0.0, 0.0, 50.0, 30.0]));
            ctx.text("plain", 3.0, 4.0, 10.0, [9, 9, 9]);
        });
        let list = ctx.finish();
        match &list.items[0].prim {
            Prim::Text { x, y, font, bounds, .. } => {
                assert_eq!((*x, *y), (11.0, 22.0), "position translated");
                assert_eq!(font.as_deref(), Some("Mono"));
                assert_eq!(*bounds, Some([10.0, 20.0, 60.0, 50.0]), "bounds translated");
            }
            other => panic!("expected Text, got {other:?}"),
        }
        match &list.items[1].prim {
            Prim::Text { font, bounds, .. } => {
                assert_eq!(*font, None, "plain text carries no font");
                assert_eq!(*bounds, None);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

}
