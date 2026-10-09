//! `SpatialGrid`: widget ids bucketed by grid cell, so a hit test asks only the widgets near a
//! point.

use super::*;

pub struct SpatialGrid {
    pub cell_size: f32,
    pub cells: HashMap<(i32, i32), Vec<WidgetId>>,
}

impl SpatialGrid {
    pub fn new(cell_size: f32) -> Self {
        Self {
            cell_size,
            cells: HashMap::new(),
        }
    }

    pub fn clear(&mut self) {
        self.cells.clear();
    }

    pub fn insert(&mut self, id: WidgetId, rect: (f32, f32, f32, f32)) {
        let (x, y, w, h) = rect;
        if w <= 0.0 || h <= 0.0 {
            return;
        }
        let start_x = (x / self.cell_size).floor() as i32;
        let end_x = ((x + w) / self.cell_size).floor() as i32;
        let start_y = (y / self.cell_size).floor() as i32;
        let end_y = ((y + h) / self.cell_size).floor() as i32;

        let start_x = start_x.max(-1000);
        let end_x = end_x.min(1000);
        let start_y = start_y.max(-1000);
        let end_y = end_y.min(1000);

        for cx in start_x..=end_x {
            for cy in start_y..=end_y {
                self.cells.entry((cx, cy)).or_default().push(id);
            }
        }
    }

    pub fn query(&self, px: f32, py: f32) -> &[WidgetId] {
        let cx = (px / self.cell_size).floor() as i32;
        let cy = (py / self.cell_size).floor() as i32;
        self.cells.get(&(cx, cy)).map(|v| v.as_slice()).unwrap_or(&[])
    }
}
