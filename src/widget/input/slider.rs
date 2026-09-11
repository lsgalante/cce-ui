//! Narrow-trait `Slider` and `RangeSlider` (Phase 5h). Detached-label widgets: the adapter
//! draws the control label in the strip above the content rect the geometry here works in.
//! Drags are host-driven through the `Input` drag hooks; the
//! readout edit mode uses `EventCtx::request_focus` and the wheel gating uses the legacy scroll
//! gesture state through `EventCtx::ui`.

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton,
    NamedKey, Paint, TextEditorState,
};

/// The track/readout/thumb geometry shared by the paint and input paths, derived from the
/// content rect (the legacy code re-derived this in five places from the base rect).
struct SliderGeom {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    track_x: f32,
    track_w: f32,
}

#[derive(Debug, Clone)]
pub struct Slider {
    dragging: bool,
    pub(crate) value: f32,
    drag_offset: f32,
    pub(crate) scroll_enabled: bool,
    show_readout: bool,
    pub(crate) editing: bool,
    edit_buffer: String,
    min: f32,
    max: f32,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    label: Option<String>,
    /// Wheel-scroll glide velocity (normalized value units/sec) and the last
    /// wheel-event instant — the Ramp hover-scroll idiom: when the event
    /// stream stops (fingers lifted), `tick` keeps the value coasting with
    /// exponential decay instead of stopping dead.
    scroll_vel: f32,
    last_wheel: Option<std::time::Instant>,
    /// Keyboard focus (FocusIn / FocusOut): the band lights in the highlight;
    /// the arrows adjust, Home / End go to the ends, Enter opens the readout.
    focused: bool,
    /// Readout / edit-buffer display precision (decimal places).
    decimals: usize,
}

impl Slider {
    pub fn new() -> Adapted<Slider> {
        Adapted::new(Slider {
            dragging: false,
            value: 0.5,
            drag_offset: 0.0,
            scroll_enabled: true,
            show_readout: false,
            editing: false,
            edit_buffer: String::new(),
            min: 0.0,
            max: 1.0,
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            label: None,
            scroll_vel: 0.0,
            last_wheel: None,
            focused: false,
            decimals: 2,
        })
    }

    pub fn set_range(&mut self, min: f32, max: f32) {
        self.min = min;
        self.max = max;
    }

    pub fn set_scroll(&mut self, enabled: bool) {
        self.scroll_enabled = enabled;
    }

    pub fn set_value(&mut self, val: f32) {
        self.value = val.clamp(0.0, 1.0);
    }

    pub fn set_readout(&mut self, enabled: bool) {
        self.show_readout = enabled;
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn get_scaled_value(&self) -> f32 {
        self.min + self.value * (self.max - self.min)
    }

    pub fn range(&self) -> (f32, f32) {
        (self.min, self.max)
    }

    pub fn set_scaled_value(&mut self, val: f32) {
        let range = self.max - self.min;
        if range != 0.0 {
            self.value = ((val - self.min) / range).clamp(0.0, 1.0);
        } else {
            self.value = 0.0;
        }
    }

    /// The width the value maps over: the whole track — the band has no thumb
    /// to keep inside the ends.
    fn value_span(&self, g: &SliderGeom) -> f32 {
        g.track_w
    }


    fn geom(&self, rect: Rect) -> SliderGeom {
        let x = rect.x;
        let w = rect.width;
        let (track_x, track_w) = if self.show_readout {
            let readout_w = 60.0;
            let gap = 8.0;
            ((x), (w - readout_w - gap).max(10.0))
        } else {
            (x, w)
        };
        SliderGeom { x, y: rect.y, w, h: rect.height, track_x, track_w }
    }

    /// The band's height profile at `x` (`band_profile`, one swell at the value).
    fn band_height_at(&self, g: &SliderGeom, x: f32) -> f32 {
        let vx = g.track_x + self.value * g.track_w;
        band_profile(g.track_x, g.track_w, g.h, x, &[vx], None)
    }

    /// The wheel-capture zone. Band style: an inset halo around the DRAWN
    /// shape — the thin band and the bulge, which travels with the value — so
    /// a scroll near the visible slider adjusts it while the rest of the row
    /// stays the host pane's to scroll. Otherwise: plain rect containment.
    pub fn scroll_hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        const SCROLL_INSET: f32 = 14.0;
        let g = self.geom(rect);
        if px < g.track_x - SCROLL_INSET || px > g.track_x + g.track_w + SCROLL_INSET {
            return false;
        }
        let cy = g.y + g.h * 0.5;
        let x = px.clamp(g.track_x, g.track_x + g.track_w);
        (py - cy).abs() <= self.band_height_at(&g, x) * 0.5 + SCROLL_INSET
    }

    /// The slider: the band spanning the whole track, swelling at the value
    /// (`paint_band_shape`).
    fn paint_band(&self, g: &SliderGeom, ctx: &mut PaintCtx) {
        // A band has no rim to light: focused, the band itself is the highlight.
        let color = if self.dragging {
            colors::slider_thumb_drag()
        } else if self.focused {
            crate::color::highlight_primary_color()
        } else {
            colors::slider_thumb()
        };
        paint_band_shape(ctx, g.track_x, g.track_w, g.y + g.h * 0.5, color, &|x| self.band_height_at(g, x));
    }

    fn scaled_string(&self) -> String {
        format!("{:.*}", self.decimals, self.min + self.value * (self.max - self.min))
    }

    fn set_value_marking(&mut self, new_val: f32) -> bool {
        if (new_val - self.value).abs() > 0.0001 {
            self.value = new_val;
            self.just_changed = true;
            if self.editing {
                self.edit_buffer = self.scaled_string();
            }
            true
        } else {
            false
        }
    }

    fn commit_edit(&mut self) {
        if self.editing {
            self.editing = false;
            let old_val = self.value;
            if let Ok(new_val) = self.edit_buffer.parse::<f32>() {
                let range = self.max - self.min;
                if range != 0.0 {
                    self.value = ((new_val - self.min) / range).clamp(0.0, 1.0);
                } else {
                    self.value = 0.0;
                }
            }
            if (self.value - old_val).abs() > 0.0001 {
                self.just_changed = true;
            }
        }
    }
}

impl Adapted<Slider> {
    pub fn with_range(mut self, min: f32, max: f32) -> Self {
        self.set_range(min, max);
        self
    }

    pub fn with_scroll(mut self, enabled: bool) -> Self {
        self.scroll_enabled = enabled;
        self
    }


    pub fn with_value(mut self, val: f32) -> Self {
        self.set_value(val);
        self
    }

    pub fn with_readout(mut self, enabled: bool) -> Self {
        self.show_readout = enabled;
        self
    }

    /// Readout display precision in decimal places (default 2).
    pub fn with_decimals(mut self, decimals: usize) -> Self {
        self.decimals = decimals;
        self
    }

}

impl Layout for Slider {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::slider_height()))
    }
}

impl Paint for Slider {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::slider_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let g = self.geom(rect);
        let radius = crate::layout::slider_corner_radius();
        let rounded = radius > 0.0;
        let rc = (rounded, rounded, rounded, rounded);
        let rrect = |r: Rect, rad: f32, corners: (bool, bool, bool, bool), c: [f32; 4], ctx: &mut PaintCtx| {
            if rounded {
                ctx.rounded_rect(r, rad, corners, c);
            } else {
                ctx.quad(r, c);
            }
        };

        // Readout box (+ focus border) and its text.
        if self.show_readout {
            let readout_w = 60.0;
            let rx = g.x + g.w - readout_w;
            let bg_color = if self.editing { [0.06, 0.10, 0.18, 1.0] } else { [0.10, 0.10, 0.13, 1.0] };
            // NOTE: the legacy square path drew the focus border as 4 edge strips and the
            // rounded path as border+inset; replicate the rounded shape for both (visually
            // identical at 1px) — acceptable divergence flagged in the Phase 5h notes.
            if self.editing {
                rrect(Rect { x: rx, y: g.y, width: readout_w, height: g.h }, radius, rc, [0.20, 0.50, 0.85, 1.0], ctx);
                rrect(
                    Rect { x: rx + 1.0, y: g.y + 1.0, width: readout_w - 2.0, height: g.h - 2.0 },
                    (radius - 1.0).max(0.0),
                    rc,
                    bg_color,
                    ctx,
                );
            } else {
                rrect(Rect { x: rx, y: g.y, width: readout_w, height: g.h }, radius, rc, bg_color, ctx);
            }

            let text = if self.editing { self.edit_buffer.clone() } else { self.scaled_string() };
            ctx.text(text, rx + 8.0, crate::layout::align_text_y(g.y, g.h, 12.0, 0.0), 12.0, [0xee, 0xee, 0xf0]);
        }

        self.paint_band(&g, ctx);
    }
}

impl Input for Slider {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state, x: px, y: py, .. } => {
                let g = self.geom(ectx.rect);
                // Readout click enters edit mode and takes focus.
                if self.show_readout {
                    let readout_w = 60.0;
                    let rx = g.x + g.w - readout_w;
                    if *px >= rx && *px <= rx + readout_w && *py >= g.y && *py <= g.y + g.h {
                        if *state == ElementState::Pressed && !self.editing {
                            self.editing = true;
                            self.edit_buffer = self.scaled_string();
                            ectx.request_focus();
                        }
                        return true;
                    }
                }
                match state {
                    ElementState::Pressed => {
                        let thumb_x = g.track_x + self.value * self.value_span(&g);
                        if *px >= g.track_x && *px <= g.track_x + g.track_w && *py >= g.y && *py <= g.y + g.h {
                            self.dragging = true;
                            self.drag_offset = px - thumb_x;
                            // A grab overrides any wheel glide in flight.
                            self.scroll_vel = 0.0;
                            self.last_wheel = None;
                            return true;
                        }
                        false
                    }
                    ElementState::Released => std::mem::take(&mut self.dragging),
                }
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                if !self.scroll_enabled {
                    return false;
                }
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    // Band style: recognition is purely SPATIAL — anywhere in
                    // the shape halo adjusts, mid-gesture included. Trackpad
                    // swipes are one long gesture (kinetic tail included), so
                    // the initiator gate below would reject every event whose
                    // gesture began outside the halo no matter where the
                    // pointer is now — the "slider won't take my scroll" feel.
                    // The default style keeps the gate: only the widget that
                    // initiated a gesture keeps it.
                    let r = ectx.rect;
                    // Band: spatial acquisition + gesture LATCH. The halo travels
                    // with the bulge, so adjusting slides it away from the pointer
                    // — without the latch the value moves a little and stalls
                    // mid-scroll. Once a gesture engages this slider it keeps it
                    // until the gesture ends; a new gesture re-acquires by halo.
                    let latched = !ui.scroll_gesture_new && ui.scroll_initiate_widget_id == Some(ectx.id);
                    if latched || self.scroll_hit(r, *px, *py) {
                        ui.scroll_initiate_widget_id = Some(ectx.id);
                        let scroll_amount = delta.notches_y();
                        let new_val = (self.value - scroll_amount * 0.02).clamp(0.0, 1.0);
                        let applied = new_val - self.value;
                        self.set_value_marking(new_val);
                        // Velocity estimate for the release glide (the Ramp
                        // hover-scroll idiom): EMA of applied delta over
                        // inter-event time. A leisurely wheel produces
                        // negligible velocity (big gaps clamp to 0.1s); fast
                        // trackpad streams build real speed. Hitting an end
                        // stops dead — no glide pinned at the bounds.
                        let now = std::time::Instant::now();
                        let idt = self
                            .last_wheel
                            .map_or(0.1, |l| now.duration_since(l).as_secs_f32())
                            .clamp(0.008, 0.1);
                        self.last_wheel = Some(now);
                        self.scroll_vel = if new_val == 0.0 || new_val == 1.0 {
                            0.0
                        } else {
                            self.scroll_vel * 0.65 + (applied / idt) * 0.35
                        };
                        if crate::scroll_debug() {
                            eprintln!(
                                "[scroll] slider {:?}: APPLY notches={scroll_amount:.3} applied={applied:.4} value={new_val:.4} idt={idt:.3} vel={:.3}",
                                self.label, self.scroll_vel
                            );
                        }
                        return true;
                    }
                    if crate::scroll_debug() {
                        eprintln!(
                            "[scroll] slider {:?}: MISS scroll_hit at ({px:.0},{py:.0}) rect={:?}",
                            self.label, ectx.rect
                        );
                    }
                }
                false
            }
            Event::FocusIn => {
                self.focused = true;
                true
            }
            Event::KeyInput(key_event) if !self.editing => {
                // A focused band: the arrows step the value by a wheel notch,
                // Home / End go to the ends, Enter opens the readout for typing.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                let target = match key_event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) | Key::Named(NamedKey::ArrowDown) => self.value - 0.02,
                    Key::Named(NamedKey::ArrowRight) | Key::Named(NamedKey::ArrowUp) => self.value + 0.02,
                    Key::Named(NamedKey::Home) => 0.0,
                    Key::Named(NamedKey::End) => 1.0,
                    Key::Named(NamedKey::Enter) if self.show_readout => {
                        self.editing = true;
                        self.edit_buffer = self.scaled_string();
                        return true;
                    }
                    _ => return false,
                };
                self.scroll_vel = 0.0;
                self.last_wheel = None;
                self.set_value_marking(target.clamp(0.0, 1.0));
                true
            }
            Event::KeyInput(key_event) => {
                if !self.editing || key_event.state != ElementState::Pressed {
                    return false;
                }
                let mut state = TextEditorState {
                    buffer: self.edit_buffer.clone(),
                    cursor_idx: self.edit_buffer.chars().count(),
                    select_anchor: None,
                    all_selected: false,
                };
                let mut handled = false;
                match &key_event.logical_key {
                    Key::Named(NamedKey::Backspace) => {
                        state.delete_backwards();
                        handled = true;
                    }
                    Key::Named(NamedKey::Enter) => {
                        self.commit_edit();
                        handled = true;
                    }
                    Key::Named(NamedKey::Escape) => {
                        self.editing = false;
                        handled = true;
                    }
                    Key::Character(s) => {
                        for ch in s.chars() {
                            if ch.is_ascii_digit() || ch == '.' || (ch == '-' && state.buffer.is_empty()) {
                                state.insert_text(&ch.to_string());
                            }
                        }
                        handled = true;
                    }
                    _ => {}
                }
                if self.editing {
                    self.edit_buffer = state.buffer;
                }
                handled
            }
            // Focus loss commits the readout edit (legacy `unfocus` override).
            Event::FocusOut => {
                self.focused = false;
                self.commit_edit();
                true
            }
            _ => false,
        }
    }

    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    /// Wheel-glide inertia: once the event stream stops (>60ms), the value
    /// coasts on the estimated velocity with exponential decay — the same
    /// release feel as the pane scrolls and the Ramp's hover-scroll.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        let Some(last) = self.last_wheel else { return false };
        if last.elapsed().as_secs_f32() <= 0.06 {
            return false;
        }
        if self.scroll_vel.abs() > 0.02 && !self.dragging && !self.editing {
            let new_val = (self.value + self.scroll_vel * dt).clamp(0.0, 1.0);
            let moved = self.set_value_marking(new_val);
            if crate::scroll_debug() {
                eprintln!(
                    "[scroll] slider {:?}: GLIDE dt={dt:.3} vel={:.3} value={new_val:.4}",
                    self.label, self.scroll_vel
                );
            }
            if new_val == 0.0 || new_val == 1.0 {
                self.scroll_vel = 0.0;
                self.last_wheel = None;
            } else {
                self.scroll_vel *= (-5.0 * dt).exp();
            }
            moved
        } else {
            self.scroll_vel = 0.0;
            self.last_wheel = None;
            false
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.dragging
    }
    fn drag_begin(&mut self, px: f32, _py: f32, rect: Rect) {
        self.dragging = true;
        // A grab overrides any wheel glide in flight.
        self.scroll_vel = 0.0;
        self.last_wheel = None;
        let g = self.geom(rect);
        let thumb_x = g.track_x + self.value * self.value_span(&g);
        self.drag_offset = px - thumb_x;
    }
    fn drag_update(&mut self, px: f32, _py: f32, rect: Rect) -> bool {
        let g = self.geom(rect);
        let range = self.value_span(&g);
        if range > 0.0 {
            let new_val = ((px - self.drag_offset - g.track_x) / range).clamp(0.0, 1.0);
            return self.set_value_marking(new_val);
        }
        false
    }
    fn drag_end(&mut self) {
        self.dragging = false;
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.scaled_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        if let Ok(new_val) = val.trim().parse::<f32>() {
            let range = self.max - self.min;
            let mapped = if range != 0.0 { ((new_val - self.min) / range).clamp(0.0, 1.0) } else { 0.0 };
            return self.set_value_marking(mapped);
        }
        false
    }

    fn value(&self) -> i32 {
        (self.value * 100.0) as i32
    }
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveThumb {
    Low,
    High,
}

#[derive(Debug, Clone)]
pub struct RangeSlider {
    value_low: f32,
    value_high: f32,
    pub(crate) active_thumb: Option<ActiveThumb>,
    drag_offset: f32,
    label: Option<String>,
    /// Keyboard focus (FocusIn / FocusOut): the band lights; `focus_end` is
    /// the end the arrows move (Up / Down switch it), starting at the low end.
    focused: bool,
    focus_end: ActiveThumb,
}

impl RangeSlider {
    pub fn new() -> Adapted<RangeSlider> {
        Adapted::new(RangeSlider {
            value_low: 0.2,
            value_high: 0.8,
            active_thumb: None,
            drag_offset: 0.0,
            label: None,
            focused: false,
            focus_end: ActiveThumb::Low,
        })
    }

    pub fn set_values(&mut self, low: f32, high: f32) {
        self.value_low = low.clamp(0.0, 1.0);
        self.value_high = high.clamp(self.value_low, 1.0);
    }

    pub fn values(&self) -> (f32, f32) {
        (self.value_low, self.value_high)
    }
}

impl Adapted<RangeSlider> {
    pub fn with_values(mut self, low: f32, high: f32) -> Self {
        self.set_values(low, high);
        self
    }
}

/// The band's height at `x`: the flat band thickness (`style.control.slider.
/// band_thickness`), rising through a raised-cosine bell to the bulge height
/// around each of `centers` (`bulge_width` half-span, `bulge_height` peak), the
/// bell raised to a power so the flanks taper long and the crest stays plump —
/// mid-digestion, not a triangle; and, for a RangeSlider, one band thickness
/// more across `range` (between its two swells). Capsule tips: the profile
/// shrinks over a circular cap inside each track end — the band ends round, not
/// square-cut, and the well contour and wheel halo (both measured from here)
/// round with it. Public with `paint_band_shape` so app-owned scrubbers (the
/// designer's playbar) draw the same band.
pub fn band_profile(track_x: f32, track_w: f32, h: f32, x: f32, centers: &[f32], range: Option<(f32, f32)>) -> f32 {
    let band_t = crate::layout::slider_band_thickness().max(0.5);
    let bulge_h = crate::layout::slider_bulge_height().clamp(band_t, h);
    let bulge_w = crate::layout::slider_bulge_width().max(2.0);
    let mut bell = 0.0f32;
    for &vx in centers {
        let t = ((x - vx) / bulge_w).clamp(-1.0, 1.0);
        bell = bell.max(0.5 * (1.0 + (std::f32::consts::PI * t).cos()));
    }
    let base = if range.is_some_and(|(lo, hi)| x >= lo && x <= hi) { 2.0 * band_t } else { band_t };
    let h = base + (bulge_h - base) * bell.powf(1.35);
    let d = (x - track_x).min(track_x + track_w - x);
    let r = (h * 0.5).max(0.5);
    if d < r {
        let t = ((r - d.max(0.0)) / r).min(1.0);
        return h * (1.0 - t * t).max(0.0).sqrt();
    }
    h
}

/// The band, drawn from its height `profile` (`band_profile`): the well first —
/// the band appears INSET, a carve whose contour follows the drawn shape a small
/// gap outside it. The rect recess prims can't follow a bell, so the walls are
/// hand-shaded per column from the same profile the fill samples: a shadow band
/// hugging the top contour, a lit band along the bottom (the DE light sits
/// upper-left), stepped alphas like the legacy banded bevels, amplitude riding
/// `bevel_depth` like every other relief wall's. Then the band itself, one
/// column per pixel with a hair of overlap so AA seams can't open. The one
/// painter behind Slider, RangeSlider and Float3's rows.
pub fn paint_band_shape(ctx: &mut PaintCtx, track_x: f32, track_w: f32, cy: f32, color: [f32; 4], profile: &dyn Fn(f32) -> f32) {
    paint_band_shape_colored(ctx, track_x, track_w, cy, &|_| color, profile);
}

/// [`paint_band_shape`] with the band's colour sampled per column
/// (`color_at(x)`): a RangeSlider lights only the swell the keyboard is on.
pub fn paint_band_shape_colored(ctx: &mut PaintCtx, track_x: f32, track_w: f32, cy: f32, color_at: &dyn Fn(f32) -> [f32; 4], profile: &dyn Fn(f32) -> f32) {
    const WELL_GAP: f32 = 4.0;
    const WELL_WALL: f32 = 3.0;
    const WALL_STEPS: usize = 3;
    let strength = (crate::layout::bevel_depth() / 0.15).clamp(0.0, 2.0);
    let a_dark = 0.32 * strength;
    let a_light = 0.16 * strength;
    let wx0 = track_x - WELL_GAP;
    let wx1 = track_x + track_w + WELL_GAP;
    // 1px columns, EXACT widths: translucent shading quads must not overlap (a
    // seam double-blends into a visible tick) — unlike the opaque band columns
    // below, which overlap on purpose against AA gaps.
    let cols = (wx1 - wx0).ceil().max(1.0) as i32;
    let colw = (wx1 - wx0) / cols as f32;
    let sub = WELL_WALL / WALL_STEPS as f32;
    for i in 0..cols {
        let x = wx0 + i as f32 * colw;
        let xm = x + colw * 0.5;
        // Inside the track the contour rides the profile; past the tips it wraps
        // around them on a WELL_GAP circle — rounded well ends, not square-cut.
        let c = if xm < track_x {
            let e = track_x - xm;
            (WELL_GAP * WELL_GAP - e * e).max(0.0).sqrt()
        } else if xm > track_x + track_w {
            let e = xm - (track_x + track_w);
            (WELL_GAP * WELL_GAP - e * e).max(0.0).sqrt()
        } else {
            profile(xm) * 0.5 + WELL_GAP
        };
        for k in 0..WALL_STEPS {
            let fade = 1.0 - k as f32 / WALL_STEPS as f32;
            // Shadow INSIDE the well below the top contour; the lit lip OUTSIDE
            // below the bottom contour — the textbox-recess read.
            ctx.quad(Rect { x, y: cy - c + k as f32 * sub, width: colw, height: sub }, [0.0, 0.0, 0.0, a_dark * fade]);
            ctx.quad(Rect { x, y: cy + c + k as f32 * sub, width: colw, height: sub }, [1.0, 1.0, 1.0, a_light * fade]);
        }
    }
    let steps = (track_w.ceil() as i32).max(1);
    let step_w = track_w / steps as f32;
    for i in 0..steps {
        let x = track_x + i as f32 * step_w;
        let h = profile(x + step_w * 0.5);
        ctx.quad(Rect { x, y: cy - h * 0.5, width: step_w + 0.3, height: h }, color_at(x + step_w * 0.5));
    }
}

/// The detached-label strip height above a content rect (zero unlabeled) — the
/// adapter's `Widget::label_offset` over a model's synced label, for models whose
/// cached rect is the whole block.
pub(crate) fn detached_strip(label: &Option<String>) -> f32 {
    if label.is_some() { crate::layout::control_label_strip() } else { 0.0 }
}

impl Layout for RangeSlider {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::rangeslider_height()))
    }
}

impl Paint for RangeSlider {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    /// The band with two swells — one at each end of the range — and the band
    /// itself a thickness heavier between them, so the range reads as the
    /// swallowed length. The swells sit where the thumbs' centres were, so the
    /// drag geometry below is unchanged.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let thumb_size = h * 0.9;
        let range = w - thumb_size;
        let lo = x + self.value_low * range + thumb_size / 2.0;
        let hi = x + self.value_high * range + thumb_size / 2.0;
        // A band has no rim to light: focused, the swell the keyboard is on
        // is the highlight — the colour rides the swell's own bell, so it
        // blooms over that end and fades back to the band along its flanks.
        let base = if self.active_thumb.is_some() { colors::rangeslider_thumb_drag() } else { colors::rangeslider_thumb() };
        let focus_center = self.focused.then(|| if matches!(self.focus_end, ActiveThumb::Low) { lo } else { hi });
        let hl = crate::color::highlight_primary_color();
        let bulge_w = crate::layout::slider_bulge_width().max(2.0);
        let color_at = |px: f32| -> [f32; 4] {
            let Some(c) = focus_center else { return base };
            let t = ((px - c) / bulge_w).clamp(-1.0, 1.0);
            let bell = 0.5 * (1.0 + (std::f32::consts::PI * t).cos());
            let mut out = base;
            for k in 0..4 {
                out[k] = base[k] + (hl[k] - base[k]) * bell;
            }
            out
        };
        paint_band_shape_colored(ctx, x, w, y + h * 0.5, &color_at, &|px| band_profile(x, w, h, px, &[lo, hi], Some((lo, hi))));
    }
}

impl Input for RangeSlider {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::FocusIn => {
                self.focused = true;
                self.focus_end = ActiveThumb::Low;
                true
            }
            Event::FocusOut => {
                self.focused = false;
                true
            }
            Event::KeyInput(key_event) => {
                // One stop, two ends: Left / Right step the focused end by a
                // wheel notch inside the other end's bound, Up / Down switch
                // ends, Home / End send the focused end to its limit.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                let low = matches!(self.focus_end, ActiveThumb::Low);
                let (cur, min, max) = if low {
                    (self.value_low, 0.0, self.value_high)
                } else {
                    (self.value_high, self.value_low, 1.0)
                };
                let target = match key_event.logical_key {
                    Key::Named(NamedKey::ArrowLeft) => cur - 0.02,
                    Key::Named(NamedKey::ArrowRight) => cur + 0.02,
                    Key::Named(NamedKey::Home) => min,
                    Key::Named(NamedKey::End) => max,
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowDown) => {
                        self.focus_end = if low { ActiveThumb::High } else { ActiveThumb::Low };
                        return true;
                    }
                    _ => return false,
                };
                let target = target.clamp(min, max);
                if low {
                    self.value_low = target;
                } else {
                    self.value_high = target;
                }
                true
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if !ui.scroll_gesture_new && ui.scroll_initiate_widget_id != Some(ectx.id) {
                    return false;
                }
                let r = ectx.rect;
                let (x, y, w, h) = (r.x, r.y, r.width, r.height);
                if *px >= r.x && *px <= r.x + r.width && *py >= y && *py <= y + h {
                    if ui.scroll_gesture_new {
                        ui.scroll_initiate_widget_id = Some(ectx.id);
                    }
                    let thumb_size = h * 0.9;
                    let range = w - thumb_size;
                    let center_low = x + self.value_low * range + thumb_size / 2.0;
                    let center_high = x + self.value_high * range + thumb_size / 2.0;
                    let dist_low = (px - center_low).abs();
                    let dist_high = (px - center_high).abs();
                    let scroll_amount = delta.notches_y();
                    let step = 0.02;
                    let adjust_low = if dist_low < dist_high {
                        true
                    } else if dist_high < dist_low {
                        false
                    } else {
                        scroll_amount > 0.0
                    };
                    if adjust_low {
                        let new_val = (self.value_low - scroll_amount * step).clamp(0.0, self.value_high);
                        if (new_val - self.value_low).abs() > 0.0001 {
                            self.value_low = new_val;
                        }
                    } else {
                        let new_val = (self.value_high - scroll_amount * step).clamp(self.value_low, 1.0);
                        if (new_val - self.value_high).abs() > 0.0001 {
                            self.value_high = new_val;
                        }
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    fn draggable(&self, _rect: Rect) -> bool {
        true
    }
    fn is_dragging(&self) -> bool {
        self.active_thumb.is_some()
    }
    fn drag_begin(&mut self, px: f32, _py: f32, rect: Rect) {
        let (x, w) = (rect.x, rect.width);
        let thumb_size = rect.height * 0.9;
        let range = w - thumb_size;
        let thumb_low_x = x + self.value_low * range;
        let thumb_high_x = x + self.value_high * range;
        let center_low = thumb_low_x + thumb_size / 2.0;
        let center_high = thumb_high_x + thumb_size / 2.0;

        let active = if (self.value_low - self.value_high).abs() < 0.001 {
            if px < center_low { ActiveThumb::Low } else { ActiveThumb::High }
        } else if (px - center_low).abs() < (px - center_high).abs() {
            ActiveThumb::Low
        } else {
            ActiveThumb::High
        };
        self.active_thumb = Some(active);
        let active_x = match active {
            ActiveThumb::Low => thumb_low_x,
            ActiveThumb::High => thumb_high_x,
        };
        self.drag_offset = px - active_x;
    }
    fn drag_update(&mut self, px: f32, _py: f32, rect: Rect) -> bool {
        let Some(active) = self.active_thumb else { return false };
        let (x, w) = (rect.x, rect.width);
        let thumb_size = rect.height * 0.9;
        let range = w - thumb_size;
        if range <= 0.0 {
            return false;
        }
        let new_val = ((px - self.drag_offset - x) / range).clamp(0.0, 1.0);
        match active {
            ActiveThumb::Low => {
                let constrained = new_val.min(self.value_high);
                if (constrained - self.value_low).abs() > 0.001 {
                    self.value_low = constrained;
                    return true;
                }
            }
            ActiveThumb::High => {
                let constrained = new_val.max(self.value_low);
                if (constrained - self.value_high).abs() > 0.001 {
                    self.value_high = constrained;
                    return true;
                }
            }
        }
        false
    }
    fn drag_end(&mut self) {
        self.active_thumb = None;
    }

    fn value(&self) -> i32 {
        ((self.value_low * 100.0) as i32) | (((self.value_high * 100.0) as i32) << 16)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{MouseScrollDelta, WidgetHost, UiContext};

    /// The legacy rangeslider interaction test, driven through the WidgetHost drag forwards
    /// (hosts call these directly): thumb selection by proximity, constrained updates.
    #[test]
    fn rangeslider_interaction() {
        let mut rs = RangeSlider::new();
        WidgetHost::set_rect(&mut rs, 10.0, 10.0, 200.0, 20.0);
        assert_eq!(rs.values(), (0.2, 0.8));

        // Thumb size 18, range 182; low center = 55.4.
        rs.drag_begin(55.4, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::Low));
        assert!(rs.drag_update(100.9, 20.0));
        assert!((rs.values().0 - 0.45).abs() < 0.01);
        assert_eq!(rs.values().1, 0.8);
        rs.drag_end();
        assert_eq!(rs.active_thumb, None);

        // High thumb 0.8 -> 0.6.
        rs.drag_begin(164.6, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::High));
        assert!(rs.drag_update(128.2, 20.0));
        assert!((rs.values().1 - 0.6).abs() < 0.01);
        rs.drag_end();
    }

    #[test]
    fn rangeslider_overlap_and_constraint() {
        let mut rs = RangeSlider::new().with_values(0.5, 0.5);
        WidgetHost::set_rect(&mut rs, 10.0, 10.0, 200.0, 20.0);

        rs.drag_begin(109.0, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::Low));
        rs.drag_end();

        rs.drag_begin(111.0, 20.0);
        assert_eq!(rs.active_thumb, Some(ActiveThumb::High));
        rs.drag_end();

        rs.drag_begin(110.0, 20.0);
        rs.drag_update(150.0, 20.0);
        assert_eq!(rs.values().0, 0.5, "low constrained to high");
        rs.drag_end();
    }

#[test]
fn probe_slider_bridge() {
    
    
    let ctx = UiContext::new();
    let mut sl = Slider::new().with_label("Slider");
    WidgetHost::set_rect(&mut sl, 20.0, 220.0, 200.0, 40.0);
    eprintln!("rect         = {:?}", WidgetHost::rect(&sl));
    eprintln!("extra_quads  = {:?}", WidgetHost::extra_quads(&sl));
    eprintln!("rounded      = {:?}", WidgetHost::all_rounded_quads(&sl, &ctx));
    eprintln!("labels       = {:?}", sl.own_text_labels().iter().map(|l| l.text.clone()).collect::<Vec<_>>());
}

    /// Slider press-on-track begins a drag through the routed path; wheel adjusts the value
    /// with the scroll-gesture gating intact.
    #[test]
    fn slider_press_drag_and_wheel() {
        let mut ctx = UiContext::new();
        let mut sl = Slider::new().with_value(0.5);
        let (id, ptr) = (sl.id(), sl.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut sl, 0.0, 0.0, 100.0, 20.0);

        // Press on the track grabs the thumb.
        assert!(ctx.propagate_event(
            &Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: 50.0, y: 10.0, local_x: 50.0, local_y: 10.0 },
            id,
        ));
        assert!(sl.is_dragging());
        assert!(sl.drag_update(80.0, 10.0));
        assert!(sl.inner().value() > 0.5);
        sl.drag_end();

        // Wheel adjusts value when the gesture starts fresh.
        ctx.scroll_gesture_new = true;
        let before = sl.inner().value();
        assert!(sl.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0),
            50.0,
            10.0,
            &mut ctx,
        ));
        assert!(sl.inner().value() < before, "scroll up decreases value");
        assert!(sl.take_change());
    }
}

#[cfg(test)]
mod focus_tests {
    use super::*;
    use crate::widget::{Event, KeyEvent, UiContext, WidgetHost};

    fn press(key: NamedKey) -> Event {
        Event::KeyInput(KeyEvent { logical_key: Key::Named(key), state: ElementState::Pressed, text: None, repeat: false, ctrl: false, shift: false, alt: false })
    }

    /// A focused range: Right steps the low end, Down switches to the high end,
    /// Left steps it, End sends it to 1, and the low end can never pass the high.
    #[test]
    fn range_arrows_step_the_focused_end_and_up_down_switch() {
        let mut ctx = UiContext::new();
        let mut r = RangeSlider::new().with_values(0.2, 0.8);
        WidgetHost::set_rect(&mut r, 0.0, 0.0, 200.0, 16.0);
        assert!(!r.handle_event(&press(NamedKey::ArrowRight), &mut ctx), "unfocused: not this range's key");
        r.handle_event(&Event::FocusIn, &mut ctx);
        assert!(r.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
        let (lo, hi) = r.inner().values();
        assert!((lo - 0.22).abs() < 1e-5 && (hi - 0.8).abs() < 1e-5, "the low end moved");
        assert!(r.handle_event(&press(NamedKey::ArrowDown), &mut ctx));
        assert!(r.handle_event(&press(NamedKey::ArrowLeft), &mut ctx));
        let (lo, hi) = r.inner().values();
        assert!((lo - 0.22).abs() < 1e-5 && (hi - 0.78).abs() < 1e-5, "then the high end");
        assert!(r.handle_event(&press(NamedKey::End), &mut ctx));
        assert_eq!(r.inner().values().1, 1.0);
        assert!(r.handle_event(&press(NamedKey::ArrowUp), &mut ctx));
        assert!(r.handle_event(&press(NamedKey::End), &mut ctx));
        assert_eq!(r.inner().values(), (1.0, 1.0), "the low end stops at the high end");
    }

    /// A focused band steps by a wheel notch on the arrows, jumps on Home / End,
    /// and opens its readout on Enter; unfocused it ignores the keys.
    #[test]
    fn arrows_step_the_band_and_enter_opens_the_readout() {
        let mut ctx = UiContext::new();
        let mut s = Slider::new().with_readout(true);
        WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 16.0);
        let v0 = s.inner().value;
        assert!(!s.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
        assert_eq!(s.inner().value, v0, "unfocused: untouched");
        s.handle_event(&Event::FocusIn, &mut ctx);
        assert!(s.handle_event(&press(NamedKey::ArrowRight), &mut ctx));
        assert!((s.inner().value - (v0 + 0.02)).abs() < 1e-5);
        assert!(s.handle_event(&press(NamedKey::End), &mut ctx));
        assert_eq!(s.inner().value, 1.0);
        assert!(s.handle_event(&press(NamedKey::Home), &mut ctx));
        assert_eq!(s.inner().value, 0.0);
        assert!(s.handle_event(&press(NamedKey::Enter), &mut ctx));
        assert!(s.inner().editing, "Enter opens the readout for typing");
    }
}
