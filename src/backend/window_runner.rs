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
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface, wl_registry, wl_region, wl_callback},
    Connection, QueueHandle, Proxy,
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
use crate::widget::{TextItem, MouseButton, ElementState, Key, NamedKey};
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

pub struct EngineState<A: Application> {
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
    /// The `wl_display` the renderers were made from, as an address.
    pub display_ptr: usize,

    pub exit: bool,
    pub redraw: bool,
    /// A frame took the app's damage (`Application::take_damage`) and was
    /// not presented: the next presented frame is a full one.
    pub damage_owed: bool,
    pub frame_callback_pending: bool,
    /// When the pending frame callback was armed — the starvation fallback's
    /// clock (see the render gate in `run`).
    pub frame_callback_armed_at: Option<std::time::Instant>,
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
    /// The cce window-management toplevel handle, held for the window's
    /// lifetime once [`Application::utility`] declared the mode.
    pub cce_toplevel: Option<crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1>,
    /// Latest unrendered grid_patch (serial, x, y, w, h, scale) — a newer
    /// event supersedes an unconsumed older one, per protocol.
    pub pending_grid_patch: Option<(u32, f64, f64, f64, f64, f64)>,
    /// Serial of the most recent pointer press, kept for
    /// [`Application::take_window_action`] move/resize grabs.
    pub last_press_serial: Option<u32>,
    /// This frame's display-list text, shaped and held here so the `TextSpan`s built
    /// in the render pass can borrow the buffers (Phase 6 —
    /// [`Application::display_list_text`]).
    pub dl_text_items: Vec<TextItem>,

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

        let load_system_fonts = self.inner.as_ref().map_or(false, |a| a.load_system_fonts());
        // Corner radius 0: runner apps tessellate their own rounded corners.
        let renderer = unsafe { VkRenderer::try_new(display_ptr, surface_ptr, pw, ph, 0.0) }?;
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

    pub fn render(&mut self) {
        // Grid patch: resize to the patch's buffer size, tell the app what
        // world region this frame covers, and ack — the commit this render
        // produces is the one the compositor latches at the new anchor.
        if let Some((serial, px, py, pw, ph, pscale)) = self.pending_grid_patch.take() {
            self.resize((pw * pscale) as f32, (ph * pscale) as f32);
            self.inner.as_mut().unwrap().grid_patch(px, py, pw, ph, pscale);
            if let Some(tl) = &self.cce_toplevel {
                tl.ack_grid_patch(serial);
            }
        }
        let logical_w = self.logical_width;
        let logical_h = self.logical_height;
        let scale_factor = self.scale_factor;

        if let Some(ref surface) = self.surface {
            if let Some(regions) = self.inner.as_ref().unwrap().input_regions() {
                // Only re-send when it actually changes. This runs per frame,
                // and a client whose region tracks its content (the desktop
                // grid's items follow every pan) would otherwise create and
                // destroy a wl_region on every frame of a camera flight.
                if self.applied_input_regions.as_deref() != Some(regions.as_slice()) {
                    let compositor = self.compositor_state.wl_compositor();
                    let wl_region = compositor.create_region(&self.qh, ());
                    for &(rx, ry, rw, rh) in &regions {
                        wl_region.add(rx, ry, rw, rh);
                    }
                    surface.set_input_region(Some(&wl_region));
                    wl_region.destroy();
                    self.applied_input_regions = Some(regions);
                }
            }
        }
        
        // The frame itself, built with no window system in it (`backend::frame`).
        let frame = build_frame(
            self.inner.as_mut().unwrap(),
            self.font_system.as_mut().unwrap(),
            LogicalSize::new(logical_w, logical_h),
            scale_factor,
            &mut self.damage_owed,
            &mut self.dl_text_items,
        );

        // Upload the frame's glyphs. An app without display-list text owns
        // the renderer's text state itself (it stages via stage_renderer
        // below); don't wipe it here. The renderer owns swapchain
        // rebuild/recovery.
        let renderer = self.renderer.as_mut().unwrap();
        if frame.dl_text {
            let spans = frame.text_spans(&self.dl_text_items);
            renderer.prepare_text(self.font_system.as_mut().unwrap(), &mut self.swash_cache, &spans);
        }

        // Commit the buffer scale together with a buffer it is legal for: the
        // present inside draw_frame_2d is the only commit on this surface, so
        // sending the request here orders it right before a matching-size
        // attach+commit.
        //
        // Present only the EXACT extent the current logical size and scale
        // call for. Divisibility is not enough: mid scale-transition (the
        // resume output bounce) the pending extent can belong to the other
        // scale, and an even-sized old-scale buffer divides cleanly by the
        // new scale — the commit is protocol-legal, so the compositor reads
        // it as a self-resize to half/double and reconfigures the window to
        // match (how the color editor came back from suspend at exactly half
        // size with the divisibility guard green). Odd sizes at least die
        // loudly (invalid_size). On mismatch, re-request the right extent
        // and skip — before the frame-callback request below, so the loop
        // isn't left waiting on a callback no commit will ever latch.
        if let Some(ref surface) = self.surface {
            let (s, epw, eph) =
                Self::buffer_geometry(self.scale_factor, self.logical_width, self.logical_height);
            let e = renderer.pending_extent();
            if e.width != epw || e.height != eph {
                renderer.resize(epw, eph);
                self.extent_gate_skips += 1;
                // ~5s of continuous skipping at the 16ms loop cadence: nothing
                // is presenting and nothing else will say so — this is the
                // only witness to a wedged pending extent.
                if self.extent_gate_skips % 300 == 0 {
                    log::warn!(
                        "[window_runner] extent gate: pending {}x{} != expected {}x{} for {} consecutive renders; no frame is presenting",
                        e.width, e.height, epw, eph, self.extent_gate_skips,
                    );
                }
                self.redraw = true;
                return;
            }
            self.extent_gate_skips = 0;
            if s != self.committed_buffer_scale {
                surface.set_buffer_scale(s);
                self.committed_buffer_scale = s;
            }
        }

        if let Some(ref surface) = self.surface {
            let _callback = surface.frame(&self.qh, ());
            self.frame_callback_pending = true;
            self.frame_callback_armed_at = Some(std::time::Instant::now());
            if std::env::var("CCE_PRESENT_DEBUG").is_ok() {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 100000;
                eprintln!("[vk] t={} armed frame callback", t);
            }
        }

        // Direct renderer staging (3D scenes, RT panes, app-shaped text).
        if self.inner.as_mut().unwrap().stage_renderer(
            renderer,
            LogicalSize::new(logical_w, logical_h),
            scale_factor,
        ) {
            self.redraw = true;
        }

        if !renderer.draw_frame_2d(frame.frame2d()) {
            // No present happened (swapchain out-of-date, or the created
            // swapchain didn't match the requested extent). The frame
            // callback requested above will never latch without a commit —
            // clear it or the demand-driven loop stalls waiting forever.
            self.frame_callback_pending = false;
            self.redraw = true;
        } else {
            self.damage_owed = false;
        }
    }
}

impl<A: Application> Shell for EngineState<A> {
    type App = A;

    /// The driver and the app's turn, borrowed apart: every input dispatch
    /// is `let (driver, t) = self.turn(); driver.<event>(t, ..)`.
    fn turn(&mut self) -> (&mut Driver, Turn<'_, A>) {
        (
            &mut self.driver,
            Turn { app: self.inner.as_mut().unwrap(), redraw: &mut self.redraw, exit: &mut self.exit },
        )
    }

    fn app(&self) -> &A {
        self.inner.as_ref().unwrap()
    }

    fn redraw(&mut self) -> &mut bool {
        &mut self.redraw
    }

    fn exit_requested(&self) -> bool {
        self.exit
    }

    fn take_just_configured(&mut self) -> bool {
        std::mem::replace(&mut self.just_configured, false)
    }

    fn request_size(&mut self, w: u32, h: u32) {
        // desired_size is a window-frame size; the surface adds the
        // right/bottom overflow rim (0 for margin-less apps).
        let m = self.inner.as_ref().unwrap().overflow_margin() as f32;
        let (sw, sh) = (w as f32 + m, h as f32 + m);
        if (self.logical_width - sw).abs() > 0.001 || (self.logical_height - sh).abs() > 0.001 {
            self.frame_logical = (w as f32, h as f32);
            self.applied_margin = m;
            self.resize(sw, sh);
            self.redraw = true;
        }
    }

    fn sync(&mut self) {
        // Overflow-margin drift (configure-sized apps): the rim can change at
        // runtime — a popover overhanging the window frame — so re-derive the
        // surface from the stored frame whenever the app's answer moves. While
        // the rim is live, re-publish geometry every loop: the input region
        // tracks the animating popover rects.
        let m_now = self.inner.as_ref().unwrap().overflow_margin() as f32;
        if (m_now - self.applied_margin).abs() > 0.001 && self.frame_logical.0 > 0.0 {
            self.applied_margin = m_now;
            let (fw, fh) = self.frame_logical;
            self.resize(fw + m_now, fh + m_now);
            self.redraw = true;
        }
        if self.applied_margin > 0.0 || self.overflow_was_active {
            self.publish_window_geometry();
            self.overflow_was_active = self.applied_margin > 0.0;
        }
        self.send_popover_region();
        self.sync_menu_popup();
    }

    fn set_title(&mut self, title: &str) {
        if let Some(ref window) = self.window {
            window.set_title(title);
            window.commit();
        }
    }

    fn frame_pending(&mut self) -> bool {
        // Frame-callback starvation fallback: the compositor only sends
        // frame-done for surfaces it actually renders, so a callback armed
        // while the window sat off-viewport (or the scene went static) may
        // never fire — and the vsync gate then freezes the app forever
        // with a perfectly live event loop (input processes, state changes,
        // nothing repaints). If a redraw has been waiting on a callback well
        // past any real vsync interval, stop waiting and draw.
        //
        // Gated on the renderer's present mode: forcing a present past a
        // dead callback is only safe under MAILBOX (the present replaces the
        // queued buffer). Under FIFO the driver's throttle waits on the
        // previous present's frame event, so the forced present itself
        // blocks forever inside the driver — the exact freeze this fallback
        // exists to prevent. There the gate stays closed: pixels may stale
        // until the next frame-done/configure, but the loop stays alive.
        if self.redraw
            && self.frame_callback_pending
            && self.renderer.as_ref().is_some_and(|r| r.forced_present_safe())
            && self.frame_callback_armed_at.is_none_or(|t| t.elapsed().as_millis() > 250)
        {
            self.frame_callback_pending = false;
            if std::env::var("CCE_PRESENT_DEBUG").is_ok() {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 100000;
                eprintln!("[vk] t={} starvation fallback fired (callback never came)", t);
            }
        }
        self.frame_callback_pending
    }

    fn configured(&self) -> bool {
        self.first_configure_received
    }

    fn present(&mut self, fresh: bool) {
        if !fresh {
            // A warm-down re-render: the window alone.
            self.render();
            return;
        }
        // A menu handed over from the window commits first, so
        // there is no moment with neither (`take_menu_popup_lead`).
        let lead = self.take_menu_popup_lead();
        if lead {
            self.render_menu_popup();
        }
        self.render();
        if !lead {
            self.render_menu_popup();
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

impl<A: Application> CompositorHandler for EngineState<A> {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        scale_factor: i32,
    ) {
        // Don't send set_buffer_scale here: an in-flight present can commit an
        // old-scale-sized buffer right after it, which is a fatal invalid_size
        // protocol error (seen on resume, when outputs bounce 2→1→2). The scale
        // request is sent in `render`, paired with a matching-size present.
        if crate::scale::forced_scale().is_some() {
            // Forced mode: the compositor's opinion (scale 1 under cage) must
            // not clobber the override.
            return;
        }
        if self.inner.as_ref().map_or(false, |a| a.grid()) {
            // Grid surfaces stay at scale 1 — patch.scale is the sole
            // resolution authority (see the pin at surface creation).
            return;
        }
        // Resume bounce: when the surface sits on no LIVE output (the DRM
        // connector was destroyed and not yet re-created), the reported
        // factor is SCTK's no-outputs fallback, not information — hold the
        // last real scale. When the reborn output arrives, surface enter
        // recomputes and this handler runs again with a live output backing
        // it. Liveness matters (not just enter/leave counting): the leave
        // for a destroyed output may never be delivered.
        let on_live_output = self
            .entered_outputs
            .iter()
            .any(|o| self.output_state.info(o).is_some());
        if !on_live_output && (scale_factor as f64) < self.scale_factor {
            return;
        }
        self.scale_factor = scale_factor as f64;
        self.resize(self.logical_width, self.logical_height);
        self.redraw = true;
    }
    
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {}
    
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {}
    
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        if !self.entered_outputs.contains(output) {
            self.entered_outputs.push(output.clone());
        }
        // Dead entries (destroyed outputs never send leave) are harmless —
        // the liveness check in scale_factor_changed skips them — but drop
        // them here so the list doesn't grow across suspend cycles.
        self.entered_outputs
            .retain(|o| self.output_state.info(o).is_some());
        self.redraw = true;
    }

    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        output: &wl_output::WlOutput,
    ) {
        self.entered_outputs.retain(|o| o != output);
    }
}

impl<A: Application> OutputHandler for EngineState<A> {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    
    fn new_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
        crate::units::set_metric(crate::wayland::detect_metric(&self.output_state, scale));
    }
    fn update_output(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {
        let scale = crate::wayland::detect_scale_factor(&self.output_state);
        crate::scale::set_scale_factor(scale as f32);
        crate::units::set_metric(crate::wayland::detect_metric(&self.output_state, scale));
    }
    fn output_destroyed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _output: wl_output::WlOutput) {}
}

impl<A: Application> ShmHandler for EngineState<A> {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm_state
    }
}

impl<A: Application> ProvidesRegistryState for EngineState<A> {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    
    fn runtime_add_global(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
        _version: u32,
    ) {}
    
    fn runtime_remove_global(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _name: u32,
        _interface: &str,
    ) {}
}

impl<A: Application> WindowHandler for EngineState<A> {
    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _window: &XdgWindow,
        configure: WindowConfigure,
        _serial: u32,
    ) {
        let is_fs = configure.is_fullscreen();
        let is_max = configure.is_maximized();
        crate::scale::set_fullscreen(is_fs);
        crate::scale::set_maximized(is_max);

        let (w, h) = configure.new_size;
        // Configure sizes are window-geometry sizes; with an overflow margin
        // the surface is a rim larger on the right and bottom.
        let m = self.inner.as_ref().unwrap().overflow_margin() as f32;
        if let (Some(w), Some(h)) = (w, h) {
            let width = w.get();
            let height = h.get();
            // Forced mode: the compositor's logical size is really physical
            // pixels (scale-1 output); divide to get the app's logical space.
            let f = crate::scale::forced_scale().unwrap_or(1.0);
            self.frame_logical = (width as f32 / f, height as f32 / f);
            self.applied_margin = m;
            self.resize(width as f32 / f + m, height as f32 / f + m);
        } else if self.inner.as_ref().unwrap().grid() && self.logical_width > 1.0 {
            // A grid app's size belongs to its PATCHES: the compositor's
            // "you choose" 0x0 must not bounce the surface back to the
            // settings size — that thrash recreated multi-hundred-MB
            // swapchains per bounce (6.3G peak in 10s). Keep the current
            // size; the next grid_patch is the only resizer.
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            self.frame_logical = (settings.width as f32, settings.height as f32);
            self.applied_margin = m;
            self.resize(settings.width as f32 + m, settings.height as f32 + m);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
        self.just_configured = true;
    }

    fn request_close(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _window: &XdgWindow) {
        self.exit = true;
    }
}

impl<A: Application> LayerShellHandler for EngineState<A> {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // new_size is in logical pixels; 0 means "client decides", so fall back
        // to the app's requested size (mirrors the xdg WindowHandler above).
        let (w, h) = configure.new_size;
        if w > 0 && h > 0 {
            self.resize(w as f32, h as f32);
        } else {
            let settings = self.inner.as_ref().unwrap().settings();
            self.resize(settings.width as f32, settings.height as f32);
        }
        self.redraw = true;
        self.frame_callback_pending = false;
        self.first_configure_received = true;
        self.just_configured = true;
    }
}

impl<A: Application> SeatHandler for EngineState<A> {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    
    fn new_seat(&mut self, _conn: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.ensure_data_device(qh, &seat);
        self.seats.push(seat);
    }
    
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        // Every seat arrives here, unlike `new_seat` — SCTK binds the seats
        // that already exist at startup without announcing them, so a device
        // created only there is never created at all on a normal launch.
        self.ensure_data_device(qh, &seat);
        if capability == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor_state.create_surface::<Self>(qh);
            let themed_pointer = self.seat_state.get_pointer_with_theme(
                qh,
                &seat,
                self.shm_state.wl_shm(),
                surface,
                ThemeSpec::System,
            ).unwrap();
            if let Some(ref pg) = self.pointer_gestures {
                self.pinch_gesture = Some(pg.get_pinch_gesture(themed_pointer.pointer(), qh, ()));
            }
            self.pointer = Some(themed_pointer);
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let keyboard = self.seat_state.get_keyboard(qh, &seat, None).unwrap();
            self.keyboard = Some(keyboard);
        }
    }
    
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer {
            self.pinch_gesture = None;
            self.pointer = None;
        }
        if capability == Capability::Keyboard {
            self.keyboard = None;
        }
    }
    
    fn remove_seat(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, seat: wl_seat::WlSeat) {
        self.seats.retain(|s| s != &seat);
    }
}

impl<A: Application> PointerHandler for EngineState<A> {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[smithay_client_toolkit::seat::pointer::PointerEvent],
    ) {
        use smithay_client_toolkit::seat::pointer::PointerEventKind;
        let mut scroll = ScrollFrame::default();
        let mut has_scroll = false;
        let (mut last_lx, mut last_ly) = (0.0f32, 0.0f32);

        // Forced mode: pointer positions arrive in the compositor's scale-1
        // logical space (= physical); divide into the app's logical space.
        let forced = crate::scale::forced_scale().unwrap_or(1.0);
        for event in events {
            let (x, y) = event.position;
            // Overflow-margin mode needs no translation: the rim is
            // right/bottom-only, so frame coords == surface coords.
            let lx = x as f32 / forced;
            let ly = y as f32 / forced;
            // An event on the context menu's popup surface is the app's too,
            // at the popup's offset from the window: menu dispatch works in
            // window coordinates, which now reach outside the window.
            let popup_offset = self.menu_popup_offset(&event.surface);
            let on_popup = popup_offset.is_some();
            let (lx, ly) = match popup_offset {
                Some((ox, oy)) => (lx + ox, ly + oy),
                None => (lx, ly),
            };
            let pos = LogicalPosition::new(lx, ly);

            self.driver.cursor_pos = (lx, ly);
            match &event.kind {
                PointerEventKind::Enter { .. } => {
                    let (driver, t) = self.turn();
                    driver.pointer_enter(t, pos);

                    let cursor_icon = self.cursor_icon_at(lx, ly);
                    self.current_cursor_icon = Some(cursor_icon);
                    if let Some(ref themed_pointer) = self.pointer {
                        let _ = themed_pointer.set_cursor(_conn, cursor_icon);
                    }
                }
                PointerEventKind::Leave { .. } => {
                    self.current_cursor_icon = None;
                    let (driver, t) = self.turn();
                    driver.pointer_leave(t);
                }
                PointerEventKind::Motion { .. } => {
                    let (driver, t) = self.turn();
                    driver.pointer_motion(t, pos);

                    let cursor_icon = self.cursor_icon_at(lx, ly);
                    if self.current_cursor_icon != Some(cursor_icon) {
                        self.current_cursor_icon = Some(cursor_icon);
                        if let Some(ref themed_pointer) = self.pointer {
                            let _ = themed_pointer.set_cursor(_conn, cursor_icon);
                        }
                    }
                }
                PointerEventKind::Press { button, serial, .. } => {
                    let Some(btn) = evdev_button(*button) else { continue };
                    self.last_press_serial = Some(*serial);
                    let seat = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                    let site = PressSite {
                        size: self.logical_size(),
                        on_popup,
                        can_grab: self.window.is_some() && seat.is_some(),
                    };
                    let (driver, t) = self.turn();
                    let press = driver.pointer_press(t, btn, pos, site);
                    if let (Some(window), Some(seat)) = (&self.window, &seat) {
                        match press {
                            Press::Dispatched => {}
                            Press::Resize(edge) => window.resize(seat, *serial, xdg_resize_edge(edge)),
                            Press::Move => window.move_(seat, *serial),
                        }
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    let Some(btn) = evdev_button(*button) else { continue };
                    let (driver, t) = self.turn();
                    driver.pointer_release(t, btn, pos);
                }
                PointerEventKind::Axis { horizontal, vertical, source, .. } => {
                    scroll.h += horizontal.absolute;
                    scroll.v += vertical.absolute;
                    scroll.discrete_h += horizontal.discrete;
                    scroll.discrete_v += vertical.discrete;
                    // The source and the finger-lift stop ride in the same
                    // frame as the deltas (or alone, for the lift): they
                    // decide the smooth-scroll phase.
                    if let Some(source) = source {
                        scroll.source = Some(scroll_source(*source));
                    }
                    scroll.stop |= horizontal.stop || vertical.stop;
                    last_lx = lx;
                    last_ly = ly;
                    has_scroll = true;
                }
            }
        }

        if has_scroll {
            let (driver, t) = self.turn();
            driver.scroll(t, scroll, LogicalPosition::new(last_lx, last_ly));
        }

        // App-driven window move/resize (non-standard CSD; see WindowAction):
        // executed with the serial of the most recent pointer press.
        if let Some(action) = self.inner.as_mut().unwrap().take_window_action() {
            if let (Some(ref window), Some(serial)) = (&self.window, self.last_press_serial) {
                let seat_owned = self.seats.first().cloned().or_else(|| self.seat_state.seats().next());
                if let Some(ref seat) = seat_owned {
                    match action {
                        WindowAction::Move => window.move_(seat, serial),
                        WindowAction::Resize(edge) => window.resize(seat, serial, edge),
                    }
                }
            }
        }
    }
}

/// An evdev button code as one of cce-ui's buttons; the rest are not routed.
fn evdev_button(code: u32) -> Option<MouseButton> {
    match code {
        272 => Some(MouseButton::Left),
        273 => Some(MouseButton::Right),
        274 => Some(MouseButton::Middle),
        _ => None,
    }
}

fn xdg_resize_edge(edge: ResizeEdge) -> xdg_toplevel::ResizeEdge {
    match edge {
        ResizeEdge::Top => xdg_toplevel::ResizeEdge::Top,
        ResizeEdge::Bottom => xdg_toplevel::ResizeEdge::Bottom,
        ResizeEdge::Left => xdg_toplevel::ResizeEdge::Left,
        ResizeEdge::Right => xdg_toplevel::ResizeEdge::Right,
        ResizeEdge::TopLeft => xdg_toplevel::ResizeEdge::TopLeft,
        ResizeEdge::TopRight => xdg_toplevel::ResizeEdge::TopRight,
        ResizeEdge::BottomLeft => xdg_toplevel::ResizeEdge::BottomLeft,
        ResizeEdge::BottomRight => xdg_toplevel::ResizeEdge::BottomRight,
    }
}

/// A `wl_pointer` axis source as the driver's. Anything newer than the four
/// known sources scrolls as a wheel, as it did when the runner matched on
/// the protocol enum itself.
fn scroll_source(source: wl_pointer::AxisSource) -> ScrollSource {
    match source {
        wl_pointer::AxisSource::Finger => ScrollSource::Finger,
        wl_pointer::AxisSource::Continuous => ScrollSource::Continuous,
        wl_pointer::AxisSource::WheelTilt => ScrollSource::WheelTilt,
        _ => ScrollSource::Wheel,
    }
}

impl<A: Application> KeyboardHandler for EngineState<A> {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
        _raw_modifiers: &[u32],
        _keysyms: &[xkeysym::Keysym],
    ) {
        let (driver, t) = self.turn();
        driver.keyboard_focus(t, true);
    }

    fn leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _surface: &wl_surface::WlSurface,
        _serial: u32,
    ) {
        let (driver, t) = self.turn();
        driver.keyboard_focus(t, false);
    }

    fn press_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, ElementState::Pressed);
    }

    fn release_key(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        event: smithay_client_toolkit::seat::keyboard::KeyEvent,
    ) {
        self.handle_key(event, ElementState::Released);
    }

    fn update_modifiers(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        _serial: u32,
        modifiers: smithay_client_toolkit::seat::keyboard::Modifiers,
        _layout: u32,
    ) {
        let mods = Modifiers {
            ctrl: modifiers.ctrl,
            shift: modifiers.shift,
            alt: modifiers.alt,
            logo: modifiers.logo,
        };
        self.driver.set_modifiers(self.inner.as_mut().unwrap(), mods);
    }

    fn update_repeat_info(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _keyboard: &wl_keyboard::WlKeyboard,
        info: smithay_client_toolkit::seat::keyboard::RepeatInfo,
    ) {
        match info {
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Repeat { rate, delay } => {
                // Store/expose delay/rate if required by the application
                let _ = (rate, delay);
            }
            smithay_client_toolkit::seat::keyboard::RepeatInfo::Disable => {}
        }
    }
}

impl<A: Application> EngineState<A> {
    fn handle_key(&mut self, event: smithay_client_toolkit::seat::keyboard::KeyEvent, state: ElementState) {
        let Some(logical_key) = xkb_logical_key(&event, self.driver.mods.ctrl) else { return };
        let (driver, t) = self.turn();
        driver.key(t, logical_key, event.utf8, state);
    }
}

/// An xkb key event as one of cce-ui's keys, or `None` for a key with no
/// meaning to it (no name here and no text).
fn xkb_logical_key(event: &smithay_client_toolkit::seat::keyboard::KeyEvent, ctrl: bool) -> Option<Key> {
    Some(match event.keysym {
        xkeysym::Keysym::Escape => Key::Named(NamedKey::Escape),
        xkeysym::Keysym::Return => Key::Named(NamedKey::Enter),
        xkeysym::Keysym::BackSpace => Key::Named(NamedKey::Backspace),
        xkeysym::Keysym::Down => Key::Named(NamedKey::ArrowDown),
        xkeysym::Keysym::Up => Key::Named(NamedKey::ArrowUp),
        xkeysym::Keysym::Left => Key::Named(NamedKey::ArrowLeft),
        xkeysym::Keysym::Right => Key::Named(NamedKey::ArrowRight),
        // xkb reports Shift+Tab as ISO_Left_Tab; apps see plain Tab plus
        // the shift modifier, matching winit.
        xkeysym::Keysym::Tab | xkeysym::Keysym::ISO_Left_Tab => Key::Named(NamedKey::Tab),
        xkeysym::Keysym::Delete => Key::Named(NamedKey::Delete),
        xkeysym::Keysym::space => Key::Named(NamedKey::Space),
        xkeysym::Keysym::Page_Up => Key::Named(NamedKey::PageUp),
        xkeysym::Keysym::Page_Down => Key::Named(NamedKey::PageDown),
        xkeysym::Keysym::Home => Key::Named(NamedKey::Home),
        xkeysym::Keysym::End => Key::Named(NamedKey::End),
        xkeysym::Keysym::Super_L | xkeysym::Keysym::Super_R => Key::Named(NamedKey::Super),
        xkeysym::Keysym::Alt_L | xkeysym::Keysym::Alt_R => Key::Named(NamedKey::Alt),
        xkeysym::Keysym::Control_L | xkeysym::Keysym::Control_R => Key::Named(NamedKey::Control),
        xkeysym::Keysym::Shift_L | xkeysym::Keysym::Shift_R => Key::Named(NamedKey::Shift),
        xkeysym::Keysym::F1 => Key::Named(NamedKey::F1),
        xkeysym::Keysym::F2 => Key::Named(NamedKey::F2),
        xkeysym::Keysym::F3 => Key::Named(NamedKey::F3),
        xkeysym::Keysym::F4 => Key::Named(NamedKey::F4),
        xkeysym::Keysym::F5 => Key::Named(NamedKey::F5),
        xkeysym::Keysym::F6 => Key::Named(NamedKey::F6),
        xkeysym::Keysym::F7 => Key::Named(NamedKey::F7),
        xkeysym::Keysym::F8 => Key::Named(NamedKey::F8),
        xkeysym::Keysym::F9 => Key::Named(NamedKey::F9),
        xkeysym::Keysym::F10 => Key::Named(NamedKey::F10),
        xkeysym::Keysym::F11 => Key::Named(NamedKey::F11),
        xkeysym::Keysym::F12 => Key::Named(NamedKey::F12),
        _ => {
            // With Ctrl held, xkb's utf8 goes through the legacy control-character
            // transformation (ctrl+j = "\n", ctrl+a = 0x01, ...); the keysym is
            // untransformed, so prefer it there or ctrl+<letter> shortcuts can
            // never match their letter.
            if ctrl {
                if let Some(ch) = event.keysym.key_char() {
                    Key::Character(ch.to_string())
                } else if let Some(ref text) = event.utf8 {
                    Key::Character(text.clone())
                } else {
                    return None;
                }
            } else if let Some(ref text) = event.utf8 {
                Key::Character(text.clone())
            } else if let Some(ch) = event.keysym.key_char() {
                Key::Character(ch.to_string())
            } else {
                return None;
            }
        }
    })
}

impl<A: Application> wayland_client::Dispatch<wl_registry::WlRegistry, GlobalList, Self> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &wl_registry::WlRegistry,
        _event: wl_registry::Event,
        _data: &GlobalList,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::zcce_inspector_v1::ZcceInspectorV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::zcce_inspector_v1::ZcceInspectorV1,
        _event: crate::protocol::zcce_inspector_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1,
        _event: crate::protocol::cce_window_management_v1::zcce_window_manager_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}

    wayland_client::event_created_child!(
        EngineState<A>,
        crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1,
        [
            6 => (crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1, ()),
            7 => (crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1, ()),
            8 => (crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1, ()),
        ]
    );
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_window_v1::ZcceWindowV1,
        _event: crate::protocol::cce_window_management_v1::zcce_window_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_output_v1::ZcceOutputV1,
        _event: crate::protocol::cce_window_management_v1::zcce_output_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_seat_v1::ZcceSeatV1,
        _event: crate::protocol::cce_window_management_v1::zcce_seat_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &crate::protocol::cce_window_management_v1::zcce_toplevel_v1::ZcceToplevelV1,
        event: crate::protocol::cce_window_management_v1::zcce_toplevel_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        use crate::protocol::cce_window_management_v1::zcce_toplevel_v1::Event;
        if let Event::GridPatch { serial, x, y, width, height, scale } = event {
            // A newer patch supersedes an unconsumed older one.
            state.pending_grid_patch = Some((serial, x, y, width, height, scale));
            state.redraw = true;
        }
    }
}

delegate_compositor!(@<A: Application> EngineState<A>);
delegate_xdg_shell!(@<A: Application> EngineState<A>);
delegate_xdg_window!(@<A: Application> EngineState<A>);
delegate_layer!(@<A: Application> EngineState<A>);
delegate_shm!(@<A: Application> EngineState<A>);
delegate_seat!(@<A: Application> EngineState<A>);
delegate_pointer!(@<A: Application> EngineState<A>);
delegate_keyboard!(@<A: Application> EngineState<A>);
delegate_registry!(@<A: Application> EngineState<A>);
delegate_output!(@<A: Application> EngineState<A>);

impl<A: Application> wayland_client::Dispatch<wl_region::WlRegion, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &wl_region::WlRegion,
        _event: wl_region::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<wl_callback::WlCallback, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &wl_callback::WlCallback,
        event: wl_callback::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            state.frame_callback_pending = false;
            if std::env::var("CCE_PRESENT_DEBUG").is_ok() {
                let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() % 100000;
                let waited = state.frame_callback_armed_at.map(|a| a.elapsed().as_millis()).unwrap_or(0);
                eprintln!("[vk] t={} frame-done (waited {}ms)", t, waited);
            }
        }
    }
}

impl<A: Application> wayland_client::Dispatch<ZwpPointerGesturesV1, ()> for EngineState<A> {
    fn event(
        _state: &mut Self,
        _proxy: &ZwpPointerGesturesV1,
        _event: zwp_pointer_gestures::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {}
}

impl<A: Application> wayland_client::Dispatch<ZwpPointerGesturePinchV1, ()> for EngineState<A> {
    fn event(
        state: &mut Self,
        _proxy: &ZwpPointerGesturePinchV1,
        event: zwp_pointer_gesture_pinch_v1::Event,
        _data: &(),
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
    ) {
        match event {
            zwp_pointer_gesture_pinch_v1::Event::Begin { .. } => state.driver.pinch_begin(),
            zwp_pointer_gesture_pinch_v1::Event::Update { scale, .. } => {
                let (driver, t) = state.turn();
                driver.pinch_update(t, scale as f32);
            }
            zwp_pointer_gesture_pinch_v1::Event::End { .. } => state.driver.pinch_end(),
            _ => {}
        }
    }
}

/// Why a session's event loop stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SessionEnd {
    /// The app asked to exit.
    AppExit,
    /// The compositor connection died while the compositor itself may well be
    /// alive — a broken transport. The `Application` is intact and can be
    /// re-attached to a fresh connection.
    ConnectionLost,
    /// Nothing answered at the display socket: the compositor this app
    /// belonged to is gone. A deliberate exit unlinks the socket and a crash
    /// leaves it refusing; either way there is no session left to rejoin.
    NoCompositor,
}

/// What [`run`] does once a session has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AfterSession {
    /// Leave the process-lifetime loop: run `on_exit` and quit.
    Exit,
    /// Sleep this long, then open a fresh session on the same `Application`.
    Reconnect(std::time::Duration),
    /// The compositor is gone and the app outlives it
    /// ([`Application::outlives_compositor`]): wait for a successor's socket,
    /// then open a fresh session on the same `Application`.
    AwaitCompositor,
}

/// The display socket this process connects to: `$WAYLAND_DISPLAY` (absolute,
/// or a name under `$XDG_RUNTIME_DIR`), `wayland-0` when unset — the lookup
/// `Connection::connect_to_env` makes.
fn wayland_socket_path() -> Option<std::path::PathBuf> {
    let name = std::env::var_os("WAYLAND_DISPLAY").unwrap_or_else(|| "wayland-0".into());
    let name = std::path::PathBuf::from(name);
    if name.is_absolute() {
        return Some(name);
    }
    Some(std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR")?).join(name))
}

/// Sleep until the display socket exists again — the successor compositor
/// has bound it. Polled at 250 ms: a quarter-second after the next login is
/// soon enough, and a daemon waiting through a logged-out hour costs four
/// `stat`s a second. A stale socket a crash left behind satisfies the poll
/// and fails the connect, which comes back here after the same pause.
fn await_compositor_socket() {
    loop {
        std::thread::sleep(std::time::Duration::from_millis(250));
        match wayland_socket_path() {
            Some(path) if path.exists() => return,
            Some(_) => {}
            // No runtime dir to look in: keep trying the connect itself.
            None => return,
        }
    }
}

/// How many consecutive failed reconnects before giving up. Reset once a
/// session has survived [`RECONNECT_RESET`], so a long-lived window that loses
/// its connection twice in a day still gets a full budget the second time.
const RECONNECT_ATTEMPTS: u32 = 8;
const RECONNECT_RESET: std::time::Duration = std::time::Duration::from_secs(10);

/// Decide whether a finished session is followed by another.
///
/// `lived` is how long the session that just ended lasted, `has_app` whether
/// an `Application` exists to carry over, and `attempt` the running count of
/// consecutive reconnects (reset here once a session outlives
/// [`RECONNECT_RESET`]).
///
/// Only a lost connection is retried, and only while the compositor is still
/// there to reconnect to. A reconnect is a repair of THIS session's transport
/// — the fd-exhaustion break `raise_fd_limit` documents — not a way to outlive
/// the compositor. When the connect itself fails the compositor has exited,
/// and it has already saved this window for restore: the next compositor
/// respawns the app from `state.json` on its own. A client that kept
/// retrying instead (the backoff below spans ~25s) reattached to that
/// successor beside the respawned copy, and every restore after a forced
/// exit or a crash came up with two of each cce-ui window. So the process
/// exits, as a Wayland client whose display went away always has.
///
/// Unless the app OUTLIVES the compositor (`outlives`,
/// [`Application::outlives_compositor`]) — a daemon the compositor does not
/// restore. Then there is no copy to collide with and every reason to stay:
/// it waits for the successor and rejoins it.
fn after_session(
    end: SessionEnd,
    has_app: bool,
    lived: std::time::Duration,
    attempt: &mut u32,
    outlives: bool,
) -> AfterSession {
    match end {
        SessionEnd::NoCompositor if has_app && outlives => {
            *attempt = 0;
            AfterSession::AwaitCompositor
        }
        SessionEnd::AppExit | SessionEnd::NoCompositor => AfterSession::Exit,
        SessionEnd::ConnectionLost => {
            // Nothing to preserve if we never got as far as building the
            // app — that is a failure to start, not a lost window.
            if !has_app {
                return AfterSession::Exit;
            }
            if lived > RECONNECT_RESET {
                *attempt = 0;
            }
            *attempt += 1;
            if *attempt > RECONNECT_ATTEMPTS {
                return AfterSession::Exit;
            }
            AfterSession::Reconnect(std::time::Duration::from_millis(
                100 * (1 << (*attempt).min(6)),
            ))
        }
    }
}

/// Raise this process's file-descriptor soft limit toward its hard limit.
///
/// A cce-ui client's fd usage is not bounded by anything the app controls.
/// Every dmabuf-feedback event the compositor sends carries a format-table
/// fd, and those arrive per surface whenever scanout candidacy changes —
/// entering the overview re-sends one for every window at once. Long-lived
/// windows sit at 700+ open fds in normal use, against a soft limit of 1024.
///
/// Crossing that limit does not fail politely. `recvmsg` drops the SCM_RIGHTS
/// payload when it cannot allocate descriptors, while still delivering the
/// message body — so libwayland hits a message whose fd never arrived,
/// reports "file descriptor expected", and the connection dies. That is
/// precisely the transport break [`run`] reconnects from below, at the cost
/// of a rebuilt window.
///
/// The compositor raises itself to 65536 for the same reason and then
/// deliberately restores the inherited limit for the programs it spawns
/// (cce-compositor `process.rs::cleanup_child`) — right for an arbitrary
/// child, far too low for a dmabuf-heavy Wayland client. So each client
/// raises its own, to the same ceiling.
fn raise_fd_limit() {
    unsafe {
        let mut lim: libc::rlimit = std::mem::zeroed();
        if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) != 0 {
            return;
        }
        let want = std::cmp::min(65536, lim.rlim_max);
        if lim.rlim_cur >= want {
            return;
        }
        let raised = libc::rlimit { rlim_cur: want, rlim_max: lim.rlim_max };
        if libc::setrlimit(libc::RLIMIT_NOFILE, &raised) == 0 {
            log::info!("[window_runner] fd limit raised {} -> {}", lim.rlim_cur, want);
        } else {
            log::warn!("[window_runner] could not raise fd limit from {}", lim.rlim_cur);
        }
    }
}

/// Run an [`Application`] to completion, surviving loss of the compositor
/// connection.
///
/// A Wayland connection cannot be repaired once its transport state breaks — a
/// single dropped file descriptor on a dmabuf-feedback event is enough, and
/// libwayland then fails every dispatch with `EINVAL`. Exiting the process on
/// that error (the old behavior) threw away everything the window held: a
/// terminal's shell and scrollback, an editor's unsaved buffer.
///
/// So a connection is one *session*. Objects that belong to the connection —
/// the Wayland globals, the surface, the swapchain, the renderer — are rebuilt
/// per session. The things that carry user state outlive it: the `Application`
/// itself, the calloop loop, and the message channel. Keeping the **same
/// channel** matters as much as keeping the app: worker threads hold clones of
/// its `Sender` (cce-terminal's pty reader is the canonical case), and a fresh
/// channel would orphan them into a live-but-deaf process.
///
/// What is repaired is the transport, never the compositor: a reconnect only
/// goes through while the compositor that owned the lost session is still
/// listening. If the connect itself fails the compositor has exited, and the
/// process exits with it — see [`after_session`] for why staying alive there
/// duplicated every window on the next session restore.
///
/// Caveat: GPU resources belong to the renderer, so a rebuild re-runs
/// [`Application::renderer_init`]. Images uploaded outside it (e.g. in
/// [`Application::new`]) are not replayed into the new renderer — upload from
/// `renderer_init` if they must survive a reconnect.
pub fn run<A: Application>() {
    raise_fd_limit();

    // Outlives every session: worker threads hold this Sender, and the app's
    // own event sources are registered on this loop once.
    let (sender, channel) = calloop::channel::channel::<A::Message>();
    // Drop payloads come back from the per-drop reader threads (see
    // `backend::dnd`); registered once, like the app channel, because the
    // loop outlives a reconnect while the EngineState does not.
    let (drop_tx, drop_rx) =
        calloop::channel::channel::<crate::backend::dnd::DroppedData>();
    let mut event_loop = match EventLoop::try_new() {
        Ok(l) => l,
        Err(e) => {
            log::error!("[window_runner] cannot create event loop: {e}");
            return;
        }
    };
    event_loop
        .handle()
        .insert_source(channel, |event, _metadata, app_state: &mut EngineState<A>| {
            if let calloop::channel::Event::Msg(msg) = event {
                let mut rebuild = false;
                app_state.inner.as_mut().unwrap().update(msg, &mut rebuild, &mut app_state.exit);
                if rebuild {
                    app_state.redraw = true;
                }
            }
        })
        .unwrap();
    event_loop
        .handle()
        .insert_source(drop_rx, |event, _metadata, app_state: &mut EngineState<A>| {
            if let calloop::channel::Event::Msg(drop) = event {
                // The transfer is complete, so the source can be released now
                // — doing it any earlier costs the payload.
                if let Some(offer) = app_state.pending_drop_offer.take() {
                    offer.finish();
                    offer.destroy();
                }
                let mut rebuild = false;
                if let Some(app) = app_state.inner.as_mut() {
                    app.handle_drop(&drop.mime, &drop.bytes, drop.pos, &mut rebuild);
                }
                if rebuild {
                    app_state.redraw = true;
                }
            }
        })
        .unwrap();

    let mut app: Option<A> = None;
    let mut sources_registered = false;
    let mut attempt: u32 = 0;

    loop {
        let started = std::time::Instant::now();
        let (returned_app, end) =
            run_session(&mut event_loop, sender.clone(), drop_tx.clone(), app.take(), !sources_registered);
        app = returned_app;
        sources_registered = true;

        let outlives = app.as_ref().is_some_and(|a| a.outlives_compositor());
        match after_session(end, app.is_some(), started.elapsed(), &mut attempt, outlives) {
            AfterSession::Exit => {
                match end {
                    SessionEnd::AppExit => {}
                    SessionEnd::NoCompositor if app.is_some() => log::warn!(
                        "[window_runner] compositor is gone; exiting (its successor restores the session itself)"
                    ),
                    SessionEnd::NoCompositor => {
                        log::error!("[window_runner] no compositor connection; giving up")
                    }
                    SessionEnd::ConnectionLost if app.is_some() => log::error!(
                        "[window_runner] connection lost; giving up after {} attempts",
                        attempt - 1
                    ),
                    SessionEnd::ConnectionLost => {
                        log::error!("[window_runner] no compositor connection; giving up")
                    }
                }
                break;
            }
            AfterSession::Reconnect(backoff) => {
                log::warn!(
                    "[window_runner] compositor connection lost; reconnecting in {backoff:?} (attempt {attempt})"
                );
                std::thread::sleep(backoff);
            }
            AfterSession::AwaitCompositor => {
                log::warn!("[window_runner] compositor is gone; waiting for the next one");
                await_compositor_socket();
                log::info!("[window_runner] a compositor is back; rejoining");
            }
        }
    }

    if let Some(mut app) = app {
        app.on_exit();
    }
}

/// One connection's lifetime: connect, build the surface and renderer, pump
/// events until the app exits or the connection dies. Returns the
/// `Application` so the caller can hand it to the next session.
fn run_session<'l, A: Application>(
    event_loop: &mut EventLoop<'l, EngineState<A>>,
    sender: calloop::channel::Sender<A::Message>,
    drop_tx: calloop::channel::Sender<crate::backend::dnd::DroppedData>,
    existing_app: Option<A>,
    register_app_sources: bool,
) -> (Option<A>, SessionEnd) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            log::error!("[window_runner] cannot connect to compositor: {e}");
            return (existing_app, SessionEnd::NoCompositor);
        }
    };
    let (globals, mut event_queue) = match registry_queue_init(&conn) {
        Ok(v) => v,
        Err(e) => {
            log::error!("[window_runner] registry init failed: {e}");
            return (existing_app, SessionEnd::ConnectionLost);
        }
    };
    let qh = event_queue.handle();

    let compositor_state = CompositorState::bind(&globals, &qh).unwrap();
    let xdg_shell_state = XdgShell::bind(&globals, &qh).unwrap();
    let layer_shell_state = LayerShell::bind(&globals, &qh).ok();
    let shm_state = Shm::bind(&globals, &qh).unwrap();
    let seat_state = SeatState::new(&globals, &qh);
    let output_state = OutputState::new(&globals, &qh);

    let pointer_gestures: Option<ZwpPointerGesturesV1> = globals.bind(&qh, 1..=3, ()).ok();

    let mut engine_state = EngineState {
        data_device_manager: DataDeviceManagerState::bind(&globals, &qh).ok(),
        data_devices: Vec::new(),
        drag_mime: None,
        drag_pos: LogicalPosition::new(0.0, 0.0),
        drop_tx: Some(drop_tx),
        pending_drop_offer: None,
        applied_input_regions: None,
        registry_state: RegistryState::new(&globals),
        compositor_state,
        xdg_shell_state,
        layer_shell_state,
        shm_state,
        seat_state,
        output_state,
        seats: Vec::new(),
        pointer: None,
        keyboard: None,
        window: None,
        layer_surface: None,
        surface: None,
        inner: None,
        renderer: None,
        font_system: None,
        swash_cache: cosmic_text::SwashCache::new(),
        scale_factor: 1.0,
        committed_buffer_scale: 1,
        entered_outputs: Vec::new(),
        logical_width: 0.0,
        logical_height: 0.0,
        frame_logical: (0.0, 0.0),
        applied_margin: 0.0,
        overflow_was_active: false,
        sent_popover_region: None,
        menu_popup: None,
        menu_renderer: None,
        display_ptr: 0,
        exit: false,
        redraw: false,
        damage_owed: true,
        frame_callback_pending: false,
        frame_callback_armed_at: None,
        extent_gate_skips: 0,
        first_configure_received: false,
        driver: Driver::new(),
        sender,
        current_cursor_icon: None,
        qh: qh.clone(),
        just_configured: false,
        pointer_gestures,
        pinch_gesture: None,
        cce_toplevel: None,
        pending_grid_patch: None,
        last_press_serial: None,
        dl_text_items: Vec::new(),
    };

    if let Err(e) = event_queue.roundtrip(&mut engine_state) {
        log::error!("[window_runner] initial roundtrip failed: {e}");
        return (existing_app, SessionEnd::ConnectionLost);
    }

    let scale = detect_scale_factor(&engine_state.output_state);
    engine_state.scale_factor = scale;
    crate::scale::set_scale_factor(scale as f32);
    crate::units::set_metric(crate::wayland::detect_metric(&engine_state.output_state, scale));

    // A reconnect re-attaches the SAME app: its state is the thing worth
    // saving, and `A::new` would both discard it and hand a fresh Sender to
    // worker threads that are still holding the original.
    let inner = match existing_app {
        Some(app) => app,
        None => A::new(&qh, engine_state.sender.clone()),
    };
    let settings = inner.settings();
    crate::scale::set_app_id(settings.app_id.clone());
    engine_state.logical_width = settings.width as f32;
    engine_state.logical_height = settings.height as f32;
    engine_state.inner = Some(inner);

    let surface = engine_state.compositor_state.create_surface(&qh);
    // A grid app's surface is pinned to scale 1: the patch's `scale` is
    // BUFFER px per virtual unit and already carries the output scale (the
    // patch manager folds it in), so adopting the output scale here would
    // square it — the client renders a doubled buffer and the compositor
    // downsamples it straight back into blur.
    if engine_state.inner.as_ref().unwrap().grid() {
        engine_state.scale_factor = 1.0;
    }
    // Forced-scale mode renders scaled-up into a buffer_scale-1 surface.
    let buffer_scale = if crate::scale::forced_scale().is_some()
        || engine_state.inner.as_ref().unwrap().grid()
    {
        1
    } else {
        scale as i32
    };
    surface.set_buffer_scale(buffer_scale);
    engine_state.committed_buffer_scale = buffer_scale;

    if settings.app_id.starts_with("cce-status") {
        let compositor = engine_state.compositor_state.wl_compositor();
        let region = compositor.create_region(&qh, ());
        region.add(0, 0, settings.width as i32, settings.height as i32);
        surface.set_input_region(Some(&region));
        region.destroy();
    }

    let layer_settings = engine_state.inner.as_ref().unwrap().layer();
    if let Some(ls) = layer_settings {
        let layer_shell = engine_state
            .layer_shell_state
            .as_ref()
            .expect("compositor does not support wlr-layer-shell");
        let layer_surface = layer_shell.create_layer_surface(
            &qh,
            surface.clone(),
            ls.layer,
            Some(ls.namespace.clone()),
            None,
        );
        layer_surface.set_anchor(ls.anchor);
        layer_surface.set_exclusive_zone(ls.exclusive_zone);
        layer_surface.set_keyboard_interactivity(ls.keyboard_interactivity);
        let (t, r, b, l) = ls.margin;
        layer_surface.set_margin(t, r, b, l);
        layer_surface.set_size(settings.width, settings.height);
        layer_surface.commit();
        engine_state.layer_surface = Some(layer_surface);
    } else {
        let window = engine_state.xdg_shell_state.create_window(surface.clone(), WindowDecorations::None, &qh);
        window.set_title(&settings.title);
        window.set_app_id(&settings.app_id);
        if settings.fullscreen {
            window.set_fullscreen(None);
        }
        if let Some((min_w, min_h)) = settings.min_size {
            window.set_min_size(Some((min_w, min_h)));
        }
        let wants_utility = engine_state.inner.as_ref().unwrap().utility();
        let wants_grid = engine_state.inner.as_ref().unwrap().grid();
        {
            // Bound for EVERY app now, not just utility/grid ones: the
            // toplevel also carries the popover-region hint (manager v7),
            // which any app with a dropdown wants. Role declarations go
            // BEFORE the initial commit so the mode is set by the time the
            // compositor maps the window. Version floors: set_utility
            // appeared at manager 5, the grid role at 6; the range tops at 7
            // so a newer compositor grants the hint and an older one simply
            // yields a lower-versioned toplevel — the hint send is gated on
            // version() >= 7 (send_popover_region), and on a pre-5
            // compositor the bind fails and the app runs plain.
            let version = if wants_grid { 6..=7 } else { 5..=7 };
            match globals.bind::<crate::protocol::cce_window_management_v1::zcce_window_manager_v1::ZcceWindowManagerV1, _, _>(&qh, version, ()) {
                Ok(cce_wm) => {
                    let toplevel = cce_wm.get_cce_toplevel(&surface, &qh, ());
                    if wants_utility {
                        toplevel.set_utility();
                    }
                    if wants_grid {
                        toplevel.set_grid();
                    }
                    engine_state.cce_toplevel = Some(toplevel);
                }
                Err(e) => {
                    log::warn!("[window_runner] cce window-management declaration unavailable: {e}");
                }
            }
        }
        window.commit();
        engine_state.window = Some(window);
    }
    engine_state.surface = Some(surface);

    // Overflow-margin mode: the surface (and so the GPU swapchain) is a rim
    // larger than the window frame on every side; geometry/input-region are
    // published per-resize.
    let rim = 2.0 * engine_state.inner.as_ref().unwrap().overflow_margin() as f32;
    if let Err(lost) = engine_state.init_gpu(&conn, settings.width as f32 + rim, settings.height as f32 + rim) {
        log::error!("[window_runner] cannot create the renderer, ending session: {lost}");
        return (engine_state.inner.take(), SessionEnd::ConnectionLost);
    }
    engine_state
        .inner
        .as_mut()
        .unwrap()
        .renderer_init(engine_state.renderer.as_mut().unwrap());

    let loop_handle = event_loop.handle();
    let wayland_token = match WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone())
    {
        Ok(token) => token,
        Err(e) => {
            log::error!("[window_runner] cannot register the wayland source: {e}");
            return (engine_state.inner.take(), SessionEnd::ConnectionLost);
        }
    };

    // The app's own sources live on the persistent loop, so they are registered
    // once for the process — re-registering per session would double-deliver
    // every event on them.
    if register_app_sources {
        engine_state.inner.as_mut().unwrap().register_sources(&loop_handle);
    }

    /// Same switch as the renderer's present tracer, resolved once — this sits
    /// in the per-iteration path, so a `std::env::var` call here would be I/O
    /// on the loop that is under measurement.
    fn loop_debug() -> bool {
        static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *FLAG.get_or_init(|| std::env::var_os("CCE_PRESENT_DEBUG").is_some())
    }

    /// Seconds after session start at which to inject a simulated connection
    /// loss, from `CCE_UI_FAULT_RECONNECT`. Resolved once: this is read from
    /// the per-iteration path.
    fn fault_reconnect_after() -> Option<std::time::Duration> {
        static AFTER: std::sync::OnceLock<Option<std::time::Duration>> =
            std::sync::OnceLock::new();
        *AFTER.get_or_init(|| {
            std::env::var("CCE_UI_FAULT_RECONNECT")
                .ok()
                .and_then(|v| v.parse::<f32>().ok())
                .map(std::time::Duration::from_secs_f32)
        })
    }

    let mut end = SessionEnd::AppExit;
    let session_start = std::time::Instant::now();
    // The loop's pacing — what a turn does and how long to sleep after it —
    // is the shared `Pacer`'s (backend::shell); this loop is the Wayland
    // side: dispatch, the connection's health, the close fade.
    let mut pacer = Pacer::new(settings.title.clone());
    let mut next_timeout = ACTIVE_DISPATCH;
    loop {
        // Frame callbacks arrive with a p50 of 0ms but a ~0.5s tail, while the
        // compositor's own trace shows it firing them within one or two vsyncs
        // of the arm. Tracing each iteration bisects that: if this loop keeps
        // turning at ~16ms all through a long wait, the event was not there to
        // read, and the delay is upstream rather than in dispatching it.
        let iter_start = if loop_debug() {
            Some(std::time::Instant::now())
        } else {
            None
        };
        if let Err(e) = event_loop.dispatch(next_timeout, &mut engine_state) {
            log::error!("[window_runner] event loop error, ending session: {e:?}");
            end = SessionEnd::ConnectionLost;
            break;
        }
        if let Some(start) = iter_start {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
                % 100000;
            eprintln!(
                "[vk] t={} loop dispatch={}us pending_cb={}",
                t,
                start.elapsed().as_micros(),
                engine_state.frame_callback_pending
            );
        }
        // A protocol error kills the connection permanently, but it surfaces
        // through queue flushes whose errors calloop's WaylandSource swallows
        // (it only treats Io errors as fatal) — without this check the loop
        // spins forever on a dead display while wayland-backend re-prints the
        // error on every flush attempt.
        if let Some(perr) = conn.protocol_error() {
            log::error!("[window_runner] wayland protocol error, ending session: {perr}");
            end = SessionEnd::ConnectionLost;
            break;
        }
        // Fault injection for the reconnect path (`CCE_UI_FAULT_RECONNECT=<secs>`):
        // real connection loss is a rare race that cannot be provoked on demand,
        // so this drops the session exactly as a transport error would. One-shot
        // per process, so the app reconnects and then stays up.
        if let Some(after) = fault_reconnect_after() {
            static FIRED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if session_start.elapsed() >= after
                && !FIRED.swap(true, std::sync::atomic::Ordering::Relaxed)
            {
                log::warn!("[window_runner] CCE_UI_FAULT_RECONNECT: dropping the session");
                end = SessionEnd::ConnectionLost;
                break;
            }
        }
        match pacer.turn(&mut engine_state) {
            Step::Sleep(timeout) => next_timeout = timeout,
            Step::Exit => {
                // The close dissolve. It is the COMPOSITOR that fades us — it
                // ramps our scene subtree's opacity, which takes the backdrop
                // blur, drop shadow and bevel down with the window; all this side
                // has to do is not vanish before it finishes. So keep the surface
                // mapped and the loop turning for exactly as long as the
                // compositor asked for, then leave. Dispatching (rather than
                // sleeping) keeps the connection pumped and lets any last
                // animation finish on screen while the window dissolves.
                let fade = crate::ipc::request_close_fade();
                if !fade.is_zero() {
                    let until = std::time::Instant::now() + fade;
                    loop {
                        let left = until.saturating_duration_since(std::time::Instant::now());
                        if left.is_zero() {
                            break;
                        }
                        if event_loop.dispatch(left.min(ACTIVE_DISPATCH), &mut engine_state).is_err() {
                            break;
                        }
                    }
                }
                break;
            }
        }
    }

    // Tear the session down: drop its Wayland source from the persistent loop
    // (leaving it would leak a dead source per reconnect), then hand the app
    // back before `engine_state` drops the renderer and the surface with it.
    // `on_exit` and process cleanup belong to the app's real exit, in `run`.
    loop_handle.remove(wayland_token);
    let app = engine_state.inner.take();
    drop(engine_state);
    (app, end)
}

#[cfg(test)]
mod reconnect_tests {
    use super::{after_session, AfterSession, SessionEnd, RECONNECT_ATTEMPTS, RECONNECT_RESET};
    use std::time::Duration;

    const LONG: Duration = Duration::from_secs(60);
    const SHORT: Duration = Duration::from_millis(50);

    #[test]
    fn app_exit_ends_the_process() {
        let mut attempt = 0;
        assert_eq!(after_session(SessionEnd::AppExit, true, LONG, &mut attempt, false), AfterSession::Exit);
        assert_eq!(attempt, 0);
    }

    #[test]
    fn lost_transport_reconnects_with_backoff() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, LONG, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        assert_eq!(attempt, 1);
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(400))
        );
        assert_eq!(attempt, 2);
    }

    /// The compositor exited (its socket is unlinked, or refusing after a
    /// crash). It saved this window for restore, so the successor respawns
    /// the app itself; a client that waited for it reattached beside the
    /// respawned copy, and the restore came up with two of every window.
    #[test]
    fn compositor_gone_exits_instead_of_waiting_for_a_successor() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, LONG, &mut attempt, false),
            AfterSession::Exit
        );
        // Even mid-budget: a reconnect that finds nobody listening is the
        // compositor leaving, not another transport break.
        let mut attempt = 3;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
    }

    /// A daemon the compositor does not restore (the status bar, the
    /// notifier) waits for the successor instead — with no copy to collide
    /// with, exiting only took its D-Bus names down with it. It starts a fresh
    /// budget, and a transport break still reconnects as before.
    #[test]
    fn an_app_that_outlives_the_compositor_waits_for_the_next() {
        let mut attempt = 3;
        assert_eq!(
            after_session(SessionEnd::NoCompositor, true, SHORT, &mut attempt, true),
            AfterSession::AwaitCompositor
        );
        assert_eq!(attempt, 0);
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, LONG, &mut attempt, true),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        // Asked to exit, or never started: it still goes.
        assert_eq!(after_session(SessionEnd::AppExit, true, LONG, &mut attempt, true), AfterSession::Exit);
        assert_eq!(after_session(SessionEnd::NoCompositor, false, SHORT, &mut attempt, true), AfterSession::Exit);
    }

    #[test]
    fn nothing_to_carry_over_gives_up() {
        let mut attempt = 0;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, false, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
        assert_eq!(
            after_session(SessionEnd::NoCompositor, false, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
    }

    #[test]
    fn budget_is_bounded_and_resets_after_a_long_session() {
        let mut attempt = 0;
        for _ in 0..RECONNECT_ATTEMPTS {
            assert!(matches!(
                after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
                AfterSession::Reconnect(_)
            ));
        }
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Exit
        );
        // A session that outlived the reset window earns a fresh budget.
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, RECONNECT_RESET + SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(200))
        );
        assert_eq!(attempt, 1);
    }

    #[test]
    fn backoff_caps_at_six_point_four_seconds() {
        let mut attempt = 6;
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(6400))
        );
        assert_eq!(
            after_session(SessionEnd::ConnectionLost, true, SHORT, &mut attempt, false),
            AfterSession::Reconnect(Duration::from_millis(6400))
        );
    }
}
