//! Narrow-trait `Checkbox` and `Toggle` (Phase 5e — first interactive widgets off `WidgetHost`).
//!
//! Both are inline-label widgets: they paint their own label (with hover/focus-dependent color)
//! inside their rect, so they track `hovered`/`focused` themselves from the `MouseEnter`/
//! `MouseLeave`/`FocusIn`/`FocusOut` events the adapter forwards — the migration shape for the
//! state that becomes `Animated<f32>` in RFC §3.6.
//!
//! Geometry parity: `paint` emits the same conditional geometry as the legacy `extra_quads` /
//! `extra_arcs` / `all_rounded_quads` overrides did, in the matching prim kinds, so the adapter's
//! per-prim reverse bridges reproduce the legacy getters byte-for-byte.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::{Field, PaintCtx};
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Justification, Layout, MouseButton, Paint,
};

/// How much of a checked box's side its plate takes, standing square in the
/// middle of the well — enough to read as a plate, little enough to leave the
/// well showing all round it, which is what tells it from an empty box. (All
/// run, a box filled flush, was tried first: at a control's size its outline
/// is the empty well's, and the two states were hard to tell apart. Then the
/// toggle's run, half the width and the field's whole height: in a square box
/// that is a tall bar, and a ticked box read as having narrowed.)
const PLATE_SHARE: f32 = 0.6;

/// Paint a check box — its well ([`Checkbox::box_field`]) and, checked, the
/// plate in it ([`Checkbox::box_plate`]): both fields with relief on; off,
/// as the toggle's — a well is its frame, the hairline every well falls back
/// to (lit by `hovered` / `focused`), and the plate a lit face in it.
pub(crate) fn paint_box(ctx: &mut PaintCtx, well: &Field, plate: Option<&Field>, hovered: bool, focused: bool) {
    if crate::layout::control_relief() {
        ctx.field(well);
        if let Some(plate) = plate {
            ctx.field(plate);
        }
        return;
    }
    let bw = crate::layout::toggle_border_width().max(1.0);
    ctx.border(well.rect, well.radii, [0.0; 4], colors::well_frame_color(hovered, focused), bw);
    if let Some(plate) = plate {
        let (face, face_r) = inset(plate.rect, plate.radii.0, plate.depth * 0.5);
        let lit = (0.16 * (crate::layout::bevel_depth() / 0.15)).clamp(0.0, 0.5);
        ctx.rounded_rect(face, face_r, (true, true, true, true), [1.0, 1.0, 1.0, lit]);
    }
}

/// A rect shrunk by `g` on every side, its uniform corner radius shrunk to
/// match so the inner silhouette stays concentric with the outer one.
fn inset(rect: Rect, radius: f32, g: f32) -> (Rect, f32) {
    (
        Rect {
            x: rect.x + g,
            y: rect.y + g,
            width: (rect.width - 2.0 * g).max(0.0),
            height: (rect.height - 2.0 * g).max(0.0),
        },
        (radius - g).max(0.0),
    )
}

fn parse_bool(val: &str) -> Option<bool> {
    match val.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// A check box: a square well ([`Field::well`]) with an optional label to
/// its right — empty unchecked; checked, a flush square plate standing in the
/// middle of it ([`Field::run`] on [`PLATE_SHARE`] of its side), the well
/// showing all round. Not the toggle's run with no travel, though it was: a
/// run spans its field's whole height, so in a square it is a tall bar.
/// Standalone, the box is the largest square in the rect, centred.
///
/// A check drawn inline in text — a list row (cce-list), a markdown task
/// item, the doc editor — is the same box, through [`Checkbox::paint_inline`].
/// Until 2026-10-02 the widget and all of those drew a ring-and-dot mark.
pub struct Checkbox {
    checked: bool,
    just_clicked: bool,
    pub just_changed: bool,
    label: Option<String>,
    hovered: bool,
    focused: bool,
}

impl Checkbox {
    /// Half the side of an inline box ([`Checkbox::paint_inline`]) at a
    /// list row's size: a 14px square, the size of the round mark it
    /// replaced.
    pub const INLINE_HALF: f32 = 7.0;

    /// The box's well: a square as tall as the control (at most a toggle's
    /// height) at the rect's left when labelled, the largest square in the
    /// rect, centred, when not ([`Checkbox::box_field`]). Lit while focused.
    pub fn field(&self, rect: Rect) -> Field {
        let side = if self.label.is_some() {
            rect.height.min(crate::layout::toggle_height())
        } else {
            rect.width.min(rect.height)
        }
        .max(1.0);
        let x = if self.label.is_some() { rect.x } else { rect.x + (rect.width - side) * 0.5 };
        let square = Rect { x, y: rect.y + (rect.height - side) * 0.5, width: side, height: side };
        Self::box_field(square).with_tint(self.focused.then(crate::scene::paint::ControlPlate::focus_tint))
    }

    /// A check box's well on `square` — its footprint — carved inside it
    /// through [`crate::layout::carve_inside`] as every field is. With
    /// [`Checkbox::box_plate`], the ONE description the widget and every
    /// inline box share.
    ///
    /// The corner is the toggle's in PROPORTION — its radius over its height
    /// — so a box as tall as a toggle has the toggle's corner exactly, and a
    /// 14px box in a line of text is a rounded square rather than a disc,
    /// which the toggle's radius taken whole would make it.
    pub fn box_field(square: Rect) -> Field {
        let side = square.width.min(square.height).max(1.0);
        let r = (crate::layout::toggle_corner_radius() * side / crate::layout::toggle_height().max(1.0)).min(side * 0.5);
        let depth = crate::layout::bevel_width().min(side * 0.2);
        let (outline, radii) = crate::layout::carve_inside(square, (r, r, r, r), depth);
        Field::well(outline, radii, depth)
    }

    /// A checked box's plate in `well` ([`Checkbox::box_field`]): a square
    /// [`PLATE_SHARE`] of its side, centred, all run — its corner the well's
    /// in proportion, so it is the box again, smaller.
    pub fn box_plate(well: &Field) -> Field {
        let o = well.rect;
        let side = (o.width.min(o.height) * PLATE_SHARE).max(1.0);
        let rect = Rect { x: o.x + (o.width - side) * 0.5, y: o.y + (o.height - side) * 0.5, width: side, height: side };
        let r = well.radii.0 * side / o.width.max(1.0);
        Field::run(rect, (r, r, r, r), well.depth.min(side * 0.2))
    }

    /// Paint a check box centred on (`cx`, `cy`), `half` its half-side, for
    /// a host that draws one inline — cce-list's rows, a markdown task item,
    /// the doc editor — where the widget would be a whole control. The same
    /// box the widget draws ([`Checkbox::box_field`], [`Checkbox::box_plate`]),
    /// relief on or off.
    pub fn paint_inline(ctx: &mut PaintCtx, cx: f32, cy: f32, half: f32, checked: bool) {
        let square = Rect { x: cx - half, y: cy - half, width: 2.0 * half, height: 2.0 * half };
        let well = Self::box_field(square);
        paint_box(ctx, &well, checked.then(|| Self::box_plate(&well)).as_ref(), false, false);
    }

    pub fn new() -> Adapted<Checkbox> {
        Adapted::new(Checkbox {
            checked: false,
            just_clicked: false,
            just_changed: false,
            label: None,
            hovered: false,
            focused: false,
        })
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    /// Hover state, also settable directly for immediate-mode hosts that do their own
    /// hit-testing instead of routing `MouseEnter`/`MouseLeave` (json_layout).
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }
}

impl Layout for Checkbox {
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for Checkbox {
    fn color(&self) -> [f32; 4] {
        // The box is carved; the widget itself has no background.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        // The box leads and the label follows, a list row's reading order.
        let field = self.field(rect);
        let plate = self.checked.then(|| Self::box_plate(&field));
        paint_box(ctx, &field, plate.as_ref(), self.hovered, self.focused);
        if let Some(ref label) = self.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            let ty = crate::layout::align_text_y(y, h, font_size, 0.0);
            // From the box's footprint, not its carved outline.
            let box_right = field.rect.x + field.rect.width + field.depth * 0.5;
            ctx.text_with(
                label.clone(),
                box_right + 8.0,
                ty,
                font_size,
                colors::control_label_color_for_state(self.hovered, self.focused),
                None,
                // The label is caller text and the box is caller-sized; a
                // control has no business drawing past its own rect.
                Some([x, y, x + w, y + h]),
            );
        }
    }
}

impl Input for Checkbox {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                // Already hit-gated by the adapter.
                self.checked = !self.checked;
                self.just_clicked = true;
                self.just_changed = true;
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
                        self.checked = !self.checked;
                        self.just_clicked = true;
                        self.just_changed = true;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_clicked)
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.checked.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some(new_checked) = parse_bool(val) else { return false };
        if self.checked != new_checked {
            self.checked = new_checked;
            self.just_changed = true;
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        if self.checked { 1 } else { 0 }
    }
}

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
    focused: bool,
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
    slide_t: f32,
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
            ctx.border(field.rect, field.radii, [0.0; 4], colors::well_frame_color(self.hovered, self.focused), bw);
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
                colors::control_label_color_for_state(self.hovered, false),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{WidgetHost, UiContext};

    fn click_at(x: f32, y: f32) -> Event {
        Event::MouseButton {
            button: MouseButton::Left,
            state: ElementState::Pressed,
            x,
            y,
            local_x: x,
            local_y: y,
        }
    }

    #[test]
    fn checkbox_click_toggles_and_polls_like_legacy() {
        let mut ctx = UiContext::new();
        let mut cb = Checkbox::new();
        let id = cb.id();
        ctx.register_host(&mut cb);
        WidgetHost::set_rect(&mut cb, 0.0, 0.0, 20.0, 20.0);

        assert!(ctx.propagate_event(&click_at(10.0, 10.0), id), "in-rect click consumed");
        assert!(cb.checked(), "click checked it");
        assert!(cb.take_click(), "take_click reads once");
        assert!(!cb.take_click(), "...then clears");
        assert!(cb.take_change());

        assert!(!ctx.propagate_event(&click_at(100.0, 100.0), id), "miss is not consumed");
        assert!(cb.checked(), "miss does not toggle");
    }

    #[test]
    fn checkbox_value_string_round_trip() {
        let mut cb = Checkbox::new();
        assert_eq!(cb.get_value_string(), Some("false".to_string()));
        assert!(cb.set_value_string("on"));
        assert!(cb.checked());
        assert_eq!(cb.value(), 1);
        assert!(!cb.set_value_string("on"), "unchanged value reports false");
        assert!(!cb.set_value_string("junk"), "unparsable reports false");
        assert!(cb.take_change(), "set_value_string marked the change");
    }

    /// A labelled checkbox is a square field at the left of its label: an
    /// empty well, or checked a run standing in its middle.
    #[test]
    fn a_checkbox_is_a_well_with_a_square_plate_in_it_or_not() {
        use crate::scene::paint::Prim;
        let ctx = UiContext::new();
        let mut cb = Checkbox::new().with_label("Enable");
        WidgetHost::set_rect(&mut cb, 0.0, 0.0, 200.0, 24.0);
        let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 24.0 };
        let prims = |cb: &Adapted<Checkbox>| -> Vec<Prim> {
            let mut pc = crate::scene::paint::PaintCtx::new();
            crate::widget::Paint::paint(cb.inner(), rect, &mut pc);
            pc.finish().items.into_iter().map(|i| i.prim).filter(|p| !matches!(p, Prim::Text { .. })).collect()
        };

        // A square at the left, as tall as the control, carved inside it.
        let f = cb.inner().field(rect);
        let side = 24.0f32.min(crate::layout::toggle_height());
        assert!((f.rect.width - f.rect.height).abs() < 1e-4, "square");
        assert!(f.rect.x >= 0.0 && f.rect.x + f.rect.width <= side, "at the left, inside its square");
        assert!(f.has_well() && f.run_span().is_none(), "all well");

        // Checked, a square plate in the middle, the well all round it: the
        // box keeps its shape — a ticked box must not read as narrower.
        cb.inner_mut().set_checked(true);
        assert_eq!(cb.inner().field(rect), f, "the well is the same either way");
        let p = Checkbox::box_plate(&f);
        assert!((p.rect.width - p.rect.height).abs() < 1e-4, "the plate is square");
        assert!((p.rect.width - f.rect.width * PLATE_SHARE).abs() < 1e-3);
        let (pcx, pcy) = (p.rect.x + p.rect.width * 0.5, p.rect.y + p.rect.height * 0.5);
        let (fcx, fcy) = (f.rect.x + f.rect.width * 0.5, f.rect.y + f.rect.height * 0.5);
        assert!((pcx - fcx).abs() < 1e-3 && (pcy - fcy).abs() < 1e-3, "centred");
        assert!(!p.has_well(), "the plate is all run");
        assert!(WidgetHost::extra_circles(&cb).is_empty(), "no mark");

        if crate::layout::control_relief() {
            cb.inner_mut().set_checked(false);
            assert!(matches!(prims(&cb)[..], [Prim::Recess { .. }]), "a well paints as the recess it groups as");
            cb.inner_mut().set_checked(true);
            assert!(matches!(prims(&cb)[..], [Prim::Recess { .. }, Prim::Field { .. }]), "the plate paints as a field in it");
            cb.focused = true;
            assert!(matches!(prims(&cb)[..], [Prim::Recess { tint: Some(_), .. }, _]), "focus lights the rim");
        }

        // The label follows the box.
        let labels = cb.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "Enable");
        assert!((labels[0].x - (side + 8.0)).abs() < 1e-3, "{} vs {}", labels[0].x, side + 8.0);

        // Inline label => no set_rect inflation.
        assert_eq!(WidgetHost::rect(&cb), (0.0, 0.0, 200.0, 24.0));
        let _ = &ctx;
    }

    /// An inline check — a list row's, a markdown task item's — is the
    /// widget's box at the size it is given: the same field, its corner the
    /// toggle's in proportion, so a 14px box is a rounded square and not a
    /// disc, and a box a toggle tall has the toggle's corner exactly.
    #[test]
    fn an_inline_check_is_the_widgets_box() {
        use crate::scene::paint::Prim;
        let h = Checkbox::INLINE_HALF;
        let square = Rect { x: 50.0 - h, y: 20.0 - h, width: 2.0 * h, height: 2.0 * h };
        for checked in [false, true] {
            let mut pc = crate::scene::paint::PaintCtx::new();
            Checkbox::paint_inline(&mut pc, 50.0, 20.0, h, checked);
            let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
            let want = Checkbox::box_field(square);
            if crate::layout::control_relief() {
                let mut expect = crate::scene::paint::PaintCtx::new();
                expect.field(&want);
                if checked {
                    expect.field(&Checkbox::box_plate(&want));
                }
                let expect: Vec<Prim> = expect.finish().items.into_iter().map(|i| i.prim).collect();
                assert_eq!(format!("{prims:?}"), format!("{expect:?}"), "the box_field and box_plate, painted");
            }
            assert!(want.radii.0 < want.rect.width * 0.5 - 0.5, "a rounded square, not a disc: {:?}", want.radii);
        }
        // A box a toggle tall has the toggle's corner, as the widget's always had.
        let th = crate::layout::toggle_height();
        let big = Checkbox::box_field(Rect { x: 0.0, y: 0.0, width: th, height: th });
        let r = crate::layout::toggle_corner_radius().min(th * 0.5);
        let (_, radii) = crate::layout::carve_inside(Rect { x: 0.0, y: 0.0, width: th, height: th }, (r, r, r, r), big.depth);
        assert_eq!(big.radii, radii);
    }

    #[test]
    fn toggle_click_glides_the_run_across_its_field() {
        let mut ctx = UiContext::new();
        let mut t = Toggle::new();
        let id = t.id();
        ctx.register_host(&mut t);
        WidgetHost::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);

        let rect = Rect { x: 0.0, y: 0.0, width: 60.0, height: 30.0 };
        let painted = |t: &Adapted<Toggle>| {
            let mut pc = crate::scene::paint::PaintCtx::new();
            crate::widget::Paint::paint(t.inner(), rect, &mut pc);
            pc.finish().items.into_iter().map(|i| format!("{:?}", i.prim)).collect::<Vec<_>>()
        };
        let before = painted(&t);
        let plate_x = |t: &Adapted<Toggle>| t.inner().field(rect).run_span().unwrap().0;
        let left = plate_x(&t);

        assert!(ctx.propagate_event(&click_at(30.0, 15.0), id), "toggle consumed the click");
        assert!(t.toggled());
        assert!(t.take_click());

        // A click sets the target; the plate GLIDES there (`tick`), so the
        // geometry only moves once time passes — the rocker's halves used to
        // swap on the press itself.
        assert_eq!(plate_x(&t), left, "the click alone does not move the plate");
        for _ in 0..60 {
            crate::widget::Input::tick(t.inner_mut(), 1.0 / 60.0, rect);
        }
        assert!(plate_x(&t) > left, "the plate glided toward the on end");
        assert!(painted(&t) != before, "toggling changes the emitted geometry");

        // preferred_height forwards the legacy toggle height.
        assert_eq!(WidgetHost::preferred_height(&t), Some(crate::layout::toggle_height()));
    }

    /// The toggle is ONE field, the form a text row's picker and a spinbox's
    /// -/+ run take: off, its run is the left half and the well the right;
    /// on, the other way round; between, a well either side. The run's face
    /// stands half a wall inside the run, so the run reaches the well.
    #[test]
    fn a_toggle_is_a_field_whose_run_glides() {
        use crate::scene::paint::{Prim, FIELD_RUN_ONLY};
        let rect = Rect { x: 10.0, y: 4.0, width: 120.0, height: 24.0 };
        let mut t = Toggle::new().with_raised(true);
        let Field { rect: field, radii, depth, .. } = t.inner().field(rect);
        assert!(field.x >= rect.x && field.y >= rect.y, "the field carves inside the rect");
        let (fl, fr) = (field.x, field.x + field.width);

        let fields = |t: &Adapted<Toggle>| -> Vec<Prim> {
            let mut pc = crate::scene::paint::PaintCtx::new();
            crate::widget::Paint::paint(t.inner(), rect, &mut pc);
            pc.finish().items.into_iter().map(|i| i.prim).filter(|p| !matches!(p, Prim::Text { .. })).collect()
        };
        let off = fields(&t);
        assert_eq!(off.len(), 1, "one prim, the field: {off:?}");
        let Prim::Field { rect: r, radii: rr, depth: d, split, end, tint } = off[0] else { panic!("{off:?}") };
        assert_eq!((r, rr, d, tint), (field, radii, depth, None));
        assert!(split <= fl - FIELD_RUN_ONLY + 1.0, "off: the run reaches the field's left end — no well there");
        assert!((end - (fl + field.width * 0.5)).abs() < 1e-4, "half the field, the well beyond it");

        t.set_toggled(true); // programmatic syncs snap, so this is the on-end geometry
        let Prim::Field { split, end, .. } = fields(&t)[0] else { panic!() };
        assert!((split - (fl + field.width * 0.5)).abs() < 1e-4, "on: the right half");
        assert!(end >= fr + FIELD_RUN_ONLY - 1.0, "reaching the field's right end");

        t.slide_t = 0.5;
        let Prim::Field { split, end, .. } = fields(&t)[0] else { panic!() };
        assert!(split > fl + 1.0 && end < fr - 1.0, "mid-glide, a well either side of the run");
        assert!(end < fr, "and the run's end is its own, not the field's");

        let (face, face_r) = t.inner().face(rect);
        let (a, b) = t.inner().field(rect).run_span().unwrap();
        assert!((face.x - (a + depth * 0.5)).abs() < 1e-4 && (face.x + face.width - (b - depth * 0.5)).abs() < 1e-4);
        assert!((face_r - (radii.0 - depth * 0.5).max(0.0)).abs() < 1e-4, "concentric with the field");

        // Focus lights the field's rim.
        t.focused = true;
        assert!(matches!(fields(&t)[0], Prim::Field { tint: Some(_), .. }), "focus tints the field");
    }

    #[test]
    fn toggle_set_label_via_deref_reaches_paint() {
        let mut t = Toggle::new();
        WidgetHost::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);
        t.set_label("ON"); // the network.rs pattern: live label updates through Deref
        let labels = t.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "ON");
    }
}
