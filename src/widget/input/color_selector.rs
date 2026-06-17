use crate::colors;
use crate::widget::*;

#[derive(Debug)]
pub struct ColorSelector {
    pub(crate) base: Widget,
    pub color: [u8; 3],
    pub alpha: u8,
    just_clicked: bool,
    pub editing: bool,
    pub(crate) edit_buffer: String,
    pub cursor_idx: usize,
    pub font_family: String,
    pub command: String,
    pub parent: Option<*mut (dyn Element + 'static)>,
    pub children: Vec<*mut (dyn Element + 'static)>,
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    pub with_alpha: bool,
}

impl Clone for ColorSelector {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            color: self.color,
            alpha: self.alpha,
            just_clicked: self.just_clicked,
            editing: self.editing,
            edit_buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            font_family: self.font_family.clone(),
            command: self.command.clone(),
            parent: self.parent,
            children: self.children.clone(),
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: self.editor_state.clone(),
            just_changed: self.just_changed,
            with_alpha: self.with_alpha,
        }
    }
}

impl ColorSelector {
    pub fn new(color: [u8; 3]) -> Self {
        Self {
            base: Widget::new(),
            color,
            alpha: 255,
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            font_family: crate::layout::color_selector_font(),
            command: "cce-color-interface".to_string(),
            parent: None,
            children: Vec::new(),
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            with_alpha: false,
        }
    }

    pub fn new_rgba(color: [u8; 4]) -> Self {
        Self {
            base: Widget::new(),
            color: [color[0], color[1], color[2]],
            alpha: color[3],
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            font_family: crate::layout::color_selector_font(),
            command: "cce-color-interface".to_string(),
            parent: None,
            children: Vec::new(),
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            with_alpha: true,
        }
    }

    pub fn with_alpha(mut self, with_alpha: bool) -> Self {
        self.with_alpha = with_alpha;
        self
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_font_family(mut self, font_family: &str) -> Self {
        self.font_family = font_family.to_string();
        self
    }

    pub fn with_command(mut self, command: &str) -> Self {
        self.command = command.to_string();
        self
    }
}

impl Element for ColorSelector {
    crate::impl_widget_base!(ColorSelector);

    fn get_value_string(&self) -> Option<String> {
        if self.with_alpha {
            Some(format!("#{:02x}{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2], self.alpha))
        } else {
            Some(format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]))
        }
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        if let Some(c) = parse_hex(val) {
            let target_color = [c[0], c[1], c[2]];
            let target_alpha = if self.with_alpha { c[3] } else { 255 };
            if self.color != target_color || (self.with_alpha && self.alpha != target_alpha) {
                self.color = target_color;
                self.alpha = target_alpha;
                self.just_changed = true;
                if self.editing {
                    self.edit_buffer = self.get_value_string().unwrap();
                    self.cursor_idx = self.edit_buffer.chars().count();
                }
                return true;
            }
        }
        false
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn preferred_height(&self) -> Option<f32> {
        Some(crate::layout::color_selector_height())
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::color_selector_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::color_selector_corner_radius()
    }

    fn widget_font(&self) -> Option<String> {
        Some(self.font_family.clone())
    }

    fn color_u8(&self) -> Option<[u8; 4]> {
        Some([self.color[0], self.color[1], self.color[2], self.alpha])
    }

    fn color(&self) -> [f32; 4] {
        colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            self.alpha as f32 / 255.0,
        ])
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button == MouseButton::Right && state == ElementState::Pressed {
            if self.hit_test(px, py, ctx) {
                ctx.handle_right_click(self.as_ptr(), px, py);
                return true;
            }
        }
        if button != MouseButton::Left { return false; }
        if state != ElementState::Pressed { return false; }
        if !self.hit_test(px, py, ctx) { return false; }
        if px >= self.base.x + self.base.w * 0.65 {
            let hex = self.get_value_string().unwrap();
            let mut child_guard = self.child.lock().unwrap();
            if let Some(mut old_child) = child_guard.take() {
                let _ = old_child.kill();
            }
            if let Ok(child) = std::process::Command::new(&self.command)
                .arg(&hex)
                .stdout(std::process::Stdio::piped())
                .spawn()
            {
                *child_guard = Some(child);
            }
            return true;
        }
        self.focus();
        true
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn tick(&mut self, _dt: f32, ctx: &mut UiContext) -> bool {
        let mut child_opt = self.child.lock().unwrap();
        if let Some(ref mut child) = *child_opt {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    let child = child_opt.take().unwrap();
                    if let Ok(output) = child.wait_with_output() {
                        let stdout_str = String::from_utf8_lossy(&output.stdout);
                        for line in stdout_str.lines().rev() {
                            if let Some(c) = parse_hex(line.trim()) {
                                self.color = [c[0], c[1], c[2]];
                                self.alpha = if self.with_alpha { c[3] } else { 255 };
                                self.just_clicked = true;
                                return true;
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("Error checking color selector child process: {:?}", e);
                    *child_opt = None;
                }
            }
        }
        false
    }

    fn focus(&mut self) {
        self.editing = true;
        self.edit_buffer = self.get_value_string().unwrap();
        self.cursor_idx = self.edit_buffer.chars().count();
        focus::set_focused(self);
    }

    fn unfocus(&mut self) {
        if self.editing {
            self.editing = false;
            if let Some(c) = parse_hex(&self.edit_buffer) {
                self.color = [c[0], c[1], c[2]];
                self.alpha = if self.with_alpha { c[3] } else { 255 };
            }
        }
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if !self.editing { return false; }
        if event.state != ElementState::Pressed { return false; }
        
        let mut state = TextEditorState {
            buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            select_anchor: None,
            all_selected: false,
        };
        
        let mut handled = false;
        match &event.logical_key {
            Key::Named(NamedKey::Backspace) => {
                state.delete_backwards();
                handled = true;
            }
            Key::Named(NamedKey::Delete) => {
                state.delete_forwards();
                handled = true;
            }
            Key::Named(NamedKey::ArrowLeft) => {
                state.move_cursor_left(false);
                handled = true;
            }
            Key::Named(NamedKey::ArrowRight) => {
                state.move_cursor_right(false);
                handled = true;
            }
            Key::Named(NamedKey::Enter) => {
                if let Some(c) = parse_hex(&state.buffer) {
                    self.color = [c[0], c[1], c[2]];
                    self.alpha = if self.with_alpha { c[3] } else { 255 };
                }
                self.editing = false;
                handled = true;
            }
            Key::Named(NamedKey::Escape) => {
                self.editing = false;
                handled = true;
            }
            _ => {
                if let Some(text) = &event.text {
                    for ch in text.chars() {
                        match ch {
                            '#' => {
                                if state.buffer.is_empty() {
                                    state.insert_text("#");
                                    handled = true;
                                } else if state.cursor_idx == 0 && !state.buffer.starts_with('#') {
                                    state.insert_text("#");
                                    handled = true;
                                }
                            }
                            '0'..='9' | 'a'..='f' | 'A'..='F' => {
                                let count = state.buffer.chars().count();
                                let max_len = if state.buffer.starts_with('#') {
                                    if self.with_alpha { 9 } else { 7 }
                                } else {
                                    if self.with_alpha { 8 } else { 6 }
                                };
                                if count < max_len {
                                    state.insert_text(&ch.to_ascii_lowercase().to_string());
                                    handled = true;
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        
        if self.editing {
            self.edit_buffer = state.buffer;
            self.cursor_idx = state.cursor_idx;
        }
        handled
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        let pick_x = self.base.x + self.base.w * 0.65;
        let pick_w = self.base.w * 0.35;

        let bg_color = if self.editing {
            [0.06, 0.10, 0.18, 1.0]
        } else {
            [0.08, 0.08, 0.12, 1.0]
        };
        let border_color = if self.editing {
            [0.20, 0.50, 0.85, 1.0]
        } else if self.base.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        quads.push((self.base.x, self.base.y + top, self.base.w, visual_h, border_color));
        quads.push((self.base.x + 1.0, self.base.y + top + 1.0, self.base.w - 2.0, visual_h - 2.0, bg_color));

        if self.editing {
            let font_size = 12.0;
            let cursor_text: String = self.edit_buffer.chars().take(self.cursor_idx).collect();
            let text_w = TextLabel::estimate_width(&cursor_text, font_size);
            let caret_x = self.base.x + 4.0 + text_w;
            let caret_h = font_size * 1.15;
            let caret_y = self.base.y + top + (visual_h - caret_h) / 2.0;
            quads.push((caret_x, caret_y, 1.5, caret_h, [0.80, 0.80, 0.85, 1.0]));
        }

        let linear_c = colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            self.alpha as f32 / 255.0,
        ]);
        let border_w = 1.0;
        let border_c = colors::color_borders_color();
        
        let r = border_c[0];
        let g = border_c[1];
        let b = border_c[2];

        let preview_radius = crate::layout::color_selector_preview_corner_radius();
        let preview_margin = crate::layout::color_selector_preview_margin();

        let px = pick_x + preview_margin;
        let py = self.base.y + top + preview_margin;
        let pw = (pick_w - 2.0 * preview_margin).max(0.0);
        let ph = (visual_h - 2.0 * preview_margin).max(0.0);

        let add_rounded_rect = |quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32| {
            let radius = radius.min(w * 0.5).min(h * 0.5);
            if radius <= 0.5 {
                quads.push((x, y, w, h, color));
                return;
            }
            
            quads.push((x + radius, y, w - 2.0 * radius, h, color));
            quads.push((x, y + radius, radius, h - 2.0 * radius, color));
            quads.push((x + w - radius, y + radius, radius, h - 2.0 * radius, color));
            
            let steps = radius.round() as i32;
            for i in 0..steps {
                let dy = i as f32;
                let next_dy = (i + 1) as f32;
                
                let cx = (radius * radius - (radius - dy) * (radius - dy)).sqrt();
                let next_cx = (radius * radius - (radius - next_dy) * (radius - next_dy)).sqrt();
                let avg_cx = (cx + next_cx) * 0.5;
                
                let strip_w = avg_cx;
                let strip_h = 1.0f32;
                
                if strip_w > 0.0 {
                    quads.push((x + radius - strip_w, y + dy, strip_w, strip_h, color));
                    quads.push((x + w - radius, y + dy, strip_w, strip_h, color));
                    quads.push((x + radius - strip_w, y + h - dy - strip_h, strip_w, strip_h, color));
                    quads.push((x + w - radius, y + h - dy - strip_h, strip_w, strip_h, color));
                }
            }
        };

        let steps = 6;
        for i in (1..=steps).rev() {
            let offset = i as f32 * 0.75;
            let rx = px - offset;
            let ry = py - offset;
            let rw = pw + 2.0 * offset;
            let rh = ph + 2.0 * offset;
            let alpha = 0.08 * (1.0 - (i as f32 / steps as f32).powf(1.5));
            if alpha > 0.001 {
                add_rounded_rect(&mut quads, [r, g, b, alpha], rx, ry, rw, rh, preview_radius + offset);
            }
        }

        add_rounded_rect(&mut quads, border_c, px, py, pw, ph, preview_radius);

        if self.with_alpha {
            add_rounded_rect(
                &mut quads,
                [0.8, 0.8, 0.8, 1.0],
                px + border_w,
                py + border_w,
                (pw - 2.0 * border_w).max(0.0),
                (ph - 2.0 * border_w).max(0.0),
                (preview_radius - border_w).max(0.0),
            );
            
            let grid_size = 6.0;
            let start_x = px + border_w;
            let start_y = py + border_w;
            let inner_w = (pw - 2.0 * border_w).max(0.0);
            let inner_h = (ph - 2.0 * border_w).max(0.0);
            
            let cols = (inner_w / grid_size).ceil() as i32;
            let rows = (inner_h / grid_size).ceil() as i32;
            for r in 0..rows {
                for c in 0..cols {
                    if (r + c) % 2 == 1 {
                        let qx = start_x + c as f32 * grid_size;
                        let qy = start_y + r as f32 * grid_size;
                        let qw = grid_size.min(start_x + inner_w - qx);
                        let qh = grid_size.min(start_y + inner_h - qy);
                        if qw > 0.0 && qh > 0.0 {
                            quads.push((qx, qy, qw, qh, [1.0, 1.0, 1.0, 1.0]));
                        }
                    }
                }
            }
        }

        add_rounded_rect(
            &mut quads,
            linear_c,
            px + border_w,
            py + border_w,
            (pw - 2.0 * border_w).max(0.0),
            (ph - 2.0 * border_w).max(0.0),
            (preview_radius - border_w).max(0.0),
        );
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(lbl) = self.control_label() {
            labels.push(lbl);
        }
        let hex = if self.editing { self.edit_buffer.clone() } else { self.get_value_string().unwrap() };
        let top = self.base.label_offset();
        let visual_h = self.base.h - top;
        labels.push(TextLabel {
            text: hex,
            x: self.base.x + 4.0,
            y: crate::layout::align_text_y(self.base.y, self.base.h, 12.0, top),
            font_size: 12.0,
            color: [0xcc, 0xcc, 0xd4],
        });
        labels
    }
}

impl Drop for ColorSelector {
    fn drop(&mut self) {
        focus::clear_if_matches(self);
    }
}

fn parse_hex(s: &str) -> Option<[u8; 4]> {
    let s = s.trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b, 255])
    } else if s.len() == 8 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        let a = u8::from_str_radix(&s[6..8], 16).ok()?;
        Some([r, g, b, a])
    } else {
        None
    }
}

impl Control for ColorSelector {}

unsafe impl Send for ColorSelector {}
unsafe impl Sync for ColorSelector {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colorselector_keyboard_navigation() {
        let mut dummy = crate::context::UiContext::new();
        let mut cs = ColorSelector::new([255, 0, 0]);
        assert_eq!(cs.color, [255, 0, 0]);
        assert_eq!(cs.alpha, 255);
        assert!(!cs.editing);

        // 1. Focus color selector
        cs.focus();
        assert!(cs.editing);
        assert_eq!(cs.edit_buffer, "#ff0000");
        assert_eq!(cs.cursor_idx, 7);

        // 2. Backspace deletes character before cursor
        let backspace_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Backspace),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = cs.keyboard_input(&backspace_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.edit_buffer, "#ff000");
        assert_eq!(cs.cursor_idx, 6);

        // 3. ArrowLeft moves cursor index
        let left_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowLeft),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = cs.keyboard_input(&left_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.cursor_idx, 5);

        // 4. Backspace at cursor index 5
        let handled = cs.keyboard_input(&backspace_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.edit_buffer, "#ff00");
        assert_eq!(cs.cursor_idx, 4);

        // 5. ArrowRight moves cursor index
        let right_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowRight),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = cs.keyboard_input(&right_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.cursor_idx, 5);

        // 6. Delete deletes character at cursor
        let delete_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Delete),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        // Move cursor to index 3
        let handled = cs.keyboard_input(&left_ev, &mut dummy); // 4
        assert!(handled);
        let handled = cs.keyboard_input(&left_ev, &mut dummy); // 3
        assert!(handled);
        assert_eq!(cs.cursor_idx, 3);
        // buffer is "#ff0", index 3 points to the first '0'. Let's delete it.
        let handled = cs.keyboard_input(&delete_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.edit_buffer, "#ff0");
        assert_eq!(cs.cursor_idx, 3);

        // 7. Typing hex character inserts at cursor index
        let type_b_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("b".to_string()),
            text: Some("b".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = cs.keyboard_input(&type_b_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.edit_buffer, "#ffb0");
        assert_eq!(cs.cursor_idx, 4);

        // Move to index 1 and insert a 'c'
        for _ in 0..3 {
            let handled = cs.keyboard_input(&left_ev, &mut dummy);
            assert!(handled);
        }
        assert_eq!(cs.cursor_idx, 1);
        let type_c_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("c".to_string()),
            text: Some("c".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = cs.keyboard_input(&type_c_ev, &mut dummy);
        assert!(handled);
        assert_eq!(cs.edit_buffer, "#cffb0");
        assert_eq!(cs.cursor_idx, 2);

        // 8. Enter commits the color
        let type_5_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("5".to_string()),
            text: Some("5".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        // Move to index 5
        for _ in 0..4 {
            cs.keyboard_input(&right_ev, &mut dummy);
        }
        assert_eq!(cs.cursor_idx, 6);
        cs.keyboard_input(&type_5_ev, &mut dummy);
        assert_eq!(cs.edit_buffer, "#cffb05");
        assert_eq!(cs.cursor_idx, 7);

        let enter_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let handled = cs.keyboard_input(&enter_ev, &mut dummy);
        assert!(handled);
        assert!(!cs.editing);
        assert_eq!(cs.color, [0xcf, 0xfb, 0x05]);
        assert_eq!(cs.alpha, 255);
    }

    #[test]
    fn test_colorselector_alpha_support() {
        let mut dummy = crate::context::UiContext::new();
        let mut cs = ColorSelector::new_rgba([255, 0, 0, 128]);
        assert_eq!(cs.color, [255, 0, 0]);
        assert_eq!(cs.alpha, 128);
        assert!(cs.with_alpha);

        cs.focus();
        assert!(cs.editing);
        assert_eq!(cs.edit_buffer, "#ff000080");
        assert_eq!(cs.cursor_idx, 9);

        let type_a_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("a".to_string()),
            text: Some("a".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        let backspace_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Backspace),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        cs.keyboard_input(&backspace_ev, &mut dummy);
        cs.keyboard_input(&backspace_ev, &mut dummy);
        assert_eq!(cs.edit_buffer, "#ff0000");

        let type_b_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Character("b".to_string()),
            text: Some("b".to_string()),
            repeat: false,
            ctrl: false,
            shift: false,
        };
        cs.keyboard_input(&type_a_ev, &mut dummy);
        cs.keyboard_input(&type_b_ev, &mut dummy);
        assert_eq!(cs.edit_buffer, "#ff0000ab");

        let enter_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Enter),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        cs.keyboard_input(&enter_ev, &mut dummy);
        assert!(!cs.editing);
        assert_eq!(cs.color, [255, 0, 0]);
        assert_eq!(cs.alpha, 171);
    }
}

