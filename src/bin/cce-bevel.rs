//! `cce-bevel` — the relief-material control interface. Each profile (the
//! wall curve every recess/boss carve renders, and the plate perimeter's
//! roll) is shown as a lit CROSS-SECTION of the actual edge — plateau, wall,
//! floor — and shaped by three semantic sliders (Shoulder / Base / Bias)
//! instead of a free-form ramp. Every edit applies live to this process (the
//! popup's own plate, wells, and buttons ARE the preview) and logs the
//! sampled spec to stdout; Save persists to `~/.config/cce/config.kdl`
//! (`style.surface.relief`) so every cce app starts with the material.
//!
//! The curve family is the two-exponent rational ease
//! `h(w) = w^a / (w^a + (1-w)^b)` over a bias pre-warp `w = v^g` — monotone,
//! endpoint-exact, with the shoulder (a) and base fillet (b) shaped
//! independently. Slider midpoints give a=b=2, g=1: the analytic smoothstep.
//! Curves are sampled into ramp-spec keys, so the config format and the
//! DE-wide loader are unchanged — free-form specs from cce-designer or a
//! hand-edited config still load everywhere.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::layout::RELIEF_PROFILE_IDENTITY_SPEC as IDENTITY_SPEC;
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{Cap, DisplayList, PaintCtx};
use cce_ui::widget::{
    Adapted, Button, Dropdown, ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta,
    Slider, WidgetHost, WidgetId,
};
use wayland_client::QueueHandle;

const HEADER_FONT_SIZE: f32 = 13.0;
const HEADER_COLOR: [u8; 3] = [0x9a, 0x9a, 0xa4];

/// Knob ranges: depth (how hard the light falls across the wall) and width
/// (how far the wall runs, logical px). Defaults per `layout::bevel_depth` /
/// `bevel_width`.
const DEPTH_RANGE: (f32, f32) = (0.0, 0.6);
const WIDTH_RANGE: (f32, f32) = (2.0, 24.0);

/// Sample count for the spec written to config — enough that the 32-slot
/// renderer LUT sees the curve, few enough that the config line stays sane.
const SPEC_SAMPLES: usize = 17;

#[derive(Debug, Clone)]
enum BevelMsg {
    Exit,
}

/// One profile section's shape state: the three knob sliders plus whether
/// the profile has diverged from the analytic default.
struct ProfileKnobs {
    shoulder: Adapted<Slider>,
    base: Adapted<Slider>,
    bias: Adapted<Slider>,
    /// False until a knob moves (or config carried saved knobs): the DE
    /// renders its analytic profile and Save writes the identity sentinel.
    custom: bool,
    /// Last spec applied+logged.
    last_spec: String,
}

impl ProfileKnobs {
    fn new(seed: Option<(f32, f32, f32)>) -> Self {
        let (s, b, c) = seed.unwrap_or((0.5, 0.5, 0.5));
        let knob = |v: f32, label: &str| {
            Slider::new()
                .with_label(label)
                .with_value(v.clamp(0.0, 1.0))
                .with_scroll(true)
                .with_band(true)
        };
        let mut this = Self {
            shoulder: knob(s, "Shoulder"),
            base: knob(b, "Base"),
            bias: knob(c, "Bias"),
            custom: seed.is_some(),
            last_spec: String::new(),
        };
        this.last_spec = if this.custom { this.spec() } else { IDENTITY_SPEC.to_string() };
        this
    }

    fn values(&self) -> (f32, f32, f32) {
        (self.shoulder.inner().value(), self.base.inner().value(), self.bias.inner().value())
    }

    /// The section's height curve `h(v)`: bias pre-warp, then the rational
    /// two-exponent ease. Exponents run 0.5 (sharp crease) → 2 (smoothstep,
    /// the midpoint) → 8 (wide round-over); bias skews the drop early/late.
    fn eval(&self, v: f32) -> f32 {
        let (s, b, c) = self.values();
        let a = 2.0 * 4f32.powf(2.0 * s - 1.0);
        let be = 2.0 * 4f32.powf(2.0 * b - 1.0);
        let g = 4f32.powf(2.0 * c - 1.0);
        let w = v.clamp(0.0, 1.0).powf(g);
        let num = w.powf(a);
        let den = num + (1.0 - w).powf(be);
        if den <= f32::EPSILON {
            return if w > 0.5 { 1.0 } else { 0.0 };
        }
        (num / den).clamp(0.0, 1.0)
    }

    /// The curve sampled as linear ramp-spec keys — what the renderer LUT
    /// and the config carry.
    fn keys(&self) -> Vec<(f32, f32)> {
        (0..SPEC_SAMPLES)
            .map(|i| {
                let v = i as f32 / (SPEC_SAMPLES - 1) as f32;
                (v, self.eval(v))
            })
            .collect()
    }

    fn spec(&self) -> String {
        cce_ui::widget::format_ramp_spec(&self.keys(), false)
    }

    fn take_change(&mut self) -> bool {
        // Bitwise-or on purpose: every slider's flag must drain.
        self.shoulder.take_change() | self.base.take_change() | self.bias.take_change()
    }

    fn set_defaults(&mut self) {
        self.shoulder.set_value(0.5);
        self.base.set_value(0.5);
        self.bias.set_value(0.5);
        self.custom = false;
        self.last_spec = IDENTITY_SPEC.to_string();
    }
}

struct BevelPopup {
    /// Which profile the single section shows/edits: 0 = wall, 1 = edge.
    profile_dropdown: Adapted<Dropdown>,
    /// The carve wall — what `carve_slope` renders on every
    /// recess/boss/ridge in the DE.
    wall: ProfileKnobs,
    /// The plate perimeter roll — `roll_slope`'s descent profile.
    edge: ProfileKnobs,
    depth_slider: Adapted<Slider>,
    width_slider: Adapted<Slider>,
    save_button: Adapted<Button>,
    reset_button: Adapted<Button>,
    /// Status line under the buttons: what the last save/reset did.
    status: String,
    ui_context: cce_ui::context::UiContext,
    width: u32,
    height: u32,
    scale_factor: f64,
    needs_rebuild: bool,
    registered: bool,
    status_pos: (f32, f32),
    cut_rect: Rect,
}

/// Parse a saved "shoulder,base,bias" knob triple.
fn parse_knobs(s: &str) -> Option<(f32, f32, f32)> {
    let mut it = s.split(',').map(|p| p.trim().parse::<f32>());
    match (it.next(), it.next(), it.next()) {
        (Some(Ok(a)), Some(Ok(b)), Some(Ok(c))) => {
            Some((a.clamp(0.0, 1.0), b.clamp(0.0, 1.0), c.clamp(0.0, 1.0)))
        }
        _ => None,
    }
}

/// Draw one profile as a lit cutaway: the material slab (plate color) inside
/// a dark opening, its surface stroked with segment lighting from the DE's
/// light azimuth. `has_floor` distinguishes the carve (wall meets a floor
/// inside the material) from the roll (the surface drops to the silhouette
/// and the material simply ends — air beyond the edge).
fn draw_section(pc: &mut PaintCtx, rect: Rect, profile: &ProfileKnobs, has_floor: bool) {
    let radius = 6.0f32;
    let radii = (radius, radius, radius, radius);
    pc.rounded_rect(rect, radius, (true, true, true, true), [0.08, 0.08, 0.10, 1.0]);

    // The section geometry: plateau band, then the wall over `wall_w`, then
    // (carve only) the floor band. Vertical span is the feature depth.
    let m = 12.0f32;
    let y_top = rect.y + 16.0;
    let y_bot = rect.y + rect.height - 22.0;
    let drop = y_bot - y_top;
    let x_l = rect.x + m;
    let x_r = rect.x + rect.width - m;
    let plateau_w = (x_r - x_l) * 0.16;
    let wall_w = (x_r - x_l) * if has_floor { 0.56 } else { 0.68 };
    let x0 = x_l + plateau_w;
    let x1 = x0 + wall_w;

    // Surface height at a section x.
    let surface_y = |x: f32| -> Option<f32> {
        if x <= x0 {
            Some(y_top)
        } else if x <= x1 {
            Some(y_top + profile.eval((x - x0) / wall_w) * drop)
        } else if has_floor {
            Some(y_bot)
        } else {
            None // past the silhouette: air
        }
    };

    // The slab: the plate material itself, filled from the surface down to
    // the cut's bottom edge. Columns share exact edges (opaque fill, but the
    // ramp-fill rule keeps seams clean under AA).
    let mut slab = cce_ui::color::page_low_color();
    slab = [slab[0] * 1.25 + 0.03, slab[1] * 1.25 + 0.03, slab[2] * 1.25 + 0.03, 1.0];
    let slab_bot = rect.y + rect.height - 10.0;
    let step = 2.0f32;
    let mut x = x_l;
    while x < x_r {
        let xm = (x + step / 2.0).min(x_r);
        if let Some(sy) = surface_y(xm) {
            let w = step.min(x_r - x);
            pc.quad(Rect { x, y: sy, width: w, height: (slab_bot - sy).max(0.0) }, slab);
        }
        x += step;
    }

    // The surface stroke, lit per segment: outward normal (material below)
    // against the DE light azimuth — the same light the real walls shade by.
    let az = cce_ui::layout::light_source_position();
    let (lx, ly) = (az.cos(), -az.sin());
    let base = [0.60f32, 0.65, 0.74];
    let n_seg = 56usize;
    let seg_end = if has_floor { x_r } else { x1 };
    let mut prev = (x_l, surface_y(x_l).unwrap_or(y_top));
    for i in 1..=n_seg {
        let x = x_l + (seg_end - x_l) * i as f32 / n_seg as f32;
        let Some(y) = surface_y(x) else { break };
        let (dx, dy) = (x - prev.0, y - prev.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (dy / len, -dx / len);
        let lit = (nx * lx + ny * ly) * 0.35;
        let c = [
            (base[0] + lit).clamp(0.0, 1.0),
            (base[1] + lit).clamp(0.0, 1.0),
            (base[2] + lit).clamp(0.0, 1.0),
            1.0,
        ];
        pc.vector(prev.0, prev.1, x, y, 2.5, c, Cap::Round);
        prev = (x, y);
    }
    // The roll's cut face: a dimmer vertical edge closing the slab at the
    // silhouette.
    if !has_floor {
        pc.vector(x1, y_bot, x1, slab_bot, 2.0, [0.36, 0.39, 0.46, 1.0], Cap::Round);
    }

    // The opening's rim, drawn last so its shading falls over the slab edges.
    let depth = cce_ui::layout::bevel_width().min(rect.height * 0.2);
    pc.recess(rect, radii, depth);
}

impl BevelPopup {
    fn root_ids(&self) -> [WidgetId; 11] {
        [
            self.profile_dropdown.id(),
            self.wall.shoulder.id(),
            self.wall.base.id(),
            self.wall.bias.id(),
            self.edge.shoulder.id(),
            self.edge.base.id(),
            self.edge.bias.id(),
            self.depth_slider.id(),
            self.width_slider.id(),
            self.save_button.id(),
            self.reset_button.id(),
        ]
    }

    fn roots(&mut self) -> [*mut (dyn WidgetHost + 'static); 11] {
        [
            self.profile_dropdown.as_ptr_mut(),
            self.wall.shoulder.as_ptr_mut(),
            self.wall.base.as_ptr_mut(),
            self.wall.bias.as_ptr_mut(),
            self.edge.shoulder.as_ptr_mut(),
            self.edge.base.as_ptr_mut(),
            self.edge.bias.as_ptr_mut(),
            self.depth_slider.as_ptr_mut(),
            self.width_slider.as_ptr_mut(),
            self.save_button.as_ptr_mut(),
            self.reset_button.as_ptr_mut(),
        ]
    }

    /// `take_*` plumbing after any routed dispatch — state-gated, so it does
    /// not matter which propagate call consumed the event.
    fn drain_widget_changes(&mut self) {
        if self.profile_dropdown.take_change() {
            // Switch which profile the section shows — re-arrange parks the
            // other set's knobs off-screen.
            self.needs_rebuild = true;
        }
        if self.wall.take_change() {
            self.wall.custom = true;
            let keys = self.wall.keys();
            cce_ui::layout::set_bevel_profile_keys(&keys, false);
            let spec = self.wall.spec();
            println!("wall {spec}");
            self.wall.last_spec = spec;
            self.needs_rebuild = true;
        }
        if self.edge.take_change() {
            self.edge.custom = true;
            let keys = self.edge.keys();
            cce_ui::layout::set_roll_profile_keys(&keys, false);
            let spec = self.edge.spec();
            println!("edge {spec}");
            self.edge.last_spec = spec;
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
    /// Untouched sections write the identity sentinel (= analytic); the knob
    /// triples ride along so this editor reopens where you left it.
    fn save_to_config(&mut self) {
        let path = cce_ui::config::get_config_path();
        let p = path.to_string_lossy().into_owned();
        let depth = format!("{:.3}", self.depth_slider.inner().get_scaled_value());
        let width = format!("{:.2}", self.width_slider.inner().get_scaled_value());
        let knob_str = |k: &ProfileKnobs| {
            let (s, b, c) = k.values();
            format!("{s:.3},{b:.3},{c:.3}")
        };
        let w = &mut |key: &str, value: &str| {
            cce_ui::config::write_config_value(&p, key, value, "style")
        };
        let ok = w("style.surface.relief.depth", &depth)
            & w("style.surface.relief.width", &width)
            & w("style.surface.relief.profile", &self.wall.last_spec)
            & w("style.surface.relief.edge_profile", &self.edge.last_spec)
            & w("style.surface.relief.profile_knobs", &knob_str(&self.wall))
            & w("style.surface.relief.edge_knobs", &knob_str(&self.edge));
        self.status = if ok {
            println!("saved {p}");
            "Saved — apps pick the material up on start.".to_string()
        } else {
            "Save FAILED — see config.kdl permissions.".to_string()
        };
    }

    /// Back to the analytic material, live only (Save persists it): knobs to
    /// their midpoints, both profiles cleared, default depth/width.
    fn reset_live(&mut self) {
        self.wall.set_defaults();
        self.edge.set_defaults();
        cce_ui::layout::clear_bevel_profile();
        cce_ui::layout::clear_roll_profile();
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
        // Force the lazy config load BEFORE reading the registry: the knob
        // strings are read directly (no getter wraps them), so nothing else
        // has triggered it yet this early in startup.
        cce_ui::layout::lazy_init_style_registry();
        let (wall_seed, edge_seed) = {
            let reg = cce_ui::layout::get_style_registry().read().unwrap();
            (
                reg.get_string("bevel_profile_knobs").as_deref().and_then(parse_knobs),
                reg.get_string("roll_profile_knobs").as_deref().and_then(parse_knobs),
            )
        };

        let depth = cce_ui::layout::bevel_depth();
        let width = cce_ui::layout::bevel_width();
        let (dmin, dmax) = DEPTH_RANGE;
        let (wmin, wmax) = WIDTH_RANGE;
        Self {
            profile_dropdown: Dropdown::new(
                vec![
                    "Wall — recess & boss carves".to_string(),
                    "Edge — plate perimeter roll".to_string(),
                ],
                0,
            )
            .with_label("Profile"),
            wall: ProfileKnobs::new(wall_seed),
            edge: ProfileKnobs::new(edge_seed),
            depth_slider: Slider::new()
                .with_label("Depth")
                .with_range(dmin, dmax)
                .with_value(((depth - dmin) / (dmax - dmin)).clamp(0.0, 1.0))
                .with_readout(true)
                .with_decimals(2)
                .with_scroll(true)
                .with_band(true),
            width_slider: Slider::new()
                .with_label("Width")
                .with_range(wmin, wmax)
                .with_value(((width - wmin) / (wmax - wmin)).clamp(0.0, 1.0))
                .with_readout(true)
                .with_decimals(1)
                .with_scroll(true)
                .with_band(true),
            save_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Save"),
            reset_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Reset"),
            status: "Edits apply live; Save writes config.kdl.".to_string(),
            ui_context: cce_ui::context::UiContext::new(),
            width: 520,
            height: 480,
            scale_factor: 1.0,
            needs_rebuild: true,
            registered: false,
            status_pos: (0.0, 0.0),
            cut_rect: Rect::ZERO,
        }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "Bevel".to_string(),
            app_id: "cce-bevel".to_string(),
            width: 520,
            height: 480,
            fullscreen: false,
            min_size: Some((440, 420)),
        }
    }

    fn update(&mut self, msg: Self::Message, _needs_rebuild: &mut bool, exit: &mut bool) {
        match msg {
            BevelMsg::Exit => *exit = true,
        }
    }

    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
        if self.ui_context.tick(dt) {
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

            // Manual column layout: the profile selector, ONE cutaway, the
            // selected profile's knobs, then the global rows. The unselected
            // profile's knobs park off-screen.
            let pad = cce_ui::layout::backplate_padding();
            let x = pad;
            let w = (self.width as f32 - 2.0 * pad).max(0.0);
            let gap = 14.0;
            let strip = {
                let (_, fsize) = cce_ui::layout::control_label_font_detached_parsed();
                fsize + cce_ui::layout::control_label_margin()
            };
            let knob_h = 22.0 + strip;
            let button_h = 26.0;
            let status_h = HEADER_FONT_SIZE + 4.0;
            let fixed = knob_h + gap + knob_h + gap + knob_h + gap + button_h + 8.0 + status_h
                + 3.0 * gap;
            // The cutaway is the hero: it absorbs whatever height the window
            // has beyond the fixed rows.
            let cut_h = (self.height as f32 - 2.0 * pad - fixed).max(90.0);

            let kw = (w - 2.0 * gap) / 3.0;
            let knob_row = |k: &mut ProfileKnobs, x: f32, y: f32| {
                k.shoulder.set_rect(x, y, kw, knob_h);
                k.base.set_rect(x + kw + gap, y, kw, knob_h);
                k.bias.set_rect(x + 2.0 * (kw + gap), y, kw, knob_h);
            };
            let park = |k: &mut ProfileKnobs| {
                k.shoulder.set_rect(-1000.0, -1000.0, 0.0, 0.0);
                k.base.set_rect(-1000.0, -1000.0, 0.0, 0.0);
                k.bias.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            };

            let mut y = pad;
            self.profile_dropdown.set_rect(x, y, (w * 0.62).max(220.0).min(w), knob_h);
            y += knob_h + gap;
            self.cut_rect = Rect { x, y, width: w, height: cut_h };
            y += cut_h + gap;
            if self.profile_dropdown.selected == 0 {
                knob_row(&mut self.wall, x, y);
                park(&mut self.edge);
            } else {
                knob_row(&mut self.edge, x, y);
                park(&mut self.wall);
            }
            y += knob_h + gap;
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
        if self.profile_dropdown.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.profile_dropdown);
        }

        let mut pc = PaintCtx::new();
        let (w, h) = (self.width as f32, self.height as f32);

        // The window plate at full opacity on purpose: its rolled perimeter
        // previews the edge profile, and the wells/buttons preview the wall
        // profile — the popup is its own material sample.
        let mut plate = cce_ui::color::page_low_color();
        if plate[3] > 0.001 {
            plate[3] = cce_ui::color::active_backplate_opacity();
        }
        let radius = cce_ui::colors::backplate_corner_radius();
        let bevel = cce_ui::layout::bevel_width();
        pc.plate(
            Rect { x: 0.0, y: 0.0, width: w, height: h },
            (radius, radius, radius, radius),
            plate,
            bevel,
        );

        pc.text_with(
            self.status.clone(),
            self.status_pos.0,
            self.status_pos.1,
            HEADER_FONT_SIZE,
            HEADER_COLOR,
            Some("monospace".to_string()),
            None,
        );

        let wall_active = self.profile_dropdown.selected == 0;
        let active = if wall_active { &self.wall } else { &self.edge };
        draw_section(&mut pc, self.cut_rect, active, wall_active);

        let knobs = if wall_active { &self.wall } else { &self.edge };
        for s in [
            &knobs.shoulder,
            &knobs.base,
            &knobs.bias,
            &self.depth_slider,
            &self.width_slider,
        ] {
            cce_ui::scene::painter::paint_root_into(&self.ui_context, s, &mut pc);
        }
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.profile_dropdown, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.save_button, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.reset_button, &mut pc);

        // The selector's popover, drawn into the frame on top of everything
        // below it (its labels carry the popover rect as bounds).
        if let Some((px, py, pw, ph)) = self.profile_dropdown.popover_rect() {
            let mut coll = cce_ui::layout::PopoverCollector::new();
            self.profile_dropdown.render_popover(&mut coll);
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

        // The shared context menu (slider Copy/Paste), last, on top.
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
