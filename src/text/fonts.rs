//! Fonts for shaping: face aliases (a family of one face), rescans and the font-op log every
//! `FontSystem` replays so their face ids agree, family resolution, and snapping attrs to a face the
//! family has.

use super::*;

/// The family-name prefix of a [`face_family`] alias.
pub(super) const FACE_ALIAS_PREFIX: &str = "cce-face:";

/// One [`face_family`] alias: the face at `path`#`index`, named `cce-face:<n>`
/// for its place `n` in [`FONT_OPS`].
pub(super) struct FaceAlias {
    pub(super) path: std::path::PathBuf,
    pub(super) index: u32,
}

/// A font-directory rescan ([`crate::rescan_fonts`]): what changed on disk,
/// as fixed lists rather than a scan each database makes for itself, so every
/// database applies the same change however long after the others. A change,
/// not the whole set: a database built after the rescan already has it, and
/// replaying it there is a no-op rather than a drop of whatever arrived since.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))] // only `rescan_fonts` makes one, and the browser has no font dirs
pub(crate) struct Rescan {
    /// Font files new on disk, in a system-fonts build's load order.
    pub(super) added: Vec<std::path::PathBuf>,
    /// Font files gone from disk.
    pub(super) removed: std::collections::HashSet<std::path::PathBuf>,
    /// The files a bundled-only build loads ([`crate::create_font_system`]):
    /// the CCE fonts dir and the fallback faces.
    pub(super) bundled: std::collections::HashSet<std::path::PathBuf>,
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
pub(super) enum FontOp {
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
pub(super) static FONT_OPS: std::sync::Mutex<Vec<FontOp>> = std::sync::Mutex::new(Vec::new());

thread_local! {
    /// Database address → how many of [`FONT_OPS`] it has applied, and the ID
    /// and op number of the last alias it pushed — checked before it is
    /// trusted, since a dropped `FontSystem`'s address can be reused by a
    /// fresh one.
    pub(super) static FONT_OPS_SYNCED: std::cell::RefCell<
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
pub(super) fn sync_font_ops(fs: &mut FontSystem) {
    let ops = FONT_OPS.lock().unwrap_or_else(|e| e.into_inner());
    replay_font_ops(fs, &ops);
}

/// [`sync_font_ops`] against a given log (the tests keep their own, so their
/// rescans reach no other test's databases).
pub(super) fn replay_font_ops(fs: &mut FontSystem, ops: &[FontOp]) {
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
pub(super) fn apply_rescan(fs: &mut FontSystem, rescan: &Rescan) {
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

pub(super) fn find_cased_family(fs: &FontSystem, name: &str) -> Option<String> {
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
    pub(super) static MONO_FAMILY_CACHE: std::cell::RefCell<std::collections::HashMap<String, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

pub(super) fn family_is_monospaced(fs: &FontSystem, name: &str) -> bool {
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
    pub(super) static FAMILY_FACES_CACHE: std::cell::RefCell<
        std::collections::HashMap<String, Vec<(cosmic_text::Style, cosmic_text::Stretch, u16)>>,
    > = std::cell::RefCell::new(std::collections::HashMap::new());
}

/// The `cosmic_text::Stretch` for an OpenType width class (1–9, clamped; see
/// [`TextAttrs::stretch`](crate::scene::paint::TextAttrs::stretch)).
pub(super) fn stretch_from_width_class(class: u16) -> cosmic_text::Stretch {
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
pub(super) fn snap_to_family_face<'a>(fs: &FontSystem, attrs: Attrs<'a>) -> Attrs<'a> {
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
