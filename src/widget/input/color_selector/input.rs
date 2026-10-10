//! A `ColorSelector`'s input: editing the hex, a press on the swatch launching the picker, the tick
//! that reads the picker's stream, and the text as an accessibility reader sees it.

use super::*;

impl Input for ColorSelector {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Well
    }
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

    fn a11y_text(&self) -> Option<crate::a11y::A11yText> {
        // The hex field: what it shows, its caret while it is being typed into.
        let text = if self.editing { self.edit_buffer.clone() } else { self.value_hex() };
        let caret = self.cursor_idx.min(text.chars().count());
        Some(crate::a11y::A11yText {
            text,
            selection: self.editing.then_some((caret, caret)),
            editable: true,
            kind: Some(crate::l10n::tr("a11y-colour")),
            ..Default::default()
        })
    }

    fn a11y_set_text(&mut self, text: &str) -> bool {
        // A colour, as the field takes one when typed: a hex it parses, or nothing.
        let text = text.trim();
        if self.set_value_string(text) {
            return true;
        }
        // The colour it already holds, set while a half-typed hex is up: the field shows it.
        if self.editing && parse_hex(text).is_some() && self.edit_buffer != self.value_hex() {
            self.edit_buffer = self.value_hex();
            self.cursor_idx = self.edit_buffer.chars().count();
            return true;
        }
        false
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
            // A live picker session is activity: the runner sleeps between
            // ticks when nothing is animating, and these lines come from a
            // reader thread it cannot see, so keep the frame cadence for as
            // long as the picker is open.
            redraw = true;
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

            place_picker_at_pointer(&self.command);

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
                                // A "#" only leads the value.
                                if state.buffer.is_empty() || (state.cursor_idx == 0 && !state.buffer.starts_with('#')) {
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
