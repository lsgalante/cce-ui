//! Plate-grouping diagnostics: `CCE_PLATE_DEBUG`, and the debug build's loud warning for a
//! carve that falls back to overlay shading near its host plate's roll.

/// Prim discriminant name, for `CCE_PLATE_DEBUG` reporting only.
pub(super) fn prim_kind(p: &crate::scene::paint::Prim) -> &'static str {
    use crate::scene::paint::Prim as P;
    match p {
        P::Quad { .. } => "Quad", P::RoundedRect { .. } => "RoundedRect",
        P::Border { .. } => "Border", P::Bevel { .. } => "Bevel",
        P::Recess { .. } => "Recess", P::Boss { .. } => "Boss",
        P::Ridge { .. } => "Ridge", P::Trough { .. } => "Trough", P::Field { .. } => "Field", P::Plate { .. } => "Plate",
        P::Arc { .. } => "Arc", P::ArcShaded { .. } => "ArcShaded",
        P::Vector { .. } => "Vector", P::Circle { .. } => "Circle",
        P::Sphere { .. } => "Sphere", P::Droplet { .. } => "Droplet",
        P::DropletScrim { .. } => "DropletScrim", P::Frame { .. } => "Frame",
        P::ConcaveFillet { .. } => "ConcaveFillet",
        P::Groove { .. } => "Groove", P::Lattice { .. } => "Lattice", P::Grout { .. } => "Grout", P::Fill { .. } => "Fill",
        P::CarveUnion { .. } => "CarveUnion", P::Glow { .. } => "Glow",
        P::Text { .. } => "Text", P::Image { .. } => "Image",
    }
}

/// `CCE_PLATE_DEBUG=1` — trace which carves group into their host plate as exact
/// CSG features and which fall back to the standalone overlay shading.
///
/// The two paths do NOT look the same: a grouped carve is part of the plate's
/// single height field, so its wall meets the plate's rolled perimeter as a real
/// junction, while the fallback approximates that with the host-box fade. Six
/// conditions decide it, three of them dynamic (draw order, neighbouring plates,
/// whether another carve already claimed the host's feature run), so the SAME
/// widget can render either way depending on what is around it — and it does so
/// silently. That has already shipped as a bug once: a hovered button's opaque
/// fill used to sever every later button from the root plate they carve into,
/// which is why `plate_stack` is a stack (see its comment below).
///
/// Off by default and read once; the classification below runs only when set.
pub(super) fn plate_debug() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("CCE_PLATE_DEBUG").is_ok_and(|v| v != "0"))
}

/// Debug builds make one kind of fallback LOUD without `CCE_PLATE_DEBUG`: a
/// carve that could group (full ring, untinted) failing to while a still-open
/// plate encloses it and the carve's shaded region reaches that plate's
/// perimeter roll. There the grouped and overlay paths shade the junction
/// differently, and the rejection is one of the dynamic rules — so the SAME
/// widget can flip looks frame to frame with nothing on stderr. Not an
/// assert/panic: every rejection is conservative-CORRECT (the audit that
/// shipped CCE_PLATE_DEBUG found no misgrouping; a later plate overlapping the
/// carve genuinely must be shaded over, not under) — it is the frame-to-frame
/// LOOK that flips, so the right loudness is an unmissable warning, not a
/// crash. The ubiquitous quiet case stays quiet by construction: ordinary
/// geometry closing every grouping window empties `plate_stack`, so no
/// enclosing OPEN plate exists and this never runs — that is draw-order
/// design, not a flip.
///
/// Returns the dynamic rule to report, or `None` when the fallback is not the
/// loud case. Pure so the classification is unit-testable; `later_plates` are
/// the open plates emitted after the enclosing host.
#[cfg(debug_assertions)]
pub(super) fn near_roll_fallback_reason(
    carve: &crate::scene::layout::Rect,
    depth: f32,
    host: &crate::scene::layout::Rect,
    roll: f32,
    later_plates: &[crate::scene::layout::Rect],
    budget_full: bool,
) -> Option<&'static str> {
    // The carve's shaded region — the overlay path's cover-quad inflation.
    let infl = depth * 0.5 + 2.0;
    let (sx0, sy0) = (carve.x - infl, carve.y - infl);
    let (sx1, sy1) = (carve.x + carve.width + infl, carve.y + carve.height + infl);
    // "Near the roll" = the shaded region leaves the host rect deflated by the
    // host's own roll width on any side.
    let near = sx0 < host.x + roll
        || sy0 < host.y + roll
        || sx1 > host.x + host.width - roll
        || sy1 > host.y + host.height - roll;
    if !near {
        return None;
    }
    // The dynamic rules, in the order the grouping guard tests them.
    if budget_full {
        return Some("the feature budget is full");
    }
    if later_plates
        .iter()
        .any(|o| sx0 < o.x + o.width && sx1 > o.x && sy0 < o.y + o.height && sy1 > o.y)
    {
        return Some("a later plate overlaps the carve's shaded region");
    }
    Some("the host's feature run is closed (another plate appended features since)")
}

/// Print a near-roll fallback warning once per distinct message — a carve in a
/// steady layout would otherwise repeat it every frame.
#[cfg(debug_assertions)]
pub(super) fn plate_carve_warn_once(msg: String) {
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<std::collections::HashSet<String>>> = OnceLock::new();
    let seen = SEEN.get_or_init(|| Mutex::new(std::collections::HashSet::new()));
    if seen.lock().unwrap().insert(msg.clone()) {
        eprintln!("{msg}");
    }
}

// `all(test, debug_assertions)`: the function under test only exists in
// debug builds, so a `cargo test --release` must compile the module out too.
#[cfg(all(test, debug_assertions))]
mod near_roll_fallback_tests {
    use super::near_roll_fallback_reason;
    use crate::scene::layout::Rect;

    fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, width: w, height: h }
    }

    const HOST: Rect = Rect { x: 0.0, y: 0.0, width: 800.0, height: 600.0 };
    const ROLL: f32 = 8.0;

    #[test]
    fn interior_carve_is_quiet() {
        // Well inside the deflated host: the overlay fallback is exact there.
        let carve = r(100.0, 100.0, 200.0, 100.0);
        assert_eq!(near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[], false), None);
    }

    #[test]
    fn shaded_region_reaching_the_roll_is_loud() {
        // Carve rect stops 3px short of the roll band, but its shaded region
        // (depth*0.5 + 2 = 5px) crosses in — the inflation must count.
        let carve = r(ROLL + 3.0, 100.0, 200.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[], false),
            Some("the host's feature run is closed (another plate appended features since)")
        );
    }

    #[test]
    fn occlusion_is_named_before_run_contiguity() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let occluder = r(150.0, 150.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[occluder], false),
            Some("a later plate overlaps the carve's shaded region")
        );
    }

    #[test]
    fn non_overlapping_later_plate_is_not_occlusion() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let elsewhere = r(500.0, 400.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[elsewhere], false),
            Some("the host's feature run is closed (another plate appended features since)")
        );
    }

    #[test]
    fn budget_wins_over_every_other_reason() {
        let carve = r(2.0, 100.0, 200.0, 100.0);
        let occluder = r(150.0, 150.0, 100.0, 100.0);
        assert_eq!(
            near_roll_fallback_reason(&carve, 6.0, &HOST, ROLL, &[occluder], true),
            Some("the feature budget is full")
        );
    }
}

/// One frame's `CCE_PLATE_DEBUG` bookkeeping (see [`plate_debug`]).
#[derive(Default)]
pub(super) struct PlateDbg {
    pub(super) on: bool,
    pub(super) grouped: usize,
    pub(super) fell_back: Vec<String>,
    pub(super) opened: usize,
    /// Which prim kind closed a still-open grouping window, and how many plates it closed —
    /// the answer to "why was there no enclosing plate?".
    pub(super) closed_by: std::collections::BTreeMap<&'static str, usize>,
}

impl PlateDbg {
    /// The frame's verdicts, on stderr.
    pub(super) fn report(&self) {
        if self.on && (self.grouped > 0 || !self.fell_back.is_empty()) {
            eprintln!(
                "plate-dbg: {} carves — {} grouped (exact CSG), {} overlay fallback",
                self.grouped + self.fell_back.len(),
                self.grouped,
                self.fell_back.len(),
            );
            eprintln!(
                "plate-dbg:   {} grouping window(s) opened by a filled plate; closed early by {}",
                self.opened,
                if self.closed_by.is_empty() {
                    "nothing".to_string()
                } else {
                    self.closed_by
                        .iter()
                        .map(|(k, n)| format!("{k}x{n}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            );
            for line in &self.fell_back {
                eprintln!("plate-dbg: {line}");
            }
        }
    }
}

