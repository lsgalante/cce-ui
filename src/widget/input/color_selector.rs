use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::*;

#[derive(Debug)]
pub struct ColorSelector {
    pub color: [u8; 3],
    pub alpha: u8,
    just_clicked: bool,
    pub editing: bool,
    pub(crate) edit_buffer: String,
    pub cursor_idx: usize,
    pub font_family: String,
    pub command: String,
    hovered: bool,
    child: std::sync::Arc<std::sync::Mutex<Option<std::process::Child>>>,
    pub editor_state: TextEditorState,
    pub just_changed: bool,
    pub with_alpha: bool,
    /// Live color lines from the running picker (`cce-color-editor --stream` prints
    /// every change), forwarded by a reader thread — the value applies while
    /// the editor stays open instead of on exit.
    live_rx: Option<std::sync::mpsc::Receiver<String>>,
    /// The value at picker launch, restored when the stream reports `cancel`.
    revert_hex: Option<String>,
}

impl Clone for ColorSelector {
    fn clone(&self) -> Self {
        Self {
            color: self.color,
            alpha: self.alpha,
            just_clicked: self.just_clicked,
            editing: self.editing,
            edit_buffer: self.edit_buffer.clone(),
            cursor_idx: self.cursor_idx,
            font_family: self.font_family.clone(),
            command: self.command.clone(),
            hovered: self.hovered,
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: self.editor_state.clone(),
            just_changed: self.just_changed,
            with_alpha: self.with_alpha,
            live_rx: None,
            revert_hex: None,
        }
    }
}

impl ColorSelector {
    pub fn new(color: [u8; 3]) -> Adapted<ColorSelector> {
        Adapted::new(ColorSelector {
            color,
            alpha: 255,
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            font_family: crate::layout::color_selector_font(),
            command: "cce-color-editor".to_string(),
            hovered: false,
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            with_alpha: false,
            live_rx: None,
            revert_hex: None,
        })
    }

    pub fn new_rgba(color: [u8; 4]) -> Adapted<ColorSelector> {
        Adapted::new(ColorSelector {
            color: [color[0], color[1], color[2]],
            alpha: color[3],
            just_clicked: false,
            editing: false,
            edit_buffer: String::new(),
            cursor_idx: 0,
            font_family: crate::layout::color_selector_font(),
            command: "cce-color-editor".to_string(),
            hovered: false,
            child: std::sync::Arc::new(std::sync::Mutex::new(None)),
            editor_state: TextEditorState::new(String::new()),
            just_changed: false,
            with_alpha: true,
            live_rx: None,
            revert_hex: None,
        })
    }

}

impl Adapted<ColorSelector> {
    pub fn with_alpha(mut self, with_alpha: bool) -> Self {
        self.with_alpha = with_alpha;
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

impl ColorSelector {
    fn value_hex(&self) -> String {
        if self.with_alpha {
            Some(format!("#{:02x}{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2], self.alpha))
        } else {
            Some(format!("#{:02x}{:02x}{:02x}", self.color[0], self.color[1], self.color[2]))
        }
    
        .unwrap()
    }

    fn begin_edit(&mut self) {
        self.editing = true;
        self.edit_buffer = self.value_hex();
        self.cursor_idx = self.edit_buffer.chars().count();
    }
}

impl Layout for ColorSelector {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(0.0, crate::layout::color_selector_height()))
    }
}

impl Paint for ColorSelector {
    fn color(&self) -> [f32; 4] {
        colors::to_linear([
            self.color[0] as f32 / 255.0,
            self.color[1] as f32 / 255.0,
            self.color[2] as f32 / 255.0,
            self.alpha as f32 / 255.0,
        ])
    }

    fn widget_font(&self) -> Option<String> {
        Some(self.font_family.clone())
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::color_selector_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    /// The legacy `extra_quads` body against the laid-out rect (field, caret while
    /// editing, and the soft-glow rounded color preview), plus the hex readout label.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The well is color_selector_height tall, seated at the rect's TOP —
        // the label rides above the rect (inflating-label convention) and the
        // row's bottom band belongs to the NEXT row's label. Hosts that hand
        // over a whole param row (ParametersBg's 40px color rows) get a
        // standard control-height well instead of a row-tall one.
        let well_h = crate::layout::color_selector_height().min(rect.height);
        let rect = Rect { x: rect.x, y: rect.y, width: rect.width, height: well_h };
        let mut quads: Vec<(f32, f32, f32, f32, [f32; 4])> = Vec::new();
        let visual_h = rect.height;
        let pick_x = rect.x + rect.width * 0.65;
        let pick_w = rect.width * 0.35;

        // The text field has NO face of its own — a frame over the host plate,
        // like a relief TextBox well (transparent fill, the outline defines
        // it) and the closed-dropdown convention. The first colorless pass
        // used the textbox background here, but under the DE's relief themes
        // real text wells draw no fill, so even a neutral one read as "the
        // color selector has a background". Neutral greys for the frame; the
        // caret is the editing affordance.
        let border_color = if self.editing {
            [0.45, 0.45, 0.52, 1.0]
        } else if self.hovered {
            [0.25, 0.25, 0.35, 1.0]
        } else {
            [0.18, 0.18, 0.24, 1.0]
        };

        // A real frame, not a border-quad-under-fill-quad: with no fill, the
        // old full-rect border quad would read as a solid slab.
        let bw = 1.0;
        quads.push((rect.x, rect.y, rect.width, bw, border_color));
        quads.push((rect.x, rect.y + visual_h - bw, rect.width, bw, border_color));
        quads.push((rect.x, rect.y, bw, visual_h, border_color));
        quads.push((rect.x + rect.width - bw, rect.y, bw, visual_h, border_color));

        if self.editing {
            let font_size = 12.0;
            let cursor_text: String = self.edit_buffer.chars().take(self.cursor_idx).collect();
            let text_w = crate::widget::display::measure_text(&cursor_text, font_size);
            let caret_x = rect.x + 4.0 + text_w;
            let caret_h = font_size * 1.15;
            let caret_y = rect.y + (visual_h - caret_h) / 2.0;
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
        let py = rect.y + preview_margin;
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
    
        for (qx, qy, qw, qh, qc) in quads {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }

        let hex = if self.editing { self.edit_buffer.clone() } else { self.value_hex() };
        ctx.text(
            hex,
            rect.x + 4.0,
            crate::layout::align_text_y(rect.y, rect.height, 12.0, 0.0),
            12.0,
            [0xcc, 0xcc, 0xd4],
        );
    }
}

impl Input for ColorSelector {
    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn value_string(&self) -> Option<String> {
        Some(self.value_hex())
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
                    self.edit_buffer = self.value_hex();
                    self.cursor_idx = self.edit_buffer.chars().count();
                }
                return true;
            }
        }
        false
    
    }

    fn wants_tick(&self) -> bool {
        true
    }

    fn tick(&mut self, _dt: f32, _rect: Rect) -> bool {
        // Apply streamed picker lines as they arrive — the color changes live
        // while the editor stays open. The picker's Apply prints a final line
        // (already applied here); Cancel prints `cancel`, restoring the value
        // the picker launched with.
        let mut redraw = false;
        if let Some(rx) = &self.live_rx {
            let mut lines = Vec::new();
            let mut disconnected = false;
            loop {
                match rx.try_recv() {
                    Ok(line) => lines.push(line),
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            if disconnected {
                self.live_rx = None;
            }
            for line in lines {
                let line = line.trim().to_string();
                let target = if line == "cancel" {
                    self.revert_hex.clone().and_then(|h| parse_hex(&h))
                } else {
                    parse_hex(&line)
                };
                if let Some(c) = target {
                    let color = [c[0], c[1], c[2]];
                    let alpha = if self.with_alpha { c[3] } else { 255 };
                    if self.color != color || self.alpha != alpha {
                        self.color = color;
                        self.alpha = alpha;
                        self.just_changed = true;
                        redraw = true;
                    }
                }
            }
        }
        let mut child_opt = self.child.lock().unwrap();
        if let Some(ref mut child) = *child_opt {
            match child.try_wait() {
                Ok(Some(_status)) => {
                    // The reader thread owns stdout and has already forwarded
                    // every line (including Apply's final one) — just reap.
                    *child_opt = None;
                }
                Ok(None) => {}
                Err(e) => {
                    log::error!("Error checking color selector child process: {:?}", e);
                    *child_opt = None;
                }
            }
        }
        redraw
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button, state, x, y, .. } => {
                if *button != MouseButton::Left {
                    return false;
                }
                if *state != ElementState::Pressed {
                    return false;
                }
                let (px, py) = (*x, *y);
                let _ = py;
                let rect = ectx.rect;
                if px >= rect.x + rect.width * 0.65 {
            let hex = self.value_hex();
            let mut child_guard = self.child.lock().unwrap();
            if let Some(mut old_child) = child_guard.take() {
                let _ = old_child.kill();
            }

            let cmd_path = if let Ok(mut exe_path) = std::env::current_exe() {
                exe_path.pop(); // remove executable name
                let local_path = exe_path.join(&self.command);
                if local_path.exists() {
                    local_path.to_string_lossy().into_owned()
                } else {
                    let home = std::env::var("HOME").unwrap_or_default();
                    let local_bin = std::path::Path::new(&home).join(".local/bin").join(&self.command);
                    if local_bin.exists() {
                        local_bin.to_string_lossy().into_owned()
                    } else {
                        self.command.clone()
                    }
                }
            } else {
                self.command.clone()
            };

            // Ask the compositor to open the picker at this control instead of
            // its remembered position: the pointer is on the swatch right now,
            // so its location IS the control's location. One-shot, best-effort
            // (`place-next` consumed at the picker's map; ignored off-cce).
            if let Ok(reply) = crate::ipc::send_command("cce", "pointer-location") {
                let mut px = None;
                let mut py = None;
                for tok in reply.split_whitespace() {
                    if let Some(v) = tok.strip_prefix("x=") {
                        px = v.parse::<f64>().ok();
                    } else if let Some(v) = tok.strip_prefix("y=") {
                        py = v.parse::<f64>().ok();
                    }
                }
                if let (Some(x), Some(y)) = (px, py) {
                    let app_id = std::path::Path::new(&self.command)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| self.command.clone());
                    let _ = crate::ipc::send_command(
                        "cce",
                        &format!("place-next {} {:.0} {:.0}", app_id, x, y),
                    );
                }
            }

            let mut cmd = std::process::Command::new(&cmd_path);
            cmd.arg(&hex);
            if self.with_alpha {
                cmd.arg("--alpha");
            }
            // Live picking: the picker streams every change on stdout; a
            // reader thread forwards lines so `tick` applies them while the
            // editor stays open. `cancel` restores the launch value.
            cmd.arg("--stream");
            if let Ok(mut child) = cmd.stdout(std::process::Stdio::piped()).spawn() {
                if let Some(stdout) = child.stdout.take() {
                    let (tx, rx) = std::sync::mpsc::channel::<String>();
                    std::thread::spawn(move || {
                        use std::io::BufRead;
                        let reader = std::io::BufReader::new(stdout);
                        for line in reader.lines().map_while(Result::ok) {
                            if tx.send(line).is_err() {
                                break;
                            }
                        }
                    });
                    self.live_rx = Some(rx);
                    self.revert_hex = Some(hex.clone());
                }
                *child_guard = Some(child);
            }
                    return true;
                }
                ectx.request_focus();
                true
            }
            Event::KeyInput(event) => {
                if event.state != ElementState::Pressed {
                    return false;
                }
                if !self.editing {
                    return false;
                }


        
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
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.begin_edit();
                false
            }
            Event::FocusOut => {
                if self.editing {
                    self.editing = false;
                    if let Some(c) = parse_hex(&self.edit_buffer) {
                        self.color = [c[0], c[1], c[2]];
                        self.alpha = if self.with_alpha { c[3] } else { 255 };
                    }
                }
                false
            }
            _ => false,
        }
    }
}

fn parse_hex(s: &str) -> Option<[u8; 4]> {
    crate::color::parse_hex_bytes(s)
}

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
            alt: false,
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
            alt: false,
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
            alt: false,
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
            alt: false,
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
            alt: false,
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
            alt: false,
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
            alt: false,
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
            alt: false,
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
            alt: false,
        };
        let backspace_ev = KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::Backspace),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
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
            alt: false,
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
            alt: false,
        };
        cs.keyboard_input(&enter_ev, &mut dummy);
        assert!(!cs.editing);
        assert_eq!(cs.color, [255, 0, 0]);
        assert_eq!(cs.alpha, 171);
    }
}

