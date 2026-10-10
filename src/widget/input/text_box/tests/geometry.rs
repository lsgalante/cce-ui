//! Horizontal scrolling, the border, and where a tall box's text starts.

use super::*;

#[test]
fn test_textbox_line_wrap_disabled_horizontal_scrolling() {
    let _dummy = crate::context::UiContext::new();

    // Wrap is stated on the widget (`line_wrap_override`), not on the
    // process-global `textbox_line_wrap` — this box is single-line, which
    // is already unwrapped, and the override says so whatever the config
    // does. The global would be visible to every other test in parallel.
    let mut tb = TextBox::new("Very long text that should not wrap and instead scroll horizontally".to_string())
        .with_line_wrap(false);
    tb.set_rect(10.0, 10.0, 100.0, 30.0);
    assert!(!tb.line_wrap_enabled());

    assert_eq!(tb.scroll_x, 0.0);

    tb.focus();
    tb.cursor_idx = tb.edit_buffer.chars().count();
    tb.scroll_to_cursor();

    assert!(tb.scroll_x > 0.0, "scroll_x should be scrolled horizontally to keep the cursor visible");
}

/// A single-line box's border is always the hairline; a multiline box's
/// is whatever `textbox_multiline_border_width` resolves to. Read, never
/// written: that width is a process global and the suite runs in
/// parallel, so setting it here would widen every other test's borders.
#[test]
fn test_multiline_textbox_border_width() {
    let _dummy = crate::context::UiContext::new();
    let tb_single = TextBox::new("Singleline".to_string()).with_multiline(false);
    let tb_multi = TextBox::new("Multiline".to_string()).with_multiline(true);

    let configured = crate::layout::textbox_multiline_border_width();
    assert_eq!(tb_single.border_width(), 1.0);
    assert_eq!(tb_multi.border_width(), configured);
}

/// A tall recessed box's wall is the full `bevel_width`, deeper than the
/// old fixed 8px inset: its text starts on the well's floor, past the
/// wall, not on it (cce-fonts' preview box drew its sample over its relief).
#[test]
fn tall_box_text_starts_past_its_relief_wall() {
    let _dummy = crate::context::UiContext::new();
    let mut tb = TextBox::new("The quick brown fox".to_string())
        .with_multiline(true)
        .with_recessed(true);
    tb.set_rect(10.0, 10.0, 400.0, 180.0);
    let field = tb.well().expect("a recessed box with rounded corners carves a well");
    let floor_x = field.rect.x + field.depth;
    let floor_y = field.rect.y + field.depth;
    let first = &tb.value_labels()[0];
    assert!(first.x >= floor_x, "text x {} is on the wall (floor at {})", first.x, floor_x);
    assert!(first.y >= floor_y, "text y {} is on the wall (floor at {})", first.y, floor_y);
}
