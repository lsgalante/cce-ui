use crate::widget::Widget;

pub trait RenderTarget {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32);
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]);
}

pub fn render_widget<T: Widget>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32) {
    w.set_rect(x, y, ww, wh);
    for (qx, qy, qw, qh, qc) in w.extra_quads() {
        pc.rect(qc, qx, qy, qw, qh);
    }
    // Render hover highlight on top of widget surfaces
    if let Some(hc) = w.hover_highlight() {
        pc.rect(hc, x, y, ww, wh);
    }
    for label in w.text_labels() {
        pc.text(
            &label.text,
            label.x,
            label.y,
            label.font_size,
            [
                label.color[0] as f32 / 255.0,
                label.color[1] as f32 / 255.0,
                label.color[2] as f32 / 255.0,
                1.0,
            ],
        );
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

    pub fn widget<T: Widget>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, wh: f32) {
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
        };
        f(&mut row);
        self.y = self.y + h;
    }
}

pub struct Row<'a> {
    pc: &'a mut dyn RenderTarget,
    base_x: f32,
    y: f32,
}

impl<'a> Row<'a> {
    pub fn text(&mut self, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        self.pc
            .text(text, self.base_x + x_off, self.y + y_off, font_size, color);
    }

    pub fn widget<T: Widget>(&mut self, w: &mut T, x_off: f32, ww: f32, wh: f32) {
        render_widget(self.pc, w, self.base_x + x_off, self.y, ww, wh);
    }
}

pub struct Section {
    left: f32,
    top: f32,
    pub content_y: f32,
    pub cw: f32,
}

impl Section {
    pub fn new(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str) -> Self {
        pc.text(label, left + 12.0, top, 14.0, [0.83, 0.83, 0.83, 1.0]);
        pc.rect([0.18, 0.18, 0.27, 1.0], left + 8.0, top + 22.0, cw - 16.0, 1.0);
        Self { left, top, content_y: top + 40.0, cw }
    }

    pub fn ax(&self, x_off: f32) -> f32 { self.left + x_off }

    pub fn ay(&self) -> f32 { self.content_y }

    pub fn spacing(&mut self, dy: f32) { self.content_y += dy; }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        pc.text(text, self.ax(x_off), self.ay() + y_off, font_size, color);
    }

    pub fn widget<T: Widget>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, wh: f32) {
        w.set_row_rect(self.left + 8.0, self.cw - 16.0);
        render_widget(pc, w, self.ax(x_off), self.ay(), ww, wh);
        self.content_y += wh + w.top_room();
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let x = self.ax(8.0);
        let y = self.ay();
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 16.0, 1.0);
        self.content_y += 8.0;
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.content_y += h;
    }

    pub fn finish(&mut self, pc: &mut dyn RenderTarget) -> f32 {
        let border: [f32; 4] = [0.25, 0.25, 0.35, 1.0];
        let x = self.left + 8.0;
        let y = self.top + 22.0;
        let w = self.cw - 16.0;
        let h = self.content_y - y;
        pc.rect(border, x, y, w, 1.0);
        pc.rect(border, x, y + h + 4.0, w, 1.0);
        pc.rect(border, x, y, 1.0, h + 4.0);
        pc.rect(border, x + w - 1.0, y, 1.0, h + 4.0);
        self.content_y + 12.0
    }
}
