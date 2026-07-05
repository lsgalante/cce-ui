use crate::widget::Element;

pub struct ColumnLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub gap: f32,
    pub margin: f32,
    current_y: f32,
}

impl ColumnLayout {
    pub fn new(x: f32, y: f32, width: f32, gap: f32, margin: f32) -> Self {
        Self {
            x,
            y,
            width,
            gap,
            margin,
            current_y: y + margin,
        }
    }

    pub fn add_widget(&mut self, widget: &mut dyn Element, height: f32) {
        let label_off = widget.base().map_or(0.0, |b| b.label_offset());
        let total_h = height + label_off;
        widget.set_rect(self.x + self.margin, self.current_y, self.width - 2.0 * self.margin, height);
        self.current_y += total_h + self.gap;
    }

    pub fn add_row(&mut self, widgets: &[*mut dyn Element], height: f32, gap: f32) {
        let count = widgets.len();
        if count == 0 {
            return;
        }
        let mut max_label_off = 0.0;
        for &widget_ptr in widgets {
            unsafe {
                let off = (*widget_ptr).base().map_or(0.0, |b| b.label_offset());
                if off > max_label_off {
                    max_label_off = off;
                }
            }
        }
        let total_h = height + max_label_off;
        let total_width = self.width - 2.0 * self.margin;
        let widget_w = (total_width - (count as f32 - 1.0) * gap) / count as f32;
        let mut curr_x = self.x + self.margin;
        for &widget_ptr in widgets {
            unsafe {
                (*widget_ptr).set_rect(curr_x, self.current_y, widget_w, height);
            }
            curr_x += widget_w + gap;
        }
        self.current_y += total_h + self.gap;
    }

    pub fn current_y(&self) -> f32 {
        self.current_y - self.gap + self.margin
    }
}

pub struct RowLayout {
    pub x: f32,
    pub y: f32,
    pub height: f32,
    pub gap: f32,
    pub margin: f32,
    current_x: f32,
}

impl RowLayout {
    pub fn new(x: f32, y: f32, height: f32, gap: f32, margin: f32) -> Self {
        Self {
            x,
            y,
            height,
            gap,
            margin,
            current_x: x + margin,
        }
    }

    pub fn add_widget(&mut self, widget: &mut dyn Element, width: f32) {
        widget.set_rect(self.current_x, self.y + self.margin, width, self.height - 2.0 * self.margin);
        self.current_x += width + self.gap;
    }

    pub fn current_x(&self) -> f32 {
        self.current_x - self.gap + self.margin
    }
}
