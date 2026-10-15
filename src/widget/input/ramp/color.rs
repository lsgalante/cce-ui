//! `ColorRamp`: the colour gradient editor — keys of colour along a bar, interpolated between,
//! the selected one edited by R, G and B sliders and deleted by a button. One keyboard stop
//! whose fields — the track, then the sliders and the button — take the keyboard in turn, as
//! the float ramp's do ([`Fields`]); to a screen reader, a group of its keys and controls.

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
    /// Where the keyboard is among the fields ([`TRACK`](Self::TRACK) …
    /// [`DELETE`](Self::DELETE); [`Fields`]).
    walk: FieldWalk,

    // Child controls for color editing & deletion
    pub r_slider: Adapted<Slider>,
    pub g_slider: Adapted<Slider>,
    pub b_slider: Adapted<Slider>,
    pub del_button: Adapted<Button>,
}

impl ColorRamp {
    /// The colour ramp's fields, in the order Tab walks them: the track (the ramp itself),
    /// the Red, Green and Blue sliders (channel `ch` is field `RED + ch`) and the delete
    /// button.
    pub(super) const TRACK: usize = OWN;
    pub(super) const RED: usize = 1;
    pub(super) const DELETE: usize = 4;

    pub fn new() -> Adapted<ColorRamp> {
        let keys = vec![
            ColorRampKey { pos: 0.0, color: [0.0, 0.0, 0.0] },
            ColorRampKey { pos: 1.0, color: [1.0, 1.0, 1.0] },
        ];

        let r_slider = Slider::new().with_label("Red");
        let g_slider = Slider::new().with_label("Green");
        let b_slider = Slider::new().with_label("Blue");
        // Sized by `arrange_fields`: `DEL_W` wide, the toolkit's button height.
        let del_button = Button::new(0.0, 0.0, 0.0, 0.0).with_label("Delete Key");

        Adapted::new(ColorRamp {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            walk: FieldWalk::default(),
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

    /// Restore sort order after `keys[i]` changed position and return the key's new index
    /// ([`resettle`]).
    pub(super) fn resettle_key(&mut self, i: usize) -> usize {
        resettle(&mut self.keys, i, |k| k.pos)
    }

    /// Delete the selected key — what a click on the delete button does; nothing is selected
    /// after. A ramp keeps two keys at least: with two, nothing happens.
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

    /// Channel `ch`'s slider: red, green, blue.
    fn slider(&self, ch: usize) -> &Adapted<Slider> {
        match ch {
            0 => &self.r_slider,
            1 => &self.g_slider,
            _ => &self.b_slider,
        }
    }

    fn slider_mut(&mut self, ch: usize) -> &mut Adapted<Slider> {
        match ch {
            0 => &mut self.r_slider,
            1 => &mut self.g_slider,
            _ => &mut self.b_slider,
        }
    }

    /// Write channel `ch`'s slider into the selected key's colour (with none selected,
    /// nothing).
    fn write_channel(&mut self, ch: usize) {
        if let Some(idx) = self.selected_key_idx {
            self.keys[idx].color[ch] = self.slider(ch).inner().value();
        }
    }

    /// Select key `idx`, the sliders showing its colour.
    fn select_key(&mut self, idx: usize) {
        self.selected_key_idx = Some(idx);
        self.load_sliders(self.keys[idx].color);
    }

    /// Show colour `c` on the R, G and B sliders, stopping a wheel glide: left running, it
    /// went on into the key now shown (`tick_ctx` writes a gliding slider into the selected
    /// key).
    fn load_sliders(&mut self, c: [f32; 3]) {
        for (ch, v) in c.into_iter().enumerate() {
            let slider = self.slider_mut(ch).inner_mut();
            slider.stop_glide();
            slider.set_value(v);
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
    fn peg_order(&self) -> impl Iterator<Item = usize> {
        peg_order(self.keys.len(), self.selected_key_idx)
    }

    /// The key whose peg is under `(px, py)`, each peg within its drawn radius
    /// ([`peg_drawn_r`](Self::peg_drawn_r)), topmost first ([`topmost_peg`]).
    pub(super) fn key_at(&self, px: f32, py: f32) -> Option<usize> {
        topmost_peg(self.keys.len(), self.selected_key_idx, px, py, |i| (self.peg_centre(i), self.peg_drawn_r(i)))
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
    /// its pegs, while a key is selected — the button while it is shown
    /// ([`deletable`](Fields::deletable)). Each slider row is its label strip over a slider of
    /// the toolkit's height; the button is the toolkit's height, beside the rows' middle.
    pub(super) fn arrange_fields(&mut self) {
        let (x, w) = (self.base.x, self.base.w);
        let sy = self.slider_top();
        let row_h = Self::slider_row_h();
        let bh = crate::layout::button_height();
        let slider_w = (w - 2.0 * Self::MARGIN - Self::DEL_W - 10.0).max(40.0);

        if self.selected_key_idx.is_some() {
            for ch in 0..3 {
                self.slider_mut(ch).set_rect(x + Self::MARGIN, sy + ch as f32 * (row_h + Self::ROW_GAP), slider_w, row_h);
            }
            if self.deletable() {
                let rows_h = Self::slider_block_h();
                self.del_button.set_rect(x + w - Self::MARGIN - Self::DEL_W, sy + (rows_h - bh) / 2.0, Self::DEL_W, bh);
            } else {
                park(&mut self.del_button);
            }
        } else {
            for ch in 0..3 {
                park(self.slider_mut(ch));
            }
            park(&mut self.del_button);
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

    // The fields are the editor's own values, never in the registry, and painted here
    // (while a key is selected): the walk must not also descend.
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
        // Lit while the track has the keyboard.
        pc.well_rim_lit(track, radius, crate::layout::control_relief(), self.walk.at == Some(Self::TRACK));

        self.paint_pegs(pc);

        // The fields paint themselves (`paint_self`), once.
        if self.selected_key_idx.is_some() {
            let dummy = UiContext::new();
            self.r_slider.paint_self(&dummy, pc);
            self.g_slider.paint_self(&dummy, pc);
            self.b_slider.paint_self(&dummy, pc);
            if self.deletable() {
                self.del_button.paint_self(&dummy, pc);
            }
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

impl Fields for ColorRamp {
    const TOTAL: usize = 5;

    /// The track, then the three sliders while a key is selected and the delete button while
    /// it is shown ([`deletable`](Fields::deletable)).
    fn field_count(&self) -> usize {
        if self.deletable() {
            Self::TOTAL
        } else if self.selected_key_idx.is_some() {
            Self::DELETE
        } else {
            Self::RED
        }
    }

    fn field(&mut self, i: usize) -> &mut dyn WidgetHost {
        match i {
            Self::DELETE => &mut self.del_button,
            _ => self.slider_mut(i - Self::RED),
        }
    }

    fn field_id(&self, i: usize) -> WidgetId {
        match i {
            Self::DELETE => self.del_button.base().id(),
            _ => self.slider(i - Self::RED).base().id(),
        }
    }

    fn walk(&self) -> &FieldWalk {
        &self.walk
    }

    fn walk_mut(&mut self) -> &mut FieldWalk {
        &mut self.walk
    }

    /// The track's keys pick and edit the keys ([`own_key`](Fields::own_key)); a slider keeps
    /// its own keys, and a move of it moves the selected key's channel with it; the delete
    /// button's press deletes.
    fn field_key(&mut self, i: usize, event: &KeyEvent, own: WidgetId, ui: &mut UiContext) -> bool {
        match i {
            Self::TRACK => self.own_key(&event.logical_key),
            Self::DELETE => self.delete_button_key(event, own, ui),
            _ => {
                let ch = i - Self::RED;
                let before = self.slider(ch).inner().value();
                let used = self.slider_mut(ch).keyboard_input(event, ui);
                if self.slider(ch).inner().value() != before {
                    self.write_channel(ch);
                    self.just_changed = true;
                }
                used
            }
        }
    }

    fn key_count(&self) -> usize {
        self.keys.len()
    }

    fn selected_key(&self) -> Option<usize> {
        self.selected_key_idx
    }

    fn key_pos(&self, i: usize) -> f32 {
        self.keys[i].pos
    }

    fn pick_key(&mut self, idx: usize) {
        self.select_key(idx);
        self.arrange_fields();
    }

    /// A key of the gradient's colour at `pos`.
    fn add_key_at(&mut self, pos: f32) -> usize {
        let color = self.get_interpolated_color(pos);
        self.keys.push(ColorRampKey { pos, color });
        self.just_changed = true;
        self.resettle_key(self.keys.len() - 1)
    }

    fn remove_selected_key(&mut self) {
        self.delete_selected();
    }

    fn delete_button(&mut self) -> &mut Adapted<Button> {
        &mut self.del_button
    }
}

impl ColorRamp {
    /// The control field item `idx` of [`a11y_items`](Input::a11y_items) stands for, past
    /// the keys: Red, Green, Blue, Delete.
    fn item_field(&self, idx: usize) -> Option<usize> {
        let field = match idx.checked_sub(self.keys.len())? {
            k @ 0..=2 => Self::RED + k,
            3 => Self::DELETE,
            _ => return None,
        };
        (field < self.field_count()).then_some(field)
    }
}

/// A key's colour as a reader hears it: `#rrggbb`, each channel as its slider shows it.
fn colour_hex(c: [f32; 3]) -> String {
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c[0]), byte(c[1]), byte(c[2]))
}

impl Input for ColorRamp {
    fn wants_tick(&self) -> bool {
        true
    }

    /// One keyboard stop, the track first: the ramp walks its own fields (`keeps_tab`).
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    /// Tab stays in the ramp while it has a field to go that way ([`Fields`]).
    fn keeps_tab(&self, reverse: bool) -> bool {
        self.keeps_field_tab(reverse)
    }

    /// The ramp's parts as a screen reader's nodes, in reading order: a radio button per key
    /// (on while selected, and the keyboard's node while the track has it), named with its
    /// place, position and colour; while a key is selected the Red, Green and Blue sliders,
    /// with their numbers, which a reader may set; and the delete button while it is shown.
    /// Each control's item stands for its field, so it is the keyboard's node while that
    /// field has it.
    fn a11y_items(&self, _rect: Rect) -> Vec<crate::a11y::A11yItem> {
        use crate::a11y::A11yItem;
        use accesskit::Role;
        let n = self.keys.len();
        let r = Self::peg_outer_r();
        let mut items: Vec<A11yItem> = (0..n)
            .map(|i| {
                let (cx, cy) = self.peg_centre(i);
                let key = &self.keys[i];
                let (place, count) = ((i + 1).to_string(), n.to_string());
                let (pos, colour) = (format!("{:.2}", key.pos), colour_hex(key.color));
                let args = [("n", place.as_str()), ("count", &count), ("position", &pos), ("colour", &colour)];
                let label = crate::l10n::tr_args("ramp-colour-key", &args);
                let selected = self.selected_key_idx == Some(i);
                A11yItem {
                    toggled: Some(selected),
                    focused: selected,
                    ..A11yItem::new(Role::RadioButton, label, Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r })
                }
            })
            .collect();
        let rect = |w: &dyn WidgetHost| {
            let (x, y, width, height) = w.rect();
            Rect { x, y, width, height }
        };
        if self.selected_key_idx.is_some() {
            for ch in 0..3 {
                let s = self.slider(ch);
                items.push(A11yItem {
                    value: s.a11y_value(),
                    range: s.a11y_range(),
                    field: Some(self.field_id(Self::RED + ch)),
                    ..A11yItem::new(Role::Slider, s.label().unwrap_or_default(), rect(s))
                });
            }
        }
        if self.deletable() {
            items.push(A11yItem {
                field: Some(self.field_id(Self::DELETE)),
                ..A11yItem::new(Role::Button, self.del_button.label().unwrap_or_default(), rect(&self.del_button))
            });
        }
        items
    }

    /// A reader's Click on item `idx`: on a key, select it and put the keyboard on the
    /// track; on a control, give it the keyboard, and on Delete press it. The keyboard moves
    /// at the next tick, which has the context.
    fn a11y_select_item(&mut self, idx: usize) -> bool {
        if idx < self.keys.len() {
            let changed = self.selected_key_idx != Some(idx);
            if changed {
                self.pick_key(idx);
            }
            return self.reader_to_own() || changed;
        }
        let Some(field) = self.item_field(idx) else { return false };
        self.reader_to_field(field, field == Self::DELETE);
        true
    }

    /// A reader's number for a Red, Green or Blue item, in its slider's range: the selected
    /// key's channel moves with the slider.
    fn a11y_set_item_value(&mut self, idx: usize, value: f64) -> bool {
        let Some(field) = self.item_field(idx).filter(|&f| f != Self::DELETE) else { return false };
        let ch = field - Self::RED;
        if !self.slider_mut(ch).a11y_set_value(value) {
            return false;
        }
        self.write_channel(ch);
        self.just_changed = true;
        true
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        let own = ectx.id;
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = self.just_changed;
        self.just_changed = false;
        if self.follow_focus(own, ui) {
            changed = true;
        }

        if self.selected_key_idx.is_some() {
            for ch in 0..3 {
                if self.slider_mut(ch).tick(dt, ui) {
                    self.write_channel(ch);
                    changed = true;
                }
            }
        }
        changed
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                let (button, state, px, py) = (*button, *state, *x, *y);
                let own = ectx.id;
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if button != MouseButton::Left {
                    return false;
                }

                if state == ElementState::Pressed {
                    // A press that takes or makes a key gives the track the keyboard when one
                    // of the ramp's controls has it, so the track's keys act on that key — as
                    // they do when the press brings the keyboard from outside the ramp (the
                    // host focuses it: `FocusIn`).
                    let to_track = self.focused_field(own, ui).is_some_and(|f| f != Self::TRACK);
                    // The key whose peg is under the press, the topmost where pegs overlap.
                    if let Some(idx) = self.key_at(px, py) {
                        if to_track {
                            self.focus_field(Self::TRACK, own, ui);
                        }
                        self.selected_key_idx = Some(idx);
                        self.is_dragging_key = true;
                        self.load_sliders(self.keys[idx].color);
                        self.arrange_fields();
                        return true;
                    }

                    let track = self.track_rect();
                    if within(track, px, py) {
                        if to_track {
                            self.focus_field(Self::TRACK, own, ui);
                        }
                        let t = (px - track.x) / track.width;
                        let col = self.get_interpolated_color(t);
                        self.keys.push(ColorRampKey { pos: t, color: col });
                        let new_idx = self.resettle_key(self.keys.len() - 1);
                        self.just_changed = true;
                        self.selected_key_idx = Some(new_idx);
                        self.load_sliders(col);
                        self.arrange_fields();
                        return true;
                    }

                    if self.selected_key_idx.is_some() {
                        for ch in 0..3 {
                            if self.slider_mut(ch).mouse_input(button, state, px, py, ui) {
                                return true;
                            }
                        }
                        // Pressed only: the button clicks on its release, below.
                        if self.del_button.mouse_input(button, state, px, py, ui) {
                            return true;
                        }
                    }
                    false
                } else {
                    // A release is the ramp's only when something of it used it: a key drag
                    // ending, a slider or the delete button letting go. Releases reach widgets
                    // without a hit test, and claiming every one while a key was selected kept
                    // them from whatever the press was on. Every field is told regardless of the
                    // selection, so none is left holding a press.
                    let dragged = std::mem::take(&mut self.is_dragging_key);
                    let mut used = dragged;
                    for ch in 0..3 {
                        used |= self.slider_mut(ch).mouse_input(button, state, px, py, ui);
                    }
                    let del = self.del_button.mouse_input(button, state, px, py, ui);
                    if del && self.del_button.take_click() {
                        self.delete_selected();
                    }
                    used || del
                }
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                let mut changed = false;

                if self.is_dragging_key {
                    if let Some(idx) = self.selected_key_idx {
                        let track = self.track_rect();
                        self.keys[idx].pos = ((px - track.x) / track.width).clamp(0.0, 1.0);
                        self.selected_key_idx = Some(self.resettle_key(idx));
                        changed = true;
                    }
                }

                if self.selected_key_idx.is_some() {
                    for ch in 0..3 {
                        if self.slider_mut(ch).cursor_moved(px, py, ui) {
                            self.write_channel(ch);
                            changed = true;
                        }
                    }
                    if self.del_button.cursor_moved(px, py, ui) {
                        changed = true;
                    }
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::MouseWheel { delta, x, y, .. } => {
                // The sliders' wheel, the key's colour following at once.
                let (delta, px, py) = (*delta, *x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                if self.selected_key_idx.is_none() {
                    return false;
                }
                let mut changed = false;
                for ch in 0..3 {
                    if self.slider_mut(ch).mouse_wheel(&delta, px, py, ui) {
                        self.write_channel(ch);
                        changed = true;
                    }
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::KeyInput(event) => {
                // To the field with the keyboard ([`Fields::walk_key`]).
                let own = ectx.id;
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                self.walk_key(event, own, ui)
            }
            Event::FocusIn => {
                // Focused itself — by the Tab walk (either way), a click, a reader — the
                // ramp takes the keyboard on its track.
                self.base.focused = true;
                self.walk_focus_in();
                true
            }
            Event::FocusOut => {
                self.base.focused = false;
                self.walk_focus_out();
                true
            }
            _ => false,
        }
    }

    // A key drag and the sliders' own drags run through the editor: the router records it as
    // the drag target once a press is handled here, and a slider's moves sync the key's
    // colour as they go.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_key || (0..3).any(|ch| self.slider(ch).is_dragging())
    }
    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        let mut changed = false;
        for ch in 0..3 {
            let slider = self.slider_mut(ch);
            if slider.is_dragging() && slider.drag_update(px, py) {
                self.write_channel(ch);
                changed = true;
            }
        }
        if changed {
            self.just_changed = true;
        }
        changed
    }
    fn drag_end(&mut self) {
        for ch in 0..3 {
            self.slider_mut(ch).drag_end();
        }
        self.is_dragging_key = false;
    }
}
