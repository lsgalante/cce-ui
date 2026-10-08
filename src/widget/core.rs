use crate::widget::WidgetHost;

/// Keyboard focus lives in the window's [`crate::context::UiContext`]
/// (`focused_widget`, `set_focused_id`, `clear_focus`, `is_focused_id`, `has_focus`): ONE
/// store, which the Tab walk, the accessibility tree and every widget read. Until
/// 2026-10-08 this module kept a second, per-thread, that widgets claimed in `FocusIn`
/// and apps set directly — kept in step by convention, and apt to disagree with the
/// context on any path that skipped the event (`docs/rfc-global-state.md`, phase 1).
pub mod focus {
    use super::WidgetHost;

    pub fn link_parent_child(parent: &mut dyn WidgetHost, child: &mut dyn WidgetHost, ctx: &mut crate::context::UiContext) {
        let parent_ptr = unsafe {
            std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(parent as *mut dyn WidgetHost)
        };
        let child_ptr = unsafe {
            std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(child as *mut dyn WidgetHost)
        };
        let (p_id, c_id) = (parent.base().id(), child.base().id());
        // SAFETY: both derived from the live borrows we were handed.
        unsafe {
            ctx.register_widget(p_id, parent_ptr);
            ctx.register_widget(c_id, child_ptr);
        }
        // The old add_child + set_parent pair, as the tree ops they always were.
        ctx.tree.link(p_id, c_id);
        ctx.tree.set_parent(c_id, Some(p_id));
    }

    // `navigate_focus` is DELETED (the plumbing retype): it resolved parent/children
    // through a freshly-made EMPTY UiContext, so the parent-based arms (ctrl+u/j/k) could
    // never fire and ctrl+i only fired for a focused container-children widget (Paginator
    // — never focusable). Its one caller (settings) already runs its own section nav.
}

pub mod hover_animation {
    use std::cell::RefCell;

    #[derive(Debug, Clone)]
    pub struct HoverState {
        pub current_x: f32,
        pub current_y: f32,
        pub current_w: f32,
        pub current_h: f32,
        pub current_alpha: f32,

        pub target_x: Option<f32>,
        pub target_y: Option<f32>,
        pub target_w: Option<f32>,
        pub target_h: Option<f32>,
        pub target_alpha: f32,

        pub registered_this_frame: bool,
        pub scroll_offset: f32,
    }

    impl HoverState {
        pub fn new() -> Self {
            Self {
                current_x: 0.0,
                current_y: 0.0,
                current_w: 0.0,
                current_h: 0.0,
                current_alpha: 0.0,

                target_x: None,
                target_y: None,
                target_w: None,
                target_h: None,
                target_alpha: 0.0,

                registered_this_frame: false,
                scroll_offset: 0.0,
            }
        }
    }

    // The highlight and the cursor it follows are the current window's
    // (`crate::window_state`), not the thread's.
    fn hover_state<R>(f: impl FnOnce(&RefCell<HoverState>) -> R) -> R {
        crate::window_state::with(|w| f(&w.hover))
    }
    fn cursor_state<R>(f: impl FnOnce(&RefCell<(f32, f32)>) -> R) -> R {
        crate::window_state::with(|w| f(&w.cursor))
    }

    pub fn set_cursor_pos(x: f32, y: f32) {
        cursor_state(|pos| {
            *pos.borrow_mut() = (x, y);
        });
    }

    pub fn reset_frame_registration() {
        hover_state(|state| {
            state.borrow_mut().registered_this_frame = false;
        });
    }

    pub fn set_scroll_offset(offset: f32) {
        hover_state(|state| {
            state.borrow_mut().scroll_offset = offset;
        });
    }

    pub fn get_scroll_offset() -> f32 {
        hover_state(|state| {
            state.borrow().scroll_offset
        })
    }

    pub fn register_hovered(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        hover_state(|state| {
            let mut s = state.borrow_mut();
            s.target_x = Some(x);
            s.target_y = Some(y);
            s.target_w = Some(w);
            s.target_h = Some(h);
            s.target_alpha = color[3];
            s.registered_this_frame = true;
        });
    }

    pub fn post_render_check() {
        hover_state(|state| {
            let mut s = state.borrow_mut();
            if !s.registered_this_frame {
                s.target_alpha = 0.0;
                let (cx, cy) = cursor_state(|pos| *pos.borrow());
                s.target_x = Some(cx);
                s.target_y = Some(cy + s.scroll_offset);
                s.target_w = Some(0.0);
                s.target_h = Some(0.0);
            }
        });
    }

    pub fn tick(dt: f32) -> bool {
        hover_state(|state| {
            let mut s = state.borrow_mut();
            let decay = 15.0;
            // The per-tick approach fraction; 1 lands on the target at once,
            // which is the whole of animations-off for the highlight.
            let k = if crate::motion::enabled() { 1.0 - (-decay * dt).exp() } else { 1.0 };
            let mut changed = false;

            if s.current_alpha <= 0.001 && s.target_alpha > 0.0 {
                if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
                    s.current_x = tx;
                    s.current_y = ty;
                    s.current_w = tw;
                    s.current_h = th;
                }
            }

            if (s.current_alpha - s.target_alpha).abs() > 0.001 {
                s.current_alpha += (s.target_alpha - s.current_alpha) * k;
                changed = true;
            } else if s.current_alpha != s.target_alpha {
                s.current_alpha = s.target_alpha;
                changed = true;
            }

            if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
                if (s.current_x - tx).abs() > 0.1 {
                    s.current_x += (tx - s.current_x) * k;
                    changed = true;
                } else if s.current_x != tx {
                    s.current_x = tx;
                    changed = true;
                }

                if (s.current_y - ty).abs() > 0.1 {
                    s.current_y += (ty - s.current_y) * k;
                    changed = true;
                } else if s.current_y != ty {
                    s.current_y = ty;
                    changed = true;
                }

                if (s.current_w - tw).abs() > 0.1 {
                    s.current_w += (tw - s.current_w) * k;
                    changed = true;
                } else if s.current_w != tw {
                    s.current_w = tw;
                    changed = true;
                }

                if (s.current_h - th).abs() > 0.1 {
                    s.current_h += (th - s.current_h) * k;
                    changed = true;
                } else if s.current_h != th {
                    s.current_h = th;
                    changed = true;
                }
            }

            changed
        })
    }

    pub fn get_quad() -> Option<(f32, f32, f32, f32, [f32; 4])> {
        hover_state(|state| {
            let s = state.borrow();
            if s.current_alpha > 0.001 {
                Some((
                    s.current_x,
                    s.current_y,
                    s.current_w,
                    s.current_h,
                    [1.0, 1.0, 1.0, s.current_alpha],
                ))
            } else {
                None
            }
        })
    }
}

/// The system clipboard, as text: what every widget's copy, cut and paste go
/// through. One synchronous pair, with a backend per platform:
///
/// - **Wayland** (Linux and the other non-Apple unixes): `wl-copy` /
///   `wl-paste`, `xclip` where those are missing.
/// - **macOS**: the general `NSPasteboard`, plain-text type.
/// - **A page**: a browser hands a page the clipboard only inside a `paste`
///   event, so a read is the text of the last one the browser shell saw (it
///   holds a ⌘/Ctrl+V back until the event has come; `web::shell`), or what
///   the page itself copied last. A copy writes through the async Clipboard
///   API where the page is a secure context, and the shell also answers the
///   `copy` / `cut` event a ⌘/Ctrl+C or X raises with it, which needs none.
///   Before this a copy in a page panicked (`std::thread::spawn`).
pub mod clipboard {
    /// Put `text` on the clipboard.
    pub fn copy_to_clipboard(text: &str) {
        imp::copy(text);
    }

    /// The clipboard's text, if it has any.
    pub fn read_from_clipboard() -> Option<String> {
        imp::read()
    }

    #[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
    mod imp {
        pub fn copy(text: &str) {
            let text = text.to_string();
            std::thread::spawn(move || {
                if let Ok(mut child) = std::process::Command::new("wl-copy")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        use std::io::Write;
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    let _ = child.wait();
                } else if let Ok(mut child) = std::process::Command::new("xclip")
                    .arg("-selection")
                    .arg("clipboard")
                    .stdin(std::process::Stdio::piped())
                    .spawn()
                {
                    if let Some(mut stdin) = child.stdin.take() {
                        use std::io::Write;
                        let _ = stdin.write_all(text.as_bytes());
                    }
                    let _ = child.wait();
                }
            });
        }

        pub fn read() -> Option<String> {
            if let Ok(output) = std::process::Command::new("wl-paste")
                .arg("-n")
                .output() {
                if output.status.success() {
                    if let Ok(text) = String::from_utf8(output.stdout) {
                        return Some(text);
                    }
                }
            }
            if let Ok(output) = std::process::Command::new("xclip")
                .arg("-selection")
                .arg("clipboard")
                .arg("-o")
                .output() {
                if output.status.success() {
                    if let Ok(text) = String::from_utf8(output.stdout) {
                        return Some(text);
                    }
                }
            }
            None
        }
    }

    #[cfg(target_os = "macos")]
    mod imp {
        use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
        use objc2_foundation::NSString;

        pub fn copy(text: &str) {
            let board = NSPasteboard::generalPasteboard();
            board.clearContents();
            // SAFETY: an extern static AppKit defines.
            let ty = unsafe { NSPasteboardTypeString };
            board.setString_forType(&NSString::from_str(text), ty);
        }

        pub fn read() -> Option<String> {
            // SAFETY: as above.
            let ty = unsafe { NSPasteboardTypeString };
            NSPasteboard::generalPasteboard().stringForType(ty).map(|s| s.to_string())
        }
    }

    #[cfg(target_arch = "wasm32")]
    mod imp {
        use std::cell::RefCell;

        #[derive(Default)]
        struct Page {
            /// What a read answers: the last paste the shell saw, or the
            /// page's own last copy, whichever came later.
            text: Option<String>,
            /// A copy not yet handed to a `copy` / `cut` event.
            copied: Option<String>,
        }

        thread_local! {
            static PAGE: RefCell<Page> = RefCell::new(Page::default());
        }

        pub fn copy(text: &str) {
            PAGE.with(|p| {
                let mut p = p.borrow_mut();
                p.text = Some(text.to_string());
                p.copied = Some(text.to_string());
            });
            write_text(text);
        }

        pub fn read() -> Option<String> {
            PAGE.with(|p| p.borrow().text.clone())
        }

        /// Write through the async Clipboard API, if the page has one: it is
        /// undefined outside a secure context, and calling into undefined
        /// would throw through the wasm frames. Its promise is awaited only
        /// to keep a refusal (no user activation, no focus) off the console
        /// as an unhandled rejection; the `copy` event path covers it.
        fn write_text(text: &str) {
            let Some(nav) = web_sys::window().map(|w| w.navigator()) else { return };
            let has = js_sys::Reflect::get(&nav, &"clipboard".into()).is_ok_and(|c| !c.is_undefined() && !c.is_null());
            if !has {
                return;
            }
            let promise = nav.clipboard().write_text(text);
            wasm_bindgen_futures::spawn_local(async move {
                let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
            });
        }

        /// The copy a `copy` / `cut` event should carry, taken.
        pub(crate) fn take_copied() -> Option<String> {
            PAGE.with(|p| p.borrow_mut().copied.take())
        }

        /// A `paste` event's text: what reads answer from now on.
        pub(crate) fn pasted(text: String) {
            PAGE.with(|p| p.borrow_mut().text = Some(text));
        }
    }

    /// The browser shell's half: the clipboard events it answers.
    #[cfg(target_arch = "wasm32")]
    pub(crate) use imp::{pasted, take_copied};
}

pub mod context_menu {
    use crate::widget::*;
    use std::cell::RefCell;

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

    /// The mark a row that leads to a PAGE carries at its right end. Drawn
    /// as the `chevron-right` glyph; the character is what a host that lays
    /// out its own rows (the designer's dialog) may reserve room by.
    pub const PAGE_MARK: &str = "›";
    /// What a page's back band leads with, before the title of the plate it
    /// goes back to. Drawn as the `chevron-left` glyph.
    pub const BACK_MARK: &str = "‹";

    /// A row whose label BEGINS with one of these wears the mark as a
    /// cce-icons glyph at its left, and the label is drawn without it: a
    /// checked item (`check`), a switch or radio that is on (`circle`), one
    /// that is off (`circle-outline`). The text stays the row's identity —
    /// a host matching its own labels still matches `"✓ Show Grid"` — and
    /// the toolkit owns how a mark looks, so no menu draws one as a
    /// character in whatever face its font falls back to.
    pub const MARK_CHECK: &str = "✓ ";
    /// See [`MARK_CHECK`]: a switch or radio row that is on.
    pub const MARK_ON: &str = "● ";
    /// See [`MARK_CHECK`]: a switch or radio row that is off.
    pub const MARK_OFF: &str = "○ ";

    /// The glyph a label's leading mark names, and the label without it.
    /// A row's action read off its English label: the fallback for a menu built without
    /// [`set_row_actions`], as every menu was until 2026-10-08. It breaks the moment a label
    /// is translated (`docs/rfc-accessibility-locale.md`), so the toolkit's own menus set
    /// their actions and this serves only menus that do not.
    pub fn legacy_action_for_label(label: &str) -> Option<crate::widget::ContextAction> {
        use crate::widget::ContextAction as CA;
        Some(match label {
            "Cut" => CA::Cut,
            "Copy" => CA::Copy,
            "Paste" => CA::Paste,
            "Select All" => CA::SelectAll,
            "Undo" => CA::Undo,
            "Redo" => CA::Redo,
            "Clear" => CA::ClearText,
            "Copy Key" => CA::CopyKey,
            "Copy Value" => CA::CopyValue,
            "Delete" => CA::DeleteKey,
            "Expand" => CA::ExpandNode,
            "Collapse" => CA::CollapseNode,
            "Expand All" => CA::ExpandAll,
            "Collapse All" => CA::CollapseAll,
            "Copy Path" => CA::CopyPath,
            // The Ramp toggle carries its check state in the label.
            "✓ Collapse controls" | "Collapse controls" => CA::ToggleRampControls,
            _ => return None,
        })
    }

    pub fn split_mark(label: &str) -> (Option<&'static str>, &str) {
        for (mark, glyph) in [(MARK_CHECK, "check"), (MARK_ON, "circle"), (MARK_OFF, "circle-outline")] {
            if let Some(rest) = label.strip_prefix(mark) {
                return (Some(glyph), rest);
            }
        }
        (None, label)
    }

    /// How wide a row mark is drawn, at menu font size `size`, and the gap
    /// after it: the glyph box at the font size, so the mark (a disc fills
    /// three quarters of its box) stands about as tall as a capital.
    fn mark_size(size: f32) -> f32 {
        (size * 0.95).round()
    }
    const MARK_GAP: f32 = 6.0;
    /// The page and back chevrons, smaller still: they point, they do not
    /// label.
    fn chevron_size(size: f32) -> f32 {
        (size * 0.8).round()
    }

    /// A glyph a menu draws: name, top-left, side, colour.
    #[derive(Debug, Clone)]
    pub(crate) struct MenuGlyph {
        pub(crate) name: &'static str,
        x: f32,
        y: f32,
        side: f32,
        color: [f32; 4],
    }

    /// Draw `g` (shifted by `dx`, at `alpha`): the glyph tinted to the
    /// label colour beside it — `PaintCtx::icon`.
    fn paint_glyph(ctx: &mut crate::scene::paint::PaintCtx, g: &MenuGlyph, dx: f32, alpha: f32) {
        let r = crate::scene::layout::Rect { x: g.x + dx, y: g.y, width: g.side, height: g.side };
        let [cr, cg, cb, ca] = g.color;
        ctx.icon(g.name, r, [cr, cg, cb, ca * alpha]);
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

    /// A page turn in progress — see [`TURN_MS`].
    #[derive(Debug, Clone)]
    struct Turning {
        /// The plate as it stood when the turn began: its size and its rows.
        from: Box<ContextMenuState>,
        start: web_time::Instant,
        /// +1 forward (the page comes in from the right, where `›` points),
        /// -1 back.
        dir: f32,
    }

    /// The ease of a turn at `t` in 0..=1: fast out of the old plate,
    /// settling into the new one.
    pub fn turn_ease(t: f32) -> f32 {
        let u = 1.0 - t.clamp(0.0, 1.0);
        1.0 - u * u * u
    }

    /// How strongly the rows a turn leaves and the rows it brings are drawn
    /// at eased progress `e`: squared, so the two are seldom both legible at
    /// once — the old ones mostly gone before the new ones mostly come.
    fn turn_fades(e: f32) -> (f32, f32) {
        ((1.0 - e) * (1.0 - e), e * e)
    }

    /// A toolkit color as the `[u8; 3]` a [`TextLabel`] carries.
    fn rgb8(c: [f32; 4]) -> [u8; 3] {
        [
            (c[0] * 255.0).round().clamp(0.0, 255.0) as u8,
            (c[1] * 255.0).round().clamp(0.0, 255.0) as u8,
            (c[2] * 255.0).round().clamp(0.0, 255.0) as u8,
        ]
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
                            crate::backend::text::shaped_cluster_offsets(&mut fs, s, size, Some(&family))
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

        /// Show `options` as a PAGE, the plate's top-left at `(x, y)` — where
        /// the plate it turns from stood — with a back band reading `‹ back`
        /// across its top when `back` names that plate. The rows are new
        /// rows (sliders and page rows are set again after, as after
        /// [`show`](Self::show)); what changes is that a placement keeps the
        /// corner rather than opening from a pointer.
        ///
        /// The turn is ANIMATED from the plate that stood there — up, or
        /// hidden in the same moment (`TURN_HANDOFF`) — forward for a page
        /// with a back band, back for one without (a menu returned to).
        pub fn show_page(&mut self, x: f32, y: f32, back: Option<&str>, options: Vec<String>, header_count: usize, target: WidgetId) {
            const TURN_HANDOFF: std::time::Duration = std::time::Duration::from_millis(100);
            let from = (self.visible || self.hidden_at.is_some_and(|t| t.elapsed() < TURN_HANDOFF)).then(|| {
                let mut from = self.clone();
                from.turning = None;
                from.hovered_item = None;
                from.back_hovered = false;
                Box::new(from)
            });
            self.show(x, y, options, header_count, target);
            self.turned = true;
            self.turning = from.map(|from| Turning {
                from,
                start: web_time::Instant::now(),
                dir: if back.is_some() { 1.0 } else { -1.0 },
            });
            if let Some(title) = back {
                let (family, size) = label_font();
                let need = PAD + chevron_size(size) + MARK_GAP + crate::widget::display::measure_text_width(title, &family, size) + PAD;
                self.w = self.w.max(need);
                self.back = Some(title.to_string());
                self.content_h += ROW_H;
                self.h = self.content_h;
            }
        }

        /// New labels and slider values for the rows that are showing, in
        /// place — the hover, the scroll, the back band, the page rows and a
        /// held slider are kept — which is how a host re-marks a row of a
        /// menu that stays up after the row ran. `false`, and nothing
        /// changed, when the number of rows differs: that is another menu,
        /// to be shown afresh.
        pub fn refill(&mut self, options: Vec<String>, sliders: &[Option<MenuSlider>]) -> bool {
            if options.len() != self.options.len() {
                return false;
            }
            self.options = options;
            for i in 0..self.options.len() {
                // A held slider is the pointer's until the release.
                if self.slider_drag == Some(i) {
                    continue;
                }
                self.sliders[i] = sliders.get(i).copied().flatten();
            }
            true
        }

        /// Animate the shown menu as turning from a plate of `w` x `h` at its
        /// corner that had no rows of its own to show going — what a host
        /// turning back from a plate the menu does not draw (a dialog) asks
        /// for. `forward` as for [`Self::show_page`].
        pub fn turn_from_size(&mut self, w: f32, h: f32, forward: bool) {
            let mut from = self.clone();
            from.turning = None;
            from.options.clear();
            from.sliders.clear();
            from.pages.clear();
            from.back = None;
            from.hovered_item = None;
            from.w = w;
            from.h = h;
            self.turning = Some(Turning { from: Box::new(from), start: web_time::Instant::now(), dir: if forward { 1.0 } else { -1.0 } });
        }

        /// How far the turn in progress has gone, eased, 0..1; `None` when
        /// there is none or it has finished.
        pub fn turn_progress(&self) -> Option<f32> {
            let t = self.turning.as_ref()?;
            let raw = t.start.elapsed().as_secs_f32() * 1000.0 / turn_ms();
            (raw < 1.0).then(|| turn_ease(raw))
        }

        /// The rect the plate is drawn at: its own, or on its way there from
        /// the one it turned from.
        pub fn drawn_rect(&self) -> crate::scene::layout::Rect {
            let mut r = crate::scene::layout::Rect { x: self.x, y: self.y, width: self.w, height: self.h };
            if let (Some(e), Some(t)) = (self.turn_progress(), self.turning.as_ref()) {
                r.width = t.from.w + (self.w - t.from.w) * e;
                r.height = t.from.h + (self.h - t.from.h) * e;
            }
            r
        }

        /// The size a host surface has to give the plate: its own, or while
        /// it turns, the larger of the two it turns between.
        pub fn surface_size(&self) -> (f32, f32) {
            match (self.turn_progress(), self.turning.as_ref()) {
                (Some(_), Some(t)) => (self.w.max(t.from.w), self.content_h.max(t.from.h)),
                _ => (self.w, self.content_h),
            }
        }

        /// The band's height above the rows: one row on a page that goes
        /// back somewhere, none otherwise.
        fn band_h(&self) -> f32 {
            if self.back.is_some() { ROW_H } else { 0.0 }
        }

        /// The top of the back band, where it is drawn.
        pub fn back_band_y(&self) -> f32 {
            self.y + PAD - self.scroll
        }

        /// Whether `(px, py)` is on the back band.
        pub fn on_back_band(&self, px: f32, py: f32) -> bool {
            if self.back.is_none() || !self.hit_test(px, py) {
                return false;
            }
            let top = self.back_band_y();
            py >= top && py < top + ROW_H
        }

        /// The turn a press at `(px, py)` asks for: the back band goes back,
        /// a page row goes into its page.
        pub fn turn_at(&self, px: f32, py: f32) -> Option<PageTurn> {
            if !self.visible {
                return None;
            }
            if self.on_back_band(px, py) {
                return Some(PageTurn::Back);
            }
            self.row_at(px, py).filter(|&i| self.leads_to_page(i)).map(PageTurn::Into)
        }

        /// The turn a side swipe asked for since the last call.
        pub fn take_turn(&mut self) -> Option<PageTurn> {
            self.turn.take()
        }

        /// Make row `idx` a slider. Widens the plate to hold the label, the
        /// readout at its widest (both ends of the range) and the band.
        pub fn set_row_slider(&mut self, idx: usize, slider: MenuSlider) {
            if idx >= self.options.len() {
                return;
            }
            self.sliders[idx] = Some(slider);
            let (family, size) = label_font();
            let measure = |t: &str| crate::widget::display::measure_text_width(t, &family, size);
            let readout_w = [slider.min, slider.max]
                .iter()
                .map(|&v| measure(&MenuSlider { value: v, ..slider }.readout()))
                .fold(0.0f32, f32::max);
            let need = PAD + measure(&self.options[idx]) + SLIDER_GAP + readout_w + SLIDER_GAP + SLIDER_W + PAD;
            self.w = self.w.max(need);
        }

        /// Put the menu at `(x, y)`, shown at most `max_h` tall: the rows
        /// scroll when that cuts them off. Never shorter than one row, so a
        /// placement that leaves no room still shows something to scroll.
        /// Where the popup's configure lands the menu, and the in-window
        /// fallback's [`constrain_to`](Self::constrain_to).
        pub fn place(&mut self, x: f32, y: f32, max_h: f32) {
            self.x = x;
            self.y = y;
            let floor = self.content_h.min(ROW_H + 2.0 * PAD);
            self.h = self.content_h.min(max_h).max(floor);
            self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        }

        /// Keep the menu inside `(bx, by, bw, bh)` the way an xdg positioner
        /// with flip-y, slide-x, slide-y and resize-y does, from the anchor
        /// `show` was given: it opens down and right; if it does not fit
        /// below, it flips to open UP from the anchor; if it fits neither
        /// way it slides to the bottom edge, and if it is taller than the
        /// whole box it is cut to the box and scrolls. Recomputed from the
        /// anchor every call, so it can run every frame. For hosts with no
        /// popup surface (a layer surface, or the popup disabled) — there the
        /// window is the only room there is.
        pub fn constrain_to(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
            let (ax, ay) = self.anchor;
            let x = if ax + self.w > bx + bw { (bx + bw - self.w).max(bx) } else { ax.max(bx) };
            let (y, max_h) = if ay + self.content_h <= by + bh {
                (ay.max(by), self.content_h)
            } else if !self.turned && ay - self.content_h >= by {
                (ay - self.content_h, self.content_h)
            } else {
                ((by + bh - self.content_h).max(by), bh)
            };
            self.place(x, y, max_h);
        }

        /// How far the rows can scroll: zero when the menu shows them all.
        pub fn max_scroll(&self) -> f32 {
            (self.content_h - self.h).max(0.0)
        }

        /// Whether a press on row `idx` could run it: not a header, not a
        /// separator.
        fn actionable(&self, idx: usize) -> bool {
            idx >= self.header_count && self.options.get(idx).is_some_and(|o| o != "-")
        }

        /// Highlight row `idx` as the pointer would, scrolled into view —
        /// the keyboard's hover. `None`, or a row that cannot run, clears it.
        pub fn set_hovered_item(&mut self, idx: Option<usize>) {
            self.hovered_item = idx.filter(|&i| self.actionable(i));
            if let Some(i) = self.hovered_item {
                self.scroll_into_view(i);
            }
        }

        /// Move the highlight to the next row that can run, `dir` > 0 down
        /// and < 0 up, skipping headers and separators; from no highlight,
        /// the first (or last) such row. Stops at either end rather than
        /// wrapping. Returns the highlighted row.
        pub fn step_hovered(&mut self, dir: i32) -> Option<usize> {
            let n = self.options.len() as i32;
            let step = if dir < 0 { -1 } else { 1 };
            let mut i = match self.hovered_item {
                Some(h) => h as i32,
                None if step > 0 => -1,
                None => n,
            };
            loop {
                i += step;
                if i < 0 || i >= n {
                    return self.hovered_item;
                }
                if self.actionable(i as usize) {
                    self.set_hovered_item(Some(i as usize));
                    return self.hovered_item;
                }
            }
        }

        /// Scroll so row `idx` is wholly inside the plate. Unlike
        /// [`Self::scroll_by`] this does not re-hover the row under the
        /// pointer: the keyboard put the highlight where it is.
        fn scroll_into_view(&mut self, idx: usize) {
            let top = self.band_h() + idx as f32 * ROW_H;
            let bottom = top + ROW_H + 2.0 * PAD;
            if top < self.scroll {
                self.scroll = top;
            } else if bottom > self.scroll + self.h {
                self.scroll = bottom - self.h;
            }
            self.scroll = self.scroll.clamp(0.0, self.max_scroll());
        }

        /// Scroll the rows by `dy` px (positive shows rows further down),
        /// clamped; re-hovers whatever row the pointer now sits on. `true`
        /// when anything moved.
        pub fn scroll_by(&mut self, dy: f32) -> bool {
            let next = (self.scroll + dy).clamp(0.0, self.max_scroll());
            if (next - self.scroll).abs() < f32::EPSILON {
                return false;
            }
            self.scroll = next;
            if let Some((px, py)) = self.last_cursor {
                self.rehover(px, py);
            }
            true
        }

        /// The slider on row `idx`, if it is one.
        pub fn slider(&self, idx: usize) -> Option<MenuSlider> {
            self.sliders.get(idx).copied().flatten()
        }

        /// The band row `idx`'s slider draws over and a press grabs.
        pub fn slider_band(&self, idx: usize) -> crate::scene::layout::Rect {
            crate::scene::layout::Rect {
                x: self.x + self.w - PAD - SLIDER_W,
                y: self.row_y(idx) + 3.0,
                width: SLIDER_W,
                height: ROW_H - 6.0,
            }
        }

        fn set_slider_value(&mut self, idx: usize, v: f32) -> bool {
            let Some(Some(s)) = self.sliders.get_mut(idx) else { return false };
            let v = s.clamp_snap(v);
            if (v - s.value).abs() < f32::EPSILON {
                return false;
            }
            s.value = v;
            self.slider_change = Some((idx, v));
            true
        }

        /// The wheel over a slider row steps it; anywhere else it does
        /// nothing and says so. Up is more, as on every slider in the DE.
        pub fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
            if !self.visible {
                return false;
            }
            // A side swipe turns a page: forward with the pointer on a page
            // row, back from anywhere on a page that has somewhere to go.
            if self.hit_test(px, py) && self.slider_drag.is_none() {
                let turn = match crate::widget::side_swipe::feed(delta) {
                    Some(crate::widget::SwipeDir::Forward) => {
                        self.row_at(px, py).filter(|&i| self.leads_to_page(i)).map(PageTurn::Into)
                    }
                    Some(crate::widget::SwipeDir::Back) => self.back.is_some().then_some(PageTurn::Back),
                    None => None,
                };
                if turn.is_some() {
                    self.turn = turn;
                    return true;
                }
            }
            let Some(idx) = self.row_at(px, py) else { return false };
            let Some(s) = self.slider(idx) else {
                // Not a slider: a menu cut down to fit scrolls its rows.
                // Up shows the rows above, as every list in the DE does.
                if self.max_scroll() <= 0.0 {
                    return false;
                }
                self.scroll_by(-delta.notches_y() * ROW_H);
                return true;
            };
            self.wheel_accum += delta.value_notches_y();
            let whole = self.wheel_accum.trunc();
            if whole == 0.0 {
                return false;
            }
            self.wheel_accum -= whole;
            let step = if s.step > 0.0 { s.step } else { (s.max - s.min) * 0.02 };
            self.set_slider_value(idx, s.value + whole * step)
        }

        /// A left press on a slider row: on the band it takes hold and jumps
        /// the value to the pointer; anywhere on the row it is the slider's
        /// and the menu stays open. `false` for any other row.
        pub fn slider_press(&mut self, px: f32, py: f32) -> bool {
            if !self.visible {
                return false;
            }
            let Some(idx) = self.row_at(px, py) else { return false };
            if self.slider(idx).is_none() {
                return false;
            }
            let band = self.slider_band(idx);
            if px >= band.x && px <= band.x + band.width {
                self.slider_drag = Some(idx);
                self.slider_drag_to(px);
            }
            true
        }

        /// Move the held slider to the pointer's place along its band —
        /// wherever the pointer is, so a drag that leaves the plate keeps
        /// working. `false` when nothing is held or nothing moved.
        pub fn slider_drag_to(&mut self, px: f32) -> bool {
            let Some(idx) = self.slider_drag else { return false };
            let Some(s) = self.slider(idx) else { return false };
            let band = self.slider_band(idx);
            let t = ((px - band.x) / band.width.max(1.0)).clamp(0.0, 1.0);
            self.set_slider_value(idx, s.min + t * (s.max - s.min))
        }

        /// End a slider drag; `true` if one was held.
        pub fn slider_release(&mut self) -> bool {
            self.slider_drag.take().is_some()
        }

        pub fn take_slider_change(&mut self) -> Option<(usize, f32)> {
            self.slider_change.take()
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

        pub fn hit_test(&self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
        }

        /// The top of row `idx`, where it is drawn: scrolled, so a row above
        /// the view lies above `y`.
        pub fn row_y(&self, idx: usize) -> f32 {
            self.y + PAD + self.band_h() + idx as f32 * ROW_H - self.scroll
        }

        /// The row under `(px, py)`, or `None` outside the plate or in its
        /// padding — the padding is plate, not a row, so a press there
        /// neither hovers nor fires row 0.
        pub fn row_at(&self, px: f32, py: f32) -> Option<usize> {
            if px < self.x || px > self.x + self.w || py < self.y || py > self.y + self.h {
                return None;
            }
            let rel = py - self.y - PAD - self.band_h() + self.scroll;
            if rel < 0.0 {
                return None;
            }
            let idx = (rel / ROW_H) as usize;
            (idx < self.options.len()).then_some(idx)
        }

        pub fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            self.last_cursor = Some((px, py));
            if self.slider_drag.is_some() {
                return self.slider_drag_to(px);
            }
            let was = (self.hovered_item, self.back_hovered);
            self.rehover(px, py);
            (self.hovered_item, self.back_hovered) != was
        }

        fn rehover(&mut self, px: f32, py: f32) -> bool {
            let was_hovered = (self.hovered_item, self.back_hovered);
            self.hovered_item = None;
            self.back_hovered = self.on_back_band(px, py);
            if let Some(idx) = self.row_at(px, py) {
                // A "-" row is a SEPARATOR (the dropdown's convention):
                // engraved, never hovered, never an action.
                if idx >= self.header_count && self.options[idx] != "-" {
                    self.hovered_item = Some(idx);
                }
            }
            (self.hovered_item, self.back_hovered) != was_hovered
        }

        pub fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: Option<&mut crate::context::UiContext>) -> bool {
            if !self.visible { return false; }
            if button == MouseButton::Left && state == ElementState::Released && self.slider_release() {
                return true;
            }
            if button == MouseButton::Left && state == ElementState::Pressed && self.slider_press(px, py) {
                return true;
            }
            if button != MouseButton::Left || state != ElementState::Pressed {
                if state == ElementState::Pressed {
                    self.hide();
                    return true;
                }
                return false;
            }

            if let Some(turn) = self.turn_at(px, py) {
                self.turn = Some(turn);
                return true;
            }
            if self.hit_test(px, py) {
                if let Some(idx) = self.row_at(px, py) {
                    if idx >= self.header_count {
                        let opt = self.options[idx].clone();
                        if let (Some(target_id), Some(ctx)) = (self.target, ctx) {
                            if let Some(target_ptr) = ctx.tree.get_ptr(target_id) {
                                unsafe {
                                    let target = &mut *target_ptr;
                                    let action = self.actions.get(idx).copied().flatten().or_else(|| legacy_action_for_label(&opt));
                                    if let Some(action) = action {
                                        let _ = target.context_action(action);
                                    }
                                }
                            }
                        }
                    }
                }
                self.hide();
                true
            } else {
                self.hide();
                true
            }
        }

        /// Paint the menu as a lit plate: a rounded face in the DE's plate color,
        /// translucent and frosted (the negative-alpha blur-behind sentinel), with
        /// the rolled perimeter — the material every other floating surface in the
        /// DE wears. Hosts on the display-list path call this INSTEAD of iterating
        /// [`ContextMenuState::extra_quads`], then draw [`text_labels`] over it.
        ///
        /// A transparent configured plate color degrades to the edges-only boss,
        /// as the breadcrumb's raised run does: with no face to tint, a plate
        /// would paint a hole.
        ///
        /// [`text_labels`]: ContextMenuState::text_labels
        pub fn paint(&self, ctx: &mut crate::scene::paint::PaintCtx) {
            if !self.visible || self.hosted {
                return;
            }
            let r = crate::layout::menu_corner_radius();
            if let (Some(e), Some(t)) = (self.turn_progress(), self.turning.as_ref()) {
                // Turning: the plate on its way between the two sizes; the
                // rows it turned from — sliders, separators — sliding away
                // and fading, the page's sliding in from the side the turn
                // comes from and coming up. Each set is drawn aside and
                // replayed moved and faded (`Prim::faded`); their labels
                // are `paint_with_labels`', at the same strengths.
                let rect = self.drawn_rect();
                let depth = crate::layout::bevel_width().min(rect.height * 0.2);
                paint_menu_plate(ctx, rect, self.in_popup);
                let (out, into) = turn_fades(e);
                let (lo, hi) = (-t.dir * e * TURN_SLIDE, t.dir * (1.0 - e) * TURN_SLIDE);
                ctx.clip_rounded(rect, r, |ctx| {
                    for (rows, dx, alpha) in [(&*t.from, lo, out), (self, hi, into)] {
                        let mut aside = crate::scene::paint::PaintCtx::new();
                        rows.paint_rows(&mut aside, rect, r, depth);
                        ctx.translate(dx, 0.0, |ctx| {
                            for item in aside.finish().items {
                                if let Some(c) = item.clip {
                                    ctx.push_clip(c);
                                }
                                // The rows draw no text; a label would be
                                // doubled with `paint_with_labels`'.
                                let _ = ctx.replay(item.prim.faded(alpha));
                                if item.clip.is_some() {
                                    ctx.pop_clip();
                                }
                            }
                        });
                    }
                });
                return;
            }
            let rect = crate::scene::layout::Rect {
                x: self.x,
                y: self.y,
                width: self.w,
                height: self.h,
            };
            let depth = crate::layout::bevel_width().min(self.h * 0.2);
            paint_menu_plate(ctx, rect, self.in_popup);

            // What the rows draw — hover, separators, slider bands — is cut at
            // the plate, so a row scrolled half out of a shortened menu stops
            // at its edge instead of hanging off it.
            ctx.clip_rounded(rect, r, |ctx| self.paint_rows(ctx, rect, r, depth));
            if self.max_scroll() > 0.0 {
                // A scrolled menu says so: a thumb in the right padding, as
                // long against the plate as the view is against the rows.
                let track = (rect.y + PAD, rect.height - 2.0 * PAD);
                let len = (track.1 * self.h / self.content_h).max(12.0).min(track.1);
                let at = track.0 + (track.1 - len) * (self.scroll / self.max_scroll());
                let tw = 3.0;
                let c = crate::color::TEXT_DIM;
                ctx.rounded_rect(
                    crate::scene::layout::Rect { x: rect.x + rect.width - PAD * 0.5 - tw * 0.5, y: at, width: tw, height: len },
                    tw * 0.5,
                    (true, true, true, true),
                    [c[0], c[1], c[2], 0.6],
                );
            }
        }

        fn paint_rows(&self, ctx: &mut crate::scene::paint::PaintCtx, rect: crate::scene::layout::Rect, r: f32, depth: f32) {
            if self.back.is_some() {
                // The back band: lit like a row under the pointer, and cut off
                // from the page's rows by the separator's groove.
                let top = self.back_band_y();
                if self.back_hovered {
                    let inset = (depth * 0.5).max(2.0).max(PAD * 0.5);
                    ctx.rounded_rect(
                        crate::scene::layout::Rect { x: self.x + inset, y: top + 2.0, width: self.w - 2.0 * inset, height: ROW_H - 4.0 },
                        (r - inset).max(0.0),
                        (true, true, true, true),
                        [0.20, 0.40, 0.65, 0.6],
                    );
                }
                let inset = (depth * 0.5).max(PAD);
                let cy = top + ROW_H;
                ctx.groove((self.x + inset, cy), (self.x + self.w - inset, cy), 0.75, depth, rect);
            }
            if let Some(h_idx) = self.hovered_item {
                // Inset off the roll so the fill sits on the face instead of
                // climbing the lit edge, and round the corners it actually meets:
                // the first and last rows touch the plate's, and a header row is
                // never hovered, so the top pair only rounds when there is no
                // header above.
                // Inside the padding on every side — the rows no longer
                // touch the plate's edge, so the fill is its own rounded
                // tablet on the face rather than a band that meets the roll.
                let iy = self.row_y(h_idx);
                let inset = (depth * 0.5).max(2.0).max(PAD * 0.5);
                ctx.rounded_rect(
                    crate::scene::layout::Rect {
                        x: self.x + inset,
                        y: iy + 2.0,
                        width: self.w - 2.0 * inset,
                        height: ROW_H - 4.0,
                    },
                    (r - inset).max(0.0),
                    (true, true, true, true),
                    [0.20, 0.40, 0.65, 0.6],
                );
            }

            // Separator rows ("-"): an engraved line across the face at the
            // row's vertical centre — the breadcrumb seam's language, cut
            // into the menu plate instead of a printed dash.
            for (idx, opt) in self.options.iter().enumerate() {
                if opt == "-" {
                    let cy = self.row_y(idx) + ROW_H * 0.5;
                    let inset = (depth * 0.5).max(PAD);
                    ctx.groove(
                        (self.x + inset, cy),
                        (self.x + self.w - inset, cy),
                        0.75,
                        depth,
                        rect,
                    );
                }
            }
            self.paint_sliders(ctx);
        }

        /// The slider rows' bands — the toolkit's own `Slider`, one stamp set
        /// to each row's range and value, so a slider in a menu is the slider
        /// everywhere else. Its readout is off: the menu draws the readout as
        /// a label, so it wears the menu font and clears the popover clamp.
        fn paint_sliders(&self, ctx: &mut crate::scene::paint::PaintCtx) {
            for idx in 0..self.options.len() {
                let Some(s) = self.slider(idx) else { continue };
                let mut stamp = Slider::new().with_readout(false);
                stamp.set_scroll(false);
                stamp.set_range(s.min, s.max);
                stamp.set_scaled_value(s.value);
                crate::widget::model::Paint::paint(&*stamp, self.slider_band(idx), ctx);
            }
        }

        /// The flat-quad menu: a 1px border rect, a near-black fill and the hover
        /// row. Superseded by [`ContextMenuState::paint`], which draws the menu as
        /// the lit plate the rest of the DE's floating surfaces wear; this stays
        /// for hosts that have not migrated, and renders as it always has.
        pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
            let mut quads = Vec::new();
            if !self.visible || self.hosted { return quads; }

            // border
            quads.push((self.x, self.y, self.w, self.h, [0.22, 0.22, 0.28, 1.0]));
            // bg
            quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, [0.06, 0.06, 0.09, 1.0]));

            if let Some(h_idx) = self.hovered_item {
                let iy = self.row_y(h_idx);
                quads.push((self.x + PAD * 0.5, iy + 2.0, self.w - PAD, ROW_H - 4.0, [0.20, 0.40, 0.65, 0.6]));
            }

            // Separator rows ("-"): a hairline in place of the engraved
            // groove the plate path cuts.
            for (idx, opt) in self.options.iter().enumerate() {
                if opt == "-" {
                    let cy = self.row_y(idx) + ROW_H * 0.5;
                    quads.push((self.x + PAD, cy, self.w - 2.0 * PAD, 1.0, [0.22, 0.22, 0.28, 1.0]));
                }
            }

            quads
        }

        /// [`paint`](Self::paint) plus the label run, in the menu font.
        ///
        /// A [`TextLabel`] carries a size but no family, so a consumer that
        /// hand-rolls `paint()` + a `text_labels()` loop has to remember to
        /// pass [`label_font`]'s family itself — and every one of them passed
        /// `None`, which is why menus rendered in the default sans over lists
        /// wearing the configured face. This is the call that cannot forget
        /// it; prefer it over the pair.
        pub fn paint_with_labels(&self, ctx: &mut crate::scene::paint::PaintCtx) {
            self.paint(ctx);
            if !self.visible || self.hosted {
                return;
            }
            let (family, _) = label_font();
            if let (Some(e), Some(t)) = (self.turn_progress(), self.turning.as_ref()) {
                // The labels of both, cut at the plate as it is drawn: the
                // ones it turned from sliding away and fading, the page's
                // sliding in and coming up.
                let rect = self.drawn_rect();
                let bounds = Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]);
                let (lo, hi) = (-t.dir * e * TURN_SLIDE, t.dir * (1.0 - e) * TURN_SLIDE);
                let (out, into) = turn_fades(e);
                for (rows, dx, alpha) in [(&*t.from, lo, out), (self, hi, into)] {
                    for label in rows.labels() {
                        ctx.text_faded(label.text, label.x + dx, label.y, label.font_size, label.color, alpha, Some(family.clone()), bounds);
                    }
                    ctx.clip(rect, |ctx| {
                        for g in rows.glyphs() {
                            paint_glyph(ctx, &g, dx, alpha);
                        }
                    });
                }
                return;
            }
            // The menu's own rect: the engine's popover clamp exempts exactly
            // these bounds, so the labels render inside the plate instead of
            // being clipped to the page content beneath it.
            let bounds = Some([self.x, self.y, self.x + self.w, self.y + self.h]);
            for label in self.text_labels() {
                ctx.text_with(
                    label.text,
                    label.x,
                    label.y,
                    label.font_size,
                    label.color,
                    Some(family.clone()),
                    bounds,
                );
            }
            let plate = crate::scene::layout::Rect { x: self.x, y: self.y, width: self.w, height: self.h };
            ctx.clip(plate, |ctx| {
                for g in self.glyphs() {
                    paint_glyph(ctx, &g, 0.0, 1.0);
                }
            });
        }

        pub fn text_labels(&self) -> Vec<TextLabel> {
            if !self.visible || self.hosted {
                return Vec::new();
            }
            self.labels()
        }

        /// The labels as the rows stand, shown or not.
        pub(crate) fn labels(&self) -> Vec<TextLabel> {
            let mut labels = Vec::new();

            if let Some(title) = &self.back {
                let (_, label_size) = label_font();
                labels.push(TextLabel {
                    text: title.clone(),
                    x: self.x + PAD + chevron_size(label_size) + MARK_GAP,
                    y: self.back_band_y() + (ROW_H - label_size) / 2.0,
                    font_size: label_size,
                    color: rgb8(if self.back_hovered { crate::color::TEXT_HEADER } else { crate::color::TEXT_DIM }),
                });
            }
            for (idx, opt) in self.options.iter().enumerate() {
                if opt == "-" {
                    continue;
                }
                // Scrolled wholly out of a shortened menu: nothing to draw.
                // A row partly in view is drawn and cut at the plate by the
                // label's bounds.
                let top = self.row_y(idx);
                if top + ROW_H < self.y || top > self.y + self.h {
                    continue;
                }
                let (_, label_size) = label_font();
                let iy = self.row_y(idx) + (ROW_H - label_size) / 2.0;
                // The toolkit's semantic colors rather than greys hand-mixed
                // against the old near-black fill: on the plate's mid-slate the
                // header's 0x70 was a step above its background and read as
                // nothing.
                let text_color = if idx < self.header_count {
                    rgb8(crate::color::TEXT_DIM)
                } else if self.hovered_item == Some(idx) {
                    rgb8(crate::color::TEXT_HEADER)
                } else {
                    rgb8(crate::color::TEXT_FG)
                };

                // A leading mark is drawn as a glyph (see `glyphs`); the
                // label follows it.
                let (mark, text) = split_mark(opt);
                let mark_w = if mark.is_some() { mark_size(label_size) + MARK_GAP } else { 0.0 };
                labels.push(TextLabel {
                    text: text.to_string(),
                    x: self.x + PAD + mark_w,
                    y: iy,
                    font_size: label_size,
                    color: text_color,
                });
                // A slider row's readout, right-aligned against its band.
                if let Some(s) = self.slider(idx) {
                    let (family, _) = label_font();
                    let text = s.readout();
                    let tw = crate::widget::display::measure_text_width(&text, &family, label_size);
                    labels.push(TextLabel {
                        text,
                        x: self.slider_band(idx).x - SLIDER_GAP - tw,
                        y: iy,
                        font_size: label_size,
                        color: text_color,
                    });
                }
            }
            labels
        }

        /// The glyphs the rows wear, as they stand: each row's leading mark
        /// (see [`MARK_CHECK`]), the chevron at the right end of a row that
        /// leads to a page, and the back band's chevron. Each takes its
        /// row's text colour.
        pub(crate) fn glyphs(&self) -> Vec<MenuGlyph> {
            let mut glyphs = Vec::new();
            let (_, size) = label_font();
            let (ms, cs) = (mark_size(size), chevron_size(size));
            if self.back.is_some() {
                glyphs.push(MenuGlyph {
                    name: "chevron-left",
                    x: self.x + PAD,
                    y: self.back_band_y() + (ROW_H - cs) / 2.0,
                    side: cs,
                    color: if self.back_hovered { crate::color::TEXT_HEADER } else { crate::color::TEXT_DIM },
                });
            }
            for (idx, opt) in self.options.iter().enumerate() {
                if opt == "-" {
                    continue;
                }
                let top = self.row_y(idx);
                if top + ROW_H < self.y || top > self.y + self.h {
                    continue;
                }
                let color = if idx < self.header_count {
                    crate::color::TEXT_DIM
                } else if self.hovered_item == Some(idx) {
                    crate::color::TEXT_HEADER
                } else {
                    crate::color::TEXT_FG
                };
                if let (Some(name), _) = split_mark(opt) {
                    glyphs.push(MenuGlyph { name, x: self.x + PAD, y: top + (ROW_H - ms) / 2.0, side: ms, color });
                }
                if self.leads_to_page(idx) {
                    glyphs.push(MenuGlyph {
                        name: "chevron-right",
                        x: self.x + self.w - PAD - cs,
                        y: top + (ROW_H - cs) / 2.0,
                        side: cs,
                        color,
                    });
                }
            }
            glyphs
        }
    }

    /// The menu PLATE alone, over `rect`: [`Material::menu`] on a rounded
    /// face at `style.surface.menu.corner_radius` with the rolled perimeter
    /// at the relief width (capped at a fifth of the height) — or, for a
    /// transparent configured face, the edges-only boss. What
    /// [`ContextMenuState::paint`] draws under its rows, and what any other
    /// surface that should look like a menu draws under its own (the
    /// designer's command palette): one function, so the two cannot be
    /// configured apart.
    ///
    /// `in_popup` says the plate is being painted into the runner's popup
    /// surface, where the compositor's blur frosts but cannot COMPRESS: the
    /// in-app pass pulls the backdrop's luminance a fraction `k` toward the
    /// plate's key, which is what keeps the labels legible over a bright
    /// scene. Over glass that only blurs, the same swing is held with
    /// opacity instead — the backdrop reaches the eye at (1 - a)(1 - k)
    /// either way — or a menu opened over something white washes out.
    ///
    /// [`Material::menu`]: crate::scene::material::Material::menu
    pub fn paint_menu_plate(ctx: &mut crate::scene::paint::PaintCtx, rect: crate::scene::layout::Rect, in_popup: bool) {
        let r = crate::layout::menu_corner_radius();
        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
        let face = crate::color::menu_color();
        if face[3] > 0.001 {
            let material = crate::scene::material::Material::menu();
            let material = if in_popup {
                let k = crate::color::menu_compression().clamp(0.0, 1.0);
                let mut m = material.for_role(crate::scene::material::PlateRole::Root);
                m.tint[3] = 1.0 - (1.0 - m.tint[3]) * (1.0 - k);
                m
            } else {
                material
            };
            ctx.plate(rect, (r, r, r, r), &material, depth);
        } else {
            let (plateau, radii) = crate::layout::carve_inside(rect, (r, r, r, r), depth);
            ctx.boss(plateau, radii, depth);
        }
    }

    /// Run `f` on the current window's menu (`crate::window_state`): the one menu every
    /// function here acts on. It was a thread-local, `CONTEXT_MENU`, until 2026-10-08;
    /// `with_state(..)` callers read it through this now.
    pub fn with_state<R>(f: impl FnOnce(&RefCell<ContextMenuState>) -> R) -> R {
        crate::window_state::with(|w| f(&w.context_menu))
    }

    /// Make row `idx` of the shown menu lead to a page — see [`PageTurn`].
    /// Call after [`show`] / [`show_page`], which clear every row back to an
    /// action.
    /// Give the open menu's rows their actions — see [`ContextMenuState::set_row_actions`].
    pub fn set_row_actions(actions: Vec<Option<crate::widget::ContextAction>>) {
        with_state(|m| m.borrow_mut().set_row_actions(actions));
    }

    /// The action row `idx` of the open menu runs, if it was given one.
    pub fn row_action(idx: usize) -> Option<crate::widget::ContextAction> {
        with_state(|m| m.borrow().actions.get(idx).copied().flatten())
    }

    /// How many of the open menu's first rows are headers (a title, `File:` / `Key:`).
    pub fn header_count() -> usize {
        with_state(|m| m.borrow().header_count)
    }

    pub fn set_row_page(idx: usize) {
        with_state(|m| m.borrow_mut().set_row_page(idx));
    }
    pub fn leads_to_page(idx: usize) -> bool {
        with_state(|m| m.borrow().leads_to_page(idx))
    }
    /// See [`ContextMenuState::show_page`].
    pub fn show_page(x: f32, y: f32, back: Option<&str>, options: Vec<String>, header_count: usize, target: WidgetId) {
        with_state(|m| m.borrow_mut().show_page(x, y, back, options, header_count, target));
    }
    /// See [`ContextMenuState::refill`].
    pub fn refill(options: Vec<String>, sliders: &[Option<MenuSlider>]) -> bool {
        with_state(|m| m.borrow_mut().refill(options, sliders))
    }
    /// See [`ContextMenuState::turn_at`].
    pub fn turn_at(px: f32, py: f32) -> Option<PageTurn> {
        with_state(|m| m.borrow().turn_at(px, py))
    }
    /// See [`ContextMenuState::take_turn`].
    pub fn take_turn() -> Option<PageTurn> {
        with_state(|m| m.borrow_mut().take_turn())
    }
    /// Whether the shown menu is a page turned to in place of another plate.
    pub fn is_turned() -> bool {
        with_state(|m| m.borrow().turned)
    }
    /// The title of the plate the shown page goes back to.
    pub fn back_title() -> Option<String> {
        with_state(|m| m.borrow().back.clone())
    }

    pub fn is_visible() -> bool {
        with_state(|m| m.borrow().visible)
    }

    pub fn show(x: f32, y: f32, options: Vec<String>, header_count: usize, target: WidgetId) {
        with_state(|m| m.borrow_mut().show(x, y, options, header_count, target));
    }

    pub fn hide() {
        with_state(|m| m.borrow_mut().hide());
    }

    pub fn clear_if_matches(w: &dyn WidgetHost) {
        let id = w.base().id();
        with_state(|m| {
            // Already borrowed means the widget is being dropped from INSIDE
            // the menu's own code — the slider rows' paint stamp, dropped at
            // the end of `paint` under `paint_with_labels`' borrow. A widget
            // the menu made for itself cannot be its target, so there is
            // nothing to clear; `borrow_mut` here panicked on every paint of
            // a menu with a slider row.
            let Ok(mut menu) = m.try_borrow_mut() else { return };
            if menu.target == Some(id) {
                menu.hide();
            }
        });
    }

    /// Whether the runner draws the menu in its own popup surface — see
    /// [`ContextMenuState::hosted`]. Set by the runner, never by an app.
    pub fn set_hosted(hosted: bool) {
        with_state(|m| m.borrow_mut().hosted = hosted);
    }
    pub fn is_hosted() -> bool {
        with_state(|m| m.borrow().hosted)
    }
    /// See [`ContextMenuState::generation`].
    pub fn generation() -> u64 {
        with_state(|m| m.borrow().generation)
    }
    /// See [`ContextMenuState::place`].
    pub fn place(x: f32, y: f32, max_h: f32) {
        with_state(|m| m.borrow_mut().place(x, y, max_h));
    }
    /// See [`ContextMenuState::constrain_to`].
    pub fn constrain_to(bx: f32, by: f32, bw: f32, bh: f32) {
        with_state(|m| m.borrow_mut().constrain_to(bx, by, bw, bh));
    }
    /// `(anchor, w, content_h)` — what a popup positioner is built from.
    pub fn natural_geometry() -> ((f32, f32), f32, f32) {
        with_state(|m| {
            let m = m.borrow();
            let (w, h) = m.surface_size();
            (m.anchor, w, h)
        })
    }
    /// Whether a page turn is being animated: a host drawing the menu
    /// itself asks for frames while it is.
    pub fn is_turning() -> bool {
        with_state(|m| m.borrow().turn_progress().is_some())
    }
    /// See [`ContextMenuState::turn_from_size`].
    pub fn turn_from_size(w: f32, h: f32, forward: bool) {
        with_state(|m| m.borrow_mut().turn_from_size(w, h, forward));
    }
    /// Paint the menu with its top-left at the origin, whether or not it is
    /// [hosted](set_hosted) — the runner's popup surface draws it this way.
    /// Paints a COPY, so no borrow of the menu is held while the paint runs
    /// (the slider stamp's drop reaches back into this cell).
    pub fn paint_hosted(ctx: &mut crate::scene::paint::PaintCtx) {
        let mut menu = with_state(|m| m.borrow().clone());
        menu.hosted = false;
        menu.in_popup = true;
        let (x, y) = (menu.x, menu.y);
        ctx.translate(-x, -y, |ctx| menu.paint_with_labels(ctx));
    }

    pub fn x() -> f32 { with_state(|m| m.borrow().x) }
    pub fn y() -> f32 { with_state(|m| m.borrow().y) }
    pub fn w() -> f32 { with_state(|m| m.borrow().w) }
    pub fn h() -> f32 { with_state(|m| m.borrow().h) }
    pub fn hovered_item() -> Option<usize> { with_state(|m| m.borrow().hovered_item) }
    pub fn options() -> Vec<String> { with_state(|m| m.borrow().options.clone()) }
    /// Highlight a row from the keyboard — see
    /// [`ContextMenuState::set_hovered_item`]. A host that walks a menu
    /// with the arrow keys (a dropdown) sets the open row with this and
    /// moves with [`step_hovered`], and runs [`hovered_item`] on Enter.
    pub fn set_hovered_item(idx: Option<usize>) {
        with_state(|m| m.borrow_mut().set_hovered_item(idx));
    }
    /// See [`ContextMenuState::step_hovered`].
    pub fn step_hovered(dir: i32) -> Option<usize> {
        with_state(|m| m.borrow_mut().step_hovered(dir))
    }

    /// The row under a point, PAD-aware — the ONE row hit test. Every host
    /// that dispatches the menu itself should ask this rather than divide
    /// `(py - y()) / ROW_H`: the rows start `PAD` below the plate's top, so
    /// that division names the row below over the bottom third of every
    /// row, and runs off the end on the last one (2026-09-22 audit: six
    /// call sites across five apps had it).
    pub fn row_at(px: f32, py: f32) -> Option<usize> {
        with_state(|m| m.borrow().row_at(px, py))
    }
    /// A row's top, PAD-aware — for a host painting the rows itself.
    pub fn row_y(idx: usize) -> f32 {
        with_state(|m| m.borrow().row_y(idx))
    }
    pub fn hit_test(px: f32, py: f32) -> bool {
        with_state(|m| m.borrow().hit_test(px, py))
    }

    pub fn cursor_moved(px: f32, py: f32) -> bool {
        with_state(|m| m.borrow_mut().cursor_moved(px, py))
    }

    /// Make row `idx` of the shown menu a slider — see [`MenuSlider`]. Call
    /// after [`show`], which clears every row back to an action.
    pub fn set_row_slider(idx: usize, slider: MenuSlider) {
        with_state(|m| m.borrow_mut().set_row_slider(idx, slider));
    }
    pub fn slider(idx: usize) -> Option<MenuSlider> {
        with_state(|m| m.borrow().slider(idx))
    }
    /// The wheel, for hosts that route it: steps the slider under the
    /// pointer. `false` when no slider row is there — let it scroll the page.
    pub fn mouse_wheel(delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        with_state(|m| m.borrow_mut().mouse_wheel(delta, px, py))
    }
    /// A left press, for hosts that dispatch the menu themselves: `true` when
    /// it landed on a slider row, which the host must then NOT treat as an
    /// action or a dismissal.
    pub fn slider_press(px: f32, py: f32) -> bool {
        with_state(|m| m.borrow_mut().slider_press(px, py))
    }
    pub fn slider_dragging() -> bool {
        with_state(|m| m.borrow().slider_drag.is_some())
    }
    pub fn slider_release() -> bool {
        with_state(|m| m.borrow_mut().slider_release())
    }
    /// `(row, value)` of the last slider change since the last call.
    pub fn take_slider_change() -> Option<(usize, f32)> {
        with_state(|m| m.borrow_mut().take_slider_change())
    }

    pub fn mouse_input(button: MouseButton, state: ElementState, px: f32, py: f32, ctx: Option<&mut crate::context::UiContext>) -> bool {
        with_state(|m| m.borrow_mut().mouse_input(button, state, px, py, ctx))
    }

    /// Paint the menu as a lit plate — see [`ContextMenuState::paint`]. Hosts on
    /// the display-list path call this in place of the [`extra_quads`] loop.
    pub fn paint(ctx: &mut crate::scene::paint::PaintCtx) {
        with_state(|m| m.borrow().paint(ctx));
    }

    pub fn extra_quads() -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        with_state(|m| m.borrow().extra_quads())
    }

    pub fn text_labels() -> Vec<TextLabel> {
        with_state(|m| m.borrow().text_labels())
    }

    /// Plate and labels in one call — see
    /// [`ContextMenuState::paint_with_labels`].
    pub fn paint_with_labels(ctx: &mut crate::scene::paint::PaintCtx) {
        with_state(|m| m.borrow().paint_with_labels(ctx));
    }
}

#[derive(Debug, Clone)]
pub struct Widget {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub label: Option<String>,
    pub hovered: bool,
    pub row_x: f32,
    pub row_w: f32,
    pub focused: bool,
    pub id: std::cell::Cell<Option<crate::widget::WidgetId>>,
    pub dirty: bool,
    pub config_file: Option<String>,
    pub config_key: Option<String>,
    /// Lives exactly as long as this base: the registry holds a watch on it and will
    /// not hand out the widget's pointer once it is gone. See [`Liveness`].
    pub(crate) live: Liveness,
}

/// A token owned by a widget's [`Widget`] base, watched by the [`UiContext`] registry
/// (`scene::tree::WidgetTree`).
///
/// The registry holds raw pointers to widgets the APP owns, so an app that drops a
/// widget without unregistering it (a rebuilt `Vec` of rows — the case
/// `UiContext::unregister_widget` warns about) used to leave a dangling pointer that
/// the next registry sweep dereferenced. The registry now keeps a [`Weak`] to this
/// token beside each pointer and resolves the pointer only while the token is alive,
/// so a dropped widget reads back as unregistered instead.
///
/// It does NOT catch a widget that was MOVED while registered (a `Vec` that
/// reallocated, a struct returned by value): the token moves with it, and the stored
/// pointer still names the old address. That is what [`Owned`](crate::widget::Owned) is
/// for: a box whose ALLOCATION carries the token the registry watches instead.
///
/// A clone is a different widget at a different address, so it gets a fresh token —
/// not a share of the original's, which would keep a dropped original "alive".
///
/// [`UiContext`]: crate::context::UiContext
/// [`Weak`]: std::sync::Weak
#[derive(Debug)]
pub(crate) struct Liveness(std::sync::Arc<()>);

impl Liveness {
    pub(crate) fn new() -> Self {
        Liveness(std::sync::Arc::new(()))
    }

    /// A watch that reports whether this token still exists.
    pub(crate) fn watch(&self) -> std::sync::Weak<()> {
        std::sync::Arc::downgrade(&self.0)
    }
}

impl Clone for Liveness {
    fn clone(&self) -> Self {
        Liveness::new()
    }
}

impl Widget {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            label: None,
            hovered: false,
            row_x: 0.0,
            row_w: 0.0,
            focused: false,
            id: std::cell::Cell::new(None),
            dirty: true,
            config_file: None,
            config_key: None,
            live: Liveness::new(),
        }
    }

    pub fn new_rect(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            x,
            y,
            w,
            h,
            label: None,
            hovered: false,
            row_x: 0.0,
            row_w: 0.0,
            focused: false,
            id: std::cell::Cell::new(None),
            dirty: true,
            config_file: None,
            config_key: None,
            live: Liveness::new(),
        }
    }

    pub fn id(&self) -> crate::widget::WidgetId {
        let current = self.id.get();
        if let Some(id) = current {
            id
        } else {
            let next = crate::widget::NEXT_WIDGET_ID.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let id = crate::widget::WidgetId(next);
            self.id.set(Some(id));
            id
        }
    }

    /// The detached-label strip this widget carries above its content: the one
    /// control-label formula (`layout::control_label_strip`) when a label is set,
    /// zero otherwise.
    pub fn label_offset(&self) -> f32 {
        if self.label.is_some() { crate::layout::control_label_strip() } else { 0.0 }
    }
}


pub fn clear_widget_references(w: &dyn WidgetHost) {
    // Focus needs no clearing: the context keeps an id, which a dropped widget's
    // generation no longer resolves.
    context_menu::clear_if_matches(w);
}

#[macro_export]
macro_rules! impl_widget_base {
    ($name:ident) => {
        fn base(&self) -> &$crate::widget::Widget { &self.base }
        fn base_mut(&mut self) -> &mut $crate::widget::Widget { &mut self.base }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
    };
}

#[cfg(test)]
mod context_menu_slider_tests {
    use super::context_menu::{ContextMenuState, MenuSlider, PAD, ROW_H, SLIDER_W};
    use crate::widget::{ElementState, MouseButton, MouseScrollDelta, Position, WidgetId};

    fn menu() -> ContextMenuState {
        let mut m = ContextMenuState::new();
        m.show(100.0, 50.0, vec!["Frame All".into(), "Opacity".into()], 0, WidgetId(7));
        m.set_row_slider(1, MenuSlider { value: 50.0, min: 0.0, max: 100.0, step: 5.0, decimals: 0, suffix: "%" });
        m
    }
    fn row_mid(m: &ContextMenuState, idx: usize) -> f32 {
        m.row_y(idx) + ROW_H * 0.5
    }

    /// Twenty rows: 20 * ROW_H + 2 * PAD tall, far more than the boxes below.
    fn long_menu() -> ContextMenuState {
        let mut m = ContextMenuState::new();
        let rows: Vec<String> = (0..20).map(|i| format!("Row {i}")).collect();
        m.show(100.0, 50.0, rows, 0, WidgetId(7));
        m
    }

    /// The keyboard walks a menu: a step skips the header and the
    /// separators, stops at both ends rather than wrapping, and scrolls a
    /// shortened menu to the row it lands on.
    #[test]
    fn the_keyboard_steps_the_highlight_over_what_cannot_run() {
        let mut m = ContextMenuState::new();
        let rows = ["Header", "A", "-", "B", "C"].map(String::from).to_vec();
        m.show(100.0, 50.0, rows, 1, WidgetId(7));
        assert_eq!(m.step_hovered(1), Some(1), "the header is skipped");
        assert_eq!(m.step_hovered(1), Some(3), "and the separator");
        assert_eq!(m.step_hovered(1), Some(4));
        assert_eq!(m.step_hovered(1), Some(4), "the last row stays");
        assert_eq!(m.step_hovered(-1), Some(3));
        assert_eq!(m.step_hovered(-1), Some(1));
        assert_eq!(m.step_hovered(-1), Some(1), "the header is not reached");
        m.set_hovered_item(Some(2));
        assert_eq!(m.hovered_item, None, "a separator cannot be highlighted");
        assert_eq!(m.step_hovered(-1), Some(4), "from nothing, up starts at the bottom");

        let mut long = long_menu();
        long.place(100.0, 50.0, 200.0);
        long.set_hovered_item(Some(15));
        assert!(long.row_y(15) >= long.y && long.row_y(15) + ROW_H <= long.y + long.h, "scrolled into view");
        long.set_hovered_item(Some(0));
        assert_eq!(long.scroll, 0.0);
    }

    /// Placed shorter than its rows, the menu scrolls: the wheel moves the
    /// rows a row a notch, up shows the rows above, it stops at both ends,
    /// and the row under the pointer — hover, press — is the one DRAWN
    /// there, scroll included.
    #[test]
    fn a_shortened_menu_scrolls_its_rows() {
        let mut m = long_menu();
        let full = m.content_h;
        assert_eq!(full, 20.0 * ROW_H + 2.0 * PAD);
        m.place(100.0, 50.0, 200.0);
        assert_eq!(m.h, 200.0);
        assert_eq!(m.max_scroll(), full - 200.0);

        let (px, py) = (130.0, m.y + PAD + ROW_H * 0.5);
        m.cursor_moved(px, py);
        assert_eq!(m.row_at(px, py), Some(0));
        // Wheel down (negative notches): three rows further on.
        assert!(m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), px, py));
        assert_eq!(m.scroll, 3.0 * ROW_H);
        assert_eq!(m.row_at(px, py), Some(3), "the row under the pointer moved with the scroll");
        assert_eq!(m.hovered_item, Some(3), "and the hover followed it without a motion event");
        assert_eq!(m.row_y(3), m.y + PAD, "row 3 is drawn where row 0 was");
        // Up past the top stops at the top; down past the end stops there.
        m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 10.0), px, py);
        assert_eq!(m.scroll, 0.0);
        m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -100.0), px, py);
        assert_eq!(m.scroll, m.max_scroll());
        // Nothing to scroll: the wheel is not the menu's.
        let mut short = menu();
        assert!(!short.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 110.0, short.row_y(0) + 1.0));
    }

    /// A row outside the shown plate is not under the pointer, even though
    /// the rows' arithmetic would reach it — the plate ends at `h`.
    #[test]
    fn rows_below_a_shortened_plate_are_not_hit() {
        let mut m = long_menu();
        m.place(100.0, 50.0, 200.0);
        assert!(m.row_at(130.0, m.y + m.h + 5.0).is_none());
        assert!(!m.hit_test(130.0, m.y + m.h + 5.0));
    }

    /// The in-window placement, from the anchor: fits below → stays; not
    /// below but above → flips to open up from the anchor; neither → slides
    /// to the bottom edge; taller than the box → cut to it, scrolling. And
    /// the right edge slides the menu left.
    #[test]
    fn constrain_flips_slides_and_shortens_like_a_positioner() {
        // Fits below.
        let mut m = menu();
        m.constrain_to(0.0, 0.0, 800.0, 600.0);
        assert_eq!((m.x, m.y, m.h), (100.0, 50.0, m.content_h));

        // Opened near the bottom: flips up from the anchor.
        let mut m = ContextMenuState::new();
        m.show(100.0, 580.0, vec!["A".into(), "B".into(), "C".into()], 0, WidgetId(7));
        m.constrain_to(0.0, 0.0, 800.0, 600.0);
        assert_eq!(m.y, 580.0 - m.content_h, "flipped to open upward");

        // No room either way: slides to the bottom edge, whole.
        let mut m = long_menu(); // 496 tall
        m.show(100.0, 300.0, (0..20).map(|i| format!("{i}")).collect(), 0, WidgetId(7));
        m.constrain_to(0.0, 0.0, 800.0, 600.0);
        assert_eq!(m.y + m.h, 600.0);
        assert_eq!(m.h, m.content_h);

        // Taller than the box: cut to it, and it scrolls.
        m.constrain_to(0.0, 0.0, 800.0, 300.0);
        assert_eq!((m.y, m.h), (0.0, 300.0));
        assert!(m.max_scroll() > 0.0);

        // The right edge: slides left to fit.
        let mut m = menu();
        m.show(790.0, 50.0, vec!["A".into()], 0, WidgetId(7));
        m.constrain_to(0.0, 0.0, 800.0, 600.0);
        assert_eq!(m.x + m.w, 800.0);

        // Re-running is stable: it works from the anchor, not from where
        // the last run put it.
        let mut m = long_menu();
        m.constrain_to(0.0, 0.0, 800.0, 300.0);
        let first = (m.x, m.y, m.h);
        m.constrain_to(0.0, 0.0, 800.0, 300.0);
        assert_eq!((m.x, m.y, m.h), first);
    }

    /// Hosted in the popup, the menu draws nothing into the window's list —
    /// every app still calls the in-window paint — while the runner's
    /// `paint_hosted` draws it at the origin. Hit testing is untouched.
    #[test]
    fn a_hosted_menu_paints_only_through_the_popup() {
        use super::context_menu as cm;
        cm::show(100.0, 50.0, vec!["Frame All".into(), "Opacity".into()], 0, WidgetId(7));
        cm::set_hosted(true);
        let mut pc = crate::scene::paint::PaintCtx::new();
        cm::paint_with_labels(&mut pc);
        assert!(pc.finish().items.is_empty(), "nothing in the window");
        assert!(cm::text_labels().is_empty());
        assert!(cm::hit_test(110.0, 60.0), "still hit-tested where it is");

        let mut pc = crate::scene::paint::PaintCtx::new();
        cm::paint_hosted(&mut pc);
        let dl = pc.finish();
        assert!(!dl.items.is_empty(), "the popup draws it");
        let texts: Vec<(f32, f32)> = dl
            .items
            .iter()
            .filter_map(|i| match &i.prim {
                crate::scene::paint::Prim::Text { x, y, .. } => Some((*x, *y)),
                _ => None,
            })
            .collect();
        assert!(texts.iter().all(|&(x, y)| x < 100.0 && y < 50.0 + 2.0 * ROW_H), "at the popup's origin, not the window's");
        cm::set_hosted(false);
        cm::hide();
    }

    /// Painted through the thread-local, as every host paints it: the slider
    /// stamp is dropped while `CONTEXT_MENU` is borrowed, and its drop clears
    /// widget references in that same cell. The tests above paint a bare
    /// `ContextMenuState` and never held the borrow.
    #[test]
    fn a_slider_row_paints_through_the_shared_menu() {
        use super::context_menu as cm;
        cm::show(100.0, 50.0, vec!["Frame All".into(), "Opacity".into()], 0, WidgetId(7));
        cm::set_row_slider(1, MenuSlider { value: 50.0, min: 0.0, max: 100.0, step: 5.0, decimals: 0, suffix: "%" });
        let mut pc = crate::scene::paint::PaintCtx::new();
        cm::paint_with_labels(&mut pc);
        cm::paint(&mut pc);
        assert!(cm::is_visible(), "painting leaves the menu up");
        cm::hide();
    }

    /// A notch over the slider row steps it by `step`, up is more, and the
    /// change is reported once; over an action row the wheel is not the
    /// menu's. A trackpad's fractions add up to whole steps.
    #[test]
    fn the_wheel_steps_a_slider_row() {
        let mut m = menu();
        let y = row_mid(&m, 1);
        assert!(m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, y));
        assert_eq!(m.slider(1).unwrap().value, 55.0);
        assert_eq!(m.take_slider_change(), Some((1, 55.0)));
        assert_eq!(m.take_slider_change(), None, "reported once");
        m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 150.0, y);
        assert_eq!(m.slider(1).unwrap().value, 45.0);

        assert!(!m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, row_mid(&m, 0)), "an action row does not take the wheel");

        // 30 px is half a notch: nothing yet, then the second half lands a step.
        let half = MouseScrollDelta::PixelDelta(Position { x: 0.0, y: 30.0 });
        assert!(!m.mouse_wheel(&half, 150.0, y));
        assert!(m.mouse_wheel(&half, 150.0, y));
        assert_eq!(m.slider(1).unwrap().value, 50.0);

        // Clamped at the ends, and a clamp that moves nothing reports nothing.
        for _ in 0..30 {
            m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, y);
        }
        assert_eq!(m.slider(1).unwrap().value, 100.0);
        m.take_slider_change();
        assert!(!m.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 1.0), 150.0, y));
        assert_eq!(m.take_slider_change(), None);
    }

    /// A press on the band jumps to the pointer (snapped to the step) and
    /// drags; the menu stays open through it, and the release ends the drag.
    /// A press on an action row still fires and closes, as before.
    #[test]
    fn a_press_on_the_band_drags_and_keeps_the_menu_open() {
        let mut m = menu();
        let y = row_mid(&m, 1);
        let band = m.slider_band(1);
        assert!((band.x + SLIDER_W - (m.x + m.w - PAD)).abs() < 1e-3, "the band ends at the padding");
        assert!(m.mouse_input(MouseButton::Left, ElementState::Pressed, band.x + band.width * 0.8, y, None));
        assert!(m.visible, "a slider row does not close the menu");
        assert_eq!(m.slider(1).unwrap().value, 80.0);
        m.cursor_moved(band.x + band.width * 0.21, y + 200.0);
        assert_eq!(m.slider(1).unwrap().value, 20.0, "the drag follows off the plate, snapped to 5");
        assert!(m.mouse_input(MouseButton::Left, ElementState::Released, 0.0, 0.0, None));
        assert!(m.slider_drag.is_none());
        m.cursor_moved(band.x, y);
        assert_eq!(m.slider(1).unwrap().value, 20.0, "released: motion is hover again");

        // A press on the row's label end: the slider's, nothing moves.
        assert!(m.mouse_input(MouseButton::Left, ElementState::Pressed, m.x + PAD + 2.0, y, None));
        assert!(m.visible);
        assert_eq!(m.slider(1).unwrap().value, 20.0);

        m.mouse_input(MouseButton::Left, ElementState::Pressed, m.x + PAD + 2.0, row_mid(&m, 0), None);
        assert!(!m.visible, "an action row still fires and closes");
    }

    /// The plate widens for the label, the readout and the band; a fresh
    /// `show` clears every slider back to an action row.
    #[test]
    fn a_slider_row_widens_the_plate_and_show_clears_it() {
        let mut m = ContextMenuState::new();
        m.show(0.0, 0.0, vec!["Opacity".into()], 0, WidgetId(7));
        let narrow = m.w;
        m.set_row_slider(0, MenuSlider { value: 1.0, min: 0.0, max: 100.0, step: 1.0, decimals: 0, suffix: "%" });
        assert!(m.w >= narrow.max(SLIDER_W + 2.0 * PAD));
        let labels = m.text_labels();
        assert!(labels.iter().any(|l| l.text == "1%"), "the readout is a label: {:?}", labels.iter().map(|l| &l.text).collect::<Vec<_>>());
        m.show(0.0, 0.0, vec!["Opacity".into()], 0, WidgetId(7));
        assert!(m.slider(0).is_none());
    }
}

#[cfg(test)]
mod context_menu_padding_tests {
    use super::context_menu::{self, split_mark, ContextMenuState, PAD, ROW_H};
    use crate::widget::WidgetId;

    /// The shared menu is a popover for the window-drag question too: a
    /// press on one of its rows must never start a window move, whatever
    /// sits under the menu. It has no widget id to register, so the veto
    /// asks the thread-local directly. And the free `row_at` is the
    /// PAD-aware row hit test hosts dispatch by.
    #[test]
    fn an_open_menu_vetoes_window_drags_under_it() {
        let ctx = crate::context::UiContext::new();
        context_menu::show(100.0, 200.0, vec!["Copy".into(), "Paste".into()], 0, WidgetId(1));
        assert!(!ctx.drag_allowed_at(110.0, 200.0 + PAD + ROW_H * 0.5), "a press on a row is not a drag");
        assert_eq!(context_menu::row_at(110.0, 200.0 + PAD + ROW_H * 1.5), Some(1));
        assert_eq!(context_menu::row_y(1), 200.0 + PAD + ROW_H);
        assert!(ctx.drag_allowed_at(10.0, 10.0), "away from the menu the drag question is the widgets'");
        context_menu::hide();
        assert!(ctx.drag_allowed_at(110.0, 200.0 + PAD + ROW_H * 0.5), "hidden, it vetoes nothing");
    }

    /// A row's leading mark is a glyph, not a character: the label is drawn
    /// without it and after the glyph's room, the glyph is the mark's
    /// (`check`, `circle`, `circle-outline`), and a page row's chevron and a
    /// page's back chevron are glyphs too — no row draws "✓", "●", "○", "›"
    /// or "‹" as text.
    #[test]
    fn menu_marks_and_chevrons_are_glyphs() {
        let mut m = ContextMenuState::new();
        m.show(0.0, 0.0, vec!["✓ Show Grid".into(), "● On".into(), "○ Off".into(), "Plain".into(), "Add Tab".into()], 0, WidgetId(1));
        m.set_row_page(4);
        let labels = m.labels();
        let texts: Vec<&str> = labels.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Show Grid", "On", "Off", "Plain", "Add Tab"]);
        assert!(labels[0].x > labels[3].x, "a marked label stands after its mark's room");
        let names: Vec<&str> = m.glyphs().iter().map(|g| g.name).collect();
        assert_eq!(names, ["check", "circle", "circle-outline", "chevron-right"]);
        m.show_page(0.0, 0.0, Some("View"), vec!["○ Wireframe".into()], 0, WidgetId(1));
        assert_eq!(m.labels()[0].text, "View", "the back band reads its title, its chevron a glyph");
        assert_eq!(m.glyphs().iter().map(|g| g.name).collect::<Vec<_>>(), ["chevron-left", "circle-outline"]);
        for l in m.labels() {
            assert!(!l.text.contains(['✓', '●', '○', '›', '‹']), "{:?} draws a mark as text", l.text);
        }
        assert_eq!(split_mark("✓ Collapse controls"), (Some("check"), "Collapse controls"));
        assert_eq!(split_mark("Collapse controls"), (None, "Collapse controls"));
    }

    /// Every glyph the toolkit draws by name is in the icon set: a name
    /// with no file draws NOTHING (a missing icon is not an error at paint),
    /// so the miss is caught here. Skipped where the icon set is not checked
    /// out beside the crate.
    #[test]
    fn every_glyph_the_toolkit_names_is_in_the_icon_set() {
        let dir = std::path::Path::new(&crate::icons_dir()).to_path_buf();
        if !dir.is_dir() {
            eprintln!("skipped: no icon set at {}", dir.display());
            return;
        }
        let mut names = std::collections::BTreeSet::new();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).unwrap().flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).unwrap();
                for call in [".icon(\"", "upload_icon(\"", "upload_icon_tinted(\"", "with_icon_name(\"", "new_icon(\""] {
                    for (i, _) in src.match_indices(call) {
                        let rest = &src[i + call.len()..];
                        if let Some(end) = rest.find('"') {
                            let n = &rest[..end];
                            if !n.is_empty() && n.chars().all(|c| c.is_ascii_lowercase() || c == '-') {
                                names.insert(n.to_string());
                            }
                        }
                    }
                }
            }
        }
        for g in ["check", "circle", "circle-outline", "chevron-left", "chevron-right", "chevron-up", "chevron-down"] {
            names.insert(g.to_string());
        }
        let missing: Vec<_> = names.iter().filter(|n| !dir.join(format!("{n}.svg")).is_file()).collect();
        assert!(missing.is_empty(), "named but not in {}: {missing:?}", dir.display());
    }

    /// The plate pads its rows evenly: the height is the rows plus a pad
    /// above and below, the labels sit one pad in from the left with the
    /// widest one a pad from the right, and the padding is plate — a pointer
    /// in it hovers no row, and a pointer a row down from the top pad is on
    /// row 1, not row 0 plus a fraction.
    #[test]
    fn rows_sit_inside_an_even_pad() {
        let mut m = ContextMenuState::new();
        m.show(100.0, 200.0, vec!["Hide Geometry".into(), "-".into(), "Delete".into()], 0, WidgetId(1));
        assert_eq!(m.h, 3.0 * ROW_H + 2.0 * PAD);
        assert!(m.w >= 2.0 * PAD);
        let labels = m.text_labels();
        assert!(labels.iter().all(|l| l.x == 100.0 + PAD), "labels start one pad in");
        assert_eq!(labels[0].y, 200.0 + PAD + (ROW_H - labels[0].font_size) / 2.0, "row 0 starts under the top pad");

        assert_eq!(m.row_at(110.0, 200.0 + PAD * 0.5), None, "the top pad is no row");
        assert_eq!(m.row_at(110.0, 200.0 + PAD + ROW_H * 0.5), Some(0));
        assert_eq!(m.row_at(110.0, 200.0 + PAD + ROW_H * 2.5), Some(2));
        assert_eq!(m.row_at(110.0, 200.0 + m.h - PAD * 0.5), None, "the bottom pad is no row");

        m.cursor_moved(110.0, 200.0 + PAD * 0.5);
        assert_eq!(m.hovered_item, None);
        m.cursor_moved(110.0, 200.0 + PAD + ROW_H * 1.5);
        assert_eq!(m.hovered_item, None, "a separator row never hovers");
        m.cursor_moved(110.0, 200.0 + PAD + ROW_H * 2.5);
        assert_eq!(m.hovered_item, Some(2));
    }

    /// A label with a glyph the menu face lacks — the radio marks the
    /// designer's pin rows carry — is measured as the renderer shapes it,
    /// fallback face and all, so the plate is wide enough for what is drawn.
    /// The SVG-inked measure alone called the mark next to nothing.
    #[test]
    fn a_fallback_glyph_widens_the_plate_as_drawn() {
        let mut m = ContextMenuState::new();
        m.show(0.0, 0.0, vec!["● Follow Active Editor".into()], 0, WidgetId(1));
        let (family, size) = super::context_menu::label_font();
        let drawn = {
            let mut fs = crate::geometry_font_system().lock().unwrap();
            crate::backend::text::shaped_cluster_offsets(&mut fs, "● Follow Active Editor", size, Some(&family))
                .last()
                .map(|&(_, t)| t)
                .unwrap()
        };
        assert!(drawn > 0.0);
        assert!(m.w >= drawn + 2.0 * PAD, "plate {} narrower than the drawn label {} plus pads", m.w, drawn);
    }
}

#[cfg(test)]
mod context_menu_page_tests {
    use super::context_menu::{self, PageTurn, PAD, ROW_H};
    use crate::widget::{MouseScrollDelta, Position, WidgetId};

    fn open() {
        context_menu::show(400.0, 200.0, vec!["Frame".into(), "Style".into(), "Markers".into(), "Exit".into()], 0, WidgetId(1));
        context_menu::set_row_page(1);
        context_menu::set_row_page(2);
    }

    fn over(idx: usize) -> (f32, f32) {
        (context_menu::x() + 20.0, context_menu::row_y(idx) + ROW_H * 0.5)
    }

    /// One swipe, a few events long, the fingers lifted after. The runner's
    /// phase is left alone — it is one value for the whole process and
    /// other tests set it.
    fn swipe(dx: f64, x: f32, y: f32) -> bool {
        crate::widget::side_swipe::end_gesture();
        let mut took = false;
        for _ in 0..4 {
            took |= context_menu::mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: dx / 4.0, y: 0.0 }), x, y);
        }
        took
    }

    /// A page row is a row that turns the plate: a press on it, or a swipe
    /// forward with the pointer on it, asks for its page, and nothing opens
    /// on hover alone.
    #[test]
    fn a_page_row_turns_on_a_press_or_a_swipe() {
        open();
        let (x, y) = over(1);
        context_menu::cursor_moved(x, y);
        assert_eq!(context_menu::take_turn(), None, "hovering turns nothing");
        assert_eq!(context_menu::turn_at(x, y), Some(PageTurn::Into(1)));
        let (ex, ey) = over(3);
        assert_eq!(context_menu::turn_at(ex, ey), None, "an action row is no page");

        assert!(swipe(-80.0, x, y), "the swipe was the menu's");
        assert_eq!(context_menu::take_turn(), Some(PageTurn::Into(1)));
        assert_eq!(context_menu::take_turn(), None, "taken once");
        for _ in 0..4 {
            context_menu::mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: -20.0, y: 0.0 }), x, y);
        }
        assert_eq!(context_menu::take_turn(), None, "one turn a gesture");
        open();
        assert!(!swipe(-80.0, ex, ey), "forward over an action row turns nothing");
        open();
        assert!(!swipe(80.0, x, y), "back from a menu that was opened goes nowhere");
        assert_eq!(context_menu::take_turn(), None);
        context_menu::hide();
    }

    /// A turn is animated: the plate grows or shrinks from the size of the
    /// one it replaced, a host surface is given room for both meanwhile, and
    /// after `TURN_MS` it is the page's own. A plain show does not animate.
    #[test]
    fn a_page_turn_grows_the_plate_from_the_one_it_replaced() {
        context_menu::show(400.0, 200.0, (0..12).map(|i| format!("Row {i}")).collect(), 0, WidgetId(1));
        assert!(!context_menu::is_turning(), "a menu opened is not a turn");
        let (_, w0, h0) = context_menu::natural_geometry();
        let (mx, my) = (context_menu::x(), context_menu::y());
        context_menu::show_page(mx, my, Some("View"), vec!["One".into()], 0, WidgetId(1));
        assert!(context_menu::is_turning());
        let drawn = context_menu::with_state(|m| m.borrow().drawn_rect());
        let own = context_menu::h();
        assert!(own < h0 && drawn.height > own && drawn.height <= h0, "on its way down: {} between {own} and {h0}", drawn.height);
        let (_, w, h) = context_menu::natural_geometry();
        assert_eq!((w, h), (w0.max(context_menu::w()), h0), "room for both while it turns");

        std::thread::sleep(std::time::Duration::from_millis(context_menu::TURN_MS as u64 + 40));
        assert!(!context_menu::is_turning());
        let (_, w, h) = context_menu::natural_geometry();
        assert_eq!((w, h), (context_menu::w(), own), "its own size once it has landed");

        // A page shown in the moment the menu was put down turns from it too.
        context_menu::hide();
        context_menu::show_page(mx, my, None, (0..12).map(|i| format!("Row {i}")).collect(), 0, WidgetId(1));
        assert!(context_menu::is_turning(), "handed over from the menu just hidden");
        context_menu::hide();
    }

    /// A turn fades the rows' geometry too, not only their labels: the
    /// separators of the plate it leaves and of the page it brings are both
    /// drawn, each at part of its strength, and at the end only the page's,
    /// whole.
    #[test]
    fn a_turn_fades_the_separators_of_both_plates() {
        let grooves = || {
            let mut pc = crate::scene::paint::PaintCtx::new();
            context_menu::paint(&mut pc);
            pc.finish()
                .items
                .into_iter()
                .filter_map(|it| match it.prim {
                    crate::scene::paint::Prim::Groove { strength, .. } => Some(strength),
                    _ => None,
                })
                .collect::<Vec<f32>>()
        };
        context_menu::show(400.0, 200.0, vec!["A".into(), "-".into(), "B".into()], 0, WidgetId(1));
        assert_eq!(grooves(), vec![1.0], "a menu's separator, whole");
        let (mx, my) = (context_menu::x(), context_menu::y());
        context_menu::show_page(mx, my, Some("View"), vec!["C".into(), "-".into(), "D".into(), "E".into()], 0, WidgetId(1));
        // The band's groove is drawn with the page's rows at their strength.
        let mid = grooves();
        assert!(mid.len() >= 2, "both plates' separators while it turns: {mid:?}");
        assert!(mid.iter().all(|s| *s < 1.0), "each faded: {mid:?}");
        std::thread::sleep(std::time::Duration::from_millis(context_menu::turn_ms() as u64 + 40));
        let after = grooves();
        assert!(!after.is_empty() && after.iter().all(|s| *s == 1.0), "the page's alone, whole: {after:?}");
        context_menu::hide();
    }

    /// A page stands where the menu stood, under a back band that a press or
    /// a swipe back turns back from; its rows begin under the band.
    #[test]
    fn a_page_stands_in_the_menus_place_with_a_way_back() {
        open();
        let (mx, my) = (context_menu::x(), context_menu::y());
        context_menu::show_page(mx, my, Some("View"), vec!["○ Wireframe".into(), "Opacity".into()], 0, WidgetId(1));
        assert!(context_menu::is_turned());
        assert_eq!((context_menu::x(), context_menu::y()), (mx, my));
        assert_eq!(context_menu::back_title().as_deref(), Some("View"));
        assert_eq!(context_menu::row_y(0), my + PAD + ROW_H, "the rows begin under the band");
        assert_eq!(context_menu::h(), 2.0 * ROW_H + ROW_H + 2.0 * PAD);

        let band = (mx + 20.0, my + PAD + ROW_H * 0.5);
        assert_eq!(context_menu::row_at(band.0, band.1), None, "the band is no row");
        assert_eq!(context_menu::turn_at(band.0, band.1), Some(PageTurn::Back));
        let (x, y) = over(1);
        assert_eq!(context_menu::row_at(x, y), Some(1));
        assert!(swipe(80.0, x, y));
        assert_eq!(context_menu::take_turn(), Some(PageTurn::Back), "back from anywhere on the page");

        // A placement that cannot fit it below slides it, never flips it up
        // from the corner it took over.
        context_menu::constrain_to(0.0, 0.0, 2000.0, my + 10.0);
        assert!(context_menu::y() + context_menu::h() <= my + 10.0 + 0.01);
        assert!(context_menu::y() >= 0.0);
        context_menu::hide();
    }
}

#[cfg(test)]
mod context_menu_action_tests {
    use super::context_menu::{self, ROW_H};
    use crate::context::UiContext;
    use crate::widget::{ContextAction, ElementState, MouseButton, Owned, Widget, WidgetHost};

    /// A widget that remembers the last action a menu ran on it.
    struct Recorder {
        base: Widget,
        got: Option<ContextAction>,
    }
    impl WidgetHost for Recorder {
        crate::impl_widget_base!(Recorder);
        fn color(&self) -> [f32; 4] {
            [0.0; 4]
        }
        fn context_action(&mut self, action: ContextAction) -> bool {
            self.got = Some(action);
            true
        }
    }

    fn press_row(ctx: &mut UiContext, idx: usize) {
        let (x, y) = (context_menu::x() + 10.0, context_menu::row_y(idx) + ROW_H * 0.5);
        context_menu::mouse_input(MouseButton::Left, ElementState::Pressed, x, y, Some(ctx));
    }

    #[test]
    fn a_row_runs_its_action_whatever_its_label_says() {
        let mut ctx = UiContext::new();
        let mut w = Owned::new(Recorder { base: Widget::new(), got: None });
        ctx.register_host(&mut w);
        let id = w.base().id();

        // A label the English table has never seen: only the row's action can say what it is.
        context_menu::show(0.0, 0.0, vec!["[Recorder]".into(), "Kopieren".into()], 1, id);
        context_menu::set_row_actions(vec![None, Some(ContextAction::Copy)]);
        press_row(&mut ctx, 1);
        assert_eq!(w.got, Some(ContextAction::Copy));

        // A row with an action and an English label that names ANOTHER: the action wins.
        w.got = None;
        context_menu::show(0.0, 0.0, vec!["[Recorder]".into(), "Paste".into()], 1, id);
        context_menu::set_row_actions(vec![None, Some(ContextAction::SelectAll)]);
        press_row(&mut ctx, 1);
        assert_eq!(w.got, Some(ContextAction::SelectAll));

        // A menu built without actions still works in English, through the fallback.
        w.got = None;
        context_menu::show(0.0, 0.0, vec!["[Recorder]".into(), "Paste".into()], 1, id);
        press_row(&mut ctx, 1);
        assert_eq!(w.got, Some(ContextAction::Paste));
    }

    #[test]
    fn the_toolkits_own_menus_carry_their_actions() {
        let mut ctx = UiContext::new();
        let mut tb = Owned::new(crate::widget::TextBox::new(String::new()).with_label("Name"));
        ctx.register_host(&mut tb);
        let ptr = &mut *tb as &mut (dyn WidgetHost + 'static) as *mut (dyn WidgetHost + 'static);
        // SAFETY: `tb` is live and registered for the whole test.
        unsafe { ctx.handle_right_click(ptr, 5.0, 5.0) };
        let options = context_menu::options();
        let header = context_menu::header_count();
        assert!(options.len() > header, "a text box's menu has rows");
        for (i, label) in options.iter().enumerate().skip(header) {
            assert!(context_menu::row_action(i).is_some(), "row {label:?} has no action of its own");
        }
        context_menu::hide();
    }
}
