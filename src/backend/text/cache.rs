//! The shaped-buffer cache: a buffer per text and everything that shapes it, shared on a hit,
//! evicted least recently used past its cap.

use super::*;

/// One shaped buffer in the text cache, keyed by everything that shapes it
/// beyond its text (the text is the outer map's key, so a lookup borrows it
/// rather than allocating).
pub(super) struct CachedBuffer {
    /// Physical font size, in thousandths of a px.
    pub(super) size_milli: u32,
    /// The family as the font string names it, size stripped.
    pub(super) font: Option<String>,
    pub(super) is_vertical: bool,
    pub(super) attrs: crate::scene::paint::TextAttrs,
    /// The scale factor's bits: a laid-out buffer's wrap width and box are
    /// logical px turned physical by it.
    pub(super) scale_bits: u32,
    /// `Some` for a boxed [`Prim::Text`](crate::scene::paint::Prim::Text) laid
    /// out by [`get_text_buffer_laid_out`]; `None` for a single run.
    pub(super) layout: Option<crate::scene::paint::TextLayout>,
    /// The laid-out buffer's vertical offset in its box (0 for a single run).
    pub(super) voff: f32,
    /// Shared, not cloned, on a hit: a `Buffer` owns every shaped line and
    /// glyph, and the frame used to deep-copy one per text prim per frame.
    pub(super) buffer: Rc<Buffer>,
    /// [`BUFFER_TICK`] at the last hit, for least-recently-used eviction.
    pub(super) last_used: u64,
}

impl CachedBuffer {
    pub(super) fn matches(&self, k: &BufferKey<'_>) -> bool {
        self.size_milli == k.size_milli
            && self.font.as_deref() == k.font
            && self.is_vertical == k.is_vertical
            && self.attrs == k.attrs
            && self.scale_bits == k.scale_bits
            && self.layout == k.layout
    }
}

/// A cache lookup's key, borrowed from the caller.
pub(super) struct BufferKey<'a> {
    pub(super) size_milli: u32,
    pub(super) font: Option<&'a str>,
    pub(super) is_vertical: bool,
    pub(super) attrs: crate::scene::paint::TextAttrs,
    pub(super) scale_bits: u32,
    pub(super) layout: Option<crate::scene::paint::TextLayout>,
}

/// The text cache holds at most this many buffers; past it the least
/// recently used [`BUFFER_EVICT`] go.
pub(super) const BUFFER_CAP: usize = 2000;

pub(super) const BUFFER_EVICT: usize = 100;

std::thread_local! {
    /// Text → every shaped variant of it. Variants per text are few (a size,
    /// a weight, a box), so they are scanned rather than hashed.
    pub(super) static BUFFER_CACHE: std::cell::RefCell<std::collections::HashMap<String, Vec<CachedBuffer>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    pub(super) static BUFFER_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    pub(super) static BUFFER_TICK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

pub(super) fn buffer_tick() -> u64 {
    BUFFER_TICK.with(|t| {
        let n = t.get() + 1;
        t.set(n);
        n
    })
}

/// The cached buffer for `text` under `key`, and its vertical offset.
pub(super) fn buffer_cache_get(text: &str, key: &BufferKey<'_>) -> Option<(Rc<Buffer>, f32)> {
    BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let hit = cache.get_mut(text)?.iter_mut().find(|e| e.matches(key))?;
        hit.last_used = buffer_tick();
        Some((Rc::clone(&hit.buffer), hit.voff))
    })
}

pub(super) fn buffer_cache_put(text: &str, key: &BufferKey<'_>, buffer: Rc<Buffer>, voff: f32) {
    BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if BUFFER_COUNT.with(|c| c.get()) >= BUFFER_CAP {
            let mut ticks: Vec<u64> = cache.values().flatten().map(|e| e.last_used).collect();
            ticks.sort_unstable();
            let cutoff = ticks[BUFFER_EVICT.min(ticks.len()) - 1];
            cache.retain(|_, v| {
                v.retain(|e| e.last_used > cutoff);
                !v.is_empty()
            });
            BUFFER_COUNT.with(|c| c.set(cache.values().map(Vec::len).sum()));
        }
        let entry = CachedBuffer {
            size_milli: key.size_milli,
            font: key.font.map(str::to_owned),
            is_vertical: key.is_vertical,
            attrs: key.attrs,
            scale_bits: key.scale_bits,
            layout: key.layout,
            voff,
            buffer,
            last_used: buffer_tick(),
        };
        match cache.get_mut(text) {
            Some(v) => v.push(entry),
            None => {
                cache.insert(text.to_owned(), vec![entry]);
            }
        }
        BUFFER_COUNT.with(|c| c.set(c.get() + 1));
    });
}
