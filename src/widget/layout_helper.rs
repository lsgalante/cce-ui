use crate::widget::WidgetHost;

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

    /// A column inside a pane plate: the pane rung's padding as the margin
    /// and its gap between widgets (`plate_padding` / `plate_gap`).
    pub fn pane(x: f32, y: f32, width: f32) -> Self {
        Self::new(x, y, width, crate::layout::plate_gap(), crate::layout::plate_padding())
    }

    /// A column of controls with the control gap between them and no
    /// margin of its own (`control_gap`).
    pub fn controls(x: f32, y: f32, width: f32) -> Self {
        Self::new(x, y, width, crate::layout::control_gap(), 0.0)
    }

    /// `height` is the CONTENT height; the widget's block adds its label strip.
    pub fn add_widget(&mut self, widget: &mut dyn WidgetHost, height: f32) {
        let total_h = height + widget.label_strip();
        widget.set_rect(self.x + self.margin, self.current_y, self.width - 2.0 * self.margin, total_h);
        self.current_y += total_h + self.gap;
    }

    pub fn add_row(&mut self, widgets: &[*mut dyn WidgetHost], height: f32, gap: f32) {
        let count = widgets.len();
        if count == 0 {
            return;
        }
        let mut max_label_off = 0.0;
        for &widget_ptr in widgets {
            unsafe {
                let off = (*widget_ptr).label_strip();
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
                // Content lines up on one row; a shorter label strip starts lower.
                let off = (*widget_ptr).label_strip();
                (*widget_ptr).set_rect(curr_x, self.current_y + max_label_off - off, widget_w, height + off);
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

    pub fn add_widget(&mut self, widget: &mut dyn WidgetHost, width: f32) {
        widget.set_rect(self.current_x, self.y + self.margin, width, self.height - 2.0 * self.margin);
        self.current_x += width + self.gap;
    }

    pub fn current_x(&self) -> f32 {
        self.current_x - self.gap + self.margin
    }
}
