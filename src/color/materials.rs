//! The named materials config defines and the rung bindings (`docs/rfc-material.md` § 5).

use super::*;
use crate::scene::material::MaterialDef;

/// The named materials config defines (`style.surface.material { <name> {…} }`)
/// and the rung bindings (`plate material=`, `plate { root material= }`,
/// `control material=`) — see `docs/rfc-material.md` § 5. Replaced wholesale
/// on every config load, so a node or binding removed from config is gone
/// after a reload.
pub(super) static NAMED_MATERIALS: crate::style::StyleCell<Vec<(String, MaterialDef)>> =
    crate::style::StyleCell::merging(|s| &s.color.NAMED_MATERIALS, |s| &mut s.color.NAMED_MATERIALS, |b, m, n| merge_materials(b, m, n));
pub(super) static MATERIAL_BINDINGS: crate::style::StyleCell<[Option<String>; 3]> =
    crate::style::StyleCell::merging(|s| &s.color.MATERIAL_BINDINGS, |s| &mut s.color.MATERIAL_BINDINGS, merge_bindings);

/// `NAMED_MATERIALS`' merge (`crate::style::StyleCell::merging`): a write's change, from `base`
/// to `mine`, landed on `newest` name by name, so two setters defining different materials
/// keep both. A name `mine` dropped is removed; one it added or redefined is put in.
fn merge_materials(base: &[(String, MaterialDef)], mine: Vec<(String, MaterialDef)>, newest: &mut Vec<(String, MaterialDef)>) {
    let entry = |list: &[(String, MaterialDef)], name: &str| list.iter().find(|(n, _)| n == name).map(|(_, d)| d.clone());
    for (name, _) in base {
        if entry(&mine, name).is_none() {
            put_material(newest, name, None);
        }
    }
    for (name, def) in mine {
        if entry(base, &name).as_ref() != Some(&def) {
            put_material(newest, &name, Some(def));
        }
    }
}

/// `MATERIAL_BINDINGS`' merge: a write's change landed rung by rung.
fn merge_bindings(base: &[Option<String>; 3], mine: [Option<String>; 3], newest: &mut [Option<String>; 3]) {
    for (i, binding) in mine.into_iter().enumerate() {
        if binding != base[i] {
            newest[i] = binding;
        }
    }
}

/// Define `name` as `def` in `list`, or remove it for `None`.
fn put_material(list: &mut Vec<(String, MaterialDef)>, name: &str, def: Option<MaterialDef>) {
    list.retain(|(n, _)| n != name);
    if let Some(d) = def {
        list.push((name.to_string(), d));
    }
}

fn rung_index(rung: crate::scene::material::PlateRung) -> usize {
    match rung {
        crate::scene::material::PlateRung::Root => 0,
        crate::scene::material::PlateRung::Pane => 1,
        crate::scene::material::PlateRung::Control => 2,
    }
}

pub fn named_material(name: &str) -> Option<MaterialDef> {
    load_colors_once();
    style_read(&NAMED_MATERIALS).iter().find(|(n, _)| n == name).map(|(_, d)| d.clone())
}

/// Every defined material's name, sorted.
pub fn material_names() -> Vec<String> {
    load_colors_once();
    let mut v: Vec<String> = style_read(&NAMED_MATERIALS).into_iter().map(|(n, _)| n).collect();
    v.sort();
    v
}

pub fn set_named_material(name: &str, def: Option<MaterialDef>) {
    style_update(&NAMED_MATERIALS, |list| put_material(list, name, def));
}

pub fn material_binding(rung: crate::scene::material::PlateRung) -> Option<String> {
    load_colors_once();
    style_read(&MATERIAL_BINDINGS)[rung_index(rung)].clone()
}

pub fn set_material_binding(rung: crate::scene::material::PlateRung, name: Option<String>) {
    style_update(&MATERIAL_BINDINGS, |b| b[rung_index(rung)] = name);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    fn def(light: f32) -> MaterialDef {
        MaterialDef { light: Some(light), ..MaterialDef::default() }
    }

    /// The merges land only what the write changed: names it added, redefined or dropped,
    /// and rungs it rebound; everything else stays as the newest has it.
    #[test]
    fn the_material_merges_land_only_what_the_write_changed() {
        let base = vec![("kept".to_string(), def(1.0)), ("dropped".to_string(), def(2.0)), ("redefined".to_string(), def(3.0))];
        let mut mine = base.clone();
        put_material(&mut mine, "dropped", None);
        put_material(&mut mine, "redefined", Some(def(4.0)));
        put_material(&mut mine, "added", Some(def(5.0)));
        let mut newest = base.clone();
        put_material(&mut newest, "kept", Some(def(6.0)));
        put_material(&mut newest, "theirs", Some(def(7.0)));
        merge_materials(&base, mine, &mut newest);
        let mut got: Vec<(String, Option<f32>)> = newest.into_iter().map(|(n, d)| (n, d.light)).collect();
        got.sort_by(|a, b| a.0.cmp(&b.0));
        let want = [("added", 5.0), ("kept", 6.0), ("redefined", 4.0), ("theirs", 7.0)];
        assert_eq!(got, want.map(|(n, l)| (n.to_string(), Some(l))));

        let base = [Some("a".to_string()), None, None];
        let mut newest = [Some("a".to_string()), None, Some("theirs".to_string())];
        merge_bindings(&base, [Some("a".to_string()), Some("mine".to_string()), None], &mut newest);
        assert_eq!(newest, [Some("a".to_string()), Some("mine".to_string()), Some("theirs".to_string())]);
    }

    /// Two writes held at once on the real cells keep each other's change: a material another
    /// thread defined, or a rung it bound, while this write's guard was held survives the drop.
    #[test]
    fn concurrent_material_writes_keep_each_others_change() {
        // Colour reloads replace both cells whole; keep them out of the window.
        let _lock = test_color_state_lock();
        let bindings_before = MATERIAL_BINDINGS.get();
        let (held, wait_held) = channel();
        let (published, wait_published) = channel::<()>();
        let writer = std::thread::spawn(move || {
            let mut list = NAMED_MATERIALS.write().unwrap();
            put_material(&mut list, "race-held", Some(def(1.0)));
            let mut bindings = MATERIAL_BINDINGS.write().unwrap();
            bindings[0] = Some("race-held".to_string());
            held.send(()).unwrap();
            wait_published.recv().unwrap();
        });
        wait_held.recv().unwrap();
        put_material(&mut NAMED_MATERIALS.write().unwrap(), "race-meanwhile", Some(def(2.0)));
        MATERIAL_BINDINGS.write().unwrap()[2] = Some("race-meanwhile".to_string());
        published.send(()).unwrap();
        writer.join().unwrap();

        let names: Vec<String> = NAMED_MATERIALS.get().into_iter().map(|(n, _)| n).collect();
        let bindings = MATERIAL_BINDINGS.get();
        // Leave the cells as found for the tests after this one.
        {
            let mut list = NAMED_MATERIALS.write().unwrap();
            put_material(&mut list, "race-held", None);
            put_material(&mut list, "race-meanwhile", None);
        }
        *MATERIAL_BINDINGS.write().unwrap() = bindings_before;
        assert!(names.contains(&"race-held".to_string()) && names.contains(&"race-meanwhile".to_string()), "{names:?}");
        assert_eq!((bindings[0].as_deref(), bindings[2].as_deref()), (Some("race-held"), Some("race-meanwhile")));
    }
}
