//! The legacy layout engines — `Column`, `Row`, `Section`, `Grid`, the `LayoutStrategy`
//! family (`FlexLayout`, `ColumnLayout`, `AdaptiveGrid`, `RadialLayout`), `PageLayoutBuilder`
//! and `VStack` — from before `scene::layout`'s box model. Still used by
//! cce-system-interface, cce-files and cce-gallery; new layout goes through
//! `scene::layout`.

use super::*;
use crate::widget::WidgetHost;
use crate::context::UiContext;

pub struct UiFrame;

impl UiFrame {
    pub fn start(scroll_offset: f32) -> Self {
        crate::widget::hover_animation::reset_frame_registration();
        crate::widget::hover_animation::set_scroll_offset(scroll_offset);
        Self
    }

    pub fn finish(self, pc: &mut dyn RenderTarget) {
        crate::widget::hover_animation::post_render_check();
        if let Some((qx, qy, qw, qh, qc)) = crate::widget::hover_animation::get_quad() {
            pc.rect(qc, qx, qy, qw, qh);
        }
        // render_popovers(pc);
    }
}

pub struct Column {
    ox: f32,
    oy: f32,
    cx: f32,
    pub y: f32,
    pub cw: f32,
}

impl Column {
    pub fn new(ox: f32, oy: f32, cx: f32, cy: f32, cw: f32) -> Self {
        Self { ox, oy, cx, y: cy, cw }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        self.ox + self.cx + x_off
    }

    pub fn ay(&self) -> f32 {
        self.oy + self.y
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.y += h;
    }

    pub fn advance(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn spacing(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let x = self.ax(8.0);
        let y = self.ay();
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 16.0, 1.0);
        self.y += 8.0;
    }

    pub fn header(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32) {
        let x = self.ax(x_off);
        let y = self.ay();
        pc.text(text, x, y, 14.0, [0.83, 0.83, 0.83, 1.0]);
        self.y += 22.0;
    }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let x = self.ax(x_off);
        let y = self.ay() + y_off;
        pc.text(text, x, y, font_size, color);
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let top_room = w.label_strip();
        let total_h = wh + top_room;
        let x = self.ax(x_off);
        let y = self.ay();
        w.set_row_rect(self.ox + self.cx + 8.0, self.cw - 16.0);
        let clamped_w = ww.min((self.cw - x_off).max(0.0));
        render_widget(pc, w, x, y, clamped_w, total_h, ctx);
        self.y += total_h;
    }

    pub fn row<F: FnOnce(&mut Row)>(&mut self, pc: &mut dyn RenderTarget, h: f32, f: F) {
        let row_y = self.ay();
        let mut row = Row {
            pc: &mut *pc,
            base_x: self.ox + self.cx,
            y: row_y,
            cursor_x: 0.0,
            spacing: CONTROL_GAP,
        };
        f(&mut row);
        self.y = self.y + h;
    }
}

pub struct Row<'a> {
    pc: &'a mut dyn RenderTarget,
    base_x: f32,
    y: f32,
    pub cursor_x: f32,
    pub spacing: f32,
}

impl<'a> Row<'a> {
    pub fn set_spacing(&mut self, spacing: f32) {
        self.spacing = spacing;
    }

    pub fn gap(&mut self, width: f32) {
        self.cursor_x += width;
    }

    pub fn text(&mut self, text: &str, y_off: f32, font_size: f32, color: [f32; 4], width: f32) {
        self.pc
            .text(text, self.base_x + self.cursor_x, self.y + y_off, font_size, color);
        self.cursor_x += width + self.spacing;
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, w: &mut T, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        render_widget(self.pc, w, self.base_x + self.cursor_x, self.y, ww, wh, ctx);
        self.cursor_x += ww + self.spacing;
    }
}

fn estimate_label_width_helper(label: &str, font_size: f32, font_fam: &str) -> f32 {
    let fam_lower = font_fam.to_lowercase();
    let is_mono = fam_lower.contains("mono") || fam_lower.contains("courier") || fam_lower == "monospace";
    if is_mono {
        label.chars().count() as f32 * font_size * 0.60
    } else {
        let mut width = 0.0;
        for c in label.chars() {
            let factor = match c {
                'i' | 'l' | 'I' | ' ' | '.' | ',' | '!' | ';' | ':' | '\'' | '"' | '(' | ')' | '[' | ']' | '-' => 0.30,
                'f' | 'j' | 't' => 0.35,
                'r' | 's' | 'c' | 'z' => 0.50,
                'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'k' | 'n' | 'o' | 'p' | 'q' | 'u' | 'v' | 'x' | 'y' => 0.60,
                'm' | 'w' | 'M' | 'W' | '&' | '@' | 'O' | 'Q' | 'G' => 0.85,
                'A' | 'B' | 'C' | 'D' | 'H' | 'N' | 'U' | 'V' | 'X' | 'Y' => 0.75,
                'E' | 'F' | 'K' | 'L' | 'P' | 'R' | 'S' | 'T' | 'Z' | 'J' => 0.68,
                '0'..='9' => 0.60,
                _ => 0.60,
            };
            width += factor * font_size;
        }
        width
    }
}

pub struct Section {
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub is_child: bool,
    pub grid: Grid,
    pub last_col: usize,
}

impl Section {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        section_padding()
    }

    pub fn new(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str) -> Self {
        Self::new_opt(pc, left, top, cw, label, false)
    }

    pub fn new_opt(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else {
            left + (cw - label_width) / 2.0
        };
        pc.text_with_font(label, label_x, top, font_size, font_color, &font_fam);

        let pad = section_padding();
        let margin_x = 2.0 * pad + 12.0;
        let usable_w = (cw - 2.0 * margin_x).max(1.0);
        let min_col_width = 130.0;
        let gap = 8.0;
        let max_cols = if is_child {
            1
        } else {
            ((usable_w + gap) / (min_col_width + gap)).floor().max(1.0).min(2.0) as usize
        };
        let content_start_y = top + pad + 19.0;
        let grid = Grid::new(left + margin_x, content_start_y, usable_w, min_col_width, gap, max_cols);

        Self { left, top, content_y: content_start_y, cw, label_width, is_child, grid, last_col: usize::MAX }
    }

    /// Horizontal inset of content from this section's left edge — the same
    /// one `row_layout`, the column `Grid` and `widget` use, so everything in
    /// a section lines up. See `SectionContext::content_margin`.
    pub fn content_margin(&self) -> f32 {
        2.0 * self.padding() + 12.0
    }

    pub fn content_left(&self) -> f32 {
        self.left + self.content_margin()
    }

    pub fn content_width(&self) -> f32 {
        (self.cw - 2.0 * self.content_margin()).max(0.0)
    }

    /// See `SectionContext::ax` — same mapping, same reason it is no longer
    /// stepped at `x_off == 12.0`.
    pub fn ax(&self, x_off: f32) -> f32 {
        self.left + 2.0 * self.padding() + x_off
    }

    pub fn ay(&self) -> f32 { self.content_y }

    pub fn spacing(&mut self, dy: f32) {
        if self.grid.col_heights.len() >= 2 {
            if self.last_col == usize::MAX {
                for h in &mut self.grid.col_heights {
                    *h += dy;
                }
            } else if self.last_col < self.grid.col_heights.len() {
                self.grid.col_heights[self.last_col] += dy;
            }
            self.content_y = self.grid.max_height();
        } else {
            self.content_y += dy;
            for h in &mut self.grid.col_heights {
                *h += dy;
            }
        }
    }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let x = self.ax(x_off);
        let y = self.ay() + y_off;
        // Bounded to the content box, like `SectionContext::text` — text is
        // the only thing a section draws that is not already sized to fit it.
        let right = self.content_left() + self.content_width();
        let bounds = if right > x {
            Some([x, y - font_size, right, y + 2.0 * font_size])
        } else {
            None
        };
        pc.text_with_bounds(text, x, y, font_size, color, bounds);
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, _x_off: f32, _ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        let top_room = w.label_strip();
        let total_h = wh + top_room;

        let name = w.type_name();
        let span_full = name == "Trackpad"
            || name == "Canvas"
            || name == "UsageBar"
            || name == "ProgressBar"
            || name == "ButtonStrip"
            || name == "Spreadsheet"
            || name == "Graph";

        if span_full {
            let margin_x = 2.0 * pad + 12.0;
            let x = self.left + margin_x;
            let clamped_w = (self.cw - 2.0 * margin_x).max(0.0);
            let max_h = self.grid.max_height().max(self.content_y);
            let y = max_h;

            w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
            render_widget(pc, w, x, y, clamped_w, total_h, ctx);

            let new_bottom = y + total_h;
            self.content_y = new_bottom;
            for h in &mut self.grid.col_heights {
                *h = new_bottom;
            }
        } else {
            let max_h = self.grid.max_height();
            if self.content_y > max_h {
                for h in &mut self.grid.col_heights {
                    *h = self.content_y;
                }
            }

            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let y = self.grid.col_heights[col];

            w.set_row_rect(x, self.grid.col_width);
            render_widget(pc, w, x, y, self.grid.col_width, total_h, ctx);
            self.grid.col_heights[col] += total_h;
            self.content_y = self.grid.max_height();
        }
    }

    pub fn widget_full<T: WidgetHost + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off);
        self.widget(pc, w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let pad = self.padding();
        let x = self.ax(pad);
        let max_h = self.grid.max_height().max(self.content_y);
        let y = max_h;
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y = max_h + 8.0;
        for h in &mut self.grid.col_heights {
            *h = self.content_y;
        }
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        let max_h = self.grid.max_height().max(self.content_y);
        pc.rect(color, self.ax(x_off), max_h, w, h);
        self.content_y = max_h + h;
        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = self.content_margin();
        let usable_w = self.content_width();
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let max_h = self.grid.max_height().max(self.content_y);
        for col_h in &mut self.grid.col_heights {
            *col_h = max_h;
        }
        self.content_y = max_h;

        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;

        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn finish(&mut self, pc: &mut dyn RenderTarget) -> f32 {
        self.finish_focused(pc, false)
    }

    pub fn finish_focused(&mut self, pc: &mut dyn RenderTarget, focused: bool) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if focused {
                [0.30, 0.50, 0.32, 1.0]
            } else {
                [0.25, 0.25, 0.35, 1.0]
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;
        
        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = if self.is_child {
                let base_x = match nested_section_label_alignment() {
                    0 => self.left + 12.0,
                    1 => self.left + (self.cw - self.label_width) / 2.0,
                    2 => self.left + self.cw - 12.0 - self.label_width,
                    _ => self.left + 12.0,
                };
                base_x + nested_section_label_offset()
            } else {
                self.left + (self.cw - self.label_width) / 2.0
            };
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            pc.rect(border, left_edge, y, w, 1.0);
        }

        let extra_bottom = pad + 12.0;
        pc.rect(border, x, y + h + extra_bottom, w, 1.0);
        pc.rect(border, x, y, 1.0, h + extra_bottom);
        pc.rect(border, x + w - 1.0, y, 1.0, h + extra_bottom);
        self.content_y + extra_bottom + 8.0
    }

    pub fn vstack<'a>(&'a mut self, pc: &'a mut dyn RenderTarget, spacing: f32) -> SectionVStack<'a> {
        SectionVStack {
            section: self,
            pc,
            spacing,
        }
    }
}

pub struct SectionVStack<'a> {
    section: &'a mut Section,
    pc: &'a mut dyn RenderTarget,
    spacing: f32,
}

impl<'a> SectionVStack<'a> {
    pub fn add_widget<T: WidgetHost + 'static>(&mut self, w: &mut T, ww: f32, wh: f32, ctx: &mut UiContext) {
        self.section.widget(self.pc, w, Section::DEFAULT_MARGIN_X, ww, wh, ctx);
        self.section.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        self.section.row(count, gap, h, f);
        self.section.spacing(self.spacing);
    }
}


pub struct SplitterLayout {
    pub splitter1_x: f32,
    pub splitter2_x: f32,
    pub splitter_width: f32,
    pub min_column_width: f32,
}

impl SplitterLayout {
    pub fn new(width: f32, splitter_width: f32, min_column_width: f32) -> Self {
        let s1 = (width - 2.0 * splitter_width) / 3.0;
        let s2 = s1 + splitter_width + (width - 2.0 * splitter_width) / 3.0;
        Self {
            splitter1_x: s1,
            splitter2_x: s2,
            splitter_width,
            min_column_width,
        }
    }

    pub fn clamp(&mut self, total_width: f32, detached_circular_network: bool) {
        if detached_circular_network {
            let min_s2 = self.min_column_width;
            let max_s2 = (total_width - self.min_column_width).max(min_s2);
            self.splitter2_x = self.splitter2_x.clamp(min_s2, max_s2);
        } else {
            let min_s1 = self.min_column_width;
            let max_s1 = (self.splitter2_x - self.splitter_width - self.min_column_width).max(min_s1);
            self.splitter1_x = self.splitter1_x.clamp(min_s1, max_s1);
            let min_s2 = self.splitter1_x + self.splitter_width + self.min_column_width;
            let max_s2 = (total_width - self.min_column_width).max(min_s2);
            self.splitter2_x = self.splitter2_x.clamp(min_s2, max_s2);
        }
    }

    pub fn scale(&mut self, factor: f32) {
        self.splitter1_x *= factor;
        self.splitter2_x *= factor;
    }

    pub fn left_col(&self) -> (f32, f32) { // (x, width)
        (0.0, self.splitter1_x)
    }

    pub fn center_col(&self) -> (f32, f32) { // (x, width)
        let x = self.splitter1_x + self.splitter_width;
        (x, self.splitter2_x - x)
    }

    pub fn right_col(&self, total_width: f32) -> (f32, f32) { // (x, width)
        let x = self.splitter2_x + self.splitter_width;
        (x, (total_width - x).max(0.0))
    }
}

#[derive(Debug, Clone)]
pub struct Grid {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub col_width: f32,
    pub gap: f32,
    pub col_heights: Vec<f32>,
    pub col_lefts: Vec<f32>,
}

impl Grid {
    pub fn new(left: f32, top: f32, width: f32, min_col_width: f32, gap: f32, count: usize) -> Self {
        let total_gap = gap * (count - 1) as f32;
        let col_width = if count > 0 {
            (width - total_gap).max(0.0) / count as f32
        } else {
            min_col_width
        };
        let left_offset = 0.0;

        let mut col_lefts = Vec::with_capacity(count);
        let col_heights = vec![top; count];
        for i in 0..count {
            col_lefts.push(left + left_offset + i as f32 * (col_width + gap));
        }

        Self {
            left,
            top,
            width,
            col_width,
            gap,
            col_heights,
            col_lefts,
        }
    }

    pub fn next_column(&self) -> usize {
        let mut min_idx = 0;
        let mut min_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] < min_h {
                min_h = self.col_heights[i];
                min_idx = i;
            }
        }
        min_idx
    }

    pub fn max_height(&self) -> f32 {
        let mut max_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] > max_h {
                max_h = self.col_heights[i];
            }
        }
        max_h
    }
}

pub struct CircularPaneLayout {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl CircularPaneLayout {
    pub fn new(x: f32, y: f32, r: f32) -> Self {
        Self { x, y, r }
    }

    pub fn hit_test_content(&self, cx: f32, cy: f32, menubar_h: f32, breadcrumb_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= self.r * self.r && cy >= self.y - self.r + 45.0 + menubar_h + breadcrumb_h
    }

    pub fn hit_test_menubar(&self, cx: f32, cy: f32, menubar_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist >= self.r - menubar_h && dist <= self.r && cy < self.y
    }

    pub fn hit_test_breadcrumb(&self, cx: f32, cy: f32, menubar_h: f32, breadcrumb_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= self.r * self.r && cy >= self.y - self.r + menubar_h && cy < self.y - self.r + 45.0 + menubar_h + breadcrumb_h
    }

    pub fn hit_test_border(&self, cx: f32, cy: f32, border_thickness: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist >= self.r - border_thickness && dist <= self.r
    }
}

#[derive(Debug, Clone)]
pub struct Radial {
    pub center_x: f32,
    pub center_y: f32,
    pub aspect_ratio: f32,
    pub base_spacing: f32,
}

impl Radial {
    pub fn new(center_x: f32, center_y: f32, aspect_ratio: f32, base_spacing: f32) -> Self {
        Self { center_x, center_y, aspect_ratio, base_spacing }
    }

    pub fn widget_rect(&self, idx: usize, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if idx == 0 {
            (self.center_x - ww / 2.0, self.center_y - wh / 2.0, ww, wh)
        } else {
            let mut ring = 1;
            let mut ring_start = 1;
            loop {
                let ring_capacity = ring * 6;
                if idx < ring_start + ring_capacity {
                    let pos_in_ring = idx - ring_start;
                    let angle = (pos_in_ring as f32) * (2.0 * std::f32::consts::PI / ring_capacity as f32);
                    let radius = (ring as f32) * self.base_spacing;

                    let x_offset = radius * angle.cos() * self.aspect_ratio;
                    let y_offset = radius * angle.sin();

                    return (
                        self.center_x + x_offset - ww / 2.0,
                        self.center_y + y_offset - wh / 2.0,
                        ww,
                        wh,
                    );
                }
                ring_start += ring_capacity;
                ring += 1;
            }
        }
    }

}



pub trait LayoutStrategy: std::fmt::Debug {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32);
    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32);
    fn set_section_count(&mut self, _count: usize) {}
    fn get_column_width(&self) -> Option<f32> { None }
    fn get_gap(&self) -> f32 { 20.0 }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &mut crate::context::UiContext) -> f32;
    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size;
    fn box_clone(&self) -> Box<dyn LayoutStrategy>;
}

impl Clone for Box<dyn LayoutStrategy> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlexDirection {
    Row,
    Column,
}

#[derive(Debug, Clone)]
pub struct FlexLayout {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
    direction: FlexDirection,
    spacing: f32,
    current_x: f32,
    current_y: f32,
}

impl FlexLayout {
    pub fn new(direction: FlexDirection, spacing: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            height: 0.0,
            direction,
            spacing,
            current_x: 0.0,
            current_y: 0.0,
        }
    }
}

impl LayoutStrategy for FlexLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32) {
        self.left = left;
        self.top = top;
        self.width = width;
        self.height = height;
        self.current_x = left;
        self.current_y = top;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        match self.direction {
            FlexDirection::Row => {
                let rx = self.current_x;
                let ry = self.current_y;
                self.current_x += ww + self.spacing;
                (rx, ry, ww, wh)
            }
            FlexDirection::Column => {
                let rx = self.current_x;
                let ry = self.current_y;
                self.current_y += wh + self.spacing;
                (rx, ry, ww, wh)
            }
        }
    }

    fn get_gap(&self) -> f32 {
        self.spacing
    }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &mut crate::context::UiContext) -> f32 {
        let (cur_x, cur_y) = (x, y);
        // Blocks: the label row (`label_lead`) above every child's content, the
        // gap between blocks, so a row's controls are level and a carve-out tab
        // sits a full gap from its neighbour.
        let lead = crate::widget::container::container_layout::label_lead(children);
        match self.direction {
            FlexDirection::Row => {
                let mut cur_x = cur_x;
                for &child_ptr in children {
                    unsafe {
                        let child = &mut *child_ptr;
                        let child_w = child.rect().2;
                        let child_h = crate::widget::container::container_layout::content_height(child);
                        let use_h = if child_h > 0.0 { child_h } else { h };
                        child.layout(
                            crate::widget::Point { x: cur_x, y: cur_y + lead },
                            crate::widget::LayoutConstraints::new(child_w, child_w, use_h, use_h),
                            ctx,
                        );
                        cur_x += child_w + self.spacing;
                    }
                }
                (cur_x - x).max(0.0)
            }
            FlexDirection::Column => {
                let mut cur_y = cur_y;
                for &child_ptr in children {
                    unsafe {
                        let child = &mut *child_ptr;
                        let child_h = crate::widget::container::container_layout::content_height(child);
                        let use_h = if child_h > 0.0 { child_h } else { 44.0 };
                        child.layout(
                            crate::widget::Point { x, y: cur_y + lead },
                            crate::widget::LayoutConstraints::new(w, w, use_h, use_h),
                            ctx,
                        );
                        cur_y += lead + use_h + self.spacing;
                    }
                }
                (cur_y - y).max(0.0)
            }
        }
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        match self.direction {
            FlexDirection::Row => {
                let mut total_w = 0.0f32;
                let mut max_h = 0.0f32;
                for (i, &child_ptr) in children.iter().enumerate() {
                    unsafe {
                        let size = (*child_ptr).measure(constraints, ctx);
                        total_w += size.width;
                        max_h = max_h.max(size.height);
                        if i > 0 {
                            total_w += self.spacing;
                        }
                    }
                }
                crate::widget::Size {
                    width: total_w.clamp(constraints.min_width, constraints.max_width),
                    height: max_h.clamp(constraints.min_height, constraints.max_height),
                }
            }
            FlexDirection::Column => {
                let mut total_h = 0.0f32;
                let mut max_w = 0.0f32;
                for (i, &child_ptr) in children.iter().enumerate() {
                    unsafe {
                        let size = (*child_ptr).measure(constraints, ctx);
                        total_h += size.height;
                        max_w = max_w.max(size.width);
                        if i > 0 {
                            total_h += self.spacing;
                        }
                    }
                }
                crate::widget::Size {
                    width: max_w.clamp(constraints.min_width, constraints.max_width),
                    height: total_h.clamp(constraints.min_height, constraints.max_height),
                }
            }
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct ColumnLayout {
    left: f32,
    top: f32,
    width: f32,
    current_y: f32,
    gap: f32,
}

impl ColumnLayout {
    pub fn new(gap: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            current_y: 0.0,
            gap,
        }
    }
}

impl LayoutStrategy for ColumnLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        self.left = left;
        self.top = top;
        self.width = width;
        self.current_y = top;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        let x = self.left;
        let y = self.current_y;
        self.current_y += wh + self.gap;
        (x, y, ww, wh)
    }

    fn get_column_width(&self) -> Option<f32> {
        Some(self.width)
    }

    fn get_gap(&self) -> f32 {
        self.gap
    }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let mut cur_y = y;
        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let child_h = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if child_h > 0.0 { child_h } else { 44.0 };
                child.set_rect(x, cur_y, w, use_h);
                cur_y += use_h + self.gap;
            }
        }
        (cur_y - y).max(0.0)
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        let mut total_h = 0.0f32;
        let mut max_w = 0.0f32;
        for (i, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                total_h += size.height;
                max_w = max_w.max(size.width);
                if i > 0 {
                    total_h += self.gap;
                }
            }
        }
        crate::widget::Size {
            width: max_w.clamp(constraints.min_width, constraints.max_width),
            height: total_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveGrid {
    grid: Option<Grid>,
    #[allow(dead_code)]
    min_col_width: f32,
    #[allow(dead_code)]
    gap: f32,
    num_sections: Option<usize>,
}

impl AdaptiveGrid {
    pub fn new(min_col_width: f32, gap: f32) -> Self {
        Self {
            grid: None,
            min_col_width,
            gap,
            num_sections: None,
        }
    }
}

impl LayoutStrategy for AdaptiveGrid {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        let min_col_width = crate::layout::grid_min_col_width();
        let gap = crate::layout::grid_gap();
        let max_cols = ((width + gap) / (min_col_width + gap)).floor().max(1.0) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(max_cols).max(1)
        } else {
            max_cols
        };
        self.grid = Some(Grid::new(left, top, width, min_col_width, gap, count));
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref mut grid) = self.grid {
            let num_cols = grid.col_heights.len();
            let num_cols_spanned = (((ww + grid.gap) / (grid.col_width + grid.gap)).round() as usize)
                .min(num_cols)
                .max(1);

            if num_cols_spanned >= num_cols {
                let y = grid.max_height();
                let x = grid.left;
                let allocated_w = grid.width;
                for col_h in &mut grid.col_heights {
                    *col_h = y + wh + grid.gap;
                }
                (x, y, allocated_w, wh)
            } else if num_cols_spanned == 1 {
                let col = grid.next_column();
                let x = grid.col_lefts[col];
                let y = grid.col_heights[col];
                grid.col_heights[col] += wh + grid.gap;
                (x, y, grid.col_width, wh)
            } else {
                let n = num_cols_spanned;
                let mut best_start_col = 0;
                let mut min_max_h = f32::MAX;
                for c in 0..=(num_cols - n) {
                    let mut max_h = 0.0f32;
                    for i in 0..n {
                        if grid.col_heights[c + i] > max_h {
                            max_h = grid.col_heights[c + i];
                        }
                    }
                    if max_h < min_max_h {
                        min_max_h = max_h;
                        best_start_col = c;
                    }
                }
                let x = grid.col_lefts[best_start_col];
                let y = min_max_h;
                let allocated_w = n as f32 * grid.col_width + (n - 1) as f32 * grid.gap;
                for i in 0..n {
                    grid.col_heights[best_start_col + i] = y + wh + grid.gap;
                }
                (x, y, allocated_w, wh)
            }
        } else {
            (0.0, 0.0, 0.0, wh)
        }
    }

    fn set_section_count(&mut self, count: usize) {
        self.num_sections = Some(count);
    }

    fn get_column_width(&self) -> Option<f32> {
        self.grid.as_ref().map(|g| g.col_width)
    }

    fn get_gap(&self) -> f32 {
        crate::layout::grid_gap()
    }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let usable_w = w.max(1.0);
        let min_col_width = crate::layout::grid_min_col_width();
        let gap = crate::layout::grid_gap();
        let cols = (((usable_w + gap) / (min_col_width + gap)).floor().max(1.0)) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(cols).max(1)
        } else {
            cols
        };

        let total_gap = gap * (count - 1) as f32;
        let available_w = (w - total_gap).max(1.0);
        let col_w = available_w / count as f32;
        
        let mut col_heights = vec![y; count];

        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                
                let mut min_col = 0;
                let mut min_h = col_heights[0];
                for i in 1..count {
                    if col_heights[i] < min_h {
                        min_h = col_heights[i];
                        min_col = i;
                    }
                }
                
                let cx = x + min_col as f32 * (col_w + gap);
                let cy = col_heights[min_col];
                child.set_rect(cx, cy, col_w, use_h);
                col_heights[min_col] += use_h + gap;
            }
        }
        
        let max_h = col_heights.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        (max_h - y).max(0.0)
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        let usable_w = constraints.max_width.max(1.0);
        let min_col_width = crate::layout::grid_min_col_width();
        let gap = crate::layout::grid_gap();
        let cols = (((usable_w + gap) / (min_col_width + gap)).floor().max(1.0)) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(cols).max(1)
        } else {
            cols
        };

        let mut col_heights = vec![0.0f32; count];
        let total_gap = gap * (count - 1) as f32;
        let available_w = (constraints.max_width - total_gap).max(1.0);
        let col_w = available_w / count as f32;
        
        let child_constraints = crate::widget::LayoutConstraints::new(col_w, col_w, constraints.min_height, constraints.max_height);

        for &child_ptr in children {
            unsafe {
                let size = (*child_ptr).measure(child_constraints, ctx);
                let mut min_col = 0;
                let mut min_h = col_heights[0];
                for i in 1..count {
                    if col_heights[i] < min_h {
                        min_h = col_heights[i];
                        min_col = i;
                    }
                }
                col_heights[min_col] += size.height + gap;
            }
        }
        
        let max_h = col_heights.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        crate::widget::Size {
            width: constraints.max_width,
            height: max_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct RadialLayout {
    radial: Option<Radial>,
    aspect_ratio: f32,
    base_spacing: f32,
    idx: usize,
}

impl RadialLayout {
    pub fn new(aspect_ratio: f32, base_spacing: f32) -> Self {
        Self {
            radial: None,
            aspect_ratio,
            base_spacing,
            idx: 0,
        }
    }
}

impl LayoutStrategy for RadialLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32) {
        let cx = left + width / 2.0;
        let cy = top + height / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (width / height.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        self.radial = Some(Radial::new(cx, cy, aspect, self.base_spacing));
        self.idx = 0;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref radial) = self.radial {
            let rect = radial.widget_rect(self.idx, ww, wh);
            self.idx += 1;
            rect
        } else {
            (0.0, 0.0, ww, wh)
        }
    }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], _ctx: &mut crate::context::UiContext) -> f32 {
        let cx = x + w / 2.0;
        let cy = y + h / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (w / h.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        let radial = Radial::new(cx, cy, aspect, self.base_spacing);
        for (idx, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let child = &mut *child_ptr;
                let cw = child.rect().2;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                let (rx, ry, rw, rh) = radial.widget_rect(idx, cw, use_h);
                child.set_rect(rx, ry, rw, rh);
            }
        }
        h
    }

    fn measure(&self, constraints: crate::widget::LayoutConstraints, children: &[*mut (dyn crate::widget::WidgetHost + 'static)], ctx: &crate::context::UiContext) -> crate::widget::Size {
        let cx = constraints.max_width / 2.0;
        let cy = constraints.max_height / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (constraints.max_width / constraints.max_height.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        let radial = Radial::new(cx, cy, aspect, self.base_spacing);
        let mut max_w = 0.0f32;
        let mut max_h = 0.0f32;
        for (idx, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                let (rx, ry, rw, rh) = radial.widget_rect(idx, size.width, size.height);
                max_w = max_w.max(rx + rw);
                max_h = max_h.max(ry + rh);
            }
        }
        crate::widget::Size {
            width: max_w.clamp(constraints.min_width, constraints.max_width),
            height: max_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn LayoutStrategy> {
        Box::new(self.clone())
    }
}

/// Vertical slack for a section's content clip. The clip exists to stop
/// content escaping its section SIDEWAYS, which is the axis a section's width
/// actually fixes; a section's height is only known once its content has been
/// placed, so bounding that axis too would risk cutting content off rather
/// than keeping it in. Deliberately far larger than any section.
const SECTION_CLIP_SLACK: f32 = 100_000.0;

pub struct PageLayoutBuilder<'a, P> {
    pub strategy: &'a mut dyn LayoutStrategy,
    pub cx: f32,
    pub cy: f32,
    pub cw: f32,
    pub ch: f32,
    pub section_width: f32,
    pub idx: usize,
    _phantom: std::marker::PhantomData<P>,
}

impl<'a, P: RenderTarget + Default> PageLayoutBuilder<'a, P> {
    pub fn new(
        strategy: &'a mut dyn LayoutStrategy,
        cx: f32,
        cy: f32,
        cw: f32,
        ch: f32,
        section_width: f32,
    ) -> Self {
        strategy.init(cx, cy, cw, ch);
        Self {
            strategy,
            cx,
            cy,
            cw,
            ch,
            section_width,
            idx: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_section_count(self, count: usize) -> Self {
        self.strategy.set_section_count(count);
        self.strategy.init(self.cx, self.cy, self.cw, self.ch);
        self
    }

    pub fn add_section<F>(&mut self, final_pc: &mut P, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let width = self.strategy.get_column_width().unwrap_or(self.section_width);
        let mut dummy = P::default();
        let mut dummy_ctx = SectionContext::new(&mut dummy, 0.0, 0.0, width, label, focused, false);
        render_fn(&mut dummy_ctx);
        let wh = dummy_ctx.finish();
        let (rx, ry, rw, _) = self.strategy.allocate(width, wh);
        let mut real_ctx = SectionContext::new(final_pc, rx, ry, rw, label, focused, false);
        let (clip_x, clip_w) = (real_ctx.content_left(), real_ctx.content_width());
        real_ctx.pc.push_clip_rect(clip_x, ry - SECTION_CLIP_SLACK, clip_w, 2.0 * SECTION_CLIP_SLACK);
        render_fn(&mut real_ctx);
        real_ctx.pc.pop_clip_rect();
        real_ctx.finish();
        self.idx += 1;
    }

    pub fn add_section_with_width<F>(&mut self, final_pc: &mut P, width: f32, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let mut dummy = P::default();
        let mut dummy_ctx = SectionContext::new(&mut dummy, 0.0, 0.0, width, label, focused, false);
        render_fn(&mut dummy_ctx);
        let wh = dummy_ctx.finish();
        let (rx, ry, rw, _) = self.strategy.allocate(width, wh);
        let mut real_ctx = SectionContext::new(final_pc, rx, ry, rw, label, focused, false);
        let (clip_x, clip_w) = (real_ctx.content_left(), real_ctx.content_width());
        real_ctx.pc.push_clip_rect(clip_x, ry - SECTION_CLIP_SLACK, clip_w, 2.0 * SECTION_CLIP_SLACK);
        render_fn(&mut real_ctx);
        real_ctx.pc.pop_clip_rect();
        real_ctx.finish();
        self.idx += 1;
    }

    pub fn add_section_spanned<F>(&mut self, final_pc: &mut P, label: &str, span: usize, focused: bool, render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let col_width = self.strategy.get_column_width().unwrap_or(self.section_width);
        let gap = self.strategy.get_gap();
        let width = span as f32 * col_width + (span - 1) as f32 * gap;
        self.add_section_with_width(final_pc, width, label, focused, render_fn);
    }
}

pub struct SectionContext<'a, P> {
    pub pc: &'a mut P,
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub label_x: f32,
    /// Whether the host renders sections as sunken wells (`section_relief_style`).
    pub relief_style: bool,
    /// The title tab box (x, y, w, h) when the host's relief styling laid the
    /// label out left-aligned — `finish` offers it with the section carve.
    /// None under relief styling means a label-less section: the well carves
    /// tabless, flush with the allocation top.
    pub relief_tab: Option<(f32, f32, f32, f32)>,
    pub focused: bool,
    pub is_child: bool,
    pub grid: Grid,
    pub last_col: usize,
    pub content_start_y: f32,
    pub row_gap: f32,
}

impl<'a, P: RenderTarget> SectionContext<'a, P> {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        section_padding()
    }

    pub fn new(pc: &'a mut P, left: f32, top: f32, cw: f32, label: &str, focused: bool, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let relief_style = pc.section_relief_style();
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else if relief_style {
            // Sunken style: the title sits in a tab flush with the well's left
            // edge (the designer look), not centered on the border.
            left + 12.0
        } else {
            left + (cw - label_width) / 2.0
        };
        // Under relief styling the well fills the whole allocated rect (so the
        // page's gaps and margins are the visual gaps) — the tab tops the
        // allocation and the label centers inside it.
        let label_y = if relief_style { top + 4.0 } else { top };
        pc.text_with_font(label, label_x, label_y, font_size, font_color, &font_fam);

        // The tab wraps the label, flush on the well's top-left corner; clamped
        // so off-default child alignments can't push it outside the well.
        let relief_tab = if relief_style && label_width > 0.0 {
            let tab_x = (label_x - 12.0).max(left);
            Some((tab_x, top, label_width + 24.0, font_size + 10.0))
        } else {
            None
        };

        let pad = section_padding();
        let margin_x = 2.0 * pad + 12.0;
        let usable_w = (cw - 2.0 * margin_x).max(1.0);
        let min_col_width = 130.0;
        let gap = 8.0;
        let max_cols = if is_child {
            1
        } else {
            ((usable_w + gap) / (min_col_width + gap)).floor().max(1.0).min(2.0) as usize
        };
        // Under relief styling the content stands off the well's top wall by
        // the same inset it keeps from the side walls (`margin_x`, which is
        // also what `finish` leaves below it), so a well reads as one even
        // frame. The outline style's `pad + 19` predates the title tab: under
        // relief the tab alone is `font_size + 10` tall, which left the first
        // line touching the well's top wall at the default padding while the
        // sides kept 20px and more.
        let content_start_y = if relief_style {
            relief_tab.map(|t| t.1 + t.3).unwrap_or(top) + margin_x
        } else {
            top + pad + 19.0
        };
        let grid = Grid::new(left + margin_x, content_start_y, usable_w, min_col_width, gap, max_cols);

        Self {
            pc,
            left,
            top,
            content_y: content_start_y,
            cw,
            label_width,
            label_x,
            relief_style,
            relief_tab,
            focused,
            is_child,
            grid,
            last_col: usize::MAX,
            content_start_y,
            row_gap: Self::DEFAULT_ROW_GAP,
        }
    }

    pub fn with_row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    /// The section's content-box top edge: the body well's top (the tab's
    /// bottom) under relief styling, the outline's border line otherwise. Lets
    /// a page place content at an exact inset from the well's walls.
    pub fn well_top(&self) -> f32 {
        if self.relief_style {
            self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top)
        } else {
            self.top + 7.0
        }
    }

    pub fn set_row_gap(&mut self, gap: f32) {
        self.row_gap = gap;
    }

    /// Horizontal inset of section CONTENT from the section's left edge.
    ///
    /// One number, used by every content placer in here — `row_layout`, the
    /// column `Grid`, `widget`, `VStack` and `ax` (so `text`) — because they
    /// share a section and have to line up inside it. The section's border is
    /// drawn at `left + padding()` (see `finish`), so content clears the
    /// border by `padding() + 12`.
    pub fn content_margin(&self) -> f32 {
        2.0 * self.padding() + 12.0
    }

    /// Left edge of the content box: where a row, a widget or a `text(_, 12.0,
    /// ..)` starts.
    pub fn content_left(&self) -> f32 {
        self.left + self.content_margin()
    }

    /// Width of the content box — the section's width less the inset on both
    /// sides. Nothing a section draws should extend past `content_left() +
    /// content_width()`.
    pub fn content_width(&self) -> f32 {
        (self.cw - 2.0 * self.content_margin()).max(0.0)
    }

    /// `x_off` px into the content box's coordinate space, where 12.0 is the
    /// content's own left edge — the offset 46 of the ~55 call sites in the
    /// tree already pass, and the one that lines text up with the buttons and
    /// widgets beside it.
    ///
    /// This used to add `padding()` only when `x_off >= 12.0`, which made the
    /// mapping DISCONTINUOUS: asking for 11 instead of 12 moved the text 5px
    /// LEFT rather than 1px, and silently dropped it out of alignment with
    /// every row in the same section. `cce-mail` and two others sit on the
    /// wrong side of that cliff today.
    pub fn ax(&self, x_off: f32) -> f32 {
        self.left + 2.0 * self.padding() + x_off
    }

    pub fn ay(&self) -> f32 {
        self.content_y
    }

    pub fn spacing(&mut self, dy: f32) {
        if self.grid.col_heights.len() >= 2 {
            if self.last_col == usize::MAX {
                for h in &mut self.grid.col_heights {
                    *h += dy;
                }
            } else if self.last_col < self.grid.col_heights.len() {
                self.grid.col_heights[self.last_col] += dy;
            }
            self.content_y = self.grid.max_height();
        } else {
            self.content_y += dy;
            for h in &mut self.grid.col_heights {
                *h += dy;
            }
        }
    }

    pub fn text(&mut self, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let mut y = self.content_y + y_off;
        if y > self.content_start_y {
            y += self.row_gap;
        }
        let x = self.ax(x_off);
        // Bound it to the content box. A section's text was drawn unbounded,
        // so a string wider than its section simply kept going — over the
        // border, over whatever sat to the right, and off the window (the
        // settings app's GPU names did all three). Rows and widgets have
        // always been sized to the section; text was the one thing that could
        // leave it. The vertical band is generous on purpose: it is the
        // horizontal overrun that has to be cut, and a tight band would
        // shave descenders.
        let right = self.content_left() + self.content_width();
        let bounds = if right > x {
            Some([x, y - font_size, right, y + 2.0 * font_size])
        } else {
            None
        };
        self.pc.text_with_bounds(text, x, y, font_size, color, bounds);
        let new_bottom = y + font_size + 4.0;
        self.content_y = new_bottom;
        for h in &mut self.grid.col_heights {
            *h = new_bottom;
        }
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, w: &mut T, _x_off: f32, _ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        let top_room = w.label_strip();
        let total_h = wh + top_room;

        let name = w.type_name();
        let span_full = name == "Trackpad"
            || name == "Canvas"
            || name == "UsageBar"
            || name == "ProgressBar"
            || name == "ButtonStrip"
            || name == "Spreadsheet"
            || name == "Graph";

        if span_full {
            let margin_x = 2.0 * pad + 12.0;
            let x = self.left + margin_x;
            let clamped_w = (self.cw - 2.0 * margin_x).max(0.0);
            let mut max_h = self.grid.max_height().max(self.content_y);
            if max_h > self.content_start_y {
                max_h += self.row_gap;
            }
            let y = max_h;

            w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
            render_widget(self.pc, w, x, y, clamped_w, total_h, ctx);

            let new_bottom = y + total_h;
            self.content_y = new_bottom;
            for h in &mut self.grid.col_heights {
                *h = new_bottom;
            }
        } else {
            let max_h = self.grid.max_height();
            if self.content_y > max_h {
                for h in &mut self.grid.col_heights {
                    *h = self.content_y;
                }
            }

            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let mut y = self.grid.col_heights[col];
            if y > self.content_start_y {
                y += self.row_gap;
            }

            let aligned_x = x;
            let aligned_w = self.grid.col_width;

            w.set_row_rect(aligned_x, aligned_w);
            render_widget(self.pc, w, aligned_x, y, aligned_w, total_h, ctx);
            self.grid.col_heights[col] = y + total_h;
            self.content_y = self.grid.max_height();
        }
    }

    pub fn widget_full<T: WidgetHost + 'static>(&mut self, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off); // cw - 40.0
        self.widget(w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self) {
        let pad = self.padding();
        let x = self.ax(pad);
        let max_h = self.grid.max_height().max(self.content_y);
        let y = max_h;
        self.pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y = max_h + 8.0;
        for h in &mut self.grid.col_heights {
            *h = self.content_y;
        }
    }

    pub fn rect(&mut self, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        let max_h = self.grid.max_height().max(self.content_y);
        self.pc.rect(color, self.ax(x_off), max_h, w, h);
        self.content_y = max_h + h;
        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = self.content_margin();
        let usable_w = self.content_width();
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    /// A row whose columns are sized to what goes IN them: each gets the width
    /// it asked for in `needs`, and whatever is left over is shared equally.
    ///
    /// `row_layout` splits a row evenly and knows nothing about content, so it
    /// hands "Reboot" and "Hibernate" the same width — one floats in slack
    /// while the other is cut off, which is what a row of mismatched labels
    /// looks like. Sharing the SLACK equally rather than sizing proportionally
    /// is deliberate: proportional widths would make a two-character label a
    /// sliver, where what is wanted is "everyone fits, then everyone gets the
    /// same bonus".
    ///
    /// When the needs do not fit, every column is scaled by the same factor, so
    /// the row still cannot overflow its section and the shortfall is shared
    /// rather than landing entirely on the last column.
    pub fn row_layout_for(&self, needs: &[f32], gap: f32) -> Vec<(f32, f32)> {
        let count = needs.len();
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let room = (self.content_width() - total_gap).max(0.0);
        let total_need: f32 = needs.iter().map(|n| n.max(0.0)).sum();

        let widths: Vec<f32> = if total_need <= room {
            let extra = (room - total_need) / count as f32;
            needs.iter().map(|n| n.max(0.0) + extra).collect()
        } else if total_need > 0.0 {
            let scale = room / total_need;
            needs.iter().map(|n| n.max(0.0) * scale).collect()
        } else {
            vec![room / count as f32; count]
        };

        let mut cols = Vec::with_capacity(count);
        let mut x = self.content_left();
        for w in widths {
            cols.push((x, w));
            x += w + gap;
        }
        cols
    }


    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let max_h = self.grid.max_height().max(self.content_y);
        for col_h in &mut self.grid.col_heights {
            *col_h = max_h;
        }
        self.content_y = max_h;

        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;

        for col_h in &mut self.grid.col_heights {
            *col_h = self.content_y;
        }
    }

    pub fn vstack(&mut self, spacing: f32) -> VStack<'_, 'a, P> {
        VStack {
            context: self,
            spacing,
        }
    }

    pub fn add_section<F>(&mut self, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let pad = self.padding();
        let (left, top, cw, is_side_by_side) = if self.grid.col_heights.len() >= 2 {
            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let mut y = self.grid.col_heights[col];
            if y > self.content_start_y {
                y += self.row_gap;
            }
            (x, y, self.grid.col_width, true)
        } else {
            let left = self.ax(0.0) + pad;
            let mut max_h = self.grid.max_height().max(self.content_y);
            if max_h > self.content_start_y {
                max_h += self.row_gap;
            }
            let top = max_h;
            let cw = self.cw - 2.0 * pad;
            (left, top, cw, false)
        };

        let mut sub_ctx = SectionContext::new(self.pc, left, top, cw, label, focused, true);
        render_fn(&mut sub_ctx);
        let new_bottom = sub_ctx.finish();

        if is_side_by_side {
            let col = self.last_col;
            self.grid.col_heights[col] = new_bottom;
            self.content_y = self.grid.max_height();
        } else {
            self.content_y = new_bottom;
            for h in &mut self.grid.col_heights {
                *h = new_bottom;
            }
        }
    }

    pub fn finish(self) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if self.focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if self.focused {
                [0.30, 0.50, 0.32, 1.0] // Focused green
            } else {
                [0.25, 0.25, 0.35, 1.0] // Default gray
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;
        // Under relief the well's walls are the allocation's edges, so the
        // floor below the content matches the inset beside and above it (see
        // `new`). The outline frame sits `pad` in from its allocation and
        // keeps its own slack.
        let extra_bottom = if self.relief_style { self.content_margin() } else { pad + 12.0 };
        let bottom = y + h + extra_bottom;

        if self.relief_style {
            // Sunken style: the well spans the full allocated rect — body top
            // edge at the tab's bottom (the tab is flush ON the body, the
            // designer union shape; label-less sections carve tabless from the
            // allocation top), walls on the allocation's edges, and no
            // trailing slack so the layout gap IS the visual gap.
            let body_y = self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top);
            let frame = SectionFrame {
                x: self.left,
                y: body_y,
                w: self.cw,
                h: bottom - body_y,
                tab: self.relief_tab,
                focused: self.focused,
                is_child: self.is_child,
            };
            if self.pc.section_relief(&frame) {
                return self.content_y + extra_bottom;
            }
        }

        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = self.label_x;
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                self.pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                self.pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            self.pc.rect(border, left_edge, y, w, 1.0);
        }

        self.pc.rect(border, x, y + h + extra_bottom, w, 1.0);
        self.pc.rect(border, x, y, 1.0, h + extra_bottom);
        self.pc.rect(border, x + w - 1.0, y, 1.0, h + extra_bottom);
        self.content_y + extra_bottom + 8.0
    }
}


pub struct VStack<'b, 'a, P> {
    pub context: &'b mut SectionContext<'a, P>,
    pub spacing: f32,
}

impl<'b, 'a, P: RenderTarget> VStack<'b, 'a, P> {
    pub fn add_widget<T: WidgetHost + 'static>(&mut self, w: &mut T, _ww: f32, wh: f32, ctx: &mut UiContext) {
        let pad = self.context.padding();
        let margin_x = 2.0 * pad + 12.0;
        let x = self.context.left + margin_x;
        let clamped_w = (self.context.cw - 2.0 * margin_x).max(0.0);

        let mut max_h = self.context.grid.max_height().max(self.context.content_y);
        if max_h > self.context.content_start_y {
            max_h += self.context.row_gap;
        }
        let y = max_h;

        let pref_h = w.preferred_height().unwrap_or(wh);
        let top_room = w.label_strip();
        let total_h = pref_h + top_room;

        w.set_row_rect(self.context.left + pad, self.context.cw - 2.0 * pad);
        render_widget(self.context.pc, w, x, y, clamped_w, total_h, ctx);

        let new_bottom = y + total_h;
        self.context.content_y = new_bottom;
        for h in &mut self.context.grid.col_heights {
            *h = new_bottom;
        }
        self.context.spacing(self.spacing);
    }

    /// [`add_row`](Self::add_row) with per-column widths from `needs` — see
    /// [`SectionContext::row_layout_for`]. For a row of buttons, `needs` is
    /// each label's measured width plus the plate's own inset.
    pub fn add_row_for<F>(&mut self, needs: &[f32], gap: f32, h: f32, mut f: F)
    where
        F: FnMut(&mut SectionContext<'a, P>, usize, f32, f32),
    {
        let max_h = self.context.grid.max_height().max(self.context.content_y);
        for col_h in &mut self.context.grid.col_heights {
            *col_h = max_h;
        }
        self.context.content_y = max_h;

        let cols = self.context.row_layout_for(needs, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            self.context.content_y = max_h;
            f(self.context, i, x, w);
        }

        let new_bottom = max_h + h;
        self.context.content_y = new_bottom;
        for col_h in &mut self.context.grid.col_heights {
            *col_h = new_bottom;
        }
        self.context.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(&mut SectionContext<'a, P>, usize, f32, f32),
    {
        let max_h = self.context.grid.max_height().max(self.context.content_y);
        for col_h in &mut self.context.grid.col_heights {
            *col_h = max_h;
        }
        self.context.content_y = max_h;

        let cols = self.context.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            self.context.content_y = max_h;
            f(self.context, i, x, w);
        }

        let new_bottom = max_h + h;
        self.context.content_y = new_bottom;
        for col_h in &mut self.context.grid.col_heights {
            *col_h = new_bottom;
        }
        self.context.spacing(self.spacing);
    }
}

pub fn get_system_monospace_font() -> &'static str {
    static MONOSPACE_FONT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MONOSPACE_FONT.get_or_init(|| {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "monospace"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let parsed = name.split(',').next().unwrap_or("monospace").trim();
            if !parsed.is_empty() {
                return parsed.to_string();
            }
        }
        "monospace".to_string()
    })
}

pub fn get_system_sans_serif_font() -> &'static str {
    static SANS_SERIF_FONT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SANS_SERIF_FONT.get_or_init(|| {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "sans-serif"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let parsed = name.split(',').next().unwrap_or("sans-serif").trim();
            if !parsed.is_empty() {
                return parsed.to_string();
            }
        }
        "sans-serif".to_string()
    })
}
