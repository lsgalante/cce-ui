//! `ColorRamp`: the colour gradient editor — keys of colour along a bar, interpolated between,
//! the selected one edited by R, G and B sliders and deleted by a button.

use super::*;

#[derive(Debug, Clone)]
pub struct ColorRampKey {
    pub pos: f32,
    pub color: [f32; 3],
}

pub struct ColorRamp {
    pub base: Widget,
    pub keys: Vec<ColorRampKey>,
    pub selected_key_idx: Option<usize>,
    pub is_dragging_key: bool,
    pub just_changed: bool,
    
    // Child controls for color editing & deletion
    pub r_slider: Adapted<Slider>,
    pub g_slider: Adapted<Slider>,
    pub b_slider: Adapted<Slider>,
    pub del_button: Adapted<Button>,
    
}

impl ColorRamp {
    pub fn new() -> Adapted<ColorRamp> {
        let keys = vec![
            ColorRampKey { pos: 0.0, color: [0.0, 0.0, 0.0] },
            ColorRampKey { pos: 1.0, color: [1.0, 1.0, 1.0] },
        ];
        
        let r_slider = Slider::new().with_label("Red");
        let g_slider = Slider::new().with_label("Green");
        let b_slider = Slider::new().with_label("Blue");
        let del_button = Button::new(0.0, 0.0, 70.0, 28.0).with_label("Delete Key");
        
        Adapted::new(ColorRamp {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            r_slider,
            g_slider,
            b_slider,
            del_button,
        })
    }
    
    pub fn get_interpolated_color(&self, t: f32) -> [f32; 3] {
        if self.keys.is_empty() {
            return [0.0, 0.0, 0.0];
        }
        if t <= self.keys[0].pos {
            return self.keys[0].color;
        }
        if t >= self.keys[self.keys.len() - 1].pos {
            return self.keys[self.keys.len() - 1].color;
        }
        
        for i in 0..self.keys.len() - 1 {
            let k1 = &self.keys[i];
            let k2 = &self.keys[i+1];
            if t >= k1.pos && t <= k2.pos {
                let range = k2.pos - k1.pos;
                if range.abs() < 0.0001 {
                    return k1.color;
                }
                let w = (t - k1.pos) / range;
                return [
                    k1.color[0] * (1.0 - w) + k2.color[0] * w,
                    k1.color[1] * (1.0 - w) + k2.color[1] * w,
                    k1.color[2] * (1.0 - w) + k2.color[2] * w,
                ];
            }
        }
        self.keys[0].color
    }
    
    /// Restore sort order after `keys[i]` changed position, by adjacent swaps, and
    /// return the key's new index — the float ramp's `resettle_key`. It re-sorted and
    /// found the moved key again by its position, within 0.0001: dragged onto a
    /// neighbour's pixel, the match was the neighbour and the selection jumped to it
    /// (and the sort's comparison panicked on a NaN).
    pub(super) fn resettle_key(&mut self, mut i: usize) -> usize {
        while i + 1 < self.keys.len() && self.keys[i].pos > self.keys[i + 1].pos {
            self.keys.swap(i, i + 1);
            i += 1;
        }
        while i > 0 && self.keys[i].pos < self.keys[i - 1].pos {
            self.keys.swap(i, i - 1);
            i -= 1;
        }
        i
    }

    /// Delete the selected key — what the delete button does, clicked or pressed from the
    /// keyboard. A ramp keeps two keys at least: with two, nothing happens.
    pub fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_key_idx {
            if self.keys.len() > 2 {
                self.keys.remove(idx);
                self.selected_key_idx = None;
                self.just_changed = true;
                self.arrange_fields();
            }
        }
    }

    /// Write the R, G and B sliders into the selected key's colour.
    pub(super) fn apply_sliders_to_selected(&mut self) {
        if let Some(idx) = self.selected_key_idx {
            self.keys[idx].color = [
                self.r_slider.inner().value(),
                self.g_slider.inner().value(),
                self.b_slider.inner().value(),
            ];
            self.just_changed = true;
        }
    }
}

impl ColorRamp {
    /// The delete button's width: room for "Delete Key" in the button font.
    const DEL_W: f32 = 100.0;
    /// The gap between the slider rows.
    const ROW_GAP: f32 = 8.0;
    /// The margin around the whole editor, the float Ramp's.
    const MARGIN: f32 = 10.0;
    /// The peg row under the track: the pegs' centres sit `PEG_DROP` below the track, and
    /// the slider block starts `PEG_ROW` below it.
    const PEG_DROP: f32 = 15.0;
    const PEG_ROW: f32 = 35.0;
    /// A peg's face radius: the key's colour, as a disc plate.
    pub(super) const PEG_R: f32 = 7.0;
    /// The selection ring outside the face: the gap to it, and its width.
    const PEG_RING_GAP: f32 = 1.5;
    const PEG_RING_W: f32 = 2.0;

    /// The gradient track: the editor's width less its margins, `ramp_height()` tall.
    pub(super) fn track_rect(&self) -> Rect {
        Rect {
            x: self.base.x + Self::MARGIN,
            y: self.base.y + Self::MARGIN,
            width: (self.base.w - 2.0 * Self::MARGIN).max(1.0),
            height: crate::layout::ramp_height(),
        }
    }

    /// Key `idx`'s peg centre, under the track at its position.
    pub(super) fn peg_centre(&self, idx: usize) -> (f32, f32) {
        let track = self.track_rect();
        (track.x + self.keys[idx].pos * track.width, track.y + track.height + Self::PEG_DROP)
    }

    /// The flat peg's hairline, outside the face.
    const PEG_HAIRLINE: f32 = 1.0;

    /// A selected peg's outer radius: the face and the selection ring around it.
    pub(super) fn peg_outer_r() -> f32 {
        Self::PEG_R + Self::PEG_RING_GAP + Self::PEG_RING_W
    }

    /// The radius key `idx`'s peg is drawn to, and a press on it lands within: the selected
    /// peg's ring; an unselected peg's face, in its hairline when flat. Past the drawn edge a
    /// press is not on the peg, so it never takes a neighbour whose face is beside the one
    /// under the pointer.
    pub(super) fn peg_drawn_r(&self, idx: usize) -> f32 {
        if Some(idx) == self.selected_key_idx {
            Self::peg_outer_r()
        } else if crate::layout::control_relief() {
            Self::PEG_R
        } else {
            Self::PEG_R + Self::PEG_HAIRLINE
        }
    }

    /// The pegs in paint order, bottom first: by index, the selected one last (on top).
    fn peg_order(&self) -> impl Iterator<Item = usize> + '_ {
        let sel = self.selected_key_idx.filter(|&i| i < self.keys.len());
        (0..self.keys.len()).filter(move |&i| Some(i) != sel).chain(sel)
    }

    /// The key whose peg is under `(px, py)`, each peg within its drawn radius
    /// ([`peg_drawn_r`](Self::peg_drawn_r)), topmost first (the reverse of the paint order),
    /// so where pegs overlap the press takes the one drawn on top.
    pub(super) fn key_at(&self, px: f32, py: f32) -> Option<usize> {
        let order: Vec<usize> = self.peg_order().collect();
        order.into_iter().rev().find(|&idx| {
            let (cx, cy) = self.peg_centre(idx);
            let (dx, dy) = (px - cx, py - cy);
            let r = self.peg_drawn_r(idx);
            dx * dx + dy * dy <= r * r
        })
    }

    /// One slider row: its label strip over a slider of the toolkit's height.
    fn slider_row_h() -> f32 {
        crate::layout::control_label_strip() + crate::layout::slider_height()
    }

    /// The R, G and B rows stacked, `ROW_GAP` apart.
    fn slider_block_h() -> f32 {
        3.0 * Self::slider_row_h() + 2.0 * Self::ROW_GAP
    }

    /// The slider block's top: under the peg row.
    fn slider_top(&self) -> f32 {
        let track = self.track_rect();
        track.y + track.height + Self::PEG_ROW
    }

    /// The height the editor needs: the track, the peg row and the three slider rows, in
    /// its margins. The slider rows are reserved whether or not a key is selected (like the
    /// float Ramp's control strip), so selecting a key never changes the editor's rect.
    /// What it declares (`intrinsic_size`).
    pub fn natural_height(&self) -> f32 {
        Self::MARGIN + crate::layout::ramp_height() + Self::PEG_ROW + Self::slider_block_h() + Self::MARGIN
    }

    /// The R, G and B sliders and the delete button, in the slider block under the track and
    /// its pegs, while a key is selected. Each slider row is its label strip over a slider of
    /// the toolkit's height; the button is the toolkit's height, beside the rows' middle.
    pub(super) fn arrange_fields(&mut self) {
        let (x, w) = (self.base.x, self.base.w);
        let sy = self.slider_top();
        let row_h = Self::slider_row_h();
        let bh = crate::layout::button_height();
        let slider_w = (w - 2.0 * Self::MARGIN - Self::DEL_W - 10.0).max(40.0);

        if self.selected_key_idx.is_some() {
            for (k, slider) in [&mut self.r_slider, &mut self.g_slider, &mut self.b_slider].into_iter().enumerate() {
                slider.set_rect(x + Self::MARGIN, sy + k as f32 * (row_h + Self::ROW_GAP), slider_w, row_h);
            }
            let rows_h = Self::slider_block_h();
            self.del_button.set_rect(x + w - Self::MARGIN - Self::DEL_W, sy + (rows_h - bh) / 2.0, Self::DEL_W, bh);
        } else {
            self.r_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.g_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.b_slider.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            self.del_button.set_rect(-1000.0, -1000.0, 0.0, 0.0);
        }
    }
}

impl Layout for ColorRamp {
    /// [`natural_height`](ColorRamp::natural_height), any width.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, self.natural_height()))
    }

    /// Take the CONTENT rect and arrange the fields in it — the TreeList's rule. Not
    /// `rect_assigned`'s block: the adapter's block holds the detached label strip above
    /// the content, and from its top the track sat under a label, which is why the gallery
    /// left the ramps unlabelled.
    fn arrange_children(&mut self, rect: Rect) {
        self.base.x = rect.x;
        self.base.y = rect.y;
        self.base.w = rect.width;
        self.base.h = rect.height;
        self.arrange_fields();
    }

}

impl Paint for ColorRamp {
    /// No container box: the editor draws its track, pegs and fields itself, on the host's
    /// plate, as the float Ramp does.
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    // Field children are ctx-linked for event propagation but painted here (gated on a
    // key being selected) — the walk must not also descend.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    fn paint(&self, _rect: Rect, pc: &mut PaintCtx) {
        // The track is a canvas well cut through the host's plate, the one the float Ramp's
        // graph and every draw-in canvas share: its floor, the gradient, then its rim over the
        // gradient's edges. At the CONTROL radius, not the panel one: the track is a control's
        // height (`ramp_height`, 32), and the panel's 24 would round it into a pill; at the
        // text box's radius it reads as the field it is. The gradient is clipped to the well's
        // rounded shape, so its square slices never poke past the corners.
        let track = self.track_rect();
        let radius = crate::layout::control_corner_radius();
        pc.well_floor(track, radius, &crate::scene::Material::pane(), false);
        pc.clip_rounded(track, radius, |pc| {
            let slices = 100;
            let slice_w = track.width / slices as f32;
            for i in 0..slices {
                let t1 = i as f32 / slices as f32;
                let col = self.get_interpolated_color(t1 + 0.5 / slices as f32);
                let sx = track.x + t1 * track.width;
                pc.quad(Rect { x: sx, y: track.y, width: slice_w, height: track.height }, [col[0], col[1], col[2], 1.0]);
            }
        });
        pc.well_rim(track, radius, crate::layout::control_relief());

        self.paint_pegs(pc);

        // The fields paint themselves (`paint_self`), once.
        if self.selected_key_idx.is_some() {
            let dummy = UiContext::new();
            self.r_slider.paint_self(&dummy, pc);
            self.g_slider.paint_self(&dummy, pc);
            self.b_slider.paint_self(&dummy, pc);
            self.del_button.paint_self(&dummy, pc);
        }
    }
}

impl ColorRamp {
    /// The key pegs, in paint order — the selected one last, on top. A peg's face is the
    /// key's colour, opaque: the face IS the key's data, so selection never tints it; the
    /// selected peg wears the focus ring outside the face instead. Under relief the face is a
    /// disc plate ([`PaintCtx::plate_shaped`] at `Some(2.0)`, a true circle) with a control
    /// plate's roll (the relief width, at most 0.4 of the radius); flat, a flat disc in a
    /// hairline of the wells' frame colour, so a key the colour of the plate still shows.
    fn paint_pegs(&self, pc: &mut PaintCtx) {
        let r = Self::PEG_R;
        let relief = crate::layout::control_relief();
        let depth = crate::layout::bevel_width().min(0.4 * r);
        let ring = crate::scene::paint::ControlPlate::focus_tint();
        for idx in self.peg_order() {
            let (cx, cy) = self.peg_centre(idx);
            let c = self.keys[idx].color;
            let face = [c[0], c[1], c[2], 1.0];
            if relief {
                let disc = Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r };
                pc.plate_shaped(disc, (r, r, r, r), &crate::scene::Material::opaque(face), depth, Some(2.0));
            } else {
                pc.circle(cx, cy, r + Self::PEG_HAIRLINE, crate::color::well_frame_color(false, false));
                pc.circle(cx, cy, r, face);
            }
            if Some(idx) == self.selected_key_idx {
                pc.arc(
                    cx,
                    cy,
                    Self::peg_outer_r(),
                    Self::PEG_RING_W,
                    0.0,
                    std::f32::consts::TAU,
                    [ring[0], ring[1], ring[2], 1.0],
                );
            }
        }
    }
}

impl Input for ColorRamp {
    fn wants_tick(&self) -> bool {
        true
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        // (The per-tick field-widget re-parenting is gone, 6bd: it was a dummy-ctx
        // `set_parent` whose every effect was discarded — legacy behaved the same.)
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;
        
        if self.selected_key_idx.is_some() {
            if self.r_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[0] = self.r_slider.inner().value();
                }
                changed = true;
            }
            if self.g_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[1] = self.g_slider.inner().value();
                }
                changed = true;
            }
            if self.b_slider.tick(dt, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[2] = self.b_slider.inner().value();
                }
                changed = true;
            }
            if self.del_button.tick(dt, ui) {
                changed = true;
            }
        }
        changed
    
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py_event) = (*button, *state, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        if button != MouseButton::Left { return false; }
        
        let track = self.track_rect();

        if state == ElementState::Pressed {
            // The key whose peg is under the press, the topmost where pegs overlap.
            if let Some(idx) = self.key_at(px, py_event) {
                let color = self.keys[idx].color;
                self.selected_key_idx = Some(idx);
                self.is_dragging_key = true;
                self.r_slider.set_value(color[0]);
                self.g_slider.set_value(color[1]);
                self.b_slider.set_value(color[2]);
                self.arrange_fields();
                return true;
            }

            if px >= track.x && px <= track.x + track.width && py_event >= track.y && py_event <= track.y + track.height {
                let t = (px - track.x) / track.width;
                let col = self.get_interpolated_color(t);
                let new_key = ColorRampKey { pos: t, color: col };
                self.keys.push(new_key);
                let new_idx = self.resettle_key(self.keys.len() - 1);
                self.just_changed = true;
                self.selected_key_idx = Some(new_idx);
                self.r_slider.set_value(col[0]);
                self.g_slider.set_value(col[1]);
                self.b_slider.set_value(col[2]);
                self.arrange_fields();
                return true;
            }
            
            if self.selected_key_idx.is_some() {
                if self.r_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                if self.g_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                if self.b_slider.mouse_input(button, state, px, py_event, ui) { return true; }
                // Pressed only: the button clicks on its release, below.
                if self.del_button.mouse_input(button, state, px, py_event, ui) {
                    return true;
                }
            }
        } else {
            // A release is the ramp's only when something of it used it: a key drag
            // ending, a slider or the delete button letting go. Releases reach widgets
            // without a hit test, and claiming every one while a key was selected kept
            // them from whatever the press was on. Every field is told regardless of the
            // selection, so none is left holding a press.
            let dragged = std::mem::take(&mut self.is_dragging_key);
            let r = self.r_slider.mouse_input(button, state, px, py_event, ui);
            let g = self.g_slider.mouse_input(button, state, px, py_event, ui);
            let b = self.b_slider.mouse_input(button, state, px, py_event, ui);
            let del = self.del_button.mouse_input(button, state, px, py_event, ui);
            if del && self.del_button.take_click() {
                self.delete_selected();
            }
            return dragged || r || g || b || del;
        }
        false
    
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py_event) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        let mut changed = false;
        let track = self.track_rect();

        if self.is_dragging_key {
            if let Some(idx) = self.selected_key_idx {
                let t = ((px - track.x) / track.width).clamp(0.0, 1.0);
                self.keys[idx].pos = t;
                self.selected_key_idx = Some(self.resettle_key(idx));
                changed = true;
            }
        }
        
        if self.selected_key_idx.is_some() {
            if self.r_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[0] = self.r_slider.inner().value();
                    changed = true;
                }
            }
            if self.g_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[1] = self.g_slider.inner().value();
                    changed = true;
                }
            }
            if self.b_slider.cursor_moved(px, py_event, ui) {
                if let Some(idx) = self.selected_key_idx {
                    self.keys[idx].color[2] = self.b_slider.inner().value();
                    changed = true;
                }
            }
            if self.del_button.cursor_moved(px, py_event, ui) {
                changed = true;
            }
        }
        if changed {
            self.just_changed = true;
        }
        changed
    
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // Wheel forwarding (6bd self-routing): with the field widgets no longer
                // tree-linked, the sliders' wheel rides this arm — and the key color syncs
                // immediately (the old descent path left it stale until the next hover flip).
                let (delta, px, py) = (*delta, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
                if self.selected_key_idx.is_none() {
                    return false;
                }
                let mut changed = false;
                if self.r_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].color[0] = self.r_slider.inner().value();
                    }
                    changed = true;
                }
                if self.g_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].color[1] = self.g_slider.inner().value();
                    }
                    changed = true;
                }
                if self.b_slider.mouse_wheel(&delta, px, py, ui) {
                    if let Some(idx) = self.selected_key_idx {
                        self.keys[idx].color[2] = self.b_slider.inner().value();
                    }
                    changed = true;
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::KeyInput(event) => {
                let Some(ui) = ectx.ui.as_deref_mut() else { return false; };
        // A slider the keyboard moves moves the key's colour with it: it moved only
        // the slider, and the gradient and the slider disagreed.
        let used = if ui.is_focused(&self.r_slider) {
            self.r_slider.keyboard_input(event, ui)
        } else if ui.is_focused(&self.g_slider) {
            self.g_slider.keyboard_input(event, ui)
        } else if ui.is_focused(&self.b_slider) {
            self.b_slider.keyboard_input(event, ui)
        } else {
            false
        };
        if used {
            self.apply_sliders_to_selected();
            return true;
        }
        if ui.is_focused(&self.del_button) {
            // Enter/Space press the button: its click is read here, at once. Left
            // unread, it fired on the next mouse press on the button.
            let used = self.del_button.keyboard_input(event, ui);
            if self.del_button.take_click() {
                self.delete_selected();
            }
            return used;
        }
        false
    
            }
            _ => false,
        }
    }

    // Field-slider drags forward through the composite (6bd self-routing): the router
    // records THIS widget as the drag target once a press is handled here, so the hooks
    // hand DragUpdate to whichever slider armed itself — and sync the key color, which
    // the old descent path never did mid-drag.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging_key
            || self.r_slider.is_dragging()
            || self.g_slider.is_dragging()
            || self.b_slider.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key
            || self.r_slider.is_dragging()
            || self.g_slider.is_dragging()
            || self.b_slider.is_dragging()
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        let mut changed = false;
        if self.r_slider.is_dragging() && self.r_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].color[0] = self.r_slider.inner().value();
            }
            changed = true;
        }
        if self.g_slider.is_dragging() && self.g_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].color[1] = self.g_slider.inner().value();
            }
            changed = true;
        }
        if self.b_slider.is_dragging() && self.b_slider.drag_update(px, py) {
            if let Some(idx) = self.selected_key_idx {
                self.keys[idx].color[2] = self.b_slider.inner().value();
            }
            changed = true;
        }
        if changed {
            self.just_changed = true;
        }
        changed
    }
    fn drag_end(&mut self) {
        self.r_slider.drag_end();
        self.g_slider.drag_end();
        self.b_slider.drag_end();
        self.is_dragging_key = false;
    }
}
