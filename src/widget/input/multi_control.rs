use crate::colors;
use crate::widget::*;
use crate::widget::input::{TextBox, Spinbox, Dropdown, Button, Toggle, Slider};
use crate::widget::TextLabel;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct InstancedControl {
    pub key: String,
    pub control_type: String,
    pub value: String,
}

#[derive(Clone, Debug)]
pub enum InstancedWidget {
    TextBox(TextBox),
    Spinbox(Adapted<Spinbox>),
    Toggle(Adapted<Toggle>),
    Slider(Adapted<Slider>),
}

impl InstancedWidget {
    pub fn get_value_string(&self) -> Option<String> {
        match self {
            InstancedWidget::TextBox(w) => w.get_value_string(),
            InstancedWidget::Spinbox(w) => w.get_value_string(),
            InstancedWidget::Toggle(w) => w.get_value_string(),
            InstancedWidget::Slider(w) => w.get_value_string(),
        }
    }

    pub fn set_value_string(&mut self, val: &str) -> bool {
        match self {
            InstancedWidget::TextBox(w) => w.set_value_string(val),
            InstancedWidget::Spinbox(w) => w.set_value_string(val),
            InstancedWidget::Toggle(w) => w.set_value_string(val),
            InstancedWidget::Slider(w) => w.set_value_string(val),
        }
    }

    pub fn take_change(&mut self) -> bool {
        match self {
            InstancedWidget::TextBox(w) => w.take_change(),
            InstancedWidget::Spinbox(w) => w.take_change(),
            InstancedWidget::Toggle(w) => w.take_change(),
            InstancedWidget::Slider(w) => w.take_change(),
        }
    }

    pub fn preferred_height(&self) -> Option<f32> {
        match self {
            InstancedWidget::TextBox(w) => w.preferred_height(),
            InstancedWidget::Spinbox(w) => w.preferred_height(),
            InstancedWidget::Toggle(w) => w.preferred_height(),
            InstancedWidget::Slider(w) => w.preferred_height(),
        }
    }

    pub fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        match self {
            InstancedWidget::TextBox(w) => w.layout(origin, constraints, ctx),
            InstancedWidget::Spinbox(w) => w.layout(origin, constraints, ctx),
            InstancedWidget::Toggle(w) => w.layout(origin, constraints, ctx),
            InstancedWidget::Slider(w) => w.layout(origin, constraints, ctx),
        }
    }

    pub fn rect(&self) -> (f32, f32, f32, f32) {
        match self {
            InstancedWidget::TextBox(w) => w.rect(),
            InstancedWidget::Spinbox(w) => w.rect(),
            InstancedWidget::Toggle(w) => w.rect(),
            InstancedWidget::Slider(w) => w.rect(),
        }
    }

    pub fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        match self {
            InstancedWidget::TextBox(w) => w.all_quads(ctx),
            InstancedWidget::Spinbox(w) => w.all_quads(ctx),
            InstancedWidget::Toggle(w) => w.all_quads(ctx),
            InstancedWidget::Slider(w) => w.all_quads(ctx),
        }
    }

    pub fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        match self {
            InstancedWidget::TextBox(w) => w.text_labels_with_bounds(ctx),
            InstancedWidget::Spinbox(w) => w.text_labels_with_bounds(ctx),
            InstancedWidget::Toggle(w) => w.text_labels_with_bounds(ctx),
            InstancedWidget::Slider(w) => w.text_labels_with_bounds(ctx),
        }
    }

    pub fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        match self {
            InstancedWidget::TextBox(w) => w.text_labels_with_font_and_bounds(ctx),
            InstancedWidget::Spinbox(w) => w.text_labels_with_font_and_bounds(ctx),
            InstancedWidget::Toggle(w) => w.text_labels_with_font_and_bounds(ctx),
            InstancedWidget::Slider(w) => w.text_labels_with_font_and_bounds(ctx),
        }
    }

    pub fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        match self {
            InstancedWidget::TextBox(w) => w.handle_event(event, ctx),
            InstancedWidget::Spinbox(w) => w.handle_event(event, ctx),
            InstancedWidget::Toggle(w) => w.handle_event(event, ctx),
            InstancedWidget::Slider(w) => w.handle_event(event, ctx),
        }
    }

    pub fn unfocus(&mut self) {
        match self {
            InstancedWidget::TextBox(w) => w.unfocus(),
            InstancedWidget::Spinbox(w) => w.unfocus(),
            InstancedWidget::Toggle(w) => w.unfocus(),
            InstancedWidget::Slider(w) => w.unfocus(),
        }
    }

    pub fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        match self {
            InstancedWidget::TextBox(w) => w.hit_test(px, py, ctx),
            InstancedWidget::Spinbox(w) => w.hit_test(px, py, ctx),
            InstancedWidget::Toggle(w) => w.hit_test(px, py, ctx),
            InstancedWidget::Slider(w) => w.hit_test(px, py, ctx),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MultiControlRow {
    pub key_input: TextBox,
    pub type_dropdown: Dropdown,
    pub value_widget: InstancedWidget,
    pub remove_button: Adapted<Button>,
    pub layout_y: f32,
    pub layout_height: f32,
    pub natural_y: f32,
}

impl MultiControlRow {
    pub fn new(key: String, control_type: String, value: String) -> Self {
        let key_input = TextBox::new(key);
        let dropdown_options = vec![
            "TextBox".to_string(),
            "Spinbox".to_string(),
            "Toggle".to_string(),
            "Slider".to_string(),
        ];
        let type_idx = match control_type.as_str() {
            "Spinbox" => 1,
            "Toggle" => 2,
            "Slider" => 3,
            _ => 0,
        };
        let type_dropdown = Dropdown::new(dropdown_options, type_idx);

        let value_widget = match type_idx {
            1 => {
                let initial_val = value.parse::<i32>().unwrap_or(0);
                InstancedWidget::Spinbox(Spinbox::new(initial_val, 0, 1000000, 1))
            }
            2 => {
                let initial_val = value.trim().to_lowercase();
                let toggled = initial_val == "true" || initial_val == "1" || initial_val == "yes" || initial_val == "on";
                let mut tg = Toggle::new();
                tg.set_toggled(toggled);
                InstancedWidget::Toggle(tg)
            }
            3 => {
                let mut sl = Slider::new();
                sl.set_value_string(&value);
                InstancedWidget::Slider(sl)
            }
            _ => {
                InstancedWidget::TextBox(TextBox::new(value))
            }
        };

        let remove_button = Button::new(0.0, 0.0, 0.0, 0.0).with_label("Remove");

        Self {
            key_input,
            type_dropdown,
            value_widget,
            remove_button,
            layout_y: 0.0,
            layout_height: 0.0,
            natural_y: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MultiControl {
    pub base: Widget,
    pub name: String,
    pub rows: Vec<MultiControlRow>,
    pub add_button: Adapted<Button>,
    pub just_changed: bool,
    pub add_popover_open: bool,
    pub add_popover_hovered_idx: Option<usize>,
    pub active_drag_index: Option<usize>,
    pub drag_start_mouse_y: f32,
    pub drag_y_offset: f32,
    pub hovered_drag_index: Option<usize>,
}

impl MultiControl {
    pub fn new(name: String) -> Self {
        let mut mc = Self {
            base: Widget::new(),
            name,
            rows: Vec::new(),
            add_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Add Widget +"),
            just_changed: false,
            add_popover_open: false,
            add_popover_hovered_idx: None,
            active_drag_index: None,
            drag_start_mouse_y: 0.0,
            drag_y_offset: 0.0,
            hovered_drag_index: None,
        };
        mc.load_from_config();
        mc
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self.load_from_config();
        self
    }

    pub fn load_from_config(&mut self) {
        let name = if self.base.label.is_some() {
            self.base.label.as_ref().unwrap()
        } else {
            &self.name
        };
        let loaded = load_config(name);
        self.rows = loaded.into_iter().map(|c| {
            MultiControlRow::new(c.key, c.control_type, c.value)
        }).collect();
    }

    pub fn save_to_config(&self) {
        let name = if self.base.label.is_some() {
            self.base.label.as_ref().unwrap()
        } else {
            &self.name
        };
        let controls: Vec<InstancedControl> = self.rows.iter().map(|row| {
            let key = row.key_input.get_value_string().unwrap_or_default();
            let control_type = match row.type_dropdown.selected {
                1 => "Spinbox".to_string(),
                2 => "Toggle".to_string(),
                3 => "Slider".to_string(),
                _ => "TextBox".to_string(),
            };
            let value = row.value_widget.get_value_string().unwrap_or_default();
            InstancedControl { key, control_type, value }
        }).collect();
        save_config(name, &controls);
    }

    pub fn get_drag_handle_quads(&self, idx: usize, theme: colors::Theme) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if idx >= self.rows.len() {
            return quads;
        }
        let row = &self.rows[idx];
        let (rx, _, _, _) = self.rect();
        let pad_x = 8.0;
        let drag_handle_w = 20.0;
        
        let key_h = crate::layout::textbox_height();
        let type_h = crate::layout::dropdown_height();
        let line1_h = key_h.max(type_h);
        
        let grip_center_y = row.layout_y + line1_h * 0.5;
        let grip_center_x = rx + pad_x + drag_handle_w * 0.5;
        
        // Let's decide color based on hover / drag state
        let color = if Some(idx) == self.active_drag_index {
            theme.primary_accent
        } else if Some(idx) == self.hovered_drag_index {
            [
                (theme.primary_accent[0] + 0.1).min(1.0),
                (theme.primary_accent[1] + 0.1).min(1.0),
                (theme.primary_accent[2] + 0.1).min(1.0),
                0.8
            ]
        } else {
            [
                theme.surface_border[0],
                theme.surface_border[1],
                theme.surface_border[2],
                0.5
            ]
        };

        // Let's draw 3 horizontal bars for the grip
        let bar_w = 10.0;
        let bar_h = 2.0;
        let bar_gap = 2.0;
        let total_grip_h = 3.0 * bar_h + 2.0 * bar_gap;
        let start_y = grip_center_y - total_grip_h * 0.5;
        let start_x = grip_center_x - bar_w * 0.5;

        for i in 0..3 {
            let y = start_y + i as f32 * (bar_h + bar_gap);
            quads.push((start_x, y, bar_w, bar_h, color));
        }

        quads
    }
}

fn link_child(parent_ptr: *mut (dyn Element + 'static), parent_id: WidgetId, child: &mut dyn Element, ctx: &mut UiContext) {
    let c_ptr = child.as_ptr();
    if let Some(c_base) = child.base() {
        let c_id = c_base.id();
        ctx.register_widget(parent_id, parent_ptr);
        ctx.register_widget(c_id, c_ptr);
        ctx.link_ids(parent_id, c_id);
    }
    child.set_parent(Some(parent_ptr), ctx);
}

impl Element for MultiControl {
    crate::impl_widget_base!(MultiControl);

    fn preferred_height(&self) -> Option<f32> {
        let gap_between_lines = 4.0;
        let gap_between_rows = 12.0;
        let add_btn_h = 36.0;

        let key_h = crate::layout::textbox_height();
        let type_h = crate::layout::dropdown_height();
        let line1_h = key_h.max(type_h);

        let mut total_h = 0.0;
        if self.rows.is_empty() {
            total_h = add_btn_h + 16.0;
        } else {
            for row in &self.rows {
                let line2_h = row.value_widget.preferred_height().unwrap_or(44.0);
                total_h += line1_h + gap_between_lines + line2_h;
            }
            total_h += (self.rows.len() - 1) as f32 * gap_between_rows + add_btn_h + 28.0;
        }
        Some(total_h)
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        self.set_rect(origin.x, origin.y, size.width, size.height);

        let pad_x = 8.0;
        let pad_y = 8.0;
        let gap_between_lines = 4.0;
        let gap_between_rows = 12.0;

        let key_h = crate::layout::textbox_height();
        let type_h = crate::layout::dropdown_height();
        let line1_h = key_h.max(type_h);

        let drag_handle_w = 20.0;
        let drag_gap_x = 6.0;
        let left_shift = drag_handle_w + drag_gap_x;

        let usable_w = (size.width - 2.0 * pad_x - left_shift).max(1.0);
        let gap_x = 6.0;
        let top_usable_w = usable_w - 2.0 * gap_x;

        let key_w = top_usable_w * 0.50;
        let type_w = top_usable_w * 0.32;
        let remove_w = top_usable_w * 0.18;

        let mut curr_y = origin.y + pad_y;
        let self_ptr = self.as_ptr();
        let self_id = self.base.id();

        // Calculate rows area height for clamping
        let mut total_rows_h = 0.0;
        for (i, row) in self.rows.iter().enumerate() {
            let line2_h = row.value_widget.preferred_height().unwrap_or(44.0);
            let row_h = line1_h + gap_between_lines + line2_h;
            total_rows_h += row_h;
            if i < self.rows.len() - 1 {
                total_rows_h += gap_between_rows;
            }
        }
        let top_limit = origin.y + pad_y;
        let bottom_limit = top_limit + total_rows_h;

        for (i, row) in self.rows.iter_mut().enumerate() {
            let line2_h = row.value_widget.preferred_height().unwrap_or(44.0);
            let row_h = line1_h + gap_between_lines + line2_h;
            row.layout_height = row_h;
            row.natural_y = curr_y;

            let mut row_y = curr_y;
            if Some(i) == self.active_drag_index {
                row_y = (curr_y + self.drag_y_offset).clamp(top_limit, (bottom_limit - row_h).max(top_limit));
            }
            row.layout_y = row_y;

            // Line 1: Label, Type, Remove
            let key_x = origin.x + pad_x + left_shift;
            row.key_input.layout(Point { x: key_x, y: row_y }, LayoutConstraints::new(key_w, key_w, line1_h, line1_h), ctx);

            let type_x = key_x + key_w + gap_x;
            row.type_dropdown.layout(Point { x: type_x, y: row_y }, LayoutConstraints::new(type_w, type_w, line1_h, line1_h), ctx);

            let remove_x = type_x + type_w + gap_x;
            row.remove_button.layout(Point { x: remove_x, y: row_y }, LayoutConstraints::new(remove_w, remove_w, line1_h, line1_h), ctx);

            // Line 2: Value Control Widget
            let value_y = row_y + line1_h + gap_between_lines;
            let value_x = origin.x + pad_x + left_shift;
            row.value_widget.layout(Point { x: value_x, y: value_y }, LayoutConstraints::new(usable_w, usable_w, line2_h, line2_h), ctx);

            link_child(self_ptr, self_id, &mut row.key_input, ctx);
            link_child(self_ptr, self_id, &mut row.type_dropdown, ctx);
            match &mut row.value_widget {
                InstancedWidget::TextBox(tb) => link_child(self_ptr, self_id, tb, ctx),
                InstancedWidget::Spinbox(sb) => link_child(self_ptr, self_id, sb, ctx),
                InstancedWidget::Toggle(tg) => link_child(self_ptr, self_id, tg, ctx),
                InstancedWidget::Slider(sl) => link_child(self_ptr, self_id, sl, ctx),
            }
            link_child(self_ptr, self_id, &mut row.remove_button, ctx);

            curr_y += row_h + gap_between_rows;
        }

        // Lay out add button
        let add_btn_w = 120.0f32.min(usable_w);
        let add_btn_h = 36.0;
        let add_x = origin.x + pad_x + left_shift;
        self.add_button.layout(Point { x: add_x, y: curr_y }, LayoutConstraints::new(add_btn_w, add_btn_w, add_btn_h, add_btn_h), ctx);
        link_child(self_ptr, self_id, &mut self.add_button, ctx);
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.extra_quads();
        if let Some(hq) = self.highlight_quad(ctx) {
            if hq.4 != colors::HIGHLIGHT_SECONDARY {
                quads.push(hq);
            }
        }

        let theme = colors::active_theme();

        // 1. Draw all non-dragged rows first
        for (idx, row) in self.rows.iter().enumerate() {
            if Some(idx) == self.active_drag_index {
                continue;
            }
            let handle_quads = self.get_drag_handle_quads(idx, theme);
            quads.extend(handle_quads);

            quads.extend(row.key_input.all_quads(ctx));
            quads.extend(row.type_dropdown.all_quads(ctx));
            quads.extend(row.value_widget.all_quads(ctx));
            quads.extend(row.remove_button.all_quads(ctx));
        }

        // 2. Draw active dragged row on top!
        if let Some(idx) = self.active_drag_index {
            if idx < self.rows.len() {
                let row = &self.rows[idx];
                
                let (rx, _, rw, _) = self.rect();
                let row_y = row.layout_y;
                let row_h = row.layout_height;
                let pad_x = 8.0;
                let bg_color = [theme.surface_bg[0], theme.surface_bg[1], theme.surface_bg[2], 0.95];
                let border_color = theme.primary_accent;
                
                // Add a backing plate/shadow for the dragged row
                quads.push((rx + pad_x, row_y - 2.0, rw - 2.0 * pad_x, row_h + 4.0, [0.0, 0.0, 0.0, 0.25])); // shadow
                quads.push((rx + pad_x, row_y - 1.0, rw - 2.0 * pad_x, row_h + 2.0, bg_color)); // background
                quads.push((rx + pad_x, row_y - 1.0, rw - 2.0 * pad_x, 1.0, border_color)); // top border
                quads.push((rx + pad_x, row_y + row_h + 1.0, rw - 2.0 * pad_x, 1.0, border_color)); // bottom border

                let handle_quads = self.get_drag_handle_quads(idx, theme);
                quads.extend(handle_quads);

                quads.extend(row.key_input.all_quads(ctx));
                quads.extend(row.type_dropdown.all_quads(ctx));
                quads.extend(row.value_widget.all_quads(ctx));
                quads.extend(row.remove_button.all_quads(ctx));
            }
        }

        quads.extend(self.add_button.all_quads(ctx));
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        Vec::new()
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        // 1. Draw non-dragged first
        for (idx, row) in self.rows.iter().enumerate() {
            if Some(idx) == self.active_drag_index {
                continue;
            }
            labels.extend(row.key_input.text_labels_with_bounds(ctx));
            labels.extend(row.type_dropdown.text_labels_with_bounds(ctx));
            labels.extend(row.value_widget.text_labels_with_bounds(ctx));
            labels.extend(row.remove_button.text_labels_with_bounds(ctx));
        }

        // 2. Draw active dragged row last (on top)
        if let Some(idx) = self.active_drag_index {
            if idx < self.rows.len() {
                let row = &self.rows[idx];
                labels.extend(row.key_input.text_labels_with_bounds(ctx));
                labels.extend(row.type_dropdown.text_labels_with_bounds(ctx));
                labels.extend(row.value_widget.text_labels_with_bounds(ctx));
                labels.extend(row.remove_button.text_labels_with_bounds(ctx));
            }
        }

        labels.extend(self.add_button.text_labels_with_bounds(ctx));
        labels
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        let font = self.widget_font();
        for l in self.text_labels() {
            labels.push((l, font.clone(), None));
        }

        // 1. Draw non-dragged first
        for (idx, row) in self.rows.iter().enumerate() {
            if Some(idx) == self.active_drag_index {
                continue;
            }
            labels.extend(row.key_input.text_labels_with_font_and_bounds(ctx));
            labels.extend(row.type_dropdown.text_labels_with_font_and_bounds(ctx));
            labels.extend(row.value_widget.text_labels_with_font_and_bounds(ctx));
            labels.extend(row.remove_button.text_labels_with_font_and_bounds(ctx));
        }

        // 2. Draw active dragged row last (on top)
        if let Some(idx) = self.active_drag_index {
            if idx < self.rows.len() {
                let row = &self.rows[idx];
                labels.extend(row.key_input.text_labels_with_font_and_bounds(ctx));
                labels.extend(row.type_dropdown.text_labels_with_font_and_bounds(ctx));
                labels.extend(row.value_widget.text_labels_with_font_and_bounds(ctx));
                labels.extend(row.remove_button.text_labels_with_font_and_bounds(ctx));
            }
        }

        labels.extend(self.add_button.text_labels_with_font_and_bounds(ctx));
        labels
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        // 1. Intercept events if add popover is open
        if self.add_popover_open {
            let (bx, by, bw, bh) = self.add_button.rect();
            let dy = by + bh;
            let dh = 4.0 * 24.0;
            match event {
                Event::PointerMove { x, y, .. } => {
                    if *x >= bx && *x <= bx + bw && *y >= dy && *y <= dy + dh {
                        let idx = ((*y - dy) / 24.0).floor() as usize;
                        if idx < 4 {
                            self.add_popover_hovered_idx = Some(idx);
                            return true;
                        }
                    }
                    self.add_popover_hovered_idx = None;
                }
                Event::MouseButton { button, state, x, y, .. } => {
                    if *button == MouseButton::Left && *state == ElementState::Pressed {
                        if *x >= bx && *x <= bx + bw && *y >= dy && *y <= dy + dh {
                            let idx = ((*y - dy) / 24.0).floor() as usize;
                            if idx < 4 {
                                let new_type = match idx {
                                    1 => "Spinbox".to_string(),
                                    2 => "Toggle".to_string(),
                                    3 => "Slider".to_string(),
                                    _ => "TextBox".to_string(),
                                };
                                let default_val = match idx {
                                    1 => "0".to_string(),
                                    2 => "false".to_string(),
                                    3 => "0.5".to_string(),
                                    _ => "".to_string(),
                                };
                                let new_row = MultiControlRow::new("new_widget".to_string(), new_type, default_val);
                                self.rows.push(new_row);
                                self.save_to_config();
                                self.just_changed = true;
                            }
                            self.add_popover_open = false;
                            self.add_popover_hovered_idx = None;
                            return true;
                        } else {
                            // Clicked outside add popover, close it
                            self.add_popover_open = false;
                            self.add_popover_hovered_idx = None;
                            return true;
                        }
                    }
                }
                _ => {}
            }
        }

        let mut handled = false;

        // 1.5 Handle drag events and drag handle interactions
        let mut drag_handled = false;
        match event {
            Event::PointerMove { x, y, .. } => {
                let (rx, _, _, _) = self.rect();
                let pad_x = 8.0;
                let drag_handle_w = 20.0;
                let old_hovered = self.hovered_drag_index;
                self.hovered_drag_index = None;
                
                if *x >= rx + pad_x && *x <= rx + pad_x + drag_handle_w {
                    for (i, row) in self.rows.iter().enumerate() {
                        if *y >= row.layout_y && *y <= row.layout_y + row.layout_height {
                            self.hovered_drag_index = Some(i);
                            break;
                        }
                    }
                }
                
                if old_hovered != self.hovered_drag_index {
                    drag_handled = true;
                }
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, .. } => {
                let (rx, _, _, _) = self.rect();
                let pad_x = 8.0;
                let drag_handle_w = 20.0;
                if *x >= rx + pad_x && *x <= rx + pad_x + drag_handle_w {
                    for (i, row) in self.rows.iter().enumerate() {
                        if *y >= row.layout_y && *y <= row.layout_y + row.layout_height {
                            self.active_drag_index = Some(i);
                            self.drag_start_mouse_y = *y;
                            self.drag_y_offset = 0.0;
                            drag_handled = true;
                            break;
                        }
                    }
                }
            }
            Event::DragStart { .. } => {
                if self.active_drag_index.is_some() {
                    drag_handled = true;
                }
            }
            Event::DragUpdate { y, .. } => {
                if let Some(mut i) = self.active_drag_index {
                    self.drag_y_offset = *y - self.drag_start_mouse_y;
                    
                    let gap = 12.0; // gap_between_rows
                    let mut swapped = true;
                    while swapped {
                        swapped = false;
                        let row_h = self.rows[i].layout_height;
                        let mid_y = self.rows[i].natural_y + self.drag_y_offset + row_h * 0.5;
                        
                        if i > 0 {
                            let mid_above = self.rows[i-1].natural_y + self.rows[i-1].layout_height * 0.5;
                            if mid_y < mid_above {
                                self.drag_start_mouse_y -= self.rows[i-1].layout_height + gap;
                                self.rows.swap(i, i-1);
                                
                                // Recalculate natural_y for all rows to prevent infinite swap loops
                                let top_limit = self.rect().1 + 8.0;
                                let mut curr_y = top_limit;
                                for r in &mut self.rows {
                                    r.natural_y = curr_y;
                                    curr_y += r.layout_height + gap;
                                }

                                i -= 1;
                                self.active_drag_index = Some(i);
                                self.drag_y_offset = *y - self.drag_start_mouse_y;
                                swapped = true;
                                continue;
                            }
                        }
                        if i < self.rows.len() - 1 {
                            let mid_below = self.rows[i+1].natural_y + self.rows[i+1].layout_height * 0.5;
                            if mid_y > mid_below {
                                self.drag_start_mouse_y += self.rows[i+1].layout_height + gap;
                                self.rows.swap(i, i+1);
                                
                                // Recalculate natural_y for all rows to prevent infinite swap loops
                                let top_limit = self.rect().1 + 8.0;
                                let mut curr_y = top_limit;
                                for r in &mut self.rows {
                                    r.natural_y = curr_y;
                                    curr_y += r.layout_height + gap;
                                }

                                i += 1;
                                self.active_drag_index = Some(i);
                                self.drag_y_offset = *y - self.drag_start_mouse_y;
                                swapped = true;
                                continue;
                            }
                        }
                    }
                    drag_handled = true;
                }
            }
            Event::DragEnd | Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, .. } => {
                if self.active_drag_index.is_some() {
                    self.active_drag_index = None;
                    self.drag_y_offset = 0.0;
                    self.save_to_config();
                    self.just_changed = true;
                    drag_handled = true;
                }
            }
            _ => {}
        }
        
        if drag_handled {
            return true;
        }

        // If we are actively dragging, swallow all other events
        if self.active_drag_index.is_some() {
            return true;
        }

        // 2. Intercept row dropdown open popovers first
        for row in &mut self.rows {
            if row.type_dropdown.open {
                if row.type_dropdown.handle_event(event, ctx) {
                    handled = true;
                }
            }
        }

        if !handled {
            if self.add_button.handle_event(event, ctx) {
                handled = true;
            }

            let mut row_to_remove = None;
            for (i, row) in self.rows.iter_mut().enumerate() {
                if row.key_input.handle_event(event, ctx) {
                    handled = true;
                }
                if row.type_dropdown.handle_event(event, ctx) {
                    handled = true;
                }
                if row.value_widget.handle_event(event, ctx) {
                    handled = true;
                }
                if row.remove_button.handle_event(event, ctx) {
                    handled = true;
                }
                if row.remove_button.take_click() {
                    row_to_remove = Some(i);
                    handled = true;
                }
            }

            if let Some(idx) = row_to_remove {
                self.rows.remove(idx);
                self.save_to_config();
                self.just_changed = true;
            }
        }

        if self.add_button.take_click() {
            self.add_popover_open = !self.add_popover_open;
            self.add_popover_hovered_idx = None;
            handled = true;
        }

        let mut type_changed = false;
        let mut value_changed = false;
        let mut key_changed = false;

        for row in &mut self.rows {
            if row.key_input.take_change() {
                key_changed = true;
            }
            if row.type_dropdown.take_change() {
                type_changed = true;
                let new_type_idx = row.type_dropdown.selected;
                let old_val = row.value_widget.get_value_string().unwrap_or_default();
                row.value_widget = match new_type_idx {
                    1 => {
                        let val_i = old_val.parse::<i32>().unwrap_or(0);
                        InstancedWidget::Spinbox(Spinbox::new(val_i, 0, 1000000, 1))
                    }
                    2 => {
                        let toggled = old_val.trim().to_lowercase() == "true" || old_val == "1";
                        let mut tg = Toggle::new();
                        tg.set_toggled(toggled);
                        InstancedWidget::Toggle(tg)
                    }
                    3 => {
                        let mut sl = Slider::new();
                        sl.set_value_string(&old_val);
                        InstancedWidget::Slider(sl)
                    }
                    _ => {
                        InstancedWidget::TextBox(TextBox::new(old_val))
                    }
                };
            }
            if row.value_widget.take_change() {
                value_changed = true;
            }
        }

        if key_changed || type_changed || value_changed {
            self.save_to_config();
            self.just_changed = true;
        }

        if !handled {
            match event {
                Event::PointerMove { x, y, .. } => {
                    let is_hit = self.hit_test(*x, *y, ctx);
                    let was = self.hovered();
                    self.set_hovered(is_hit);
                    if was != is_hit {
                        handled = true;
                    }
                }
                _ => {}
            }
        }

        handled
    }

    fn take_change(&mut self) -> bool {
        let ret = self.just_changed;
        self.just_changed = false;
        ret
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::MouseButton { button, state, x: px, y: py, local_x: px, local_y: py }, ctx)
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::KeyInput(event.clone()), ctx)
    }

    fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::PointerMove { x: px, y: py, local_x: px, local_y: py }, ctx)
    }

    fn unfocus(&mut self) {
        self.add_popover_open = false;
        self.add_popover_hovered_idx = None;
        for row in &mut self.rows {
            row.key_input.unfocus();
            row.type_dropdown.open = false;
            row.value_widget.unfocus();
        }
    }

    fn hit_test(&self, px: f32, py: f32, _ctx: &UiContext) -> bool {
        if let Some((x, y, w, h)) = self.popover_rect() {
            if px >= x && px <= x + w && py >= y && py <= y + h {
                return true;
            }
        }
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        for row in &self.rows {
            if let Some(r) = row.type_dropdown.popover_rect() {
                return Some(r);
            }
        }
        if self.add_popover_open {
            let (bx, by, bw, bh) = self.add_button.rect();
            Some((bx, by + bh, bw, 4.0 * 24.0))
        } else {
            None
        }
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        for row in &self.rows {
            if row.type_dropdown.popover_rect().is_some() {
                row.type_dropdown.render_popover(pc);
                return;
            }
        }
        if !self.add_popover_open { return; }

        let (bx, by, bw, bh) = self.add_button.rect();
        let dy = by + bh;
        let dh = 4.0 * 24.0;
        let options = vec!["TextBox", "Spinbox", "Toggle", "Slider"];

        // Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], bx + 1.0, dy + 1.0, bw, dh);
        pc.rect([0.02, 0.02, 0.05, 0.08], bx + 3.0, dy + 3.0, bw, dh);
        pc.rect([0.02, 0.02, 0.05, 0.04], bx + 5.0, dy + 5.0, bw, dh);

        let theme = colors::active_theme();
        pc.rect(theme.surface_border, bx, dy, bw, dh);
        pc.rect(theme.surface_bg, bx + 1.0, dy + 1.0, bw - 2.0, dh - 2.0);

        if let Some(h_idx) = self.add_popover_hovered_idx {
            let iy = dy + h_idx as f32 * 24.0;
            pc.rect(theme.primary_accent, bx + 2.0, iy + 2.0, bw - 4.0, 20.0);
        }

        for (idx, opt) in options.iter().enumerate() {
            let iy = crate::layout::align_text_y(dy + idx as f32 * 24.0, 24.0, 12.0, 0.0);
            let text_color = if self.add_popover_hovered_idx == Some(idx) {
                [0xff, 0xff, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };
            let color_f32 = [
                text_color[0] as f32 / 255.0,
                text_color[1] as f32 / 255.0,
                text_color[2] as f32 / 255.0,
                1.0,
            ];
            pc.text(opt, bx + 8.0, iy, 12.0, color_f32);
        }
    }
}

impl Control for MultiControl {}

fn get_application_config_path() -> std::path::PathBuf {
    let base_dir = crate::config::get_config_path()
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| {
            let home = std::env::var("HOME").unwrap_or_default();
            std::path::PathBuf::from(home).join(".config").join("cce")
        });

    let mut app_name = std::env::current_exe()
        .ok()
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
        .unwrap_or_else(|| "this-application".to_string());

    if app_name.ends_with(" (deleted)") {
        app_name = app_name[..app_name.len() - " (deleted)".len()].to_string();
    }

    base_dir.join(app_name).join("this-application.json")
}

fn save_config(name: &str, controls: &[InstancedControl]) {
    let path = get_application_config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let mut map = if path.exists() {
        let content = std::fs::read_to_string(&path).unwrap_or_default();
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&content)
            .unwrap_or_default()
    } else {
        serde_json::Map::new()
    };

    if let Ok(json_val) = serde_json::to_value(controls) {
        map.insert(name.to_string(), json_val);
    }

    if let Ok(updated_content) = serde_json::to_string_pretty(&map) {
        let _ = std::fs::write(&path, updated_content);
    }
}

fn load_config(name: &str) -> Vec<InstancedControl> {
    let path = get_application_config_path();
    
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(&path) {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str(&content) {
                if let Some(val) = map.get(name) {
                    if let Ok(controls) = serde_json::from_value::<Vec<InstancedControl>>(val.clone()) {
                        return controls;
                    }
                }
            }
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instanced_control_serialization() {
        let controls = vec![
            InstancedControl {
                key: "width".to_string(),
                control_type: "Spinbox".to_string(),
                value: "42".to_string(),
            },
            InstancedControl {
                key: "label".to_string(),
                control_type: "TextBox".to_string(),
                value: "hello".to_string(),
            },
        ];

        let serialized = serde_json::to_string(&controls).unwrap();
        let deserialized: Vec<InstancedControl> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(controls, deserialized);
    }

    static ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn test_save_load_config_files() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let temp_dir = std::env::temp_dir().join("cce_test_home");
        let _ = std::fs::create_dir_all(&temp_dir);
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_dir.to_str().unwrap());

        let controls = vec![
            InstancedControl {
                key: "x_offset".to_string(),
                control_type: "TextBox".to_string(),
                value: "10".to_string(),
            },
        ];

        let name = "my_multicontrol";
        save_config(name, &controls);

        let loaded = load_config(name);
        assert_eq!(controls, loaded);

        let path = get_application_config_path();
        assert!(path.exists());

        let _ = std::fs::remove_dir_all(&temp_dir);
        if let Some(h) = old_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_popover_interaction() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let mut dummy = crate::context::UiContext::new();
        let temp_dir = std::env::temp_dir().join("cce_test_home_popover");
        let _ = std::fs::create_dir_all(&temp_dir);
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_dir.to_str().unwrap());

        let mut mc = MultiControl::new("test_mc".to_string());
        mc.set_rect(10.0, 10.0, 300.0, 200.0);
        mc.layout(Point { x: 10.0, y: 10.0 }, LayoutConstraints::new(300.0, 300.0, 200.0, 200.0), &mut dummy);

        assert!(!mc.add_popover_open);

        let (bx, by, _bw, bh) = mc.add_button.rect();
        let clicked = mc.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 5.0, by + 5.0, &mut dummy);
        assert!(clicked);
        let released = mc.mouse_input(MouseButton::Left, ElementState::Released, bx + 5.0, by + 5.0, &mut dummy);
        assert!(released);
        assert!(mc.add_popover_open);

        let dy = by + bh;
        let hover_y = dy + 60.0;
        let moved = mc.cursor_moved(bx + 5.0, hover_y, &mut dummy);
        assert!(moved);
        assert_eq!(mc.add_popover_hovered_idx, Some(2));

        let clicked_item = mc.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 5.0, hover_y, &mut dummy);
        assert!(clicked_item);
        assert!(!mc.add_popover_open);
        assert_eq!(mc.rows.len(), 1);

        let row = &mc.rows[0];
        assert_eq!(row.key_input.get_value_string().unwrap(), "new_widget");
        match &row.value_widget {
            InstancedWidget::Toggle(_) => {},
            _ => panic!("Expected Toggle widget"),
        }

        let _ = std::fs::remove_dir_all(&temp_dir);
        if let Some(h) = old_home {
            std::env::set_var("HOME", h);
        }
    }

    #[test]
    fn test_multicontrol_drag_reorder() {
        let _guard = ENV_MUTEX.lock().unwrap();
        let mut dummy = crate::context::UiContext::new();
        let temp_dir = std::env::temp_dir().join("cce_test_home_drag");
        let _ = std::fs::remove_dir_all(&temp_dir);
        let _ = std::fs::create_dir_all(&temp_dir);
        let old_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", temp_dir.to_str().unwrap());

        let mut mc = MultiControl::new("test_mc_drag".to_string());
        // Add two rows
        mc.rows.push(MultiControlRow::new("param1".to_string(), "Spinbox".to_string(), "10".to_string()));
        mc.rows.push(MultiControlRow::new("param2".to_string(), "TextBox".to_string(), "val".to_string()));
        mc.save_to_config();

        // Perform initial layout
        mc.set_rect(10.0, 10.0, 300.0, 250.0);
        mc.layout(Point { x: 10.0, y: 10.0 }, LayoutConstraints::new(300.0, 300.0, 250.0, 250.0), &mut dummy);

        assert_eq!(mc.rows[0].key_input.get_value_string().unwrap(), "param1");
        assert_eq!(mc.rows[1].key_input.get_value_string().unwrap(), "param2");

        // Click on the drag handle of the first row (param1)
        // Drag handle x is in: rx + pad_x (10 + 8 = 18) to rx + pad_x + drag_handle_w (18 + 20 = 38).
        // Let's click at x = 25, y = mc.rows[0].layout_y + 5.
        let click_x = 25.0;
        let click_y = mc.rows[0].layout_y + 5.0;

        let handled_press = mc.mouse_input(MouseButton::Left, ElementState::Pressed, click_x, click_y, &mut dummy);
        assert!(handled_press);
        assert_eq!(mc.active_drag_index, Some(0));

        // Drag down to swap with the second row (param2)
        // Row 1 starts around natural_y + height + gap. Let's move mouse to mid-point of row 1.
        let target_y = mc.rows[1].natural_y + mc.rows[1].layout_height * 0.5 + 5.0;
        
        let drag_update_evt = Event::DragUpdate {
            dx: 0.0,
            dy: target_y - click_y,
            x: click_x,
            y: target_y,
            local_x: click_x - 10.0,
            local_y: target_y - 10.0,
        };
        let handled_drag = mc.handle_event(&drag_update_evt, &mut dummy);
        assert!(handled_drag);

        // Verify that the swap occurred!
        assert_eq!(mc.active_drag_index, Some(1));
        assert_eq!(mc.rows[0].key_input.get_value_string().unwrap(), "param2");
        assert_eq!(mc.rows[1].key_input.get_value_string().unwrap(), "param1");

        // End drag
        let handled_end = mc.handle_event(&Event::DragEnd, &mut dummy);
        assert!(handled_end);
        assert_eq!(mc.active_drag_index, None);

        // Load config from disk and verify the new order is persisted!
        let loaded = load_config("test_mc_drag");
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].key, "param2");
        assert_eq!(loaded[1].key, "param1");

        let _ = std::fs::remove_dir_all(&temp_dir);
        if let Some(h) = old_home {
            std::env::set_var("HOME", h);
        }
    }
}
