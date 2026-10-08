//! The named materials config defines and the rung bindings (`docs/rfc-material.md` § 5).

use super::*;

/// The named materials config defines (`style.surface.material { <name> {…} }`)
/// and the rung bindings (`plate material=`, `plate { root material= }`,
/// `control material=`) — see `docs/rfc-material.md` § 5. Replaced wholesale
/// on every config load, so a node or binding removed from config is gone
/// after a reload.
pub(super) static NAMED_MATERIALS: crate::style::StyleCell<Vec<(String, crate::scene::material::MaterialDef)>> = crate::style::StyleCell::new(|s| &s.color.NAMED_MATERIALS, |s| &mut s.color.NAMED_MATERIALS);
pub(super) static MATERIAL_BINDINGS: crate::style::StyleCell<[Option<String>; 3]> = crate::style::StyleCell::new(|s| &s.color.MATERIAL_BINDINGS, |s| &mut s.color.MATERIAL_BINDINGS);

fn rung_index(rung: crate::scene::material::PlateRung) -> usize {
    match rung {
        crate::scene::material::PlateRung::Root => 0,
        crate::scene::material::PlateRung::Pane => 1,
        crate::scene::material::PlateRung::Control => 2,
    }
}

pub fn named_material(name: &str) -> Option<crate::scene::material::MaterialDef> {
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

pub fn set_named_material(name: &str, def: Option<crate::scene::material::MaterialDef>) {
    let mut list = style_read(&NAMED_MATERIALS);
    list.retain(|(n, _)| n != name);
    if let Some(d) = def {
        list.push((name.to_string(), d));
    }
    style_write(&NAMED_MATERIALS, list);
}

pub fn material_binding(rung: crate::scene::material::PlateRung) -> Option<String> {
    load_colors_once();
    style_read(&MATERIAL_BINDINGS)[rung_index(rung)].clone()
}

pub fn set_material_binding(rung: crate::scene::material::PlateRung, name: Option<String>) {
    let mut b = style_read(&MATERIAL_BINDINGS);
    b[rung_index(rung)] = name;
    style_write(&MATERIAL_BINDINGS, b);
}
