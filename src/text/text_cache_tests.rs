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
