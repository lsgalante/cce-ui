//! `SliderSpinbox`: a standard [`Slider`] with a standard [`Spinbox`] at its right end, holding
//! one number — an integer, or a float to a fixed number of decimals. Drag or scroll the band
//! for a coarse value; type, step or scroll the box for an exact one. The two are
//! [`Embedded`] children: once the composite is inserted they are the context's, linked under
//! it, so the router hands each its own presses, drags, wheel and keys, and each is its own
//! focus stop. The composite keeps them agreeing and is the one widget a host reads.
//!
//! The value is held as the spinbox holds it: whole units of `10^-decimals`, stepped within a
//! range ([`Spinbox::with_decimals`]), so an integer control is the zero-decimal case and a
//! float has no rounding drift. The slider maps the same range, unquantized — a wheel notch is
//! a fraction of the range and only adds up to a step over several notches — and the box shows
//! its value snapped to the step grid. A value set on the box (typed or stepped) moves the
//! band to it exactly.
//!
//! The children report as they change, so the box follows the band at the next tick or layout,
//! the cadence every composite's children are placed at (`register_embedded_children`), and
//! the composite reports `Changed` (its `take_change`) when the number it holds moves.

use crate::context::UiContext;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::input::{Slider, Spinbox};
use crate::widget::{Adapted, Embedded, Input, Layout, Paint, WidgetHost, WidgetId};

pub struct SliderSpinbox {
    slider: Embedded<Adapted<Slider>>,
    spinbox: Embedded<Adapted<Spinbox>>,
    /// The content rect the children are placed from.
    rect: Rect,
    /// The value, in units of `10^-decimals` (the spinbox's representation).
    units: i32,
    min: i32,
    max: i32,
    step: i32,
    decimals: u32,
    /// The slider's scaled value when the two last agreed: a slider that has moved off it was
    /// moved by hand.
    slider_seen: f32,
    /// A value set by the host while the children are the context's, for the next sync to
    /// hand them.
    push: bool,
    just_changed: bool,
}

impl SliderSpinbox {
    /// The spinbox's width at the right end of the row: its value field and its -/+ run.
    pub const SPINBOX_W: f32 = 100.0;
    /// Between the band's end and the spinbox — a slider's own readout gap.
    pub const GAP: f32 = Slider::READOUT_GAP;

    /// An integer control: `value` within `min..=max`, stepped by `step`.
    pub fn new_int(value: i32, min: i32, max: i32, step: i32) -> Adapted<SliderSpinbox> {
        Self::build(value, min, max, step, 0)
    }

    /// A float control to `decimals` places: `value` within `min..=max`, stepped by `step`.
    /// Each is rounded to the nearest `10^-decimals`; a step that rounds to nothing steps by
    /// one unit.
    pub fn new_float(value: f32, min: f32, max: f32, step: f32, decimals: u32) -> Adapted<SliderSpinbox> {
        let scale = 10f32.powi(decimals as i32);
        let units = |v: f32| (v * scale).round() as i32;
        Self::build(units(value), units(min), units(max), units(step), decimals)
    }

    fn build(value: i32, min: i32, max: i32, step: i32, decimals: u32) -> Adapted<SliderSpinbox> {
        let (min, max) = (min.min(max), min.max(max));
        let step = step.max(1);
        let units = value.clamp(min, max);
        let scale = 10f32.powi(decimals as i32);
        let mut slider = Slider::new().with_range(min as f32 / scale, max as f32 / scale);
        slider.set_scaled_value(units as f32 / scale);
        let slider_seen = slider.get_scaled_value();
        let spinbox = Spinbox::new(units, min, max, step).with_decimals(decimals);
        Adapted::new(SliderSpinbox {
            slider: Embedded::new(slider),
            spinbox: Embedded::new(spinbox),
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            units,
            min,
            max,
            step,
            decimals,
            slider_seen,
            push: false,
            just_changed: false,
        })
    }

    fn scale(&self) -> f32 {
        10f32.powi(self.decimals as i32)
    }

    /// The value. (`value()` is the adapter's integer read, as on every widget: here
    /// [`Self::int_value`].)
    pub fn float_value(&self) -> f32 {
        self.units as f32 / self.scale()
    }

    /// The value rounded to a whole number — exact for an integer control.
    pub fn int_value(&self) -> i32 {
        self.float_value().round() as i32
    }

    /// The value as the box shows it, to the control's decimals.
    pub fn value_text(&self) -> String {
        format!("{:.*}", self.decimals as usize, self.float_value())
    }

    /// The range, as given (rounded to the decimals).
    pub fn range(&self) -> (f32, f32) {
        (self.min as f32 / self.scale(), self.max as f32 / self.scale())
    }

    pub fn decimals(&self) -> u32 {
        self.decimals
    }

    /// The slider and the spinbox's ids, for a host that tells them apart in the event sweep.
    pub fn child_ids(&self) -> (WidgetId, WidgetId) {
        (self.slider.id(), self.spinbox.id())
    }

    /// Set the value, clamped to the range and snapped to the step grid. Like
    /// `Slider::set_value`, a typed setter: no change is reported for it.
    pub fn set_value(&mut self, value: f32) {
        self.units = self.snap((value * self.scale()).round() as i32);
        let real = self.float_value();
        match (self.slider.here_mut(), self.spinbox.here_mut()) {
            (Some(slider), Some(spinbox)) => {
                slider.set_scaled_value(real);
                spinbox.value = self.units;
                self.slider_seen = slider.get_scaled_value();
            }
            // The children are the context's: the next sync hands them the value.
            _ => self.push = true,
        }
    }

    /// `units` on the step grid from the range's start, within the range.
    fn snap(&self, units: i32) -> i32 {
        let k = ((units - self.min) as f32 / self.step as f32).round() as i32;
        (self.min + k * self.step).clamp(self.min, self.max)
    }

    /// The children's rects in the content rect `r`: the band left, the box right, each at
    /// its own height, centred in the row.
    fn child_rects(r: Rect) -> (Rect, Rect) {
        let spin_w = Self::SPINBOX_W.min(r.width);
        let slider_w = (r.width - spin_w - Self::GAP).max(0.0);
        let centred = |h: f32| r.y + ((r.height - h) * 0.5).max(0.0);
        let sh = crate::layout::slider_height().min(r.height);
        let bh = crate::layout::spinbox_height().min(r.height);
        (
            Rect { x: r.x, y: centred(sh), width: slider_w, height: sh },
            Rect { x: r.x + r.width - spin_w, y: centred(bh), width: spin_w, height: bh },
        )
    }

    /// Bring the two children to one value: the one a host set, else the box's if it moved,
    /// else the band's if it did.
    fn sync(&mut self, slider: &mut Slider, spinbox: &mut Spinbox) {
        let scale = self.scale();
        if std::mem::take(&mut self.push) {
            slider.set_scaled_value(self.units as f32 / scale);
            spinbox.value = self.units;
        } else if spinbox.value != self.units {
            // Typed or stepped: the band goes to it exactly.
            self.units = spinbox.value;
            slider.set_scaled_value(self.units as f32 / scale);
            self.just_changed = true;
        } else if (slider.get_scaled_value() - self.slider_seen).abs() > f32::EPSILON {
            // Dragged or scrolled: the box shows it on the step grid.
            let units = self.snap((slider.get_scaled_value() * scale).round() as i32);
            if units != self.units {
                self.units = units;
                spinbox.value = units;
                self.just_changed = true;
            }
        }
        self.slider_seen = slider.get_scaled_value();
    }
}

impl Adapted<SliderSpinbox> {
    /// A unit after the box's number (`Spinbox::with_unit`).
    pub fn with_unit(mut self, unit: &str) -> Self {
        if let Some(spinbox) = self.spinbox.here_mut() {
            spinbox.set_unit(unit);
        }
        self
    }

    pub fn with_value(mut self, value: f32) -> Self {
        self.set_value(value);
        self
    }
}

impl Layout for SliderSpinbox {
    /// One row, as tall as the taller of the two controls.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::slider_height().max(crate::layout::spinbox_height())))
    }

    /// Keep the rect the children are placed from, and place them while they are held here.
    fn arrange_children(&mut self, rect: Rect) {
        self.rect = rect;
        let (sr, br) = Self::child_rects(rect);
        if let Some(slider) = self.slider.here_mut() {
            slider.set_rect(sr.x, sr.y, sr.width, sr.height);
        }
        if let Some(spinbox) = self.spinbox.here_mut() {
            spinbox.set_rect(br.x, br.y, br.width, br.height);
        }
    }

    /// The children in the context, linked under the composite and placed side by side, then
    /// brought to one value — every layout and every tick.
    fn register_embedded_children(&mut self, host_id: WidgetId, ctx: &mut UiContext) {
        self.slider.attach(ctx);
        self.spinbox.attach(ctx);
        let (sr, br) = Self::child_rects(self.rect);
        let (Some(sh), Some(bh)) = (self.slider.handle(), self.spinbox.handle()) else { return };
        ctx[sh].set_rect(sr.x, sr.y, sr.width, sr.height);
        ctx[bh].set_rect(br.x, br.y, br.width, br.height);
        // Both out on loan at once for the sync.
        ctx.lend_h(sh, |slider, ctx| {
            ctx.lend_h(bh, |spinbox, _| self.sync(slider.inner_mut(), spinbox.inner_mut()));
        });
        ctx.link_ids(host_id, self.slider.id());
        ctx.link_ids(host_id, self.spinbox.id());
    }

    fn release_embedded_children(&mut self, ctx: &mut UiContext) {
        self.slider.detach(ctx);
        self.spinbox.detach(ctx);
    }
}

impl Paint for SliderSpinbox {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Nothing of its own: the children are painted by the walk, through the link (and, held
    /// here, the composite is never on screen).
    fn paint(&self, _rect: Rect, _ctx: &mut PaintCtx) {}
}

impl Input for SliderSpinbox {
    /// The children take the presses (the router reaches them first); the composite only
    /// holds them together.
    fn blocks_root_plate_drag(&self) -> bool {
        false
    }

    /// Ticked so the children are synced and placed even with no layout pass.
    fn wants_tick(&self) -> bool {
        true
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value(&self) -> i32 {
        self.int_value()
    }

    fn value_string(&self) -> Option<String> {
        Some(self.value_text())
    }

    /// A number in, as a host or a paste sets it: clamped, on the grid, and reported as a
    /// change when it moves the value.
    fn set_value_string(&mut self, val: &str) -> bool {
        let Ok(v) = val.trim().parse::<f32>() else { return false };
        let before = self.units;
        self.set_value(v);
        let changed = self.units != before;
        self.just_changed |= changed;
        changed
    }
}

/// Unused by the composite itself, which reads its children through the context: a held
/// child is reached through its cell. Kept so a test can see both before insertion.
#[cfg(test)]
impl SliderSpinbox {
    fn held(&self) -> (&Adapted<Slider>, &Adapted<Spinbox>) {
        (self.slider.here().expect("held"), self.spinbox.here().expect("held"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{ElementState, Event, Handle, MouseButton};

    fn placed(ctx: &mut UiContext, w: Adapted<SliderSpinbox>) -> Handle<Adapted<SliderSpinbox>> {
        let h = ctx.insert(w);
        ctx.lend_h(h, |c, ctx| {
            WidgetHost::set_rect(c, 0.0, 0.0, 300.0, 30.0);
            c.attach_embedded(ctx);
        });
        h
    }

    fn tick(ctx: &mut UiContext, h: Handle<Adapted<SliderSpinbox>>) {
        ctx.lend_h(h, |c, ctx| WidgetHost::tick(c, 0.016, ctx));
    }

    /// An integer control and a float one hold the number as the box does, and start the band
    /// at it.
    #[test]
    fn ints_and_floats_start_at_their_value() {
        let i = SliderSpinbox::new_int(7, 0, 10, 1);
        assert_eq!((i.float_value(), i.int_value(), i.value(), i.decimals()), (7.0, 7, 7, 0));
        let (slider, spinbox) = i.held();
        assert_eq!(spinbox.value, 7);
        assert!((slider.get_scaled_value() - 7.0).abs() < 1e-4);

        let f = SliderSpinbox::new_float(0.25, 0.0, 1.0, 0.05, 2);
        assert_eq!((f.float_value(), f.decimals(), f.range(), f.value_text()), (0.25, 2, (0.0, 1.0), "0.25".to_string()));
        let (slider, spinbox) = f.held();
        assert_eq!((spinbox.value, spinbox.decimals), (25, 2));
        assert!((slider.get_scaled_value() - 0.25).abs() < 1e-4);
    }

    /// The band left, the box right at its width, a gap between, both in the content rect;
    /// both children are the context's, linked under the composite, and leave with it.
    #[test]
    fn the_children_sit_side_by_side_and_are_the_contexts() {
        let mut ctx = UiContext::new();
        let h = placed(&mut ctx, SliderSpinbox::new_int(3, 0, 10, 1));
        let (sid, bid) = ctx[h].child_ids();
        let (sx, _, sw, _) = ctx.get_widget(sid).unwrap().rect();
        let (bx, _, bw, _) = ctx.get_widget(bid).unwrap().rect();
        assert_eq!((sx, bw), (0.0, SliderSpinbox::SPINBOX_W));
        assert_eq!(bx + bw, 300.0);
        assert_eq!(sx + sw + SliderSpinbox::GAP, bx);
        for id in [sid, bid] {
            assert!(ctx.tree.is_registered(id));
            assert!(ctx.tree.child_ids(h.id()).contains(&id), "linked under the composite");
        }
        let c = ctx.remove(h).expect("given back");
        assert!(!ctx.tree.is_registered(sid) && !ctx.tree.is_registered(bid));
        assert_eq!(c.held().1.value, 3);
    }

    /// The band moved by hand puts its value, snapped to the step, in the box, and the
    /// composite reports the change once.
    #[test]
    fn the_box_follows_the_band_on_the_step_grid() {
        let mut ctx = UiContext::new();
        let h = placed(&mut ctx, SliderSpinbox::new_float(0.5, 0.0, 1.0, 0.05, 2));
        let slider = ctx[h].slider.handle().unwrap();
        let spinbox = ctx[h].spinbox.handle().unwrap();
        ctx[slider].set_scaled_value(0.731);
        tick(&mut ctx, h);
        assert_eq!(ctx[spinbox].value, 75, "0.731 on the 0.05 grid");
        assert_eq!(ctx[h].float_value(), 0.75);
        assert!(ctx[h].take_change());
        assert!(!ctx[h].take_change(), "reported once");
        // A move that stays within the same step is no change.
        ctx[slider].set_scaled_value(0.74);
        tick(&mut ctx, h);
        assert!(!ctx[h].take_change());
        assert_eq!(ctx[spinbox].value, 75);
    }

    /// A press on the box's + goes to the box by the router, which steps it; the band
    /// follows to the new value exactly.
    #[test]
    fn the_band_follows_the_box() {
        let mut ctx = UiContext::new();
        let h = placed(&mut ctx, SliderSpinbox::new_int(4, 0, 10, 2));
        let (_, bid) = ctx[h].child_ids();
        let (bx, by, bw, bh) = ctx.get_widget(bid).unwrap().rect();
        let (px, py) = (bx + bw - 4.0, by + bh * 0.5);
        for state in [ElementState::Pressed, ElementState::Released] {
            ctx.propagate_event(&Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, local_x: px, local_y: py }, h.id());
        }
        tick(&mut ctx, h);
        assert_eq!(ctx[h].int_value(), 6, "stepped by 2");
        let slider = ctx[h].slider.handle().unwrap();
        assert!((ctx[slider].get_scaled_value() - 6.0).abs() < 1e-4);
        assert!(ctx[h].take_change());
    }

    /// A drag on the band, routed as an app routes the pointer to the composite, moves the
    /// band; the box follows it at the next tick.
    #[test]
    fn a_routed_drag_on_the_band_moves_the_value() {
        let mut ctx = UiContext::new();
        let h = placed(&mut ctx, SliderSpinbox::new_int(5, 0, 10, 1));
        let (sid, _) = ctx[h].child_ids();
        let (sx, sy, sw, sh) = ctx.get_widget(sid).unwrap().rect();
        let y = sy + sh * 0.5;
        let at = |x: f32| sx + sw * x;
        let ev = |state| Event::MouseButton { button: MouseButton::Left, state, x: at(0.5), y, local_x: at(0.5), local_y: y };
        assert!(ctx.propagate_event(&ev(ElementState::Pressed), h.id()), "the band takes the press");
        for f in [0.45, 0.4, 0.3, 0.2] {
            ctx.propagate_event(&Event::PointerMove { x: at(f), y, local_x: at(f), local_y: y }, h.id());
        }
        let release = Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x: at(0.2), y, local_x: at(0.2), local_y: y };
        ctx.propagate_event(&release, h.id());
        tick(&mut ctx, h);
        assert_eq!(ctx[h].int_value(), 2, "dragged to a fifth of the way");
    }

    /// A value string (a paste, a config line) sets the value on the grid and reports the
    /// change; one that is not a number is refused.
    #[test]
    fn a_value_string_sets_the_value() {
        let mut ctx = UiContext::new();
        let h = placed(&mut ctx, SliderSpinbox::new_float(0.5, 0.0, 1.0, 0.05, 2));
        assert!(ctx[h].set_value_string("0.62"));
        tick(&mut ctx, h);
        assert_eq!(ctx[h].value_string().as_deref(), Some("0.60"));
        let spinbox = ctx[h].spinbox.handle().unwrap();
        assert_eq!(ctx[spinbox].value, 60);
        assert!(ctx[h].take_change());
        assert!(!ctx[h].set_value_string("lots"));
    }

    /// A value the host sets reaches both children — at once while they are held here, at
    /// the next tick once they are the context's — clamped and on the grid, and is not
    /// reported as a change.
    #[test]
    fn a_set_value_reaches_both_children() {
        let held = SliderSpinbox::new_int(0, 0, 10, 1).with_value(4.4);
        assert_eq!(held.held().1.value, 4);

        let mut ctx = UiContext::new();
        let h = placed(&mut ctx, SliderSpinbox::new_int(0, 0, 10, 1));
        ctx[h].set_value(42.0);
        tick(&mut ctx, h);
        let spinbox = ctx[h].spinbox.handle().unwrap();
        let slider = ctx[h].slider.handle().unwrap();
        assert_eq!(ctx[spinbox].value, 10, "clamped to the range");
        assert!((ctx[slider].get_scaled_value() - 10.0).abs() < 1e-4);
        assert!(!ctx[h].take_change(), "a typed setter reports nothing");
    }
}
