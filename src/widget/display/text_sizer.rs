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

/// Truncate to at most `max_chars` characters, replacing the tail with "..."
/// (for names/titles where the head identifies the item). Char-boundary safe —
/// byte-slicing a multi-byte string panics; this never does.
pub fn truncate_tail(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let mut out: String = s.chars().take(keep).collect();
    out.push_str("...");
    out
}

/// Truncate to at most `max_chars` characters, replacing the head with "..."
/// (for paths/targets where the tail identifies the item). Char-boundary safe.
pub fn truncate_head(s: &str, max_chars: usize) -> String {
    let count = s.chars().count();
    if count <= max_chars {
        return s.to_string();
    }
    let keep = max_chars.saturating_sub(3);
    let tail: String = s.chars().skip(count - keep).collect();
    format!("...{tail}")
}

/// `s` with the five XML specials escaped, for text and attribute values
/// alike.
fn xml_escape(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains(['&', '<', '>', '"', '\'']) {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    std::borrow::Cow::Owned(out)
}

/// The one-line SVG the measurement renders: `text` centered on a canvas
/// 1000 logical px wide. Both the text and the family are escaped — until
/// 2026-10-06 they went in raw, so a label with `&` or `<` (a family with a
/// quote) was not XML, failed to parse, and fell back to the estimate.
fn measurement_svg(text: &str, font_family: &str, font_size: f32, scale: f32) -> (String, u32, u32) {
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
        xml_escape(font_family),
        font_size,
        xml_escape(text)
    );
    (svg_data, w_px, h_px)
}

fn perform_svg_measurement(text: &str, font_family: &str, font_size: f32, scale: f32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let (svg_data, w_px, h_px) = measurement_svg(text, font_family, font_size, scale);

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

#[cfg(test)]
mod tests {
    use super::{measurement_svg, truncate_head, truncate_tail, xml_escape};

    #[test]
    fn xml_specials_are_escaped() {
        assert_eq!(xml_escape("plain"), "plain");
        assert_eq!(xml_escape(r#"R&D <x> "q" 'a'"#), "R&amp;D &lt;x&gt; &quot;q&quot; &apos;a&apos;");
    }

    #[test]
    fn a_label_with_xml_specials_still_parses() {
        // The raw interpolation made each of these an XML error, and the
        // width fell back to the character-count estimate.
        let opt = resvg::usvg::Options::default();
        let fontdb = resvg::usvg::fontdb::Database::new();
        for (text, family) in [("Tom & Jerry", "Sans"), ("a < b", "Sans"), ("x > y", "Sans"), ("plain", r#"My "Font""#)] {
            let (svg, _, _) = measurement_svg(text, family, 14.0, 1.0);
            assert!(
                resvg::usvg::Tree::from_data(svg.as_bytes(), &opt, &fontdb).is_ok(),
                "{text:?} in {family:?} did not parse: {svg}"
            );
        }
    }

    #[test]
    fn short_strings_pass_through() {
        assert_eq!(truncate_tail("abc", 30), "abc");
        assert_eq!(truncate_head("abc", 30), "abc");
        assert_eq!(truncate_tail("", 5), "");
        assert_eq!(truncate_head("", 5), "");
    }

    #[test]
    fn exact_length_passes_through() {
        let s = "a".repeat(30);
        assert_eq!(truncate_tail(&s, 30), s);
        assert_eq!(truncate_head(&s, 30), s);
    }

    #[test]
    fn tail_truncates_to_max() {
        let s = "abcdefghij";
        assert_eq!(truncate_tail(s, 8), "abcde...");
        assert_eq!(truncate_tail(s, 8).chars().count(), 8);
    }

    #[test]
    fn head_truncates_keeping_tail() {
        let s = "/very/long/path/to/file";
        // "..." + 7 tail chars = 10 visible chars budgeted
        assert_eq!(truncate_head(s, 10), "...to/file");
        assert_eq!(truncate_head(s, 10).chars().count(), 10);
    }

    #[test]
    fn multibyte_at_the_old_panic_boundary() {
        // 30+ two-byte chars: the old `&name[..27]` byte-slice panicked when
        // byte 27 fell inside a code point. Char-based truncation must not.
        let s = "é".repeat(35);
        let t = truncate_tail(&s, 30);
        assert_eq!(t.chars().count(), 30);
        assert!(t.ends_with("..."));
        let h = truncate_head(&s, 40);
        assert_eq!(h, s); // 35 chars <= 40: untouched despite 70 bytes
        let h2 = truncate_head(&s, 30);
        assert!(h2.starts_with("..."));
        assert_eq!(h2.chars().count(), 30);
    }

    #[test]
    fn tiny_budget_degrades_gracefully() {
        assert_eq!(truncate_tail("abcdef", 3), "...");
        assert_eq!(truncate_head("abcdef", 2), "...");
    }
}


