//! Narrow-trait `ContentBg` (Phase 5n) — a standalone gradient-grid page background. Despite
//! the name it owns no children; its GraphController impl is a stub except the grid-geometry
//! setters (hosts configure the grid through the controller interface). Never hittable; the
//! background color itself is drawn by hosts reading `color()` (the geometry here is only the
//! cell/gap gradient grid, exactly the legacy `extra_quads`).

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, GraphController, GraphNode, Input, Layout, Paint};

pub struct ContentBg {
    show_network_grid: bool,
    grid_size_x: f32,
    grid_size_y: f32,
    grid_origin_x: f32,
    grid_origin_y: f32,
    skipped_row_h: f32,
    skipped_col_w: f32,
}

impl ContentBg {
    pub fn new() -> Adapted<ContentBg> {
        Adapted::new(ContentBg { show_network_grid: false, grid_size_x: 150.0, grid_size_y: 75.0, grid_origin_x: 0.0, grid_origin_y: 0.0, skipped_row_h: 37.5, skipped_col_w: 37.5 })
    }
}

impl Layout for ContentBg {}

impl Input for ContentBg {
    /// Never hittable (legacy hit_test returned false unconditionally).
    fn hit(&self, _rect: Rect, _x: f32, _y: f32) -> bool {
        false
    }

}

impl Paint for ContentBg {
    fn color(&self) -> [f32; 4] {
        if self.show_network_grid {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            colors::CONTENT_BG
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        for (qx, qy, qw, qh, qc) in self.grid_quads(rect) {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
    }
}

impl ContentBg {
    /// The gradient grid geometry (legacy `extra_quads`), against `rect`.
    fn grid_quads(&self, rect: Rect) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        if !self.show_network_grid || self.grid_size_x <= 0.0 || self.grid_size_y <= 0.0 {
            return vec![];
        }
        let mut quads = Vec::new();
        let grid_color = [0.0, 0.0, 0.0, 0.0];
        let max_alpha = colors::CONTENT_BG[3]; // Peak opacity in the middle of gradient cells matches non-gradient cells
        let steps = 20; // Silky-smooth gradient transition

        let step_y = self.grid_size_y + self.skipped_row_h;
        let step_x = self.grid_size_x + self.skipped_col_w;

        if step_y >= 4.0 && step_x >= 4.0 {
            let ry_start = ((y - self.grid_origin_y) / step_y).floor() as i32 - 1;
            let ry_end = ((y + h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
            let ry_start = ry_start.max(-100_000);
            let ry_end = ry_end.min(100_000);

            let cx_start = ((x - self.grid_origin_x) / step_x).floor() as i32 - 1;
            let cx_end = ((x + w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
            let cx_start = cx_start.max(-100_000);
            let cx_end = cx_end.min(100_000);

            // Draw individual cell backgrounds to avoid stacking with gradients
            for ry in ry_start..=ry_end {
                let y1 = self.grid_origin_y + (ry as f32) * step_y;
                let draw_start_y = y1.max(y);
                let draw_end_y = (y1 + self.grid_size_y).min(y + h);
                if draw_start_y < draw_end_y {
                    for cx in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (cx as f32) * step_x;
                        let draw_start_x = x1.max(x);
                        let draw_end_x = (x1 + self.grid_size_x).min(x + w);
                        if draw_start_x < draw_end_x {
                            quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, colors::CONTENT_BG));
                        }
                    }
                }
            }
        }

        // Draw interstitial row gradients (horizontal bands fading to 0 alpha at left and right sides)
        if self.skipped_row_h > 0.0 {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;
            if step_y >= 4.0 && step_x >= 4.0 {
                let k_start = ((y - self.grid_origin_y) / step_y).floor() as i32 - 1;
                let k_end = ((y + h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
                let k_start = k_start.max(-100_000);
                let k_end = k_end.min(100_000);

                let cx_start = ((x - self.grid_origin_x) / step_x).floor() as i32 - 1;
                let cx_end = ((x + w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
                let cx_start = cx_start.max(-100_000);
                let cx_end = cx_end.min(100_000);

                for k in k_start..=k_end {
                    let y1 = self.grid_origin_y + (k as f32) * step_y;
                    let y2 = y1 + self.grid_size_y;
                    if y1 >= y + h {
                        continue;
                    }
                    let draw_start_y = y2.max(y);
                    let draw_end_y = (y2 + self.skipped_row_h).min(y + h);
                    if draw_start_y >= draw_end_y {
                        continue;
                    }

                    for cx in cx_start..=cx_end {
                        let x1 = self.grid_origin_x + (cx as f32) * step_x;
                        let x_mid = x1 + self.grid_size_x / 2.0;
                        let w_total = self.grid_size_x;
                        let sub_w = w_total / steps as f32;

                        for i in 0..steps {
                            let sx_start = x1 + i as f32 * sub_w;
                            let sx_end = sx_start + sub_w;
                            let draw_start_x = sx_start.max(x);
                            let draw_end_x = sx_end.min(x + w);
                            if draw_start_x < draw_end_x {
                                let sx_mid = (sx_start + sx_end) / 2.0;
                                let dist = (sx_mid - x_mid).abs();
                                let d = (dist / (w_total / 2.0)).min(1.0);
                                
                                // Fade the cell background color from max_alpha in the middle to transparent at the edges
                                let alpha = max_alpha * (1.0 - d);
                                if alpha > 0.001 {
                                    quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, [colors::CONTENT_BG[0], colors::CONTENT_BG[1], colors::CONTENT_BG[2], alpha]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw interstitial column gradients (vertical bands fading to 0 alpha at top and bottom)
        if self.skipped_col_w > 0.0 {
            let step_y = self.grid_size_y + self.skipped_row_h;
            let step_x = self.grid_size_x + self.skipped_col_w;
            if step_y >= 4.0 && step_x >= 4.0 {
                let k_start = ((x - self.grid_origin_x) / step_x).floor() as i32 - 1;
                let k_end = ((x + w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
                let k_start = k_start.max(-100_000);
                let k_end = k_end.min(100_000);

                let ry_start = ((y - self.grid_origin_y) / step_y).floor() as i32 - 1;
                let ry_end = ((y + h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
                let ry_start = ry_start.max(-100_000);
                let ry_end = ry_end.min(100_000);

                for k in k_start..=k_end {
                    let x1 = self.grid_origin_x + (k as f32) * step_x;
                    let x2 = x1 + self.grid_size_x;
                    if x1 >= x + w {
                        continue;
                    }
                    let draw_start_x = x2.max(x);
                    let draw_end_x = (x2 + self.skipped_col_w).min(x + w);
                    if draw_start_x >= draw_end_x {
                        continue;
                    }

                    for ry in ry_start..=ry_end {
                        let y1 = self.grid_origin_y + (ry as f32) * step_y;
                        let y_mid = y1 + self.grid_size_y / 2.0;
                        let h_total = self.grid_size_y;
                        let sub_h = h_total / steps as f32;

                        for i in 0..steps {
                            let sy_start = y1 + i as f32 * sub_h;
                            let sy_end = sy_start + sub_h;
                            let draw_start_y = sy_start.max(y);
                            let draw_end_y = sy_end.min(y + h);
                            if draw_start_y < draw_end_y {
                                let sy_mid = (sy_start + sy_end) / 2.0;
                                let dist = (sy_mid - y_mid).abs();
                                let d = (dist / (h_total / 2.0)).min(1.0);
                                
                                // Fade the cell background color from max_alpha in the middle to transparent at the edges
                                let alpha = max_alpha * (1.0 - d);
                                if alpha > 0.001 {
                                    quads.push((draw_start_x, draw_start_y, draw_end_x - draw_start_x, draw_end_y - draw_start_y, [colors::CONTENT_BG[0], colors::CONTENT_BG[1], colors::CONTENT_BG[2], alpha]));
                                }
                            }
                        }
                    }
                }
            }
        }

        // Draw the grid borders
        let step_y = self.grid_size_y + self.skipped_row_h;
        if step_y >= 4.0 {
            let k_start = ((y - self.grid_origin_y) / step_y).floor() as i32 - 1;
            let k_end = ((y + h - self.grid_origin_y) / step_y).ceil() as i32 + 1;
            let k_start = k_start.max(-100_000);
            let k_end = k_end.min(100_000);
            for k in k_start..=k_end {
                let y1 = self.grid_origin_y + (k as f32) * step_y;
                let y2 = y1 + self.grid_size_y;
                if y1 >= y + h {
                    continue;
                }
                if y1 >= y {
                    quads.push((x, y1, w, 1.0, grid_color));
                }
                if y2 >= y && y2 < y + h {
                    quads.push((x, y2, w, 1.0, grid_color));
                }
            }
        }

        let step_x = self.grid_size_x + self.skipped_col_w;
        if step_x >= 4.0 {
            let k_start = ((x - self.grid_origin_x) / step_x).floor() as i32 - 1;
            let k_end = ((x + w - self.grid_origin_x) / step_x).ceil() as i32 + 1;
            let k_start = k_start.max(-100_000);
            let k_end = k_end.min(100_000);
            for k in k_start..=k_end {
                let x1 = self.grid_origin_x + (k as f32) * step_x;
                let x2 = x1 + self.grid_size_x;
                if x1 >= x + w {
                    continue;
                }
                if x1 >= x {
                    quads.push((x1, y, 1.0, h, grid_color));
                }
                if x2 >= x && x2 < x + w {
                    quads.push((x2, y, 1.0, h, grid_color));
                }
            }
        }
        quads
    }
}

impl GraphController for ContentBg {
    fn set_nodes(&mut self, _nodes: &[GraphNode]) {}
    fn get_nodes(&self) -> Vec<GraphNode> { Vec::new() }
    fn selected_node(&self) -> Option<usize> { None }
    fn set_selected_node(&mut self, _idx: Option<usize>) {}
    fn double_clicked_node(&self) -> Option<usize> { None }
    fn clear_double_clicked_node(&mut self) {}
    fn set_grid_snap_enabled(&mut self, _enabled: bool) {}
    fn take_node_geom_toggle(&mut self) -> Option<(usize, bool)> { None }
    fn set_grid_snap(&mut self, _gx: f32, _gy: f32) {}
    fn set_grid_sizes(&mut self, gx: f32, gy: f32) { self.grid_size_x = gx; self.grid_size_y = gy; }
    fn set_skipped_sizes(&mut self, row_h: f32, col_w: f32) { self.skipped_row_h = row_h; self.skipped_col_w = col_w; }
    fn set_grid_origin(&mut self, ox: f32, oy: f32) { self.grid_origin_x = ox; self.grid_origin_y = oy; }
    fn grid_origin(&self) -> (f32, f32) { (self.grid_origin_x, self.grid_origin_y) }
    fn set_show_network_grid(&mut self, show: bool) { self.show_network_grid = show; }
    fn take_pending_connection(&mut self) -> Option<(String, String)> { None }
    fn cancel_connecting(&mut self) {}
    fn is_node_rect(&self, _qx: f32, _qy: f32, _qw: f32, _qh: f32) -> bool { false }
}
