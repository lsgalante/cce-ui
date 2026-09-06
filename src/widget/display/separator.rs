//! Narrow-trait separator line (Phase 5c leaf sweep). The legacy struct carried its own public
//! x/y/w/h fields instead of a `Widget` base; the rect now lives on the [`Adapted`] base, so
//! callers position it via `set_rect`/`rect` (cce-status-interface's rotation loop was updated
//! accordingly).

use crate::widget::{Adapted, WidgetHost, Input, Layout, Paint};

#[derive(Debug, Clone)]
pub struct Separator {
    pub color: [f32; 4],
}

impl Separator {
    pub fn new(x: f32, y: f32, w: f32, h: f32, color: [f32; 4]) -> Adapted<Separator> {
        let mut sep = Adapted::new(Separator { color });
        WidgetHost::set_rect(&mut sep, x, y, w, h);
        sep
    }
}

impl Layout for Separator {}

impl Paint for Separator {
    fn color(&self) -> [f32; 4] {
        self.color
    }
}

impl Input for Separator {
    fn blocks_root_plate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructor_places_the_rect_and_bridge_emits_it() {
        let sep = Separator::new(100.0, 0.0, 1.0, 24.0, [0.3, 0.3, 0.3, 1.0]);
        assert_eq!(WidgetHost::rect(&sep), (100.0, 0.0, 1.0, 24.0));
        assert_eq!(WidgetHost::color(&sep), [0.3, 0.3, 0.3, 1.0]);
        assert_eq!(WidgetHost::extra_quads(&sep), vec![(100.0, 0.0, 1.0, 24.0, [0.3, 0.3, 0.3, 1.0])]);
        assert!(!WidgetHost::blocks_root_plate_drag(&sep));
    }

    /// The status bar's vertical-rotation pattern, post-migration: transpose via rect/set_rect.
    #[test]
    fn rotation_via_set_rect() {
        let mut sep = Separator::new(100.0, 0.0, 1.0, 24.0, [0.3, 0.3, 0.3, 1.0]);
        let (x, y, w, h) = WidgetHost::rect(&sep);
        WidgetHost::set_rect(&mut sep, y, x, h, w);
        assert_eq!(WidgetHost::rect(&sep), (0.0, 100.0, 24.0, 1.0));
    }
}
