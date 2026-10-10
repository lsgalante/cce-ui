//! `Float3`: a labeled group of two to four STANDARD [`Slider`]s (X Y, X Y Z, X Y Z W), each with
//! the toolkit's readout and fronted by its axis letter, embedded by value inside `ParametersBg`
//! (its only consumer). The group label is the ordinary detached control label (the adapter's,
//! exactly like a slider row's); the rows are `Adapted<Slider>` children in whatever style the DE
//! config gives every other slider. The model caches its laid-out rect
//! ([`Layout::rect_assigned`]) and lays the rows out from it; paint and input delegate to them,
//! so the rows look and feel like plain slider rows. A three-wide group can carry a trackball
//! left of its rows (`ball`).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, construction, components and values, the rows' layout, `impl Layout` and `impl Paint` |
//! | `ball` | the trackball: the host's camera view, rolling the vector, its rings, painting it, its drag and scroll |
//! | `input` | the wheel routed to a row, the nearest band, `impl Input` |

mod ball;
mod input;
#[cfg(test)]
mod tests;

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

/// The rings on the ball: circles of latitude about the vector, this many
/// degrees from it. Five, a sixth of a half turn apart, so there is one on
/// the equator and the far hemisphere carries its own pair — a vector
/// pointing away still shows rings on the side that faces out.
const RING_ANGLES: [f32; 5] = [30.0, 60.0, 90.0, 120.0, 150.0];

/// Segments a ring is drawn in.
const RING_SEGMENTS: usize = 48;

/// How far one wheel notch rolls the ball. A drag is 1:1 with the ball's
/// surface; a scroll is the fine handle, a quarter turn in six notches.
const SCROLL_TURN: f32 = std::f32::consts::PI / 12.0;

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
    /// Four rows, of which the first [`Float3::components`] are the group's
    /// (three unless a host asks for two or four — `float2` / `float4`
    /// parameter rows). The rest are never laid out, drawn or hit.
    sliders: [Adapted<Slider>; 4],
    axes: [&'static str; 4],
    /// How many rows the group has, 1..=4; three by default.
    n: usize,
    label: Option<String>,
    dragging_idx: Option<usize>,
    /// Whether the group carries a TRACKBALL left of its rows: a ball the
    /// vector is drawn on and turned by. See [`Float3::set_trackball`].
    ball: bool,
    ball_drag: Option<BallDrag>,
    /// The ball's own id in the scroll-gesture bookkeeping
    /// (`UiContext::scroll_initiate_widget_id`): a gesture that begins on
    /// the ball is the ball's until it ends, as one on a band is the band's.
    ball_id: crate::widget::WidgetId,
    /// The vector a SCROLL is turning, at full precision, as a direction
    /// and a length — what [`BallDrag`] is to a drag. Kept between events
    /// for as long as the rows still hold what it rounds to
    /// ([`Float3::fine`]); a trackpad sends a pixel at a time, and a pixel
    /// turns a short vector by less than the rows can hold.
    fine: Option<([f32; 3], f32)>,
    /// The view the ball is seen from: the vector's space to the view's,
    /// as three rows — the view's right, its up, and the axis toward the
    /// viewer, each a direction in the vector's space. The identity (X
    /// right, Y up, Z toward the viewer) until a host sets one
    /// ([`Float3::set_view`]).
    view: [[f32; 3]; 3],
}

/// The ball's view with nothing set: X right, Y up, Z toward the viewer.
pub const IDENTITY_VIEW: [[f32; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

impl Float3 {
    pub fn new() -> Adapted<Float3> {
        let row = || Slider::new().with_readout(true).with_decimals(DECIMALS);
        Adapted::new(Float3 {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            sliders: [row(), row(), row(), row()],
            axes: ["X", "Y", "Z", "W"],
            n: 3,
            label: None,
            dragging_idx: None,
            ball: false,
            ball_drag: None,
            ball_id: crate::widget::WidgetId(crate::widget::NEXT_WIDGET_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)),
            fine: None,
            view: IDENTITY_VIEW,
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
    /// The view is X to the right, Y up, Z toward the viewer — the axes
    /// the rows are lettered by — until a host gives it a camera
    /// ([`Float3::set_view`]). A vector of no length has no direction to
    /// turn, so the first drag gives it a length of one, toward the viewer.
    pub fn set_trackball(&mut self, on: bool) {
        // A direction is three numbers: two or four have no ball.
        self.ball = on && self.n == 3;
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

    /// Give the group `n` rows (1..=4: X, Y, Z, W) — a `float2` or
    /// `float4` value is the same control with fewer or more of them. A
    /// group of other than three has no trackball.
    pub fn set_components(&mut self, n: usize) {
        self.n = n.clamp(1, 4);
        if self.n != 3 {
            self.ball = false;
            self.ball_drag = None;
        }
        self.layout_rows();
    }

    /// How many rows the group has.
    pub fn components(&self) -> usize {
        self.n
    }

    /// Give every row a soft range (`Slider::set_soft`): a value typed
    /// past an end widens that row's range.
    pub fn set_soft(&mut self, soft: bool) {
        for s in self.sliders.iter_mut() {
            s.set_soft(soft);
        }
    }

    fn decimals(&self) -> usize {
        if self.ball { BALL_DECIMALS } else { DECIMALS }
    }

    /// The height a labeled (`labeled`) group lays out to: the detached label band plus three slider rows and their gaps — the row-height table entry.
    pub fn preferred_height(labeled: bool) -> f32 {
        Self::preferred_height_for(labeled, 3)
    }

    /// [`Self::preferred_height`] for a group of `n` rows.
    pub fn preferred_height_for(labeled: bool, n: usize) -> f32 {
        let n = n.clamp(1, 4) as f32;
        let top = if labeled { crate::layout::control_label_strip() } else { 0.0 };
        top + n * crate::layout::slider_height() + (n - 1.0) * ROW_GAP
    }

    /// Normalized (0..1) values, X/Y/Z.
    pub fn values(&self) -> [f32; 3] {
        [self.sliders[0].value, self.sliders[1].value, self.sliders[2].value]
    }

    /// The scaled values of the group's rows, as many as it has.
    pub fn scaled_values(&self) -> Vec<f32> {
        self.sliders[..self.n].iter().map(|s| s.get_scaled_value()).collect()
    }

    /// Normalized values in for the group's rows, as many as it has; each
    /// row clamps to 0..1, and a row with no value given keeps its own.
    pub fn set_values_n(&mut self, values: &[f32]) {
        for (s, v) in self.sliders[..self.n].iter_mut().zip(values) {
            s.set_value(*v);
        }
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
        let d = self.decimals();
        self.sliders[..self.n].iter().map(|s| format!("{:.*}", d, s.get_scaled_value())).collect::<Vec<_>>().join(":")
    }

    /// The three rows, X/Y/Z — for hosts that draw this group through the legacy flat views
    /// and need each row's relief prims (`track_relief`, `thumb_sphere`) over its
    /// [`Self::get_row_rects`] rect.
    pub fn sliders(&self) -> &[Adapted<Slider>] {
        &self.sliders[..self.n]
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
        (0..self.n).map(|i| (x, top + i as f32 * (h + ROW_GAP), w, h)).collect()
    }

    fn layout_rows(&mut self) {
        let rows = self.get_row_rects();
        for (i, s) in self.sliders.iter_mut().enumerate() {
            // A row past the group's count is laid out nowhere, so it is
            // hit by nothing.
            let r = rows.get(i).copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
            s.set_rect(r.0, r.1, r.2, r.3);
        }
    }
}

impl Adapted<Float3> {
    /// See [`Float3::set_trackball`].
    pub fn with_trackball(mut self, on: bool) -> Self {
        self.set_trackball(on);
        self
    }

    /// See [`Float3::set_components`].
    pub fn with_components(mut self, n: usize) -> Self {
        self.set_components(n);
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
    // The Slider convention: the label eats into the assigned rect, the host sizes the row
    // for it ([`Float3::preferred_height`]).

    /// The three rows alone: the adapter adds the detached-label strip itself
    /// (`Adapted::preferred_height`), as it does for every non-inflating widget.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, Float3::preferred_height_for(false, self.n)))
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
