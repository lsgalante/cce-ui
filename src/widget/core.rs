use crate::widget::WidgetHost;

pub mod focus {
    use super::WidgetHost;
    use crate::widget::WidgetId;
    use std::cell::Cell;

    // Phase 6bc: the thread-local focus store keys by id, not pointer. Dispatching to the
    // previous holder (`unfocus`) resolves through the caller's generational tree, so a
    // stale id is skipped instead of dereferencing freed memory (the 6w settings UAF class).
    thread_local! {
        static FOCUSED_WIDGET: Cell<Option<WidgetId>> = Cell::new(None);
    }

    /// Resolve `id` in `ctx`'s tree (when a ctx is in reach) and call `unfocus()` on it.
    fn unfocus_via(ctx: Option<&mut crate::context::UiContext>, id: WidgetId) {
        if let Some(ctx) = ctx {
            if let Some(ptr) = ctx.tree.get_ptr(id) {
                unsafe {
                    (*ptr).unfocus();
                }
            }
        }
    }

    pub fn set_focused(w: &mut dyn WidgetHost, ctx: Option<&mut crate::context::UiContext>) {
        set_focused_id(w.base().id(), ctx);
    }

    pub fn set_focused_id(id: WidgetId, ctx: Option<&mut crate::context::UiContext>) {
        let old = FOCUSED_WIDGET.with(|cell| cell.get());
        if let Some(old_id) = old {
            if old_id != id {
                unfocus_via(ctx, old_id);
                FOCUSED_WIDGET.with(|cell| cell.set(Some(id)));
            }
        } else {
            FOCUSED_WIDGET.with(|cell| cell.set(Some(id)));
        }
    }

    pub fn is_focused(w: &dyn WidgetHost) -> bool {
        is_focused_id(w.base().id())
    }

    pub fn is_focused_id(id: WidgetId) -> bool {
        FOCUSED_WIDGET.with(|cell| cell.get() == Some(id))
    }

    pub fn clear_focus(ctx: Option<&mut crate::context::UiContext>) {
        if let Some(id) = FOCUSED_WIDGET.with(|cell| cell.take()) {
            unfocus_via(ctx, id);
        }
    }

    pub fn clear_if_matches(w: &dyn WidgetHost) {
        clear_if_matches_id(w.base().id());
    }

    pub fn clear_if_matches_id(id: WidgetId) {
        FOCUSED_WIDGET.with(|cell| {
            if cell.get() == Some(id) {
                cell.set(None);
            }
        });
    }

    pub fn has_focus() -> bool {
        FOCUSED_WIDGET.with(|cell| cell.get().is_some())
    }

    pub fn link_parent_child(parent: &mut dyn WidgetHost, child: &mut dyn WidgetHost, ctx: &mut crate::context::UiContext) {
        let parent_ptr = unsafe {
            std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(parent as *mut dyn WidgetHost)
        };
        let child_ptr = unsafe {
            std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(child as *mut dyn WidgetHost)
        };
        let (p_id, c_id) = (parent.base().id(), child.base().id());
        ctx.register_widget(p_id, parent_ptr);
        ctx.register_widget(c_id, child_ptr);
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

    thread_local! {
        pub static HOVER_STATE: RefCell<HoverState> = RefCell::new(HoverState::new());
        pub static CURSOR_POS: RefCell<(f32, f32)> = RefCell::new((0.0, 0.0));
    }

    pub fn set_cursor_pos(x: f32, y: f32) {
        CURSOR_POS.with(|pos| {
            *pos.borrow_mut() = (x, y);
        });
    }

    pub fn reset_frame_registration() {
        HOVER_STATE.with(|state| {
            state.borrow_mut().registered_this_frame = false;
        });
    }

    pub fn set_scroll_offset(offset: f32) {
        HOVER_STATE.with(|state| {
            state.borrow_mut().scroll_offset = offset;
        });
    }

    pub fn get_scroll_offset() -> f32 {
        HOVER_STATE.with(|state| {
            state.borrow().scroll_offset
        })
    }

    pub fn register_hovered(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) {
        HOVER_STATE.with(|state| {
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
        HOVER_STATE.with(|state| {
            let mut s = state.borrow_mut();
            if !s.registered_this_frame {
                s.target_alpha = 0.0;
                let (cx, cy) = CURSOR_POS.with(|pos| *pos.borrow());
                s.target_x = Some(cx);
                s.target_y = Some(cy + s.scroll_offset);
                s.target_w = Some(0.0);
                s.target_h = Some(0.0);
            }
        });
    }

    pub fn tick(dt: f32) -> bool {
        HOVER_STATE.with(|state| {
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
        HOVER_STATE.with(|state| {
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

pub mod clipboard {
    pub fn copy_to_clipboard(text: &str) {
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

    pub fn read_from_clipboard() -> Option<String> {
        match std::process::Command::new("wl-paste")
            .arg("-n")
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    if let Ok(text) = String::from_utf8(output.stdout) {
                        return Some(text);
                    }
                }
            }
            Err(_) => {}
        }
        match std::process::Command::new("xclip")
            .arg("-selection")
            .arg("clipboard")
            .arg("-o")
            .output()
        {
            Ok(output) => {
                if output.status.success() {
                    if let Ok(text) = String::from_utf8(output.stdout) {
                        return Some(text);
                    }
                }
            }
            Err(_) => {}
        }
        None
    }
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
        /// The slider row a press took hold of, until the release.
        pub slider_drag: Option<usize>,
        /// A trackpad's leftover fraction of a wheel notch.
        wheel_accum: f32,
        /// The last value a slider was moved to, drained by the host.
        slider_change: Option<(usize, f32)>,
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
                slider_drag: None,
                wheel_accum: 0.0,
                slider_change: None,
            }
        }

        pub fn show(&mut self, x: f32, y: f32, options: Vec<String>, header_count: usize, target: WidgetId) {
            self.x = x;
            self.y = y;
            self.options = options;
            self.h = self.options.len() as f32 * ROW_H + 2.0 * PAD;
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
                    let inked = crate::widget::display::measure_text_width(s, &family, size);
                    let shaped = crate::geometry_font_system()
                        .lock()
                        .ok()
                        .and_then(|mut fs| {
                            crate::backend::window_runner::shaped_cluster_offsets(&mut fs, s, size, Some(&family))
                                .last()
                                .map(|&(_, total)| total)
                        })
                        .unwrap_or(0.0);
                    inked.max(shaped)
                })
                .fold(0.0f32, f32::max);
            self.w = (widest + 2.0 * PAD).max(120.0);
            self.visible = true;
            self.hovered_item = None;
            self.target = Some(target);
            self.header_count = header_count;
            self.sliders = vec![None; self.options.len()];
            self.slider_drag = None;
            self.wheel_accum = 0.0;
            self.slider_change = None;
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
            let Some(idx) = self.row_at(px, py) else { return false };
            let Some(s) = self.slider(idx) else { return false };
            self.wheel_accum += delta.notches_y();
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
        }

        pub fn hit_test(&self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
        }

        /// The top of row `idx`.
        pub fn row_y(&self, idx: usize) -> f32 {
            self.y + PAD + idx as f32 * ROW_H
        }

        /// The row under `(px, py)`, or `None` outside the plate or in its
        /// padding — the padding is plate, not a row, so a press there
        /// neither hovers nor fires row 0.
        pub fn row_at(&self, px: f32, py: f32) -> Option<usize> {
            if px < self.x || px > self.x + self.w {
                return None;
            }
            let rel = py - self.y - PAD;
            if rel < 0.0 {
                return None;
            }
            let idx = (rel / ROW_H) as usize;
            (idx < self.options.len()).then_some(idx)
        }

        pub fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            if self.slider_drag.is_some() {
                return self.slider_drag_to(px);
            }
            let was_hovered = self.hovered_item;
            self.hovered_item = None;
            if let Some(idx) = self.row_at(px, py) {
                // A "-" row is a SEPARATOR (the dropdown's convention):
                // engraved, never hovered, never an action.
                if idx >= self.header_count && self.options[idx] != "-" {
                    self.hovered_item = Some(idx);
                }
            }
            self.hovered_item != was_hovered
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

            if self.hit_test(px, py) {
                if let Some(idx) = self.row_at(px, py) {
                    if idx >= self.header_count {
                        let opt = self.options[idx].clone();
                        if let (Some(target_id), Some(ctx)) = (self.target, ctx) {
                            if let Some(target_ptr) = ctx.tree.get_ptr(target_id) {
                                unsafe {
                                    let target = &mut *target_ptr;
                                    use crate::widget::ContextAction as CA;
                                    let action = match opt.as_str() {
                                        "Cut" => Some(CA::Cut),
                                        "Copy" => Some(CA::Copy),
                                        "Paste" => Some(CA::Paste),
                                        "Select All" => Some(CA::SelectAll),
                                        "Undo" => Some(CA::Undo),
                                        "Redo" => Some(CA::Redo),
                                        "Clear" => Some(CA::ClearText),
                                        "Copy Key" => Some(CA::CopyKey),
                                        "Copy Value" => Some(CA::CopyValue),
                                        "Delete" => Some(CA::DeleteKey),
                                        "Expand" => Some(CA::ExpandNode),
                                        "Collapse" => Some(CA::CollapseNode),
                                        "Expand All" => Some(CA::ExpandAll),
                                        "Collapse All" => Some(CA::CollapseAll),
                                        "Copy Path" => Some(CA::CopyPath),
                                        // The Ramp toggle carries its check state in the label.
                                        "✓ Collapse controls" | "Collapse controls" => Some(CA::ToggleRampControls),
                                        _ => None,
                                    };
                                    if let Some(action) = action {
                                        let _ = target.context_action(action);
                                    }
                                }
                            }
                        }
                    }
                }
                self.hide();
                return true;
            } else {
                self.hide();
                return true;
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
            if !self.visible {
                return;
            }
            let rect = crate::scene::layout::Rect {
                x: self.x,
                y: self.y,
                width: self.w,
                height: self.h,
            };
            let r = crate::layout::menu_corner_radius();
            let depth = crate::layout::bevel_width().min(self.h * 0.2);
            let face = crate::color::page_low_color();
            if face[3] > 0.001 {
                // The popover material: the page colour at menu_opacity,
                // frosted (an opaque page colour would resolve the frost
                // to a solid tint, invisible).
                ctx.plate(rect, (r, r, r, r), &crate::scene::material::Material::popover(face), depth);
            } else {
                let (plateau, radii) = crate::layout::carve_inside(rect, (r, r, r, r), depth);
                ctx.boss(plateau, radii, depth);
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
            if !self.visible { return quads; }

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
            if !self.visible {
                return;
            }
            let (family, _) = label_font();
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
        }

        pub fn text_labels(&self) -> Vec<TextLabel> {
            let mut labels = Vec::new();
            if !self.visible { return labels; }

            for (idx, opt) in self.options.iter().enumerate() {
                if opt == "-" {
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

                labels.push(TextLabel {
                    text: opt.clone(),
                    x: self.x + PAD,
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
    }

    thread_local! {
        pub static CONTEXT_MENU: RefCell<ContextMenuState> = RefCell::new(ContextMenuState::new());
    }

    pub fn is_visible() -> bool {
        CONTEXT_MENU.with(|m| m.borrow().visible)
    }

    pub fn show(x: f32, y: f32, options: Vec<String>, header_count: usize, target: WidgetId) {
        CONTEXT_MENU.with(|m| m.borrow_mut().show(x, y, options, header_count, target));
    }

    pub fn hide() {
        CONTEXT_MENU.with(|m| m.borrow_mut().hide());
    }

    pub fn clear_if_matches(w: &dyn WidgetHost) {
        let id = w.base().id();
        CONTEXT_MENU.with(|m| {
            // Already borrowed means the widget is being dropped from INSIDE
            // the menu's own code — the slider rows' paint stamp, dropped at
            // the end of `paint` under `paint_with_labels`' borrow. A widget
            // the menu made for itself cannot be its target, so there is
            // nothing to clear; `borrow_mut` here panicked on every paint of
            // a menu with a slider row.
            let Ok(mut menu) = m.try_borrow_mut() else { return };
            if menu.target == Some(id) {
                menu.target = None;
                menu.visible = false;
            }
        });
    }

    pub fn x() -> f32 { CONTEXT_MENU.with(|m| m.borrow().x) }
    pub fn y() -> f32 { CONTEXT_MENU.with(|m| m.borrow().y) }
    pub fn w() -> f32 { CONTEXT_MENU.with(|m| m.borrow().w) }
    pub fn h() -> f32 { CONTEXT_MENU.with(|m| m.borrow().h) }
    pub fn hovered_item() -> Option<usize> { CONTEXT_MENU.with(|m| m.borrow().hovered_item) }
    pub fn options() -> Vec<String> { CONTEXT_MENU.with(|m| m.borrow().options.clone()) }

    /// The row under a point, PAD-aware — the ONE row hit test. Every host
    /// that dispatches the menu itself should ask this rather than divide
    /// `(py - y()) / ROW_H`: the rows start `PAD` below the plate's top, so
    /// that division names the row below over the bottom third of every
    /// row, and runs off the end on the last one (2026-09-22 audit: six
    /// call sites across five apps had it).
    pub fn row_at(px: f32, py: f32) -> Option<usize> {
        CONTEXT_MENU.with(|m| m.borrow().row_at(px, py))
    }
    /// A row's top, PAD-aware — for a host painting the rows itself.
    pub fn row_y(idx: usize) -> f32 {
        CONTEXT_MENU.with(|m| m.borrow().row_y(idx))
    }
    pub fn hit_test(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow().hit_test(px, py))
    }

    pub fn cursor_moved(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().cursor_moved(px, py))
    }

    /// Make row `idx` of the shown menu a slider — see [`MenuSlider`]. Call
    /// after [`show`], which clears every row back to an action.
    pub fn set_row_slider(idx: usize, slider: MenuSlider) {
        CONTEXT_MENU.with(|m| m.borrow_mut().set_row_slider(idx, slider));
    }
    pub fn slider(idx: usize) -> Option<MenuSlider> {
        CONTEXT_MENU.with(|m| m.borrow().slider(idx))
    }
    /// The wheel, for hosts that route it: steps the slider under the
    /// pointer. `false` when no slider row is there — let it scroll the page.
    pub fn mouse_wheel(delta: &MouseScrollDelta, px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().mouse_wheel(delta, px, py))
    }
    /// A left press, for hosts that dispatch the menu themselves: `true` when
    /// it landed on a slider row, which the host must then NOT treat as an
    /// action or a dismissal.
    pub fn slider_press(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().slider_press(px, py))
    }
    pub fn slider_dragging() -> bool {
        CONTEXT_MENU.with(|m| m.borrow().slider_drag.is_some())
    }
    pub fn slider_release() -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().slider_release())
    }
    /// `(row, value)` of the last slider change since the last call.
    pub fn take_slider_change() -> Option<(usize, f32)> {
        CONTEXT_MENU.with(|m| m.borrow_mut().take_slider_change())
    }

    pub fn mouse_input(button: MouseButton, state: ElementState, px: f32, py: f32, ctx: Option<&mut crate::context::UiContext>) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().mouse_input(button, state, px, py, ctx))
    }

    /// Paint the menu as a lit plate — see [`ContextMenuState::paint`]. Hosts on
    /// the display-list path call this in place of the [`extra_quads`] loop.
    pub fn paint(ctx: &mut crate::scene::paint::PaintCtx) {
        CONTEXT_MENU.with(|m| m.borrow().paint(ctx));
    }

    pub fn extra_quads() -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        CONTEXT_MENU.with(|m| m.borrow().extra_quads())
    }

    pub fn text_labels() -> Vec<TextLabel> {
        CONTEXT_MENU.with(|m| m.borrow().text_labels())
    }

    /// Plate and labels in one call — see
    /// [`ContextMenuState::paint_with_labels`].
    pub fn paint_with_labels(ctx: &mut crate::scene::paint::PaintCtx) {
        CONTEXT_MENU.with(|m| m.borrow().paint_with_labels(ctx));
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
    focus::clear_if_matches(w);
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
    use super::context_menu::{self, ContextMenuState, PAD, ROW_H};
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
            crate::backend::window_runner::shaped_cluster_offsets(&mut fs, "● Follow Active Editor", size, Some(&family))
                .last()
                .map(|&(_, t)| t)
                .unwrap()
        };
        assert!(drawn > 0.0);
        assert!(m.w >= drawn + 2.0 * PAD, "plate {} narrower than the drawn label {} plus pads", m.w, drawn);
    }
}
