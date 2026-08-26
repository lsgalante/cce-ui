// Drag-and-drop DESTINATION support (wl_data_device).
//
// Opt-in per client: `Application::drop_mimes` returns the mime types the app
// will take, in preference order, and `Application::handle_drop` receives the
// bytes once the source has written them. Both have defaults, so a client that
// implements neither behaves exactly as it did before this module existed —
// it never even accepts an offer, so the drag reads as "not droppable here".
//
// The pipe read runs OFF the main loop on purpose. The source writes into a
// pipe and can be slow (a browser serialising a large image), while the
// engine's calloop loop also drives rendering: reading inline stalls frames,
// and against a source that fills the pipe buffer and waits for us to drain
// it, deadlocks outright. The reader thread posts the finished payload back
// through a calloop channel, so `handle_drop` still runs on the main loop like
// every other Application hook.

use std::io::Read;

use smithay_client_toolkit::data_device_manager::{
    data_device::{DataDeviceData, DataDeviceHandler},
    data_offer::{DataOfferHandler, DragOffer},
    data_source::DataSourceHandler,
    WritePipe,
};
use smithay_client_toolkit::delegate_data_device;
use smithay_client_toolkit::reexports::client::protocol::{
    wl_data_device::WlDataDevice, wl_data_device_manager::DndAction,
    wl_data_source::WlDataSource, wl_surface::WlSurface,
};
use smithay_client_toolkit::reexports::client::{Connection, Proxy, QueueHandle};

use super::window_runner::{Application, EngineState, LogicalPosition};

/// One completed drop, handed from the reader thread back to the main loop.
pub struct DroppedData {
    pub mime: String,
    pub bytes: Vec<u8>,
    pub pos: LogicalPosition,
}

/// Bytes we refuse to accumulate from a single drop. A drop is a pasted
/// image or a URL list, not a disk image; without a ceiling a hostile or
/// broken source can grow the reader thread's buffer without bound.
const MAX_DROP_BYTES: usize = 64 * 1024 * 1024;

impl<A: Application> EngineState<A> {
    /// Create this seat's data device unless it already has one. Drops arrive
    /// on the seat carrying the drag, and the device has to exist *before* the
    /// drag starts to be offered anything, so both seat hooks call this.
    pub(crate) fn ensure_data_device(
        &mut self,
        qh: &QueueHandle<Self>,
        seat: &smithay_client_toolkit::reexports::client::protocol::wl_seat::WlSeat,
    ) {
        if self.data_devices.iter().any(|d| d.data().seat() == seat) {
            return;
        }
        if let Some(manager) = self.data_device_manager.as_ref() {
            self.data_devices.push(manager.get_data_device(qh, seat));
        }
    }

    /// Surface-local logical coordinates for a drag position, matching the
    /// transform the pointer path applies (see `handle_pointer_event`).
    fn drag_logical(&self, x: f64, y: f64) -> LogicalPosition {
        let forced = crate::scale::forced_scale().unwrap_or(1.0);
        LogicalPosition::new(x as f32 / forced, y as f32 / forced)
    }

    /// The first mime type the app asked for that this offer actually
    /// carries. Preference order is the APP's, not the source's — a browser
    /// lists `text/html` before `text/uri-list`, and which of those is more
    /// useful is the app's call.
    fn preferred_mime(&self, offer: &DragOffer) -> Option<String> {
        let wanted = self.inner.as_ref()?.drop_mimes();
        if wanted.is_empty() {
            return None;
        }
        offer.with_mime_types(|offered| {
            wanted
                .iter()
                .find(|w| offered.iter().any(|o| o == *w))
                .map(|w| w.to_string())
        })
    }

    /// Accept (or explicitly decline) the offer and mirror that in the DnD
    /// action, which is what drives the source's cursor feedback: declining
    /// with `None` + no action is what makes a browser show "can't drop
    /// here" over a client that doesn't want the payload.
    fn negotiate_drag(&mut self, offer: &DragOffer, serial: u32) {
        let mime = self.preferred_mime(offer);
        offer.accept_mime_type(serial, mime.clone());
        if mime.is_some() {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
        } else {
            offer.set_actions(DndAction::empty(), DndAction::empty());
        }
        self.drag_mime = mime;
    }
}

impl<A: Application> DataDeviceHandler for EngineState<A> {
    fn enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        x: f64,
        y: f64,
        _wl_surface: &WlSurface,
    ) {
        let Some(offer) = data_device.data::<DataDeviceData>().and_then(|d| d.drag_offer())
        else {
            return;
        };
        self.drag_pos = self.drag_logical(x, y);
        let serial = offer.serial;
        offer.with_mime_types(|m| log::debug!("[dnd] enter, offered: {m:?}"));
        self.negotiate_drag(&offer, serial);
        log::debug!("[dnd] accepted mime: {:?}", self.drag_mime);
    }

    fn leave(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _data_device: &WlDataDevice) {
        self.drag_mime = None;
    }

    fn motion(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        self.drag_pos = self.drag_logical(x, y);
        // Re-accept on motion: a source may add mime types mid-drag, and the
        // action has to be restated or the compositor can clear it.
        if let Some(offer) = data_device.data::<DataDeviceData>().and_then(|d| d.drag_offer()) {
            let serial = offer.serial;
            self.negotiate_drag(&offer, serial);
        }
    }

    /// Clipboard offers are not consumed here — the toolkit has no paste
    /// path yet. Ignoring the event leaves the offer to SCTK's own cleanup.
    fn selection(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _dd: &WlDataDevice) {}

    fn drop_performed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        log::debug!("[dnd] drop_performed, mime={:?}", self.drag_mime);
        let Some(mime) = self.drag_mime.clone() else { return };
        let Some(tx) = self.drop_tx.clone() else { return };
        let Some(offer) = data_device.data::<DataDeviceData>().and_then(|d| d.drag_offer())
        else {
            return;
        };
        let pipe = match offer.receive(mime.clone()) {
            Ok(pipe) => pipe,
            Err(e) => {
                log::warn!("[dnd] receive({mime}) failed: {e}");
                return;
            }
        };
        let pos = self.drag_pos;
        // `finish` is deliberately NOT sent here. It tells the source the
        // operation is complete, and a source is entitled to tear down the
        // moment it arrives — Firefox does, before it has written a byte, so
        // finishing before the read drains an empty pipe. The offer is parked
        // instead and finished once the reader thread reports EOF (see the
        // drop-channel handler in the engine's `run`).
        self.pending_drop_offer = Some(offer);
        std::thread::spawn(move || {
            let mut pipe = pipe;
            let mut buf = Vec::new();
            let mut chunk = [0u8; 16 * 1024];
            loop {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        if buf.len() + n > MAX_DROP_BYTES {
                            log::warn!("[dnd] drop exceeded {MAX_DROP_BYTES} bytes, truncating");
                            break;
                        }
                        buf.extend_from_slice(&chunk[..n]);
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                    Err(e) => {
                        log::warn!("[dnd] read failed: {e}");
                        return;
                    }
                }
            }
            log::debug!("[dnd] read {} bytes for {mime}", buf.len());
            let _ = tx.send(DroppedData { mime, bytes: buf, pos });
        });
        self.drag_mime = None;
    }
}

impl<A: Application> DataOfferHandler for EngineState<A> {
    fn source_actions(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        offer: &mut DragOffer,
        _actions: DndAction,
    ) {
        if self.drag_mime.is_some() {
            offer.set_actions(DndAction::Copy, DndAction::Copy);
        }
    }

    fn selected_action(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _offer: &mut DragOffer,
        _actions: DndAction,
    ) {
    }
}

/// Source-side events. The toolkit never creates a `WlDataSource`, so none of
/// these can fire; the impl exists because `delegate_data_device!` dispatches
/// all three protocol objects through one type.
impl<A: Application> DataSourceHandler for EngineState<A> {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}
    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: String, _: WritePipe) {}
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

delegate_data_device!(@<A: Application> EngineState<A>);
