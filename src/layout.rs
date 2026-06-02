use crate::widget::Widget;

pub trait RenderTarget {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32);
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]);
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _font: &str) {
        self.text(content, x, y, size, color);
    }
}

pub struct PopoverCollector {
    pub rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    pub texts: Vec<(String, f32, f32, f32, [f32; 4], Option<String>)>,
}

impl PopoverCollector {
    pub fn new() -> Self {
        Self { rects: Vec::new(), texts: Vec::new() }
    }
}

impl RenderTarget for PopoverCollector {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.rects.push((color, x, y, w, h));
    }

    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        self.texts.push((content.to_string(), size, x, y, color, None));
    }

    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string())));
    }
}

pub fn render_widget<T: Widget + 'static>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32) {
    w.set_rect(x, y, ww, wh);
    for (qx, qy, qw, qh, qc) in w.all_quads() {
        pc.rect(qc, qx, qy, qw, qh);
    }
    let font_opt = w.widget_font();
    for label in w.text_labels() {
        let color_f32 = [
            label.color[0] as f32 / 255.0,
            label.color[1] as f32 / 255.0,
            label.color[2] as f32 / 255.0,
            1.0,
        ];
        if let Some(ref font) = font_opt {
            pc.text_with_font(&label.text, label.x, label.y, label.font_size, color_f32, font);
        } else {
            pc.text(&label.text, label.x, label.y, label.font_size, color_f32);
        }
    }
    if w.popover_rect().is_some() {
        crate::widget::popovers::register(w);
    }
}

pub fn render_popovers(pc: &mut dyn RenderTarget) {
    for popover_ptr in crate::widget::popovers::get_active() {
        unsafe {
            (*popover_ptr).render_popover(pc);
        }
    }
}

pub struct UiFrame;

impl UiFrame {
    pub fn start(scroll_offset: f32) -> Self {
        crate::widget::hover_animation::reset_frame_registration();
        crate::widget::hover_animation::set_scroll_offset(scroll_offset);
        crate::widget::popovers::clear();
        Self
    }

    pub fn finish(self, pc: &mut dyn RenderTarget) {
        crate::widget::hover_animation::post_render_check();
        if let Some((qx, qy, qw, qh, qc)) = crate::widget::hover_animation::get_quad() {
            pc.rect(qc, qx, qy, qw, qh);
        }
        render_popovers(pc);
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

    pub fn widget<T: Widget + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, wh: f32) {
        let x = self.ax(x_off);
        let y = self.ay();
        w.set_row_rect(self.ox + self.cx + 8.0, self.cw - 16.0);
        render_widget(pc, w, x, y, ww, wh);
        self.y += wh + w.top_room();
    }

    pub fn row<F: FnOnce(&mut Row)>(&mut self, pc: &mut dyn RenderTarget, h: f32, f: F) {
        let row_y = self.ay();
        let mut row = Row {
            pc: &mut *pc,
            base_x: self.ox + self.cx,
            y: row_y,
            cursor_x: 0.0,
            spacing: 8.0,
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

    pub fn widget<T: Widget + 'static>(&mut self, w: &mut T, ww: f32, wh: f32) {
        render_widget(self.pc, w, self.base_x + self.cursor_x, self.y, ww, wh);
        self.cursor_x += ww + self.spacing;
    }
}

pub struct Section {
    left: f32,
    top: f32,
    pub content_y: f32,
    pub cw: f32,
}

impl Section {
    pub const ROW_PADDING_X: f32 = 8.0;
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    pub fn new(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str) -> Self {
        pc.text(label, left + Self::DEFAULT_MARGIN_X, top, 14.0, [0.83, 0.83, 0.83, 1.0]);
        pc.rect([0.18, 0.18, 0.27, 1.0], left + Self::ROW_PADDING_X, top + 22.0, cw - 2.0 * Self::ROW_PADDING_X, 1.0);
        Self { left, top, content_y: top + 34.0, cw }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        let shift = if x_off >= 12.0 { 8.0 } else { 0.0 };
        self.left + x_off + shift
    }

    pub fn ay(&self) -> f32 { self.content_y }

    pub fn spacing(&mut self, dy: f32) { self.content_y += dy; }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        pc.text(text, self.ax(x_off), self.ay() + y_off, font_size, color);
    }

    pub fn widget<T: Widget + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, wh: f32) {
        w.set_row_rect(self.left + Self::ROW_PADDING_X, self.cw - 2.0 * Self::ROW_PADDING_X);
        let top_room = w.top_room();
        self.content_y += top_room;
        render_widget(pc, w, self.ax(x_off), self.ay(), ww, wh);
        self.content_y += wh;
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let x = self.ax(Self::ROW_PADDING_X);
        let y = self.ay();
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * Self::ROW_PADDING_X, 1.0);
        self.content_y += 8.0;
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.content_y += h;
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = Self::ROW_PADDING_X + 12.0; // 20.0 px (12.0 px inner padding)
        let usable_w = self.cw - 2.0 * margin_x; // padding on left and right inside outer bounds
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
        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;
    }

    pub fn finish(&mut self, pc: &mut dyn RenderTarget) -> f32 {
        self.finish_focused(pc, false)
    }

    pub fn finish_focused(&mut self, pc: &mut dyn RenderTarget, focused: bool) -> f32 {
        let border: [f32; 4] = if focused {
            [0.30, 0.50, 0.32, 1.0] // Focused green
        } else {
            [0.25, 0.25, 0.35, 1.0] // Default gray
        };
        let x = self.left + Self::ROW_PADDING_X;
        let y = self.top + 22.0;
        let w = self.cw - 2.0 * Self::ROW_PADDING_X;
        let h = self.content_y - y;
        pc.rect(border, x, y, w, 1.0);
        pc.rect(border, x, y + h + 12.0, w, 1.0);
        pc.rect(border, x, y, 1.0, h + 12.0);
        pc.rect(border, x + w - 1.0, y, 1.0, h + 12.0);
        self.content_y + 20.0
    }

    pub fn vstack<'a>(&'a mut self, pc: &'a mut dyn RenderTarget, spacing: f32) -> VStack<'a> {
        VStack {
            section: self,
            pc,
            spacing,
        }
    }
}

pub struct VStack<'a> {
    section: &'a mut Section,
    pc: &'a mut dyn RenderTarget,
    spacing: f32,
}

impl<'a> VStack<'a> {
    pub fn add_widget<T: Widget + 'static>(&mut self, w: &mut T, ww: f32, wh: f32) {
        self.section.widget(self.pc, w, Section::DEFAULT_MARGIN_X, ww, wh);
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

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRenderTarget {
        rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    }

    impl RenderTarget for MockRenderTarget {
        fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
            self.rects.push((color, x, y, w, h));
        }
        fn text(&mut self, _content: &str, _x: f32, _y: f32, _size: f32, _color: [f32; 4]) {}
    }

    struct MockWidget {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    }

    impl Widget for MockWidget {
        fn rect(&self) -> (f32, f32, f32, f32) {
            (self.x, self.y, self.w, self.h)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            self.x = x;
            self.y = y;
            self.w = w;
            self.h = h;
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    struct MockWidgetWithLabel {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        top_room: f32,
    }

    impl Widget for MockWidgetWithLabel {
        fn rect(&self) -> (f32, f32, f32, f32) {
            (self.x, self.y, self.w, self.h)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            self.x = x;
            self.y = y;
            self.w = w;
            self.h = h;
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
        fn top_room(&self) -> f32 {
            self.top_room
        }
    }

    #[test]
    fn test_vstack_flow() {
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut sec = Section::new(&mut mock_pc, 10.0, 20.0, 200.0, "Test Section");
        
        let start_y = sec.ay();
        let mut stack = sec.vstack(&mut mock_pc, 10.0);

        let mut w1 = MockWidget { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w1, 50.0, 30.0);

        // Standard margin should be applied
        assert_eq!(w1.x, 30.0);
        assert_eq!(w1.y, start_y);

        let mut w2 = MockWidget { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w2, 60.0, 40.0);

        // Second widget should start after first widget height + vstack spacing
        assert_eq!(w2.y, start_y + 30.0 + 10.0);

        let mut w3 = MockWidgetWithLabel { x: 0.0, y: 0.0, w: 0.0, h: 0.0, top_room: 15.0 };
        stack.add_widget(&mut w3, 70.0, 50.0);

        // Third widget has top_room = 15.0, so its y should be shifted by 15.0
        assert_eq!(w3.y, start_y + 30.0 + 10.0 + 40.0 + 10.0 + 15.0);
    }
}
