//! The reference `Application` (Phase 6ad) — a small widget gallery on the target
//! architecture, end to end. This is the file to copy when starting a new `cce-*` client.
//!
//! The shape every migrated app shares:
//!
//! 1. **One paint path.** The whole frame — geometry AND text — is built in
//!    [`Application::display_list`] as prims on a [`PaintCtx`], with
//!    [`Application::display_list_text`] returning `true`. There is no `view*`/`text_items`
//!    pair, no app-side `FontSystem`, no glyphon buffers: text is a `Prim::Text` shaped by
//!    the engine's shared cache.
//! 2. **Layout via the scene solver.** The frame is a plain `Arena<LayoutBox>` tree the
//!    app builds and solves with [`compute_layout`]; widgets get their rects from the
//!    solved leaves. No container widgets, no hand-summed offsets.
//! 3. **Routed events.** Each input handler builds one [`Event`] and routes it through
//!    `UiContext::propagate_event` per widget root. The router owns press hit-gating,
//!    Enter/Leave synthesis, drag-target recording, and KeyInput-to-focused delivery;
//!    the app keeps only state-gated `take_*` plumbing.
//! 4. **No embedded bases.** Widgets are app-owned values (all [`Adapted`]); the window
//!    plate is prims, not a `Backplate`; popovers draw INTO the frame (there is no popup
//!    surface); app state — not any widget tree — is the source of truth.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::arena::Arena;
use cce_ui::scene::layout::{
    compute_layout, CrossAlign, LayoutBox, Length, Rect, Size as LSize, Style,
};
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::widget::{
    Adapted, Button, Dropdown, WidgetHost, WidgetId, ElementState, Event, KeyEvent, MouseButton,
    MouseScrollDelta, Slider, TextBox, Toggle,
};
use wayland_client::QueueHandle;

#[derive(Debug, Clone)]
enum DemoMessage {
    Exit,
}

/// Chrome typography: the title/status font sizes, and the layout leaves that
/// hold them derived as one line-height (×1.2, the toolkit convention) — so the
/// header and status bands, which anchor to those solved rects, resize with the
/// typography instead of relying on magic leaf heights.
const TITLE_FONT_SIZE: f32 = 15.0;
const STATUS_FONT_SIZE: f32 = 12.0;
/// Vertical padding on each side of the status band's text line.
const STATUS_BAND_PAD: f32 = 5.0;
fn text_leaf_height(font_size: f32) -> f32 {
    (font_size * 1.2).ceil()
}

struct DemoApp {
    // ── Widgets: app-owned values on the narrow-trait adapter. Their addresses must be
    // stable across frames (plain struct fields, not Vec elements): the UiContext
    // registry and the router's drag-target bookkeeping hold pointers to them.
    button: Adapted<Button>,
    toggle: Adapted<Toggle>,
    slider: Adapted<Slider>,
    name_box: Adapted<TextBox>,
    theme_dropdown: Adapted<Dropdown>,

    // ── App state: the source of truth. Widgets are re-asserted from it every rebuild
    // (`set_toggled` below); `take_*` changes flow back into it, never the reverse.
    toggle_on: bool,
    clicks: u32,
    status: String,

    ui_context: cce_ui::context::UiContext,
    width: u32,
    height: u32,
    scale_factor: f64,
    needs_rebuild: bool,
    widgets_registered: bool,
    title_rect: Rect,
    status_rect: Rect,
}

impl DemoApp {
    /// The widget root ids, in paint order — what the router dispatches over.
    /// `propagate_event` takes a `WidgetId` and resolves it through the registry, so the
    /// event paths need no raw pointers and no unsafe self-alias.
    fn root_ids(&self) -> [WidgetId; 5] {
        [
            self.button.id(),
            self.toggle.id(),
            self.slider.id(),
            self.name_box.id(),
            self.theme_dropdown.id(),
        ]
    }

    /// The widget roots as pointers, for the one genuinely pointer-consuming path left:
    /// registration (the registry stores them). The paint walk takes shared borrows.
    fn roots(&mut self) -> [*mut (dyn WidgetHost + 'static); 5] {
        [
            self.button.as_ptr_mut(),
            self.toggle.as_ptr_mut(),
            self.slider.as_ptr_mut(),
            self.name_box.as_ptr_mut(),
            self.theme_dropdown.as_ptr_mut(),
        ]
    }

    /// `take_*` plumbing: translate widget changes into app state. Runs after any routed
    /// dispatch; every check is STATE-gated, so it does not matter which propagate call
    /// consumed the event (see the KeyInput note in `handle_key_input`).
    fn drain_widget_changes(&mut self) {
        if self.button.take_click() {
            self.clicks += 1;
            self.status = format!("Button clicked {} time(s)", self.clicks);
            self.needs_rebuild = true;
        }
        if self.toggle.take_change() {
            self.toggle_on = !self.toggle_on;
            self.status = format!("Toggle: {}", if self.toggle_on { "on" } else { "off" });
            self.needs_rebuild = true;
        }
        if self.slider.take_change() {
            self.status = format!("Slider: {:.0}", self.slider.get_scaled_value());
            self.needs_rebuild = true;
        }
        if self.theme_dropdown.take_change() {
            let idx = self.theme_dropdown.selected;
            if let Some(opt) = self.theme_dropdown.options.get(idx) {
                self.status = format!("Theme: {opt}");
            }
            self.needs_rebuild = true;
        }
        if self.name_box.take_change() {
            self.status = format!("Name: {}", self.name_box.text);
            self.needs_rebuild = true;
        }
    }
}

impl Application for DemoApp {
    type Message = DemoMessage;

    fn new(
        _qh: &QueueHandle<EngineState<Self>>,
        _sender: calloop::channel::Sender<Self::Message>,
    ) -> Self {
        cce_ui::scale::set_scale_factor(1.0);
        Self {
            button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Click me").with_raised(true),
            toggle: Toggle::new().with_raised(true),
            // Slider `value` is NORMALIZED 0..1; `with_range` only scales the readout
            // (`get_scaled_value`). Wheel nudging is an explicit opt-in.
            slider: Slider::new()
                .with_range(0.0, 100.0)
                .with_value(0.4)
                .with_scroll(true)
                .with_recessed(true),
            name_box: TextBox::new(String::new())
                .with_placeholder("Type a name...")
                .with_recessed(true),
            theme_dropdown: Dropdown::new(
                vec!["Forest".into(), "Ocean".into(), "Ember".into()],
                0,
            )
            .with_raised(true),
            toggle_on: false,
            clicks: 0,
            status: "Ready.".to_string(),
            ui_context: cce_ui::context::UiContext::new(),
            width: 560,
            height: 420,
            scale_factor: 1.0,
            needs_rebuild: true,
            widgets_registered: false,
            title_rect: Rect::ZERO,
            status_rect: Rect::ZERO,
        }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: "cce-ui reference gallery".to_string(),
            app_id: "cce-ui-demo".to_string(),
            width: 560,
            height: 420,
            fullscreen: false,
            min_size: Some((360, 300)),
        }
    }

    fn update(&mut self, msg: Self::Message, _needs_rebuild: &mut bool, exit: &mut bool) {
        match msg {
            DemoMessage::Exit => *exit = true,
        }
    }

    /// Widget animations (cursor blink, hover fades) tick through the UiContext; the
    /// loop is demand-driven, so returning a redraw request only when something moved
    /// keeps the app idle otherwise.
    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
        if self.ui_context.tick(dt) {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn display_list(&mut self, size: LogicalSize, scale: f64) -> Option<DisplayList> {
        // Register once: the registry backs the router (drag targets are looked up by
        // widget id) and drag_allowed_at. Pointers into `self` are stable only once
        // `self` sits at its final address — hence here, not in `new()`.
        if !self.widgets_registered {
            self.widgets_registered = true;
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

            // Re-assert widget visuals from app state (the app is the source of truth).
            self.toggle.set_toggled(self.toggle_on);
            self.toggle.set_label(if self.toggle_on { "ON" } else { "OFF" });

            // ── Layout: a plain LayoutBox tree, solved in one call. Leaves carry their
            // intrinsic sizes; `grow` distributes leftover space; the solved rects are
            // assigned straight onto the widgets.
            let mut arena: Arena<LayoutBox> = Arena::new();
            let root = arena.insert(LayoutBox::container(
                Style::column().padding(16.0).gap(14.0).cross_align(CrossAlign::Stretch),
            ));
            let title = arena.insert(LayoutBox::leaf(
                Style::row(),
                LSize::new(0.0, text_leaf_height(TITLE_FONT_SIZE)),
            ));
            // Clearance under the header band: the recess step rolls over `bevel_width`
            // past the band's bottom edge, so the first content row must stand off by at
            // least that or it crowds the carve.
            let band_gap = arena.insert(LayoutBox::leaf(
                Style::row(),
                LSize::new(0.0, cce_ui::layout::bevel_width()),
            ));
            // One shared height for the whole control row, so the button,
            // toggle, and dropdown plates land on the same top and bottom edge.
            const CONTROL_H: f32 = 28.0;
            let controls = arena.insert(LayoutBox::container(
                Style::row().gap(14.0).height(Length::Fixed(CONTROL_H)),
            ));
            // `shrink` lets the fixed leaves give up width when the window is at its
            // minimum instead of overflowing the row.
            let button = arena.insert(LayoutBox::leaf(Style::row().shrink(1.0), LSize::new(120.0, CONTROL_H)));
            let toggle = arena.insert(LayoutBox::leaf(Style::row(), LSize::new(64.0, CONTROL_H)));
            let dropdown = arena.insert(LayoutBox::leaf(Style::row().shrink(1.0), LSize::new(150.0, CONTROL_H)));
            let slider = arena.insert(LayoutBox::leaf(Style::row(), LSize::new(0.0, 24.0)));
            let name_box = arena.insert(LayoutBox::leaf(Style::row(), LSize::new(0.0, 30.0)));
            let spacer = arena.insert(LayoutBox::container(Style::column().grow(1.0)));
            let status = arena.insert(LayoutBox::leaf(
                Style::row(),
                LSize::new(0.0, text_leaf_height(STATUS_FONT_SIZE)),
            ));
            arena.append_child(root, title);
            arena.append_child(root, band_gap);
            arena.append_child(root, controls);
            arena.append_child(controls, button);
            arena.append_child(controls, toggle);
            arena.append_child(controls, dropdown);
            arena.append_child(root, slider);
            arena.append_child(root, name_box);
            arena.append_child(root, spacer);
            arena.append_child(root, status);
            compute_layout(
                &mut arena,
                root,
                LSize::new(self.width as f32, self.height as f32),
            );

            // Stretched children fill the column width; fixed leaves keep their size.
            let r = |id| arena.value(id).unwrap().rect;
            let b = r(button);
            self.button.set_rect(b.x, b.y, b.width, b.height);
            let t = r(toggle);
            self.toggle.set_rect(t.x, t.y, t.width, t.height);
            let d = r(dropdown);
            self.theme_dropdown.set_rect(d.x, d.y, d.width, d.height);
            let s = r(slider);
            self.slider.set_rect(s.x, s.y, s.width, s.height);
            let n = r(name_box);
            self.name_box.set_rect(n.x, n.y, n.width, n.height);
            self.title_rect = r(title);
            self.status_rect = r(status);

            self.needs_rebuild = false;
            self.ui_context.rebuild_spatial_grid();
        }

        // ── Popover registration: ui_context ONLY. It drives the engine's display-list
        // text occlusion clamp (labels under the open popover get clipped); the popover
        // itself is drawn into this frame below — there is no popup surface.
        self.ui_context.clear_popovers();
        if self.theme_dropdown.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.theme_dropdown);
        }

        let mut pc = PaintCtx::new();
        let w = self.width as f32;
        let h = self.height as f32;

        // The window plate — the dissolved root Backplate as a lit object: page-low color
        // at the configured opacity, config corner radius, perimeter rolled over
        // `bevel_width` so the surface reads as a physical plate rather than a flat fill.
        let mut plate = cce_ui::color::page_low_color();
        if plate[3] > 0.001 {
            plate[3] = cce_ui::color::active_backplate_opacity();
        }
        let radius = cce_ui::colors::backplate_corner_radius();
        let frame = Rect { x: 0.0, y: 0.0, width: w, height: h };
        let bevel = cce_ui::layout::bevel_width();
        pc.plate(frame, (radius, radius, radius, radius), plate, bevel);

        // Header band: the title strip carved one step down into the plate. Flush to the
        // window's top and sides, so its only real wall is the bottom one facing the
        // content (the recessed-MenuBar idiom — the other three would fight the plate's
        // own rolled perimeter).
        let band_h = self.title_rect.y + self.title_rect.height + 10.0;
        pc.recess_edges(
            Rect { x: 0.0, y: 0.0, width: w, height: band_h },
            (0.0, 0.0, 0.0, 0.0),
            cce_ui::layout::bar_wall_width(),
            (false, false, true, false),
        );

        // Status band: the header's mirror — carved into the bottom of the
        // plate, flush to the window's bottom and sides, its only wall the top
        // one facing the content. Emitted here, before any widget geometry, so
        // it CSG-groups into the plate like the header band does. Sized from
        // the status font plus a symmetric pad (the layout's status leaf only
        // reserves the space; the band and its text center independently).
        let status_h = text_leaf_height(STATUS_FONT_SIZE) + 2.0 * STATUS_BAND_PAD;
        let status_top = h - status_h;
        pc.recess_edges(
            Rect { x: 0.0, y: status_top, width: w, height: status_h },
            (0.0, 0.0, 0.0, 0.0),
            cce_ui::layout::bar_wall_width(),
            (true, false, false, false),
        );

        // App chrome text: plain prims. `text_with` carries an optional font family and
        // optional bounds; unbounded text is clamped to the surface by the engine.
        pc.text_with(
            "cce-ui reference gallery".to_string(),
            self.title_rect.x,
            self.title_rect.y,
            TITLE_FONT_SIZE,
            [0xdd, 0xdd, 0xe2],
            Some("monospace".to_string()),
            None,
        );
        pc.text_with(
            self.status.clone(),
            self.status_rect.x,
            cce_ui::layout::align_text_y(status_top, status_h, STATUS_FONT_SIZE, 0.0),
            STATUS_FONT_SIZE,
            [0x9a, 0x9a, 0xa4],
            // None here falls through fontconfig's unbundled sans alias to the
            // serif fallback — always name a family.
            Some("monospace".to_string()),
            None,
        );

        // Widgets: each root walked through the single paint pass. The walk recurses,
        // clips, and emits each widget's own geometry AND text (`Adapted::paint_self`
        // serves per-widget fonts and bounds).
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.button, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.toggle, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.slider, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.name_box, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.theme_dropdown, &mut pc);

        // The dropdown popover — geometry and labels last, on top of everything, exactly
        // where it hit-tests. Labels carry bounds equal to the popover rect: that clips
        // them to the plate AND exempts them from the occlusion clamp (text whose bounds
        // equal an overlay rect is treated as the overlay's own).
        if let Some((px, py, pw, ph)) = self.theme_dropdown.popover_rect() {
            let mut coll = cce_ui::layout::PopoverCollector::new();
            self.theme_dropdown.render_popover(&mut coll);
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

        Some(pc.finish())
    }

    /// Text prims in the display list ARE the frame's text — no `text_items` twin.
    fn display_list_text(&self) -> bool {
        true
    }

    fn ui_context(&self) -> Option<&cce_ui::context::UiContext> {
        Some(&self.ui_context)
    }

    /// Window dragging for a dissolved root: the surface is the movable plate; drag
    /// anywhere a drag-blocking registered widget isn't.
    fn is_movable_backplate_at(&self, px: f32, py: f32) -> bool {
        self.ui_context.drag_allowed_at(px, py)
    }

    fn clear_color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool) {
        let (px, py) = (pos.x, pos.y);
        let ev = Event::PointerMove { x: px, y: py, local_x: px, local_y: py };
        let mut changed = false;
        // PointerMove visits every root: hover bookkeeping everywhere, and the
        // router forwards DragUpdate to the recorded drag target (slider thumb,
        // text selection) once its 3px threshold trips.
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
        let (px, py) = (pos.x, pos.y);
        let ev = Event::MouseButton { button, state, x: px, y: py, local_x: px, local_y: py };
        let mut changed = false;
        // Presses are hit-gated per widget by the adapter and releases delivered
        // everywhere (press-tracking widgets commit or cancel on them) — a straight
        // loop is correct for pointer-positioned events.
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
        let (px, py) = (pos.x, pos.y);
        let ev = Event::MouseWheel { delta: delta.clone(), x: px, y: py, local_x: px, local_y: py };
        let mut changed = false;
        // Wheel is hit-scoped per widget (the slider nudges its value under the
        // cursor); roots that miss return false.
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
        // App-level shortcuts before widget routing.
        if event.ctrl && event.state == ElementState::Pressed {
            if let cce_ui::widget::Key::Character(ref c) = event.logical_key {
                if c == "q" {
                    return Some(DemoMessage::Exit);
                }
            }
        }

        // KeyInput MUST short-circuit: the router delivers keys to the ctx-focused
        // widget FIRST on every propagate call, so a non-short-circuited chain would
        // hand a typed character to the focused widget once per root (N-time
        // insertion). Plumbing that would key off "which call handled it" belongs in
        // the state-gated `drain_widget_changes` instead.
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
    cce_ui::engine::run::<DemoApp>();
}
