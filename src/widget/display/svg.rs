use crate::widget::*;

#[derive(Debug, Clone)]
pub struct Svg {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub quads: Vec<(f32, f32, f32, f32, [f32; 4])>,
}

impl Svg {
    pub fn new(svg_data: &[u8], x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let opt = resvg::usvg::Options::default();
        let fontdb = resvg::usvg::fontdb::Database::new();
        let tree = resvg::usvg::Tree::from_data(svg_data, &opt, &fontdb).ok()?;
        
        let target_w = w as u32;
        let target_h = h as u32;
        if target_w == 0 || target_h == 0 {
            return None;
        }
        let mut pixmap = resvg::tiny_skia::Pixmap::new(target_w, target_h)?;
        
        let orig_w = tree.size().width();
        let orig_h = tree.size().height();
        let sx = target_w as f32 / orig_w;
        let sy = target_h as f32 / orig_h;
        let transform = resvg::tiny_skia::Transform::from_scale(sx, sy);
        
        resvg::render(&tree, transform, &mut pixmap.as_mut());
        
        let mut quads = Vec::new();
        let pixels = pixmap.data();
        for row in 0..target_h {
            for col in 0..target_w {
                let idx = ((row * target_w + col) * 4) as usize;
                if idx + 3 < pixels.len() {
                    let a = pixels[idx + 3] as f32 / 255.0;
                    if a > 0.0 {
                        let r = pixels[idx] as f32 / 255.0;
                        let g = pixels[idx + 1] as f32 / 255.0;
                        let b = pixels[idx + 2] as f32 / 255.0;
                        quads.push((
                            x + col as f32,
                            y + row as f32,
                            1.0,
                            1.0,
                            [r, g, b, a],
                        ));
                    }
                }
            }
        }
        
        Some(Self { x, y, w, h, quads })
    }

    pub fn from_file<P: AsRef<std::path::Path>>(path: P, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        let data = std::fs::read(path).ok()?;
        Self::new(&data, x, y, w, h)
    }

    pub fn from_str(svg_str: &str, x: f32, y: f32, w: f32, h: f32) -> Option<Self> {
        Self::new(svg_str.as_bytes(), x, y, w, h)
    }
}

impl Element for Svg {
    fn rect(&self) -> (f32, f32, f32, f32) { (self.x, self.y, self.w, self.h) }
    
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let dx = x - self.x;
        let dy = y - self.y;
        for quad in &mut self.quads {
            quad.0 += dx;
            quad.1 += dy;
        }
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
    }
    
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.quads.clone()
    }
}
