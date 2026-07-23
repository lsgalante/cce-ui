//! Narrow-trait `Slider` and `RangeSlider` (Phase 5h). Detached-label widgets that do NOT
//! inflate their rect (`inflates_label_rect = false`): the label eats into the assigned rect,
//! so the adapter's content rect is exactly the legacy `y + label_offset` / `h - label_offset`
//! band the old geometry used. The side-label inset (`label_x_offset`) is computed by the model
//! from its synced label + config. Drags are host-driven through the `Input` drag hooks; the
//! readout edit mode uses `EventCtx::request_focus` and the wheel gating uses the legacy scroll
//! gesture state through `EventCtx::ui`.

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton,
    MouseScrollDelta, NamedKey, Paint, TextEditorState,
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
    thumb_size: f32,
}

fn side_offset(label: &Option<String>) -> f32 {
    if crate::layout::control_label_layout() == "side" && label.is_some() {
        90.0
    } else {
        0.0
    }
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
    /// Recessed-track style: the track is a well carved into the plate below
    /// (the TextBox `with_recessed` idiom) — the carve's shading defines the
    /// channel, and with a transparent track color the plate itself is its
    /// floor.
    recessed: bool,
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
            recessed: crate::layout::control_relief(),
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

    /// The recessed track's carve for hosts that draw this control through the
    /// legacy flat views (see `ParametersBg::reliefs`): (x, y, w, h, radius,
    /// depth) over the widget's assigned `rect`, or None when the style is off.
    /// The same geometry `paint` carves.
    pub fn track_relief(&self, rect: Rect) -> Option<(f32, f32, f32, f32, f32, f32)> {
        if !self.recessed {
            return None;
        }
        let g = self.geom(rect);
        let radius = crate::layout::slider_corner_radius();
        let depth = crate::layout::bevel_width().min(g.h * 0.2);
        Some((g.track_x, g.y, g.track_w, g.h, radius, depth))
    }

    /// The thumb knob's circle (cx, cy, radius, color) for hosts that draw this
    /// control through the legacy flat views (see `ParametersBg::spheres`): the
    /// `Prim::Sphere` the rounded-corner paint emits — no flat view can carry
    /// it. None under the square style, whose quad thumb already reaches the
    /// plain-quad view. The same geometry `paint` draws.
    pub fn thumb_sphere(&self, rect: Rect) -> Option<(f32, f32, f32, [f32; 4])> {
        if crate::layout::slider_corner_radius() <= 0.0 {
            return None;
        }
        let g = self.geom(rect);
        let recess_t = crate::layout::bevel_width().min(g.h * 0.2);
        let thumb_x = g.track_x + self.value * (g.track_w - g.thumb_size);
        let thumb_y = g.y + (g.h - g.thumb_size) / 2.0;
        let diameter = if self.recessed { g.h - recess_t } else { g.thumb_size };
        let color = if self.dragging { colors::slider_thumb_drag() } else { colors::slider_thumb() };
        Some((thumb_x + g.thumb_size / 2.0, thumb_y + g.thumb_size / 2.0, diameter / 2.0, color))
    }

    /// The detached-label strip height above the content rect — a replica of
    /// `Widget::label_offset` over the synced label (zero in side layout or
    /// unlabeled).
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

    fn geom(&self, rect: Rect) -> SliderGeom {
        let side = side_offset(&self.label);
        let x = rect.x + side;
        let w = rect.width - side;
        let (track_x, track_w) = if self.show_readout {
            let readout_w = 60.0;
            let gap = 8.0;
            ((x), (w - readout_w - gap).max(10.0))
        } else {
            (x, w)
        };
        SliderGeom { x, y: rect.y, w, h: rect.height, track_x, track_w, thumb_size: rect.height * 0.9 }
    }

    fn scaled_string(&self) -> String {
        format!("{:.2}", self.min + self.value * (self.max - self.min))
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

    /// Recessed-track style: see the `recessed` field.
    pub fn with_recessed(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
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
}

impl Layout for Slider {
    fn inflates_label_rect(&self) -> bool {
        false // legacy Slider::set_rect stored the assigned rect verbatim
    }

    /// Detached label x inset — keeps the label clear of its carve-out tab's
    /// left wall (the Dropdown value).
    fn detached_label_inset(&self) -> f32 {
        4.0
    }


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

        // Track. Recessed style draws no background at all — the plate below
        // is the well's floor (the TextBox bare-recess look), and the carve
        // emitted after the fill defines the channel.
        let track_rect = Rect { x: g.track_x, y: g.y, width: g.track_w, height: g.h };
        // Carve wall width, the TextBox formula: capped against the bar height
        // (the wall straddles the track boundary, intruding half its width).
        let recess_t = crate::layout::bevel_width().min(g.h * 0.2);
        if !self.recessed {
            rrect(track_rect, radius, rc, colors::slider_track(), ctx);
        }

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

        // Fill up to the thumb center. Recessed style insets the fill onto the
        // well's flat floor (past the wall's inner half-span), so the liquid
        // sits in the well instead of climbing its walls.
        let thumb_x = g.track_x + self.value * (g.track_w - g.thumb_size);
        if let Some(fill_color) = colors::slider_fill() {
            let (fx, fy, fmax_w, fh) = if self.recessed {
                let inset = recess_t * 0.5;
                (g.track_x + inset, g.y + inset, g.track_w - 2.0 * inset, g.h - 2.0 * inset)
            } else {
                (g.track_x, g.y, g.track_w, g.h)
            };
            let fill_w = (thumb_x + g.thumb_size / 2.0 - fx).max(0.0).min(fmax_w);
            let fill_rad = if rounded { radius.min(fh / 2.0) } else { radius };
            rrect(Rect { x: fx, y: fy, width: fill_w, height: fh }, fill_rad, (true, true, true, true), fill_color, ctx);
        }

        // Carve AFTER the fill so the wall's shading modulates whatever it
        // crosses — the same order TextBox uses for its edit fill.
        if self.recessed {
            let strip = self.label_top();
            if strip > 0.0 {
                // Labeled: the label sits in a CARVE-OUT tab, the section-
                // title idiom (the labeled Dropdown's composition) — a flat
                // recessed well hugging the label run, bottom open into the
                // track's well; the well's top wall picks up right of the
                // tab's throat.
                let (fam, fsize) = crate::layout::control_label_font_detached_parsed();
                let text_w = self
                    .label
                    .as_deref()
                    .map(|l| crate::widget::display::measure_text_width(l, &fam, fsize))
                    .unwrap_or(0.0);
                let inset = 4.0; // Layout::detached_label_inset — the label's x offset
                let tab_w = (text_w + 2.0 * inset).max(2.0 * radius + 8.0).min(g.track_w);
                let tab_r = g.track_x + tab_w;
                // The labeled-Dropdown composition: pieces extend `recess_t`
                // past interior seams (host-fade crossfade), the tab's right
                // wall ends at the fillet's vertical tangent (or it ghosts
                // through the arc), and a left-only bridge carries the left
                // wall across the fillet span.
                let fr = 6.0_f32.min(strip * 0.5);
                let filleted = g.track_x + g.track_w - tab_r > fr + 4.0;
                let tab_bottom = if filleted { g.y - fr } else { g.y };
                ctx.recess_edges(
                    Rect { x: g.track_x, y: g.y - strip, width: tab_w, height: tab_bottom - (g.y - strip) + recess_t },
                    (radius, radius.min(strip * 0.5), 0.0, 0.0),
                    recess_t,
                    (true, true, false, true),
                );
                if filleted {
                    ctx.recess_edges(
                        Rect { x: g.track_x, y: g.y - fr, width: tab_w, height: fr + recess_t },
                        (0.0, 0.0, 0.0, 0.0),
                        recess_t,
                        (false, false, false, true),
                    );
                }
                ctx.recess_edges(track_rect, (0.0, 0.0, radius, radius), recess_t, (false, true, true, true));
                if filleted {
                    ctx.concave_fillet(
                        tab_r + fr,
                        g.y - fr,
                        fr,
                        recess_t,
                        std::f32::consts::FRAC_PI_2,
                        false,
                    );
                    ctx.recess_edges(
                        Rect {
                            x: tab_r + fr - recess_t,
                            y: g.y,
                            width: g.track_x + g.track_w - tab_r - fr + recess_t,
                            height: g.h,
                        },
                        (0.0, radius, 0.0, 0.0),
                        recess_t,
                        (true, false, false, false),
                    );
                } else if g.track_x + g.track_w - tab_r > 0.5 {
                    ctx.recess_edges(
                        Rect { x: tab_r - recess_t, y: g.y, width: g.track_x + g.track_w - tab_r + recess_t, height: g.h },
                        (0.0, radius, 0.0, 0.0),
                        recess_t,
                        (true, false, false, false),
                    );
                }
            } else {
                ctx.recess(track_rect, (radius, radius, radius, radius), recess_t);
            }
        }

        // Thumb. A real Circle prim, not a full-radius rounded rect: rounded
        // rects follow the DE-wide corner_shape family, and a squircle knob
        // reads wrong — the thumb should stay round under any corner style.
        let thumb_y = g.y + (g.h - g.thumb_size) / 2.0;
        let thumb_color = if self.dragging { colors::slider_thumb_drag() } else { colors::slider_thumb() };
        // Recessed style: the knob sits IN the well, so its diameter is the
        // flat floor between the walls (each intrudes half its width) —
        // drawn size only; the drag/hit geometry keeps the full thumb_size.
        if let Some((cx, cy, r, c)) = self.thumb_sphere(rect) {
            ctx.sphere(cx, cy, r, c);
        } else {
            rrect(
                Rect { x: thumb_x, y: thumb_y, width: g.thumb_size, height: g.thumb_size },
                0.0,
                (true, true, true, true),
                thumb_color,
                ctx,
            );
        }
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
                        let thumb_x = g.track_x + self.value * (g.track_w - g.thumb_size);
                        if *px >= g.track_x && *px <= g.track_x + g.track_w && *py >= g.y && *py <= g.y + g.h {
                            self.dragging = true;
                            self.drag_offset = px - thumb_x;
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
                // Scroll-gesture gating: only the widget that initiated the gesture keeps it.
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if !ui.scroll_gesture_new && ui.scroll_initiate_widget_id != Some(ectx.id) {
                        return false;
                    }
                    let r = ectx.rect;
                    if *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height {
                        if ui.scroll_gesture_new {
                            ui.scroll_initiate_widget_id = Some(ectx.id);
                        }
                        let scroll_amount = match delta {
                            MouseScrollDelta::LineDelta(_x, y) => *y,
                            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                        };
                        let new_val = (self.value - scroll_amount * 0.02).clamp(0.0, 1.0);
                        self.set_value_marking(new_val);
                        return true;
                    }
                }
                false
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
                self.commit_edit();
                false
            }
            _ => false,
        }
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
    fn drag_begin(&mut self, px: f32, _py: f32, rect: Rect) {
        self.dragging = true;
        let g = self.geom(rect);
        let thumb_x = g.track_x + self.value * (g.track_w - g.thumb_size);
        self.drag_offset = px - thumb_x;
    }
    fn drag_update(&mut self, px: f32, _py: f32, rect: Rect) -> bool {
        let g = self.geom(rect);
        let range = g.track_w - g.thumb_size;
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
}

impl RangeSlider {
    pub fn new() -> Adapted<RangeSlider> {
        Adapted::new(RangeSlider {
            value_low: 0.2,
            value_high: 0.8,
            active_thumb: None,
            drag_offset: 0.0,
            label: None,
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

impl Layout for RangeSlider {
    fn inflates_label_rect(&self) -> bool {
        false
    }

    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::rangeslider_height()))
    }
}

impl Paint for RangeSlider {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::rangeslider_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let side = side_offset(&self.label);
        let (x, y, w, h) = (rect.x + side, rect.y, rect.width - side, rect.height);
        let radius = crate::layout::rangeslider_corner_radius();
        let rounded = radius > 0.0;
        let rc = (rounded, rounded, rounded, rounded);
        let thumb_size = h * 0.9;
        let range = w - thumb_size;
        let thumb_low_x = x + self.value_low * range;
        let thumb_high_x = x + self.value_high * range;
        let thumb_y = y + (h - thumb_size) / 2.0;
        let highlight = Rect {
            x: thumb_low_x + thumb_size / 2.0,
            y: y + h * 0.35,
            width: thumb_high_x - thumb_low_x,
            height: h * 0.3,
        };
        let low_color = if self.active_thumb == Some(ActiveThumb::Low) {
            colors::rangeslider_thumb_drag()
        } else {
            colors::rangeslider_thumb()
        };
        let high_color = if self.active_thumb == Some(ActiveThumb::High) {
            colors::rangeslider_thumb_drag()
        } else {
            colors::rangeslider_thumb()
        };

        let rrect = |r: Rect, rad: f32, corners: (bool, bool, bool, bool), c: [f32; 4], ctx: &mut PaintCtx| {
            if rounded {
                ctx.rounded_rect(r, rad, corners, c);
            } else {
                ctx.quad(r, c);
            }
        };
        rrect(Rect { x, y, width: w, height: h }, radius, rc, colors::rangeslider_track(), ctx);
        rrect(highlight, radius.min(highlight.height / 2.0), (true, true, true, true), colors::rangeslider_fill(), ctx);
        rrect(
            Rect { x: thumb_low_x, y: thumb_y, width: thumb_size, height: thumb_size },
            if rounded { thumb_size / 2.0 } else { 0.0 },
            (true, true, true, true),
            low_color,
            ctx,
        );
        rrect(
            Rect { x: thumb_high_x, y: thumb_y, width: thumb_size, height: thumb_size },
            if rounded { thumb_size / 2.0 } else { 0.0 },
            (true, true, true, true),
            high_color,
            ctx,
        );
    }
}

impl Input for RangeSlider {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if !ui.scroll_gesture_new && ui.scroll_initiate_widget_id != Some(ectx.id) {
                    return false;
                }
                let side = side_offset(&self.label);
                let r = ectx.rect;
                let (x, y, w, h) = (r.x + side, r.y, r.width - side, r.height);
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
                    let scroll_amount = match delta {
                        MouseScrollDelta::LineDelta(_x, y) => *y,
                        MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                    };
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
        let side = side_offset(&self.label);
        let (x, w) = (rect.x + side, rect.width - side);
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
        let side = side_offset(&self.label);
        let (x, w) = (rect.x + side, rect.width - side);
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
    use crate::widget::{WidgetHost, UiContext};

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
