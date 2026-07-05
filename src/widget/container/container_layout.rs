use crate::widget::*;
use crate::context::UiContext;

pub trait ContainerLayout: crate::layout::LayoutStrategy {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout>;
}

impl Clone for Box<dyn ContainerLayout> {
    fn clone(&self) -> Self {
        self.box_clone_container()
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlayLayout {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

impl crate::layout::LayoutStrategy for OverlayLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32) {
        self.left = left;
        self.top = top;
        self.width = width;
        self.height = height;
    }

    fn allocate(&mut self, _ww: f32, _wh: f32) -> (f32, f32, f32, f32) {
        (self.left, self.top, self.width, self.height)
    }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], _ctx: &mut UiContext) -> f32 {
        for &child_ptr in children {
            unsafe {
                (*child_ptr).set_rect(x, y, w, h);
            }
        }
        h
    }

    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size {
        let mut max_w = 0.0f32;
        let mut max_h = 0.0f32;
        for &child_ptr in children {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                max_w = max_w.max(size.width);
                max_h = max_h.max(size.height);
            }
        }
        Size {
            width: max_w.clamp(constraints.min_width, constraints.max_width),
            height: max_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn crate::layout::LayoutStrategy> {
        Box::new(*self)
    }
}

impl ContainerLayout for OverlayLayout {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VerticalLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
    left: f32,
    current_y: f32,
}

impl Default for VerticalLayout {
    fn default() -> Self {
        Self {
            padding_x: 0.0,
            padding_y: 0.0,
            spacing: 8.0,
            left: 0.0,
            current_y: 0.0,
        }
    }
}

impl crate::layout::LayoutStrategy for VerticalLayout {
    fn init(&mut self, left: f32, top: f32, _width: f32, _height: f32) {
        self.left = left;
        self.current_y = top + self.padding_y;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        let x = self.left + self.padding_x;
        let y = self.current_y;
        self.current_y += wh + self.spacing;
        (x, y, ww, wh)
    }

    fn get_gap(&self) -> f32 {
        self.spacing
    }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let left_x = x + self.padding_x;
        let available_w = (w - 2.0 * self.padding_x).max(1.0);
        let mut current_y = y + self.padding_y;

        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                child.layout(
                    Point { x: left_x, y: current_y },
                    LayoutConstraints::new(available_w, available_w, use_h, use_h),
                    ctx,
                );
                current_y += use_h + self.spacing;
            }
        }
        (current_y - y).max(0.0)
    }

    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size {
        let mut total_h = self.padding_y * 2.0;
        let mut max_w = 0.0f32;
        let spacing = self.spacing;

        for (i, &child_ptr) in children.iter().enumerate() {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                max_w = max_w.max(size.width);
                total_h += size.height;
                if i > 0 {
                    total_h += spacing;
                }
            }
        }

        Size {
            width: (max_w + self.padding_x * 2.0).clamp(constraints.min_width, constraints.max_width),
            height: total_h.clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn crate::layout::LayoutStrategy> {
        Box::new(*self)
    }
}

impl ContainerLayout for VerticalLayout {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}

#[derive(Debug, Clone)]
pub struct GridLayout {
    pub columns: usize,
    pub gap: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub grid: Option<crate::layout::Grid>,
}

impl crate::layout::LayoutStrategy for GridLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        let count = self.columns.max(1);
        let usable_w = (width - 2.0 * self.padding_x).max(1.0);
        let grid = crate::layout::Grid::new(
            left + self.padding_x,
            top + self.padding_y,
            usable_w,
            usable_w / count as f32,
            self.gap,
            count,
        );
        self.grid = Some(grid);
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref mut grid) = self.grid {
            let col = grid.next_column();
            let x = grid.col_lefts[col];
            let y = grid.col_heights[col];
            grid.col_heights[col] += wh + grid.gap;
            (x, y, grid.col_width, wh)
        } else {
            (0.0, 0.0, ww, wh)
        }
    }

    fn get_column_width(&self) -> Option<f32> {
        self.grid.as_ref().map(|g| g.col_width)
    }

    fn get_gap(&self) -> f32 {
        self.gap
    }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let count = children.len();
        if count == 0 {
            return 0.0;
        }
        let cols = self.columns.max(1);
        let total_gap = self.gap * (cols - 1) as f32;
        let available_w = (w - 2.0 * self.padding_x - total_gap).max(1.0);
        let col_w = available_w / cols as f32;
        
        let mut col_heights = vec![y + self.padding_y; cols];

        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                
                let mut min_col = 0;
                let mut min_h = col_heights[0];
                for i in 1..cols {
                    if col_heights[i] < min_h {
                        min_h = col_heights[i];
                        min_col = i;
                    }
                }
                
                let cx = x + self.padding_x + min_col as f32 * (col_w + self.gap);
                let cy = col_heights[min_col];
                child.layout(
                    Point { x: cx, y: cy },
                    LayoutConstraints::new(col_w, col_w, use_h, use_h),
                    ctx,
                );
                col_heights[min_col] += use_h + self.gap;
            }
        }
        
        let max_h = col_heights.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        (max_h - y).max(0.0)
    }

    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size {
        let cols = self.columns.max(1);
        let mut col_heights = vec![self.padding_y; cols];
        let total_gap = self.gap * (cols - 1) as f32;
        let available_w = (constraints.max_width - 2.0 * self.padding_x - total_gap).max(1.0);
        let col_w = available_w / cols as f32;
        
        let child_constraints = LayoutConstraints::new(col_w, col_w, constraints.min_height, constraints.max_height);

        for &child_ptr in children {
            unsafe {
                let size = (*child_ptr).measure(child_constraints, ctx);
                let mut min_col = 0;
                let mut min_h = col_heights[0];
                for i in 1..cols {
                    if col_heights[i] < min_h {
                        min_h = col_heights[i];
                        min_col = i;
                    }
                }
                col_heights[min_col] += size.height + self.gap;
            }
        }
        
        let max_h = col_heights.iter().cloned().fold(0.0f32, |a, b| a.max(b));
        Size {
            width: constraints.max_width,
            height: (max_h + self.padding_y).clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn crate::layout::LayoutStrategy> {
        Box::new(self.clone())
    }
}

impl ContainerLayout for GridLayout {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone)]
pub struct AdaptiveGridLayout {
    pub min_col_width: f32,
    pub gap: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub grid: Option<crate::layout::Grid>,
}

impl crate::layout::LayoutStrategy for AdaptiveGridLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        let usable_w = (width - 2.0 * self.padding_x).max(1.0);
        let cols = (((usable_w + self.gap) / (self.min_col_width + self.gap)).floor().max(1.0)) as usize;
        let grid = crate::layout::Grid::new(
            left + self.padding_x,
            top + self.padding_y,
            usable_w,
            self.min_col_width,
            self.gap,
            cols,
        );
        self.grid = Some(grid);
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref mut grid) = self.grid {
            let col = grid.next_column();
            let x = grid.col_lefts[col];
            let y = grid.col_heights[col];
            grid.col_heights[col] += wh + grid.gap;
            (x, y, grid.col_width, wh)
        } else {
            (0.0, 0.0, ww, wh)
        }
    }

    fn get_column_width(&self) -> Option<f32> {
        self.grid.as_ref().map(|g| g.col_width)
    }

    fn get_gap(&self) -> f32 {
        self.gap
    }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let usable_w = (w - 2.0 * self.padding_x).max(1.0);
        let cols = (((usable_w + self.gap) / (self.min_col_width + self.gap)).floor().max(1.0)) as usize;
        let grid = GridLayout {
            columns: cols,
            gap: self.gap,
            padding_x: self.padding_x,
            padding_y: self.padding_y,
            grid: None,
        };
        grid.layout(x, y, w, h, children, ctx)
    }

    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size {
        let usable_w = (constraints.max_width - 2.0 * self.padding_x).max(1.0);
        let cols = (((usable_w + self.gap) / (self.min_col_width + self.gap)).floor().max(1.0)) as usize;
        let grid = GridLayout {
            columns: cols,
            gap: self.gap,
            padding_x: self.padding_x,
            padding_y: self.padding_y,
            grid: None,
        };
        grid.measure(constraints, children, ctx)
    }

    fn box_clone(&self) -> Box<dyn crate::layout::LayoutStrategy> {
        Box::new(self.clone())
    }
}

impl ContainerLayout for AdaptiveGridLayout {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout> {
        Box::new(self.clone())
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ColumnsLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
}

impl Default for ColumnsLayout {
    fn default() -> Self {
        Self {
            padding_x: 8.0,
            padding_y: 10.0,
            spacing: 12.0,
        }
    }
}

impl crate::layout::LayoutStrategy for ColumnsLayout {
    fn init(&mut self, _left: f32, _top: f32, _width: f32, _height: f32) {}
    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) { (0.0, 0.0, ww, wh) }
    fn get_gap(&self) -> f32 { self.spacing }

    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let count = children.len();
        if count == 0 {
            return 0.0;
        }
        let total_spacing = self.spacing * (count - 1) as f32;
        let total_padding = self.padding_x * 2.0;
        let available_w = (w - total_padding - total_spacing).max(1.0);
        let col_w = available_w / count as f32;
        let use_h = (h - 2.0 * self.padding_y).max(1.0);
        let start_y = y + self.padding_y;

        let mut current_x = x + self.padding_x;
        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                child.layout(
                    Point { x: current_x, y: start_y },
                    LayoutConstraints::new(col_w, col_w, use_h, use_h),
                    ctx,
                );
                current_x += col_w + self.spacing;
            }
        }
        use_h + 2.0 * self.padding_y
    }

    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size {
        let count = children.len();
        if count == 0 {
            return Size { width: constraints.min_width, height: constraints.min_height };
        }
        let mut max_h = 0.0f32;
        for &child_ptr in children {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                max_h = max_h.max(size.height);
            }
        }
        Size {
            width: constraints.max_width,
            height: (max_h + 2.0 * self.padding_y).clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn crate::layout::LayoutStrategy> {
        Box::new(*self)
    }
}

impl ContainerLayout for ColumnsLayout {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MosaicLayout {
    pub gap: f32,
    pub padding_x: f32,
    pub padding_y: f32,
}

impl Default for MosaicLayout {
    fn default() -> Self {
        Self {
            gap: 8.0,
            padding_x: 8.0,
            padding_y: 10.0,
        }
    }
}

struct Packer {
    free_rects: Vec<(f32, f32, f32, f32)>, // (x, y, w, h)
    max_w: f32,
    max_h: f32,
    gap: f32,
}

impl Packer {
    fn new(start_x: f32, start_y: f32, max_width: f32, gap: f32) -> Self {
        Self {
            free_rects: vec![(start_x, start_y, max_width, 100000.0)],
            max_w: max_width,
            max_h: 0.0,
            gap,
        }
    }

    fn pack(&mut self, cw: f32, ch: f32) -> (f32, f32) {
        let cw_clamped = cw.min(self.max_w);
        
        let mut best_idx = None;
        let mut best_y = f32::MAX;
        let mut best_x = f32::MAX;

        for (idx, &(rx, ry, rw, rh)) in self.free_rects.iter().enumerate() {
            if rw >= cw_clamped && rh >= ch {
                if ry < best_y || (ry == best_y && rx < best_x) {
                    best_y = ry;
                    best_x = rx;
                    best_idx = Some(idx);
                }
            }
        }

        let chosen_idx = match best_idx {
            Some(idx) => idx,
            None => {
                let new_y = self.max_h + self.gap;
                let new_rect = (self.free_rects[0].0, new_y, self.max_w, 100000.0);
                self.free_rects.push(new_rect);
                self.free_rects.len() - 1
            }
        };

        let (fx, fy, fw, fh) = self.free_rects.remove(chosen_idx);
        let px = fx;
        let py = fy;

        let rx = fx + cw_clamped + self.gap;
        let rw = fw - cw_clamped - self.gap;
        if rw > 0.0 && ch > 0.0 {
            self.free_rects.push((rx, py, rw, ch));
        }

        let by = py + ch + self.gap;
        let bh = fh - ch - self.gap;
        if bh > 0.0 && fw > 0.0 {
            self.free_rects.push((fx, by, fw, bh));
        }

        self.max_h = self.max_h.max(py + ch);

        (px, py)
    }
}

impl crate::layout::LayoutStrategy for MosaicLayout {
    fn init(&mut self, _left: f32, _top: f32, _width: f32, _height: f32) {}
    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) { (0.0, 0.0, ww, wh) }
    fn get_gap(&self) -> f32 { self.gap }

    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let count = children.len();
        if count == 0 {
            return 0.0;
        }
        let total_padding_x = self.padding_x * 2.0;
        let available_w = (w - total_padding_x).max(1.0);

        let mut packer = Packer::new(x + self.padding_x, y + self.padding_y, available_w, self.gap);

        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let child_rect = child.rect();
                let child_w = child_rect.2;
                let child_h = child.preferred_height().unwrap_or(child_rect.3);
                let use_h = if child_h > 0.0 { child_h } else { 44.0 };
                
                let (px, py) = packer.pack(child_w, use_h);
                child.layout(
                    Point { x: px, y: py },
                    LayoutConstraints::new(child_w.min(available_w), child_w.min(available_w), use_h, use_h),
                    ctx,
                );
            }
        }

        (packer.max_h - y).max(0.0)
    }

    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size {
        let count = children.len();
        if count == 0 {
            return Size { width: constraints.min_width, height: constraints.min_height };
        }
        let total_padding_x = self.padding_x * 2.0;
        let available_w = (constraints.max_width - total_padding_x).max(1.0);

        let mut packer = Packer::new(self.padding_x, self.padding_y, available_w, self.gap);

        for &child_ptr in children {
            unsafe {
                let size = (*child_ptr).measure(constraints, ctx);
                packer.pack(size.width, size.height);
            }
        }

        Size {
            width: constraints.max_width,
            height: (packer.max_h + self.padding_y).clamp(constraints.min_height, constraints.max_height),
        }
    }

    fn box_clone(&self) -> Box<dyn crate::layout::LayoutStrategy> {
        Box::new(*self)
    }
}

impl ContainerLayout for MosaicLayout {
    fn box_clone_container(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}
