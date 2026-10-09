//! `PaintCtx`'s relief emitters: plates (root, pane, control), bevels and frames, recesses, bosses,
//! ridges and troughs, fields and wells, grooves, lattices and grout, carve unions, droplets, the
//! sphere.

use super::*;

impl PaintCtx {

    /// A sphere-lit circle — see `Prim::Sphere`.
    pub fn sphere(&mut self, cx: f32, cy: f32, radius: f32, material: &Material) {
        let (ox, oy) = self.offset;
        self.push(Prim::Sphere { cx: cx + ox, cy: cy + oy, radius, material: *material });
    }

    /// A hanging water droplet clinging to `rect`'s top edge — see
    /// [`Prim::Droplet`] and [`DropletSpec`].
    /// See [`Prim::DropletScrim`]. `feather` is how far in from the drop's
    /// edge the fill reaches full opacity, in logical px.
    pub fn droplet_scrim(&mut self, rect: Rect, material: &Material, spec: DropletSpec, feather: f32) {
        let rect = self.apply_offset(rect);
        self.push(Prim::DropletScrim { rect, material: *material, spec, feather });
    }

    /// The drop's finish is the material's (see [`DropletSpec::finish`]).
    pub fn droplet(&mut self, rect: Rect, material: &Material, spec: DropletSpec) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Droplet { rect, material: *material, spec });
    }

    /// A concave inside-corner fillet — see `Prim::ConcaveFillet`. `start` is
    /// the quarter wedge's start angle; the arc's centre sits in the corner's
    /// pocket and the wall descends (or rises, `raised`) away from it.
    pub fn concave_fillet(&mut self, cx: f32, cy: f32, radius: f32, depth: f32, start: f32, raised: bool) {
        let (ox, oy) = self.offset;
        self.push(Prim::ConcaveFillet { cx: cx + ox, cy: cy + oy, radius, depth, start, raised });
    }

    /// An engraved line from `a` to `b` cut into `host` — see [`Prim::Groove`].
    pub fn groove(&mut self, a: (f32, f32), b: (f32, f32), width: f32, depth: f32, host: Rect) {
        self.groove_strength(a, b, width, depth, host, 1.0);
    }

    /// [`Self::groove`] at a fraction of its strength — see [`Prim::Groove`].
    pub fn groove_strength(&mut self, a: (f32, f32), b: (f32, f32), width: f32, depth: f32, host: Rect, strength: f32) {
        let (ox, oy) = self.offset;
        let host = self.apply_offset(host);
        self.push(Prim::Groove {
            a: (a.0 + ox, a.1 + oy),
            b: (b.0 + ox, b.1 + oy),
            width,
            depth,
            host,
            strength,
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

    /// A flat fill of `material` — see [`Prim::Fill`].
    pub fn fill_material(&mut self, rect: Rect, radii: Radii, material: &Material) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Fill { rect, radii, material: *material });
    }

    /// Grout between a periodic field of rounded cells — see [`Prim::Grout`].
    /// `origin` is any one cell's centre; `rect` bounds the paint.
    pub fn grout(&mut self, rect: Rect, period: (f32, f32), origin: (f32, f32), cell: (f32, f32), radius: f32, color: [f32; 4]) {
        let (ox, oy) = self.offset;
        let rect = self.apply_offset(rect);
        self.push(Prim::Grout { rect, period, origin: (origin.0 + ox, origin.1 + oy), cell, radius, color });
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

    pub fn bevel(&mut self, rect: Rect, radii: Radii, material: &Material, depth: f32) {
        self.bevel_tinted(rect, radii, material, depth, [1.0, 1.0, 1.0]);
    }

    /// A plate whose face is `rect` outside `hole` — see [`Prim::Frame`].
    pub fn frame(&mut self, rect: Rect, hole: Rect, hole_radii: Radii, material: &Material, depth: f32) {
        let rect = self.apply_offset(rect);
        let hole = self.apply_offset(hole);
        self.push(Prim::Frame { rect, hole, hole_radii, material: *material, depth });
    }

    /// `bevel` with a specular tint — see `Prim::Bevel::tint`.
    pub fn bevel_tinted(&mut self, rect: Rect, radii: Radii, material: &Material, depth: f32, tint: [f32; 3]) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Bevel { rect, radii, material: *material, depth, tint });
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
                if let Some(face) = plate.faced() {
                    match plate.tint {
                        Some(t) => self.bevel_tinted(plate.rect, plate.radii, face, plate.depth, t),
                        None => self.bevel(plate.rect, plate.radii, face, plate.depth),
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
                    Some(t) => self.inset_plate_tinted(trough, radii, plate.faced(), plate.depth, t),
                    None => self.inset_plate(trough, radii, plate.faced(), plate.depth),
                }
            }
            PlateStance::Flat => {
                if let Some(face) = plate.faced() {
                    // A QUAD deliberately, not the `Border` the relief stances
                    // fill through: carrying the blur-behind sentinel is half
                    // the point of this stance, and only quads reach it.
                    self.rounded_rect(
                        plate.rect,
                        plate.radii.0,
                        (true, true, true, true),
                        face.fill(PlateRole::Nested),
                    );
                }
                if let Some(t) = plate.tint {
                    // No relief, so no rim to light: the focus ring is drawn as
                    // one, over the face and keeping the per-corner silhouette.
                    self.border(plate.rect, plate.radii, [0.0; 4], [t[0], t[1], t[2], 1.0], 1.0);
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
    pub fn inset_plate(&mut self, rect: Rect, radii: Radii, face: Option<&Material>, depth: f32) {
        // A transparent material is no face either — only a visible tint
        // fills; a frosted one fills with the sentinel.
        if let Some(face) = face.filter(|m| m.tint[3] > 0.001) {
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
            self.border(rect, radii, face.fill(PlateRole::Nested), [0.0; 4], 0.0);
        }
        // The edge is a field's RUN (a field with no well, its seam put
        // [`FIELD_RUN_ONLY`] px to the left): the outer half a well's own
        // fall, the inner half that fall mirrored back up to the face — so
        // every flush control has the edge of the run at the end of a text
        // row's field, and the well beside it. Until 2026-10-02 this was a
        // [`Prim::Trough`], whose outer half is a compressed copy of a step;
        // the dropdown, button, breadcrumb, font selector and menubar
        // triggers had been switched to the run's edge one by one the day
        // before, and the apps' own flush plates (the calendar's, cce-cloud's,
        // cce-files', the system interface's) kept the trough until here.
        self.field(&Field::run(rect, radii, depth));
    }

    /// [`inset_plate`](Self::inset_plate) with the rim lit — the focused flush
    /// control plate's ring (`ControlPlate::with_tint`); the face fill as
    /// there, the trough tinted.
    pub fn inset_plate_tinted(&mut self, rect: Rect, radii: Radii, face: Option<&Material>, depth: f32, tint: [f32; 3]) {
        if let Some(face) = face.filter(|m| m.tint[3] > 0.001) {
            self.border(rect, radii, face.fill(PlateRole::Nested), [0.0; 4], 0.0);
        }
        self.field(&Field::run(rect, radii, depth).with_tint(Some(tint)));
    }

    /// A canvas well's floor — the opening you look into or draw in (a
    /// Trackpad, a Slider2D pad, a bevel or ramp preview) — cut into `host`,
    /// the material of the plate it sits on (`Material::pane()` for a pane).
    /// `lifted` is a clickable canvas's hover cue: the floor rises toward
    /// the plate.
    ///
    /// An opaque host's floor is that plate darkened, drawn as the darkening
    /// itself ([`crate::colors::WELL_FLOOR`] over whatever the plate resolved
    /// to — exact at any plate alpha, and what every floor drew before
    /// materials). A FROSTED host's floor is deeper glass
    /// ([`Material::floor`]: the host's material with the tint darkened,
    /// frost and finish carried), so a well in glass blurs and compresses
    /// what is under it again instead of being the one opaque patch in a
    /// frosted pane (RFC material § 11 (3)).
    pub fn well_floor(&mut self, rect: Rect, radius: f32, host: &Material, lifted: bool) {
        let fill = if host.frost.is_frosted() {
            host.floor(lifted).fill(PlateRole::Nested)
        } else if lifted {
            crate::colors::WELL_FLOOR_LIFTED
        } else {
            crate::colors::WELL_FLOOR
        };
        self.rounded_rect(rect, radius, (true, true, true, true), fill);
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
    pub fn canvas_well(&mut self, rect: Rect, radius: f32, host: &Material, relief: bool, lifted: bool) {
        self.well_floor(rect, radius, host, lifted);
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

    /// [`PaintCtx::trough_edges`] with a specular tint on the lit rim — see
    /// `Prim::Trough::tint`.
    pub fn trough_edges_tinted(
        &mut self,
        rect: Rect,
        radii: Radii,
        depth: f32,
        edges: (bool, bool, bool, bool),
        tint: [f32; 3],
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Trough { rect, radii, depth, edges, tint: Some(tint) });
    }

    /// Paint a [`Field`] — see it for the forms. One with a run is a
    /// [`Prim::Field`]; one that is all well a [`Prim::Recess`], which groups
    /// into the plate under it.
    pub fn field(&mut self, field: &Field) {
        let Field { rect, radii, depth, tint, .. } = *field;
        match field.prim_span() {
            None => match tint {
                Some(t) => self.recess_tinted(rect, radii, depth, t),
                None => self.recess(rect, radii, depth),
            },
            Some((split, end)) => {
                let (split, end) = (split + self.offset.0, end + self.offset.0);
                let rect = self.apply_offset(rect);
                self.push(Prim::Field { rect, radii, depth, split, end, tint });
            }
        }
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

    /// [`PaintCtx::recess_edges`] with a specular tint on the lit rim — see
    /// `Prim::Recess::tint`. A tinted carve never groups into its host plate
    /// (the tint could only land on the whole plate's specular), so it
    /// shades through the overlay path with its own lit rim.
    pub fn recess_edges_tinted(
        &mut self, rect: Rect, radii: Radii, depth: f32,
        edges: (bool, bool, bool, bool),
        tint: [f32; 3],
    ) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Recess { rect, radii, depth, edges, tint: Some(tint) });
    }

    /// The window's glass slab: rounded fill at full size plus a rolled, lit perimeter.
    /// `depth` is the roll-off width in px — pass [`crate::layout::bevel_width`] unless the
    /// window wants a shallower edge than the DE default.
    ///
    /// A NEGATIVE `depth` is the fill-less sentinel: no fill is drawn, and the
    /// rolled perimeter (width `-depth`) renders as an overlay — translucent
    /// white screen / black multiply — over whatever is beneath, for a root
    /// plate whose face is not a fill (the designer's full-bleed 3D canvas).
    /// `material` is ignored; the roll profile, crest and specular are exactly the
    /// positive-depth plate's.
    pub fn plate(&mut self, rect: Rect, radii: Radii, material: &Material, depth: f32) {
        self.plate_shaped(rect, radii, material, depth, None);
    }

    /// [`plate`](Self::plate) with an explicit corner exponent — see
    /// [`Prim::Plate`]'s `shape`. `Some(2.0)` on a plate whose radii are its
    /// half-extent draws a circle; `None` is exactly `plate`.
    pub fn plate_shaped(&mut self, rect: Rect, radii: Radii, material: &Material, depth: f32, shape: Option<f32>) {
        let rect = self.apply_offset(rect);
        self.push(Prim::Plate { rect, radii, material: *material, depth, shape });
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
        self.plate(spec.rect, (tl / f, tr / f, br / f, bl / f), &spec.material.for_role(spec.role()), spec.depth);
    }

    /// The standard root plate of a `width` x `height` window —
    /// [`PlateSpec::window`] emitted. The first prim of a standard cce app's
    /// frame: everything else is laid on this surface (pane plates atop it,
    /// bands and wells carved into it), starting
    /// [`crate::layout::root_plate_inset`] in from each window edge.
    pub fn root_plate(&mut self, width: f32, height: f32) {
        self.plate_spec(&PlateSpec::window(width, height));
    }
}
