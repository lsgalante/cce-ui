//! `RangeSlider`: two thumbs on one band, the span between them raised; its layout, paint and
//! input.

use super::*;

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
        let focus_center = self.focused.then_some(if matches!(self.focus_end, ActiveThumb::Low) { lo } else { hi });
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
                    let scroll_amount = delta.value_notches_y();
                    let step = 0.02;
                    let adjust_low = if dist_low < dist_high {
                        true
                    } else if dist_high < dist_low {
                        false
                    } else {
                        scroll_amount > 0.0
                    };
                    if adjust_low {
                        let new_val = (self.value_low + scroll_amount * step).clamp(0.0, self.value_high);
                        if (new_val - self.value_low).abs() > 0.0001 {
                            self.value_low = new_val;
                        }
                    } else {
                        let new_val = (self.value_high + scroll_amount * step).clamp(self.value_low, 1.0);
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
