use crate::widget::{Element, Key};

pub mod focus {
    use super:: Element;
    use std::cell::Cell;

    thread_local! {
        static FOCUSED_WIDGET: Cell<Option<*mut (dyn Element + 'static)>> = Cell::new(None);
    }

    pub fn set_focused(w: &mut dyn Element) {
        FOCUSED_WIDGET.with(|cell| {
            let new_ptr = unsafe {
                std::mem::transmute::<*mut dyn Element, *mut (dyn Element + 'static)>(w as *mut dyn Element)
            };
            if let Some(old_ptr) = cell.get() {
                let old_data = old_ptr as *mut () as usize;
                let new_data = new_ptr as *mut () as usize;
                if old_data != new_data {
                    unsafe {
                        (*old_ptr).unfocus();
                    }
                    cell.set(Some(new_ptr));
                }
            } else {
                cell.set(Some(new_ptr));
            }
        });
    }

    pub fn is_focused(w: &dyn Element) -> bool {
        FOCUSED_WIDGET.with(|cell| {
            if let Some(ptr) = cell.get() {
                let current_data = ptr as *const () as usize;
                let query_data = w as *const dyn Element as *const () as usize;
                current_data == query_data
            } else {
                false
            }
        })
    }

    pub fn clear_focus() {
        FOCUSED_WIDGET.with(|cell| {
            if let Some(ptr) = cell.take() {
                unsafe {
                    (*ptr).unfocus();
                }
            }
        });
    }

    pub fn clear_if_matches(w: &dyn Element) {
        FOCUSED_WIDGET.with(|cell| {
            if let Some(ptr) = cell.get() {
                let current_data = ptr as *const () as usize;
                let query_data = w as *const dyn Element as *const () as usize;
                if current_data == query_data {
                    cell.set(None);
                }
            }
        });
    }

    pub fn has_focus() -> bool {
        FOCUSED_WIDGET.with(|cell| cell.get().is_some())
    }

    pub fn link_parent_child(parent: &mut dyn Element, child: &mut dyn Element, ctx: &mut crate::context::UiContext) {
        let parent_ptr = unsafe {
            std::mem::transmute::<*mut dyn Element, *mut (dyn Element + 'static)>(parent as *mut dyn Element)
        };
        let child_ptr = unsafe {
            std::mem::transmute::<*mut dyn Element, *mut (dyn Element + 'static)>(child as *mut dyn Element)
        };
        if let (Some(p_base), Some(c_base)) = (parent.base(), child.base()) {
            ctx.register_widget(p_base.id(), parent_ptr);
            ctx.register_widget(c_base.id(), child_ptr);
        }
        parent.add_child(child_ptr, ctx);
        child.set_parent(Some(parent_ptr), ctx);
    }

    pub fn navigate_focus(key: &super::Key, ctrl: bool) -> bool {
        FOCUSED_WIDGET.with(|cell| {
            let ptr = match cell.get() {
                Some(p) => p,
                None => return false,
            };

            unsafe {
                match (key, ctrl) {
                    (super::Key::Character(c), true) if c == "u" || c == "U" => {
                        let dummy = crate::context::UiContext::new();
                        if let Some(parent_ptr) = (*ptr).parent(&dummy) {
                            let parent_ref = &mut *parent_ptr;
                            set_focused(parent_ref);
                            parent_ref.focus();
                            return true;
                        }
                    }
                    (super::Key::Character(c), true) if c == "i" || c == "I" => {
                        let dummy = crate::context::UiContext::new();
                        let mut children = (*ptr).children(&dummy);
                        if !children.is_empty() {
                            let child_ref = &mut *children[0];
                            set_focused(child_ref);
                            child_ref.focus();
                            return true;
                        }
                    }
                    (super::Key::Character(c), true) if c == "j" || c == "J" => {
                        let dummy = crate::context::UiContext::new();
                        if let Some(parent_ptr) = (*ptr).parent(&dummy) {
                            let mut siblings = (*parent_ptr).children(&dummy);
                            let current_idx = siblings.iter().position(|&x| {
                                let a = x as *mut () as usize;
                                let b = ptr as *mut () as usize;
                                a == b
                            });
                            if let Some(idx) = current_idx {
                                let next_idx = (idx + 1) % siblings.len();
                                let sibling_ref = &mut *siblings[next_idx];
                                set_focused(sibling_ref);
                                sibling_ref.focus();
                                return true;
                            }
                        }
                    }
                    (super::Key::Character(c), true) if c == "k" || c == "K" => {
                        let dummy = crate::context::UiContext::new();
                        if let Some(parent_ptr) = (*ptr).parent(&dummy) {
                            let mut siblings = (*parent_ptr).children(&dummy);
                            let current_idx = siblings.iter().position(|&x| {
                                let a = x as *mut () as usize;
                                let b = ptr as *mut () as usize;
                                a == b
                            });
                            if let Some(idx) = current_idx {
                                let prev_idx = if idx == 0 { siblings.len() - 1 } else { idx - 1 };
                                let sibling_ref = &mut *siblings[prev_idx];
                                set_focused(sibling_ref);
                                sibling_ref.focus();
                                return true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            false
        })
    }
}

pub mod popovers {
    use super:: Element;
    use std::cell::RefCell;

    thread_local! {
        static ACTIVE_POPOVERS: RefCell<Vec<*const (dyn Element + 'static)>> = RefCell::new(Vec::new());
    }

    pub fn clear() {
        ACTIVE_POPOVERS.with(|list| {
            list.borrow_mut().clear();
        });
    }

    pub fn register(w: &(dyn Element + 'static)) {
        ACTIVE_POPOVERS.with(|list| {
            let ptr = w as *const (dyn Element + 'static);
            let mut list = list.borrow_mut();
            if !list.contains(&ptr) {
                list.push(ptr);
            }
        });
    }

    pub fn get_active() -> Vec<*const (dyn Element + 'static)> {
        ACTIVE_POPOVERS.with(|list| {
            list.borrow().clone()
        })
    }

    pub fn is_coordinate_covered(query_address: usize, px: f32, py: f32) -> bool {
        ACTIVE_POPOVERS.with(|list| {
            let list = list.borrow();
            for popover_ptr in list.iter() {
                let current_data = *popover_ptr as *const () as usize;
                if query_address == current_data {
                    continue;
                }
                unsafe {
                    if let Some(popover) = popover_ptr.as_ref() {
                        if let Some((x, y, width, height)) = popover.popover_rect() {
                            if px >= x && px <= x + width && py >= y && py <= y + height {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        })
    }
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
                s.current_alpha += (s.target_alpha - s.current_alpha) * (1.0 - (-decay * dt).exp());
                changed = true;
            } else if s.current_alpha != s.target_alpha {
                s.current_alpha = s.target_alpha;
                changed = true;
            }

            if let (Some(tx), Some(ty), Some(tw), Some(th)) = (s.target_x, s.target_y, s.target_w, s.target_h) {
                if (s.current_x - tx).abs() > 0.1 {
                    s.current_x += (tx - s.current_x) * (1.0 - (-decay * dt).exp());
                    changed = true;
                } else if s.current_x != tx {
                    s.current_x = tx;
                    changed = true;
                }

                if (s.current_y - ty).abs() > 0.1 {
                    s.current_y += (ty - s.current_y) * (1.0 - (-decay * dt).exp());
                    changed = true;
                } else if s.current_y != ty {
                    s.current_y = ty;
                    changed = true;
                }

                if (s.current_w - tw).abs() > 0.1 {
                    s.current_w += (tw - s.current_w) * (1.0 - (-decay * dt).exp());
                    changed = true;
                } else if s.current_w != tw {
                    s.current_w = tw;
                    changed = true;
                }

                if (s.current_h - th).abs() > 0.1 {
                    s.current_h += (th - s.current_h) * (1.0 - (-decay * dt).exp());
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

    #[derive(Debug, Clone)]
    pub struct ContextMenuState {
        pub x: f32,
        pub y: f32,
        pub w: f32,
        pub h: f32,
        pub visible: bool,
        pub options: Vec<String>,
        pub hovered_item: Option<usize>,
        pub target: Option<*mut (dyn Element + 'static)>,
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
            }
        }

        pub fn show(&mut self, x: f32, y: f32, options: Vec<String>, target: *mut (dyn Element + 'static)) {
            self.x = x;
            self.y = y;
            self.options = options;
            self.h = self.options.len() as f32 * 24.0;
            let max_len = self.options.iter().map(|s| s.len()).max().unwrap_or(0);
            self.w = ((max_len as f32 * 7.5) + 24.0).max(120.0);
            self.visible = true;
            self.hovered_item = None;
            self.target = Some(target);
        }

        pub fn hide(&mut self) {
            self.visible = false;
            self.target = None;
        }

        pub fn hit_test(&self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h
        }

        pub fn cursor_moved(&mut self, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            let was_hovered = self.hovered_item;
            self.hovered_item = None;
            if px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h {
                let idx = ((py - self.y) / 24.0) as usize;
                if idx < self.options.len() && idx > 0 {
                    self.hovered_item = Some(idx);
                }
            }
            self.hovered_item != was_hovered
        }

        pub fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
            if !self.visible { return false; }
            if button != MouseButton::Left || state != ElementState::Pressed {
                if state == ElementState::Pressed {
                    self.hide();
                    return true;
                }
                return false;
            }

            if px >= self.x && px <= self.x + self.w && py >= self.y && py <= self.y + self.h {
                let idx = ((py - self.y) / 24.0) as usize;
                if idx < self.options.len() {
                    if idx > 0 {
                        let opt = self.options[idx].clone();
                        if let Some(target_ptr) = self.target {
                            unsafe {
                                let target = &mut *target_ptr;
                                match opt.as_str() {
                                    "Cut" => {
                                        let _ = target.cut_selection();
                                    }
                                    "Copy" => {
                                        target.copy_selection();
                                    }
                                    "Paste" => {
                                        let _ = target.paste_from_clipboard();
                                    }
                                    "Select All" => {
                                        target.select_all();
                                    }
                                    _ => {}
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

        pub fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
            let mut quads = Vec::new();
            if !self.visible { return quads; }

            // border
            quads.push((self.x, self.y, self.w, self.h, [0.22, 0.22, 0.28, 1.0]));
            // bg
            quads.push((self.x + 1.0, self.y + 1.0, self.w - 2.0, self.h - 2.0, [0.06, 0.06, 0.09, 1.0]));

            if let Some(h_idx) = self.hovered_item {
                let iy = self.y + h_idx as f32 * 24.0;
                quads.push((self.x + 2.0, iy + 2.0, self.w - 4.0, 20.0, [0.20, 0.40, 0.65, 0.6]));
            }

            quads
        }

        pub fn text_labels(&self) -> Vec<TextLabel> {
            let mut labels = Vec::new();
            if !self.visible { return labels; }

            for (idx, opt) in self.options.iter().enumerate() {
                let iy = self.y + idx as f32 * 24.0 + (24.0 - 12.0) / 2.0;
                let text_color = if idx == 0 {
                    [0x70, 0x70, 0x78]
                } else if self.hovered_item == Some(idx) {
                    [0xff, 0xff, 0xff]
                } else {
                    [0xcc, 0xcc, 0xd4]
                };

                labels.push(TextLabel {
                    text: opt.clone(),
                    x: self.x + 8.0,
                    y: iy,
                    font_size: 12.0,
                    color: text_color,
                });
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

    pub fn show(x: f32, y: f32, options: Vec<String>, target: *mut (dyn Element + 'static)) {
        CONTEXT_MENU.with(|m| m.borrow_mut().show(x, y, options, target));
    }

    pub fn hide() {
        CONTEXT_MENU.with(|m| m.borrow_mut().hide());
    }

    pub fn x() -> f32 { CONTEXT_MENU.with(|m| m.borrow().x) }
    pub fn y() -> f32 { CONTEXT_MENU.with(|m| m.borrow().y) }
    pub fn w() -> f32 { CONTEXT_MENU.with(|m| m.borrow().w) }
    pub fn h() -> f32 { CONTEXT_MENU.with(|m| m.borrow().h) }
    pub fn hovered_item() -> Option<usize> { CONTEXT_MENU.with(|m| m.borrow().hovered_item) }
    pub fn options() -> Vec<String> { CONTEXT_MENU.with(|m| m.borrow().options.clone()) }

    pub fn hit_test(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow().hit_test(px, py))
    }

    pub fn cursor_moved(px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().cursor_moved(px, py))
    }

    pub fn mouse_input(button: MouseButton, state: ElementState, px: f32, py: f32) -> bool {
        CONTEXT_MENU.with(|m| m.borrow_mut().mouse_input(button, state, px, py))
    }

    pub fn extra_quads() -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        CONTEXT_MENU.with(|m| m.borrow().extra_quads())
    }

    pub fn text_labels() -> Vec<TextLabel> {
        CONTEXT_MENU.with(|m| m.borrow().text_labels())
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

    pub fn label_offset(&self) -> f32 {
        if self.label.is_some() {
            12.0 + crate::layout::label_margin()
        } else {
            0.0
        }
    }
}


#[macro_export]
macro_rules! impl_widget_base {
    ($name:ident) => {
        fn base(&self) -> Option<&$crate::widget::Widget> { Some(&self.base) }
        fn base_mut(&mut self) -> Option<&mut $crate::widget::Widget> { Some(&mut self.base) }
        fn as_any(&self) -> &dyn std::any::Any { self }
        fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
        fn as_ptr(&self) -> *mut (dyn $crate::widget::Element + 'static) {
            self as *const Self as *mut Self as *mut (dyn $crate::widget::Element + 'static)
        }
    };
}
