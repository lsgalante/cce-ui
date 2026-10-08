//! The reference `Application` (Phase 6ad) — a small widget gallery on the target
//! architecture, end to end. This is the file to copy when starting a new `cce-*` client.
//!
//! The shape every migrated app shares:
//!
//! 1. **One paint path.** The whole frame — geometry AND text — is built in
//!    [`Application::display_list`] as prims on a [`PaintCtx`], with
//!    [`Application::display_list_text`] returning `true`. There is no `view*`/`text_items`
//!    pair, no app-side `FontSystem`, no cosmic-text buffers: text is a `Prim::Text` shaped by
//!    the engine's shared cache.
//! 2. **Layout via the scene solver.** The frame is a plain `Arena<LayoutBox>` tree the
//!    app builds and solves with [`compute_layout`]; widgets get their rects from the
//!    solved leaves. No container widgets, no hand-summed offsets.
//! 3. **Routed events.** Each input handler builds one [`Event`] and routes it through
//!    `UiContext::propagate_event` per widget root. The router owns press hit-gating,
//!    Enter/Leave synthesis, drag-target recording, and KeyInput-to-focused delivery;
//!    the app keeps only state-gated `take_*` plumbing.
//! 4. **A modal dialog is a lasso.** The Options dialog's members are ordinary widgets the app
//!    owns and lays out; [`Dialog`] is the plate under them, and opening it traps the Tab
//!    walk among them and covers the window behind (`UiContext::open_modal`). Closed, its
//!    members are hidden, so they are neither Tab stops nor in the accessibility tree.
//! 5. **The context owns the widgets.** The app holds a [`Handle`] to each (all [`Adapted`])
//!    and reaches it through the context — `ui[h]`, `ui.get_mut(h)`, `ui.lend_h(h, ..)` when the
//!    widget and the context are both needed — so the compiler keeps the app's access and the
//!    context's own from overlapping (`docs/rfc-owning-registry.md`). The window plate is prims,
//!    not a root plate container; popovers draw INTO the frame (there is no popup surface); app
//!    state — not any widget tree — is the source of truth.

use cce_ui::context::UiContext;
use cce_ui::engine::{Application, AppSender, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::scene::arena::Arena;
use cce_ui::scene::layout::{
    compute_layout, FitMode, LayoutBox, Length, Rect, Size as LSize, Style,
};
use cce_ui::scene::paint::{DisplayList, PaintCtx};
use cce_ui::widget::{
    Adapted, Button, Dialog, Dropdown, ElementState, Event, Handle, ImageView, KeyEvent, MouseButton,
    MouseScrollDelta, NamedKey, RadioGroup, Slider, TextBox, Toggle, WidgetHost, WidgetId,
};

#[derive(Debug, Clone)]
pub(crate) enum DemoMessage {
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
/// The Options dialog's choices.
const TEXT_SIZES: [&str; 3] = ["Small", "Medium", "Large"];
/// Its buttons' width.
const DIALOG_BUTTON_W: f32 = 88.0;
fn text_leaf_height(font_size: f32) -> f32 {
    (font_size * 1.2).ceil()
}

pub(crate) struct DemoApp {
    // ── Widgets: owned by `ui_context`, named here by handle (narrow-trait adapters).
    button: Handle<Adapted<Button>>,
    toggle: Handle<Adapted<Toggle>>,
    slider: Handle<Adapted<Slider>>,
    name_box: Handle<Adapted<TextBox>>,
    theme_dropdown: Handle<Adapted<Dropdown>>,
    // ImageView pair sharing ONE uploaded texture (the widget borrows ids —
    // upload/free stay app-side): Contain letterboxes, Stretch fills.
    image_contain: Handle<Adapted<ImageView>>,
    image_stretch: Handle<Adapted<ImageView>>,
    // The Options dialog: a button that opens it, the plate, and what stands on it.
    options_button: Handle<Adapted<Button>>,
    dialog: Handle<Adapted<Dialog>>,
    text_size: Handle<Adapted<RadioGroup>>,
    dialog_cancel: Handle<Adapted<Button>>,
    dialog_ok: Handle<Adapted<Button>>,

    // ── App state: the source of truth. Widgets are re-asserted from it every rebuild
    // (`set_toggled` below); `take_*` changes flow back into it, never the reverse.
    toggle_on: bool,
    clicks: u32,
    status: String,
    /// The text size chosen in the Options dialog (an index into `TEXT_SIZES`).
    text_size_choice: usize,

    ui_context: UiContext,
    width: u32,
    height: u32,
    scale_factor: f64,
    needs_rebuild: bool,
    /// The dialog's members are laid out once, on the first frame.
    laid_out: bool,
    /// Set while `open_dialog` lays the members out before the dialog is open.
    dialog_pending: bool,
    title_rect: Rect,
    status_rect: Rect,
}

impl DemoApp {
    /// The widget root ids, in paint order — what the router dispatches over.
    fn root_ids(&self) -> [WidgetId; 12] {
        // While the dialog is open the rest are still dispatched to: the context answers
        // every one of them "covered", so none takes a press or a hover.
        [
            self.button.id(),
            self.toggle.id(),
            self.slider.id(),
            self.name_box.id(),
            self.theme_dropdown.id(),
            self.image_contain.id(),
            self.image_stretch.id(),
            self.options_button.id(),
            self.dialog.id(),
            self.text_size.id(),
            self.dialog_cancel.id(),
            self.dialog_ok.id(),
        ]
    }

    /// The dialog's members, in the order Tab walks them.
    fn dialog_members(&self) -> Vec<WidgetId> {
        vec![self.text_size.id(), self.dialog_cancel.id(), self.dialog_ok.id()]
    }

    /// Lay the dialog's members out in the middle of the window, shown while it is open.
    fn layout_dialog(&mut self) {
        let ui = &mut self.ui_context;
        let open = ui[self.dialog].inner().is_open() || self.dialog_pending;
        for id in [self.text_size.id(), self.dialog_cancel.id(), self.dialog_ok.id()] {
            if let Some(w) = ui.get_widget_mut(id) {
                w.set_visible(open);
            }
        }
        if !open {
            return;
        }
        let gap = cce_ui::layout::control_gap();
        let group_h = ui[self.text_size].intrinsic_size().map_or(0.0, |s| s.height);
        let (button_w, button_h) = (DIALOG_BUTTON_W, 28.0);
        let content_w = 2.0 * button_w + gap;
        let content_h = group_h + gap * 2.0 + button_h;
        let (pad, top) = (ui[self.dialog].inner().padding(), ui[self.dialog].inner().headroom());
        let x = (self.width as f32 - content_w) * 0.5;
        let y = (self.height as f32 - (top + content_h + pad)) * 0.5 + top;
        ui[self.text_size].set_rect(x, y, content_w, group_h);
        let by = y + group_h + gap * 2.0;
        ui[self.dialog_cancel].set_rect(x, by, button_w, button_h);
        ui[self.dialog_ok].set_rect(x + button_w + gap, by, button_w, button_h);
        let window = Rect { x: 0.0, y: 0.0, width: self.width as f32, height: self.height as f32 };
        ui[self.dialog].set_backdrop(Some(window));
        // The dialog fits itself around its members, which it reads through the context:
        // lent for the call, so it and the context are both in hand.
        ui.lend_h(self.dialog, |d, ui| d.fit(ui));
    }

    /// Open the Options dialog on the size the app holds.
    fn open_dialog(&mut self) {
        self.ui_context[self.text_size].inner_mut().set_selected(self.text_size_choice);
        self.dialog_pending = true;
        self.layout_dialog();
        self.dialog_pending = false;
        let members = self.dialog_members();
        self.ui_context.lend_h(self.dialog, |d, ui| d.open(ui, members));
        self.needs_rebuild = true;
    }

    /// Close it: `keep` takes the choice into the app, else it is dropped.
    fn close_dialog(&mut self, keep: bool) {
        if keep {
            self.text_size_choice = self.ui_context[self.text_size].inner().selected();
            self.status = format!("Text size: {}", TEXT_SIZES[self.text_size_choice]);
        }
        self.ui_context.lend_h(self.dialog, |d, ui| d.close(ui));
        self.layout_dialog();
        self.needs_rebuild = true;
    }

    /// `take_*` plumbing: translate widget changes into app state. Runs after any routed
    /// dispatch; every check is STATE-gated, so it does not matter which propagate call
    /// consumed the event (see the KeyInput note in `handle_key_input`).
    fn drain_widget_changes(&mut self) {
        let ui = &mut self.ui_context;
        let (options, ok, cancel) = (
            ui[self.options_button].take_click(),
            ui[self.dialog_ok].take_click(),
            ui[self.dialog_cancel].take_click(),
        );
        if ui[self.text_size].take_change() {
            self.needs_rebuild = true;
        }
        if ui[self.button].take_click() {
            self.clicks += 1;
            self.status = format!("Button clicked {} time(s)", self.clicks);
            self.needs_rebuild = true;
        }
        if ui[self.toggle].take_change() {
            self.toggle_on = !self.toggle_on;
            self.status = format!("Toggle: {}", if self.toggle_on { "on" } else { "off" });
            self.needs_rebuild = true;
        }
        if ui[self.slider].take_change() {
            self.status = format!("Slider: {:.0}", ui[self.slider].get_scaled_value());
            self.needs_rebuild = true;
        }
        if ui[self.theme_dropdown].take_change() {
            let dropdown = &ui[self.theme_dropdown];
            if let Some(opt) = dropdown.options.get(dropdown.selected) {
                self.status = format!("Theme: {opt}");
            }
            self.needs_rebuild = true;
        }
        if ui[self.name_box].take_change() {
            self.status = format!("Name: {}", ui[self.name_box].text);
            self.needs_rebuild = true;
        }
        if options {
            self.open_dialog();
        }
        if ok {
            self.close_dialog(true);
        }
        if cancel {
            self.close_dialog(false);
        }
    }
}

impl Application for DemoApp {
    type Message = DemoMessage;

    fn create(_sender: AppSender<Self::Message>) -> Self {
        cce_ui::scale::set_scale_factor(1.0);
        // One procedurally generated gradient (no asset dependency), uploaded
        // once and SHARED by both ImageViews — the widget borrows ids;
        // upload/free stay app-side. upload_rgba queues into the renderer's
        // pending list, so calling it before the first frame is safe.
        const GRADIENT_W: u32 = 64;
        const GRADIENT_H: u32 = 40;
        let mut gradient = Vec::with_capacity((GRADIENT_W * GRADIENT_H * 4) as usize);
        for y in 0..GRADIENT_H {
            for x in 0..GRADIENT_W {
                gradient.push((x * 255 / (GRADIENT_W - 1)) as u8);
                gradient.push((y * 255 / (GRADIENT_H - 1)) as u8);
                gradient.push(160);
                gradient.push(255);
            }
        }
        let gradient_id = cce_ui::draw::upload_rgba(gradient, GRADIENT_W, GRADIENT_H);
        // The context owns every widget; the app keeps the handles.
        let mut ui = UiContext::new();
        Self {
            // Relief styling (raised buttons/toggles/dropdowns, recessed
            // wells) is the `control_relief` config default — no opt-in.
            button: ui.insert(Button::new(0.0, 0.0, 0.0, 0.0).with_label("Click me")),
            toggle: ui.insert(Toggle::new()),
            // Slider `value` is NORMALIZED 0..1; `with_range` only scales the readout
            // (`get_scaled_value`). Wheel nudging is an explicit opt-in.
            slider: ui.insert(Slider::new().with_range(0.0, 100.0).with_value(0.4).with_scroll(true)),
            name_box: ui.insert(TextBox::new(String::new()).with_placeholder("Type a name...")),
            theme_dropdown: ui.insert(Dropdown::new(vec!["Forest".into(), "Ocean".into(), "Ember".into()], 0)),
            image_contain: ui.insert(
                ImageView::new()
                    .with_image(gradient_id, GRADIENT_W, GRADIENT_H)
                    .with_fit(FitMode::Contain { max_upscale: 4.0 })
                    .with_bg([0.10, 0.10, 0.16, 1.0]),
            ),
            image_stretch: ui.insert(ImageView::new().with_image(gradient_id, GRADIENT_W, GRADIENT_H).with_fit(FitMode::Stretch)),
            options_button: ui.insert(Button::new(0.0, 0.0, 0.0, 0.0).with_label("Options…")),
            dialog: ui.insert(Dialog::new().with_label("Text size")),
            text_size: ui.insert(RadioGroup::new(TEXT_SIZES).with_selected(1)),
            dialog_cancel: ui.insert(Button::new(0.0, 0.0, 0.0, 0.0).with_label("Cancel")),
            dialog_ok: ui.insert(Button::new(0.0, 0.0, 0.0, 0.0).with_label("OK")),
            text_size_choice: 1,
            dialog_pending: false,
            toggle_on: false,
            clicks: 0,
            status: "Ready.".to_string(),
            ui_context: ui,
            width: 560,
            height: 420,
            scale_factor: 1.0,
            needs_rebuild: true,
            laid_out: false,
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
        if !self.laid_out {
            self.laid_out = true;
            self.layout_dialog();
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
            let ui = &mut self.ui_context;
            ui[self.toggle].set_toggled(self.toggle_on);
            ui[self.toggle].set_label(if self.toggle_on { "ON" } else { "OFF" });

            // ── Layout: a plain LayoutBox tree, solved in one call. Leaves carry their
            // intrinsic sizes; `grow` distributes leftover space; the solved rects are
            // assigned straight onto the widgets.
            // DE-wide spacing by rung, never by number: the root preset insets
            // by the plate's roll plus one padding and spaces siblings by the
            // root gap; the controls preset puts the control gap between a
            // form's controls (`Style::root_column` / `Style::controls_row`).
            let mut arena: Arena<LayoutBox> = Arena::new();
            let root = arena.insert(LayoutBox::container(Style::root_column()));
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
                Style::controls_row().height(Length::Fixed(CONTROL_H)),
            ));
            // `shrink` lets the fixed leaves give up width when the window is at its
            // minimum instead of overflowing the row.
            let button = arena.insert(LayoutBox::leaf(Style::row().shrink(1.0), LSize::new(120.0, CONTROL_H)));
            let toggle = arena.insert(LayoutBox::leaf(Style::row(), LSize::new(64.0, CONTROL_H)));
            let dropdown = arena.insert(LayoutBox::leaf(Style::row().shrink(1.0), LSize::new(150.0, CONTROL_H)));
            let options = arena.insert(LayoutBox::leaf(Style::row().shrink(1.0), LSize::new(DIALOG_BUTTON_W, CONTROL_H)));
            let slider = arena.insert(LayoutBox::leaf(Style::row(), LSize::new(0.0, 24.0)));
            let name_box = arena.insert(LayoutBox::leaf(Style::row(), LSize::new(0.0, 30.0)));
            // ImageView row: same texture through two fit modes side by side.
            let images = arena.insert(LayoutBox::container(
                Style::row().gap(cce_ui::layout::root_plate_gap()).height(Length::Fixed(72.0)),
            ));
            let image_contain = arena.insert(LayoutBox::leaf(Style::row().grow(1.0), LSize::new(0.0, 72.0)));
            let image_stretch = arena.insert(LayoutBox::leaf(Style::row().grow(1.0), LSize::new(0.0, 72.0)));
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
            arena.append_child(controls, options);
            arena.append_child(root, slider);
            arena.append_child(root, name_box);
            arena.append_child(root, images);
            arena.append_child(images, image_contain);
            arena.append_child(images, image_stretch);
            arena.append_child(root, spacer);
            arena.append_child(root, status);
            compute_layout(
                &mut arena,
                root,
                LSize::new(self.width as f32, self.height as f32),
            );

            // Stretched children fill the column width; fixed leaves keep their size.
            let r = |id| arena.value(id).unwrap().rect;
            let placed: [(WidgetId, Rect); 8] = [
                (self.button.id(), r(button)),
                (self.toggle.id(), r(toggle)),
                (self.theme_dropdown.id(), r(dropdown)),
                (self.options_button.id(), r(options)),
                (self.slider.id(), r(slider)),
                (self.name_box.id(), r(name_box)),
                (self.image_contain.id(), r(image_contain)),
                (self.image_stretch.id(), r(image_stretch)),
            ];
            for (id, b) in placed {
                if let Some(w) = self.ui_context.get_widget_mut(id) {
                    w.set_rect(b.x, b.y, b.width, b.height);
                }
            }
            self.title_rect = r(title);
            self.status_rect = r(status);
            self.layout_dialog();

            self.needs_rebuild = false;
            self.ui_context.rebuild_spatial_grid();
        }

        // ── Popover registration: ui_context ONLY. It drives the engine's display-list
        // text occlusion clamp (labels under the open popover get clipped); the popover
        // itself is drawn into this frame below — there is no popup surface.
        self.ui_context.clear_popovers();
        if self.ui_context[self.theme_dropdown].popover_rect().is_some() {
            self.ui_context.register_popover_id(self.theme_dropdown.id());
        }

        let mut pc = PaintCtx::new();
        let w = self.width as f32;
        let h = self.height as f32;

        // The standard root plate (`PlateSpec::window`): the DE's root
        // material at its configured opacity, the shared silhouette arc on
        // all four corners, the perimeter rolled over `bevel_width`. This
        // demo is the reference app, so its base is the one every cce app
        // should paint first.
        pc.root_plate(w, h);

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
        // one facing the content. (Neither band CSG-groups: edge-suppressed
        // carves never do — their extended walls would smear across the
        // plate's whole-surface draw. Both shade through the overlay fallback,
        // whose host-box fade owns the junction with the roll.) Sized from
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
        let ui = &self.ui_context;
        let roots = [
            self.button.id(),
            self.toggle.id(),
            self.slider.id(),
            self.name_box.id(),
            self.image_contain.id(),
            self.image_stretch.id(),
            self.theme_dropdown.id(),
            self.options_button.id(),
        ];
        for id in roots {
            if let Some(w) = ui.get_widget(id) {
                cce_ui::scene::painter::paint_root_into(ui, w, &mut pc);
            }
        }

        // The dropdown popover — geometry and labels last, on top of everything, exactly
        // where it hit-tests. Labels carry bounds equal to the popover rect: that clips
        // them to the plate AND exempts them from the occlusion clamp (text whose bounds
        // equal an overlay rect is treated as the overlay's own).
        if ui[self.theme_dropdown].popover_rect().is_some() {
            // PaintCtx is a RenderTarget: the popover draws its real prims (the
            // dropdown's expanded inset-plate surface) with its own bounds.
            ui[self.theme_dropdown].render_popover(&mut pc);
        }

        // The dialog over everything: its backdrop, its plate, and its members on the plate
        // (it paints them; they are never painted on their own).
        cce_ui::scene::painter::paint_root_into(ui, &ui[self.dialog], &mut pc);

        Some(pc.finish())
    }

    /// Text prims in the display list ARE the frame's text — no `text_items` twin.
    fn display_list_text(&self) -> bool {
        true
    }

    fn ui_context(&self) -> Option<&cce_ui::context::UiContext> {
        Some(&self.ui_context)
    }

    // Engine-driven animation frames for the dropdown expand/contract.
    fn ui_context_mut(&mut self) -> Option<&mut cce_ui::context::UiContext> {
        Some(&mut self.ui_context)
    }

    /// Window dragging for a dissolved root: the surface is the movable plate; drag
    /// anywhere a drag-blocking registered widget isn't.
    fn is_movable_root_plate_at(&self, px: f32, py: f32) -> bool {
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
        let ev = Event::MouseWheel { delta: *delta, x: px, y: py, local_x: px, local_y: py };
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

        // Escape cancels the open dialog, as its Cancel does.
        if self.ui_context[self.dialog].inner().is_open()
            && event.state == ElementState::Pressed
            && event.logical_key == cce_ui::widget::Key::Named(NamedKey::Escape)
        {
            self.close_dialog(false);
            *needs_rebuild = true;
            return None;
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
    // In a browser the same app is run by `examples/demo_web.rs`.
    #[cfg(not(target_arch = "wasm32"))]
    cce_ui::engine::run::<DemoApp>();
}
