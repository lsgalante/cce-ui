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

/// A plate that slides in a well: the toggle's footprint is a track carved
/// into the plate it sits on, and a half-width control plate stands on that
/// well's floor at the left (off) or right (on) end, gliding between them.
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
    /// Relief style: the track is a real carved well and the glider a raised
    /// plate standing in it. Without it the track falls back to the hairline
    /// frame every well shares and the plate to a lit face — see `paint`.
    raised: Option<bool>,
    /// The glider's animated position along its well, 0 (left/off) → 1 (right/on).
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

    pub fn toggled(&self) -> bool {
        self.toggled
    }

    /// The well the glider lives in: the toggle's whole footprint carved one
    /// step down. Taken through [`crate::layout::carve_inside`], so the walls
    /// stay inside the rect and the gap beside a toggle is the gap, exactly as
    /// a TextBox's well is taken. `(rect, per-corner radii, wall width)`.
    ///
    /// The SINGLE source for the track geometry: `paint` carves it here and
    /// [`Toggle::slide_plate`] measures the floor from it.
    pub fn well(&self, rect: Rect) -> (Rect, crate::scene::paint::Radii, f32) {
        let r = crate::layout::toggle_corner_radius();
        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
        let (well, radii) = crate::layout::carve_inside(rect, (r, r, r, r), depth);
        (well, radii, depth)
    }

    /// The sliding plate, as `(footprint, corner radius, wall width)`: half the
    /// well's floor wide, gliding between the floor's ends by the animated
    /// `slide_t` — left is off, right is on.
    ///
    /// It stands ON the floor — the flat region inside the well's walls, which
    /// start half a wall in from the well's own boundary — and its roll abuts
    /// that floor's edge instead of shading over the well's wall. Its corners
    /// run concentric with the well's.
    ///
    /// Its wall is HALF the well's. A plate in a well is the shallower part of
    /// the pair, and at a control's height it has to be: a toggle is 24px, a
    /// well wall 4.8, and two full-depth walls stacked leave the boss no flat
    /// top at all — its own walls meet in the middle and the plate reads as a
    /// ridge drawn across the track rather than a thing standing in it.
    pub fn slide_plate(&self, rect: Rect) -> (Rect, f32, f32) {
        let (well, radii, wd) = self.well(rect);
        let (floor, floor_r) = inset(well, radii.0, wd * 0.5);
        let pd = (wd * 0.5).max(1.0);
        let (travel, pr) = inset(floor, floor_r, pd * 0.5);
        let pw = travel.width * 0.5;
        let plate = Rect {
            x: travel.x + self.slide_t * (travel.width - pw),
            y: travel.y,
            width: pw,
            height: travel.height,
        };
        (plate, pr, pd)
    }

    /// The carves this Toggle paints: the track's well (a `Recess`) and the
    /// glider standing in it (a `Boss`, the faceless raised plate
    /// [`crate::scene::paint::PaintCtx::control_plate`] emits) — in that order,
    /// the order `paint` emits them.
    ///
    /// Exists because a Toggle paints NO fill in any state: on a legacy-view
    /// host (`ParametersBg::reliefs`) these carves are the ENTIRE control, and
    /// without them the row is a bare label. Empty without `raised` styling,
    /// where the frame and the lit face stand in for them.
    pub fn flat_carves(&self, rect: Rect) -> Vec<crate::layout::ReliefCarve> {
        use crate::layout::{CarveKind, ReliefCarve};
        if !self.raised() {
            return Vec::new();
        }
        let all = (true, true, true, true);
        let (well, well_radii, depth) = self.well(rect);
        let (plate, pr, pd) = self.slide_plate(rect);
        let (boss, boss_radii) = crate::layout::carve_inside(plate, (pr, pr, pr, pr), pd);
        vec![
            ReliefCarve {
                kind: CarveKind::Recess { tint: None },
                x: well.x,
                y: well.y,
                w: well.width,
                h: well.height,
                radii: well_radii,
                depth,
                edges: all,
            },
            ReliefCarve {
                kind: CarveKind::Boss { tint: None },
                x: boss.x,
                y: boss.y,
                w: boss.width,
                h: boss.height,
                radii: boss_radii,
                depth: pd,
                edges: all,
            },
        ]
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
        use crate::scene::paint::{ControlPlate, PlateStance};
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);

        // A toggle paints NO fill of its own: it is worked out of the plate it
        // sits on, so the plate's own material (tint, blur, whatever it is)
        // shows through both the well's floor and the glider's face, and the
        // state reads from light and relief alone — the DE's transparent-face
        // convention (closed dropdowns, inset troughs). The state colors this
        // used to tint with (enabled/disabled/background) are retired with the
        // rest of the toggle's palette.
        let (well, well_radii, depth) = self.well(rect);
        let (plate, plate_r, plate_depth) = self.slide_plate(rect);
        if self.raised() {
            // The track is a WELL — the same recess a TextBox carves — and the
            // glider is a control plate standing on its floor, raised faceless
            // (a `Boss`, the surface below as its face). Its position IS the
            // read: left off, right on, animated in `tick`. Both also reach
            // legacy-view hosts through `flat_carves` (see
            // `ParametersBg::reliefs`), which must emit them in this order.
            ctx.recess(well, well_radii, depth);
            let focus = if self.focused { Some(ControlPlate::focus_tint()) } else { None };
            ctx.control_plate(
                &ControlPlate::control(plate, plate_r, PlateStance::Raised, [0.0; 4])
                    .with_depth(plate_depth)
                    .with_tint(focus),
            );
        } else {
            // Relief off: a well is its frame — the one hairline every well
            // falls back to, lit while focused, exactly `well_rim`'s flat arm —
            // and the plate standing in it is a lit face, the neutral overlay
            // the DE gives a surface it cannot carve (what the rocker's halves
            // wore). Scaled by the relief strength, as that lighting was.
            //
            // The plate's overlay is a ROUNDED RECT on purpose: the legacy
            // reverse bridge reads only those, never a `Border`, so a
            // legacy-view host (`ParametersBg`, whose carves are gated on
            // relief) still shows which end the plate is at.
            let bw = crate::layout::toggle_border_width().max(1.0);
            ctx.border(well, well_radii, [0.0; 4], colors::well_frame_color(self.hovered, self.focused), bw);
            let lit = (0.16 * (crate::layout::bevel_depth() / 0.15)).clamp(0.0, 0.5);
            ctx.rounded_rect(plate, plate_r, (true, true, true, true), [1.0, 1.0, 1.0, lit]);
        }

        if let Some(ref label) = self.label {
            let (font_fam, font_size) = crate::layout::control_label_font_parsed();
            let est_w = crate::widget::display::measure_text_width(label, &font_fam, font_size);
            let tx = match self.justify {
                Justification::Left => x + crate::layout::CONTROL_TEXT_INSET,
                Justification::Right => x + w - est_w - crate::layout::CONTROL_TEXT_INSET,
                Justification::Center => x + (w - est_w) / 2.0,
            };
            // The label never moves with the state. When the glider covers it
            // the text shows through: the plate carries no face of its own, and
            // the glyphs land in the engine's later text pass either way.
            //
            // Focus is the glider plate's own lit rim (`ControlPlate::with_tint`)
            // — the ring every other plate wears, which the rocker's partial
            // carves could not — so the label stays the label.
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
    fn toggle_click_glides_the_plate_across_its_well() {
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
        let plate_x = |t: &Adapted<Toggle>| t.inner().slide_plate(rect).0.x;
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

    /// The plate stands ON the well's floor, clear of its walls by one wall
    /// width on every side, and travels between the floor's ends: off is flush
    /// left, on is flush right, and it never reaches outside the toggle's rect.
    #[test]
    fn toggle_plate_lives_inside_the_well() {
        let rect = Rect { x: 10.0, y: 4.0, width: 120.0, height: 24.0 };
        let mut t = Toggle::new();

        let (well, _, depth) = t.inner().well(rect);
        assert!(well.x >= rect.x && well.y >= rect.y, "the well carves inside the rect");

        // The floor is the flat region inside the well's walls; the plate's own
        // (half-depth) roll abuts its edge, so the plate is inset one more
        // half-wall of its own from there.
        let (off, _, pd) = t.inner().slide_plate(rect);
        assert!((pd - depth * 0.5).abs() < 1e-4, "the plate's wall is half the well's");
        let edge = depth * 0.5 + pd * 0.5;
        assert!((off.x - (well.x + edge)).abs() < 1e-4, "off sits at the travel's left end");
        assert!((off.y - (well.y + edge)).abs() < 1e-4, "and clear of the floor's top");
        assert!(
            off.height > pd * 2.0,
            "the plate keeps a flat top: a boss no taller than its own wall is a ridge",
        );

        t.set_toggled(true); // programmatic syncs snap, so this is the on-end geometry
        let (on, _, _) = t.inner().slide_plate(rect);
        assert!(on.x > off.x, "on is to the right of off");
        assert!(
            (on.x + on.width - (well.x + well.width - edge)).abs() < 1e-4,
            "on sits flush against the travel's right end",
        );
        assert!(on.x + on.width <= rect.x + rect.width, "and never leaves the toggle's rect");
        assert!((off.width * 2.0 - (well.width - 2.0 * edge)).abs() < 1e-4, "half the travel wide");
    }

    /// The carves a legacy-view host re-emits (`ParametersBg::reliefs`) are the
    /// well then the plate, in the order `paint` emits them — and nothing at
    /// all with relief off, where the flat frames stand in.
    #[test]
    fn toggle_flat_carves_are_the_well_then_the_plate() {
        use crate::layout::CarveKind;
        let rect = Rect { x: 0.0, y: 0.0, width: 120.0, height: 24.0 };
        let t = Toggle::new().with_raised(true);
        let carves = t.inner().flat_carves(rect);
        assert_eq!(carves.len(), 2);
        assert!(matches!(carves[0].kind, CarveKind::Recess { .. }), "the track's well first");
        assert!(matches!(carves[1].kind, CarveKind::Boss { .. }), "then the plate standing in it");
        assert!(carves.iter().all(|c| c.edges == (true, true, true, true)), "both are full rings");

        let flat = Toggle::new().with_raised(false);
        assert!(flat.inner().flat_carves(rect).is_empty(), "relief off carves nothing");
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
