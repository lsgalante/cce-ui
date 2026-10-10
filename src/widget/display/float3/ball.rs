//! The trackball: a ball left of a three-wide group with the vector drawn on it, seen from the
//! host's camera (`set_view`). A drag rolls it under the pointer and a scroll rolls it as content
//! is scrolled, each turning a full-precision copy of the vector; its rings are circles of latitude
//! about the vector, so a rotation reads.

use super::*;

impl Float3 {
    /// See the ball from a host's camera: `view`'s rows are the camera's
    /// right, its up, and the direction from the scene toward it, in the
    /// vector's own space. The vector is then drawn on the ball as it lies
    /// in the host's 3D view — pointing at the viewer on the ball when it
    /// points at the camera in the scene — and a drag or a scroll rolls it
    /// about the camera's axes, so pushing the ball right swings the vector
    /// to the right of the SCREEN, whatever that is in the scene. The rows
    /// and the value are untouched: only what the ball shows and how it
    /// turns. Rows that are not unit length or not square to each other
    /// are made so, and a degenerate view is refused.
    pub fn set_view(&mut self, view: [[f32; 3]; 3]) -> bool {
        let dot = |a: [f32; 3], b: [f32; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let unit = |v: [f32; 3]| {
            let l = dot(v, v).sqrt();
            (l > 1e-6).then(|| v.map(|c| c / l))
        };
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        // Toward the viewer is kept; right is squared to it, and up follows.
        let Some(toward) = unit(view[2]) else { return false };
        let along = dot(view[0], toward);
        let Some(right) = unit([0, 1, 2].map(|k| view[0][k] - along * toward[k])) else { return false };
        let up = cross(toward, right);
        self.view = [right, up, toward];
        true
    }

    pub fn view(&self) -> [[f32; 3]; 3] {
        self.view
    }

    /// A direction of the vector's space, as the view sees it.
    pub(super) fn to_view(&self, v: [f32; 3]) -> [f32; 3] {
        self.view.map(|row| row[0] * v[0] + row[1] * v[1] + row[2] * v[2])
    }

    /// And back: a direction of the view's, in the vector's space.
    #[allow(clippy::wrong_self_convention)] // a transform's inverse, beside `to_view`
    pub(super) fn from_view(&self, p: [f32; 3]) -> [f32; 3] {
        [0, 1, 2].map(|k| self.view[0][k] * p[0] + self.view[1][k] * p[1] + self.view[2][k] * p[2])
    }

    /// Roll a direction of the vector's space as the VIEW sees the ball
    /// roll: into the view, [`Self::rolled`], and back.
    pub(super) fn rolled_in_view(&self, dir: [f32; 3], dx: f32, dy: f32, radius: f32) -> [f32; 3] {
        self.from_view(Self::rolled(self.to_view(dir), dx, dy, radius))
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

    /// One ring on the unit ball: the circle of points `degrees` from
    /// `dir`, as `segments + 1` points, the last closing on the first. A
    /// circle of latitude about the vector as its pole — so the rings are
    /// the vector's own, and turn exactly as it does. Seen from the front
    /// they are concentric circles when the vector points at the viewer
    /// and foreshorten into ellipses as it turns away, which is what makes
    /// a rotation readable on a ball that is otherwise the same from every
    /// side.
    pub fn ring(dir: [f32; 3], degrees: f32, segments: usize) -> Vec<[f32; 3]> {
        let cross = |a: [f32; 3], b: [f32; 3]| [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
        let unit = |v: [f32; 3]| {
            let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            if l > 0.0 { v.map(|c| c / l) } else { v }
        };
        let d = unit(dir);
        // Any axis not along the vector gives a basis across it; which one
        // only moves where on the circle the points start.
        let aside = if d[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let u = unit(cross(d, aside));
        let w = cross(d, u);
        let (st, ct) = degrees.to_radians().sin_cos();
        (0..=segments)
            .map(|i| {
                let (sp, cp) = (i as f32 / segments as f32 * std::f32::consts::TAU).sin_cos();
                [0, 1, 2].map(|k| ct * d[k] + st * (cp * u[k] + sp * w[k]))
            })
            .collect()
    }

    pub(super) fn ball_begin(&mut self, px: f32, py: f32) {
        let (dir, len) = self.fine();
        self.ball_drag = Some(BallDrag { last: (px, py), dir, len });
    }

    /// The vector to turn, as a direction and a length: the full-precision
    /// copy the last scroll left, while the rows still hold what it rounds
    /// to — within half a readout tick and the rows' own float resolution
    /// over their range — and the rows' vector otherwise (someone typed, or
    /// dragged a band). A vector of no length points at the viewer with a
    /// length of one.
    pub(super) fn fine(&self) -> ([f32; 3], f32) {
        let v = self.vector();
        if let Some((dir, len)) = self.fine {
            let (min, max) = self.sliders[0].range();
            let tol = 0.5 * 10f32.powi(-(self.decimals() as i32)) + (max - min).abs() * 5e-7;
            if (0..3).all(|i| (dir[i] * len - v[i]).abs() <= tol) {
                return (dir, len);
            }
        }
        let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        // No length: toward the viewer, wherever the view puts that.
        if len > 1e-6 { (v.map(|c| c / len), len) } else { (self.view[2], 1.0) }
    }

    pub fn ball_id(&self) -> crate::widget::WidgetId {
        self.ball_id
    }

    /// Roll the ball by a scroll: the ball is scrolled as content is, its
    /// surface moving the way a page under the pointer would — a two-finger
    /// gesture in both axes at once, a wheel notch in one — by
    /// `SCROLL_TURN` a notch. The length is kept.
    pub fn ball_scroll(&mut self, delta: &MouseScrollDelta) -> bool {
        let Some((_, _, r)) = self.ball_circle() else {
            return false;
        };
        let (nx, ny) = match delta {
            MouseScrollDelta::LineDelta(x, y) => (*x, *y),
            MouseScrollDelta::PixelDelta(pos) => (pos.x as f32 / 60.0, pos.y as f32 / 60.0),
        };
        if nx == 0.0 && ny == 0.0 {
            return false;
        }
        let (dir, len) = self.fine();
        let dir = self.rolled_in_view(dir, nx * SCROLL_TURN * r, ny * SCROLL_TURN * r, r);
        self.fine = Some((dir, len));
        for (s, c) in self.sliders.iter_mut().zip(dir) {
            s.set_scaled_value(c * len);
        }
        true
    }

    /// Whether the scroll gesture in progress is this group's — the ball's
    /// or one of its bands'.
    pub fn wheel_latched(&self, ui: &UiContext) -> bool {
        !ui.scroll_gesture_new
            && ui.scroll_initiate_widget_id.is_some_and(|id| id == self.ball_id || self.sliders.iter().any(|s| s.base().id() == id))
    }

    /// How near a scroll at `(px, py)` is to something of this group's
    /// that takes one: on the ball, nothing is nearer; else the distance to
    /// the nearest band whose halo holds the pointer. `None` off both.
    pub fn wheel_zone(&self, px: f32, py: f32) -> Option<f32> {
        if self.ball_hit(px, py) {
            return Some(0.0);
        }
        self.nearest_band(px, py).map(|(_, d)| d)
    }

    pub(super) fn ball_roll(&mut self, px: f32, py: f32) -> bool {
        let (Some(mut drag), Some((_, _, r))) = (self.ball_drag, self.ball_circle()) else {
            return false;
        };
        let (dx, dy) = (px - drag.last.0, py - drag.last.1);
        if dx == 0.0 && dy == 0.0 {
            return false;
        }
        drag.dir = self.rolled_in_view(drag.dir, dx, dy, r);
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

        // The rings, under the vector: the near half of each, in short
        // strokes. The far half is behind the ball. They follow the
        // full-precision direction a drag or a scroll is turning, so they
        // move on every pixel where the rounded rows would hold them still;
        // a vector of no length shows them about the axis its first turn
        // will start from, fainter.
        let pole = match self.ball_drag {
            Some(d) => d.dir,
            None => self.fine().0,
        };
        let ring = [0.80, 0.84, 0.96, if len > 1e-6 { 0.42 } else { 0.18 }];
        let inset = r - 0.75;
        // Everything on the ball is drawn as the view sees it.
        let pole = self.to_view(pole);
        for degrees in RING_ANGLES {
            let points = Self::ring(pole, degrees, RING_SEGMENTS);
            for pair in points.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                if a[2] < 0.0 || b[2] < 0.0 {
                    continue;
                }
                ctx.vector(
                    cx + a[0] * inset,
                    cy - a[1] * inset,
                    cx + b[0] * inset,
                    cy - b[1] * inset,
                    1.0,
                    ring,
                    crate::scene::paint::Cap::Round,
                );
            }
        }
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
        let d = self.to_view(v.map(|c| c / len));
        // The tip sits on the ball's surface as the view sees it, a little in
        // from the rim so a vector lying in the screen plane stays on it.
        let reach = r - 5.0;
        let (tx, ty) = (cx + d[0] * reach, cy - d[1] * reach);
        let near = d[2] >= 0.0;
        let col = accent(if near { 1.0 } else { 0.35 });
        ctx.vector(cx, cy, tx, ty, 2.0, col, crate::scene::paint::Cap::Round);
        ctx.circle(cx, cy, 2.0, col);
        ctx.circle(tx, ty, if near { 4.5 } else { 3.0 }, col);
    }
}
