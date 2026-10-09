use super::*;

#[test]
fn test_parse_hex_rgba() {
    // 6-digit → alpha 1.0, raw sRGB
    assert_eq!(parse_hex_rgba("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
    // 8-digit → explicit alpha
    assert_eq!(parse_hex_rgba("#00ff0080"), Some([0.0, 1.0, 0.0, 128.0 / 255.0]));
    // leading '#' optional; quotes/whitespace tolerated
    assert_eq!(parse_hex_rgba("\" ffffff \""), Some([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(parse_hex_rgba("000000"), Some([0.0, 0.0, 0.0, 1.0]));
    // invalid
    assert_eq!(parse_hex_rgba("#fff"), None);
    assert_eq!(parse_hex_rgba("nothex"), None);
    assert_eq!(parse_hex_rgba(""), None);
    // rgb drops alpha; linear applies gamma to rgb only
    assert_eq!(parse_hex_rgb("#ff0000"), Some([1.0, 0.0, 0.0]));
    assert_eq!(parse_hex_rgba_linear("#000000ff"), Some([0.0, 0.0, 0.0, 1.0]));
    // byte primitive
    assert_eq!(parse_hex_bytes("#010203"), Some([1, 2, 3, 255]));
    assert_eq!(parse_hex_bytes("#01020304"), Some([1, 2, 3, 4]));
    assert_eq!(parse_hex_bytes("#fff"), None);
}

#[test]
fn perceptual_fade_alpha_walks_lightness_not_luminance() {
    let overlay = [0.62, 0.70, 0.95];
    let backdrop = [0.028, 0.028, 0.041];
    let a = |t: f32| perceptual_fade_alpha(t, overlay, backdrop, 1.0);

    // Endpoints are the plain ones, and the ramp only ever falls.
    assert!((a(0.0) - 1.0).abs() < 1e-4);
    assert!(a(1.0).abs() < 1e-4);
    for i in 1..=20 {
        assert!(a(i as f32 / 20.0) <= a((i - 1) as f32 / 20.0));
    }

    // The correction runs BELOW the straight ramp — that ramp's excess brightness
    // through the middle is the bow the eye reads as non-linear.
    assert!(a(0.5) < 0.5 - 0.1, "midpoint {} should sit well under 0.5", a(0.5));

    // What it buys: composited lightness lands on a straight line. cbrt(luminance) is
    // the same proxy the implementation uses, checked here end to end through blending.
    let lum = |c: [f32; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    let l_at = |t: f32| {
        let a = a(t);
        let y: f32 = a * lum(overlay) + (1.0 - a) * lum(backdrop);
        y.cbrt()
    };
    let (top, bottom) = (l_at(0.0), l_at(1.0));
    for i in 0..=10 {
        let t = i as f32 / 10.0;
        let ideal = top + (bottom - top) * t;
        assert!((l_at(t) - ideal).abs() < 1e-3, "t={t}: {} vs {ideal}", l_at(t));
    }

    // No lightness contrast to shape: falls back to the straight ramp.
    let flat = perceptual_fade_alpha(0.5, overlay, overlay, 1.0);
    assert!((flat - 0.5).abs() < 1e-4);
}

#[test]
fn test_print_active_config() {
    let path = crate::config::get_config_path();
    println!("ACTIVE CONFIG PATH: {:?}", path);
    if let Ok(content) = std::fs::read_to_string(&path) {
        println!("FILE READ OK! Length: {}", content.len());
        let val = crate::config::parse_kdl_to_json(&content);
        println!("PARSED JSON POINTER: {:?}", val.pointer("/style/control/dropdown/color"));
    } else {
        println!("FILE READ FAILED!");
    }
    println!("DROPDOWN COLOR GETTER: {:?}", dropdown_background_color());
    println!("LIST FONT COLOR GETTER: {:?}", list_font_color());
    println!("PLATE COLOR: {:?}", plate_color());
    println!("PLATE BORDER COLOR: {:?}", plate_border_color());
    println!("PLATE BORDER THICKNESS: {:?}", plate_border_thickness());
    println!("PLATE BLUR: {:?}", plate_blur());
}
