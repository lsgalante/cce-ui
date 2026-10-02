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
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Justification, Layout, MouseButton, Paint,
};

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

/// A ring-and-dot check mark with an optional label to its right: a 14px ring in
/// the dim text colour, filled with a dot in the toggle-on colour when checked —
/// the mark cce-list's rows have always drawn (they call `paint_round_mark` for
/// it). Standalone, the mark fills the rect.
pub struct Checkbox {
    checked: bool,
    just_clicked: bool,
    pub just_changed: bool,
    label: Option<String>,
    hovered: bool,
    focused: bool,
}

impl Checkbox {
    /// The labelled mark's radius: a 14px disc.
    pub const ROUND_RADIUS: f32 = 7.0;

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

    /// The mark, centred on (`cx`, `cy`): a dim ring, and a solid dot in the
    /// toggle-on colour when checked. Shared with hosts that draw their own rows
    /// (cce-list) so a list's marks and a `Checkbox` agree pixel for pixel.
    pub fn paint_round_mark(ctx: &mut PaintCtx, cx: f32, cy: f32, radius: f32, checked: bool) {
        Self::paint_round_mark_ringed(ctx, cx, cy, radius, checked, colors::TEXT_DIM);
    }

    /// [`Checkbox::paint_round_mark`] with the ring in `ring` — the mark's
    /// silhouette lit in the highlight colour is its keyboard-focus ring (a
    /// mark is not a plate, so it has no rim to tint; the ring it already
    /// draws is the silhouette).
    pub fn paint_round_mark_ringed(ctx: &mut PaintCtx, cx: f32, cy: f32, radius: f32, checked: bool, ring: [f32; 4]) {
        ctx.border(
            Rect { x: cx - radius, y: cy - radius, width: 2.0 * radius, height: 2.0 * radius },
            (radius, radius, radius, radius),
            [0.0, 0.0, 0.0, 0.0],
            ring,
            1.5,
        );
        if checked {
            ctx.circle(cx, cy, radius - 3.0, colors::TOGGLE_ON);
        }
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
        // The mark is painted; the widget itself has no background.
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
        // The mark leads and the label follows, a list row's reading order.
        // Standalone, the mark fills the rect.
        let r = if self.label.is_some() { Self::ROUND_RADIUS } else { (w.min(h) / 2.0).max(1.0) };
        let (cx, cy) = if self.label.is_some() { (x + r, y + h / 2.0) } else { (x + w / 2.0, y + h / 2.0) };
        // Focused: the mark's own ring lit in the highlight colour.
        let ring = if self.focused { crate::color::highlight_primary_color() } else { colors::TEXT_DIM };
        Self::paint_round_mark_ringed(ctx, cx, cy, r, self.checked, ring);
        if let Some(ref label) = self.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            let ty = crate::layout::align_text_y(y, h, font_size, 0.0);
            ctx.text_with(
                label.clone(),
                cx + r + 8.0,
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

    /// The field's outline: the toggle's whole footprint, carved one step
    /// down. Taken through [`crate::layout::carve_inside`], so the walls stay
    /// inside the rect and the gap beside a toggle is the gap, exactly as a
    /// TextBox's well and a spinbox's field are taken. `(rect, per-corner
    /// radii, wall width)`.
    ///
    /// The SINGLE source for the toggle's geometry: `paint` draws it here,
    /// [`Toggle::run`] places the run in it, and a host that draws the relief
    /// itself (`ParametersBg::fields`) asks for both.
    pub fn field(&self, rect: Rect) -> (Rect, crate::scene::paint::Radii, f32) {
        let r = crate::layout::toggle_corner_radius();
        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
        let (field, radii) = crate::layout::carve_inside(rect, (r, r, r, r), depth);
        (field, radii, depth)
    }

    /// The flush run, as the xs it spans in [`Toggle::field`]: half the field
    /// wide, placed by the animated `slide_t` — flush with the field's left
    /// end off, its right end on, a well on both sides between. As a text
    /// row's picker is, the run is laid out a wall wider than the face it
    /// carries (the field insets the face half a wall from the outline and
    /// from the seam), so the face reaches the well.
    pub fn run(&self, rect: Rect) -> (f32, f32) {
        let (field, _, _) = self.field(rect);
        let w = field.width * 0.5;
        let x = field.x + self.slide_t * (field.width - w);
        (x, x + w)
    }

    /// The run's FACE — what stands at the surface's level inside the
    /// field's valley: the run inset half a wall on every side, its corners
    /// the field's less that. `(rect, corner radius)`. The relief-off paint
    /// lights this rect, where the field cannot be carved.
    pub fn face(&self, rect: Rect) -> (Rect, f32) {
        let (field, radii, depth) = self.field(rect);
        let (a, b) = self.run(rect);
        let run = Rect { x: a, y: field.y, width: b - a, height: field.height };
        inset(run, radii.0, depth * 0.5)
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
        use crate::scene::paint::ControlPlate;
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);

        // A toggle paints NO fill of its own: it is worked out of the plate it
        // sits on, so the plate's own material (tint, blur, whatever it is)
        // shows through both the well's floor and the run's face, and the
        // state reads from light and relief alone — the DE's transparent-face
        // convention (closed dropdowns, inset troughs). The state colors this
        // used to tint with (enabled/disabled/background) are retired with the
        // rest of the toggle's palette.
        let (field, field_radii, depth) = self.field(rect);
        if self.raised() {
            // ONE field — the well and the flush run in it under one outline,
            // as a text row's picker and a spinbox's -/+ run are drawn. The
            // run's position IS the read: left off, right on, animated in
            // `tick`. A host that draws the relief itself
            // (`ParametersBg::fields`) asks for the same `field` and `run`.
            // Focus lights the field's rim, the ring every field wears.
            let focus = self.focused.then(ControlPlate::focus_tint);
            ctx.field_run(field, field_radii, depth, self.run(rect), focus);
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
            ctx.border(field, field_radii, [0.0; 4], colors::well_frame_color(self.hovered, self.focused), bw);
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
        let (id, ptr) = (cb.id(), cb.as_ptr_mut());
        ctx.register_widget(id, ptr);
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

    /// A labelled checkbox paints its ring-and-dot mark on the left and the label after
    /// it: no quads at all, one circle through the bridge once checked.
    #[test]
    fn checkbox_paints_ring_and_dot() {
        let ctx = UiContext::new();
        let mut cb = Checkbox::new().with_label("Enable");
        WidgetHost::set_rect(&mut cb, 0.0, 0.0, 200.0, 24.0);

        assert!(WidgetHost::extra_quads(&cb).is_empty());
        assert!(WidgetHost::extra_circles(&cb).is_empty());

        cb.inner_mut().set_checked(true);
        let circles = WidgetHost::extra_circles(&cb);
        assert_eq!(circles.len(), 1);
        let r = Checkbox::ROUND_RADIUS;
        assert_eq!(circles[0], (r, 12.0, r - 3.0, colors::TOGGLE_ON));

        // Label text comes through the prim-derived text bridge, after the mark.
        let labels = cb.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "Enable");
        assert_eq!(labels[0].x, 2.0 * r + 8.0);

        // Inline label => no set_rect inflation.
        assert_eq!(WidgetHost::rect(&cb), (0.0, 0.0, 200.0, 24.0));
        let _ = &ctx;
    }

    #[test]
    fn toggle_click_glides_the_run_across_its_field() {
        let mut ctx = UiContext::new();
        let mut t = Toggle::new();
        let (id, ptr) = (t.id(), t.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut t, 0.0, 0.0, 60.0, 30.0);

        let rect = Rect { x: 0.0, y: 0.0, width: 60.0, height: 30.0 };
        let painted = |t: &Adapted<Toggle>| {
            let mut pc = crate::scene::paint::PaintCtx::new();
            crate::widget::Paint::paint(t.inner(), rect, &mut pc);
            pc.finish().items.into_iter().map(|i| format!("{:?}", i.prim)).collect::<Vec<_>>()
        };
        let before = painted(&t);
        let plate_x = |t: &Adapted<Toggle>| t.inner().run(rect).0;
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
        let (field, radii, depth) = t.inner().field(rect);
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
        assert_eq!(split, fl, "off: the run reaches the field's left end — no well there");
        assert!((end - (fl + field.width * 0.5)).abs() < 1e-4, "half the field, the well beyond it");

        t.set_toggled(true); // programmatic syncs snap, so this is the on-end geometry
        let Prim::Field { split, end, .. } = fields(&t)[0] else { panic!() };
        assert!((split - (fl + field.width * 0.5)).abs() < 1e-4 && (end - fr).abs() < 1e-4, "on: the right half");

        t.slide_t = 0.5;
        let Prim::Field { split, end, .. } = fields(&t)[0] else { panic!() };
        assert!(split > fl + 1.0 && end < fr - 1.0, "mid-glide, a well either side of the run");
        assert!(end < fr + FIELD_RUN_ONLY, "and the run's end is its own, not the field's");

        let (face, face_r) = t.inner().face(rect);
        let (a, b) = t.inner().run(rect);
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
