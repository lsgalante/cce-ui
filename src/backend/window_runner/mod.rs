//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the re-exports clients reach the runner through, `EngineState`, GPU init, geometry and resize |
//! | `present` | a turn's render and present, the text input's sync, the `Shell` impl |
//! | `layer` | a layer surface's role, dropped while the app wants no surface and re-attached after |
//! | `handlers` | the SCTK handlers (compositor, output, shm, registry, window, layer shell, seat) and delegates |
//! | `pointer` | pointer input and the pinch gesture, mapped into the driver |
//! | `keyboard` | keyboard input through xkb, and the text input (`text-input-v3`) |
//! | `protocols` | the cce inspector and window-management protocols |
//! | `session` | `run`: sessions, reconnects, and waiting out a compositor restart |

mod handlers;
mod keyboard;
mod layer;
mod pointer;
mod present;
mod protocols;
mod session;

pub use session::run;
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    data_device_manager::DataDeviceManagerState,
    delegate_compositor, delegate_keyboard, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, delegate_output,
    delegate_layer,
    registry::{ProvidesRegistryState, RegistryState},
    output::{OutputHandler, OutputState},
    seat::{
        keyboard::KeyboardHandler,
        pointer::{PointerHandler, ThemedPointer, ThemeSpec, CursorIcon},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        xdg::{
            window::{Window as XdgWindow, WindowConfigure, WindowHandler, WindowDecorations},
            XdgShell, XdgSurface as XdgSurfaceExt,
        },
        wlr_layer::{LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure},
        WaylandSurface,
    },
    shm::{Shm, ShmHandler},
};
use wayland_client::{
    globals::{registry_queue_init, GlobalList},
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_touch, wl_surface, wl_registry, wl_region, wl_callback},
    Connection, QueueHandle, Proxy,
};

use wayland_protocols::wp::text_input::zv3::client::{
    zwp_text_input_manager_v3::ZwpTextInputManagerV3,
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use wayland_protocols::wp::pointer_gestures::zv1::client::{
    zwp_pointer_gesture_pinch_v1::{self, ZwpPointerGesturePinchV1},
    zwp_pointer_gestures_v1::{self as zwp_pointer_gestures, ZwpPointerGesturesV1},
};
pub use smithay_client_toolkit::reexports::protocols::xdg::shell::client::xdg_toplevel;
pub use smithay_client_toolkit::seat::pointer::CursorIcon as PointerCursorIcon;
use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use cosmic_text::FontSystem;
use crate::widget::{MouseButton, ElementState, Key, NamedKey};
use crate::wayland::detect_scale_factor;
use crate::vk::VkRenderer;

pub use super::app::*;
pub use super::driver::PressedKey;
use super::frame::build_frame;
use super::shell::{Pacer, Shell, Step, ACTIVE_DISPATCH};
use super::driver::{Driver, Modifiers, Press, PressSite, ResizeEdge, ScrollFrame, ScrollSource, Turn};
pub use super::tessellate::*;
pub use super::text::*;

pub use super::shell::IDLE_DISPATCH;

/// The `CCE_PRESENT_DEBUG` traces' timestamp: wall-clock milliseconds, mod
/// 100 s, short enough to read down a column of lines.
fn debug_clock_ms() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() % 100_000)
}

pub struct EngineState<A: Application> {
    /// The session's accessibility publisher, for an app that publishes its tree
    /// (`backend::a11y_unix`).
    pub a11y: Option<crate::backend::a11y_unix::Publisher>,
    pub registry_state: RegistryState,
    pub compositor_state: CompositorState,
    pub xdg_shell_state: XdgShell,
    pub layer_shell_state: Option<LayerShell>,
    pub shm_state: Shm,
    pub seat_state: SeatState,
    pub output_state: OutputState,
    pub seats: Vec<wl_seat::WlSeat>,
    pub pointer: Option<ThemedPointer>,
    pub keyboard: Option<wl_keyboard::WlKeyboard>,

    pub window: Option<XdgWindow>,
    pub layer_surface: Option<LayerSurface>,
    /// This session's surface is a layer surface ([`Application::layer`]),
    /// which is what makes [`Application::wants_surface`] apply.
    pub is_layer_app: bool,
    /// The app said [`Application::wants_surface`] = false and the layer
    /// surface is gone until it says true (the renderer is kept, detached).
    pub layer_hidden: bool,
    pub surface: Option<wl_surface::WlSurface>,
    
    pub inner: Option<A>,
    
    pub renderer: Option<VkRenderer>,
    pub font_system: Option<FontSystem>,
    pub swash_cache: cosmic_text::SwashCache,
    
    pub scale_factor: f64,
    /// The buffer scale last sent to the surface. Updated in [`Self::render`],
    /// paired with the present that commits a matching-size buffer — never on
    /// the scale event itself, which races in-flight presents of old buffers.
    pub committed_buffer_scale: i32,
    /// Outputs the surface has entered and not left. Used by
    /// `scale_factor_changed` to reject the SCTK no-outputs fallback: on
    /// suspend/resume the DRM connector is destroyed and re-created, the
    /// surface briefly sits on zero (live) outputs, and SCTK reports scale 1.
    /// Acting on that report rebuilds the buffer at scale-1 size while the
    /// surface's latched scale can still be 2 — a fatal `invalid_size`
    /// protocol error for odd-sized surfaces (the status bar crash-loop on
    /// every resume) and a silently HALF-SIZE window for even-sized ones
    /// (the compositor reads buffer/scale as a self-resize and the halving
    /// sticks, compounding per resume).
    pub entered_outputs: Vec<wl_output::WlOutput>,
    pub logical_width: f32,
    pub logical_height: f32,
    /// The window-frame logical size (surface minus the overflow rim) as of
    /// the last configure/desired-size — what the surface is re-derived from
    /// when [`Application::overflow_margin`] changes at runtime.
    pub frame_logical: (f32, f32),
    /// The overflow margin the current surface was actually sized with. Input
    /// translation and the dl-text overlay offsets use THIS, never a live
    /// `overflow_margin()` read — the app may have changed its answer since.
    pub applied_margin: f32,
    /// True while the previous frame ran with a nonzero margin — lets the
    /// per-frame geometry publish reset state exactly once on deactivation.
    pub overflow_was_active: bool,
    /// The popover-union rect last sent via zcce set_popover_region, logical
    /// surface px; None once a clear has been sent (or never anything).
    pub sent_popover_region: Option<(i32, i32, i32, i32)>,
    /// The context menu's popup surface while one is open — see
    /// `backend::menu_popup`.
    pub menu_popup: Option<crate::backend::menu_popup::MenuPopup>,
    /// The popup's renderer, kept across opens and moved from one popup
    /// surface to the next: a renderer costs a device and every pipeline
    /// (tens of ms), a re-attach costs one swapchain.
    pub menu_renderer: Option<VkRenderer>,
    /// The popup renderer's copies of the bundled glyphs the menu draws,
    /// by the window renderer's id for the same glyph (see
    /// `crate::icon_source`). The popup keeps images of its own — it does
    /// not take the shared upload queue — so a glyph the menu paints by the
    /// window's id is uploaded again into the popup's table, once. Cleared
    /// with the renderer it names.
    pub menu_icon_ids: std::collections::HashMap<u32, Option<u32>>,
    /// The `wl_display` the renderers were made from, as an address.
    pub display_ptr: usize,

    pub exit: bool,
    pub redraw: bool,
    /// A frame took the app's damage (`Application::take_damage`) and was
    /// not presented: the next presented frame is a full one.
    pub damage_owed: bool,
    /// The last built frame, for the damage the runner derives
    /// (`backend::frame::derive_damage`).
    pub frame_record: crate::backend::frame::FrameRecord,
    pub frame_callback_pending: bool,
    /// When the pending frame callback was armed — the starvation fallback's
    /// clock (see the render gate in `run`).
    pub frame_callback_armed_at: Option<std::time::Instant>,
    /// A warm-down commit's frame callback is outstanding (see
    /// [`EngineState::keepalive_commit`]). Separate from
    /// `frame_callback_pending` on purpose: a genuine redraw never waits on it.
    pub keepalive_pending: bool,
    pub keepalive_armed_at: Option<std::time::Instant>,
    /// Consecutive renders skipped by the extent gate (pending swapchain size
    /// != the size the current logical size and scale call for). Normally 0 or
    /// 1; a persistent count means no frame is presenting and deserves a warn.
    pub extent_gate_skips: u32,
    pub first_configure_received: bool,
    /// The session's input state and routing: modifiers, the held key, the
    /// pointer's place and held buttons, the chords (see [`Driver`]).
    pub driver: Driver,
    pub sender: calloop::channel::Sender<A::Message>,
    pub current_cursor_icon: Option<CursorIcon>,
    pub qh: QueueHandle<EngineState<A>>,
    pub just_configured: bool,
    pub pointer_gestures: Option<ZwpPointerGesturesV1>,
    pub pinch_gesture: Option<ZwpPointerGesturePinchV1>,
    /// `text-input-v3`, when the compositor offers it: the input method's
    /// way in (see `backend::text_input`). The text input is the first
    /// keyboard seat's.
    pub text_input_manager: Option<ZwpTextInputManagerV3>,
    pub text_input: Option<ZwpTextInputV3>,
    pub text_input_state: crate::backend::text_input::TextInput,
    /// The cce window-management toplevel handle, held for the window's
    /// lifetime once [`Application::utility`] declared the mode.
    pub cce_toplevel: Option<crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1>,
    /// Latest unrendered grid_patch (serial, x, y, w, h, scale) — a newer
    /// event supersedes an unconsumed older one, per protocol.
    pub pending_grid_patch: Option<(u32, f64, f64, f64, f64, f64)>,
    /// Serial of the most recent pointer press, kept for
    /// [`Application::take_window_action`] move/resize grabs.
    pub last_press_serial: Option<u32>,
    /// The touchscreen, once the seat offers one; see `backend/touch.rs`.
    pub touch: Option<wl_touch::WlTouch>,
    pub touch_tracker: super::touch::TouchTracker,
    /// The followed finger's surface offset into window coordinates (the
    /// menu popup's, or none), fixed at its down.
    pub touch_offset: (f32, f32),
    /// Where a touch scroll is dispatched: the finger's down point.
    pub touch_scroll_at: Option<(f32, f32)>,
    /// This frame's display-list text, shaped and held here so the `TextSpan`s built
    /// in the render pass can borrow the buffers (Phase 6 —
    /// [`Application::display_list_text`]).
    pub(crate) dl_text_items: Vec<DlText>,

    /// Drag-and-drop destination state (see [`crate::backend::dnd`]). The
    /// manager is absent when the compositor exposes no wl_data_device_manager;
    /// every drop path then no-ops.
    pub data_device_manager: Option<smithay_client_toolkit::data_device_manager::DataDeviceManagerState>,
    pub data_devices: Vec<smithay_client_toolkit::data_device_manager::data_device::DataDevice>,
    /// Mime type accepted for the in-flight drag; `None` means the app wants
    /// nothing this offer carries, so the drop is declined.
    pub drag_mime: Option<String>,
    /// Surface-local logical position of the last drag enter/motion — the
    /// drop point handed to [`Application::handle_drop`].
    pub drag_pos: LogicalPosition,
    /// Reader threads post completed drops here; the main loop drains it.
    pub drop_tx: Option<calloop::channel::Sender<crate::backend::dnd::DroppedData>>,
    /// The offer being read right now, held so it can be finished only once
    /// the transfer is actually done (see `dnd::drop_performed`).
    pub pending_drop_offer:
        Option<smithay_client_toolkit::data_device_manager::data_offer::DragOffer>,
    /// The input region last sent to the compositor, so a per-frame
    /// [`Application::input_regions`] only costs protocol traffic on change.
    pub applied_input_regions: Option<Vec<(i32, i32, i32, i32)>>,
}

impl<A: Application> EngineState<A> {
    /// Build the window's renderer. A [`SurfaceLost`](crate::vk::SurfaceLost)
    /// means the connection under the surface is already dead; the session
    /// ends as a lost connection, which reconnects if the compositor is still
    /// there and exits if it is not.
    pub fn init_gpu(
        &mut self,
        conn: &Connection,
        width_logical: f32,
        height_logical: f32,
    ) -> Result<(), crate::vk::SurfaceLost> {
        let s = self.scale_factor as f32;
        let pw = (width_logical * s) as u32;
        let ph = (height_logical * s) as u32;

        let surface = self.surface.as_ref().expect("surface missing");

        let display_ptr = conn.backend().display_id().as_ptr() as *mut std::ffi::c_void;
        let surface_ptr = surface.id().as_ptr() as *mut std::ffi::c_void;
        self.display_ptr = display_ptr as usize;

        let load_system_fonts = self.inner.as_ref().is_some_and(|a| a.load_system_fonts());
        // Corner radius 0: runner apps tessellate their own rounded corners.
        let mut renderer = unsafe { VkRenderer::try_new(display_ptr, surface_ptr, pw, ph, 0.0) }?;
        if self.inner.as_ref().is_some_and(|a| a.grid()) {
            // Redrawn only per patch, and a patch is several screens of pixels.
            renderer.set_minimal_swapchain();
        }
        self.font_system = Some(if load_system_fonts {
            crate::create_font_system_with_system_fonts()
        } else {
            crate::create_font_system()
        });
        self.renderer = Some(renderer);
        self.logical_width = width_logical;
        self.logical_height = height_logical;
        Ok(())
    }

    /// Buffer scale and physical extent for a logical size under the current
    /// scale factor: rounded, then snapped up so the extent divides by the
    /// buffer scale (a wl_surface requirement). In forced-scale mode the
    /// surface stays at buffer_scale 1 (the compositor believes scale 1).
    ///
    /// This is the single source of the buffer-size formula: `resize` sizes
    /// the swapchain with it and `render` refuses to present any extent that
    /// disagrees with it — a mispaired buffer/scale commit is how the resume
    /// output bounce halved even-sized windows (buffer at the old scale's
    /// size, new scale latched; the compositor reads it as a self-resize).
    pub(crate) fn buffer_geometry(scale_factor: f64, w: f32, h: f32) -> (i32, u32, u32) {
        let s = if crate::scale::forced_scale().is_some() {
            1
        } else {
            (scale_factor.round() as i32).max(1)
        };
        let su = s as u32;
        let pw = ((w as f64 * scale_factor).round() as u32).max(1).div_ceil(su) * su;
        let ph = ((h as f64 * scale_factor).round() as u32).max(1).div_ceil(su) * su;
        (s, pw, ph)
    }

    pub fn resize(&mut self, w: f32, h: f32) {
        let (w, h) = self.inner.as_ref().unwrap().adjust_size(w, h);
        if w > 0.0 && h > 0.0 {
            self.logical_width = w;
            self.logical_height = h;
            let (_, pw, ph) = Self::buffer_geometry(self.scale_factor, w, h);
            if let Some(ref mut renderer) = self.renderer {
                renderer.resize(pw, ph);
            }
            let scale = self.scale_factor;
            self.inner.as_mut().unwrap().handle_resize(w, h, scale);
            self.publish_window_geometry();
        }
    }

    /// Overflow-margin mode ([`Application::overflow_margin`]): re-publish the
    /// window frame — the surface rect inset by the margin — as the xdg window
    /// geometry, and an input region of the frame PLUS any open popover rects
    /// (an overhanging menu's rows must stay clickable; empty rim still falls
    /// through). Applied on every resize and, while the rim is live, every
    /// loop (the popover rects animate). Margin back at 0 resets both — a
    /// no-op only for apps that never had a rim. (All double-buffered surface
    /// state, latched by the next commit.)
    fn publish_window_geometry(&mut self) {
        let m = self.applied_margin;
        let Some(ref window) = self.window else { return };
        if m <= 0.0 {
            if self.overflow_was_active {
                let gw = (self.logical_width as i32).max(1);
                let gh = (self.logical_height as i32).max(1);
                window.xdg_surface().set_window_geometry(0, 0, gw, gh);
                if let Some(ref surface) = self.surface {
                    surface.set_input_region(None);
                }
            }
            return;
        }
        // Right/bottom rim: the frame keeps the surface origin — no offset,
        // frame coords == surface coords.
        let gw = ((self.logical_width - m) as i32).max(1);
        let gh = ((self.logical_height - m) as i32).max(1);
        window.xdg_surface().set_window_geometry(0, 0, gw, gh);
        if let Some(ref surface) = self.surface {
            let compositor = self.compositor_state.wl_compositor();
            let wl_region = compositor.create_region(&self.qh, ());
            wl_region.add(0, 0, gw, gh);
            // Open popovers, clamped to the surface.
            if let Some(ctx) = self.inner.as_ref().unwrap().ui_context() {
                for (id, ptr) in ctx.tree.iter_registered() {
                    unsafe {
                        let Some(w) = ptr.as_ref() else { continue };
                        if !w.visible() {
                            continue;
                        }
                        let Some((px, py, pw, ph)) = w.popover_rect() else { continue };
                        let (dx, dy) = self.inner.as_ref().unwrap().popover_offset(id);
                        let (px, py) = (px + dx, py + dy);
                        let x0 = px.max(0.0) as i32;
                        let y0 = py.max(0.0) as i32;
                        let x1 = ((px + pw).min(self.logical_width)) as i32;
                        let y1 = ((py + ph).min(self.logical_height)) as i32;
                        if x1 > x0 && y1 > y0 {
                            wl_region.add(x0, y0, x1 - x0, y1 - y0);
                        }
                    }
                }
            }
            surface.set_input_region(Some(&wl_region));
            wl_region.destroy();
        }
    }
    
    /// Report the union of the open popover rects to the compositor
    /// (zcce set_popover_region, manager v7), so its window chrome — the
    /// overview resize ring — stays out from under an in-surface menu. Sent
    /// only on change, and a clear is sent when the last popover closes;
    /// rects are clamped to the surface in logical px, the coordinate space
    /// the protocol specifies. Popovers animate, so this runs every loop —
    /// the change gate is what keeps it quiet.
    fn send_popover_region(&mut self) {
        let Some(tl) = &self.cce_toplevel else { return };
        // Version gate on the MANAGER numbering the resource carries (the
        // toplevel inherits its bind version): 7 is where the request
        // appeared. An older compositor would kill the client on the
        // unknown opcode.
        if tl.version() < 7 {
            return;
        }
        let mut union: Option<(f32, f32, f32, f32)> = None;
        if let Some(ctx) = self.inner.as_ref().unwrap().ui_context() {
            for (id, ptr) in ctx.tree.iter_registered() {
                unsafe {
                    let Some(w) = ptr.as_ref() else { continue };
                    if !w.visible() {
                        continue;
                    }
                    let Some((px, py, pw, ph)) = w.popover_rect() else { continue };
                    let (dx, dy) = self.inner.as_ref().unwrap().popover_offset(id);
                    let (px, py) = (px + dx, py + dy);
                    let (x0, y0) = (px.max(0.0), py.max(0.0));
                    let x1 = (px + pw).min(self.logical_width);
                    let y1 = (py + ph).min(self.logical_height);
                    if x1 <= x0 || y1 <= y0 {
                        continue;
                    }
                    union = Some(match union {
                        None => (x0, y0, x1, y1),
                        Some((ux0, uy0, ux1, uy1)) => {
                            (ux0.min(x0), uy0.min(y0), ux1.max(x1), uy1.max(y1))
                        }
                    });
                }
            }
        }
        let next = union.map(|(x0, y0, x1, y1)| {
            (x0 as i32, y0 as i32, (x1 - x0).ceil() as i32, (y1 - y0).ceil() as i32)
        });
        if next == self.sent_popover_region {
            return;
        }
        match next {
            Some((x, y, w, h)) => tl.set_popover_region(x, y, w, h),
            None => tl.set_popover_region(0, 0, 0, 0),
        }
        self.sent_popover_region = next;
    }

    fn logical_size(&self) -> LogicalSize {
        LogicalSize::new(self.logical_width, self.logical_height)
    }

    /// The cursor for the pointer at (lx, ly) — see [`Driver::cursor_icon_at`].
    fn cursor_icon_at(&self, lx: f32, ly: f32) -> CursorIcon {
        self.driver.cursor_icon_at(self.inner.as_ref().unwrap(), lx, ly, self.logical_size())
    }
}

/// User data of a warm-down frame callback ([`EngineState::keepalive_commit`]),
/// which clears `keepalive_pending` rather than `frame_callback_pending`.
pub struct KeepAlive;

impl<A: Application> wayland_client::Dispatch<wl_callback::WlCallback, KeepAlive> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _data: &KeepAlive,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.keepalive_pending = false;
            if crate::vk::present_debug() {
                let waited = state.keepalive_armed_at.map(|a| a.elapsed().as_millis()).unwrap_or(0);
                eprintln!("[vk] t={} keepalive-done (waited {}ms)", debug_clock_ms(), waited);
            }
        }
    }
}

impl<A: Application> Drop for EngineState<A> {
    fn drop(&mut self) {
        // The popup's renderer lets go of its surface before the popup (and
        // its wl_surface) drops with the rest of the fields.
        self.close_menu_popup();
        self.menu_renderer = None;
        self.renderer = None;
    }
}
