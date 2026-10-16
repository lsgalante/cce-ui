//! `ColorPalette`: an editable palette — a row of colour swatches in a well, ended by an add
//! slot. The selected swatch is edited by the toolkit's [`ColorSelector`] — its hex field, and
//! its swatch, which opens the DE's picker — and deleted by the ramps' button. A swatch is
//! dragged along the row to reorder it. One keyboard stop whose fields — the strip, then the
//! selector and the button — take the keyboard in turn ([`Fields`], the ramps'); to a screen
//! reader, a group of its swatches and controls.

use super::*;
use crate::widget::input::ColorSelector;

pub struct ColorPalette {
    pub base: Widget,
    /// The colours, linear RGB, in order along the strip.
    pub swatches: Vec<[f32; 3]>,
    pub selected_idx: Option<usize>,
    /// A swatch held from its press on: the pointer moves it along the strip.
    pub is_dragging_swatch: bool,
    /// Asks the next tick for a frame.
    pub just_changed: bool,
    /// The colours changed since the host last asked ([`Input::take_change`]).
    changed: bool,
    /// Where the keyboard is among the fields ([`STRIP`](Self::STRIP) …
    /// [`DELETE`](Self::DELETE); [`Fields`]).
    walk: FieldWalk,

    /// The selected swatch's colour, as the toolkit edits one: a hex field and a swatch that
    /// opens the picker. Its value is synced into the selected swatch after everything that
    /// can change it ([`sync_from_selector`](Self::sync_from_selector)).
    pub selector: Adapted<ColorSelector>,
    pub del_button: Adapted<Button>,
}

/// A new palette's colours: six hues round the wheel.
const DEFAULT_SWATCHES: [[f32; 3]; 6] = [
    [0.80, 0.12, 0.10],
    [0.90, 0.50, 0.08],
    [0.92, 0.80, 0.15],
    [0.20, 0.62, 0.25],
    [0.12, 0.35, 0.80],
    [0.50, 0.22, 0.70],
];

/// The colour a swatch added to an empty palette takes.
const NEUTRAL: [f32; 3] = [0.5, 0.5, 0.5];

/// A linear colour as the selector holds one: sRGB bytes.
fn srgb_bytes(c: [f32; 3]) -> [u8; 3] {
    crate::color::to_srgb_rgb(c).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// sRGB bytes as a swatch holds them: linear.
fn linear_of(b: [u8; 3]) -> [f32; 3] {
    crate::color::to_linear_rgb(b.map(|v| v as f32 / 255.0))
}

/// A swatch's colour as the selector shows it and a reader hears it: `#rrggbb`, sRGB.
fn swatch_hex(c: [f32; 3]) -> String {
    let [r, g, b] = srgb_bytes(c);
    format!("#{r:02x}{g:02x}{b:02x}")
}

impl ColorPalette {
    /// The palette's fields, in the order Tab walks them: the strip (the palette itself), the
    /// colour selector and the delete button.
    pub(super) const STRIP: usize = OWN;
    pub(super) const SELECTOR: usize = 1;
    pub(super) const DELETE: usize = 2;

    /// A palette of six hues ([`with_colors`](Self::with_colors) for others).
    pub fn new() -> Adapted<ColorPalette> {
        Self::with_colors(DEFAULT_SWATCHES.to_vec())
    }

    /// A palette of `colors`, linear RGB, each channel 0..1.
    pub fn with_colors(colors: Vec<[f32; 3]>) -> Adapted<ColorPalette> {
        let tr = crate::l10n::tr;
        Adapted::new(ColorPalette {
            base: Widget::new(),
            swatches: colors,
            selected_idx: None,
            is_dragging_swatch: false,
            just_changed: false,
            changed: false,
            walk: FieldWalk::default(),
            selector: ColorSelector::new(srgb_bytes(NEUTRAL)),
            // The ramps' delete button, by name so it survives a renderer rebuild.
            del_button: Button::new(0.0, 0.0, 0.0, 0.0).with_icon_name("x", &tr("palette-delete-colour")),
        })
    }

    /// The colours, in order.
    pub fn colors(&self) -> &[[f32; 3]] {
        &self.swatches
    }

    /// Replace the colours, none selected. Like the typed setters, not a change the host is
    /// told of ([`take_change`](Input::take_change)).
    pub fn set_colors(&mut self, colors: Vec<[f32; 3]>) {
        self.selector.inner_mut().close_picker();
        self.swatches = colors;
        self.selected_idx = None;
        self.is_dragging_swatch = false;
        self.just_changed = true;
        self.arrange_fields();
    }

    /// The selected swatch's colour.
    pub fn selected_color(&self) -> Option<[f32; 3]> {
        self.selected_idx.map(|i| self.swatches[i])
    }

    /// The colours changed: a frame, and a `Changed` for the host.
    fn mark_changed(&mut self) {
        self.just_changed = true;
        self.changed = true;
    }

    /// Insert a swatch at `at` (at most the end), the colour of the one before it — a copy
    /// to edit — else of the first, else grey; returns its index. Nothing is selected by it.
    pub fn add_swatch(&mut self, at: usize) -> usize {
        let at = at.min(self.swatches.len());
        let color = match at {
            0 => self.swatches.first().copied().unwrap_or(NEUTRAL),
            _ => self.swatches[at - 1],
        };
        self.swatches.insert(at, color);
        if let Some(sel) = self.selected_idx.filter(|&s| s >= at) {
            self.selected_idx = Some(sel + 1);
        }
        self.mark_changed();
        at
    }

    /// Delete the selected swatch — what a click on the delete button does; nothing is
    /// selected after, and a picker open on it closes. A palette keeps one swatch at least:
    /// with one, nothing happens.
    pub fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_idx {
            if self.swatches.len() > Self::MIN_KEYS {
                self.selector.inner_mut().close_picker();
                self.swatches.remove(idx);
                self.selected_idx = None;
                self.mark_changed();
                self.arrange_fields();
            }
        }
    }

    /// Move the selected swatch to `to` (clamped to the strip), the others closing up behind
    /// it; whether it moved.
    pub fn move_selected_to(&mut self, to: usize) -> bool {
        let Some(from) = self.selected_idx else { return false };
        let to = to.min(self.swatches.len().saturating_sub(1));
        if to == from {
            return false;
        }
        let c = self.swatches.remove(from);
        self.swatches.insert(to, c);
        self.selected_idx = Some(to);
        self.mark_changed();
        true
    }

    /// Select swatch `idx` and show it on the selector. A picker opened on another swatch
    /// closes: left open, it went on writing into the swatch now shown.
    fn select_swatch(&mut self, idx: usize) {
        if self.selected_idx != Some(idx) {
            self.selector.inner_mut().close_picker();
        }
        self.selected_idx = Some(idx);
        let sel = self.selector.inner_mut();
        if sel.color != srgb_bytes(self.swatches[idx]) {
            Input::set_value_string(sel, &swatch_hex(self.swatches[idx]));
        }
        // Shown, not edited: the host hears of no change.
        Input::take_change(sel);
    }

    /// Write the selector's colour into the selected swatch, when it holds another — typed,
    /// or streamed from the picker. Compared in the selector's bytes, so a swatch it merely
    /// shows keeps its exact linear value. Whether the swatch changed.
    fn sync_from_selector(&mut self) -> bool {
        Input::take_change(self.selector.inner_mut());
        let Some(idx) = self.selected_idx else { return false };
        let shown = self.selector.inner().color;
        if shown == srgb_bytes(self.swatches[idx]) {
            return false;
        }
        self.swatches[idx] = linear_of(shown);
        self.mark_changed();
        true
    }
}

impl ColorPalette {
    /// A swatch's face radius.
    pub(super) const SWATCH_R: f32 = 10.0;
    /// The gap between two selected swatches' rings at full spacing.
    const SWATCH_GAP: f32 = 4.0;

    /// A selected swatch's outer radius: the face and the ramps' selection ring.
    pub(super) fn swatch_outer_r() -> f32 {
        Self::SWATCH_R + PEG_RING_GAP + PEG_RING_W
    }

    /// The strip: a canvas well the editor's width, `ramp_height()` tall, [`MARGIN`] under
    /// the editor's top (the ramps' wells' depth).
    pub(super) fn strip_rect(&self) -> Rect {
        Rect { x: self.base.x, y: self.base.y + MARGIN, width: self.base.w.max(1.0), height: crate::layout::ramp_height() }
    }

    /// A slot's full spacing: a selected swatch's ring and the gap.
    fn full_pitch() -> f32 {
        2.0 * Self::swatch_outer_r() + Self::SWATCH_GAP
    }

    /// The distance between neighbouring swatches' centres: [`full_pitch`](Self::full_pitch),
    /// closed up when the swatches and the add slot would run past the strip's end (then the
    /// swatches overlap, the later on top). The add slot is never closed up on: it stays a full
    /// pitch past the last swatch, so it can always be pressed.
    fn pitch(&self) -> f32 {
        let (full, n) = (Self::full_pitch(), self.swatches.len());
        if n < 2 {
            return full;
        }
        let strip = self.strip_rect();
        full.min(((strip.width - strip.height - full) / (n - 1) as f32).max(0.0))
    }

    /// Slot `i`'s centre: swatch `i`, or past the last the add slot. The first sits half the
    /// strip's height in from its left end, so a ring clears the wall as it clears the floor.
    pub(super) fn slot_centre(&self, i: usize) -> (f32, f32) {
        let strip = self.strip_rect();
        let half = strip.height / 2.0;
        let n = self.swatches.len();
        let x = if i < n || n == 0 {
            i as f32 * self.pitch()
        } else {
            (n - 1) as f32 * self.pitch() + Self::full_pitch()
        };
        (strip.x + half + x, strip.y + half)
    }

    /// The radius swatch `idx` is drawn to, and a press on it lands within: the selected
    /// one's ring; another's face, in its hairline when flat.
    pub(super) fn swatch_drawn_r(&self, idx: usize) -> f32 {
        if Some(idx) == self.selected_idx {
            Self::swatch_outer_r()
        } else if crate::layout::control_relief() {
            Self::SWATCH_R
        } else {
            Self::SWATCH_R + ColorRamp::PEG_HAIRLINE
        }
    }

    /// The swatches in paint order, the selected one last (on top).
    pub(super) fn swatch_order(&self) -> impl Iterator<Item = usize> {
        peg_order(self.swatches.len(), self.selected_idx)
    }

    /// The swatch under `(px, py)`, topmost first.
    pub(super) fn swatch_at(&self, px: f32, py: f32) -> Option<usize> {
        topmost_peg(self.swatches.len(), self.selected_idx, px, py, |i| (self.slot_centre(i), self.swatch_drawn_r(i)))
    }

    /// The add slot's glyph square, centred in the slot after the last swatch.
    pub(super) fn add_rect(&self) -> Rect {
        let (cx, cy) = self.slot_centre(self.swatches.len());
        let r = Self::SWATCH_R;
        Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r }
    }

    /// Whether `(px, py)` is on the add slot: within a swatch's ring of its centre, and on no
    /// swatch (a selected one's ring may reach it on a strip too short for the palette).
    fn on_add(&self, px: f32, py: f32) -> bool {
        let (cx, cy) = self.slot_centre(self.swatches.len());
        let r = Self::swatch_outer_r();
        let (dx, dy) = (px - cx, py - cy);
        dx * dx + dy * dy <= r * r && self.swatch_at(px, py).is_none()
    }

    /// The slot the pointer at `px` is over, as a dragged swatch lands: the nearest swatch
    /// slot.
    fn slot_under(&self, px: f32) -> usize {
        let (x0, _) = self.slot_centre(0);
        let slot = ((px - x0) / self.pitch().max(1.0)).round().max(0.0) as usize;
        slot.min(self.swatches.len().saturating_sub(1))
    }

    /// The selector row's top: the controls' gap under the strip.
    fn row_top(&self) -> f32 {
        let strip = self.strip_rect();
        strip.y + strip.height + crate::layout::control_gap()
    }

    /// The height the editor needs: the strip and the selector row, in its margins. The row
    /// is reserved whether or not a swatch is selected, so selecting one never changes the
    /// editor's rect. What it declares (`intrinsic_size`).
    pub fn natural_height(&self) -> f32 {
        MARGIN + crate::layout::ramp_height() + crate::layout::control_gap() + crate::layout::color_selector_height() + MARGIN
    }

    /// The selector and the delete button under the strip while a swatch is selected: the
    /// selector at its own height, the button in a column at the right the controls' gap
    /// from it, centred on it. The column is kept while the button is hidden, so the
    /// selector's width never jumps as swatches come and go.
    pub(super) fn arrange_fields(&mut self) {
        let (x, w) = (self.base.x, self.base.w.max(1.0));
        let y = self.row_top();
        let (sh, bh) = (crate::layout::color_selector_height(), crate::layout::button_height());
        let (del_w, _) = delete_size(&self.del_button);
        let gap = crate::layout::control_gap();

        if self.selected_idx.is_some() {
            self.selector.set_rect(x, y, (w - del_w - gap).max(1.0), sh);
            if self.deletable() {
                self.del_button.set_rect(x + w - del_w, y + (sh - bh) / 2.0, del_w, bh);
            } else {
                park(&mut self.del_button);
            }
        } else {
            park(&mut self.selector);
            park(&mut self.del_button);
        }
    }

    /// Append a swatch and select it — a press on the add slot.
    fn append_from_pointer(&mut self) {
        let at = self.add_swatch(self.swatches.len());
        self.pick_key(at);
    }

    /// The control field item `idx` of [`a11y_items`](Input::a11y_items) stands for, past
    /// the swatches and the add slot: the selector, Delete.
    fn item_field(&self, idx: usize) -> Option<usize> {
        let field = match idx.checked_sub(self.swatches.len() + 1)? {
            0 => Self::SELECTOR,
            1 => Self::DELETE,
            _ => return None,
        };
        (field < self.field_count()).then_some(field)
    }
}

impl Layout for ColorPalette {
    /// [`natural_height`](ColorPalette::natural_height), any width.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, self.natural_height()))
    }

    /// Take the CONTENT rect and arrange the fields in it, as the ramps do.
    fn arrange_children(&mut self, rect: Rect) {
        self.base.x = rect.x;
        self.base.y = rect.y;
        self.base.w = rect.width;
        self.base.h = rect.height;
        self.arrange_fields();
    }
}

impl Fields for ColorPalette {
    const TOTAL: usize = 3;
    const MIN_KEYS: usize = 1;

    /// The strip, then the selector while a swatch is selected and the delete button while
    /// it is shown.
    fn field_count(&self) -> usize {
        if self.deletable() {
            Self::TOTAL
        } else if self.selected_idx.is_some() {
            Self::DELETE
        } else {
            Self::SELECTOR
        }
    }

    fn field(&mut self, i: usize) -> &mut dyn WidgetHost {
        match i {
            Self::DELETE => &mut self.del_button,
            _ => &mut self.selector,
        }
    }

    fn field_id(&self, i: usize) -> WidgetId {
        match i {
            Self::DELETE => self.del_button.base().id(),
            _ => self.selector.base().id(),
        }
    }

    fn walk(&self) -> &FieldWalk {
        &self.walk
    }

    fn walk_mut(&mut self) -> &mut FieldWalk {
        &mut self.walk
    }

    /// On the strip: Shift+Left and Shift+Right move the selected swatch along it, and the
    /// ramps' keys pick, add and delete ([`own_key`](Fields::own_key)). The selector keeps
    /// its own keys — the hex typed, Enter taking it — the swatch following; the delete
    /// button's press deletes.
    fn field_key(&mut self, i: usize, event: &KeyEvent, own: WidgetId, ui: &mut UiContext) -> bool {
        match i {
            Self::STRIP => {
                if event.shift {
                    let to = match (&event.logical_key, self.selected_idx) {
                        (Key::Named(NamedKey::ArrowLeft), Some(s)) => Some(s.saturating_sub(1)),
                        (Key::Named(NamedKey::ArrowRight), Some(s)) => Some(s + 1),
                        _ => None,
                    };
                    if let Some(to) = to {
                        self.move_selected_to(to);
                        self.arrange_fields();
                        return true;
                    }
                }
                self.own_key(&event.logical_key)
            }
            Self::DELETE => self.delete_button_key(event, own, ui),
            _ => {
                let used = self.selector.keyboard_input(event, ui);
                self.sync_from_selector();
                used
            }
        }
    }

    fn key_count(&self) -> usize {
        self.swatches.len()
    }

    fn selected_key(&self) -> Option<usize> {
        self.selected_idx
    }

    /// A swatch's place along the strip: its index.
    fn key_pos(&self, i: usize) -> f32 {
        i as f32
    }

    fn pick_key(&mut self, idx: usize) {
        self.select_swatch(idx);
        self.arrange_fields();
    }

    /// A swatch at index `pos` ([`add_swatch`](ColorPalette::add_swatch)).
    fn add_key_at(&mut self, pos: f32) -> usize {
        self.add_swatch(pos.max(0.0) as usize)
    }

    fn remove_selected_key(&mut self) {
        self.delete_selected();
    }

    fn delete_button(&mut self) -> &mut Adapted<Button> {
        &mut self.del_button
    }

    /// Insert a copy of the selected swatch after it (with none selected, of the last at the
    /// end) and select it — Insert on the strip, as the add slot makes one.
    fn insert_key(&mut self) -> bool {
        let at = self.selected_idx.map_or(self.swatches.len(), |s| s + 1);
        let idx = self.add_key_at(at as f32);
        self.pick_key(idx);
        true
    }
}

impl Input for ColorPalette {
    fn wants_tick(&self) -> bool {
        true
    }

    /// One keyboard stop, the strip first: the palette walks its own fields (`keeps_tab`).
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }

    fn keeps_tab(&self, reverse: bool) -> bool {
        self.keeps_field_tab(reverse)
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }

    /// The palette's parts as a screen reader's nodes, in reading order: a radio button per
    /// swatch (on while selected), named with its place and colour; the add slot, a button;
    /// while a swatch is selected the selector, its hex; and the delete button while it is
    /// shown.
    fn a11y_items(&self, _rect: Rect) -> Vec<crate::a11y::A11yItem> {
        use crate::a11y::A11yItem;
        use accesskit::Role;
        let n = self.swatches.len();
        let r = Self::swatch_outer_r();
        let disc = |(cx, cy): (f32, f32)| Rect { x: cx - r, y: cy - r, width: 2.0 * r, height: 2.0 * r };
        let mut items: Vec<A11yItem> = (0..n)
            .map(|i| {
                let (place, count, colour) = ((i + 1).to_string(), n.to_string(), swatch_hex(self.swatches[i]));
                let args = [("n", place.as_str()), ("count", &count), ("colour", &colour)];
                let selected = self.selected_idx == Some(i);
                A11yItem {
                    toggled: Some(selected),
                    focused: selected,
                    ..A11yItem::new(Role::RadioButton, crate::l10n::tr_args("palette-colour", &args), disc(self.slot_centre(i)))
                }
            })
            .collect();
        items.push(A11yItem::new(Role::Button, crate::l10n::tr("palette-add-colour"), disc(self.slot_centre(n))));
        let rect = |w: &dyn WidgetHost| {
            let (x, y, width, height) = w.rect();
            Rect { x, y, width, height }
        };
        if let Some(idx) = self.selected_idx {
            items.push(A11yItem {
                field: Some(self.field_id(Self::SELECTOR)),
                ..A11yItem::new(Role::TextInput, swatch_hex(self.swatches[idx]), rect(&self.selector))
            });
        }
        if self.deletable() {
            items.push(A11yItem {
                field: Some(self.field_id(Self::DELETE)),
                ..A11yItem::new(Role::Button, crate::l10n::tr("palette-delete-colour"), rect(&self.del_button))
            });
        }
        items
    }

    /// A reader's Click: on a swatch, select it, the keyboard on the strip; on the add slot,
    /// add a swatch as Insert does; on a control, give it the keyboard, and on Delete press
    /// it.
    fn a11y_select_item(&mut self, idx: usize) -> bool {
        let n = self.swatches.len();
        if idx < n {
            let changed = self.selected_idx != Some(idx);
            if changed {
                self.pick_key(idx);
            }
            return self.reader_to_own() || changed;
        }
        if idx == n {
            self.insert_key();
            self.reader_to_own();
            return true;
        }
        let Some(field) = self.item_field(idx) else { return false };
        self.reader_to_field(field, field == Self::DELETE);
        true
    }

    fn tick_ctx(&mut self, dt: f32, ectx: &mut EventCtx) -> bool {
        let own = ectx.id;
        let Some(ui) = ectx.ui.as_deref_mut() else {
            return false;
        };
        let mut changed = std::mem::take(&mut self.just_changed);
        if self.follow_focus(own, ui) {
            changed = true;
        }
        if self.selected_idx.is_some() {
            // The picker's stream lands in the selector's tick.
            changed |= self.selector.tick(dt, ui);
            changed |= self.sync_from_selector();
        }
        // `sync_from_selector` asked for a frame this tick already gives.
        self.just_changed = false;
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
                    // A press on the strip gives it the keyboard when one of the palette's
                    // controls has it, so the strip's keys act on the swatch it took.
                    let to_strip = self.focused_field(own, ui).is_some_and(|f| f != Self::STRIP);
                    if let Some(idx) = self.swatch_at(px, py) {
                        if to_strip {
                            self.focus_field(Self::STRIP, own, ui);
                        }
                        self.pick_key(idx);
                        self.is_dragging_swatch = true;
                        self.just_changed = true;
                        return true;
                    }
                    if self.on_add(px, py) {
                        if to_strip {
                            self.focus_field(Self::STRIP, own, ui);
                        }
                        self.append_from_pointer();
                        return true;
                    }
                    if self.selected_idx.is_some() {
                        // The selector's own press: on its hex, the field takes the keyboard
                        // (claimed by its bare id, adopted here as the palette's field, and
                        // told, so it begins editing); on its swatch, the picker opens.
                        if self.selector.mouse_input(button, state, px, py, ui) {
                            self.adopt_pressed_field(own, ui);
                            if self.walk().at == Some(Self::SELECTOR) && !self.selector.base().focused {
                                self.selector.focus();
                            }
                            self.just_changed = true;
                            return true;
                        }
                        // Pressed only: the button clicks on its release, below.
                        if self.del_button.mouse_input(button, state, px, py, ui) {
                            return true;
                        }
                    }
                    false
                } else {
                    // A release is the palette's only when something of it used it (the
                    // colour ramp's rule); every field is told, so none is left holding a
                    // press.
                    let dragged = std::mem::take(&mut self.is_dragging_swatch);
                    let sel = self.selector.mouse_input(button, state, px, py, ui);
                    let del = self.del_button.mouse_input(button, state, px, py, ui);
                    if del && self.del_button.take_click() {
                        self.delete_selected();
                    }
                    dragged || sel || del
                }
            }
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                let mut changed = false;
                if self.is_dragging_swatch {
                    changed |= self.move_selected_to(self.slot_under(px));
                }
                if self.selected_idx.is_some() {
                    changed |= self.selector.cursor_moved(px, py, ui);
                    changed |= self.del_button.cursor_moved(px, py, ui);
                }
                if changed {
                    self.just_changed = true;
                }
                changed
            }
            Event::KeyInput(event) => {
                let own = ectx.id;
                let Some(ui) = ectx.ui.as_deref_mut() else { return false };
                self.walk_key(event, own, ui)
            }
            Event::FocusIn => {
                self.base.focused = true;
                self.walk_focus_in();
                true
            }
            Event::FocusOut => {
                self.base.focused = false;
                self.walk_focus_out();
                // The selector takes a half-typed hex as it lets go.
                self.sync_from_selector();
                true
            }
            _ => false,
        }
    }

    // A swatch drag runs through the palette, as a colour ramp's key drag does.
    fn draggable(&self, _rect: Rect) -> bool {
        self.is_dragging()
    }
    fn is_dragging(&self) -> bool {
        self.is_dragging_swatch
    }
    fn drag_end(&mut self) {
        self.is_dragging_swatch = false;
    }
}
