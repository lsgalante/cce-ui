//! `Slider2D` — a two-axis pad control: one thumb dragged across a recessed
//! square well maps to an `(x, y)` pair in 0..1 × 0..1, y-up. Follows the
//! `Slider` narrow-trait shape: the adapter's detached label above the pad, host-driven drags through
//! the `Input` drag hooks, and `take_change`-based polling by composites.
//!
//! A keyboard stop (`FocusRole::Well`): focused, the arrows step the thumb on their axis and
//! the pad's rim lights. To a screen reader it is a group of two sliders, x and y.

use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, MouseScrollDelta, NamedKey,
    Paint,
};

#[derive(Debug, Clone)]
pub struct Slider2D {
    dragging: bool,
    /// Keyboard focus (FocusIn / FocusOut): the arrows move the thumb, and the pad's rim is
    /// lit in the highlight.
    focused: bool,
    pub(crate) value_x: f32,
    pub(crate) value_y: f32,
    pub just_changed: bool,
    label: Option<String>,
}

impl Slider2D {
    /// Thumb radius; also the pad's inner inset so the thumb center's range
    /// keeps the whole thumb inside the well.
    const THUMB_R: f32 = 6.0;

    /// What an arrow key or a wheel notch moves the thumb on its axis: a `Slider`'s notch.
    const STEP: f32 = 0.02;

    pub fn new() -> Adapted<Slider2D> {
        Adapted::new(Slider2D {
            dragging: false,
            focused: false,
            value_x: 0.5,
            value_y: 0.5,
            just_changed: false,
            label: None,
        })
    }

    pub fn value_x(&self) -> f32 {
        self.value_x
    }

    pub fn value_y(&self) -> f32 {
        self.value_y
    }

    pub fn set_values(&mut self, x: f32, y: f32) {
        self.value_x = x.clamp(0.0, 1.0);
        self.value_y = y.clamp(0.0, 1.0);
    }

    /// Move the thumb to `(x, y)`, each clamped to 0..1, marking a change (`take_change`)
    /// when it moved. Returns whether it did.
    fn move_to(&mut self, x: f32, y: f32) -> bool {
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        if (x - self.value_x).abs() > 0.0001 || (y - self.value_y).abs() > 0.0001 {
            self.value_x = x;
            self.value_y = y;
            self.just_changed = true;
            true
        } else {
            false
        }
    }

    /// The pad's two axes as a screen reader's sliders, `labels` naming x then y: each a
    /// number in 0..1 stepped as the arrows step it, set through [`set_axis`](Self::set_axis),
    /// over the whole pad (`rect`).
    pub(crate) fn axis_items(&self, rect: Rect, labels: [String; 2]) -> Vec<crate::a11y::A11yItem> {
        let [x, y] = labels;
        [(x, self.value_x), (y, self.value_y)]
            .into_iter()
            .map(|(label, value)| crate::a11y::A11yItem {
                value: Some(format!("{value:.3}")),
                range: Some((0.0, 1.0, f64::from(Self::STEP))),
                ..crate::a11y::A11yItem::new(accesskit::Role::Slider, label, rect)
            })
            .collect()
    }

    /// Set axis `axis` (0 x, 1 y) to `value`, clamped, marking a change when it moved — a
    /// reader's number for one of [`axis_items`](Self::axis_items). Returns whether it moved.
    pub(crate) fn set_axis(&mut self, axis: usize, value: f64) -> bool {
        let v = value as f32;
        match axis {
            0 => self.move_to(v, self.value_y),
            1 => self.move_to(self.value_x, v),
            _ => false,
        }
    }

    fn thumb_center(&self, rect: Rect) -> (f32, f32) {
        let inset = Self::THUMB_R + 2.0;
        (
            rect.x + inset + self.value_x * (rect.width - 2.0 * inset).max(1.0),
            rect.y + inset + (1.0 - self.value_y) * (rect.height - 2.0 * inset).max(1.0),
        )
    }

    fn set_from_point(&mut self, px: f32, py: f32, rect: Rect) -> bool {
        let inset = Self::THUMB_R + 2.0;
        let w = (rect.width - 2.0 * inset).max(1.0);
        let h = (rect.height - 2.0 * inset).max(1.0);
        self.move_to((px - rect.x - inset) / w, 1.0 - (py - rect.y - inset) / h)
    }
}

impl Layout for Slider2D {
    /// A 120px pad: the height of the other drag-in canvases (Trackpad, the previews).
    /// At 64 it sat in a control column as a small square with the column's width to
    /// spare. (The Ramp's key pad is its own `PAD_SIDE`, not this.)
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(120.0, 120.0))
    }

    fn intrinsic_measure_width(&self) -> bool {
        true
    }
}

impl Paint for Slider2D {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The well: the shared canvas floor (`PaintCtx::well_floor`) read as an
        // opening cut into the host's plate, rim drawn last so its shading
        // falls over the content at the edges. At the panel radius.
        let radius = crate::layout::panel_corner_radius();
        ctx.well_floor(rect, radius, &crate::scene::Material::pane(), false);

        // Crosshair through the thumb — the pad's read of both axis values.
        let (cx, cy) = self.thumb_center(rect);
        let line = [1.0, 1.0, 1.0, 0.16];
        ctx.quad(Rect { x: rect.x + 2.0, y: cy - 0.5, width: rect.width - 4.0, height: 1.0 }, line);
        ctx.quad(Rect { x: cx - 0.5, y: rect.y + 2.0, width: 1.0, height: rect.height - 4.0 }, line);

        // Thumb: glassy fill in a thin white ring.
        let fill_a = if self.dragging { 0.9 } else { 0.35 };
        ctx.circle(cx, cy, Self::THUMB_R - 1.0, [0.5, 0.75, 1.0, fill_a]);
        ctx.arc(
            cx,
            cy,
            Self::THUMB_R + 1.0,
            2.0,
            0.0,
            std::f32::consts::TAU,
            [1.0, 1.0, 1.0, 0.9],
        );

        // The well's rim (`PaintCtx::well_rim_lit`), carved inside the pad's rect and lit
        // while the pad has the keyboard; the control label above is the adapter's, outside
        // the well like every control's.
        ctx.well_rim_lit(rect, radius, crate::layout::control_relief(), self.focused);
    }
}

impl Input for Slider2D {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, .. } => {
                match state {
                    ElementState::Pressed => {
                        let r = ectx.rect;
                        if *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height {
                            self.dragging = true;
                            self.set_from_point(*px, *py, r);
                            return true;
                        }
                        false
                    }
                    ElementState::Released => std::mem::take(&mut self.dragging),
                }
            }
            Event::PointerMove { x: px, y: py, .. } => {
                if self.dragging {
                    self.set_from_point(*px, *py, ectx.rect)
                } else {
                    false
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                // Vertical wheel nudges the y axis (the Slider gesture-gated
                // pattern), hovering anywhere over the pad.
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if !ui.scroll_gesture_new && ui.scroll_initiate_widget_id != Some(ectx.id) {
                        return false;
                    }
                    let r = ectx.rect;
                    if *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height {
                        if ui.scroll_gesture_new {
                            ui.scroll_initiate_widget_id = Some(ectx.id);
                        }
                        let amount = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => *y,
                            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                        };
                        self.move_to(self.value_x, self.value_y - amount * Self::STEP);
                        return true;
                    }
                }
                false
            }
            Event::FocusIn => {
                self.focused = true;
                true
            }
            Event::FocusOut => {
                self.focused = false;
                true
            }
            Event::KeyInput(key_event) => {
                // A focused pad: the arrows step the thumb by a notch on their axis (Up is
                // more, y-up), Home / End send x to its ends, PageDown / PageUp y to its.
                // Unfocused, the keys are not this pad's (a `Slider`'s rule).
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                let (x, y) = (self.value_x, self.value_y);
                let (x, y) = match key_event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => (x - Self::STEP, y),
                    Key::Named(NamedKey::ArrowRight) => (x + Self::STEP, y),
                    Key::Named(NamedKey::ArrowDown) => (x, y - Self::STEP),
                    Key::Named(NamedKey::ArrowUp) => (x, y + Self::STEP),
                    Key::Named(NamedKey::Home) => (0.0, y),
                    Key::Named(NamedKey::End) => (1.0, y),
                    Key::Named(NamedKey::PageDown) => (x, 0.0),
                    Key::Named(NamedKey::PageUp) => (x, 1.0),
                    _ => return false,
                };
                self.move_to(x, y);
                true
            }
            _ => false,
        }
    }

    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    fn a11y_items(&self, rect: Rect) -> Vec<crate::a11y::A11yItem> {
        self.axis_items(rect, [crate::l10n::tr("slider2d-x"), crate::l10n::tr("slider2d-y")])
    }

    fn a11y_set_item_value(&mut self, idx: usize, value: f64) -> bool {
        self.set_axis(idx, value)
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn draggable(&self, _rect: Rect) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }
    fn drag_begin(&mut self, px: f32, py: f32, rect: Rect) {
        self.dragging = true;
        self.set_from_point(px, py, rect);
    }
    fn drag_update(&mut self, px: f32, py: f32, rect: Rect) -> bool {
        self.set_from_point(px, py, rect)
    }
    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(format!("{:.3},{:.3}", self.value_x, self.value_y))
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some((xs, ys)) = val.split_once(',') else { return false };
        let (Ok(x), Ok(y)) = (xs.trim().parse::<f32>(), ys.trim().parse::<f32>()) else {
            return false;
        };
        let (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
        let changed = (x - self.value_x).abs() > 0.0005 || (y - self.value_y).abs() > 0.0005;
        self.value_x = x;
        self.value_y = y;
        if changed {
            self.just_changed = true;
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::Prim;
    use crate::widget::{KeyEvent, UiContext, WidgetHost, WidgetHostExt};
    use accesskit::{Action, Role};

    fn press(key: NamedKey) -> Event {
        Event::KeyInput(KeyEvent { logical_key: Key::Named(key), state: ElementState::Pressed, text: None, repeat: false, ctrl: false, shift: false, alt: false })
    }

    /// A focused pad: Left / Right step x by a notch, Up / Down y (Up is more), Home / End
    /// send x to its ends and PageDown / PageUp y to its, each move marked as a change; a key
    /// that moves nothing is still the pad's, and marks none. Unfocused, the keys are not its.
    #[test]
    fn a_focused_pad_steps_its_thumb_on_each_axis() {
        let mut ctx = UiContext::new();
        let mut p = Slider2D::new();
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 120.0, 120.0);
        let at = |p: &Adapted<Slider2D>| (p.inner().value_x(), p.inner().value_y());
        let near = |(x, y): (f32, f32), (ex, ey): (f32, f32)| (x - ex).abs() < 1e-5 && (y - ey).abs() < 1e-5;
        assert!(!p.handle_event(&press(NamedKey::ArrowRight), &mut ctx), "unfocused: not this pad's key");
        assert!(!p.take_change() && at(&p) == (0.5, 0.5));

        p.handle_event(&Event::FocusIn, &mut ctx);
        for (key, want) in [
            (NamedKey::ArrowRight, (0.52, 0.5)),
            (NamedKey::ArrowUp, (0.52, 0.52)),
            (NamedKey::ArrowLeft, (0.5, 0.52)),
            (NamedKey::ArrowDown, (0.5, 0.5)),
            (NamedKey::End, (1.0, 0.5)),
            (NamedKey::PageUp, (1.0, 1.0)),
            (NamedKey::Home, (0.0, 1.0)),
            (NamedKey::PageDown, (0.0, 0.0)),
        ] {
            assert!(p.handle_event(&press(key), &mut ctx), "{key:?}");
            assert!(near(at(&p), want), "{key:?}: {:?}, not {want:?}", at(&p));
            assert!(p.take_change(), "{key:?} marks the change");
        }
        assert!(p.handle_event(&press(NamedKey::ArrowLeft), &mut ctx), "at the end, still the pad's");
        assert!(!p.take_change() && at(&p) == (0.0, 0.0), "nothing moved");
        assert!(!p.handle_event(&press(NamedKey::Tab), &mut ctx), "not a key it moves by");

        p.handle_event(&Event::FocusOut, &mut ctx);
        assert!(!p.handle_event(&press(NamedKey::ArrowUp), &mut ctx), "the keyboard went: the keys did too");
        assert_eq!(at(&p), (0.0, 0.0));
    }

    /// Focused, the pad's rim is lit in the highlight: under relief the recess's rim tinted,
    /// flat the frame in its active colour; unfocused, neither.
    #[test]
    fn a_focused_pad_lights_its_rim() {
        let mut ctx = UiContext::new();
        let mut p = Slider2D::new();
        let rect = Rect { x: 0.0, y: 0.0, width: 120.0, height: 120.0 };
        let hc = crate::color::highlight_primary_color();
        let lit = |p: &Adapted<Slider2D>| {
            let mut pc = PaintCtx::new();
            Paint::paint(p.inner(), rect, &mut pc);
            pc.finish().items.into_iter().any(|i| match i.prim {
                Prim::Recess { tint, .. } => tint == Some([hc[0], hc[1], hc[2]]),
                Prim::Border { border, .. } => border == crate::color::well_frame_color(false, true),
                _ => false,
            })
        };
        assert!(!lit(&p), "unfocused: the plain rim");
        p.handle_event(&Event::FocusIn, &mut ctx);
        assert!(lit(&p), "focused: the rim lit");
        p.handle_event(&Event::FocusOut, &mut ctx);
        assert!(!lit(&p));

        // The flat arm, whichever style the test runs under.
        let mut pc = PaintCtx::new();
        pc.well_rim_lit(rect, 4.0, false, true);
        assert!(matches!(pc.finish().items[..], [ref i] if matches!(i.prim, Prim::Border { border, .. } if border == crate::color::well_frame_color(false, true))));
    }

    /// To a screen reader the pad is a group you can focus, its axes two sliders X and Y:
    /// numbers in 0..1 at the arrows' step, settable by number, which moves the thumb and
    /// marks the change. The group itself is stepped by nothing.
    #[test]
    fn a_pad_is_a_group_of_two_sliders() {
        use crate::a11y::{item_id, node_id, tree_update};
        let mut ctx = UiContext::new();
        let p = ctx.insert(Slider2D::new().with_label("Key"));
        ctx[p].set_rect(10.0, 10.0, 120.0, 120.0);
        ctx[p].inner_mut().set_values(0.25, 0.75);
        let update = tree_update(&ctx, "Pad", 1.0);
        let node = |id| &update.nodes.iter().find(|(n, _)| *n == id).expect("node in the update").1;
        let group = node(node_id(p.id()));
        assert_eq!((group.role(), group.label()), (Role::Group, Some("Key")));
        assert!(group.supports_action(Action::Focus), "a keyboard stop");
        assert!(!group.supports_action(Action::Increment) && !group.supports_action(Action::Decrement));
        assert_eq!(group.children(), &[item_id(p.id(), 0), item_id(p.id(), 1)]);
        for (i, label, value) in [(0, "X", 0.25), (1, "Y", 0.75)] {
            let axis = node(item_id(p.id(), i));
            assert_eq!((axis.role(), axis.label(), axis.numeric_value()), (Role::Slider, Some(label), Some(value)));
            assert_eq!((axis.min_numeric_value(), axis.max_numeric_value()), (Some(0.0), Some(1.0)));
            assert!((axis.numeric_value_step().unwrap() - 0.02).abs() < 1e-6);
            assert!(axis.supports_action(Action::SetValue));
        }

        assert!(ctx[p].a11y_set_item_value(1, 0.5));
        assert_eq!((ctx[p].inner().value_x(), ctx[p].inner().value_y()), (0.25, 0.5));
        assert!(ctx[p].take_change(), "a reader's number is a change like any");
        assert!(ctx[p].a11y_set_item_value(0, 2.0), "clamped");
        assert_eq!(ctx[p].inner().value_x(), 1.0);
        assert!(!ctx[p].a11y_set_item_value(0, 1.0), "already there");
        assert!(!ctx[p].a11y_set_item_value(2, 0.5), "no third axis");
    }
}
