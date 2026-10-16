//! The ramp editors. [`Ramp`] is a Houdini-style float ramp: keys on a curve over a plot, a
//! preset and line-type dropdown, and while a key is selected a 2-axis key pad (its position
//! and value) and — with more than two keys — a delete button. A hover-scroll over a key
//! steers it and coasts. [`ColorRamp`] is the colour gradient: keys of colour along a bar, the
//! selected one edited by R, G and B sliders and — with more than two keys — a delete button.
//! [`ColorPalette`] is its discrete kin: a row of colour swatches, added, deleted, dragged into
//! order, and edited by the same sliders and button.
//! Each ramp's controls are its own values, never registered. Each ramp is one keyboard stop
//! whose fields take the keyboard in turn ([`Fields`], which both share): the ramp itself (its
//! graph or track: the ramp's own id focused), then each control, which the window's focus
//! record names with the ramp as its owner (`claim_field_focus`); the ramp hands it the keys.
//! To a screen reader each is a group of its keys and controls (`a11y_items`). A ramp's spec
//! string is `cce_core::ramp`'s (`format_ramp_spec` / `parse_ramp_spec`, re-exported here).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the float `Ramp`: its key, construction, presets, the spec in and out, `impl Layout`; and what both ramps share: the keyboard on their fields and keys (`Fields`), resettling a moved key, peg order and the topmost-peg hit test, the closed point test, parking a field, the margin round the well and the selected peg's ring |
//! | `geometry` | the plot, the key pegs (size, order, hit test), where a dragged key lands, the controls' places |
//! | `paint` | `impl Paint for Ramp` |
//! | `input` | `impl Input for Ramp`: key drags, the hover-scroll, its fields and keys (`impl Fields`), the accessibility items |
//! | `color` | the `ColorRamp` widget but its paint: its key, construction, geometry (track, pegs, the fields' places), `impl Layout`, `impl Fields`, `impl Input` and the accessibility items |
//! | `color_paint` | `impl Paint for ColorRamp`: the track and its gradient, the pegs (`paint_peg`), the fields |
//! | `palette` | the `ColorPalette` widget but its paint: its swatches, construction, geometry (strip, the swatches' square tiles, the add slot, the fields' places), `impl Layout`, `impl Fields`, `impl Input` and the accessibility items |
//! | `palette_paint` | `impl Paint for ColorPalette`: the strip, its swatch tiles (control plates) and add slot, the fields |

mod color;
mod color_paint;
mod geometry;
mod input;
mod paint;
mod palette;
mod palette_paint;
#[cfg(test)]
mod color_keyboard_tests;
#[cfg(test)]
mod palette_tests;
#[cfg(test)]
mod keyboard_tests;
#[cfg(test)]
mod tests;

pub use color::{ColorRamp, ColorRampKey};
pub use palette::ColorPalette;

use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::model::{EventCtx, Input, Layout, Paint};
use crate::widget::*;
use crate::widget::input::{Slider, Slider2D, Button};

#[derive(Debug, Clone)]
pub struct RampKey {
    pub pos: f32,
    pub value: f32,
}

/// A key drag's start ([`Ramp::drag_grab`](Ramp)).
#[derive(Debug, Clone, Copy)]
pub(super) struct DragGrab {
    /// The key's position when the drag began.
    pub origin: f32,
    /// The key's centre less the pointer, in plot units (position, value).
    pub offset: (f32, f32),
}

pub struct Ramp {
    pub base: Widget,
    pub keys: Vec<RampKey>,
    pub selected_key_idx: Option<usize>,
    pub is_dragging_key: bool,
    pub just_changed: bool,
    /// Where the keyboard is among the fields ([`GRAPH`] … [`DELETE`]; [`Fields`]).
    walk: FieldWalk,
    /// The key latched by the current hover-scroll gesture: a trackpad
    /// scroll starting over a key steers that key until the fingers lift
    /// (a >250ms pause reads as a new gesture and re-latches by hover).
    scroll_key_idx: Option<usize>,
    /// Context-menu toggle: hide the bottom control strip and let the graph
    /// claim its space.
    pub controls_collapsed: bool,
    /// Hover-scroll glide velocity (plot units/sec, applied-delta signs) and
    /// the last scroll-event instant: when the event stream stops, the tick
    /// keeps the latched key coasting with exponential decay.
    scroll_vel: (f32, f32),
    last_key_scroll: Option<web_time::Instant>,
    /// The dragged key as the drag found it: its position when grabbed (the soft wall's
    /// zone never starts behind it — `resisted_pos`), and where on its peg the pointer
    /// took it, in plot units (the key keeps that offset instead of snapping its centre
    /// to the pointer). `None` outside a key drag.
    drag_grab: Option<DragGrab>,

    // Child controls for key editing & deletion. The key pad is a 2-axis
    // slider driving the selected key's position (x) and value (y).
    pub key_pad: Adapted<Slider2D>,
    pub del_button: Adapted<Button>,
    pub preset_dropdown: Adapted<Dropdown>,
    pub line_type_dropdown: Adapted<Dropdown>,
}

/// The float ramp's fields, in the order Tab walks them ([`Fields`]): the graph (the ramp
/// itself), the two dropdowns, the key pad and the delete button.
const GRAPH: usize = OWN;
const PRESET: usize = 1;
const LINE: usize = 2;
const PAD: usize = 3;
const DELETE: usize = 4;
/// How many fields the float ramp has.
const FIELD_TOTAL: usize = 5;

/// The key a graph's arrow, Home or End selects among `n` keys, from `sel`: Left / Up the one
/// before and Right / Down the one after, wrapping (with none selected, Right takes the first
/// and Left the last), Home and End the ends — a radio group's keys. `None` for any other key.
fn key_to_select(key: &Key, n: usize, sel: Option<usize>) -> Option<usize> {
    if n == 0 {
        return None;
    }
    Some(match key {
        Key::Named(NamedKey::ArrowRight | NamedKey::ArrowDown) => sel.map_or(0, |i| (i + 1) % n),
        Key::Named(NamedKey::ArrowLeft | NamedKey::ArrowUp) => sel.map_or(n - 1, |i| (i + n - 1) % n),
        Key::Named(NamedKey::Home) => 0,
        Key::Named(NamedKey::End) => n - 1,
        _ => return None,
    })
}

/// Restore sort order after `keys[i]` changed position (`pos` reads a key's), by adjacent
/// swaps, and return the key's new index — both ramps' `resettle_key`. The moved key is
/// tracked by identity, not found again by its position, so a key within ε of the one it is
/// passing is never mistaken for it, and no float comparison can fail the order.
fn resettle<K>(keys: &mut [K], mut i: usize, pos: impl Fn(&K) -> f32) -> usize {
    while i + 1 < keys.len() && pos(&keys[i]) > pos(&keys[i + 1]) {
        keys.swap(i, i + 1);
        i += 1;
    }
    while i > 0 && pos(&keys[i]) < pos(&keys[i - 1]) {
        keys.swap(i, i - 1);
        i -= 1;
    }
    i
}

/// The pegs of `n` keys in paint order, bottom first: by index, the selected one (`sel`)
/// last, on top.
fn peg_order(n: usize, sel: Option<usize>) -> impl DoubleEndedIterator<Item = usize> {
    let sel = sel.filter(|&i| i < n);
    (0..n).filter(move |&i| Some(i) != sel).chain(sel)
}

/// The key whose peg is under `(px, py)`, topmost first — the reverse of [`peg_order`], so
/// where pegs overlap the press takes the one drawn on top. `peg(i)` is key `i`'s peg centre
/// and the radius it is drawn to; a press past that radius is not on it.
fn topmost_peg(n: usize, sel: Option<usize>, px: f32, py: f32, peg: impl Fn(usize) -> ((f32, f32), f32)) -> Option<usize> {
    peg_order(n, sel).rev().find(|&i| {
        let ((cx, cy), r) = peg(i);
        let (dx, dy) = (px - cx, py - cy);
        dx * dx + dy * dy <= r * r
    })
}

/// Whether `(x, y)` lies in `r`, its edges included: every ramp press test is closed
/// (`Rect::contains` is half-open, and a press on the right or bottom edge would miss).
fn within(r: Rect, x: f32, y: f32) -> bool {
    x >= r.x && x <= r.x + r.width && y >= r.y && y <= r.y + r.height
}

/// The margin a ramp keeps round its well — above it and below the controls, and at the
/// float ramp's sides: both ramps', so their wells sit at one depth in a host.
const MARGIN: f32 = 10.0;

/// A selected key peg's ring, both ramps': its width, and its gap to the face inside it.
const PEG_RING_W: f32 = 1.5;
const PEG_RING_GAP: f32 = 1.5;

/// A ramp's delete button's size: square at the button height with its icon; where the
/// icon set is missing, its label's width in the button font. Both ramps' (`arrange_fields`).
fn delete_size(button: &Adapted<Button>) -> (f32, f32) {
    let bh = crate::layout::button_height();
    let w = if button.inner().has_icon() {
        bh
    } else {
        Layout::intrinsic_size(button.inner()).map_or(bh, |s| s.width)
    };
    (w, bh)
}

/// Park a field off-screen at no size: how a ramp hides a control it is not showing.
fn park(w: &mut dyn WidgetHost) {
    w.set_rect(-1000.0, -1000.0, 0.0, 0.0);
}

/// The field that is the ramp itself, first in the walk: its graph (the float ramp) or track
/// (the colour ramp), the keyboard on it while the window's focus record names the ramp.
const OWN: usize = 0;

/// Where a ramp's keyboard is among its fields ([`Fields`]), beside the window's focus record.
#[derive(Debug, Default)]
pub(super) struct FieldWalk {
    /// The field with the keyboard while the ramp holds it: kept here for what has no
    /// context to ask — `keeps_tab`, the lit rim — and matched to the window's focus record
    /// at every tick ([`Fields::follow_focus`]).
    at: Option<usize>,
    /// A screen reader's Click on an item, waiting for the context (`a11y_select_item` has
    /// none): the field to give the keyboard, and whether to press it as Space does. The
    /// next tick carries it out.
    reader_request: Option<(usize, bool)>,
}

/// The keyboard on a ramp, which both ramps share. A ramp is one Tab stop whose fields take
/// the keyboard in turn: field [`OWN`] is the ramp itself, and the rest are controls it holds
/// by value and never registers, which the window's focus record names with the ramp as their
/// owner (`UiContext::claim_field_focus`; a control checks the record before it takes a key).
/// Keys reach the ramp, which hands them to the field with the keyboard. On the ramp itself
/// the keys pick and edit the ramp's keys ([`own_key`](Self::own_key)). Each ramp says what
/// its fields and keys are; the walk is here.
trait Fields {
    /// How many fields the ramp has at most, itself included.
    const TOTAL: usize;
    /// How few keys it keeps: a ramp two, its ends; a palette one.
    const MIN_KEYS: usize = 2;
    /// How many fields take the keyboard now, in walk order, the ramp itself first.
    fn field_count(&self) -> usize;
    /// Control field `i` (`1..TOTAL`).
    fn field(&mut self, i: usize) -> &mut dyn WidgetHost;
    /// Control field `i`'s id, which the window's focus record names while it has the
    /// keyboard.
    fn field_id(&self, i: usize) -> WidgetId;
    fn walk(&self) -> &FieldWalk;
    fn walk_mut(&mut self) -> &mut FieldWalk;
    /// Hand `event`, a key press, to field `i`; whether it was used.
    fn field_key(&mut self, i: usize, event: &KeyEvent, own: WidgetId, ui: &mut UiContext) -> bool;

    /// How many keys the ramp has, which is selected, and key `i`'s position.
    fn key_count(&self) -> usize;
    fn selected_key(&self) -> Option<usize>;
    fn key_pos(&self, i: usize) -> f32;
    /// Select key `idx`, the controls showing it and placed.
    fn pick_key(&mut self, idx: usize);
    /// Add a key on the ramp at `pos` (its value or colour what the ramp is there), the keys
    /// in order and the ramp marked changed; returns the new key's index.
    fn add_key_at(&mut self, pos: f32) -> usize;
    /// Delete the selected key as the mouse does, leaving none selected.
    fn remove_selected_key(&mut self);
    /// The delete button, a field.
    fn delete_button(&mut self) -> &mut Adapted<Button>;

    /// Whether the delete button is shown: while a key is selected and the ramp has more
    /// than the keys it keeps ([`MIN_KEYS`](Self::MIN_KEYS)). At that count it is hidden —
    /// parked, unpainted, no Tab stop — rather than shown doing nothing (a button has no
    /// disabled state).
    fn deletable(&self) -> bool {
        self.selected_key().is_some() && self.key_count() > Self::MIN_KEYS
    }

    /// The field the window's focus is on, if it is one of the ramp's: the ramp itself when
    /// the record names the ramp (`own`), a control when it names that control.
    fn focused_field(&self, own: WidgetId, ui: &UiContext) -> Option<usize> {
        let focused = ui.focused_widget?;
        if focused == own {
            return Some(OWN);
        }
        (1..Self::TOTAL).find(|&i| self.field_id(i) == focused)
    }

    /// Give field `i` the keyboard: the ramp itself is its own focus (`own`); a control is
    /// named by the window's focus record with the ramp as its owner, and it is told.
    fn focus_field(&mut self, i: usize, own: WidgetId, ui: &mut UiContext) {
        if let Some(old) = self.focused_field(own, ui).filter(|&o| o != i && o != OWN) {
            self.field(old).unfocus();
        }
        if i == OWN {
            ui.claim_focus(own);
        } else {
            ui.claim_field_focus(own, self.field_id(i));
            self.field(i).focus();
        }
        self.walk_mut().at = Some(i);
    }

    /// A control's own press that claims the keyboard for itself (a dropdown's,
    /// `EventCtx::request_focus`, its bare id): record the ramp as its owner, so the Tab
    /// walk, `keeps_tab` and a reader find the keyboard in the ramp.
    fn adopt_pressed_field(&mut self, own: WidgetId, ui: &mut UiContext) {
        let Some(i) = (1..Self::TOTAL).find(|&i| ui.focused_widget == Some(self.field_id(i))) else { return };
        if let Some(old) = self.walk().at.filter(|&o| o != i && o != OWN) {
            self.field(old).unfocus();
        }
        ui.claim_field_focus(own, self.field_id(i));
        self.walk_mut().at = Some(i);
    }

    /// `Input::keeps_tab`: Tab stays in the ramp while it has a field to go that way; past
    /// its last field (or, Shift+Tab, before the ramp itself) the walk leaves it.
    fn keeps_field_tab(&self, reverse: bool) -> bool {
        let at = self.walk().at.unwrap_or(OWN);
        if reverse { at > OWN } else { at + 1 < self.field_count() }
    }

    /// A key the ramp is handed (`Event::KeyInput`). Tab walks the fields — the runner's walk
    /// takes Tab past the last (or, with Shift, the first) field (`keeps_tab`), and a Tab the
    /// ramp is handed anyway wraps; with the keyboard elsewhere it enters on the ramp
    /// itself. Any other press goes to the field with the keyboard.
    fn walk_key(&mut self, event: &KeyEvent, own: WidgetId, ui: &mut UiContext) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        let count = self.field_count();
        let current = self.focused_field(own, ui).filter(|&i| i < count);
        if event.logical_key == Key::Named(NamedKey::Tab) {
            let next = match current {
                Some(curr) if event.shift => if curr == 0 { count - 1 } else { curr - 1 },
                Some(curr) => (curr + 1) % count,
                None => OWN,
            };
            self.focus_field(next, own, ui);
            return true;
        }
        match current {
            Some(i) => self.field_key(i, event, own, ui),
            None => false,
        }
    }

    /// A key on the ramp itself: the arrows, Home and End select a key ([`key_to_select`]),
    /// Delete or Backspace deletes the selected one
    /// ([`delete_from_keyboard`](Self::delete_from_keyboard)), Insert adds one
    /// ([`insert_key`](Self::insert_key)). Returns whether it was one of those.
    fn own_key(&mut self, key: &Key) -> bool {
        match key {
            Key::Named(NamedKey::Delete | NamedKey::Backspace) => self.delete_from_keyboard(),
            Key::Named(NamedKey::Insert) => self.insert_key(),
            _ => match key_to_select(key, self.key_count(), self.selected_key()) {
                Some(idx) => {
                    self.pick_key(idx);
                    true
                }
                None => false,
            },
        }
    }

    /// A key on the focused delete button: Enter or Space presses it, and its click is read
    /// here, at once (left unread, it fired on the next mouse press on the button). The key
    /// goes as [`delete_from_keyboard`](Self::delete_from_keyboard) deletes, and when that
    /// hides the button the keyboard goes back to the ramp itself.
    fn delete_button_key(&mut self, event: &KeyEvent, own: WidgetId, ui: &mut UiContext) -> bool {
        let used = self.delete_button().keyboard_input(event, ui);
        if self.delete_button().take_click() {
            self.delete_from_keyboard();
            if !self.deletable() {
                self.focus_field(OWN, own, ui);
            }
        }
        used
    }

    /// Delete the selected key from the keyboard — Delete on the ramp itself, the delete
    /// button pressed from the keyboard or by a reader — and select its neighbour (the key
    /// after it, else the one before), so a keyboard user can go on. A ramp keeps
    /// [`MIN_KEYS`](Self::MIN_KEYS) at least. Returns whether a key went.
    fn delete_from_keyboard(&mut self) -> bool {
        let Some(idx) = self.selected_key().filter(|_| self.key_count() > Self::MIN_KEYS) else { return false };
        self.remove_selected_key();
        self.pick_key(idx.min(self.key_count() - 1));
        true
    }

    /// Add a key halfway from the selected key (else the first) to the next (from the last,
    /// to the one before), on the ramp there, and select it — Insert on the ramp itself, the
    /// keyboard's way to make a key a press on the ramp makes.
    fn insert_key(&mut self) -> bool {
        let n = self.key_count();
        if n == 0 {
            return false;
        }
        let a = self.selected_key().unwrap_or(0).min(n - 1);
        let b = if a + 1 < n { a + 1 } else { a.saturating_sub(1) };
        let idx = self.add_key_at((self.key_pos(a) + self.key_pos(b)) / 2.0);
        self.pick_key(idx);
        true
    }

    /// A reader's Click on a key item, the key selected: the keyboard goes to the ramp
    /// itself at the next tick, unless it is there. Whether that is a change.
    fn reader_to_own(&mut self) -> bool {
        let moves = self.walk().at != Some(OWN);
        if moves {
            self.walk_mut().reader_request = Some((OWN, false));
        }
        moves
    }

    /// A reader's Click on control field `i`'s item: at the next tick the control takes the
    /// keyboard and, with `press`, the key Space.
    fn reader_to_field(&mut self, i: usize, press: bool) {
        self.walk_mut().reader_request = Some((i, press));
    }

    /// At every tick (`tick_ctx`): carry out a reader's Click, now there is a context — while
    /// the keyboard is still the ramp's (the click put it here) — and keep the field with the
    /// keyboard in step with the window's focus record. A field the record moved away from is
    /// told here: the fields are not in the registry, so the focus change could not reach
    /// them itself. A field that has gone with the keyboard on it (as the selection or a key
    /// goes) hands it back to the ramp itself. Returns whether anything changed.
    fn follow_focus(&mut self, own: WidgetId, ui: &mut UiContext) -> bool {
        let mut changed = false;
        if let Some((i, press)) = self.walk_mut().reader_request.take() {
            if ui.focus_holder() == Some(own) && i < self.field_count() {
                self.focus_field(i, own, ui);
                if press {
                    let space = KeyEvent {
                        logical_key: Key::Named(NamedKey::Space),
                        state: ElementState::Pressed,
                        text: Some(" ".into()),
                        repeat: false,
                        ctrl: false,
                        shift: false,
                        alt: false,
                    };
                    self.field_key(i, &space, own, ui);
                }
                changed = true;
            }
        }
        let focused = match self.focused_field(own, ui) {
            Some(i) if i >= self.field_count() => {
                self.focus_field(OWN, own, ui);
                changed = true;
                Some(OWN)
            }
            f => f,
        };
        if focused != self.walk().at {
            self.walk_mut().at = focused;
            changed = true;
        }
        for i in 1..Self::TOTAL {
            if Some(i) != focused && self.field(i).base().focused {
                self.field(i).unfocus();
                changed = true;
            }
        }
        changed
    }

    /// `FocusIn`: focused itself — by the Tab walk (either way), a click, a reader — the ramp
    /// takes the keyboard on itself, and a control that had it lets go.
    fn walk_focus_in(&mut self) {
        if let Some(old) = self.walk().at.filter(|&o| o != OWN) {
            self.field(old).unfocus();
        }
        self.walk_mut().at = Some(OWN);
    }

    /// `FocusOut`: the keyboard has left the ramp, and every control lets go.
    fn walk_focus_out(&mut self) {
        self.walk_mut().at = None;
        for i in 1..Self::TOTAL {
            self.field(i).unfocus();
        }
    }
}

impl Ramp {
    pub fn new() -> Adapted<Ramp> {
        let keys = vec![
            RampKey { pos: 0.0, value: 0.5 },
            RampKey { pos: 0.2, value: 1.0 },
            RampKey { pos: 0.8, value: 1.0 },
            RampKey { pos: 1.0, value: 0.5 },
        ];

        // The key pad: a 2-axis slider driving the selected key's position
        // (x) and value (y), labeled like the dropdowns.
        let key_pad = Slider2D::new().with_label("Key");
        // A square x-icon button (cce-icons); label fallback if the icon set
        // is missing on this machine. By NAME, not by a captured id: an id
        // does not survive the renderer rebuild a reconnect performs, and the
        // widget outlives the renderer (see `Button::icon_name`). Its size is
        // `arrange_fields`'.
        let del_button =
            Button::new(0.0, 0.0, 0.0, 0.0).with_icon_name("x", &crate::l10n::tr("ramp-delete-key"));
        // Short names on purpose: the strip's columns are narrow, and these
        // render inside param rows too ("Bevel (Raised)" used to clip).
        // Labeled: the dropdowns draw their own detached labels, sitting on
        // the expanded top wall of their inset (the labeled-relief style).
        // The presets, and nothing else: a curve edited by hand is no
        // preset, and the trigger says so by going blank (`sync_preset`)
        // rather than by a "Custom" entry that, picked, did nothing.
        let preset_dropdown = Dropdown::new(
            RAMP_PRESETS.iter().map(|(name, _)| name.to_string()).collect(),
            1,
        ).with_open_upward(true).with_label("Preset");
        let line_type_dropdown = Dropdown::new(
            vec![
                "Linear".to_string(),
                "Bezier".to_string(),
            ],
            0,
        ).with_open_upward(true).with_label("Line");

        Adapted::new(Ramp {
            base: Widget::new(),
            keys,
            selected_key_idx: None,
            is_dragging_key: false,
            just_changed: false,
            walk: FieldWalk::default(),
            scroll_key_idx: None,
            controls_collapsed: false,
            scroll_vel: (0.0, 0.0),
            last_key_scroll: None,
            drag_grab: None,
            key_pad,
            del_button,
            preset_dropdown,
            line_type_dropdown,
        })
    }

    /// Replace the curve with preset `idx` of [`RAMP_PRESETS`] (an index
    /// past the end changes nothing) and show it on the trigger.
    pub fn apply_preset(&mut self, idx: usize) {
        if let Some((_, keys)) = RAMP_PRESETS.get(idx) {
            self.keys = keys_from(keys);
        }
        self.selected_key_idx = None;
        self.sync_preset();
        self.just_changed = true;
    }

    /// Select key `idx` as a press on its peg does, the key pad showing it and placed; an
    /// index past the keys changes nothing. For a host that shows the ramp with a key in hand
    /// (its controls fill the strip it reserves for them).
    pub fn select(&mut self, idx: usize) {
        if idx < self.keys.len() {
            self.pick_key(idx);
        }
    }

    /// Delete the selected key — what a click on the delete button does; nothing is selected
    /// after. A ramp keeps two keys at least: with two, nothing happens.
    pub fn delete_selected(&mut self) {
        if let Some(idx) = self.selected_key_idx {
            if self.keys.len() > 2 {
                self.keys.remove(idx);
                self.selected_key_idx = None;
                self.sync_preset();
                self.just_changed = true;
                self.arrange_fields();
            }
        }
    }

    /// The height the ramp needs: a graph [`GRAPH_H`](Ramp::GRAPH_H) tall over the
    /// control strip ([`strip_reserve`](Ramp::strip_reserve)), or with its controls
    /// collapsed the graph alone in its margins ([`MARGIN`]). What it declares
    /// (`intrinsic_size`), and what a host that sizes its rows itself asks for.
    pub fn natural_height(&self) -> f32 {
        if self.controls_collapsed {
            Self::GRAPH_H + 2.0 * MARGIN
        } else {
            Self::expanded_height()
        }
    }

    /// [`natural_height`](Self::natural_height) with the controls shown, as every ramp
    /// starts: for a host sizing a row before its ramp exists.
    pub fn expanded_height() -> f32 {
        Self::GRAPH_H + Self::strip_reserve()
    }

    /// Show on the preset trigger the preset the curve IS, or nothing when
    /// it is none of them — after a hand edit, or a spec that is no preset.
    /// Blank rather than a stale name, and the dropdown keeps a pick of the
    /// preset it last showed live, so choosing it again puts it back.
    pub fn sync_preset(&mut self) {
        match RAMP_PRESETS.iter().position(|(_, keys)| self.keys_match(keys)) {
            Some(idx) => {
                self.preset_dropdown.selected = idx;
                self.preset_dropdown.custom_display_text = None;
            }
            None => self.preset_dropdown.custom_display_text = Some(String::new()),
        }
    }

    /// The curve's value at `t` — [`crate::layout::sample_ramp_keys`], the
    /// DE's one ramp interpolation, so what this widget draws is exactly
    /// what every consumer of its spec string evaluates.
    /// To sample many points, take [`key_pairs`](Self::key_pairs) once and sample them
    /// (`paint` does).
    pub fn get_interpolated_value(&self, t: f32) -> f32 {
        crate::layout::sample_ramp_keys(&self.key_pairs(), self.smooth(), t)
    }

    /// The keys as `(pos, value)` pairs, the form the DE's ramp functions take.
    pub fn key_pairs(&self) -> Vec<(f32, f32)> {
        self.keys.iter().map(|k| (k.pos, k.value)).collect()
    }

    /// Whether the keys are `pairs`, each within 0.0005 — a preset the curve is, or a spec
    /// that changes nothing.
    fn keys_match(&self, pairs: &[(f32, f32)]) -> bool {
        self.keys.len() == pairs.len()
            && self.keys.iter().zip(pairs).all(|(k, &(pos, value))| {
                (k.pos - pos).abs() <= 0.0005 && (k.value - value).abs() <= 0.0005
            })
    }

    /// Whether the curve is the smooth (monotone cubic) line type vs straight
    /// segments — see [`crate::layout::sample_ramp_keys`].
    pub fn smooth(&self) -> bool {
        self.line_type_dropdown.selected == 1
    }

    /// This ramp's state as the DE's ramp spec string ([`format_ramp_spec`]).
    pub fn spec_string(&self) -> String {
        format_ramp_spec(&self.key_pairs(), self.smooth())
    }

    /// Apply a spec string ([`parse_ramp_spec`]); returns whether anything changed.
    /// Unparsable specs are ignored (keeps the current curve).
    pub fn set_spec(&mut self, spec: &str) -> bool {
        let Some((keys, smooth)) = parse_ramp_spec(spec) else {
            return false;
        };
        let new_line = if smooth { 1 } else { 0 };
        let changed = self.line_type_dropdown.selected != new_line || !self.keys_match(&keys);
        if changed {
            self.keys = keys_from(&keys);
            self.line_type_dropdown.selected = new_line;
            self.selected_key_idx = None;
            self.sync_preset();
            self.arrange_fields();
        }
        changed
    }
}

/// Keys from `(pos, value)` pairs.
fn keys_from(pairs: &[(f32, f32)]) -> Vec<RampKey> {
    pairs.iter().map(|&(pos, value)| RampKey { pos, value }).collect()
}

/// The ramp editor's presets, in the order its Preset dropdown lists them:
/// a name and the keys, `(pos, value)`, the curve is set to.
pub const RAMP_PRESETS: &[(&str, &[(f32, f32)])] = &[
    ("Linear", &[(0.0, 0.0), (1.0, 1.0)]),
    ("Raised", &[(0.0, 0.5), (0.2, 1.0), (0.8, 1.0), (1.0, 0.5)]),
    ("Sunken", &[(0.0, 0.5), (0.2, 0.0), (0.8, 0.0), (1.0, 0.5)]),
    ("Peak", &[(0.0, 0.0), (0.5, 1.0), (1.0, 0.0)]),
    ("Valley", &[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)]),
];

pub use cce_core::ramp::{format_ramp_spec, parse_ramp_spec};

impl Layout for Ramp {
    /// [`natural_height`](Ramp::natural_height): the graph whole over the control strip.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, self.natural_height()))
    }

    /// Take the CONTENT rect and arrange the fields in it — the TreeList's rule. Not
    /// `rect_assigned`'s block: the adapter's block holds the detached label strip above
    /// the content, and from its top the graph sat under a label, which is why the gallery
    /// left the ramps unlabelled.
    fn arrange_children(&mut self, rect: Rect) {
        self.base.x = rect.x;
        self.base.y = rect.y;
        self.base.w = rect.width;
        self.base.h = rect.height;
        self.arrange_fields();
    }

    // No `register_embedded_children`: the fields are never in the registry. The ramp
    // decides which field has the keyboard (`focus_field`) and covers the spatial grid
    // itself, and a registered child dropdown's open popover would occlude the ramp's own
    // hit gate (`is_coordinate_covered` excludes by exact id only).
}
