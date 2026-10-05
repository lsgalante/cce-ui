//! The client contract: the `Application` trait and the plain types it
//! speaks in (`WindowSettings`, `LayerSettings`, `LogicalPosition`, …).
//! Moved out of `window_runner` unchanged; still re-exported from there
//! (and from `engine`) at the old paths.

use cosmic_text::FontSystem;
use crate::widget::{MouseButton, ElementState, MouseScrollDelta, KeyEvent};
#[cfg(not(target_arch = "wasm32"))]
use wayland_client::QueueHandle;
#[cfg(not(target_arch = "wasm32"))]
use crate::vk::VkRenderer;
#[cfg(not(target_arch = "wasm32"))]
use super::window_runner::EngineState;
use cursor_icon::CursorIcon;
use super::tessellate::Vertex;
pub use crate::draw::scene::Stage3D;

#[derive(Debug, Clone)]
pub struct WindowSettings {
    pub title: String,
    pub app_id: String,
    pub width: u32,
    pub height: u32,
    pub fullscreen: bool,
    pub min_size: Option<(u32, u32)>,
}

/// The edge a [`WindowAction::Resize`] grabs. On Wayland it is
/// `xdg_toplevel`'s own enum, as it always was; elsewhere the driver's.
#[cfg(not(target_arch = "wasm32"))]
pub use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel::ResizeEdge as WindowEdge;
#[cfg(target_arch = "wasm32")]
pub use super::driver::ResizeEdge as WindowEdge;

/// A compositor-side window operation requested by the app: an interactive
/// move or resize grab. Returned from [`Application::take_window_action`];
/// the runner executes it with the serial of the most recent pointer press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowAction {
    Move,
    Resize(WindowEdge),
}

// Re-export the wlr-layer-shell types apps need to describe a layer surface.
// Layer surfaces are a Wayland (wlr) notion: native only, like `layer()`.
#[cfg(not(target_arch = "wasm32"))]
pub use smithay_client_toolkit::shell::wlr_layer::{
    Anchor as LayerAnchor, KeyboardInteractivity as LayerKeyboardInteractivity, Layer as LayerKind,
};

/// Opt-in configuration for running an [`Application`] on a wlr-layer-shell
/// surface (panels, overlays, notifications) instead of an xdg toplevel.
/// Return one from [`Application::layer`] to select layer-shell.
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct LayerSettings {
    pub layer: LayerKind,
    pub anchor: LayerAnchor,
    pub exclusive_zone: i32,
    pub keyboard_interactivity: LayerKeyboardInteractivity,
    /// (top, right, bottom, left) margins in logical pixels.
    pub margin: (i32, i32, i32, i32),
    pub namespace: String,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalPosition {
    pub x: f32,
    pub y: f32,
}

impl LogicalPosition {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LogicalSize {
    pub width: f32,
    pub height: f32,
}

impl LogicalSize {
    pub fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

pub struct RenderContext<'a> {
    pub font_system: &'a mut FontSystem,
}

/// The app's handle for posting a message to itself: from a worker thread, a
/// callback, a timer the app runs itself. Each message reaches
/// [`Application::update`] on the UI thread and wakes an idle loop, so
/// background results arrive without polling (see "`tick` is not a clock"
/// in CLAUDE.md).
///
/// It names no window system. Handed to [`Application::create`], it is what
/// a client written against it can be run by any shell with; the Wayland
/// runner backs it with its calloop channel, and `From` converts both ways
/// for a client that still keeps a `calloop::channel::Sender` somewhere. In
/// the browser it is a `std::sync::mpsc` sender whose receiver the shell
/// drains every turn — still `Send`, so app code that hands it to a worker
/// compiles unchanged — and a send wakes the browser shell's loop through
/// [`set_wake`], as a calloop channel wakes the Wayland one.
pub struct AppSender<M> {
    #[cfg(not(target_arch = "wasm32"))]
    inner: calloop::channel::Sender<M>,
    #[cfg(target_arch = "wasm32")]
    inner: std::sync::mpsc::Sender<M>,
}

impl<M> AppSender<M> {
    /// Post `msg` to the app. Fails, handing it back, only once the loop is
    /// gone: the app has exited.
    pub fn send(&self, msg: M) -> Result<(), std::sync::mpsc::SendError<M>> {
        self.inner.send(msg)?;
        #[cfg(target_arch = "wasm32")]
        wake();
        Ok(())
    }
}

/// Call the hook [`set_wake`] installed, if any.
#[cfg(target_arch = "wasm32")]
pub(crate) fn wake() {
    WAKE.with(|w| {
        if let Some(wake) = w.borrow().as_ref() {
            wake();
        }
    });
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WAKE: std::cell::RefCell<Option<Box<dyn Fn()>>> = const { std::cell::RefCell::new(None) };
}

/// What an [`AppSender::send`] on this thread calls after posting: the
/// browser shell's "turn the loop soon". Per thread, and the page's app runs
/// on the one thread a page has; a send from a worker thread posts without
/// waking, and is drained at the next turn the page takes.
#[cfg(target_arch = "wasm32")]
pub fn set_wake(wake: Option<Box<dyn Fn()>>) {
    WAKE.with(|w| *w.borrow_mut() = wake);
}

impl<M> Clone for AppSender<M> {
    fn clone(&self) -> Self {
        Self { inner: self.inner.clone() }
    }
}

impl<M> std::fmt::Debug for AppSender<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AppSender")
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<M> From<calloop::channel::Sender<M>> for AppSender<M> {
    fn from(inner: calloop::channel::Sender<M>) -> Self {
        Self { inner }
    }
}

#[cfg(target_arch = "wasm32")]
impl<M> From<std::sync::mpsc::Sender<M>> for AppSender<M> {
    fn from(inner: std::sync::mpsc::Sender<M>) -> Self {
        Self { inner }
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl<M> From<AppSender<M>> for calloop::channel::Sender<M> {
    fn from(sender: AppSender<M>) -> Self {
        sender.inner
    }
}

pub trait Application: Sized + 'static {
    type Message: Send + Clone + 'static;

    /// Build the app. Implement this or the legacy [`new`](Self::new), not
    /// both: the runner calls `new`, whose default forwards here. `create`
    /// takes no Wayland type, so it is the constructor another shell can
    /// call; `new` goes once no client implements it.
    ///
    /// Implementing neither panics at startup, naming the app: the price of
    /// letting clients move one at a time.
    fn create(sender: AppSender<Self::Message>) -> Self {
        let _ = sender;
        panic!(
            "{}: implement Application::create (or the legacy Application::new)",
            std::any::type_name::<Self>()
        )
    }

    /// The legacy constructor, from before the runner had a second shell in
    /// view: the session's Wayland queue handle (no client ever used it) and
    /// its calloop sender. Prefer [`create`](Self::create); the default here
    /// forwards to it. Native only: it names the Wayland queue.
    #[cfg(not(target_arch = "wasm32"))]
    fn new(qh: &QueueHandle<EngineState<Self>>, sender: calloop::channel::Sender<Self::Message>) -> Self {
        let _ = qh;
        Self::create(AppSender::from(sender))
    }
    fn settings(&self) -> WindowSettings;
    /// Return `Some(..)` to run on a wlr-layer-shell surface (overlay/panel)
    /// instead of an xdg toplevel. Defaults to `None` (a normal window).
    #[cfg(not(target_arch = "wasm32"))]
    fn layer(&self) -> Option<LayerSettings> {
        None
    }
    /// Declare the window a UTILITY window: a tool whose shape is decided by
    /// its contents. The compositor then never dictates a size to it (every
    /// configure is the "you choose" 0x0 — [`WindowSettings::width`]/`height`
    /// become the surface's own initial size), offers no resize affordance
    /// (the whole border band moves the window), and never saves geometry
    /// for it, so a stale remembered size can't be restored over what the
    /// app asks for. Declared over the cce window-management protocol at
    /// window creation; on a compositor too old to know the request this is
    /// silently a plain floating window. Defaults to `false`.
    fn utility(&self) -> bool {
        false
    }
    /// Declare the window the DESKTOP-GRID layer (zcce set_grid): the
    /// compositor world-anchors the surface to the virtual desktop and
    /// pans/zooms it per frame like window content; the app renders only
    /// when handed a patch (see [`Application::grid_patch`]). The surface
    /// becomes input-transparent and lives behind all windows. Needs
    /// manager v6; on an older compositor the declaration is skipped.
    /// Defaults to `false`.
    fn grid(&self) -> bool {
        false
    }
    /// A grid patch to render (grid apps only): virtual origin (`x`, `y`),
    /// virtual size (`w`, `h`), and `scale` surface px per virtual unit.
    /// Called right before the frame that must show it; the runner has
    /// already resized the surface to `(w*scale, h*scale)` and acks the
    /// patch so the coming commit is latched at the new anchor.
    fn grid_patch(&mut self, _x: f64, _y: f64, _w: f64, _h: f64, _scale: f64) {}
    fn update(&mut self, msg: Self::Message, needs_rebuild: &mut bool, exit: &mut bool);
    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool);
    /// How long the runner may sleep between `tick`s while the window is
    /// idle — nothing to draw, no animation, no key held, no frame callback
    /// outstanding. `None` (the default) lets it sleep until a Wayland
    /// event or a message on the app's calloop `Sender` arrives, bounded by
    /// [`IDLE_DISPATCH`]. Override with `Some` ONLY if your `tick` polls
    /// something the loop cannot see — a `std::sync::mpsc` receiver drained
    /// in `tick`, say — because with the default that poll waits for the
    /// next unrelated event. The better fix is to send through the calloop
    /// `Sender` handed to `new`, which wakes the loop by itself.
    fn idle_poll_interval(&self) -> Option<std::time::Duration> {
        None
    }
    /// On-top overlay quads drawn after the display list and its text (e.g. the status bar's
    /// tray-hover highlights). Deliberately separate from the single paint path.
    fn overlay_quads(&mut self, _quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>, _size: LogicalSize, _scale: f64) {}
    /// The part of the surface that changed since the last frame this app
    /// painted, as (x, y, w, h) in logical px, taken (and reset) once per
    /// rendered frame right after `display_list`. `None` — the default —
    /// means all of it. Returning a rect makes the frame a partial one: only
    /// that rect is repainted and only it is reported to the compositor as
    /// damage, which is what keeps a small change on a very large surface
    /// (an image dragged across the desktop grid) from costing a full
    /// repaint on both sides. The app vouches for the rect: anything that
    /// changed outside it keeps its old pixels. A frame that was skipped is
    /// the runner's to make up — the next one is painted in full.
    fn take_damage(&mut self, _size: LogicalSize, _scale: f64) -> Option<(f32, f32, f32, f32)> {
        None
    }
    fn input_regions(&self) -> Option<Vec<(i32, i32, i32, i32)>> {
        None
    }

    /// Transparent overflow rim, in logical px, on the RIGHT and BOTTOM of
    /// the window. Non-zero opts into buffer-larger-than-geometry mode: the
    /// runner sizes the surface `margin` wider/taller than the configured
    /// window size, publishes the top-left rect as the xdg window geometry
    /// (what the compositor tiles, borders, and snaps) and an input region of
    /// the frame plus any open popover rects — an overhanging menu stays
    /// clickable while empty rim falls through to whatever is behind.
    ///
    /// Right/bottom ONLY, deliberately: the surface grows away from its
    /// origin, so the frame never moves relative to the surface and pointer
    /// coordinates stay valid across the resize (a leading rim shifts the
    /// surface under an unmoved cursor, and the compositor's stale pointer
    /// state then drops the very next click). Frame coords == surface coords:
    /// no input translation, no paint shift — the app's only obligation is to
    /// lay out against the frame (`display_list`'s `size` minus the margin);
    /// content emitted past the frame edge renders in the rim instead of
    /// clipping at the buffer edge.
    ///
    /// The value may change at runtime (return the popover overhang while a
    /// menu is open, 0 otherwise): the engine re-derives the surface from the
    /// stored frame and resizes on drift. Quantize the answer (e.g. 64px
    /// steps) so an animating popover doesn't resize the surface per frame.
    /// xdg toplevels only (layer surfaces ignore it).
    fn overflow_margin(&self) -> u32 {
        0
    }

    fn desired_size(&self) -> Option<(u32, u32)> {
        None
    }
    
    fn ui_context(&self) -> Option<&crate::context::UiContext> {
        None
    }

    fn ui_context_mut(&mut self) -> Option<&mut crate::context::UiContext> {
        None
    }

    /// Where a widget's open popover is DRAWN, as an offset from the rect it
    /// reports (`popover_rect`). A widget reports in the coordinates it was
    /// laid out in; an app that lays its page out unscrolled and shifts what
    /// it emits draws the popover `scroll` px away from there, and returns
    /// `(0.0, -scroll_y)` here for the page's widgets. Everything the engine
    /// derives from a popover rect reads it through this: the text-occlusion
    /// clamp, the overflow input region, and the region sent to the
    /// compositor. Without it a menu opened on a scrolled page had the page's
    /// text drawn over it, and cut a menu-shaped hole in the text one scroll
    /// offset away.
    fn popover_offset(&self, _id: crate::widget::WidgetId) -> (f32, f32) {
        (0.0, 0.0)
    }

    /// Whether a left-press at (px, py) should start a compositor window drag. Every root
    /// root plate container is dissolved (Phase 6), so the default is "no" — apps that want
    /// drag-anywhere override this with `ctx.drag_allowed_at(px, py)`.
    fn is_movable_root_plate_at(&self, _px: f32, _py: f32) -> bool {
        false
    }
    
    fn clear_color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn register_sources(&mut self, _handle: &calloop::LoopHandle<'_, EngineState<Self>>) {}

    fn adjust_size(&self, width: f32, height: f32) -> (f32, f32) {
        (width, height)
    }
    
    /// Mime types this app accepts from a drag, in the app's own preference
    /// order (the source's order is ignored — a browser lists `text/html`
    /// before `text/uri-list` and which is more useful is the app's call).
    /// The default is empty: the app accepts nothing and drags over it read
    /// as "can't drop here", which is what every client did before drops
    /// existed. Opting in also requires [`Application::handle_drop`].
    fn drop_mimes(&self) -> &'static [&'static str] {
        &[]
    }

    /// A completed drop: `data` is everything the source wrote for `mime`,
    /// and `pos` is where it was released in the app's logical coordinates.
    /// Runs on the main loop, after the transfer finished — this is not the
    /// place to block, since the compositor is waiting on the next frame.
    fn handle_drop(
        &mut self,
        _mime: &str,
        _data: &[u8],
        _pos: LogicalPosition,
        _needs_rebuild: &mut bool,
    ) {
    }

    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool);
    fn handle_mouse_input(&mut self, button: MouseButton, state: ElementState, pos: LogicalPosition, needs_rebuild: &mut bool) -> Option<Self::Message>;
    fn handle_mouse_wheel(&mut self, delta: &MouseScrollDelta, pos: LogicalPosition, needs_rebuild: &mut bool);
    /// Trackpad pinch (zwp_pointer_gestures pinch). `factor` is the scale
    /// change SINCE THE LAST update (1.0 = no change, >1 = fingers spreading),
    /// so direct-manipulation zoom is `content_scale *= factor`. Return true
    /// to consume; returning false falls back to the engine's legacy
    /// synthesis — a ctrl+wheel PixelDelta sized for the graph's zoom mapping
    /// (`y = (factor-1)/0.015`) — so ctrl-scroll-zoom surfaces keep working
    /// without implementing this.
    fn handle_pinch(&mut self, _factor: f32, _pos: LogicalPosition, _needs_rebuild: &mut bool) -> bool {
        false
    }
    fn handle_key_input(&mut self, event: &KeyEvent, needs_rebuild: &mut bool) -> Option<Self::Message>;

    /// Undo, after the focused widget declined the chord (a text box that is
    /// editing takes it for its own typing). Return true when something was
    /// undone; false lets the key fall through to `handle_key_input` like any
    /// other. The chords are `undo` / `redo` in `input.kdl` (cce-ui domain
    /// defaults `ctrl+z` / `ctrl+shift+z`), resolved once at startup. Build
    /// the history on `cce_ui::history::History`.
    fn undo(&mut self, _needs_rebuild: &mut bool) -> bool {
        false
    }

    /// Redo — see [`undo`](Self::undo).
    fn redo(&mut self, _needs_rebuild: &mut bool) -> bool {
        false
    }

    /// Opt into the toolkit's keyboard navigation in plate terms: Tab and
    /// Shift+Tab move focus to the next / previous plate or well in reading
    /// order (`UiContext::focus_step`), a press (Enter / Space) acts on the
    /// focused plate, a well opens for typing when focused. Default false: an
    /// app that routes Tab itself (a terminal, a web view, its own field
    /// order) is undisturbed. See "Plates, wells and seams" in `CLAUDE.md`.
    fn plate_navigation(&self) -> bool {
        false
    }

    /// Wait for the NEXT compositor when this one goes away, instead of
    /// exiting. Default false, which is right for any window the compositor
    /// saves and restores: its successor respawns the app itself, and a
    /// client that rejoined too came up beside its own copy (see
    /// [`after_session`]). Return true from a process the compositor does NOT
    /// restore and that must outlive it — a systemd user service like the
    /// status bar or the notifier, whose D-Bus names (the tray's
    /// StatusNotifierWatcher, org.freedesktop.Notifications) other programs
    /// depend on. Exiting took those names down at every logout and
    /// compositor restart, and Dropbox, starting into the gap, found no tray.
    fn outlives_compositor(&self) -> bool {
        false
    }

    /// Keyboard focus just moved by the toolkit's Tab traversal. An app that
    /// caches its geometry until its own rebuild flag (relief carves collected
    /// in a view pass, widget lists built on layout) raises that flag here, so
    /// the new ring is drawn; an app that paints fresh every frame needs
    /// nothing. Default: nothing.
    fn focus_stepped(&mut self) {}
    /// Keyboard focus entered/left the window (the compositor keyboard-focuses
    /// the focused window, so this is the "am I the focused window" signal —
    /// e.g. for focus-dependent chrome). Default: ignore.
    fn handle_focus_change(&mut self, _focused: bool, _needs_rebuild: &mut bool) {}

    fn custom_vertices(&mut self, _verts: &mut Vec<Vertex>, _size: LogicalSize, _scale: f64) {}

    /// The frame's geometry, drawn via one batched, GPU-scissor-clipped pass (the single
    /// paint path). Every rendering app implements this — the legacy `view*` sinks are gone;
    /// `None` yields an empty frame. Overlays ([`overlay_quads`](Application::overlay_quads))
    /// and [`custom_vertices`](Application::custom_vertices) still go through their own paths;
    /// text renders from the list when [`display_list_text`](Application::display_list_text)
    /// opts in. Receives the frame's logical size and HiDPI scale. Typically implemented as
    /// `Some(cce_ui::scene::painter::paint_tree(&self.ui_context, &self.root))`.
    fn display_list(&mut self, _size: LogicalSize, _scale: f64) -> Option<crate::scene::paint::DisplayList> {
        None
    }

    /// Opt in to render the display list's `Prim::Text` items through the glyph pass
    /// (shaped via the shared buffer cache, clipped to the item clip ∩ the prim bounds). An
    /// app's ENTIRE frame — geometry and text — is then one
    /// [`display_list`](Application::display_list). Default `false` draws no text (an app that
    /// only draws geometry, or none at all).
    ///
    /// Display-list text gets the same popover-occlusion clamp as the legacy `text_areas`
    /// mapping (`popover_occlusion_clamp`, driven by `ui_context().active_popovers`), so an
    /// open popover's plate clips list text beneath it on both paths.
    fn display_list_text(&self) -> bool {
        false
    }

    /// Opt into system fonts in the ENGINE's render `FontSystem` (the one that shapes
    /// display-list text and rasterizes every glyph at prepare time). Default `false`: the
    /// render FontSystem loads only the bundled CCE fonts, and text asking for a family that
    /// exists only among installed system fonts is silently invisible — buffers shaped
    /// app-side against a system-fonts `FontSystem` carry fontdb face IDs the engine's
    /// database doesn't have (the cce-colors Phase 6e bug). An app whose UI must render
    /// arbitrary installed families (the font picker) returns `true`; its own `FontSystem`,
    /// if it keeps one for measurement, should be `create_font_system_with_system_fonts()`
    /// so both databases load identically. Consulted once, at GPU init.
    fn load_system_fonts(&self) -> bool {
        false
    }

    /// Called once per renderer, right after it is created and before its
    /// first frame (a reconnect's replacement too — see "`renderer_init`"
    /// in CLAUDE.md): create persistent renderer resources here (3D meshes
    /// via [`VkRenderer::create_mesh`]). Most 2D apps never need this. The
    /// default hands the renderer to the portable [`init_3d`](Self::init_3d),
    /// so an app written against that runs here unchanged.
    #[cfg(not(target_arch = "wasm32"))]
    fn renderer_init(&mut self, renderer: &mut VkRenderer) {
        self.init_3d(renderer);
    }

    /// Direct renderer staging, called every frame after the engine's own text
    /// prep and immediately before the frame is drawn: stage 3D scene panes
    /// (`stage_scene`), path-traced panes (`stage_rt`), flush mesh updates, or
    /// prepare app-shaped text (`prepare_text` — an app that returns `false`
    /// from [`display_list_text`](Application::display_list_text) fully owns
    /// the renderer's text state, the engine never touches it). Return `true`
    /// to request another frame immediately (e.g. while a path tracer is still
    /// accumulating samples). The default is the portable
    /// [`stage_3d`](Self::stage_3d).
    #[cfg(not(target_arch = "wasm32"))]
    fn stage_renderer(&mut self, renderer: &mut VkRenderer, size: LogicalSize, scale: f64) -> bool {
        self.stage_3d(renderer, size, scale)
    }

    /// The portable [`renderer_init`](Self::renderer_init): once per
    /// renderer, before its first frame, through [`Stage3D`] — the 3D half
    /// every renderer has, the Vulkan one natively and the WebGPU one in a
    /// browser. Make the app's meshes here. Called by the browser shell, and
    /// natively by `renderer_init`'s default.
    fn init_3d(&mut self, _stage: &mut dyn Stage3D) {}

    /// The portable [`stage_renderer`](Self::stage_renderer): every frame,
    /// just before it is drawn, stage the scene through [`Stage3D`]. `size`
    /// is the window's logical size and `scale` its pixel ratio; a scissor is
    /// physical px. Return `true` to ask for another frame at once.
    fn stage_3d(&mut self, _stage: &mut dyn Stage3D, _size: LogicalSize, _scale: f64) -> bool {
        false
    }

    /// The surface was resized (or the scale factor changed): `width`/`height`
    /// are the new logical size. The renderer has already been resized; use
    /// this for stateful relayout that can't wait for the next paint callback.
    fn handle_resize(&mut self, _width: f32, _height: f32, _scale: f64) {}

    /// Whether the runner's built-in client-side decorations apply: the
    /// titlebar move band, the movable-root plate drag regions, and — when
    /// [`csd_resize_borders`](Application::csd_resize_borders) is also on —
    /// the rect-edge resize grabs and their edge cursors. Return `false` for a
    /// window whose chrome doesn't follow its rect (e.g. a circular pane) and
    /// drive moves/resizes yourself via
    /// [`take_window_action`](Application::take_window_action).
    fn standard_csd(&self) -> bool {
        true
    }

    /// Whether the standard CSD claims the outer 8px of the surface as resize
    /// grabs (with matching edge cursors). Off by default: under the cce
    /// compositor the server already provides a resize band just *outside* the
    /// window, so enabling this gives a window two adjacent 8px gutters driven
    /// by different code paths — and only the compositor's snaps to the
    /// desktop grid. It also costs the app clicks, since a press inside the
    /// band starts a grab and never reaches the widgets underneath.
    ///
    /// Turn it on for a window that must be resizable by its own edges under a
    /// compositor that provides no such affordance. Only consulted when
    /// [`standard_csd`](Application::standard_csd) is on.
    fn csd_resize_borders(&self) -> bool {
        false
    }

    /// Whether the standard CSD reserves an implicit title-bar strip (`y` in `[8, 32)`) as a
    /// drag-to-move handle. Opt-in: off by default, so a window has no title bar and is moved
    /// through the compositor (or via explicitly-declared handles —
    /// [`is_movable_root_plate_at`](Application::is_movable_root_plate_at)); nothing is
    /// implicitly draggable. An app with an actual title bar returns `true`. Separate from
    /// [`standard_csd`](Application::standard_csd), which also gates the resize borders, and
    /// only consulted when `standard_csd()` is on.
    fn csd_titlebar_move(&self) -> bool {
        false
    }

    /// Override the pointer cursor at (x, y). `None` falls back to the
    /// runner's standard CSD edge cursors (or `Default` when
    /// [`standard_csd`](Application::standard_csd) is off).
    fn cursor_icon(&self, _x: f32, _y: f32) -> Option<CursorIcon> {
        None
    }

    /// Polled after each pointer frame is dispatched: return a
    /// [`WindowAction`] to start an interactive move/resize grab with the
    /// serial of the most recent pointer press. This is take-semantics — the
    /// implementation should clear its pending action when returning it.
    fn take_window_action(&mut self) -> Option<WindowAction> {
        None
    }

    /// Called once when the event loop ends (window closed, app-requested
    /// exit): last-chance work like autosave. The surface is still alive.
    fn on_exit(&mut self) {}
}


#[cfg(all(test, not(target_arch = "wasm32")))]
mod app_sender_tests {
    use super::AppSender;
    use calloop::channel::{channel, Event};

    #[test]
    fn an_app_sender_delivers_through_the_loop_and_fails_once_it_is_gone() {
        let (tx, rx) = channel::<u32>();
        let sender = AppSender::from(tx);
        let worker = sender.clone();
        std::thread::spawn(move || worker.send(7).unwrap()).join().unwrap();
        sender.send(8).unwrap();

        let mut event_loop = calloop::EventLoop::<Vec<u32>>::try_new().unwrap();
        let token = event_loop
            .handle()
            .insert_source(rx, |event, _, got: &mut Vec<u32>| {
                if let Event::Msg(m) = event {
                    got.push(m);
                }
            })
            .unwrap();
        let mut got = Vec::new();
        event_loop.dispatch(Some(std::time::Duration::ZERO), &mut got).unwrap();
        assert_eq!(got, vec![7, 8]);

        // The loop dropping its end is the app having exited: the message
        // comes back to the sender rather than vanishing.
        event_loop.handle().remove(token);
        assert_eq!(sender.send(9).unwrap_err().0, 9);

        // And the escape hatch for a client still holding calloop's type.
        let _raw: calloop::channel::Sender<u32> = sender.into();
    }
}
