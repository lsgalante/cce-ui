
#[derive(Debug, Clone)]
pub struct TextLabel {
    pub text: String,
    pub x: f32,
    pub y: f32,
    pub font_size: f32,
    pub color: [u8; 3],
}

impl TextLabel {
    pub fn estimate_width(text: &str, font_size: f32) -> f32 {
        let mut weight_sum = 0.0;
        for c in text.chars() {
            weight_sum += match c {
                'i' | 'l' | 't' | 'j' | 'I' | ' ' | '.' | ',' | '!' | ';' | ':' | '\'' | '1' | '-' | '(' | ')' | '[' | ']' => 0.26,
                'f' | 'r' | 's' | 'J' => 0.35,
                'w' | 'm' | 'M' | 'W' => 0.72,
                'A' | 'B' | 'C' | 'D' | 'E' | 'G' | 'H' | 'K' | 'N' | 'O' | 'P' | 'Q' | 'R' | 'S' | 'T' | 'U' | 'V' | 'X' | 'Y' | 'Z' => 0.65,
                _ => 0.52,
            };
        }
        (weight_sum * font_size * 1.30).ceil()
    }

    pub fn curved_layout(
        text: &str,
        cx: f32,
        cy: f32,
        r: f32,
        start_angle: f32,
        end_angle: f32,
        font_size: f32,
        color: [u8; 3],
    ) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        let char_widths: Vec<f32> = text.chars().map(|c| {
            Self::estimate_width(&c.to_string(), font_size)
        }).collect();
        let total_width: f32 = char_widths.iter().sum();
        
        let mid_angle = (start_angle + end_angle) / 2.0;
        let angular_width = total_width / r;
        let text_start_angle = mid_angle - angular_width / 2.0;
        
        let mut current_angle = text_start_angle;
        for (i, c) in text.chars().enumerate() {
            let cw = char_widths[i];
            let dtheta = cw / r;
            let char_center_angle = current_angle + dtheta / 2.0;
            
            let x = cx + r * char_center_angle.cos() - cw / 2.0;
            let y = cy + r * char_center_angle.sin() - font_size / 2.0;
            
            labels.push(TextLabel {
                text: c.to_string(),
                x,
                y,
                font_size,
                color,
            });
            
            current_angle += dtheta;
        }
        labels
    }

    pub fn is_covered_by(&self, px: f32, py: f32, pw: f32, ph: f32) -> bool {
        let text_w = Self::estimate_width(&self.text, self.font_size);
        let x_overlap = self.x <= px + pw && (self.x + text_w) >= px;
        let y_overlap = self.y <= py + ph && (self.y + self.font_size) >= py;
        x_overlap && y_overlap
    }
}

pub(crate) fn make_widget_text_buffer(fs: &mut glyphon::FontSystem, text: &str, size: f32, font_family: &str) -> glyphon::Buffer {
    let scale = crate::scale::scale_factor();
    let physical_size = size * scale;
    let metrics = glyphon::Metrics::new(physical_size, physical_size * 1.4);
    let mut buf = glyphon::Buffer::new(fs, metrics);
    let (family_name, _) = crate::layout::parse_font_string(font_family);
    let family = match family_name.as_str() {
        "monospace" => glyphon::Family::Name(crate::layout::get_system_monospace_font()),
        "sans-serif" => glyphon::Family::SansSerif,
        "serif" => glyphon::Family::Serif,
        _ => glyphon::Family::Name(&family_name),
    };
    let attrs = glyphon::Attrs::new().family(family);
    buf.set_text(fs, text, attrs, glyphon::Shaping::Advanced);
    buf.shape_until_scroll(fs, true);
    buf
}
