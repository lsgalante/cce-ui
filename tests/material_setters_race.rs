//! The material setters change their cell under ONE write guard, merged onto the newest
//! style, so threads defining different materials, or binding different rungs, keep each
//! other's change. A read followed by a separate write put back whatever the read saw.
//!
//! An integration test on purpose: here cce-ui is built WITHOUT `cfg(test)`, so the setters
//! write the shared style, not the per-thread overlay the unit tests get. It is a stress test,
//! so on a broken setter it fails often, not always; a correct one passes every run.

use cce_ui::color;
use cce_ui::scene::{MaterialDef, PlateRung};

const ROUNDS: usize = 500;

#[test]
fn concurrent_material_setters_keep_each_others_change() {
    // The first read loads the colour config, which replaces both cells whole: get it done
    // before the race, not in the middle of it.
    color::material_names();

    let rungs = [PlateRung::Root, PlateRung::Control];
    let threads: Vec<_> = (0..rungs.len())
        .map(|t| {
            let rung = rungs[t];
            std::thread::spawn(move || {
                for i in 0..ROUNDS {
                    color::set_named_material(&format!("race-{t}-{i}"), Some(MaterialDef::default()));
                    color::set_material_binding(rung, Some(format!("race-{t}-{i}")));
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }

    let names = color::material_names();
    let missing = (0..rungs.len())
        .flat_map(|t| (0..ROUNDS).map(move |i| format!("race-{t}-{i}")))
        .filter(|n| !names.contains(n))
        .count();
    assert_eq!(missing, 0, "{missing} of {} materials lost", rungs.len() * ROUNDS);
    for (t, rung) in rungs.into_iter().enumerate() {
        let last = format!("race-{t}-{}", ROUNDS - 1);
        assert_eq!(color::material_binding(rung).as_deref(), Some(last.as_str()), "{rung:?}'s last binding lost");
    }
}
