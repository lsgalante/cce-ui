use crate::widget::*;
use crate::context::UiContext;

pub trait ContainerLayout: std::fmt::Debug {
    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32;
    fn measure(&self, constraints: LayoutConstraints, children: &[*mut (dyn Element + 'static)], ctx: &UiContext) -> Size;
    fn box_clone(&self) -> Box<dyn ContainerLayout>;
}

impl Clone for Box<dyn ContainerLayout> {
    fn clone(&self) -> Self {
        self.box_clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OverlayLayout;

impl ContainerLayout for OverlayLayout {
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

    fn box_clone(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct VerticalLayout {
    pub padding_x: f32,
    pub padding_y: f32,
    pub spacing: f32,
}

impl Default for VerticalLayout {
    fn default() -> Self {
        Self {
            padding_x: 0.0,
            padding_y: 0.0,
            spacing: 8.0,
        }
    }
}

impl ContainerLayout for VerticalLayout {
    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)], _ctx: &mut UiContext) -> f32 {
        let left_x = x + self.padding_x;
        let available_w = (w - 2.0 * self.padding_x).max(1.0);
        let mut current_y = y + self.padding_y;

        for &child_ptr in children {
            unsafe {
                let child = &mut *child_ptr;
                let ch = child.preferred_height().unwrap_or(child.rect().3);
                let use_h = if ch > 0.0 { ch } else { 44.0 };
                child.set_rect(left_x, current_y, available_w, use_h);
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

    fn box_clone(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GridLayout {
    pub columns: usize,
    pub gap: f32,
    pub padding_x: f32,
    pub padding_y: f32,
}

impl ContainerLayout for GridLayout {
    fn layout(&self, x: f32, y: f32, w: f32, _h: f32, children: &[*mut (dyn Element + 'static)], _ctx: &mut UiContext) -> f32 {
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
                child.set_rect(cx, cy, col_w, use_h);
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

    fn box_clone(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AdaptiveGridLayout {
    pub min_col_width: f32,
    pub gap: f32,
    pub padding_x: f32,
    pub padding_y: f32,
}

impl ContainerLayout for AdaptiveGridLayout {
    fn layout(&self, x: f32, y: f32, w: f32, h: f32, children: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) -> f32 {
        let usable_w = (w - 2.0 * self.padding_x).max(1.0);
        let cols = (((usable_w + self.gap) / (self.min_col_width + self.gap)).floor().max(1.0)) as usize;
        let grid = GridLayout {
            columns: cols,
            gap: self.gap,
            padding_x: self.padding_x,
            padding_y: self.padding_y,
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
        };
        grid.measure(constraints, children, ctx)
    }

    fn box_clone(&self) -> Box<dyn ContainerLayout> {
        Box::new(*self)
    }
}
