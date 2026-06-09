use crate::colors;
use crate::widget::*;
use crate::widget::display::TextLabel;

pub struct Spreadsheet {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hovered: bool,
    visible: bool,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    scroll_y: f32,
    scroll_velocity: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scrollbar_hovered: bool,
    scrollbar_thumb_hovered: bool,
}

impl Spreadsheet {
    pub fn new() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            visible: false,
            headers: Vec::new(),
            rows: Vec::new(),
            scroll_y: 0.0,
            scroll_velocity: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scrollbar_hovered: false,
            scrollbar_thumb_hovered: false,
        }
    }
}

impl Element for Spreadsheet {
    fn rect(&self) -> (f32, f32, f32, f32) {
        if !self.visible {
            (0.0, 0.0, 0.0, 0.0)
        } else {
            (self.x, self.y, self.w, self.h)
        }
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }

    fn color(&self) -> [f32; 4] {
        if !self.visible {
            [0.0, 0.0, 0.0, 0.0]
        } else {
            colors::PARAM_BG
        }
    }

    fn set_hovered(&mut self, v: bool) {
        self.hovered = v;
    }

    fn hovered(&self) -> bool {
        self.hovered
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if crate::widget::popovers::is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        let (rx, ry, rw, rh) = self.rect();
        px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        self.headers = headers;
        self.rows = rows;
        
        // Clamp scroll_y to new bounds
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        let max_scroll_y = (content_h - visible_h).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll_y);
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let was_hovered = self.hovered;
        self.hovered = self.hit_test(px, py, ctx);

        let was_sb_hovered = self.scrollbar_hovered;
        let was_thumb_hovered = self.scrollbar_thumb_hovered;

        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;

            self.scrollbar_hovered = px >= scrollbar_x - 2.0 && px <= self.x + self.w
                && py >= track_y && py <= self.y + self.h;

            let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
            let max_scroll_y = content_h - visible_h;
            let scroll_ratio = self.scroll_y / max_scroll_y;
            let track_scroll_range = visible_h - thumb_h;
            let thumb_y = track_y + scroll_ratio * track_scroll_range;

            self.scrollbar_thumb_hovered = px >= scrollbar_x - 2.0 && px <= self.x + self.w
                && py >= thumb_y && py <= thumb_y + thumb_h;
        } else {
            self.scrollbar_hovered = false;
            self.scrollbar_thumb_hovered = false;
        }

        was_hovered != self.hovered
            || was_sb_hovered != self.scrollbar_hovered
            || was_thumb_hovered != self.scrollbar_thumb_hovered
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if !self.visible {
            return false;
        }
        if self.hit_test(px, py, ctx) {
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            if visible_h > 0.0 && content_h > visible_h {
                let scroll_amount = match delta {
                    MouseScrollDelta::LineDelta(_x, y) => *y * 24.0,
                    MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };
                self.scroll_velocity += scroll_amount * 12.0;
                return true;
            }
        }
        false
    }

    fn draggable(&self) -> bool {
        if !self.visible {
            return false;
        }
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        visible_h > 0.0 && content_h > visible_h
    }

    fn is_dragging(&self) -> bool {
        self.dragging_scrollbar
    }

    fn drag_begin(&mut self, px: f32, py: f32) {
        self.scroll_velocity = 0.0;
        let content_h = self.rows.len() as f32 * 24.0;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;

            if px >= scrollbar_x - 4.0 && px <= self.x + self.w
                && py >= track_y && py <= self.y + self.h
            {
                self.dragging_scrollbar = true;

                let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
                let max_scroll_y = content_h - visible_h;
                let scroll_ratio = self.scroll_y / max_scroll_y;
                let track_scroll_range = visible_h - thumb_h;
                let thumb_y = track_y + scroll_ratio * track_scroll_range;

                if py >= thumb_y && py <= thumb_y + thumb_h {
                    self.drag_offset_y = py - thumb_y;
                } else {
                    self.drag_offset_y = thumb_h / 2.0;
                    let new_thumb_y = py - self.drag_offset_y;
                    let scroll_ratio = if track_scroll_range > 0.0 {
                        ((new_thumb_y - track_y) / track_scroll_range).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    self.scroll_y = scroll_ratio * max_scroll_y;
                }
            }
        }
    }

    fn drag_update(&mut self, _px: f32, py: f32) -> bool {
        if self.dragging_scrollbar {
            self.scroll_velocity = 0.0;
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            if visible_h > 0.0 && content_h > visible_h {
                let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
                let max_scroll_y = content_h - visible_h;
                let track_y = self.y + 24.0;
                let track_scroll_range = visible_h - thumb_h;

                let new_thumb_y = py - self.drag_offset_y;
                let scroll_ratio = if track_scroll_range > 0.0 {
                    ((new_thumb_y - track_y) / track_scroll_range).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let old_scroll_y = self.scroll_y;
                self.scroll_y = scroll_ratio * max_scroll_y;

                return (self.scroll_y - old_scroll_y).abs() > 0.01;
            }
        }
        false
    }

    fn drag_end(&mut self) {
        self.dragging_scrollbar = false;
        self.scroll_velocity = 0.0;
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        if self.scroll_velocity.abs() > 0.01 {
            let content_h = self.rows.len() as f32 * 24.0;
            let visible_h = (self.h - 24.0).max(0.0);
            let max_scroll_y = (content_h - visible_h).max(0.0);
            let old_scroll_y = self.scroll_y;

            self.scroll_y = (self.scroll_y + self.scroll_velocity * dt).clamp(0.0, max_scroll_y);

            // Decelerate with friction (exponential decay)
            let friction = 8.0;
            self.scroll_velocity *= (-friction * dt).exp();

            if self.scroll_y == 0.0 || self.scroll_y == max_scroll_y {
                self.scroll_velocity = 0.0;
            }

            if self.scroll_velocity.abs() < 5.0 {
                self.scroll_velocity = 0.0;
            }

            (self.scroll_y - old_scroll_y).abs() > 0.01
        } else {
            false
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        
        // Header bg
        quads.push((self.x, self.y, self.w, 24.0, [0.12, 0.12, 0.16, 0.4]));

        // Zebra rows
        let row_h = 24.0;
        let body_top = self.y + 24.0;
        let body_bottom = self.y + self.h;
        for i in 0..self.rows.len() {
            let ry = self.y + 24.0 + i as f32 * row_h - self.scroll_y;
            if ry + row_h <= body_top || ry >= body_bottom {
                continue;
            }
            let draw_y = ry.max(body_top);
            let draw_h = (ry + row_h).min(body_bottom) - draw_y;
            if draw_h > 0.0 {
                let row_color = if i % 2 == 0 {
                    [0.10, 0.10, 0.13, 0.15]
                } else {
                    [0.08, 0.08, 0.11, 0.05]
                };
                quads.push((self.x, draw_y, self.w, draw_h, row_color));

                // Horizontal row separator
                let sep_y = ry + row_h;
                if sep_y >= body_top && sep_y < body_bottom {
                    quads.push((self.x, sep_y, self.w, 1.0, [0.20, 0.20, 0.25, 0.15]));
                }
            }
        }

        // Header separator
        quads.push((self.x, self.y + 24.0, self.w, 1.0, [0.20, 0.20, 0.25, 0.25]));

        // Vertical separators
        let divider_h = self.h;
        if divider_h > 0.0 && !self.headers.is_empty() {
            let n_cols = self.headers.len();
            for i in 1..n_cols {
                let r = i as f32 / n_cols as f32;
                quads.push((self.x + self.w * r, self.y, 1.0, divider_h, [0.20, 0.20, 0.25, 0.15]));
            }
        }

        // Scrollbar track & thumb
        let content_h = self.rows.len() as f32 * row_h;
        let visible_h = (self.h - 24.0).max(0.0);
        if visible_h > 0.0 && content_h > visible_h {
            let scrollbar_w = 6.0;
            let scrollbar_padding = 2.0;
            let scrollbar_x = self.x + self.w - scrollbar_w - scrollbar_padding;
            let track_y = self.y + 24.0;
            let track_h = visible_h;

            // Track BG
            quads.push((scrollbar_x, track_y, scrollbar_w, track_h, [0.05, 0.05, 0.08, 0.15]));

            // Thumb
            let thumb_h = ((visible_h / content_h) * visible_h).clamp(15.0_f32.min(visible_h), visible_h);
            let max_scroll_y = content_h - visible_h;
            let scroll_ratio = self.scroll_y / max_scroll_y;
            let track_scroll_range = visible_h - thumb_h;
            let thumb_y = track_y + scroll_ratio * track_scroll_range;

            let thumb_color = if self.dragging_scrollbar {
                [0.40, 0.40, 0.48, 1.0]
            } else if self.scrollbar_thumb_hovered {
                [0.32, 0.32, 0.38, 1.0]
            } else if self.scrollbar_hovered {
                [0.24, 0.24, 0.30, 0.9]
            } else {
                [0.18, 0.18, 0.24, 0.7]
            };

            quads.push((scrollbar_x, thumb_y, scrollbar_w, thumb_h, thumb_color));
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        if !self.visible {
            return Vec::new();
        }
        let mut labels = Vec::new();
        if self.headers.is_empty() {
            return labels;
        }

        let n_cols = self.headers.len();
        for (i, header) in self.headers.iter().enumerate() {
            let cx = self.x + self.w * (i as f32 / n_cols as f32) + 8.0;
            labels.push(TextLabel {
                text: header.clone(),
                x: cx,
                y: self.y + 6.0,
                font_size: 12.0,
                color: [0xdd, 0xdd, 0xee],
            });
        }

        let row_h = 24.0;
        let body_top = self.y + 24.0;
        let body_bottom = self.y + self.h;
        for (i, row) in self.rows.iter().enumerate() {
            let ry = self.y + 24.0 + i as f32 * row_h - self.scroll_y;
            // Only show text if the row is fully inside the spreadsheet body
            if ry < body_top || ry + row_h > body_bottom {
                continue;
            }

            for (col_idx, val) in row.iter().enumerate().take(n_cols) {
                let cx = self.x + self.w * (col_idx as f32 / n_cols as f32) + 8.0;
                labels.push(TextLabel {
                    text: val.clone(),
                    x: cx,
                    y: ry + 6.0,
                    font_size: 12.0,
                    color: [0xbb, 0xbb, 0xcc],
                });
            }
        }
        labels
    }
}
