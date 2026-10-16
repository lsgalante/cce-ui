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

        let r_slider = Slider::new().with_label(&crate::l10n::tr("ramp-red"));
        let g_slider = Slider::new().with_label(&crate::l10n::tr("ramp-green"));
        let b_slider = Slider::new().with_label(&crate::l10n::tr("ramp-blue"));
        // The float Ramp's kind: a square x-icon button (at the button height, not the Ramp's
        // 22 px), its label the fallback where the icon set is missing; by name, so it
        // survives a renderer rebuild. Sized by `arrange_fields`.
        let del_button =
            Button::new(0.0, 0.0, 0.0, 0.0).with_icon_name("x", &crate::l10n::tr("ramp-delete-key"));

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
                return mix(k1.color, k2.color, (t - k1.pos) / range);
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
    /// The clearance between the track and a selected peg's ring.
    const PEG_CLEARANCE: f32 = 4.5;
    /// A peg's face radius: the key's colour, as a disc plate.
    pub(super) const PEG_R: f32 = 7.0;

    /// The editor's side margin: a selected peg's outer radius, so a key at either end keeps
    /// its ring inside the editor's rect.
    fn side_margin() -> f32 {
        Self::peg_outer_r()
    }

    /// The gradient track: the editor's width less its side margins, `ramp_height()` tall,
    /// [`MARGIN`] under the editor's top (the float Ramp's graph's).
    pub(super) fn track_rect(&self) -> Rect {
        let side = Self::side_margin();
        Rect {
            x: self.base.x + side,
            y: self.base.y + MARGIN,
            width: (self.base.w - 2.0 * side).max(1.0),
            height: crate::layout::ramp_height(),
        }
    }

    /// How far under the track the pegs' centres sit: a selected peg's ring clears the track
    /// by [`PEG_CLEARANCE`](Self::PEG_CLEARANCE).
    fn peg_drop() -> f32 {
        Self::PEG_CLEARANCE + Self::peg_outer_r()
    }

    /// Key `idx`'s peg centre, under the track at its position.
    pub(super) fn peg_centre(&self, idx: usize) -> (f32, f32) {
        let track = self.track_rect();
        (track.x + self.keys[idx].pos * track.width, track.y + track.height + Self::peg_drop())
    }

    /// The flat peg's hairline, outside the face.
    pub(super) const PEG_HAIRLINE: f32 = 1.0;

    /// A selected peg's outer radius: the face and the selection ring around it ([`PEG_RING_GAP`],
    /// [`PEG_RING_W`], the float Ramp's).
    pub(super) fn peg_outer_r() -> f32 {
        Self::PEG_R + PEG_RING_GAP + PEG_RING_W
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
    pub(super) fn peg_order(&self) -> impl Iterator<Item = usize> {
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

    /// The R, G and B rows stacked, the controls' gap apart (`control_gap`): a controls
    /// column.
    fn slider_block_h() -> f32 {
        3.0 * Self::slider_row_h() + 2.0 * crate::layout::control_gap()
    }

    /// The slider block's top: the controls' gap under a selected peg's ring.
    fn slider_top(&self) -> f32 {
        let track = self.track_rect();
        track.y + track.height + Self::peg_drop() + Self::peg_outer_r() + crate::layout::control_gap()
    }

    /// The height the editor needs: the track, the peg row and the three slider rows, in
    /// its margins. The slider rows are reserved whether or not a key is selected (like the
    /// float Ramp's control strip), so selecting a key never changes the editor's rect.
    /// What it declares (`intrinsic_size`).
    pub fn natural_height(&self) -> f32 {
        let pegs = Self::peg_drop() + Self::peg_outer_r() + crate::layout::control_gap();
        MARGIN + crate::layout::ramp_height() + pegs + Self::slider_block_h() + MARGIN
    }

    /// The delete button's width: square at the button height with its icon (the float Ramp's
    /// is a fixed 22 px); where the icon set is missing, its label's width in the button font.
    fn delete_w(&self) -> f32 {
        let bh = crate::layout::button_height();
        if self.del_button.inner().has_icon() {
            bh
        } else {
            Layout::intrinsic_size(self.del_button.inner()).map_or(bh, |s| s.width)
        }
    }

    /// The R, G and B sliders and the delete button, in the slider block under the track and
    /// its pegs, while a key is selected — the button while it is shown
    /// ([`deletable`](Fields::deletable)). Each slider row is its label strip over a slider of
    /// the toolkit's height; the button is the toolkit's height, in a column at the right the
    /// controls' gap from the sliders, centred on the Green slider's track. The column is kept
    /// while the button is hidden, so the sliders' width never jumps as keys come and go.
    pub(super) fn arrange_fields(&mut self) {
        let side = Self::side_margin();
        let (x, w) = (self.base.x + side, (self.base.w - 2.0 * side).max(1.0));
        let sy = self.slider_top();
        let (row_h, gap) = (Self::slider_row_h(), crate::layout::control_gap());
        let bh = crate::layout::button_height();
        let del_w = self.delete_w();
        let slider_w = (w - del_w - gap).max(1.0);

        if self.selected_key_idx.is_some() {
            for ch in 0..3 {
                self.slider_mut(ch).set_rect(x, sy + ch as f32 * (row_h + gap), slider_w, row_h);
            }
            if self.deletable() {
                let green_track = sy + row_h + gap + crate::layout::control_label_strip();
                let by = green_track + (crate::layout::slider_height() - bh) / 2.0;
                self.del_button.set_rect(x + w - del_w, by, del_w, bh);
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

/// `a` and `b` mixed, `w` of the way to `b`: the gradient between two keys.
pub(super) fn mix(a: [f32; 3], b: [f32; 3], w: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] * (1.0 - w) + b[i] * w)
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
                ..A11yItem::new(Role::Button, crate::l10n::tr("ramp-delete-key"), rect(&self.del_button))
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
