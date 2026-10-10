//! The context menu: one per window (its `WindowState`), shown, painted and dispatched by every
//! app through the free functions below. Behaviour reference: `docs/widgets.md`, "Context menu".
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the constants and slider rows, `ContextMenuState` (construction, show, hide) |
//! | `marks` | the row marks and chevrons drawn as glyphs, splitting a mark off a label, the label-to-action fallback |
//! | `api` | the free-function API apps call, each acting on the current window's menu |
//! | `turn` | page rows and turns: `show_page`, `refill`, the turn animation, the back band |
//! | `geometry` | placement in a window or a popup, scrolling a shortened menu, rows as drawn |
//! | `input` | hover and keyboard stepping, presses, the wheel, slider rows |
//! | `paint` | the plate, rows, sliders, labels and glyphs |

use crate::widget::*;
use std::cell::RefCell;

mod api;
mod geometry;
mod input;
mod marks;
mod paint;
mod turn;

pub use api::*;
pub use marks::*;
pub use paint::paint_menu_plate;
use turn::*;

/// Height of one menu row. Sizing, both hit tests, the label run and the
/// plate's hover fill all step by this — they were five copies of a bare
/// `24.0`, and a menu whose rows are measured differently from where they
/// are drawn selects the entry above the one under the cursor.
pub const ROW_H: f32 = 24.0;

/// The plate's padding, the same on every side: the rows start this far
/// below the top edge and end this far above the bottom, and the labels
/// sit this far in from the left, with the widest label this far from
/// the right. Before 2026-09-21 the rows ran flush to the top and bottom
/// and the labels had 8px on the left against 16px on the right.
pub const PAD: f32 = 8.0;

/// The face a menu label is drawn in: the DE's menu font, family and
/// size — the same `menubar_font` the menubar's own drop-downs use
/// ([`crate::widget::container::menu`]). A menu that hardcodes 12.0 and
/// leaves the family unset renders in the default sans while the list or
/// breadcrumb beneath it wears the configured face.
///
/// Consumers need the family too: a [`TextLabel`] carries only a size, so
/// whoever turns these labels into text prims passes this family alongside
/// them (`pc.text_with(.., Some(family), ..)`).
pub fn label_font() -> (String, f32) {
    crate::layout::menubar_font_parsed()
}

/// The band a slider row draws its control over, logical px.
pub const SLIDER_W: f32 = 120.0;

/// Gap between a slider row's label, its readout and its band.
pub const SLIDER_GAP: f32 = 10.0;

/// A row that is a SLIDER rather than an action: set on a shown menu with
/// [`set_row_slider`], it keeps the menu open while it is worked. The
/// wheel over the row steps it by `step` a notch (a trackpad's fractional
/// notches accumulate, so a fine swipe still arrives in whole steps); a
/// press on its band jumps to the pointer and drags until the release.
/// Each change is drained by the host through [`take_slider_change`] —
/// the menu has no idea what the value means, as it has none what an
/// action row does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MenuSlider {
    pub value: f32,
    pub min: f32,
    pub max: f32,
    /// One wheel notch's change, and the grid a dragged value snaps to
    /// (0 = continuous).
    pub step: f32,
    /// Decimals in the readout.
    pub decimals: usize,
    /// Appended to the readout: `"%"`, `" mm"`.
    pub suffix: &'static str,
}

impl MenuSlider {
    fn clamp_snap(&self, v: f32) -> f32 {
        let (lo, hi) = (self.min.min(self.max), self.min.max(self.max));
        let v = if self.step > 0.0 { self.min + ((v - self.min) / self.step).round() * self.step } else { v };
        v.clamp(lo, hi)
    }

    /// What the row shows to the left of its band.
    pub fn readout(&self) -> String {
        format!("{:.*}{}", self.decimals, self.value, self.suffix)
    }
}

/// A page turn the menu has been asked for, drained by the host with
/// [`take_turn`] (a swipe) or read with [`turn_at`] (a press). The menu
/// does not turn by itself: what a row leads to may be another list of
/// rows or another plate altogether (the designer's dialog), so the host
/// shows it — a list with [`show_page`], which keeps the plate where it
/// stands, so the menu reads as turning into the page rather than as a
/// second menu arriving.
///
/// Until 2026-10-02 a row could open a SUBMENU, a second menu flying out
/// beside it on hover, while rows that led to another plate swapped it
/// in place on a click: two kinds of row for one idea. A page row is
/// both, and is the only kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageTurn {
    /// Into the page row `n` leads to: a press on it, or a side swipe
    /// forward with the pointer on it.
    Into(usize),
    /// Back to the plate this page was turned to from: a press on the
    /// back band, or a side swipe back anywhere over the plate.
    Back,
}

/// How long a page turn takes: the plate grows or shrinks from the size
/// of the one it replaces to its own, the rows it had slide out and fade
/// and the page's slide in from the side the turn comes from.
pub const TURN_MS: f32 = 180.0;

/// [`TURN_MS`], or `CCE_UI_TURN_MS` from the environment — a turn in
/// slow motion, to see one frame by frame (a shadow session's capture
/// takes longer than a whole turn).
pub fn turn_ms() -> f32 {
    static MS: std::sync::OnceLock<f32> = std::sync::OnceLock::new();
    *MS.get_or_init(|| {
        std::env::var("CCE_UI_TURN_MS").ok().and_then(|v| v.parse::<f32>().ok()).filter(|v| *v > 0.0).unwrap_or(TURN_MS)
    })
}

/// How far the rows slide in a turn, logical px.
pub const TURN_SLIDE: f32 = 36.0;

/// The ease of a turn at `t` in 0..=1: fast out of the old plate,
/// settling into the new one.
pub fn turn_ease(t: f32) -> f32 {
    let u = 1.0 - t.clamp(0.0, 1.0);
    1.0 - u * u * u
}

#[derive(Debug, Clone)]
pub struct ContextMenuState {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub visible: bool,
    pub options: Vec<String>,
    pub hovered_item: Option<usize>,
    /// The action target, id-keyed (Phase 6bc slice 2): dispatch resolves it through the
    /// caller's generational tree, so a stale target is a no-op, not a UAF.
    pub target: Option<WidgetId>,
    pub header_count: usize,
    /// Slider rows by index, parallel to `options` — see [`MenuSlider`].
    /// Emptied by every `show`, so a menu's sliders are the ones its
    /// host set this time.
    pub sliders: Vec<Option<MenuSlider>>,
    /// Rows that lead to a PAGE, parallel to `options` — see
    /// [`set_row_page`](Self::set_row_page). Emptied by every `show`.
    pub pages: Vec<bool>,
    /// What each row DOES, parallel to `options` — see
    /// [`set_row_actions`](Self::set_row_actions). A row's action is its identity; its
    /// label is only what is shown, so a translated label still runs the action.
    /// Emptied by every `show`.
    pub actions: Vec<Option<crate::widget::ContextAction>>,
    /// On a page: the title of the plate it was turned to from, which
    /// the back band across its top reads as `‹ Title`. `None` on a menu
    /// that was opened rather than turned to.
    pub back: Option<String>,
    /// The pointer is on the back band.
    pub back_hovered: bool,
    /// Shown by [`show_page`](Self::show_page), in place of the plate it
    /// turned from: a placement keeps its corner where that plate's
    /// was, sliding it on screen rather than flipping it.
    pub turned: bool,
    /// A turn a swipe asked for, until the host takes it.
    turn: Option<PageTurn>,
    /// The turn being animated, from the plate this one replaced.
    turning: Option<Turning>,
    /// When the menu was last hidden: a page shown in the same moment
    /// turns from it, as a host that closes one menu and shows the next
    /// in one dispatch means it to.
    hidden_at: Option<web_time::Instant>,
    /// The slider row a press took hold of, until the release.
    pub slider_drag: Option<usize>,
    /// A trackpad's leftover fraction of a wheel notch.
    wheel_accum: f32,
    /// The last value a slider was moved to, drained by the host.
    slider_change: Option<(usize, f32)>,
    /// The point `show` opened the menu at — the anchor a placement
    /// flips and slides from. `x`/`y` are where the menu IS.
    pub anchor: (f32, f32),
    /// Height of every row plus the padding: the menu's natural height.
    /// `h` is the height it is SHOWN at, which a placement may cut down
    /// to fit the screen; the rows then scroll.
    pub content_h: f32,
    /// How far the rows are scrolled up, 0..=`content_h - h`.
    pub scroll: f32,
    /// Bumped by every `show`, so a host mirroring the menu elsewhere (the
    /// runner's popup surface) can tell a re-show from a repaint.
    pub generation: u64,
    /// The menu is drawn in its own popup surface by the runner, so the
    /// in-window paint calls ([`paint`](Self::paint), [`text_labels`],
    /// [`extra_quads`]) draw nothing — every app still makes them, and a
    /// second copy in the window would show through under the popup.
    /// Hit testing is unaffected: the popup routes its pointer events
    /// back into window coordinates, where the rect is.
    ///
    /// [`text_labels`]: Self::text_labels
    /// [`extra_quads`]: Self::extra_quads
    pub hosted: bool,
    /// The pointer's last place over the menu, to re-hover after a scroll
    /// moves a different row under it.
    last_cursor: Option<(f32, f32)>,
    /// Being painted into the popup surface, where the plate is the
    /// surface's ROOT: frosted by the compositor's blur-behind rather
    /// than the in-app pass, which has no backdrop to sample there — the
    /// popup's own frame is empty behind the plate, and the in-app frost
    /// of nothing is a flat opaque grey.
    in_popup: bool,
}

impl ContextMenuState {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 120.0,
            h: 0.0,
            visible: false,
            options: Vec::new(),
            hovered_item: None,
            target: None,
            header_count: 0,
            sliders: Vec::new(),
            pages: Vec::new(),
            actions: Vec::new(),
            back: None,
            back_hovered: false,
            turned: false,
            turn: None,
            turning: None,
            hidden_at: None,
            slider_drag: None,
            wheel_accum: 0.0,
            slider_change: None,
            anchor: (0.0, 0.0),
            content_h: 0.0,
            scroll: 0.0,
            generation: 0,
            hosted: false,
            last_cursor: None,
            in_popup: false,
        }
    }

    pub fn show(&mut self, x: f32, y: f32, options: Vec<String>, header_count: usize, target: WidgetId) {
        self.x = x;
        self.y = y;
        self.anchor = (x, y);
        self.options = options;
        self.content_h = self.options.len() as f32 * ROW_H + 2.0 * PAD;
        self.h = self.content_h;
        self.scroll = 0.0;
        self.last_cursor = None;
        self.generation = self.generation.wrapping_add(1);
        // Width from the widest label as the RENDERER shapes it —
        // `shaped_cluster_offsets`, the same cosmic-text buffer cache the
        // draw reads — not `measure_text_width`. That one rasterizes an
        // SVG through fontdb and reports inked extent in the named face
        // alone: a glyph the face lacks (the radio marks "●" / "○" the
        // designer's pin rows carry, which Berkeley Mono has not) measures
        // as next to nothing while the draw lands it from a fallback face
        // a full advance wide, and the label ran off the plate's right
        // edge (2026-09-21). The inked measure is kept as a floor, so a
        // host whose font system has no bundled faces never measures
        // narrower than before.
        let (family, size) = label_font();
        let widest = self
            .options
            .iter()
            .map(|s| {
                let (mark, s) = split_mark(s);
                let mark_w = if mark.is_some() { mark_size(size) + MARK_GAP } else { 0.0 };
                let inked = crate::widget::display::measure_text_width(s, &family, size);
                let shaped = crate::geometry_font_system()
                    .lock()
                    .ok()
                    .and_then(|mut fs| {
                        crate::text::shaped_cluster_offsets(&mut fs, s, size, Some(&family))
                            .last()
                            .map(|&(_, total)| total)
                    })
                    .unwrap_or(0.0);
                mark_w + inked.max(shaped)
            })
            .fold(0.0f32, f32::max);
        self.w = (widest + 2.0 * PAD).max(120.0);
        self.visible = true;
        self.hovered_item = None;
        self.target = Some(target);
        self.header_count = header_count;
        self.sliders = vec![None; self.options.len()];
        self.pages = vec![false; self.options.len()];
        self.actions = vec![None; self.options.len()];
        self.back = None;
        self.back_hovered = false;
        self.turned = false;
        self.turn = None;
        self.turning = None;
        self.slider_drag = None;
        self.wheel_accum = 0.0;
        self.slider_change = None;
    }

    /// Make row `idx` lead to a PAGE — see [`PageTurn`]. It wears
    /// [`PAGE_MARK`] at its right end; the plate widens for it.
    /// Give the rows their actions, parallel to `options` (a header or a row the host
    /// handles itself is `None`); called after `show`, like `set_row_page`. A press on a
    /// row runs its action on the menu's target. Rows with none fall back to matching
    /// the label (`legacy_action_for_label`), which only works in English: build
    /// menus with actions.
    pub fn set_row_actions(&mut self, actions: Vec<Option<crate::widget::ContextAction>>) {
        let n = self.options.len();
        self.actions = actions;
        self.actions.resize(n, None);
    }

    pub fn set_row_page(&mut self, idx: usize) {
        if idx >= self.options.len() {
            return;
        }
        let (family, size) = label_font();
        let measure = |t: &str| crate::widget::display::measure_text_width(t, &family, size);
        let (mark, label) = split_mark(&self.options[idx]);
        let mark_w = if mark.is_some() { mark_size(size) + MARK_GAP } else { 0.0 };
        let need = PAD + mark_w + measure(label) + SLIDER_GAP + chevron_size(size) + PAD;
        self.w = self.w.max(need);
        self.pages[idx] = true;
    }

    /// Whether row `idx` leads to a page.
    pub fn leads_to_page(&self, idx: usize) -> bool {
        self.pages.get(idx).copied().unwrap_or(false)
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.target = None;
        self.last_cursor = None;
        self.slider_drag = None;
        self.back_hovered = false;
        self.turn = None;
        self.turning = None;
        self.hidden_at = Some(web_time::Instant::now());
    }
}
