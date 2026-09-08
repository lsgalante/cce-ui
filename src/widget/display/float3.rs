//! Narrow-trait `Float3` — a labeled group of three STANDARD [`Slider`]s (X/Y/Z), each with
//! the toolkit's readout, embedded by value inside `ParametersBg` (its only consumer), which
//! drives it through direct `WidgetHost` calls. The group label is the ordinary detached
//! control label (the adapter's, exactly like a slider row's — `inflates_label_rect = false`,
//! the label eats into the assigned rect); below it sit three `Adapted<Slider>` children in
//! whatever style the DE config gives every other slider (the band that swallowed the rodent,
//! the recessed well, the square track), each fronted by its axis letter. The model caches its
//! laid-out rect ([`Layout::rect_assigned`]) and lays the children out from it; paint and input
//! delegate to them, so the rows look and feel like a plain slider row rather than the bespoke
//! flat track + square thumb this widget used to draw.
//!
//! Hosts reading this panel through the legacy flat views get the children's quads, rounded
//! rects and text through the adapter's prim bridges (the sub-sliders paint into this widget's
//! own [`Paint::paint`]); their relief prims (recessed-track carve, thumb sphere) cannot ride
//! those views and travel through [`Float3::sliders`] + [`Float3::get_row_rects`] instead, the
//! way `ParametersBg::reliefs` / `spheres` read a slider row.

use crate::context::UiContext;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::input::Slider;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, MouseScrollDelta,
    Paint, WidgetHost,
};

/// Vertical gap between the three slider rows.
const ROW_GAP: f32 = 4.0;
/// The axis-letter column left of each slider.
const AXIS_W: f32 = 16.0;
/// Readout / edit-buffer precision of the rows, and of [`Float3::value_string`].
const DECIMALS: usize = 2;

pub struct Float3 {
    /// The assigned (label-inclusive) rect.
    rect: Rect,
    sliders: [Adapted<Slider>; 3],
    axes: [&'static str; 3],
    label: Option<String>,
    dragging_idx: Option<usize>,
}

impl Float3 {
    pub fn new() -> Adapted<Float3> {
        let row = || Slider::new().with_readout(true).with_decimals(DECIMALS);
        Adapted::new(Float3 {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            sliders: [row(), row(), row()],
            axes: ["X", "Y", "Z"],
            label: None,
            dragging_idx: None,
        })
    }

    /// The height a labeled (`labeled`) group lays out to: the detached label band (none in
    /// the side layout) plus three slider rows and their gaps — the row-height table entry.
    pub fn preferred_height(labeled: bool) -> f32 {
        let top = if labeled && crate::layout::control_label_layout() != "side" {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        };
        top + 3.0 * crate::layout::slider_height() + 2.0 * ROW_GAP
    }

    /// Normalized (0..1) values, X/Y/Z.
    pub fn values(&self) -> [f32; 3] {
        [self.sliders[0].value, self.sliders[1].value, self.sliders[2].value]
    }

    /// Normalized values in; each row clamps to 0..1.
    pub fn set_values(&mut self, values: [f32; 3]) {
        for (s, v) in self.sliders.iter_mut().zip(values) {
            s.set_value(v);
        }
    }

    /// The row whose readout is open for typing, if any.
    pub fn editing_idx(&self) -> Option<usize> {
        self.sliders.iter().position(|s| s.editing)
    }

    /// The scaled values as the `x:y:z` row string (`DECIMALS` places) the parameter pane
    /// stores — the one formatter for every host sync.
    pub fn value_string(&self) -> String {
        let v = |i: usize| self.sliders[i].get_scaled_value();
        format!("{:.*}:{:.*}:{:.*}", DECIMALS, v(0), DECIMALS, v(1), DECIMALS, v(2))
    }

    /// The three rows, X/Y/Z — for hosts that draw this group through the legacy flat views
    /// and need each row's relief prims (`track_relief`, `thumb_sphere`) over its
    /// [`Self::get_row_rects`] rect.
    pub fn sliders(&self) -> &[Adapted<Slider>; 3] {
        &self.sliders
    }

    /// Detached-label band above the rows — a replica of `Widget::label_offset` over the
    /// synced label (zero in the side layout or unlabeled), the Slider's own formula.
    fn label_top(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        if self.label.is_some() {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        }
    }

    /// The side-layout label inset (`WidgetHost::label_x_offset` — this type is not exempt).
    fn side_offset(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" && self.label.is_some() {
            90.0
        } else {
            0.0
        }
    }

    /// The three slider rows' rects (`(x, y, w, h)`, X/Y/Z), laid out below the label band and
    /// right of the axis-letter column. Each is exactly the rect its sub-slider is assigned.
    pub fn get_row_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let top = self.rect.y + self.label_top();
        let side = self.side_offset();
        let x = self.rect.x + side + AXIS_W;
        let w = (self.rect.width - side - AXIS_W).max(10.0);
        let h = crate::layout::slider_height();
        (0..3).map(|i| (x, top + i as f32 * (h + ROW_GAP), w, h)).collect()
    }

    fn layout_rows(&mut self) {
        let rows = self.get_row_rects();
        for (s, r) in self.sliders.iter_mut().zip(rows) {
            s.set_rect(r.0, r.1, r.2, r.3);
        }
    }

    /// Wheel over the rows, the parameter pane's slider-row contract: under the band style the
    /// capture zone is each row's shape halo ([`Slider::scroll_hit`]) or its gesture latch,
    /// otherwise the row's rect; a row in zone takes the wheel ungated (the halo already gated
    /// spatially, and the adapter's rect gate would clip its fringe). Returns whether a row took
    /// it, whether or not the value string ticked over.
    pub fn wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let rows = self.get_row_rects();
        let band = crate::layout::slider_band();
        for (s, r) in self.sliders.iter_mut().zip(rows) {
            let rect = Rect { x: r.0, y: r.1, width: r.2, height: r.3 };
            let in_zone = if band {
                let latched = !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(s.base().id());
                latched || s.inner().scroll_hit(rect, px, py)
            } else {
                py >= rect.y && py <= rect.y + rect.height
            };
            if !in_zone {
                continue;
            }
            let was_scroll = s.scroll_enabled;
            s.set_scroll(true);
            let taken = s.mouse_wheel_ungated(delta, px, py, ui);
            s.set_scroll(was_scroll);
            if taken {
                return true;
            }
        }
        false
    }
}

impl Adapted<Float3> {
    pub fn with_values(mut self, values: [f32; 3]) -> Self {
        self.set_values(values);
        self
    }

    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        for s in self.sliders.iter_mut() {
            s.set_range(min, max);
        }
        self
    }
}

impl Layout for Float3 {
    /// The Slider convention: the label eats into the assigned rect, the host sizes the row
    /// for it ([`Float3::preferred_height`]).
    fn inflates_label_rect(&self) -> bool {
        false
    }

    /// Detached label x inset — the Slider/Dropdown value, so the group label lines up with a
    /// slider row's.
    fn detached_label_inset(&self) -> f32 {
        4.0
    }

    /// The three rows alone: the adapter adds the detached-label strip itself
    /// (`Adapted::preferred_height`), as it does for every non-inflating widget.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, Float3::preferred_height(false)))
    }

    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
        self.layout_rows();
    }
}

impl Paint for Float3 {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
        self.layout_rows();
    }

    /// The axis letters and the three rows, each painted by its own slider over its row rect
    /// (an unlabeled slider's content rect is its whole rect). The group label is the adapter's
    /// detached label, like a slider row's.
    fn paint(&self, _rect: Rect, ctx: &mut PaintCtx) {
        let rows = self.get_row_rects();
        for (i, r) in rows.into_iter().enumerate() {
            let rect = Rect { x: r.0, y: r.1, width: r.2, height: r.3 };
            ctx.text(
                self.axes[i].to_string(),
                rect.x - AXIS_W + 2.0,
                crate::layout::align_text_y(rect.y, rect.height, 12.0, 0.0),
                12.0,
                [0xaa, 0xaa, 0xbb],
            );
            Paint::paint(&*self.sliders[i], rect, ctx);
        }
    }
}

impl Input for Float3 {
    fn draggable(&self, _rect: Rect) -> bool {
        self.dragging_idx.is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_idx.is_some()
    }

    /// A host-driven drag begins on the row under the pointer — unless a press already
    /// started one (the pane's press path), in which case that row keeps it.
    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        if self.dragging_idx.is_some() {
            return;
        }
        let rows = self.get_row_rects();
        for (i, r) in rows.into_iter().enumerate() {
            if py >= r.1 && py <= r.1 + r.3 {
                self.sliders[i].drag_begin(px, py);
                self.dragging_idx = Some(i);
                return;
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        match self.dragging_idx {
            Some(i) => self.sliders[i].drag_update(px, py),
            None => false,
        }
    }

    fn drag_end(&mut self) {
        if let Some(i) = self.dragging_idx.take() {
            self.sliders[i].drag_end();
        }
    }

    /// The rows' wheel-glide inertia.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        let mut dummy = UiContext::new();
        let mut changed = false;
        for s in self.sliders.iter_mut() {
            changed |= WidgetHost::tick(s, dt, &mut dummy);
        }
        changed
    }

    fn take_change(&mut self) -> bool {
        let mut any = false;
        for s in self.sliders.iter_mut() {
            any |= s.take_change();
        }
        any
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                let mut dummy = UiContext::new();
                match state {
                    ElementState::Pressed => {
                        let rows = self.get_row_rects();
                        for (i, r) in rows.into_iter().enumerate() {
                            if *py < r.1 || *py > r.1 + r.3 {
                                continue;
                            }
                            // The child's own readout click claims focus through the
                            // dummy ctx (a no-op beyond the thread-local slot); the
                            // GROUP is the host's focus target, as before.
                            if !self.sliders[i].mouse_input(*button, *state, *px, *py, &mut dummy) {
                                continue;
                            }
                            if self.sliders[i].is_dragging() {
                                self.dragging_idx = Some(i);
                            }
                            if self.sliders[i].editing {
                                for (j, s) in self.sliders.iter_mut().enumerate() {
                                    if j != i && s.editing {
                                        s.unfocus();
                                    }
                                }
                                ectx.request_focus();
                            }
                            return true;
                        }
                        false
                    }
                    ElementState::Released => {
                        let mut any = false;
                        for s in self.sliders.iter_mut() {
                            any |= s.mouse_input(*button, *state, *px, *py, &mut dummy);
                        }
                        if self.dragging_idx.take().is_some() {
                            any = true;
                        }
                        any
                    }
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let mut dummy = UiContext::new();
                let ui = ectx.ui.as_deref_mut();
                match ui {
                    Some(ui) => self.wheel(delta, *px, *py, ui),
                    None => self.wheel(delta, *px, *py, &mut dummy),
                }
            }
            Event::PointerMove { x: px, y: py, .. } => {
                let mut dummy = UiContext::new();
                let mut changed = false;
                for s in self.sliders.iter_mut() {
                    changed |= s.on_cursor_moved(*px, *py, &mut dummy);
                }
                changed
            }
            Event::KeyInput(key_event) => {
                let mut dummy = UiContext::new();
                for s in self.sliders.iter_mut() {
                    if s.editing {
                        return s.keyboard_input(key_event, &mut dummy);
                    }
                }
                false
            }
            // Focus loss commits every open readout edit (each row's own FocusOut).
            Event::FocusOut => {
                for s in self.sliders.iter_mut() {
                    s.unfocus();
                }
                true
            }
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ParametersBg drive pattern: readout click opens the row's edit, Enter/unfocus
    /// commits back into the normalized value, a track press starts a drag.
    #[test]
    fn readout_edit_commits_on_unfocus() {
        let mut ctx = UiContext::new();
        let mut f = Float3::new().with_values([0.5, 0.5, 0.5]).with_range(0.0, 10.0);
        WidgetHost::set_rect(&mut f, 0.0, 0.0, 300.0, Float3::preferred_height(false));

        let rows = f.get_row_rects();
        assert_eq!(rows.len(), 3);
        assert_eq!(f.value_string(), "5.00:5.00:5.00");
        // Click row 1's readout (the 60px box at the row's right end).
        let rx = rows[1].0 + rows[1].2 - 30.0;
        let ry = rows[1].1 + rows[1].3 * 0.5;
        assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, rx, ry, &mut ctx));
        assert_eq!(f.editing_idx(), Some(1));

        f.sliders[1].set_value_string("7.5");
        WidgetHost::unfocus(&mut f);
        assert_eq!(f.editing_idx(), None);
        assert!((f.values()[1] - 0.75).abs() < 1e-4, "7.5 of 0..10 normalizes to 0.75");

        // Track press starts a drag; drag_update moves the value; release ends it.
        let track_x = rows[0].0 + 20.0;
        let track_y = rows[0].1 + rows[0].3 * 0.5;
        assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, track_x, track_y, &mut ctx));
        assert!(f.is_dragging());
        f.drag_update(track_x + 100.0, track_y);
        assert!(f.values()[0] > 0.5, "drag right raises the value");
        f.drag_end();
        assert!(!f.is_dragging());
    }
}
