//! Text shaping for the runner: the shaped-buffer cache, family resolution,
//! the display list's text prims gathered for the glyph pass, and the
//! popover-occlusion clamp. Platform-neutral — nothing here knows the window
//! system; moved out of `window_runner` so another shell can share it.

use cosmic_text::{FontSystem, Buffer, Attrs, Metrics};
use std::rc::Rc;
use crate::draw::TextSpan;

/// One shaped buffer in the text cache, keyed by everything that shapes it
/// beyond its text (the text is the outer map's key, so a lookup borrows it
/// rather than allocating).
struct CachedBuffer {
    /// Physical font size, in thousandths of a px.
    size_milli: u32,
    /// The family as the font string names it, size stripped.
    font: Option<String>,
    is_vertical: bool,
    attrs: crate::scene::paint::TextAttrs,
    /// The scale factor's bits: a laid-out buffer's wrap width and box are
    /// logical px turned physical by it.
    scale_bits: u32,
    /// `Some` for a boxed [`Prim::Text`](crate::scene::paint::Prim::Text) laid
    /// out by [`get_text_buffer_laid_out`]; `None` for a single run.
    layout: Option<crate::scene::paint::TextLayout>,
    /// The laid-out buffer's vertical offset in its box (0 for a single run).
    voff: f32,
    /// Shared, not cloned, on a hit: a `Buffer` owns every shaped line and
    /// glyph, and the frame used to deep-copy one per text prim per frame.
    buffer: Rc<Buffer>,
    /// [`BUFFER_TICK`] at the last hit, for least-recently-used eviction.
    last_used: u64,
}

impl CachedBuffer {
    fn matches(&self, k: &BufferKey<'_>) -> bool {
        self.size_milli == k.size_milli
            && self.font.as_deref() == k.font
            && self.is_vertical == k.is_vertical
            && self.attrs == k.attrs
            && self.scale_bits == k.scale_bits
            && self.layout == k.layout
    }
}

/// A cache lookup's key, borrowed from the caller.
struct BufferKey<'a> {
    size_milli: u32,
    font: Option<&'a str>,
    is_vertical: bool,
    attrs: crate::scene::paint::TextAttrs,
    scale_bits: u32,
    layout: Option<crate::scene::paint::TextLayout>,
}

/// The text cache holds at most this many buffers; past it the least
/// recently used [`BUFFER_EVICT`] go.
const BUFFER_CAP: usize = 2000;
const BUFFER_EVICT: usize = 100;

std::thread_local! {
    /// Text → every shaped variant of it. Variants per text are few (a size,
    /// a weight, a box), so they are scanned rather than hashed.
    static BUFFER_CACHE: std::cell::RefCell<std::collections::HashMap<String, Vec<CachedBuffer>>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
    static BUFFER_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static BUFFER_TICK: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

fn buffer_tick() -> u64 {
    BUFFER_TICK.with(|t| {
        let n = t.get() + 1;
        t.set(n);
        n
    })
}

/// Vertical text, for a process whose window is a vertical bar (the status bar on a
/// screen edge): while it is on, a [`TextLabel`](crate::widget::display::TextLabel) stacks
/// its characters one per line, centred in a column `bar_thickness` px wide, and text is
/// shaped at a looser 1.05 line height. Process-wide because it is a property of the app,
/// not of one window or one label, and shaping may run on any thread. It was two bare
/// `pub static`s at the crate root (`IS_VERTICAL`, `BAR_THICKNESS`) until 2026-10-07.
pub fn set_vertical_text(bar_thickness: Option<u32>) {
    use std::sync::atomic::Ordering::Relaxed;
    // The window's own (`crate::window_state::Props`), and the process's for a worker.
    crate::window_state::entered(|w| w.props.borrow_mut().vertical_text = bar_thickness);
    if let Some(t) = bar_thickness {
        VERTICAL_BAR_THICKNESS.store(t, Relaxed);
    }
    VERTICAL_TEXT.store(bar_thickness.is_some(), Relaxed);
}

/// The vertical bar's thickness while vertical text is on (see [`set_vertical_text`]).
pub fn vertical_text() -> Option<u32> {
    crate::window_state::entered(|w| w.props.borrow().vertical_text).unwrap_or_else(process_vertical_text)
}

/// The process-wide vertical text: the last any window set (what a worker thread reads).
pub(crate) fn process_vertical_text() -> Option<u32> {
    use std::sync::atomic::Ordering::Relaxed;
    VERTICAL_TEXT.load(Relaxed).then(|| VERTICAL_BAR_THICKNESS.load(Relaxed))
}

static VERTICAL_TEXT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static VERTICAL_BAR_THICKNESS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(24);

/// The cached buffer for `text` under `key`, and its vertical offset.
fn buffer_cache_get(text: &str, key: &BufferKey<'_>) -> Option<(Rc<Buffer>, f32)> {
    BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        let hit = cache.get_mut(text)?.iter_mut().find(|e| e.matches(key))?;
        hit.last_used = buffer_tick();
        Some((Rc::clone(&hit.buffer), hit.voff))
    })
}

fn buffer_cache_put(text: &str, key: &BufferKey<'_>, buffer: Rc<Buffer>, voff: f32) {
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

/// The family-name prefix of a [`face_family`] alias.
const FACE_ALIAS_PREFIX: &str = "cce-face:";

/// One [`face_family`] alias: the face at `path`#`index`, named `cce-face:<n>`
/// for its place `n` in [`FONT_OPS`].
struct FaceAlias {
    path: std::path::PathBuf,
    index: u32,
}

/// A font-directory rescan ([`crate::rescan_fonts`]): what changed on disk,
/// as fixed lists rather than a scan each database makes for itself, so every
/// database applies the same change however long after the others. A change,
/// not the whole set: a database built after the rescan already has it, and
/// replaying it there is a no-op rather than a drop of whatever arrived since.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // only `rescan_fonts` makes one, and the browser has no font dirs
pub(crate) struct Rescan {
    /// Font files new on disk, in a system-fonts build's load order.
    added: Vec<std::path::PathBuf>,
    /// Font files gone from disk.
    removed: std::collections::HashSet<std::path::PathBuf>,
    /// The files a bundled-only build loads ([`crate::create_font_system`]):
    /// the CCE fonts dir and the fallback faces.
    bundled: std::collections::HashSet<std::path::PathBuf>,
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // only `rescan_fonts` makes one, and the browser has no font dirs
impl Rescan {
    pub(crate) fn new(
        added: Vec<std::path::PathBuf>,
        removed: std::collections::HashSet<std::path::PathBuf>,
        bundled: std::collections::HashSet<std::path::PathBuf>,
    ) -> Self {
        Rescan { added, removed, bundled }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty()
    }
}

/// A change every `FontSystem` makes to its database, in one order.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // only `rescan_fonts` makes one, and the browser has no font dirs
enum FontOp {
    Alias(FaceAlias),
    Rescan(std::sync::Arc<Rescan>),
}

/// Every change to the font set, in the order made. Process-wide, and only
/// ever appended to: each `FontSystem` replays it into its database in this
/// order (see [`sync_font_ops`]), so two databases loaded alike give every
/// face the same fontdb ID. They must — the shaped-buffer cache is shared by
/// every `FontSystem` on the thread, and a buffer the app's measuring system
/// shaped is rasterized by the engine's with the face IDs it carries. A
/// rescan rides the same log for that reason: applied at a different point
/// relative to the aliases in two databases, it would number them apart.
static FONT_OPS: std::sync::Mutex<Vec<FontOp>> = std::sync::Mutex::new(Vec::new());

thread_local! {
    /// Database address → how many of [`FONT_OPS`] it has applied, and the ID
    /// and op number of the last alias it pushed — checked before it is
    /// trusted, since a dropped `FontSystem`'s address can be reused by a
    /// fresh one.
    static FONT_OPS_SYNCED: std::cell::RefCell<
        std::collections::HashMap<usize, (usize, Option<(cosmic_text::fontdb::ID, usize)>)>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// A family name that resolves to exactly the face at `path`#`index` (the
/// fontdb face index: non-zero only inside a collection), for a font string or
/// a [`Prim::Text`](crate::scene::paint::Prim::Text)'s font.
///
/// Text names a face by family + style, stretch and weight, so a family with
/// two faces alike in all three can show only whichever fontdb met first.
/// They are not rare: BodonianScript's seven numbered cuts and FormP Color
/// Six's colourways are all Regular 400, and Old Timey Mono's Condensed and
/// Compressed files claim normal width, so the font picker previewed each of
/// them as its family's first face. The alias is a family of that one face,
/// so any attrs land on it.
///
/// Repeated calls for the same face return the same name. Every `FontSystem`
/// the text backend shapes or draws with picks the aliases up by itself; one
/// whose database lacks the file leaves the alias unresolved, and its text
/// falls back as an unknown family's would.
pub fn face_family(path: impl AsRef<std::path::Path>, index: u32) -> String {
    let path = path.as_ref();
    let mut ops = FONT_OPS.lock().unwrap_or_else(|e| e.into_inner());
    let n = match ops.iter().position(|op| matches!(op, FontOp::Alias(a) if a.path == path && a.index == index)) {
        Some(n) => n,
        None => {
            ops.push(FontOp::Alias(FaceAlias { path: path.to_path_buf(), index }));
            ops.len() - 1
        }
    };
    format!("{FACE_ALIAS_PREFIX}{n}")
}

/// Record a rescan for every `FontSystem` to apply (see [`FontOp`]), and drop
/// this thread's caches that a family appearing or vanishing makes stale:
/// the shaped buffers (a family that fell back before resolves now, and a
/// removed face's ID must not reach the glyph pass) and the per-family face
/// lookups.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // only `rescan_fonts` makes one, and the browser has no font dirs
pub(crate) fn push_rescan(rescan: Rescan) {
    FONT_OPS.lock().unwrap_or_else(|e| e.into_inner()).push(FontOp::Rescan(std::sync::Arc::new(rescan)));
    BUFFER_CACHE.with(|c| c.borrow_mut().clear());
    BUFFER_COUNT.with(|c| c.set(0));
    MONO_FAMILY_CACHE.with(|c| c.borrow_mut().clear());
    FAMILY_FACES_CACHE.with(|c| c.borrow_mut().clear());
}

/// Bring `fs` up to date with every font-set change made so far — the face
/// aliases and the rescans ([`crate::rescan_fonts`]). The text backend does
/// this itself before it shapes or draws; call it before reading a database
/// directly (a font picker listing its faces) after a rescan.
pub fn sync_font_set(fs: &mut FontSystem) {
    sync_font_ops(fs);
}

pub(crate) fn is_alias_face(face: &cosmic_text::fontdb::FaceInfo) -> bool {
    face.families.first().is_some_and(|(fam, _)| fam.starts_with(FACE_ALIAS_PREFIX))
}

/// Replay the [`FONT_OPS`] `fs` has not applied yet into its database, in
/// their order. Cheap when there is nothing new: a map lookup and one face
/// check. Every op is idempotent, so a database the record cannot vouch for
/// (never seen, or a reused address) replays the whole log safely.
fn sync_font_ops(fs: &mut FontSystem) {
    let ops = FONT_OPS.lock().unwrap_or_else(|e| e.into_inner());
    replay_font_ops(fs, &ops);
}

/// [`sync_font_ops`] against a given log (the tests keep their own, so their
/// rescans reach no other test's databases).
fn replay_font_ops(fs: &mut FontSystem, ops: &[FontOp]) {
    use cosmic_text::fontdb::{Language, Source};
    if ops.is_empty() {
        return;
    }
    let key = fs.db() as *const cosmic_text::fontdb::Database as usize;
    let alias_name = |n: usize| format!("{FACE_ALIAS_PREFIX}{n}");
    let is_alias = |fs: &FontSystem, id: cosmic_text::fontdb::ID, n: usize| {
        fs.db().face(id).is_some_and(|f| f.families.first().is_some_and(|(fam, _)| *fam == alias_name(n)))
    };
    let synced = FONT_OPS_SYNCED.with(|m| m.borrow().get(&key).copied());
    let (mut done, mut last) = match synced {
        Some((done, last)) if last.is_none_or(|(id, n)| is_alias(fs, id, n)) => (done, last),
        _ => (0, None),
    };
    if done == ops.len() {
        return;
    }
    // Replaying from the start: the aliases already present, so none is
    // pushed twice. (Past a trusted count none can be.)
    let mut present: std::collections::HashMap<String, cosmic_text::fontdb::ID> = if done == 0 {
        fs.db().faces().filter(|f| is_alias_face(f)).map(|f| (f.families[0].0.clone(), f.id)).collect()
    } else {
        Default::default()
    };
    for (n, op) in ops.iter().enumerate().skip(done) {
        match op {
            FontOp::Alias(alias) => {
                let name = alias_name(n);
                if let Some(&id) = present.get(&name) {
                    last = Some((id, n));
                } else {
                    let source = fs
                        .db()
                        .faces()
                        .find(|f| {
                            f.index == alias.index
                                && matches!(&f.source, Source::File(p) | Source::SharedFile(p, _) if *p == alias.path)
                        })
                        .cloned();
                    if let Some(mut face) = source {
                        face.families = vec![(name.clone(), Language::English_UnitedStates)];
                        fs.db_mut().push_face_info(face);
                        if let Some(f) = fs.db().faces().find(|f| f.families.first().is_some_and(|(fam, _)| *fam == name)) {
                            present.insert(name, f.id);
                            last = Some((f.id, n));
                        }
                    }
                }
            }
            FontOp::Rescan(rescan) => apply_rescan(fs, rescan),
        }
        done = n + 1;
    }
    FONT_OPS_SYNCED.with(|m| m.borrow_mut().insert(key, (done, last)));
}

/// One rescan, into one database: the faces of removed files are dropped,
/// and added files are loaded — in the rescan's order, so databases alike stay
/// alike. A bundled-only database (one holding nothing outside the bundled
/// set) takes only added bundled files: a rescan does not hand the system's
/// fonts to an app that never loaded them. Alias faces stay even when their
/// file goes — they mark how far a database has synced; one whose file is
/// gone simply no longer resolves.
fn apply_rescan(fs: &mut FontSystem, rescan: &Rescan) {
    use cosmic_text::fontdb::Source;
    let mut present = std::collections::HashSet::new();
    let mut gone = Vec::new();
    let mut has_system = false;
    for f in fs.db().faces() {
        if is_alias_face(f) {
            continue;
        }
        let (Source::File(p) | Source::SharedFile(p, _)) = &f.source else { continue };
        if rescan.removed.contains(p) {
            gone.push(f.id);
            continue;
        }
        has_system |= !rescan.bundled.contains(p);
        present.insert(p.clone());
    }
    let db = fs.db_mut();
    for id in gone {
        db.remove_face(id);
    }
    for p in &rescan.added {
        if !present.contains(p) && (has_system || rescan.bundled.contains(p)) {
            let _ = db.load_font_file(p);
            present.insert(p.clone());
        }
    }
}

fn find_cased_family(fs: &FontSystem, name: &str) -> Option<String> {
    let lower_name = name.to_lowercase();
    for face in fs.db().faces() {
        for (family, _) in &face.families {
            if family.to_lowercase() == lower_name {
                return Some(family.clone());
            }
        }
    }
    None
}

thread_local! {
    /// Family name → is-monospaced, resolved once per family from fontdb's
    /// face metadata (the post table's isFixedPitch, as fontdb records it).
    static MONO_FAMILY_CACHE: std::cell::RefCell<std::collections::HashMap<String, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn family_is_monospaced(fs: &FontSystem, name: &str) -> bool {
    MONO_FAMILY_CACHE.with(|cache| {
        if let Some(&mono) = cache.borrow().get(name) {
            return mono;
        }
        let lower = name.to_lowercase();
        let mono = fs
            .db()
            .faces()
            .find(|face| face.families.iter().any(|(f, _)| f.to_lowercase() == lower))
            .map(|face| face.monospaced)
            .unwrap_or(false);
        cache.borrow_mut().insert(name.to_string(), mono);
        mono
    })
}

thread_local! {
    /// Family name → the (style, stretch, weight) of every face fontdb holds
    /// under that name, resolved once per family for [`snap_to_family_face`].
    static FAMILY_FACES_CACHE: std::cell::RefCell<
        std::collections::HashMap<String, Vec<(cosmic_text::Style, cosmic_text::Stretch, u16)>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The `cosmic_text::Stretch` for an OpenType width class (1–9, clamped; see
/// [`TextAttrs::stretch`](crate::scene::paint::TextAttrs::stretch)).
fn stretch_from_width_class(class: u16) -> cosmic_text::Stretch {
    use cosmic_text::Stretch::*;
    match class {
        0 | 1 => UltraCondensed,
        2 => ExtraCondensed,
        3 => Condensed,
        4 => SemiCondensed,
        5 => Normal,
        6 => SemiExpanded,
        7 => Expanded,
        8 => ExtraExpanded,
        _ => UltraExpanded,
    }
}

/// `attrs` moved onto the nearest face its named family actually has.
///
/// cosmic-text 0.12 takes a face of the requested family only when its style
/// and stretch equal the request (`Attrs::matches`) AND its weight does too
/// (`font_weight_diff == 0` in `FontFallbackIter`); anything else falls
/// through to the fallback families. So a family with no face at the asked
/// weight rendered in some other font entirely: a Thin-only cut at weight
/// 280 asked for at 400, a pixel font that only ships Medium, a
/// Condensed-only family asked for at normal width — the font picker's
/// preview showed the fallback sans for each. Matching here instead follows
/// CSS font matching's order — stretch, then style, then weight — so a named
/// family always renders as itself, in its closest face. A request the
/// family can meet exactly, a generic family, or a name fontdb does not know
/// passes through untouched.
fn snap_to_family_face<'a>(fs: &FontSystem, attrs: Attrs<'a>) -> Attrs<'a> {
    use cosmic_text::{Stretch, Style};
    let cosmic_text::Family::Name(name) = attrs.family else { return attrs };
    let faces = FAMILY_FACES_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .entry(name.to_string())
            .or_insert_with(|| {
                fs.db()
                    .faces()
                    .filter(|face| face.families.iter().any(|(f, _)| f == name))
                    .map(|face| (face.style, face.stretch, face.weight.0))
                    .collect()
            })
            .clone()
    });
    let want = (attrs.style, attrs.stretch, attrs.weight.0);
    if faces.is_empty() || faces.contains(&want) {
        return attrs;
    }
    let stretch_rank = |s: Stretch| {
        let (w, f) = (attrs.stretch.to_number() as i32, s.to_number() as i32);
        // Narrower-first below normal width, wider-first above it (CSS).
        let toward = if w <= 5 { f < w } else { f > w };
        ((w - f).abs() * 2 + if toward || f == w { 0 } else { 1 }) as u32
    };
    let style_rank = |s: Style| match (attrs.style, s) {
        (a, b) if a == b => 0u32,
        (Style::Italic, Style::Oblique) | (Style::Oblique, Style::Italic) => 1,
        _ => 2,
    };
    let weight_rank = |w: u16| {
        let want = attrs.weight.0;
        // Ties go lighter for a light-to-regular request, heavier above it.
        let off_side = if want <= 450 { w > want } else { w < want };
        (want.abs_diff(w) as u32) * 2 + off_side as u32
    };
    let Some(&(style, stretch, weight)) = faces
        .iter()
        .min_by_key(|(st, sr, w)| (stretch_rank(*sr), style_rank(*st), weight_rank(*w)))
    else {
        return attrs;
    };
    attrs.style(style).stretch(stretch).weight(cosmic_text::Weight(weight))
}

/// The shaping mode for one text run: ASCII-only text in a MONOSPACED face
/// shapes `Basic`, everything else `Advanced`.
///
/// `Basic` bypasses OpenType substitution and positioning, and for ASCII in a
/// mono face that is exactly right: a mono font's ligatures are the one thing
/// `Advanced` adds there, and they break the grid — Chivo Mono's `liga`
/// squeezes f+i into a single-advance ﬁ glyph, which is why the bar's window
/// titles rendered "file" with a cramped fi — while mono faces carry no
/// kerning to lose. Proportional faces keep `Advanced` (their kerning and
/// ligatures are wanted — a font preview must not misrepresent the face), and
/// any non-ASCII text keeps real shaping (combining marks, emoji, complex
/// scripts) whatever the face.
pub fn shaping_for(fs: &FontSystem, text: &str, family: &cosmic_text::Family) -> cosmic_text::Shaping {
    if text.is_ascii() {
        if let cosmic_text::Family::Name(name) = family {
            if family_is_monospaced(fs, name) {
                return cosmic_text::Shaping::Basic;
            }
        }
    }
    cosmic_text::Shaping::Advanced
}

pub fn get_text_buffer(fs: &mut FontSystem, text: &str, size: f32, font: Option<&str>) -> Buffer {
    get_text_buffer_attrs(fs, text, size, font, crate::scene::paint::TextAttrs::default())
}

/// [`get_text_buffer`] plus shaping attributes (italic / weight) — the backend's shape entry
/// for `Prim::Text` prims that carry [`TextAttrs`] (the font picker's style-variant previews).
pub fn get_text_buffer_attrs(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
) -> Buffer {
    Buffer::clone(&shared_text_buffer(fs, text, size, font, text_attrs))
}

/// [`get_text_buffer_attrs`] without the copy: the cached buffer itself,
/// shared. What the frame and the toolkit's own measuring use — a `Buffer`
/// owns every shaped line and glyph, so the clone the public functions hand
/// out costs as much as the text is long.
pub(crate) fn shared_text_buffer(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
) -> Rc<Buffer> {
    let scale = crate::scale::scale_factor();
    let (family_name, font_size) = match font {
        Some(font_str) => {
            let (family, parsed_size) = crate::layout::split_font_string(font_str);
            (Some(family), parsed_size.unwrap_or(size))
        }
        None => (None, size),
    };

    let physical_size = font_size * scale;
    let is_vertical = vertical_text().is_some();
    let key = BufferKey {
        size_milli: (physical_size * 1000.0).round() as u32,
        font: family_name,
        is_vertical,
        attrs: text_attrs,
        scale_bits: scale.to_bits(),
        layout: None,
    };
    if let Some((buf, _)) = buffer_cache_get(text, &key) {
        return buf;
    }
    // Before any family resolves: a face alias must be in this database first.
    sync_font_ops(fs);

    let line_height = if is_vertical {
        physical_size * 1.05
    } else {
        physical_size * 1.0
    };
    let metrics = Metrics::new(physical_size, line_height);
    let mut buf = Buffer::new(fs, metrics);
    let mut attrs = Attrs::new();

    let (sans_fallback, serif_fallback, mono_fallback, _) = crate::layout::read_preferred_fonts();

    let resolved_storage = family_name.and_then(|font_name| match font_name {
        "monospace" if !mono_fallback.is_empty() => find_cased_family(fs, &mono_fallback),
        "sans-serif" if !sans_fallback.is_empty() => find_cased_family(fs, &sans_fallback),
        "serif" if !serif_fallback.is_empty() => find_cased_family(fs, &serif_fallback),
        _ => None,
    });

    let resolved_sans = if !sans_fallback.is_empty() {
        find_cased_family(fs, &sans_fallback)
    } else {
        None
    };

    let family = if let Some(font_family) = family_name {
        match font_family {
            "monospace" => {
                if !mono_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::Name(crate::layout::get_system_monospace_font())
                    }
                } else {
                    cosmic_text::Family::Name(crate::layout::get_system_monospace_font())
                }
            }
            "sans-serif" => {
                if !sans_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::SansSerif
                    }
                } else {
                    cosmic_text::Family::SansSerif
                }
            }
            "serif" => {
                if !serif_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::Serif
                    }
                } else {
                    cosmic_text::Family::Serif
                }
            }
            name => cosmic_text::Family::Name(name),
        }
    } else {
        if !sans_fallback.is_empty() {
            if let Some(ref cased) = resolved_sans {
                cosmic_text::Family::Name(cased)
            } else {
                cosmic_text::Family::SansSerif
            }
        } else {
            cosmic_text::Family::SansSerif
        }
    };
    attrs = attrs.family(family);
    if text_attrs.italic {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    if let Some(w) = text_attrs.weight {
        attrs = attrs.weight(cosmic_text::Weight(w));
    }
    if let Some(s) = text_attrs.stretch {
        attrs = attrs.stretch(stretch_from_width_class(s));
    }
    let attrs = snap_to_family_face(fs, attrs);
    let shaping = shaping_for(fs, text, &family);
    buf.set_text(fs, text, attrs, shaping);
    buf.shape_until_scroll(fs, true);

    let buf = Rc::new(buf);
    buffer_cache_put(text, &key, Rc::clone(&buf), 0.0);
    buf
}

/// Byte-offset → x mapping of single-line `text`, shaped exactly as the renderer draws it —
/// same buffer cache as the draw, so this is a lookup when the text is already on screen.
/// Returns ascending `(byte_idx, x)` pairs (one per cluster start, logical px, relative to
/// the text origin), terminated by `(text.len(), total_advance)`. A cluster's x is its
/// LEADING edge — its left in left-to-right text, its right in right-to-left — so the pairs
/// are ascending in bytes but not in x where the text turns; the closing pair is the width,
/// which is where the caret after the text stands only in left-to-right text. For carets and
/// selections in text of either direction use [`shaped_run`]. This is the correct
/// source for caret placement and click→cursor mapping in hand-rolled text fields:
/// `measure_text_width` reports SVG-rasterized inked extent through fontdb's family
/// resolution, which disagrees with cosmic-text's advance and can even resolve a
/// different face — a caret placed with it drifts off the drawn glyphs.
pub fn shaped_cluster_offsets(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
) -> Vec<(usize, f32)> {
    let scale = crate::scale::scale_factor().max(1.0);
    let buffer = shared_text_buffer(fs, text, size, font, crate::scene::paint::TextAttrs::default());
    let run = shaped_run(&buffer, text, scale);
    let mut out: Vec<(usize, f32)> = run.clusters.iter().map(|c| (c.start, c.leading())).collect();
    out.push((text.len(), run.width));
    out
}

/// Whether `text` is a right-to-left paragraph: its first strong character is right to left
/// (Hebrew, Arabic, …), as the Unicode bidirectional algorithm decides a paragraph's base
/// direction. Text with no strong character at all is left to right.
pub fn paragraph_rtl(text: &str) -> bool {
    matches!(unicode_bidi::get_base_direction(text), unicode_bidi::Direction::Rtl)
}

/// The visual order of runs of one line, given each run's text, in a paragraph whose base
/// direction is `rtl`: the bidirectional algorithm's reordering (rule L2) at the
/// granularity of runs. A run is at the paragraph's level when it has no strong character,
/// one level up when its first strong character goes against the paragraph, and the
/// sequences at each level from the highest down are reversed. So in a left-to-right line
/// two Hebrew runs side by side swap places, and in a right-to-left line every run is
/// placed from the right while English runs keep their order among themselves. Returns
/// indices into `runs`, left to right.
pub fn visual_run_order(runs: &[&str], rtl: bool) -> Vec<usize> {
    let base: u8 = if rtl { 1 } else { 0 };
    let levels: Vec<u8> = runs
        .iter()
        .map(|t| match unicode_bidi::get_base_direction(*t) {
            unicode_bidi::Direction::Rtl => if base % 2 == 1 { base } else { base + 1 },
            unicode_bidi::Direction::Ltr => if base.is_multiple_of(2) { base } else { base + 1 },
            unicode_bidi::Direction::Mixed => base,
        })
        .collect();
    visual_order(&levels)
}

/// The embedding level of every byte of `text` as one paragraph (`rtl` its base
/// direction), neutrals resolved from their neighbours — the bidirectional algorithm's
/// levels, rules W1–I2. Odd is right to left.
pub fn bidi_levels(text: &str, rtl: bool) -> Vec<u8> {
    let base = if rtl { unicode_bidi::Level::rtl() } else { unicode_bidi::Level::ltr() };
    unicode_bidi::BidiInfo::new(text, Some(base)).levels.iter().map(|l| l.number()).collect()
}

/// Left-to-right order of items at `levels` (rule L2): from the highest level down to the
/// lowest odd one, every maximal run of items at that level or above is reversed.
pub fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let max = levels.iter().copied().max().unwrap_or(0);
    for level in (1..=max).rev() {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let start = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[start..i].reverse();
            } else {
                i += 1;
            }
        }
    }
    order
}

/// One cluster of a [`ShapedRun`]: the bytes `start..end` drawn as one unit (a glyph, a
/// ligature, a base with its marks), spanning `x0..x1` (logical px, left to right whatever
/// the direction), and whether it reads right to left.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedCluster {
    pub start: usize,
    pub end: usize,
    pub x0: f32,
    pub x1: f32,
    pub rtl: bool,
}

impl ShapedCluster {
    /// Where the caret before this cluster stands: its left in left-to-right text, its
    /// right in right-to-left.
    pub fn leading(&self) -> f32 {
        if self.rtl { self.x1 } else { self.x0 }
    }

    /// Where the caret after it stands.
    pub fn trailing(&self) -> f32 {
        if self.rtl { self.x0 } else { self.x1 }
    }
}

/// A single line of text as shaped, in the terms an editor needs whatever its direction:
/// where the caret stands at every char boundary, the clusters in logical order with their
/// boxes, the width, and the paragraph's base direction (cosmic-text takes it from the first
/// strong character, as the Unicode bidirectional algorithm does).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapedRun {
    /// `(byte, x)` for every char boundary, ascending in bytes, ending at `text.len()`: the
    /// caret before the char at `byte` stands at its cluster's leading edge (a boundary
    /// inside a cluster takes the cluster's), and the caret after the text at the last
    /// char's trailing edge — the RIGHT end of left-to-right text, the LEFT end of
    /// right-to-left. In mixed text x is not monotonic.
    pub stops: Vec<(usize, f32)>,
    /// The clusters, in logical order (by `start`).
    pub clusters: Vec<ShapedCluster>,
    /// The glyphs' extent, trailing spaces included.
    pub width: f32,
    /// The paragraph's base direction.
    pub rtl: bool,
}

impl ShapedRun {
    /// The char boundary nearest x: a click's caret.
    pub fn index_at(&self, x: f32) -> usize {
        self.stops
            .iter()
            .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
            .map_or(0, |s| s.0)
    }

    /// The caret x before byte `at` (the nearest boundary at or before it).
    pub fn x_of(&self, at: usize) -> f32 {
        self.stops.iter().rev().find(|s| s.0 <= at).map_or(0.0, |s| s.1)
    }

    /// The x spans the bytes `a..b` cover, left to right and merged where they touch: one
    /// span in text of one direction, more where a selection crosses a change of direction
    /// (the logical range is then visually apart).
    pub fn spans(&self, a: usize, b: usize) -> Vec<(f32, f32)> {
        let mut boxes: Vec<(f32, f32)> =
            self.clusters.iter().filter(|c| c.start < b && c.end > a).map(|c| (c.x0, c.x1)).collect();
        boxes.sort_by(|p, q| p.0.total_cmp(&q.0));
        let mut out: Vec<(f32, f32)> = Vec::new();
        for (x0, x1) in boxes {
            match out.last_mut() {
                Some(last) if x0 <= last.1 + 0.5 => last.1 = last.1.max(x1),
                _ => out.push((x0, x1)),
            }
        }
        out
    }
}

/// Shape-derived positions of single-line `text` from its `buffer` (see [`ShapedRun`]),
/// in logical px at `scale`.
pub fn shaped_run(buffer: &Buffer, text: &str, scale: f32) -> ShapedRun {
    let scale = scale.max(0.01);
    let mut rtl = false;
    let mut glyphs: Vec<ShapedCluster> = Vec::new();
    let mut first = true;
    for run in buffer.layout_runs() {
        if first {
            rtl = run.rtl;
            first = false;
        }
        for g in run.glyphs {
            glyphs.push(ShapedCluster { start: g.start, end: g.end, x0: g.x / scale, x1: (g.x + g.w) / scale, rtl: g.level.is_rtl() });
        }
    }
    // The Basic shaping path's span-relative starts (see `normalized_glyph_starts`): ASCII
    // only, one glyph per char in logical order.
    if text.is_ascii() && glyphs.windows(2).any(|w| w[1].start < w[0].start) {
        for (g, (i, _)) in glyphs.iter_mut().zip(text.char_indices()) {
            g.start = i;
            g.end = i + 1;
        }
    }
    // One cluster per byte range: a base and its marks are several glyphs of one cluster.
    glyphs.sort_by(|a, b| a.start.cmp(&b.start).then(a.x0.total_cmp(&b.x0)));
    let mut clusters: Vec<ShapedCluster> = Vec::new();
    for g in glyphs {
        match clusters.last_mut() {
            Some(c) if c.start == g.start && c.end == g.end => {
                c.x0 = c.x0.min(g.x0);
                c.x1 = c.x1.max(g.x1);
            }
            _ => clusters.push(g),
        }
    }
    let width = clusters.iter().map(|c| c.x1).fold(0.0, f32::max);
    let mut stops: Vec<(usize, f32)> = Vec::with_capacity(text.len() + 1);
    let mut ci = 0;
    let mut last_x = if rtl { width } else { 0.0 };
    for (b, _) in text.char_indices() {
        while ci < clusters.len() && clusters[ci].end <= b {
            ci += 1;
        }
        if let Some(c) = clusters.get(ci).filter(|c| c.start <= b) {
            last_x = c.leading();
        }
        stops.push((b, last_x));
    }
    let end_x = match clusters.last() {
        Some(c) => c.trailing(),
        None => 0.0,
    };
    stops.push((text.len(), end_x));
    ShapedRun { stops, clusters, width, rtl }
}

/// Every glyph of `buffer`'s layout runs as `(start_byte, x, w)` (physical px),
/// with `start` normalized to be text-relative.
///
/// Exists because cosmic-text 0.12's `Shaping::Basic` path (`shape_skip`) emits
/// `LayoutGlyph::start` relative to the shape SPAN — it resets to 0 at every
/// word — while the Advanced path emits line-relative starts. `shaping_for`
/// picks Basic exactly for ASCII text in a monospace family (the DE's default
/// control font), so any multi-word value hit the bug: offsets keyed by those
/// starts collide on the low columns and the caret/selection walk off the
/// glyphs. That path shapes strictly one glyph per char in logical order, and
/// only ASCII text — so in ASCII text a reset means it, and byte starts are
/// rebuilt by walking the text's chars. Anywhere else glyph starts fall
/// because the text turns right to left (glyphs are in visual order), and are
/// kept: until 2026-10-08 any fall was read as the reset, which scrambled
/// every right-to-left run. `text` must be the single line the buffer was
/// shaped from.
pub(crate) fn normalized_glyph_starts(buffer: &Buffer, text: &str) -> Vec<(usize, f32, f32)> {
    let mut glyphs: Vec<(usize, f32, f32)> = Vec::new();
    let mut monotonic = true;
    let mut prev = 0usize;
    for run in buffer.layout_runs() {
        for g in run.glyphs {
            if g.start < prev {
                monotonic = false;
            }
            prev = g.start;
            glyphs.push((g.start, g.x, g.w));
        }
    }
    if !monotonic && text.is_ascii() {
        let mut starts = text.char_indices().map(|(i, _)| i);
        for g in glyphs.iter_mut() {
            g.0 = starts.next().unwrap_or(text.len());
        }
    }
    glyphs
}

/// Shape a boxed [`Prim::Text`] (word-wrap + alignment) and return `(buffer, vertical_offset)`.
/// Starts from [`get_text_buffer_attrs`]'s single run for all the family resolution, then
/// re-lays it out: a 1.4 line-height (the placed-text convention), the wrap width, per-line
/// horizontal alignment, and re-shapes. The vertical offset positions the shaped block inside
/// the box per `align_v`. Cached beside the single runs, keyed by the box as well.
pub fn get_text_buffer_laid_out(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    layout: crate::scene::paint::TextLayout,
) -> (Buffer, f32) {
    let (buf, voff) = shared_laid_out_buffer(fs, text, size, font, text_attrs, layout);
    (Buffer::clone(&buf), voff)
}

/// [`get_text_buffer_laid_out`] without the copy, as [`shared_text_buffer`] is to
/// [`get_text_buffer_attrs`]. Until 2026-10-04 a boxed text was re-shaped from scratch
/// every frame it was drawn; the box is part of the key now.
pub(crate) fn shared_laid_out_buffer(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    layout: crate::scene::paint::TextLayout,
) -> (Rc<Buffer>, f32) {
    use crate::scene::paint::{AlignH, AlignV};
    let scale = crate::scale::scale_factor();

    // The font string may override the size ("family:size") — mirror shared_text_buffer.
    let (family, font_size) = match font {
        Some(font_str) => {
            let (family, parsed_size) = crate::layout::split_font_string(font_str);
            (Some(family), parsed_size.unwrap_or(size))
        }
        None => (None, size),
    };
    let physical_size = font_size * scale;
    let key = BufferKey {
        size_milli: (physical_size * 1000.0).round() as u32,
        font: family,
        is_vertical: vertical_text().is_some(),
        attrs: text_attrs,
        scale_bits: scale.to_bits(),
        layout: Some(layout),
    };
    if let Some(hit) = buffer_cache_get(text, &key) {
        return hit;
    }

    // Resolved family + attrs come from the single run; this copy is ours to re-lay-out.
    let mut buf = Buffer::clone(&shared_text_buffer(fs, text, size, font, text_attrs));
    let line_height = physical_size * 1.4;
    buf.set_metrics(fs, Metrics::new(physical_size, line_height));
    buf.set_size(fs, layout.wrap_width.map(|w| w * scale), Some(layout.box_height * scale));

    let align = match layout.align_h {
        AlignH::Left => cosmic_text::Align::Left,
        AlignH::Center => cosmic_text::Align::Center,
        AlignH::Right => cosmic_text::Align::Right,
    };
    for line in &mut buf.lines {
        line.set_align(Some(align));
    }
    buf.shape_until_scroll(fs, true);

    // Vertical offset (logical) from the shaped run count, matching the legacy per-app math.
    let runs = buf.layout_runs().count();
    let total_h = runs as f32 * font_size * 1.4;
    let voff = match layout.align_v {
        AlignV::Top => 0.0,
        AlignV::Middle => ((layout.box_height - total_h) / 2.0).max(0.0),
        AlignV::Bottom => (layout.box_height - total_h).max(0.0),
    };
    let buf = Rc::new(buf);
    buffer_cache_put(text, &key, Rc::clone(&buf), voff);
    (buf, voff)
}

/// A text item's clip rect in physical pixels. This was `glyphon::TextBounds` — the one
/// glyphon-owned type cce-ui ever used, everything else being a cosmic-text re-export — so
/// it is defined here now that the dependency is cosmic-text directly. Same plain
/// four-`i32` layout; it is only an intermediate on the way to `TextSpan::bounds`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// A display-list text prim ready for the glyph pass: a [`TextItem`](crate::widget::TextItem) whose
/// buffer is the cache's own, shared rather than copied. Public so a shell
/// outside the crate can hold what [`build_frame`](super::frame::build_frame)
/// fills; its fields are the frame's own.
pub struct DlText {
    pub(crate) buffer: Rc<Buffer>,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) color: cosmic_text::Color,
    pub(crate) bounds: Option<[f32; 4]>,
    pub(crate) clip_circle: Option<[f32; 3]>,
    pub(crate) clip_rrect: Option<[f32; 5]>,
}

/// The display list's Text prims, shaped through the shared buffer cache and
/// held for the glyph pass (the [`TextSpan`]s built by [`dl_text_spans`] borrow
/// these). Clip = the paint walk's item clip ∩ the prim's own bounds, in
/// logical space. Shared by the window's frame and the context-menu popup's.
pub(crate) fn collect_dl_text(fs: &mut FontSystem, dl: &crate::scene::paint::DisplayList, out: &mut Vec<DlText>) {
    // A cached buffer may have been shaped by another `FontSystem` (the app's
    // measuring one); the glyph pass rasterizes its face-alias IDs with this one.
    sync_font_ops(fs);
    for item in &dl.items {
        if let crate::scene::paint::Prim::Text { text, x, y, font_size, color, alpha, font, bounds, attrs, layout } = &item.prim {
            let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
            let merged = match (clip, *bounds) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            // Boxed text (wrap/align) is laid out in its box and shifts down by the
            // vertical offset; ordinary labels are a single run. Both cached.
            let (buffer, y_off) = match layout {
                Some(l) => shared_laid_out_buffer(fs, text, *font_size, font.as_deref(), *attrs, *l),
                None => (shared_text_buffer(fs, text, *font_size, font.as_deref(), *attrs), 0.0),
            };
            out.push(DlText {
                buffer,
                x: *x,
                y: *y + y_off,
                color: cosmic_text::Color::rgba(
                    color[0],
                    color[1],
                    color[2],
                    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                ),
                bounds: merged,
                clip_circle: item.clip_circle,
                clip_rrect: item.clip_rrect,
            });
        }
    }
}

/// The glyph pass's spans for `items`: each clamped to the surface and its
/// own bounds, then by the popover-occlusion clamp against `overlays`.
pub(crate) fn dl_text_spans<'a>(
    items: &'a [DlText],
    scale_f32: f32,
    bounds: TextBounds,
    overlays: &[(f32, f32, f32, f32)],
) -> Vec<TextSpan<'a>> {
    let mut spans: Vec<TextSpan<'a>> = Vec::new();
    for ti in items {
        let mut item_bounds = if let Some([l, t, r, b]) = ti.bounds {
            TextBounds {
                left: ((l * scale_f32).round() as i32).clamp(0, bounds.right),
                top: ((t * scale_f32).round() as i32).clamp(0, bounds.bottom),
                right: ((r * scale_f32).round() as i32).clamp(0, bounds.right),
                bottom: ((b * scale_f32).round() as i32).clamp(0, bounds.bottom),
            }
        } else {
            bounds
        };
        popover_occlusion_clamp(overlays, ti, scale_f32, &mut item_bounds);
        spans.push(TextSpan {
            buffer: &ti.buffer,
            left: (ti.x * scale_f32).round(),
            top: (ti.y * scale_f32).round(),
            // Buffers are shaped at physical size (get_text_buffer_attrs).
            scale: 1.0,
            bounds: Some([
                item_bounds.left,
                item_bounds.top,
                item_bounds.right,
                item_bounds.bottom,
            ]),
            default_color: [
                ti.color.r() as f32 / 255.0,
                ti.color.g() as f32 / 255.0,
                ti.color.b() as f32 / 255.0,
                ti.color.a() as f32 / 255.0,
            ],
            rotation: None,
            // Circle wins when both are set (the circular pane's innermost clip);
            // otherwise a rounded-rect clip rides as center+radius with extents.
            clip_circle: match (ti.clip_circle, ti.clip_rrect) {
                (Some(c), _) => [c[0] * scale_f32, c[1] * scale_f32, c[2] * scale_f32],
                (None, Some(rr)) => [rr[0] * scale_f32, rr[1] * scale_f32, rr[4] * scale_f32],
                (None, None) => [0.0; 3],
            },
            clip_extents: match (ti.clip_circle, ti.clip_rrect) {
                (None, Some(rr)) => [rr[2] * scale_f32, rr[3] * scale_f32],
                _ => [0.0; 2],
            },
        });
    }
    spans
}

/// The popover-occlusion clamp shared by the default [`Application::text_areas`] mapping and
/// the display-list text path: clip a text item's bounds so it does not bleed through an open
/// popover's plate. A text item whose own bounds coincide with a popover rect IS that popover's
/// text and is left alone; anything else that intersects gets clamped horizontally toward
/// whichever side of the popover it starts on.
/// Clamp a text item's bounds away from the registered popover rects it
/// runs under, so page text does not bleed through a floating plate.
///
/// A text item BELONGS to a popover when it carries exactly that popover's
/// rect as its bounds (the convention every popover's own labels follow),
/// and it is then clamped only against the popovers registered AFTER its
/// own — `overlay_rects` is in stacking order, the shared context menu
/// last. Before 2026-09-22 a popover's text was exempt from its own rect
/// alone and clamped against every other, so a context menu opened over a
/// modal dialog had its labels clipped by the dialog it was drawn on top
/// of, and showed as a plate with no legible entries.
fn popover_occlusion_clamp(
    overlay_rects: &[(f32, f32, f32, f32)],
    ti: &DlText,
    scale_f32: f32,
    item_bounds: &mut TextBounds,
) {
    let owner = ti.bounds.and_then(|[l, t, r, b]| {
        overlay_rects.iter().position(|&(ox, oy, ow, oh)| {
            (l - ox).abs() < 1.0
                && (t - oy).abs() < 1.0
                && (r - (ox + ow)).abs() < 1.0
                && (b - (oy + oh)).abs() < 1.0
        })
    });
    let first_above = owner.map_or(0, |k| k + 1);
    for &(ox, oy, ow, oh) in &overlay_rects[first_above..] {
        let ol = (ox * scale_f32).round() as i32;
        let ot = (oy * scale_f32).round() as i32;
        let or = ((ox + ow) * scale_f32).round() as i32;
        let ob = ((oy + oh) * scale_f32).round() as i32;

        let tx_pixel = ti.x * scale_f32;
        let ty_pixel = ti.y * scale_f32;

        let mut text_w = 0.0f32;
        let mut run_count = 0;
        for run in ti.buffer.layout_runs() {
            text_w = text_w.max(run.line_w);
            run_count += 1;
        }
        let text_h = run_count as f32 * ti.buffer.metrics().line_height;

        let actual_left = tx_pixel;
        let actual_right = tx_pixel + text_w;
        let actual_top = ty_pixel;
        let actual_bottom = ty_pixel + text_h;

        if actual_left < or as f32
            && actual_right > ol as f32
            && actual_top < ob as f32
            && actual_bottom > ot as f32
        {
            if tx_pixel < ol as f32 {
                item_bounds.right = item_bounds.right.min(ol);
            } else {
                item_bounds.left = item_bounds.left.max(or);
            }
        }
    }
}

#[cfg(test)]
mod text_cache_tests {
    use super::*;
    use crate::scene::paint::{AlignH, AlignV, TextAttrs, TextLayout};

    fn boxed(wrap: f32) -> TextLayout {
        TextLayout { wrap_width: Some(wrap), box_height: 80.0, align_h: AlignH::Center, align_v: AlignV::Middle }
    }

    /// A hit hands back the cached buffer itself: the frame used to deep-copy
    /// every text prim's shaped buffer, every frame.
    #[test]
    fn a_hit_shares_the_buffer_rather_than_copying_it() {
        let mut fs = crate::geometry_font_system().lock().unwrap();
        let attrs = TextAttrs::default();
        let a = shared_text_buffer(&mut fs, "shared run", 14.0, Some("monospace"), attrs);
        let b = shared_text_buffer(&mut fs, "shared run", 14.0, Some("monospace"), attrs);
        assert!(Rc::ptr_eq(&a, &b));
        let bold = shared_text_buffer(&mut fs, "shared run", 14.0, Some("monospace"), TextAttrs { weight: Some(700), ..attrs });
        assert!(!Rc::ptr_eq(&a, &bold), "attrs are part of the key");
        let sized = shared_text_buffer(&mut fs, "shared run", 14.0, Some("monospace 18"), attrs);
        assert!(!Rc::ptr_eq(&a, &sized), "a size in the font string is part of the key");
    }

    /// Boxed text is cached by its box, and a hit is what a fresh layout of
    /// the same box would be.
    #[test]
    fn a_laid_out_buffer_is_cached_by_its_box() {
        let mut fs = crate::geometry_font_system().lock().unwrap();
        let text = "a line long enough to wrap inside a narrow box";
        let attrs = TextAttrs::default();
        let (a, va) = shared_laid_out_buffer(&mut fs, text, 14.0, None, attrs, boxed(90.0));
        let (b, vb) = shared_laid_out_buffer(&mut fs, text, 14.0, None, attrs, boxed(90.0));
        assert!(Rc::ptr_eq(&a, &b));
        assert_eq!(va, vb);
        let (wide, _) = shared_laid_out_buffer(&mut fs, text, 14.0, None, attrs, boxed(400.0));
        assert!(!Rc::ptr_eq(&a, &wide), "a different box is a different layout");
        let single = shared_text_buffer(&mut fs, text, 14.0, None, attrs);
        assert!(!Rc::ptr_eq(&a, &single), "a box never answers for the single run");

        // The cached layout against one shaped from nothing.
        BUFFER_CACHE.with(|c| c.borrow_mut().clear());
        BUFFER_COUNT.with(|c| c.set(0));
        let (fresh, vf) = shared_laid_out_buffer(&mut fs, text, 14.0, None, attrs, boxed(90.0));
        assert!(!Rc::ptr_eq(&a, &fresh));
        assert_eq!(va, vf);
        let runs = |b: &Buffer| b.layout_runs().map(|r| (r.line_y, r.line_w, r.glyphs.len())).collect::<Vec<_>>();
        assert_eq!(runs(&a), runs(&fresh));
    }

    /// The cache holds at most `BUFFER_CAP` buffers, and eviction takes the
    /// least recently used, not the oldest inserted.
    #[test]
    fn eviction_keeps_the_cap_and_the_recently_used() {
        BUFFER_CACHE.with(|c| c.borrow_mut().clear());
        BUFFER_COUNT.with(|c| c.set(0));
        let key = |size_milli| BufferKey {
            size_milli,
            font: None,
            is_vertical: false,
            attrs: TextAttrs::default(),
            scale_bits: 1.0f32.to_bits(),
            layout: None,
        };
        let empty = || Rc::new(Buffer::new_empty(Metrics::new(10.0, 10.0)));
        for i in 0..BUFFER_CAP as u32 {
            buffer_cache_put(&format!("t{i}"), &key(i), empty(), 0.0);
        }
        // The first inserted is touched, so it is no longer the least recent.
        assert!(buffer_cache_get("t0", &key(0)).is_some());
        buffer_cache_put("over", &key(0), empty(), 0.0);
        let count = BUFFER_CACHE.with(|c| c.borrow().values().map(Vec::len).sum::<usize>());
        assert_eq!(count, BUFFER_CAP - BUFFER_EVICT + 1);
        assert_eq!(BUFFER_COUNT.with(|c| c.get()), count);
        assert!(buffer_cache_get("t0", &key(0)).is_some(), "recently used survives");
        assert!(buffer_cache_get("t1", &key(1)).is_none(), "least recently used goes");
        assert!(buffer_cache_get("over", &key(0)).is_some());
    }

    /// Two copies of one font are one family whose faces are alike in every
    /// attribute; a [`face_family`] alias reaches each copy, and two systems
    /// loaded alike give the aliases the same IDs whatever order they shape in.
    #[test]
    fn a_face_alias_reaches_its_own_face_of_a_family_of_twins() {
        use cosmic_text::fontdb::{Database, Source};
        let src = {
            let fs = crate::geometry_font_system().lock().unwrap();
            let found = fs.db().faces().find_map(|f| match &f.source {
                Source::File(p) | Source::SharedFile(p, _) if f.index == 0 && f.families.len() == 1
                    && p.extension().is_some_and(|e| e == "ttf") => Some(p.clone()),
                _ => None,
            });
            found.expect("a .ttf in the font set")
        };
        let dir = std::env::temp_dir().join(format!("cce-ui-face-alias-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.ttf"), dir.join("b.ttf"));
        std::fs::copy(&src, &a).unwrap();
        std::fs::copy(&src, &b).unwrap();
        let system = || {
            let mut db = Database::new();
            db.load_font_file(&a).unwrap();
            db.load_font_file(&b).unwrap();
            FontSystem::new_with_locale_and_db(crate::locale::locale().into(), db)
        };
        let (mut one, mut two) = (system(), system());
        let (fam_a, fam_b) = (face_family(&a, 0), face_family(&b, 0));
        assert_eq!(face_family(&b, 0), fam_b, "one face, one alias");
        let shaped_from = |fs: &mut FontSystem, family: &str| {
            let buf = shared_text_buffer(fs, "Alias", 14.0, Some(family), TextAttrs::default());
            let id = buf.layout_runs().next().unwrap().glyphs[0].font_id;
            match &fs.db().face(id).unwrap().source {
                Source::File(p) | Source::SharedFile(p, _) => (id, p.clone()),
                _ => unreachable!(),
            }
        };
        // Opposite orders, through a cleared cache so each system shapes.
        BUFFER_CACHE.with(|c| c.borrow_mut().clear());
        let one_b = shaped_from(&mut one, &fam_b);
        let one_a = shaped_from(&mut one, &fam_a);
        BUFFER_CACHE.with(|c| c.borrow_mut().clear());
        let two_a = shaped_from(&mut two, &fam_a);
        let two_b = shaped_from(&mut two, &fam_b);
        assert_eq!((one_a.1.as_path(), one_b.1.as_path()), (a.as_path(), b.as_path()));
        assert_eq!((one_a.0, one_b.0), (two_a.0, two_b.0), "the same IDs in both systems");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A rescan rides the alias log: two databases loaded alike — one synced
    /// after every change, one only at the end — give the new file's face and
    /// the aliases either side of it the same IDs; a removed file's face goes
    /// from both while its alias stays; a database built after the rescan
    /// replays it as a no-op; and a bundled-only one takes no system file.
    /// (A log of the test's own: through the real one its rescans would
    /// reach every other test's databases.)
    #[test]
    fn a_rescan_keeps_databases_alike() {
        use cosmic_text::fontdb::{Database, Source};
        use std::collections::HashSet;
        use std::path::{Path, PathBuf};
        use std::sync::Arc;
        let src = {
            let fs = crate::geometry_font_system().lock().unwrap();
            let found = fs.db().faces().find_map(|f| match &f.source {
                Source::File(p) | Source::SharedFile(p, _) if f.index == 0 && p.extension().is_some_and(|e| e == "ttf") => Some(p.clone()),
                _ => None,
            });
            found.expect("a .ttf in the font set")
        };
        let dir = std::env::temp_dir().join(format!("cce-ui-rescan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (a, b) = (dir.join("a.ttf"), dir.join("b.ttf"));
        std::fs::copy(&src, &a).unwrap();
        std::fs::copy(&src, &b).unwrap();
        let system = |file: &Path| {
            let mut db = Database::new();
            db.load_font_file(file).unwrap();
            FontSystem::new_with_locale_and_db(crate::locale::locale().into(), db)
        };
        let faces = |fs: &FontSystem| {
            fs.db()
                .faces()
                .map(|f| {
                    let (Source::File(p) | Source::SharedFile(p, _)) = &f.source else { unreachable!() };
                    (f.id, f.families[0].0.clone(), p.clone())
                })
                .collect::<Vec<_>>()
        };
        // (is an alias, file) per face, in database order.
        let names = |fs: &FontSystem| faces(fs).into_iter().map(|(_, fam, p)| (fam.starts_with(FACE_ALIAS_PREFIX), p)).collect::<Vec<_>>();
        let alias = |path: &Path| FontOp::Alias(FaceAlias { path: path.to_path_buf(), index: 0 });
        let rescan = |added: Vec<PathBuf>, removed: &[&Path], bundled: &[&Path]| {
            let set = |ps: &[&Path]| ps.iter().map(|p| p.to_path_buf()).collect::<HashSet<_>>();
            FontOp::Rescan(Arc::new(Rescan::new(added, set(removed), set(bundled))))
        };

        let (mut eager, mut lazy) = (system(&a), system(&a));
        let mut ops = vec![alias(&a)];
        replay_font_ops(&mut eager, &ops);
        ops.push(rescan(vec![b.clone()], &[], &[]));
        replay_font_ops(&mut eager, &ops);
        ops.push(alias(&b));
        replay_font_ops(&mut eager, &ops);
        replay_font_ops(&mut lazy, &ops);
        assert_eq!(
            names(&eager),
            [(false, a.clone()), (true, a.clone()), (false, b.clone()), (true, b.clone())],
            "the new file lands between the aliases"
        );
        assert_eq!(faces(&eager), faces(&lazy), "the same IDs in both");

        ops.push(rescan(Vec::new(), &[&a], &[]));
        replay_font_ops(&mut eager, &ops);
        replay_font_ops(&mut lazy, &ops);
        assert_eq!(names(&eager), [(true, a.clone()), (false, b.clone()), (true, b.clone())], "the removed file's face goes, its alias stays");
        assert_eq!(faces(&eager), faces(&lazy), "the same IDs in both");

        let mut later = system(&b);
        replay_font_ops(&mut later, &ops);
        assert_eq!(names(&later), [(false, b.clone()), (true, b.clone())], "built after the rescan: nothing dropped or doubled");

        let mut bundled_only = system(&a);
        replay_font_ops(&mut bundled_only, &[rescan(vec![b.clone()], &[], &[&a])]);
        assert_eq!(names(&bundled_only), [(false, a.clone())], "a bundled-only database takes no system file");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod visual_order_tests {
    use super::*;

    #[test]
    fn runs_are_reordered_as_the_bidi_algorithm_draws_them() {
        assert_eq!(visual_run_order(&["say ", "שלום", " עולם", " now"], false), [0, 2, 1, 3]);
        assert_eq!(visual_run_order(&["שלום ", "bold", " and", " עולם"], true), [3, 1, 2, 0], "English keeps its order inside");
        assert_eq!(visual_run_order(&["a", "b", "c"], false), [0, 1, 2]);
        assert_eq!(visual_run_order(&["א", "ב"], true), [1, 0]);
        assert_eq!(visual_run_order(&[" ", "123"], true), [1, 0], "neutral runs sit at the paragraph's level");
        assert!(paragraph_rtl("  «שלום» hello") && !paragraph_rtl("hello שלום") && !paragraph_rtl("123 ..."));
    }
}
