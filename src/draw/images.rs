//! The image-id queue. [`upload_rgba`] and its kin queue pixels from any code
//! and hand back an id that is usable in [`ImageQuad`]s at once; whichever
//! renderer the process has drains the queue at its next frame
//! ([`take_pending`]) and frees what [`free_image`] queued. See `vk::image`
//! for the Vulkan side and the reasoning behind the streaming path.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

/// One image draw in a 2D frame.
pub struct ImageQuad {
    /// Id from [`upload_rgba`].
    pub image: u32,
    /// Destination rect (x, y, w, h) in physical pixels.
    pub rect: (f32, f32, f32, f32),
    pub alpha: f32,
    /// Draw order: this quad renders before the vertex at this index of
    /// `Frame2D::verts` (so vertices below it stay below, later ones cover it).
    /// Use `u32::MAX` to draw on top of all display-list geometry.
    pub z_before: u32,
    /// Optional scissor (x, y, w, h) in physical pixels.
    pub clip: Option<(u32, u32, u32, u32)>,
}

/// Byte order of the pixels handed over.
///
/// Both are sRGB-encoded 8-bit-per-channel; the difference is only which
/// channel comes first in memory, and the hardware handles it on sample. A
/// caller whose source is already BGRA (Wayland's and WebKit's usual order)
/// should say so rather than swizzle on the CPU: at a fullscreen 3840x2400
/// that swizzle measured 7.4 ms per frame, which is most of a frame budget
/// spent rearranging bytes the sampler can read either way.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PixelFormat {
    Rgba,
    Bgra,
}

/// One queued change to the image table, drained by the renderer.
pub enum Pending {
    Upload { id: u32, pixels: Vec<u8>, width: u32, height: u32, format: PixelFormat, mips: bool },
    /// Replace the contents of an image that already exists, keeping its
    /// id, its `VkImage` and its descriptor set.
    Update { id: u32, pixels: Vec<u8>, width: u32, height: u32, format: PixelFormat },
    Free { id: u32 },
}

static PENDING: Mutex<Vec<Pending>> = Mutex::new(Vec::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(1);

/// A fresh image id from the process-wide counter, for a renderer that
/// uploads into its own table directly (`ImageStage::upload_now`) — never
/// one a queued upload also holds.
pub(crate) fn next_image_id() -> u32 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}
/// How many image tables have been built in this process. See
/// [`renderer_epoch`].
static STAGES_BUILT: AtomicU32 = AtomicU32::new(0);
/// Pixel buffers the renderer has finished with, waiting to be refilled.
/// Bounded: a streaming caller needs one or two in flight, and holding more
/// frame-sized buffers than that is just memory.
static RECYCLED: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
const MAX_RECYCLED: usize = 3;

/// Queue an RGBA8 image for upload; the id is usable in [`ImageQuad`]s right
/// away (draws before the upload lands are skipped, not errors).
pub fn upload_rgba(pixels: Vec<u8>, width: u32, height: u32) -> u32 {
    upload_pixels(pixels, width, height, PixelFormat::Rgba)
}

/// Queue an image whose bytes are in `format`. [`upload_rgba`] is this with
/// [`PixelFormat::Rgba`].
pub fn upload_pixels(pixels: Vec<u8>, width: u32, height: u32, format: PixelFormat) -> u32 {
    queue_upload(pixels, width, height, format, false)
}

/// [`upload_rgba`] for an image that will be drawn much smaller than it is,
/// or at a slant: the image gets a full mip chain, built on the GPU, and is
/// sampled from the level that matches the size it is drawn at. An update
/// ([`update_pixels`]) rebuilds the chain.
///
/// On a device that cannot build one by blitting the image is uploaded
/// without, as [`upload_rgba`] would have.
pub fn upload_rgba_mipmapped(pixels: Vec<u8>, width: u32, height: u32) -> u32 {
    queue_upload(pixels, width, height, PixelFormat::Rgba, true)
}

fn queue_upload(pixels: Vec<u8>, width: u32, height: u32, format: PixelFormat, mips: bool) -> u32 {
    assert_eq!(pixels.len(), (width * height * 4) as usize, "8888 size mismatch");
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    PENDING.lock().unwrap().push(Pending::Upload { id, pixels, width, height, format, mips });
    id
}

/// Replace what `id` holds, keeping the image itself.
///
/// For a caller that redraws the same picture over and over — a page, a video
/// frame, a live preview. Nothing is allocated, no descriptor is rewritten and
/// no image is destroyed, so none of the per-frame `device_wait_idle` that
/// freeing one costs. The size or format changing is allowed and simply falls
/// back to a fresh image under the same id, which is what a window resize
/// does.
///
/// An `id` that does not exist yet is treated as an upload, so a caller can
/// take an id from [`upload_pixels`] and update it from the next frame on
/// without sequencing the two.
pub fn update_pixels(id: u32, pixels: Vec<u8>, width: u32, height: u32, format: PixelFormat) {
    assert_eq!(pixels.len(), (width * height * 4) as usize, "8888 size mismatch");
    PENDING.lock().unwrap().push(Pending::Update { id, pixels, width, height, format });
}

/// A pixel buffer to fill, reusing one the renderer has finished with when
/// there is one of at least `len` bytes.
///
/// The returned buffer is exactly `len` long and its contents are unspecified
/// — a caller is expected to overwrite every byte, which a full-frame readback
/// does by definition. Allocating a fresh frame-sized `Vec` instead measured
/// 7.4 ms against 2.9 ms at 3840x2400: the cost is not the copy, it is the
/// zeroing and the page faults on newly mapped memory.
pub fn recycle_buffer(len: usize) -> Vec<u8> {
    let mut pool = RECYCLED.lock().unwrap();
    if let Some(index) = pool.iter().position(|b| b.capacity() >= len) {
        let mut buf = pool.swap_remove(index);
        buf.clear();
        buf.resize(len, 0);
        return buf;
    }
    vec![0u8; len]
}

/// Hand a finished buffer back to the pool (the renderer, once it has
/// uploaded what a [`Pending`] carried).
pub fn retire_buffer(mut buf: Vec<u8>) {
    let mut pool = RECYCLED.lock().unwrap();
    if pool.len() < MAX_RECYCLED {
        buf.clear();
        pool.push(buf);
    }
}

/// Which renderer's image table the ids handed out right now belong to.
///
/// `0` until the first renderer exists, and again for the whole life of that
/// first renderer: uploads queued before it was built (from `Application::new`
/// and from anything the app did on the way to its first frame) are drained
/// into it, so they are that epoch's images, not a previous one's.
/// Every later renderer — `window_runner` builds one per session, and a lost
/// Wayland transport starts a new session around the same `Application` —
/// counts as the next epoch.
///
/// This is what lets a long-lived cache of image ids notice that its ids have
/// stopped naming anything. It is the cheap half of the contract; the other
/// half is the app's, because only the app knows how to produce the pixels
/// again (see [`Application::renderer_init`], and `upload_icon` for the
/// toolkit's own use of this).
///
/// [`Application::renderer_init`]: crate::engine::Application::renderer_init
pub fn renderer_epoch() -> u32 {
    STAGES_BUILT.load(Ordering::Relaxed).saturating_sub(1)
}

/// Queue an image's GPU resources for destruction.
pub fn free_image(id: u32) {
    PENDING.lock().unwrap().push(Pending::Free { id });
}

/// Everything queued since the last call, in order. The renderer's to drain.
pub fn take_pending() -> Vec<Pending> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

/// A renderer built its image table: from here on [`renderer_epoch`] names it.
pub fn image_table_built() {
    STAGES_BUILT.fetch_add(1, Ordering::Relaxed);
}
