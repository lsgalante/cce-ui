//! Narrow-trait `Float3` — a labeled group of three STANDARD [`Slider`]s (X/Y/Z), each with
//! the toolkit's readout, embedded by value inside `ParametersBg` (its only consumer), which
//! drives it through direct `WidgetHost` calls. The group label is the ordinary detached
//! control label (the adapter's, exactly like a slider row's); below it sit three `Adapted<Slider>` children in
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
pub(crate) const AXIS_W: f32 = 16.0;
/// Readout / edit-buffer precision of the rows, and of [`Float3::value_string`].
const DECIMALS: usize = 2;
/// The same with the trackball on. Turning a vector is a small change to
/// each of its components, and at two decimals a short one — a pull of 0.06
/// — has seven directions it can point in.
const BALL_DECIMALS: usize = 3;
/// Gap between the trackball and the axis-letter column.
const BALL_GAP: f32 = 10.0;

/// A trackball drag in progress: where the pointer last was, and the vector
/// being turned at FULL precision. The rows hold it rounded to their
/// readouts, and a host may write the rounded string back between moves;
/// turning that instead would lose every step smaller than a readout tick.
#[derive(Debug, Clone, Copy)]
struct BallDrag {
    last: (f32, f32),
    dir: [f32; 3],
    len: f32,
}

pub struct Float3 {
    /// The assigned (label-inclusive) rect.
    rect: Rect,
    sliders: [Adapted<Slider>; 3],
    axes: [&'static str; 3],
    label: Option<String>,
    dragging_idx: Option<usize>,
    /// Whether the group carries a TRACKBALL left of its rows: a ball the
    /// vector is drawn on and turned by. See [`Float3::set_trackball`].
    ball: bool,
    ball_drag: Option<BallDrag>,
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
            ball: false,
            ball_drag: None,
        })
    }

    /// Give the group a trackball, or take it away.
    ///
    /// The three sliders set a vector a component at a time, which is the
    /// wrong handle for its DIRECTION: pointing a pull somewhere else means
    /// moving all three, by amounts no one can do in their head. The ball
    /// is that handle. It sits left of the rows, as tall as they are, with
    /// the vector drawn on it from the centre — bright on the near side,
    /// dim when it points away — and dragging the ball rolls it under the
    /// pointer, turning the vector with it and keeping its length. The rows
    /// stay: they are still how a component is typed or a length changed.
    ///
    /// The view is fixed: X to the right, Y up, Z toward the viewer, the
    /// axes the rows are lettered by. A vector of no length has no
    /// direction to turn, so the first drag gives it a length of one.
    pub fn set_trackball(&mut self, on: bool) {
        self.ball = on;
        self.ball_drag = None;
        let decimals = self.decimals();
        for s in self.sliders.iter_mut() {
            s.set_decimals(decimals);
        }
        self.layout_rows();
    }

    pub fn has_trackball(&self) -> bool {
        self.ball
    }

    fn decimals(&self) -> usize {
        if self.ball { BALL_DECIMALS } else { DECIMALS }
    }

    /// The ball's diameter: the three rows' height, so it costs the group
    /// no height of its own.
    pub fn ball_diameter() -> f32 {
        3.0 * crate::layout::slider_height() + 2.0 * ROW_GAP
    }

    /// What the trackball takes of the group's width: the ball and its gap.
    /// A host measuring the rows' tracks counts it as chrome.
    pub fn trackball_chrome() -> f32 {
        Self::ball_diameter() + BALL_GAP
    }

    /// The ball as `(cx, cy, radius)`, when the group has one.
    pub fn ball_circle(&self) -> Option<(f32, f32, f32)> {
        if !self.ball {
            return None;
        }
        let r = Self::ball_diameter() * 0.5;
        Some((self.rect.x + r, self.rect.y + self.label_top() + r, r))
    }

    pub fn ball_hit(&self, px: f32, py: f32) -> bool {
        self.ball_circle().is_some_and(|(cx, cy, r)| (px - cx).powi(2) + (py - cy).powi(2) <= r * r)
    }

    /// The vector the rows hold, in their scaled values.
    pub fn vector(&self) -> [f32; 3] {
        [0, 1, 2].map(|i| self.sliders[i].get_scaled_value())
    }

    /// Roll the ball by a pointer delta: `dx` turns the vector about the
    /// vertical axis, `dy` about the horizontal one, by the angle that
    /// much of the ball's surface subtends — so the point under the
    /// pointer stays under it. Pure, for the tests.
    pub fn rolled(dir: [f32; 3], dx: f32, dy: f32, radius: f32) -> [f32; 3] {
        let (s, c) = (dx / radius).sin_cos();
        let (x, y, z) = (dir[0] * c + dir[2] * s, dir[1], -dir[0] * s + dir[2] * c);
        // Screen y runs down and the vector's Y up: a pull downward turns
        // the near point toward -Y.
        let (s, c) = (dy / radius).sin_cos();
        let out = [x, y * c - z * s, y * s + z * c];
        let len = (out[0] * out[0] + out[1] * out[1] + out[2] * out[2]).sqrt();
        if len > 0.0 { out.map(|v| v / len) } else { dir }
    }

    fn ball_begin(&mut self, px: f32, py: f32) {
        let v = self.vector();
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        let (dir, len) = if len > 1e-6 { (v.map(|c| c / len), len) } else { ([0.0, 0.0, 1.0], 1.0) };
        self.ball_drag = Some(BallDrag { last: (px, py), dir, len });
    }

    fn ball_roll(&mut self, px: f32, py: f32) -> bool {
        let (Some(mut drag), Some((_, _, r))) = (self.ball_drag, self.ball_circle()) else {
            return false;
        };
        let (dx, dy) = (px - drag.last.0, py - drag.last.1);
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        drag.dir = Self::rolled(drag.dir, dx, dy, r);
        drag.last = (px, py);
        self.ball_drag = Some(drag);
        for (s, c) in self.sliders.iter_mut().zip(drag.dir) {
            s.set_scaled_value(c * drag.len);
        }
        true
    }

    /// The trackball: the ball, and the vector on it. Emitted through the
    /// host's scene path (`ParametersBg::paint_scene_rows`) rather than
    /// [`Paint::paint`], because a sphere is not a prim the legacy flat
    /// views carry.
    pub fn paint_ball(&self, ctx: &mut PaintCtx) {
        let Some((cx, cy, r)) = self.ball_circle() else {
            return;
        };
        let held = self.ball_drag.is_some();
        let body = if held { [0.26, 0.29, 0.40, 1.0] } else { [0.19, 0.21, 0.29, 1.0] };
        ctx.sphere(cx, cy, r, &crate::scene::material::Material::from_fill(body));
        ctx.arc(cx, cy, r, 1.0, 0.0, std::f32::consts::TAU, [0.42, 0.45, 0.58, 0.9]);

        // While held the ball shows the vector it is turning, not the one
        // the rows rounded it to.
        let v = match self.ball_drag {
            Some(d) => d.dir.map(|c| c * d.len),
            None => self.vector(),
        };
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        // The accent's HUE at alphas of the ball's own: the configured
        // accent carries an alpha meant for washes, and at that alpha the
        // near side drew paler than the far one.
        let accent = crate::color::highlight_primary_color();
        let accent = |a: f32| [accent[0], accent[1], accent[2], a];
        if len <= 1e-6 {
            // No length, no direction: a dim hub and nothing on the ball.
            ctx.circle(cx, cy, 2.5, accent(0.35));
            return;
        }
        let d = v.map(|c| c / len);
        // The tip sits on the ball's surface as seen from +Z, a little in
        // from the rim so a vector lying in the screen plane stays on it.
        let reach = r - 5.0;
        let (tx, ty) = (cx + d[0] * reach, cy - d[1] * reach);
        let near = d[2] >= 0.0;
        let col = accent(if near { 1.0 } else { 0.35 });
        ctx.vector(cx, cy, tx, ty, 2.0, col, crate::scene::paint::Cap::Round);
        ctx.circle(cx, cy, 2.0, col);
        ctx.circle(tx, ty, if near { 4.5 } else { 3.0 }, col);
    }

    /// The height a labeled (`labeled`) group lays out to: the detached label band plus three slider rows and their gaps — the row-height table entry.
    pub fn preferred_height(labeled: bool) -> f32 {
        let top = if labeled { crate::layout::control_label_strip() } else { 0.0 };
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
        let d = self.decimals();
        format!("{:.*}:{:.*}:{:.*}", d, v(0), d, v(1), d, v(2))
    }

    /// The three rows, X/Y/Z — for hosts that draw this group through the legacy flat views
    /// and need each row's relief prims (`track_relief`, `thumb_sphere`) over its
    /// [`Self::get_row_rects`] rect.
    pub fn sliders(&self) -> &[Adapted<Slider>; 3] {
        &self.sliders
    }

    /// Detached-label band above the rows (zero unlabeled) — the adapter's
    /// `Widget::label_offset` over the synced label.
    fn label_top(&self) -> f32 {
        crate::widget::input::slider::detached_strip(&self.label)
    }

    /// The three slider rows' rects (`(x, y, w, h)`, X/Y/Z), laid out below the label band and
    /// right of the axis-letter column. Each is exactly the rect its sub-slider is assigned.
    pub fn get_row_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let top = self.rect.y + self.label_top();
        let ball = if self.ball { Self::trackball_chrome() } else { 0.0 };
        let x = self.rect.x + ball + AXIS_W;
        let w = (self.rect.width - ball - AXIS_W).max(10.0);
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
    ///
    /// ONE row takes it: the latched one, else the NEAREST of the rows whose
    /// halo holds the pointer. A halo reaches past its band by more than the
    /// gap between rows, so two rows' halos hold any point between them —
    /// and until 2026-09-28 the first in X/Y/Z order won, so a scroll over
    /// the Y band turned X.
    pub fn wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ui: &mut UiContext) -> bool {
        let Some(i) = self.wheel_row(px, py, ui) else {
            return false;
        };
        let s = &mut self.sliders[i];
        let was_scroll = s.scroll_enabled;
        s.set_scroll(true);
        let taken = s.mouse_wheel_ungated(delta, px, py, ui);
        s.set_scroll(was_scroll);
        taken
    }

    /// The row a wheel at `(px, py)` belongs to, if any — see [`Self::wheel`] —
    /// with its band centre's distance from the pointer for a host choosing
    /// between this group and its neighbours.
    pub fn wheel_row(&self, px: f32, py: f32, ui: &UiContext) -> Option<usize> {
        let latched = self
            .sliders
            .iter()
            .position(|s| !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(s.base().id()));
        latched.or_else(|| self.nearest_band(px, py).map(|(i, _)| i))
    }

    /// The nearest row whose halo holds the pointer, with the distance from
    /// the pointer to that band's centre line.
    pub fn nearest_band(&self, px: f32, py: f32) -> Option<(usize, f32)> {
        self.get_row_rects()
            .into_iter()
            .enumerate()
            .filter(|(i, r)| self.sliders[*i].inner().scroll_hit(Rect { x: r.0, y: r.1, width: r.2, height: r.3 }, px, py))
            .map(|(i, r)| (i, (py - (r.1 + r.3 * 0.5)).abs()))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

impl Adapted<Float3> {
    /// See [`Float3::set_trackball`].
    pub fn with_trackball(mut self, on: bool) -> Self {
        self.set_trackball(on);
        self
    }

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
            // Bounded to the row PLUS its gutter: the axis letter is drawn to
            // the left of the row rect by design, so the row alone would clip
            // it away entirely.
            ctx.text_with(
                self.axes[i].to_string(),
                rect.x - AXIS_W + 2.0,
                crate::layout::align_text_y(rect.y, rect.height, 12.0, 0.0),
                12.0,
                [0xaa, 0xaa, 0xbb],
                None,
                Some([rect.x - AXIS_W, rect.y, rect.x + rect.width, rect.y + rect.height]),
            );
            Paint::paint(&*self.sliders[i], rect, ctx);
        }
    }
}

impl Input for Float3 {
    fn draggable(&self, _rect: Rect) -> bool {
        self.dragging_idx.is_some() || self.ball_drag.is_some()
    }

    fn is_dragging(&self) -> bool {
        self.dragging_idx.is_some() || self.ball_drag.is_some()
    }

    /// A host-driven drag begins on the row under the pointer — unless a press already
    /// started one (the pane's press path), in which case that row keeps it. The ball
    /// comes first: it stands beside the rows, inside their span of y.
    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        if self.dragging_idx.is_some() || self.ball_drag.is_some() {
            return;
        }
        if self.ball_hit(px, py) {
            self.ball_begin(px, py);
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
        if self.ball_drag.is_some() {
            return self.ball_roll(px, py);
        }
        match self.dragging_idx {
            Some(i) => self.sliders[i].drag_update(px, py),
            None => false,
        }
    }

    fn drag_end(&mut self) {
        self.ball_drag = None;
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
                        // A press on the ball takes hold of it; the drag
                        // that follows turns the vector (`drag_update`).
                        if self.ball_hit(*px, *py) {
                            self.ball_begin(*px, *py);
                            return true;
                        }
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
                        if self.ball_drag.take().is_some() {
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

    /// The trackball stands left of the rows, as tall as they are, and
    /// dragging it rolls the vector: a quarter turn of the ball's surface
    /// to the right carries a vector pointing at the viewer onto +X, one
    /// downward onto -Y, and the length is kept. The rows read to a third
    /// decimal, and a vector of no length is given one on the first drag.
    #[test]
    fn the_trackball_turns_the_vector_and_keeps_its_length() {
        let mut ctx = UiContext::new();
        let quarter = |r: f32| r * std::f32::consts::FRAC_PI_2;
        let group = |v: [f32; 3]| {
            // Range -10..10: a scaled value v is (v + 10) / 20 normalized.
            let mut f = Float3::new().with_range(-10.0, 10.0).with_values(v.map(|c| (c + 10.0) / 20.0)).with_trackball(true);
            WidgetHost::set_rect(&mut f, 0.0, 0.0, 400.0, Float3::preferred_height(false));
            f
        };
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 2e-3);

        let plain = {
            let mut f = Float3::new();
            WidgetHost::set_rect(&mut f, 0.0, 0.0, 400.0, Float3::preferred_height(false));
            f.get_row_rects()[0]
        };
        let mut f = group([0.0, 0.0, 2.0]);
        let (cx, cy, r) = f.ball_circle().expect("a ball");
        assert_eq!(r * 2.0, Float3::ball_diameter());
        let row = f.get_row_rects()[0];
        assert_eq!(row.0, plain.0 + Float3::trackball_chrome(), "the rows start past the ball");
        assert_eq!(row.2, plain.2 - Float3::trackball_chrome());
        assert_eq!(f.value_string(), "0.000:0.000:2.000", "three decimals with the ball on");

        // A press on the ball takes hold; the drag rolls it a quarter turn right.
        assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, cx, cy, &mut ctx));
        assert!(f.is_dragging());
        assert!(f.drag_update(cx + quarter(r), cy));
        assert!(close(f.vector(), [2.0, 0.0, 0.0]), "{:?}", f.vector());
        f.drag_end();
        assert!(!f.is_dragging());

        // Downward: the near point turns toward -Y.
        let mut f = group([0.0, 0.0, 2.0]);
        f.drag_begin(cx, cy);
        assert!(f.drag_update(cx, cy + quarter(r)));
        assert!(close(f.vector(), [0.0, -2.0, 0.0]), "{:?}", f.vector());

        // Many small moves add up to what one large one does: the drag
        // turns its own full-precision copy, not the rounded rows.
        let mut f = group([0.0, 0.0, 0.06]);
        f.drag_begin(cx, cy);
        for i in 1..=100 {
            f.drag_update(cx + quarter(r) * i as f32 / 100.0, cy);
        }
        assert!(close(f.vector(), [0.06, 0.0, 0.0]), "{:?}", f.vector());

        // No length: the first drag gives it one.
        let mut f = group([0.0, 0.0, 0.0]);
        f.drag_begin(cx, cy);
        assert!(f.drag_update(cx + quarter(r), cy));
        assert!(close(f.vector(), [1.0, 0.0, 0.0]), "{:?}", f.vector());

        // Off the ball a press is the rows', as before.
        let mut f = group([0.0, 0.0, 2.0]);
        let row = f.get_row_rects()[0];
        assert!(!f.ball_hit(row.0 + 20.0, row.1 + row.3 * 0.5));
        assert!(f.mouse_input(MouseButton::Left, ElementState::Pressed, row.0 + 20.0, row.1 + row.3 * 0.5, &mut ctx));
        assert!(f.is_dragging());
        f.drag_update(row.0 + 60.0, row.1 + row.3 * 0.5);
        assert!(f.vector()[1] == 0.0 && f.vector()[2] == 2.0, "only X moved: {:?}", f.vector());

        // The ball paints a sphere and the vector on it; without one, nothing.
        let mut pc = PaintCtx::new();
        group([0.0, 0.0, 2.0]).paint_ball(&mut pc);
        let prims: Vec<_> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        assert!(prims.iter().any(|p| matches!(p, crate::scene::paint::Prim::Sphere { .. })), "{prims:?}");
        assert!(prims.iter().any(|p| matches!(p, crate::scene::paint::Prim::Vector { .. })));
        let mut pc = PaintCtx::new();
        Float3::new().paint_ball(&mut pc);
        assert!(pc.finish().items.is_empty());
    }
}
