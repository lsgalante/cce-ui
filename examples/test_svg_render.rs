fn main() {
    let opt = resvg::usvg::Options::default();
    let fontdb = clear_ui::widget::get_font_db();

    // 1. Original 32x120
    let svg_32_120 = r##"<svg width="32" height="120" xmlns="http://www.w3.org/2000/svg">
  <text x="16" y="60" font-family="Berkeley Mono" font-size="12" fill="#E6E6F2" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 16 60)">Audio</text>
</svg>"##;
    let tree1 = resvg::usvg::Tree::from_data(svg_32_120.as_bytes(), &opt, fontdb).unwrap();
    let mut pixmap1 = resvg::tiny_skia::Pixmap::new(32, 120).unwrap();
    resvg::render(&tree1, resvg::tiny_skia::Transform::default(), &mut pixmap1.as_mut());
    pixmap1.save_png("/home/lsgalante/Dropbox/Clear/scratch/test_32_120.png").unwrap();

    // 2. Square 120x120
    let svg_120_120 = r##"<svg width="120" height="120" xmlns="http://www.w3.org/2000/svg">
  <text x="60" y="60" font-family="Berkeley Mono" font-size="12" fill="#E6E6F2" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 60 60)">Audio</text>
</svg>"##;
    let tree2 = resvg::usvg::Tree::from_data(svg_120_120.as_bytes(), &opt, fontdb).unwrap();
    let mut pixmap2 = resvg::tiny_skia::Pixmap::new(120, 120).unwrap();
    resvg::render(&tree2, resvg::tiny_skia::Transform::default(), &mut pixmap2.as_mut());
    pixmap2.save_png("/home/lsgalante/Dropbox/Clear/scratch/test_120_120.png").unwrap();

    // Count non-transparent pixels
    let mut count = 0;
    for &alpha in pixmap1.data().iter().skip(3).step_by(4) {
        if alpha > 0 {
            count += 1;
        }
    }
    println!("test_32_120.png has {} non-transparent pixels.", count);
}

