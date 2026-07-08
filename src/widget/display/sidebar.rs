//! Narrow-trait sidebar strip (Phase 5i leaf sweep). Pure colored band; the legacy struct's raw
//! x/y/w/h fields now live on the `Adapted` base.

use crate::colors;
use crate::widget::{Adapted, Element, Input, Layout, Paint};

pub struct Sidebar;

impl Sidebar {
    pub fn new(w: f32) -> Adapted<Sidebar> {
        let mut s = Adapted::new(Sidebar);
        Element::set_rect(&mut s, 0.0, 0.0, w, 0.0);
        s
    }
}

impl Layout for Sidebar {}

impl Paint for Sidebar {
    fn color(&self) -> [f32; 4] {
        colors::sidebar_bg_color()
    }
}

impl Input for Sidebar {}
