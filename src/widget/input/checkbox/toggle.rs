//! `Toggle`: a field whose run glides between its ends — the left end off, the right end on.

use super::*;

/// A field whose run glides: the toggle's footprint is ONE field
/// ([`crate::scene::paint::Prim::Field`]) — a well carved into the plate it
/// sits on, holding a flush run half its width, the run at the left end
/// (off) or the right end (on) and gliding between them.
///
/// It is the same object as a text row with its picker, a spinbox and its
/// -/+ run, and a dropdown trigger: a well in a plate with a flush plate in
/// it, one outline round both. Those hold their run still at the right end
/// (or are all run); a toggle's moves, and where it stands is the state.
/// Until 2026-10-02 it was a recess with a raised boss standing on its
/// floor, the one control whose nested plate stood ABOVE the surface where
/// every other stood flush with it, and whose outline turned its own corner
/// round the plate rather than running round the whole control.
///
/// That is the ONE toggle style. The rocker — two flat half faces with a
/// hinge between them, the state half tipped out toward the light — is gone,
/// and with it the per-widget and per-config style switch it was chosen by
/// (`style.control.toggle.style`, `Toggle::with_slide`).
#[derive(Debug, Clone)]
pub struct Toggle {
    toggled: bool,
    just_toggled: bool,
    label: Option<String>,
    hovered: bool,
    pub(super) focused: bool,
    /// Where the label sits across the track. Mirrors `Button::justify` — same enum, same
    /// 8px edge inset — so the two read as one control set wherever they share a column.
    justify: Justification,
    /// Relief style: the toggle is a field, a carved well with a flush run in
    /// it. Without it the well falls back to the hairline frame every well
    /// shares and the run to a lit face — see `paint`.
    raised: Option<bool>,
    /// The run's animated position along its field, 0 (left/off) → 1 (right/on).
    /// Chases `toggled` in `tick` after a click; programmatic state syncs
    /// (`set_toggled`, `set_value_string`) snap it, so only user interaction
    /// animates.
    pub(super) slide_t: f32,
}

impl Toggle {
    /// The style in force: the per-widget override (`with_raised`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn raised(&self) -> bool {
        self.raised.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new() -> Adapted<Toggle> {
        Adapted::new(Toggle {
            toggled: false,
            just_toggled: false,
            label: None,
            hovered: false,
            focused: false,
            justify: Justification::Center,
            raised: None,
            slide_t: 0.0,
        })
    }

    pub fn set_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    pub fn set_toggled(&mut self, v: bool) {
        self.toggled = v;
        self.slide_t = if v { 1.0 } else { 0.0 };
    }

    /// Set the hover directly, for a host that paints one toggle as a
    /// STAMP over several rows and does its own hit-testing — the
    /// designer's dialog — as `Checkbox::set_hovered` is there for.
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    pub fn toggled(&self) -> bool {
        self.toggled
    }

    /// The toggle as the [`Field`] it is, in its sliding form: the whole
    /// footprint carved one step down — taken through
    /// [`crate::layout::carve_inside`], so the walls stay inside the rect and
    /// the gap beside a toggle is the gap, exactly as a TextBox's well and a
    /// spinbox's field are taken — holding a run half its width, placed by
    /// the animated `slide_t`: flush with the left end off, the right end on,
    /// a well either side between. Lit while focused.
    ///
    /// As a text row's picker is, the run is laid out a wall wider than the
    /// face it carries (the field insets the face half a wall from the
    /// outline and from the seam), so the face reaches the well.
    ///
    /// The SINGLE source for the toggle's geometry: `paint` draws it, the
    /// relief-off paint lights its [`Toggle::face`], and a host that draws
    /// the relief itself (`ParametersBg::fields`) takes it whole.
    pub fn field(&self, rect: Rect) -> Field {
        let r = crate::layout::toggle_corner_radius();
        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
        let (outline, radii) = crate::layout::carve_inside(rect, (r, r, r, r), depth);
        let focus = self.focused.then(crate::scene::paint::ControlPlate::focus_tint);
        Field::sliding_run(outline, radii, depth, outline.width * 0.5, self.slide_t).with_tint(focus)
    }

    /// The run's FACE — what stands at the surface's level inside the
    /// field's valley: the run inset half a wall on every side, its corners
    /// the field's less that. `(rect, corner radius)`. The relief-off paint
    /// lights this rect, where the field cannot be carved.
    pub fn face(&self, rect: Rect) -> (Rect, f32) {
        let f = self.field(rect);
        let (a, b) = f.run_span().unwrap_or((f.rect.x, f.rect.x));
        let run = Rect { x: a, y: f.rect.y, width: b - a, height: f.rect.height };
        inset(run, f.radii.0, f.depth * 0.5)
    }
}

impl Adapted<Toggle> {
    pub fn with_left_align(mut self, left_align: bool) -> Self {
        self.justify = if left_align { Justification::Left } else { Justification::Center };
        self
    }

    pub fn with_justify(mut self, justify: Justification) -> Self {
        self.justify = justify;
        self
    }

    /// Raised style: see the `raised` field.
    pub fn with_raised(mut self, raised: bool) -> Self {
        self.raised = Some(raised);
        self
    }
}

impl Layout for Toggle {
    fn inline_label(&self) -> bool {
        true
    }

    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> {
        // Legacy `preferred_height`; width comes from the container.
        Some(crate::scene::layout::Size::new(0.0, crate::layout::toggle_height()))
    }
}

impl Paint for Toggle {
    /// No fill of its own: a toggle is worked out of the plate it sits on, so
    /// the plate's material (tint, blur, whatever it is) shows through and the
    /// state reads from light and relief alone — see [`Paint::paint`].
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::toggle_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);

        // A toggle paints NO fill of its own: it is worked out of the plate it
        // sits on, so the plate's own material (tint, blur, whatever it is)
        // shows through both the well's floor and the run's face, and the
        // state reads from light and relief alone — the DE's transparent-face
        // convention (closed dropdowns, inset troughs). The state colors this
        // used to tint with (enabled/disabled/background) are retired with the
        // rest of the toggle's palette.
        let field = self.field(rect);
        if self.raised() {
            // ONE field — the well and the flush run in it under one outline,
            // as a text row's picker and a spinbox's -/+ run are drawn. The
            // run's position IS the read: left off, right on, animated in
            // `tick`. A host that draws the relief itself
            // (`ParametersBg::fields`) takes the same `field`. Focus lights
            // the field's rim, the ring every field wears.
            ctx.field(&field);
        } else {
            // Relief off: a well is its frame — the one hairline every well
            // falls back to, lit while focused, exactly `well_rim`'s flat arm —
            // and the run's face is a lit face, the neutral overlay the DE
            // gives a surface it cannot carve (what the rocker's halves wore).
            // Scaled by the relief strength, as that lighting was.
            //
            // The face's overlay is a ROUNDED RECT on purpose: the legacy
            // reverse bridge reads only those, never a `Border`, so a
            // legacy-view host still shows which end the run is at.
            let bw = crate::layout::toggle_border_width().max(1.0);
            ctx.border(field.rect, field.radii, [0.0; 4], color::well_frame_color(self.hovered, self.focused), bw);
            let (face, face_r) = self.face(rect);
            let lit = (0.16 * (crate::layout::bevel_depth() / 0.15)).clamp(0.0, 0.5);
            ctx.rounded_rect(face, face_r, (true, true, true, true), [1.0, 1.0, 1.0, lit]);
        }

        if let Some(ref label) = self.label {
            let (font_fam, font_size) = crate::layout::control_label_font_parsed();
            let est_w = crate::widget::display::measure_text_width(label, &font_fam, font_size);
            let tx = match self.justify {
                Justification::Left => x + crate::layout::CONTROL_TEXT_INSET,
                Justification::Right => x + w - est_w - crate::layout::CONTROL_TEXT_INSET,
                Justification::Center => x + (w - est_w) / 2.0,
            };
            // The label never moves with the state. When the run covers it
            // the text shows through: the run carries no face of its own, and
            // the glyphs land in the engine's later text pass either way.
            //
            // Focus is the field's own lit rim — the ring every other field
            // wears, which the rocker's partial carves could not — so the
            // label stays the label.
            ctx.text_with(
                label.clone(),
                tx,
                crate::layout::align_text_y(y, h, font_size, 0.0),
                font_size,
                color::control_label_color_for_state(self.hovered, false),
                None,
                Some([x, y, x + w, y + h]),
            );
        }
    }
}

impl Input for Toggle {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                self.toggled = !self.toggled;
                self.just_toggled = true;
                true
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            Event::KeyInput(key_event) => {
                // A focused plate is pressed by Enter / Space, as a Button is.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    crate::widget::Key::Named(crate::widget::NamedKey::Enter)
                    | crate::widget::Key::Named(crate::widget::NamedKey::Space) => {
                        self.toggled = !self.toggled;
                        self.just_toggled = true;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The glide: the plate's position in its well chases the state after a
    /// click (~90ms exponential settle). Programmatic syncs snap instead — see
    /// `set_toggled` / `set_value_string` — so only user interaction animates.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        let target = if self.toggled { 1.0 } else { 0.0 };
        let d = target - self.slide_t;
        if d.abs() < 0.001 {
            return false;
        }
        if !crate::motion::enabled() {
            self.slide_t = target;
            return true;
        }
        self.slide_t += d * (1.0 - (-dt * 22.0).exp());
        if (target - self.slide_t).abs() < 0.005 {
            self.slide_t = target;
        }
        true
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_toggled)
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_toggled)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.toggled.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some(new_toggled) = parse_bool(val) else { return false };
        if self.toggled != new_toggled {
            self.toggled = new_toggled;
            self.slide_t = if new_toggled { 1.0 } else { 0.0 };
            self.just_toggled = true;
            true
        } else {
            false
        }
    }
}
