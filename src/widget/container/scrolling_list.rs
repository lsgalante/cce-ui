use crate::widget::*;
use super::scroll_box::ScrollBox;
use crate::widget::display::TextLabel;
use crate::context::UiContext;
pub use crate::widget::input::font_selector::FontSelector;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnWidth {
    Flex,
    Absolute(f32),
    RightOffset(f32),
}

#[derive(Debug, Clone)]
pub struct ListColumn {
    pub name: String,
    pub width: ColumnWidth,
    pub justification: Justification,
}

#[derive(Debug, Clone)]
pub struct ListRow {
    pub cells: Vec<String>,
    pub icon: Option<String>,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub struct List {
    pub base: Widget,
    pub scroll_box: ScrollBox,
    pub item_height: f32,
    pub item_gap: f32,
    pub columns: Option<Vec<ListColumn>>,
    pub rows: Vec<ListRow>,
    pub hovered_row: Option<usize>,
    pub pressed_row: Option<usize>,
    pub clicked_row: Option<usize>,
    pub double_clicked_row: Option<usize>,
    pub last_click_time: Option<std::time::Instant>,
}

impl List {
    pub fn new(item_height: f32, item_gap: f32) -> Self {
        let (_, font_size) = crate::layout::list_font_parsed();
        let adjusted_item_height = item_height.max(font_size + 14.0);
        Self {
            base: Widget::new(),
            scroll_box: ScrollBox::new(),
            item_height: adjusted_item_height,
            item_gap,
            columns: None,
            rows: Vec::new(),
            hovered_row: None,
            pressed_row: None,
            clicked_row: None,
            double_clicked_row: None,
            last_click_time: None,
        }
    }

    pub fn with_columns(mut self, columns: Vec<ListColumn>) -> Self {
        self.columns = Some(columns);
        self
    }

    pub fn get_column_bounds(&self, list_w: f32) -> Vec<(f32, f32)> {
        let cols = match &self.columns {
            Some(c) => c,
            None => return Vec::new(),
        };
        let mut bounds = vec![(0.0, 0.0); cols.len()];
        let mut flex_indices = Vec::new();
        let mut reserved_width = 0.0;

        // Pass 1: Resolve Absolute and RightOffset columns
        for (i, col) in cols.iter().enumerate() {
            match col.width {
                ColumnWidth::Absolute(w) => {
                    bounds[i] = (0.0, w);
                    reserved_width += w;
                }
                ColumnWidth::RightOffset(offset) => {
                    let x = list_w - offset;
                    let mut next_x = list_w;
                    for j in (i + 1)..cols.len() {
                        if let ColumnWidth::RightOffset(o) = cols[j].width {
                            next_x = list_w - o;
                            break;
                        }
                    }
                    let w = (next_x - x).max(0.0);
                    bounds[i] = (x, w);
                }
                ColumnWidth::Flex => {
                    flex_indices.push(i);
                }
            }
        }

        // Pass 2: Layout left-aligned columns (Flex and Absolute)
        let mut current_x = 0.0;
        let mut right_boundary = list_w;
        for (_, col) in cols.iter().enumerate() {
            if let ColumnWidth::RightOffset(offset) = col.width {
                if list_w - offset < right_boundary {
                    right_boundary = list_w - offset;
                }
            }
        }

        let left_space = (right_boundary - current_x).max(0.0);
        let flex_share = if !flex_indices.is_empty() {
            let flex_total = (left_space - reserved_width).max(0.0);
            flex_total / flex_indices.len() as f32
        } else {
            0.0
        };

        for (i, col) in cols.iter().enumerate() {
            match col.width {
                ColumnWidth::Absolute(w) => {
                    bounds[i] = (current_x, w);
                    current_x += w;
                }
                ColumnWidth::Flex => {
                    bounds[i] = (current_x, flex_share);
                    current_x += flex_share;
                }
                ColumnWidth::RightOffset(_) => {
                    // Already resolved
                }
            }
        }

        bounds
    }

    pub fn take_click(&mut self) -> Option<usize> {
        self.clicked_row.take()
    }

    pub fn take_double_click(&mut self) -> Option<usize> {
        self.double_clicked_row.take()
    }

    pub fn update_bounds(&mut self, count: usize, viewport_y: f32, viewport_h: f32) {
        let item_height_full = self.item_height + self.item_gap;
        let content_h = count as f32 * item_height_full + 4.0;
        self.scroll_box.update_bounds(content_h, viewport_y, viewport_h);
    }

    pub fn update_bounds_from_rows(&mut self) {
        let item_height_full = self.item_height + self.item_gap;
        let content_h = self.rows.len() as f32 * item_height_full + 4.0;
        let (_, _, _, h) = self.rect();
        self.scroll_box.update_bounds(content_h, self.scroll_box.viewport_y, h);
    }

    pub fn get_item_draw_y(&self, idx: usize, offset: f32) -> Option<f32> {
        let item_height_full = self.item_height + self.item_gap;
        let virtual_y = idx as f32 * item_height_full + offset;
        self.scroll_box.get_item_draw_y(virtual_y, self.item_height)
    }

    pub fn scroll_y(&self) -> f32 {
        self.scroll_box.scroll_y
    }

    pub fn set_scroll_y(&mut self, val: f32) {
        self.scroll_box.scroll_y = val;
    }
}

impl Default for List {
    fn default() -> Self {
        Self::new(24.0, 4.0)
    }
}

impl Element for List {
    fn base(&self) -> Option<&Widget> { Some(&self.base) }
    fn base_mut(&mut self) -> Option<&mut Widget> { Some(&mut self.base) }

    fn rect(&self) -> (f32, f32, f32, f32) {
        self.scroll_box.rect()
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.base.x = x;
        self.base.y = y;
        self.base.w = w;
        self.base.h = h;
        self.scroll_box.set_rect(x, y, w, h);
        if self.columns.is_some() && !self.rows.is_empty() {
            self.update_bounds_from_rows();
        }
    }

    fn color(&self) -> [f32; 4] {
        self.scroll_box.color()
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        (true, true, true, true)
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::list_corner_radius()
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        let is_focused = focus::is_focused(self) || focus::is_focused(&self.scroll_box);
        let is_hovered = self.hovered() || self.scroll_box.base.hovered;
        if self.scroll_box.show_border {
            let box_border_color = if is_focused {
                [0.30, 0.50, 0.32, 1.0]
            } else if is_hovered {
                [0.25, 0.25, 0.35, 1.0]
            } else {
                [0.18, 0.18, 0.24, 1.0]
            };
            Some((box_border_color, 1.0))
        } else {
            None
        }
    }

    fn as_ptr(&self) -> *mut (dyn Element + 'static) {
        self as *const Self as *mut Self as *mut (dyn Element + 'static)
    }
    fn as_ptr_mut(&mut self) -> *mut (dyn Element + 'static) {
        self as *mut Self as *mut (dyn Element + 'static)
    }

    fn set_hovered(&mut self, v: bool) {
        self.scroll_box.set_hovered(v);
        if !v {
            self.hovered_row = None;
        }
    }

    fn hovered(&self) -> bool {
        self.scroll_box.hovered()
    }

    fn highlight_color(&self, ctx: &UiContext) -> Option<[f32; 4]> {
        self.scroll_box.highlight_color(ctx)
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.scroll_box.cursor_moved(px, py, ctx);
        if self.columns.is_none() {
            return false;
        }
        let (x, _, w, _) = self.rect();
        let item_height_full = self.item_height + self.item_gap;
        
        let mut new_hovered = None;
        for idx in 0..self.rows.len() {
            let virtual_y = idx as f32 * item_height_full + 2.0;
            if let Some(draw_y) = self.scroll_box.get_item_draw_y(virtual_y, self.item_height) {
                if px >= x + 2.0 && px <= x + w - 2.0 && py >= draw_y && py <= draw_y + self.item_height {
                    new_hovered = Some(idx);
                    break;
                }
            }
        }
        
        let changed = self.hovered_row != new_hovered;
        self.hovered_row = new_hovered;
        changed
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.scroll_box.mouse_wheel(delta, px, py, ctx)
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let (x, _, w, _) = self.rect();
        let was_scroll = self.scroll_box.mouse_input(button, state, px, py, ctx);
        
        if self.columns.is_none() || button != MouseButton::Left {
            return was_scroll;
        }

        let item_height_full = self.item_height + self.item_gap;
        let mut clicked_idx = None;
        for idx in 0..self.rows.len() {
            let virtual_y = idx as f32 * item_height_full + 2.0;
            if let Some(draw_y) = self.scroll_box.get_item_draw_y(virtual_y, self.item_height) {
                if px >= x + 2.0 && px <= x + w - 2.0 && py >= draw_y && py <= draw_y + self.item_height {
                    clicked_idx = Some(idx);
                    break;
                }
            }
        }

        match state {
            ElementState::Pressed => {
                if let Some(idx) = clicked_idx {
                    self.pressed_row = Some(idx);
                    self.scroll_box.focus();
                    ctx.set_focused_ptr(self.as_ptr_mut());
                    return true;
                }
            }
            ElementState::Released => {
                let mut clicked = false;
                if let Some(idx) = clicked_idx {
                    if self.pressed_row == Some(idx) {
                        self.scroll_box.focus();
                        let now = std::time::Instant::now();
                        if let Some(last) = self.last_click_time {
                            if now.duration_since(last) < std::time::Duration::from_millis(400) {
                                self.double_clicked_row = Some(idx);
                            }
                        }
                        self.last_click_time = Some(now);
                        self.clicked_row = Some(idx);
                        clicked = true;
                    }
                }
                self.pressed_row = None;
                return clicked || was_scroll;
            }
        }
        was_scroll
    }

    fn focus(&mut self) {
        self.scroll_box.focus();
    }

    fn unfocus(&mut self) {
        self.scroll_box.unfocus();
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.scroll_box.extra_quads();
        if self.columns.is_none() {
            return quads;
        }
        let (x, _, w, _) = self.rect();
        let item_height_full = self.item_height + self.item_gap;

        let (r1, r2, r3, r4) = self.rounded_corners();
        if r1 || r2 || r3 || r4 {
            if !quads.is_empty() {
                quads.remove(0);
            }
        }

        for idx in 0..self.rows.len() {
            let virtual_y = idx as f32 * item_height_full + 2.0;
            if let Some(draw_y) = self.scroll_box.get_item_draw_y(virtual_y, self.item_height) {
                let row = &self.rows[idx];
                let is_hovered = self.hovered_row == Some(idx);
                let is_pressed = self.pressed_row == Some(idx);
                
                let bg_color = if row.selected {
                    if is_pressed { [0.30, 0.52, 0.78, 0.6] }
                    else if is_hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.4] }
                } else {
                    if is_pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if is_hovered { [0.20, 0.20, 0.25, 0.15] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                };

                if bg_color[3] > 0.001 {
                    quads.push((x + 2.0, draw_y, w - 4.0, self.item_height, bg_color));
                }
            }
        }
        quads
    }

    fn all_rounded_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        let mut quads = Vec::new();
        let (r1, r2, r3, r4) = self.rounded_corners();
        let has_rounded = r1 || r2 || r3 || r4;
        if !has_rounded {
            for &child_ptr in &self.children(ctx) {
                let widget = unsafe { &*child_ptr };
                quads.extend(widget.all_rounded_quads(ctx));
            }
            return quads;
        }

        let radius = self.corner_radius();
        let (x, y, w, h) = self.rect();
        
        if let Some((border_color, thickness)) = self.solid_border() {
            quads.push((x, y, w, h, radius, border_color, (r1, r2, r3, r4)));
            quads.push((x + thickness, y + thickness, w - 2.0 * thickness, h - 2.0 * thickness, radius - thickness, crate::color::list_bg_color(), (r1, r2, r3, r4)));
        } else {
            quads.push((x, y, w, h, radius, crate::color::list_bg_color(), (r1, r2, r3, r4)));
        }

        for &child_ptr in &self.children(ctx) {
            let widget = unsafe { &*child_ptr };
            quads.extend(widget.all_rounded_quads(ctx));
        }
        quads
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible() {
            return Vec::new();
        }
        if self.columns.is_none() {
            let font = self.widget_font();
            let mut labels = self.text_labels().into_iter().map(|l| (l, font.clone(), None::<[f32; 4]>)).collect::<Vec<_>>();
            let mut curr = self.parent(ctx);
            let mut scroll_box_bounds = None;
            while let Some(parent_ptr) = curr {
                let parent = unsafe { &*parent_ptr };
                if let Some(scroll_box) = parent.as_any().downcast_ref::<crate::widget::ScrollBox>() {
                    let (sb_x, _, sb_w, _) = scroll_box.rect();
                    let view_min = scroll_box.viewport_y + 4.0;
                    let view_max = scroll_box.viewport_y + scroll_box.viewport_h - 4.0;
                    scroll_box_bounds = Some([sb_x, view_min, sb_x + sb_w, view_max]);
                    break;
                }
                curr = parent.parent(ctx);
            }
            if let Some(sb_bounds) = scroll_box_bounds {
                for item in &mut labels {
                    item.2 = Some(sb_bounds);
                }
            }
            return labels;
        }
        let (x, _, w, _) = self.rect();
        let mut result = Vec::new();

        let item_height_full = self.item_height + self.item_gap;
        let col_bounds = self.get_column_bounds(w);

        let fg = [230, 230, 242];
        let text_dim = [140, 140, 153];
        let list_font = self.widget_font();

        let view_min = self.scroll_box.viewport_y + 4.0;
        let view_max = self.scroll_box.viewport_y + self.scroll_box.viewport_h - 4.0;
        let clipping_bounds = Some([x, view_min, x + w, view_max]);

        for idx in 0..self.rows.len() {
            let virtual_y = idx as f32 * item_height_full + 2.0;
            if let Some(draw_y) = self.scroll_box.get_item_draw_y(virtual_y, self.item_height) {
                let row = &self.rows[idx];
                let row_fg = if row.selected { fg } else { [178, 178, 191] };
                let row_dim = if row.selected { fg } else { text_dim };

                let (_, config_size) = crate::layout::list_font_parsed();
                let primary_size = config_size;
                let secondary_size = (config_size - 1.0).max(8.0);

                let y_primary = crate::layout::center_text_y(draw_y, self.item_height, primary_size);
                let y_secondary = crate::layout::center_text_y(draw_y, self.item_height, secondary_size);

                let mut start_text_offset = 8.0;
                if let Some(ref icon) = row.icon {
                    if !col_bounds.is_empty() {
                        result.push((
                            TextLabel {
                                text: icon.clone(),
                                x: x + col_bounds[0].0 + 12.0,
                                y: y_primary,
                                font_size: primary_size,
                                color: row_fg,
                            },
                            list_font.clone(),
                            clipping_bounds,
                        ));
                        start_text_offset = 32.0;
                    }
                }

                for (c_idx, cell_text) in row.cells.iter().enumerate() {
                    if c_idx >= col_bounds.len() {
                        break;
                    }
                    let (col_x, col_w) = col_bounds[c_idx];
                    if col_w <= 0.0 {
                        continue;
                    }

                    let cell_color = if c_idx == 0 { row_fg } else { row_dim };
                    let cell_y = if c_idx == 0 { y_primary } else { y_secondary };
                    let cell_size = if c_idx == 0 { primary_size } else { secondary_size };

                    let cell_draw_x = if c_idx == 0 {
                        x + col_x + start_text_offset
                    } else {
                        x + col_x
                    };

                    let max_w = if c_idx == 0 {
                        col_w - start_text_offset - 8.0
                    } else {
                        col_w - 8.0
                    };
                    let char_w = cell_size * 0.65;
                    let max_chars = (max_w / char_w).max(4.0) as usize;
                    let cell_text_truncated = if cell_text.chars().count() > max_chars {
                        let mut s: String = cell_text.chars().take(max_chars - 3).collect();
                        s.push_str("...");
                        s
                    } else {
                        cell_text.clone()
                    };

                    result.push((
                        TextLabel {
                            text: cell_text_truncated,
                            x: cell_draw_x,
                            y: cell_y,
                            font_size: cell_size,
                            color: cell_color,
                        },
                        list_font.clone(),
                        clipping_bounds,
                    ));
                }
            }
        }
        result
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if self.scroll_box.keyboard_input(event, ctx) {
            return true;
        }
        if self.columns.is_none() {
            return false;
        }
        if event.state == ElementState::Pressed {
            let mut current_selected = None;
            for (idx, r) in self.rows.iter().enumerate() {
                if r.selected {
                    current_selected = Some(idx);
                    break;
                }
            }

            let mut next_selected = None;
            if event.logical_key == Key::Named(NamedKey::ArrowDown) {
                if let Some(curr) = current_selected {
                    if curr + 1 < self.rows.len() {
                        next_selected = Some(curr + 1);
                    }
                } else if !self.rows.is_empty() {
                    next_selected = Some(0);
                }
            } else if event.logical_key == Key::Named(NamedKey::ArrowUp) {
                if let Some(curr) = current_selected {
                    if curr > 0 {
                        next_selected = Some(curr - 1);
                    }
                }
            }

            if let Some(next) = next_selected {
                for (idx, r) in self.rows.iter_mut().enumerate() {
                    r.selected = idx == next;
                }
                self.clicked_row = Some(next);

                let item_height_full = self.item_height + self.item_gap;
                let item_y = next as f32 * item_height_full + 2.0;
                let viewport_h = self.scroll_box.viewport_h;

                if item_y < self.scroll_box.scroll_y {
                    self.scroll_box.scroll_y = item_y;
                } else if item_y + self.item_height > self.scroll_box.scroll_y + viewport_h {
                    self.scroll_box.scroll_y = item_y + self.item_height - viewport_h;
                }

                self.mark_dirty(ctx);
                return true;
            }
        }
        false
    }

    fn widget_font(&self) -> Option<String> {
        let f = crate::layout::list_font();
        if f.is_empty() {
            None
        } else {
            Some(f)
        }
    }

    fn parent(&self, _ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> { self.scroll_box.parent(_ctx) }
    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) { self.scroll_box.set_parent(parent, ctx); }
    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> { self.scroll_box.children(ctx) }
    fn add_child(&mut self, child: *mut (dyn Element + 'static), ctx: &mut UiContext) { self.scroll_box.add_child(child, ctx); }
    fn clear_children(&mut self, ctx: &mut UiContext) { self.scroll_box.clear_children(ctx); }
}

unsafe impl Send for List {}
unsafe impl Sync for List {}
