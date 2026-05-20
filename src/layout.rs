use crate::widget::Widget;

pub trait RenderTarget {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32);
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]);
}

fn render_widget<T: Widget>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32) {
    w.set_rect(x, y, ww, wh);
    for (qx, qy, qw, qh, qc) in w.extra_quads() {
        pc.rect(qc, qx, qy, qw, qh);
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

pub struct Column<'a> {
    pc: &'a mut dyn RenderTarget,
    ox: f32,
    oy: f32,
    cx: f32,
    pub y: f32,
    pub cw: f32,
}

impl<'a> Column<'a> {
    pub fn new(pc: &'a mut dyn RenderTarget, ox: f32, oy: f32, cx: f32, cy: f32, cw: f32) -> Self {
        Self { pc, ox, oy, cx, y: cy, cw }
    }

    fn ax(&self, x_off: f32) -> f32 {
        self.ox + self.cx + x_off
    }

    fn ay(&self) -> f32 {
        self.oy + self.y
    }

    pub fn spacing(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn separator(&mut self) {
        let x = self.ax(8.0);
        let y = self.ay();
        self.pc
            .rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 16.0, 1.0);
        self.y += 8.0;
    }

    pub fn header(&mut self, text: &str, x_off: f32) {
        let x = self.ax(x_off);
        let y = self.ay();
        self.pc
            .text(text, x, y, 14.0, [0.83, 0.83, 0.83, 1.0]);
        self.y += 22.0;
    }

    pub fn text(&mut self, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let x = self.ax(x_off);
        let y = self.ay() + y_off;
        self.pc
            .text(text, x, y, font_size, color);
    }

    pub fn widget<T: Widget>(&mut self, w: &mut T, x_off: f32, ww: f32, wh: f32) {
        let x = self.ax(x_off);
        let y = self.ay();
        render_widget(self.pc, w, x, y, ww, wh);
        self.y += wh;
    }

    pub fn row<F: FnOnce(&mut Row)>(&mut self, h: f32, f: F) {
        let row_y = self.ay();
        let mut row = Row {
            pc: &mut *self.pc,
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
