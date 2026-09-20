//! `cce-ramp` — a minimal popup hosting the [`Ramp`] widget in isolation, for
//! iterating on the widget's look without driving a full client around it.
//! `make install` puts it on PATH; run it inside a Wayland session. Edits print
//! their ramp spec to stdout, so the popup doubles as a curve scratchpad.
//!
//! `--key <dotted.key>` (with optional `--config <path>`, default the shared
//! config.kdl) turns the scratchpad into the `(ramp)` VALUE editor: the curve
//! seeds from that key's ramp spec, Save writes the spec back to that one key
//! with the `(ramp)` annotation, and Cancel closes without saving — the
//! `cce-relief --key` convention. cce-data-editor spawns it this way from a
//! ramp value's inline preview.
//!
//! Architecture mirrors the reference `DemoApp` (`src/main.rs`): display-list
//! frame, routed events, in-frame popovers.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::widget::{
    Adapted, Button, ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, Ramp,
    WidgetHost,
};
use wayland_client::QueueHandle;

/// Transparent rim between the surface edge and the plate: room for the ramp's
/// key pegs (r=28, +45 selected halo) to render outside the window frame
/// instead of being clipped at the buffer edge.
const OVERFLOW_MARGIN: f32 = 40.0;

#[derive(Debug, Clone)]
enum RampMsg {
    Exit,
}

struct RampPopup {
    ramp: Adapted<Ramp>,
    /// Last spec printed to stdout — edits log their curve for copy/paste.
    last_spec: String,
    /// `--key` mode only; parked off-screen in the scratchpad.
    save_button: Adapted<Button>,
    cancel_button: Adapted<Button>,
    /// `--key <dotted.key>`: Save writes the spec as a `(ramp)` value at
    /// this key; the curve seeds from it. None = the stdout scratchpad.
    target_key: Option<String>,
    config_path: std::path::PathBuf,
    /// Status line under the buttons (key mode): what the last save did.
    status: String,
    /// Set by the cancel click in `drain_changes` (no exit access there);
    /// `handle_mouse_input` turns it into `RampMsg::Exit`.
    exit_requested: bool,
    ui_context: cce_ui::context::UiContext,
    width: u32,
    height: u32,
    scale_factor: f64,
    needs_rebuild: bool,
    registered: bool,
}

impl RampPopup {
    fn drain_changes(&mut self) {
        let spec = self.ramp.inner().spec_string();
        if spec != self.last_spec {
            println!("{spec}");
            self.last_spec = spec;
            self.needs_rebuild = true;
        }
        if self.save_button.take_click() {
            self.save_to_key();
            self.needs_rebuild = true;
        }
        if self.cancel_button.take_click() {
            // Discard-and-close: nothing persisted without Save.
            self.exit_requested = true;
        }
    }

    /// Persist the current curve as a `(ramp)` value at the target key.
    fn save_to_key(&mut self) {
        let Some(key) = self.target_key.clone() else { return };
        let p = self.config_path.to_string_lossy().into_owned();
        let spec = self.ramp.inner().spec_string();
        let ok = cce_ui::config::write_config_value_typed(&p, &key, &spec, "style", Some("ramp"));
        self.status = if ok {
            println!("saved {key} -> {p}");
            format!("Saved — {key} holds this curve.")
        } else {
            "Save FAILED — see config permissions.".to_string()
        };
    }
}

impl Application for RampPopup {
    type Message = RampMsg;

    fn new(
        _qh: &QueueHandle<EngineState<Self>>,
        _sender: calloop::channel::Sender<Self::Message>,
    ) -> Self {
        cce_ui::scale::set_scale_factor(1.0);
        let mut ramp = Ramp::new();

        // `--key <dotted.key>` / `--config <path>`: edit one `(ramp)` value
        // in place — seed the curve from it, Save writes it back.
        let mut config_path = cce_ui::config::get_config_path();
        let mut target_key: Option<String> = None;
        let args: Vec<String> = std::env::args().collect();
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--config" && i + 1 < args.len() {
                config_path = std::path::PathBuf::from(&args[i + 1]);
                i += 1;
            } else if args[i] == "--key" && i + 1 < args.len() {
                target_key = Some(args[i + 1].clone());
                i += 1;
            }
            i += 1;
        }
        if let Some(key) = &target_key {
            let seed = std::fs::read_to_string(&config_path)
                .ok()
                .map(|c| cce_ui::config::parse_kdl_to_json(&c))
                .and_then(|v| v.pointer(&format!("/{}", key.replace('.', "/"))).cloned())
                .and_then(|v| v.as_str().map(String::from));
            if let Some(spec) = seed {
                ramp.inner_mut().set_spec(&spec);
            }
        }

        let last_spec = ramp.inner().spec_string();
        Self {
            ramp,
            last_spec,
            save_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Save"),
            cancel_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Cancel"),
            status: match &target_key {
                Some(k) => format!("Edits are live in the curve; Save writes the {k} key."),
                None => String::new(),
            },
            target_key,
            config_path,
            exit_requested: false,
            ui_context: cce_ui::context::UiContext::new(),
            width: 540,
            height: 420,
            scale_factor: 1.0,
            needs_rebuild: true,
            registered: false,
        }
    }

    // Buffer-larger-than-geometry mode: the runner publishes the plate rect
    // as the xdg window geometry + input region, so key pegs painted on the
    // rim render outside the window frame and clicks there fall through.
    fn overflow_margin(&self) -> u32 {
        OVERFLOW_MARGIN as u32
    }

    fn settings(&self) -> WindowSettings {
        let title = match &self.target_key {
            Some(k) => {
                let parts: Vec<&str> = k.split('.').collect();
                format!("Ramp — {}", parts[parts.len().saturating_sub(2)..].join("."))
            }
            None => "Ramp".to_string(),
        };
        // The key mode adds a Save/Cancel row under the curve.
        let extra = if self.target_key.is_some() { 56 } else { 0 };
        WindowSettings {
            title,
            app_id: "cce-ramp".to_string(),
            width: 460,
            height: 340 + extra,
            fullscreen: false,
            min_size: Some((420, 300 + extra)),
        }
    }

    fn update(&mut self, msg: Self::Message, _needs_rebuild: &mut bool, exit: &mut bool) {
        match msg {
            RampMsg::Exit => *exit = true,
        }
    }

    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
        if self.ui_context.tick(dt) {
            // Tick-driven edits (hover-scroll glide) log their spec too.
            self.drain_changes();
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn display_list(&mut self, size: LogicalSize, scale: f64) -> Option<DisplayList> {
        if !self.registered {
            self.registered = true;
            let w = self.ramp.as_ptr_mut();
            let id = self.ramp.id();
            self.ui_context.register_widget(id, w);
            self.ui_context.register_widget(self.save_button.id(), self.save_button.as_ptr_mut());
            self.ui_context.register_widget(self.cancel_button.id(), self.cancel_button.as_ptr_mut());
        }

        let size_changed = self.width != size.width as u32
            || self.height != size.height as u32
            || self.scale_factor != scale;
        if self.needs_rebuild || size_changed {
            self.width = size.width as u32;
            self.height = size.height as u32;
            self.scale_factor = scale;
            cce_ui::scale::set_scale_factor(scale as f32);

            // One widget, one rect: the ramp fills the plate inside half the
            // DE pad (this popup runs tighter than a full client). The plate
            // itself is inset by OVERFLOW_MARGIN so key pegs can render past
            // the window frame into the transparent surface rim. Key mode
            // reserves a Save/Cancel band under the curve.
            let pad = OVERFLOW_MARGIN + cce_ui::layout::root_plate_padding() / 2.0;
            let band = if self.target_key.is_some() { 56.0 } else { 0.0 };
            self.ramp.set_rect(
                pad,
                pad,
                (self.width as f32 - 2.0 * pad).max(0.0),
                (self.height as f32 - 2.0 * pad - band).max(0.0),
            );
            if self.target_key.is_some() {
                let by = self.height as f32 - pad - 30.0;
                self.save_button.set_rect(pad, by, 96.0, 28.0);
                self.cancel_button.set_rect(pad + 96.0 + 12.0, by, 96.0, 28.0);
            } else {
                self.save_button.set_rect(-1000.0, -1000.0, 1.0, 1.0);
                self.cancel_button.set_rect(-1000.0, -1000.0, 1.0, 1.0);
            }

            self.needs_rebuild = false;
            self.ui_context.rebuild_spatial_grid();
        }

        self.ui_context.clear_popovers();
        if self.ramp.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.ramp);
        }

        let mut pc = PaintCtx::new();
        let (w, h) = (self.width as f32, self.height as f32);

        // The window plate (the DemoApp idiom): page-low color at the configured
        // opacity, config corner radius, rolled perimeter — inset by the
        // overflow margin so widget content (the ramp's key pegs) can spill
        // past the frame onto the transparent rim.
        let mut plate = cce_ui::color::page_low_color();
        if plate[3] > 0.001 {
            // Half the DE opacity: this popup reads better mostly-glass.
            plate[3] = cce_ui::color::root_plate_opacity() * 0.5;
        }
        let radius = cce_ui::colors::root_plate_corner_radius();
        let bevel = cce_ui::layout::bevel_width();
        let m = OVERFLOW_MARGIN;
        pc.plate(
            Rect { x: m, y: m, width: w - 2.0 * m, height: h - 2.0 * m },
            (radius, radius, radius, radius),
            &cce_ui::scene::Material::from_fill(plate),
            bevel,
        );

        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.ramp, &mut pc);
        if self.target_key.is_some() {
            cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.save_button, &mut pc);
            cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.cancel_button, &mut pc);
            if !self.status.is_empty() {
                let pad = OVERFLOW_MARGIN + cce_ui::layout::root_plate_padding() / 2.0;
                pc.text_with(
                    self.status.clone(),
                    pad + 2.0 * (96.0 + 12.0),
                    self.height as f32 - pad - 24.0,
                    12.0,
                    [0x9a, 0x9a, 0xa4],
                    None,
                    None,
                );
            }
        }

        // The ramp's field-dropdown popover, drawn into the frame on top.
        if let Some((px, py, pw, ph)) = self.ramp.popover_rect() {
            let mut coll = cce_ui::layout::PopoverCollector::new();
            self.ramp.inner().preset_dropdown.render_popover(&mut coll);
            self.ramp.inner().line_type_dropdown.render_popover(&mut coll);
            for &(c, x, y, qw, qh) in &coll.rects {
                pc.quad(Rect { x, y, width: qw, height: qh }, c);
            }
            let bounds = Some([px, py, px + pw, py + ph]);
            for (content, size, tx, ty, color, font, _b) in coll.texts {
                let color_u8 = [
                    (color[0] * 255.0).clamp(0.0, 255.0) as u8,
                    (color[1] * 255.0).clamp(0.0, 255.0) as u8,
                    (color[2] * 255.0).clamp(0.0, 255.0) as u8,
                ];
                pc.text_with(content, tx, ty, size, color_u8, font, bounds);
            }
        }

        // The shared context menu (right-click on the graph), drawn last, on
        // top of everything. Its labels carry the menu rect as bounds — the
        // runner exempts them from the menu's own text occlusion that way.
        if self.ui_context.is_context_menu_visible() {
            for (qx, qy, qw, qh, c) in self.ui_context.context_menu_quads() {
                pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, c);
            }
            let (mx, my, mw, mh) = (
                cce_ui::widget::context_menu::x(),
                cce_ui::widget::context_menu::y(),
                cce_ui::widget::context_menu::w(),
                cce_ui::widget::context_menu::h(),
            );
            let bounds = Some([mx, my, mx + mw, my + mh]);
            for l in self.ui_context.context_menu_labels() {
                pc.text_with(l.text, l.x, l.y, l.font_size, l.color, None, bounds);
            }
        }

        Some(pc.finish())
    }

    fn display_list_text(&self) -> bool {
        true
    }

    fn ui_context(&self) -> Option<&cce_ui::context::UiContext> {
        Some(&self.ui_context)
    }

    fn is_movable_root_plate_at(&self, px: f32, py: f32) -> bool {
        self.ui_context.drag_allowed_at(px, py)
    }

    fn clear_color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool) {
        if self.ui_context.cursor_moved_context_menu(pos.x, pos.y) {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        let ev = Event::PointerMove { x: pos.x, y: pos.y, local_x: pos.x, local_y: pos.y };
        let changed = self.ui_context.propagate_event(&ev, self.ramp.id());
        self.drain_changes();
        if changed || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn handle_mouse_input(
        &mut self,
        button: MouseButton,
        state: ElementState,
        pos: LogicalPosition,
        needs_rebuild: &mut bool,
    ) -> Option<Self::Message> {
        // The open context menu owns the press (item dispatch / dismiss).
        if self.ui_context.mouse_input_context_menu(button, state, pos.x, pos.y) {
            self.drain_changes();
            *needs_rebuild = true;
            self.needs_rebuild = true;
            return None;
        }
        let ev = Event::MouseButton {
            button,
            state,
            x: pos.x,
            y: pos.y,
            local_x: pos.x,
            local_y: pos.y,
        };
        let mut changed = self.ui_context.propagate_event(&ev, self.ramp.id());
        changed |= self.ui_context.propagate_event(&ev, self.save_button.id());
        changed |= self.ui_context.propagate_event(&ev, self.cancel_button.id());
        self.drain_changes();
        if self.exit_requested {
            return Some(RampMsg::Exit);
        }
        if changed || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        None
    }

    fn handle_mouse_wheel(
        &mut self,
        delta: &MouseScrollDelta,
        pos: LogicalPosition,
        needs_rebuild: &mut bool,
    ) {
        let ev = Event::MouseWheel {
            delta: delta.clone(),
            x: pos.x,
            y: pos.y,
            local_x: pos.x,
            local_y: pos.y,
        };
        let changed = self.ui_context.propagate_event(&ev, self.ramp.id());
        self.drain_changes();
        if changed || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn handle_key_input(
        &mut self,
        event: &KeyEvent,
        needs_rebuild: &mut bool,
    ) -> Option<Self::Message> {
        use cce_ui::widget::{Key, NamedKey};
        if event.state == ElementState::Pressed {
            if event.logical_key == Key::Named(NamedKey::Escape) {
                return Some(RampMsg::Exit);
            }
            if event.ctrl {
                if let Key::Character(ref c) = event.logical_key {
                    if c == "q" {
                        return Some(RampMsg::Exit);
                    }
                }
            }
        }
        let ev = Event::KeyInput(event.clone());
        let handled = self.ui_context.propagate_event(&ev, self.ramp.id());
        self.drain_changes();
        if handled || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        None
    }
}

fn main() {
    cce_ui::engine::run::<RampPopup>();
}
