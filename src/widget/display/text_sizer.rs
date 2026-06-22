use std::collections::HashMap;
use std::sync::RwLock;
use std::sync::OnceLock;
use crate::widget::display::TextLabel;

#[derive(Hash, Eq, PartialEq, Clone, Debug)]
struct TextMeasureKey {
    text: String,
    font_family: String,
    font_size_bits: u32,
    scale_bits: u32,
}

static TEXT_SIZE_CACHE: OnceLock<RwLock<HashMap<TextMeasureKey, f32>>> = OnceLock::new();

pub fn measure_text_width(text: &str, font_family: &str, font_size: f32) -> f32 {
    let scale = crate::scale::scale_factor().max(1.0);
    
    let key = TextMeasureKey {
        text: text.trim().to_string(),
        font_family: font_family.to_string(),
        font_size_bits: font_size.to_bits(),
        scale_bits: scale.to_bits(),
    };

    let cache = TEXT_SIZE_CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(lock) = cache.read() {
        if let Some(&exact_width) = lock.get(&key) {
            return exact_width;
        }
    }

    let exact_width = perform_svg_measurement(&key.text, &key.font_family, font_size, scale);

    if let Ok(mut lock) = cache.write() {
        lock.insert(key, exact_width);
    }

    exact_width
}

pub fn measure_text(text: &str, font_size: f32) -> f32 {
    let font_family = crate::layout::menubar_font_parsed().0;
    measure_text_width(text, &font_family, font_size)
}

fn perform_svg_measurement(text: &str, font_family: &str, font_size: f32, scale: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    
    let canvas_w = 1000.0;
    let canvas_h = font_size * 2.5;

    let w_px = (canvas_w * scale) as u32;
    let h_px = (canvas_h * scale) as u32;

    let svg_data = format!(
        r##"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg">
  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="#000000" text-anchor="middle" dominant-baseline="middle">{}</text>
</svg>"##,
        w_px, h_px,
        canvas_w, canvas_h,
        canvas_w / 2.0, canvas_h / 2.0,
        font_family,
        font_size,
        text
    );

    let opt = resvg::usvg::Options::default();
    let fontdb = crate::widget::input::get_font_db();
    
    if let Ok(tree) = resvg::usvg::Tree::from_data(svg_data.as_bytes(), &opt, fontdb) {
        if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w_px, h_px) {
            resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
            let pixels = pixmap.data();

            let mut min_col = None;
            let mut max_col = None;

            for row in 0..h_px {
                for col in 0..w_px {
                    let idx = ((row * w_px + col) * 4) as usize;
                    if idx + 3 < pixels.len() && pixels[idx + 3] > 0 {
                        if min_col.is_none() || col < min_col.unwrap() {
                            min_col = Some(col);
                        }
                        if max_col.is_none() || col > max_col.unwrap() {
                            max_col = Some(col);
                        }
                    }
                }
            }

            if let (Some(min), Some(max)) = (min_col, max_col) {
                return (max - min + 1) as f32 / scale;
            }
        }
    }

    TextLabel::estimate_width(text, font_size)
}
