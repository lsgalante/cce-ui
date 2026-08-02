//! `cce-bevel` — the relief-material control interface: two [`Ramp`] editors
//! shaping the DE's bevel profiles (the wall curve every recess/boss carve
//! renders, and the plate perimeter's roll curve), with the companion
//! depth/width knobs. Every edit applies live to this process — the popup's
//! own plate, wells, and buttons ARE the preview — and logs to stdout;
//! Save persists to `~/.config/cce/config.kdl` (`style.surface.relief`) so
//! every cce app starts with the styled walls.
//!
//! Architecture mirrors `cce-ramp` (single-purpose popup) with the DemoApp
//! multi-root event routing.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::layout::RELIEF_PROFILE_IDENTITY_SPEC as IDENTITY_SPEC;
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::widget::{
    Adapted, Button, ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta, Ramp, Slider,
    WidgetHost, WidgetId,
};
use wayland_client::QueueHandle;

/// Transparent rim between the surface edge and the plate: room for the
/// ramps' key pegs to render outside the window frame instead of being
/// clipped at the buffer edge.
const OVERFLOW_MARGIN: f32 = 40.0;

const HEADER_FONT_SIZE: f32 = 13.0;
const HEADER_COLOR: [u8; 3] = [0x9a, 0x9a, 0xa4];

/// Knob ranges: depth (how hard the light falls across the wall) and width
/// (how far the wall runs, logical px). Defaults per `layout::bevel_depth` /
/// `bevel_width`.
const DEPTH_RANGE: (f32, f32) = (0.0, 0.6);
const WIDTH_RANGE: (f32, f32) = (2.0, 24.0);

#[derive(Debug, Clone)]
enum BevelMsg {
    Exit,
}

struct BevelPopup {
    /// The carve wall curve — what `carve_slope` renders on every
    /// recess/boss/ridge in the DE.
    wall_ramp: Adapted<Ramp>,
    /// The plate perimeter roll curve — `roll_slope`'s descent profile.
    edge_ramp: Adapted<Ramp>,
    depth_slider: Adapted<Slider>,
    width_slider: Adapted<Slider>,
    save_button: Adapted<Button>,
    reset_button: Adapted<Button>,
    /// Last specs applied+logged — edits are detected by comparison, so
    /// tick-driven changes (hover-scroll glides) apply too.
    last_wall: String,
    last_edge: String,
    /// Status line under the buttons: what the last save/reset did.
    status: String,
    ui_context: cce_ui::context::UiContext,
    width: u32,
    height: u32,
    scale_factor: f64,
    needs_rebuild: bool,
    registered: bool,
    wall_header: (f32, f32),
    edge_header: (f32, f32),
    status_pos: (f32, f32),
}

impl BevelPopup {
    fn root_ids(&self) -> [WidgetId; 6] {
        [
            self.wall_ramp.id(),
            self.edge_ramp.id(),
            self.depth_slider.id(),
            self.width_slider.id(),
            self.save_button.id(),
            self.reset_button.id(),
        ]
    }

    fn roots(&mut self) -> [*mut (dyn WidgetHost + 'static); 6] {
        [
            self.wall_ramp.as_ptr_mut(),
            self.edge_ramp.as_ptr_mut(),
            self.depth_slider.as_ptr_mut(),
            self.width_slider.as_ptr_mut(),
            self.save_button.as_ptr_mut(),
            self.reset_button.as_ptr_mut(),
        ]
    }

    /// Install a ramp's curve as the live wall (or roll) profile. The
    /// identity-smooth spec is the "analytic" sentinel ([`IDENTITY_SPEC`]):
    /// it clears back to the built-in profile instead of installing.
    fn apply_profile(ramp: &Ramp, spec: &str, roll: bool) {
        let keys: Vec<(f32, f32)> = ramp.keys.iter().map(|k| (k.pos, k.value)).collect();
        let smooth = ramp.smooth();
        match (spec == IDENTITY_SPEC, roll) {
            (true, true) => cce_ui::layout::clear_roll_profile(),
            (true, false) => cce_ui::layout::clear_bevel_profile(),
            (false, true) => cce_ui::layout::set_roll_profile_keys(&keys, smooth),
            (false, false) => cce_ui::layout::set_bevel_profile_keys(&keys, smooth),
        }
    }

    /// `take_*` plumbing after any routed dispatch — state-gated, so it does
    /// not matter which propagate call consumed the event.
    fn drain_widget_changes(&mut self) {
        let wall_spec = self.wall_ramp.inner().spec_string();
        if wall_spec != self.last_wall {
            Self::apply_profile(self.wall_ramp.inner(), &wall_spec, false);
            println!("wall {wall_spec}");
            self.last_wall = wall_spec;
            self.needs_rebuild = true;
        }
        let edge_spec = self.edge_ramp.inner().spec_string();
        if edge_spec != self.last_edge {
            Self::apply_profile(self.edge_ramp.inner(), &edge_spec, true);
            println!("edge {edge_spec}");
            self.last_edge = edge_spec;
            self.needs_rebuild = true;
        }
        if self.depth_slider.take_change() {
            let v = self.depth_slider.inner().get_scaled_value();
            if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
                reg.set_float("bevel_depth", v);
            }
            println!("depth {v:.3}");
            self.needs_rebuild = true;
        }
        if self.width_slider.take_change() {
            let v = self.width_slider.inner().get_scaled_value();
            if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
                reg.set_float("bevel_width", v);
            }
            println!("width {v:.2}");
            self.needs_rebuild = true;
        }
        if self.save_button.take_click() {
            self.save_to_config();
            self.needs_rebuild = true;
        }
        if self.reset_button.take_click() {
            self.reset_live();
            self.needs_rebuild = true;
        }
    }

    /// Persist the current material to the shared config
    /// (`style.surface.relief` — the same keys every app reads at startup).
    /// Identity specs are written as-is; the loader reads them as analytic.
    fn save_to_config(&mut self) {
        let path = cce_ui::config::get_config_path();
        let p = path.to_string_lossy().into_owned();
        let depth = format!("{:.3}", self.depth_slider.inner().get_scaled_value());
        let width = format!("{:.2}", self.width_slider.inner().get_scaled_value());
        let ok = cce_ui::config::write_config_value(&p, "style.surface.relief.depth", &depth, "style")
            & cce_ui::config::write_config_value(&p, "style.surface.relief.width", &width, "style")
            & cce_ui::config::write_config_value(&p, "style.surface.relief.profile", &self.last_wall, "style")
            & cce_ui::config::write_config_value(
                &p,
                "style.surface.relief.edge_profile",
                &self.last_edge,
                "style",
            );
        self.status = if ok {
            println!("saved {p}");
            "Saved — apps pick the material up on start.".to_string()
        } else {
            "Save FAILED — see config.kdl permissions.".to_string()
        };
    }

    /// Back to the analytic material, live only (Save persists it): identity
    /// curves on both ramps, default depth/width.
    fn reset_live(&mut self) {
        self.wall_ramp.set_spec(IDENTITY_SPEC);
        self.edge_ramp.set_spec(IDENTITY_SPEC);
        // drain_widget_changes sees the spec change and clears the profiles.
        if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
            reg.set_float("bevel_depth", 0.15);
            reg.set_float("bevel_width", 9.3);
        }
        let (dmin, dmax) = DEPTH_RANGE;
        self.depth_slider.set_value((0.15 - dmin) / (dmax - dmin));
        let (wmin, wmax) = WIDTH_RANGE;
        self.width_slider.set_value((9.3 - wmin) / (wmax - wmin));
        self.status = "Reset to the analytic profiles (unsaved).".to_string();
        println!("reset");
    }
}

impl Application for BevelPopup {
    type Message = BevelMsg;

    fn new(
        _qh: &QueueHandle<EngineState<Self>>,
        _sender: calloop::channel::Sender<Self::Message>,
    ) -> Self {
        cce_ui::scale::set_scale_factor(1.0);
        // Force the lazy config load BEFORE reading the registry: the specs
        // are read directly (no getter wraps them), so nothing else has
        // triggered it yet this early in startup.
        cce_ui::layout::lazy_init_style_registry();
        // Seed the ramps from the configured material (reload_config has
        // already installed the live profiles); identity when unconfigured.
        let reg = cce_ui::layout::get_style_registry();
        let (wall_spec, edge_spec) = {
            let reg = reg.read().unwrap();
            (
                reg.get_string("bevel_profile_spec").unwrap_or_else(|| IDENTITY_SPEC.to_string()),
                reg.get_string("roll_profile_spec").unwrap_or_else(|| IDENTITY_SPEC.to_string()),
            )
        };
        let mut wall_ramp = Ramp::new();
        wall_ramp.set_spec(&wall_spec);
        let mut edge_ramp = Ramp::new();
        edge_ramp.set_spec(&edge_spec);
        let last_wall = wall_ramp.inner().spec_string();
        let last_edge = edge_ramp.inner().spec_string();

        let depth = cce_ui::layout::bevel_depth();
        let width = cce_ui::layout::bevel_width();
        let (dmin, dmax) = DEPTH_RANGE;
        let (wmin, wmax) = WIDTH_RANGE;
        Self {
            wall_ramp,
            edge_ramp,
            depth_slider: Slider::new()
                .with_label("Depth")
                .with_range(dmin, dmax)
                .with_value(((depth - dmin) / (dmax - dmin)).clamp(0.0, 1.0))
                .with_readout(true)
                .with_decimals(2)
                .with_scroll(true),
            width_slider: Slider::new()
                .with_label("Width")
                .with_range(wmin, wmax)
                .with_value(((width - wmin) / (wmax - wmin)).clamp(0.0, 1.0))
                .with_readout(true)
                .with_decimals(1)
                .with_scroll(true),
            save_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Save"),
            reset_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Reset"),
            last_wall,
            last_edge,
            status: "Edits apply live; Save writes config.kdl.".to_string(),
            ui_context: cce_ui::context::UiContext::new(),
            width: 620,
            height: 1020,
            scale_factor: 1.0,
            needs_rebuild: true,
            registered: false,
            wall_header: (0.0, 0.0),
            edge_header: (0.0, 0.0),
            status_pos: (0.0, 0.0),
        }
    }

    // Buffer-larger-than-geometry mode (the cce-ramp idiom): key pegs painted
    // on the rim render outside the window frame; rim clicks fall through.
    fn overflow_margin(&self) -> u32 {
        OVERFLOW_MARGIN as u32
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "Bevel".to_string(),
            app_id: "cce-bevel".to_string(),
            width: 540,
            height: 940,
            fullscreen: false,
            min_size: Some((460, 760)),
        }
    }

    fn update(&mut self, msg: Self::Message, _needs_rebuild: &mut bool, exit: &mut bool) {
        match msg {
            BevelMsg::Exit => *exit = true,
        }
    }

    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
        if self.ui_context.tick(dt) {
            // Tick-driven edits (hover-scroll glide) apply and log too.
            self.drain_widget_changes();
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn display_list(&mut self, size: LogicalSize, scale: f64) -> Option<DisplayList> {
        if !self.registered {
            self.registered = true;
            let self_ptr = self as *mut Self;
            unsafe {
                for w in (*self_ptr).roots() {
                    let id = (*w).base().id();
                    self.ui_context.register_widget(id, w);
                }
            }
        }

        let size_changed = self.width != size.width as u32
            || self.height != size.height as u32
            || self.scale_factor != scale;
        if self.needs_rebuild || size_changed {
            self.width = size.width as u32;
            self.height = size.height as u32;
            self.scale_factor = scale;
            cce_ui::scale::set_scale_factor(scale as f32);

            // Manual column layout at the popup pad (the cce-ramp idiom).
            let pad = OVERFLOW_MARGIN + cce_ui::layout::backplate_padding() / 2.0;
            let x = pad;
            let w = (self.width as f32 - 2.0 * pad).max(0.0);
            let header_h = HEADER_FONT_SIZE + 7.0;
            let gap = 16.0;
            let knob_h = 22.0 + Ramp::label_strip();
            let button_h = 26.0;
            let status_h = HEADER_FONT_SIZE + 4.0;
            let fixed = 2.0 * header_h + 3.0 * gap + knob_h + gap + button_h + status_h + 8.0;
            let ramp_h =
                ((self.height as f32 - 2.0 * pad - fixed) / 2.0).max(220.0);

            let mut y = pad;
            self.wall_header = (x, y);
            y += header_h;
            self.wall_ramp.set_rect(x, y, w, ramp_h);
            y += ramp_h + gap;
            self.edge_header = (x, y);
            y += header_h;
            self.edge_ramp.set_rect(x, y, w, ramp_h);
            y += ramp_h + gap;
            let half = (w - gap) / 2.0;
            self.depth_slider.set_rect(x, y, half, knob_h);
            self.width_slider.set_rect(x + half + gap, y, half, knob_h);
            y += knob_h + gap;
            self.save_button.set_rect(x, y, 96.0, button_h);
            self.reset_button.set_rect(x + 96.0 + 12.0, y, 96.0, button_h);
            y += button_h + 8.0;
            self.status_pos = (x, y);

            self.needs_rebuild = false;
            self.ui_context.rebuild_spatial_grid();
        }

        self.ui_context.clear_popovers();
        if self.wall_ramp.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.wall_ramp);
        }
        if self.edge_ramp.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.edge_ramp);
        }

        let mut pc = PaintCtx::new();
        let (w, h) = (self.width as f32, self.height as f32);

        // The window plate at FULL opacity on purpose: its rolled perimeter is
        // the edge profile's preview, and the wells/buttons below preview the
        // wall profile — the popup is its own material sample.
        let mut plate = cce_ui::color::page_low_color();
        if plate[3] > 0.001 {
            plate[3] = cce_ui::color::active_backplate_opacity();
        }
        let radius = cce_ui::colors::backplate_corner_radius();
        let bevel = cce_ui::layout::bevel_width();
        let m = OVERFLOW_MARGIN;
        pc.plate(
            Rect { x: m, y: m, width: w - 2.0 * m, height: h - 2.0 * m },
            (radius, radius, radius, radius),
            plate,
            bevel,
        );

        let header = |pc: &mut PaintCtx, text: &str, pos: (f32, f32)| {
            pc.text_with(
                text.to_string(),
                pos.0,
                pos.1,
                HEADER_FONT_SIZE,
                HEADER_COLOR,
                Some("monospace".to_string()),
                None,
            );
        };
        header(&mut pc, "Wall profile — recess & boss carves", self.wall_header);
        header(&mut pc, "Edge profile — plate perimeter roll", self.edge_header);
        header(&mut pc, &self.status, self.status_pos);

        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.wall_ramp, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.edge_ramp, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.depth_slider, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.width_slider, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.save_button, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.reset_button, &mut pc);

        // The ramps' field-dropdown popovers, drawn into the frame on top.
        for ramp in [&self.wall_ramp, &self.edge_ramp] {
            let Some((px, py, pw, ph)) = ramp.popover_rect() else { continue };
            let mut coll = cce_ui::layout::PopoverCollector::new();
            ramp.inner().preset_dropdown.render_popover(&mut coll);
            ramp.inner().line_type_dropdown.render_popover(&mut coll);
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

        // The shared context menu, last, on top of everything.
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

    fn is_movable_backplate_at(&self, px: f32, py: f32) -> bool {
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
        let mut changed = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                changed = true;
            }
        }
        self.drain_widget_changes();
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
            self.drain_widget_changes();
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
        let mut changed = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                changed = true;
            }
        }
        self.drain_widget_changes();
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
        let mut changed = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                changed = true;
            }
        }
        self.drain_widget_changes();
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
            // Escape exits — unless the context menu is up (the toolkit-wide
            // Escape-dismiss should win the first press).
            if event.logical_key == Key::Named(NamedKey::Escape)
                && !self.ui_context.is_context_menu_visible()
            {
                return Some(BevelMsg::Exit);
            }
            if event.ctrl {
                if let Key::Character(ref c) = event.logical_key {
                    if c == "q" {
                        return Some(BevelMsg::Exit);
                    }
                }
            }
        }
        let ev = Event::KeyInput(event.clone());
        let mut handled = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                handled = true;
                break;
            }
        }
        self.drain_widget_changes();
        if handled || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        None
    }
}

fn main() {
    cce_ui::engine::run::<BevelPopup>();
}
