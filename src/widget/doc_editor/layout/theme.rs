//! The editor's look: the `EditorTheme` (fonts, sizes, colours), the list and quote indents, the
//! gutter, and the properties table's key column and pills.

/// Colours and fonts; colours are linear (what prims take).
#[derive(Clone, Debug, PartialEq)]
pub struct EditorTheme {
    pub body_font: String,
    pub mono_font: String,
    pub size: f32,
    /// Line height as a multiple of the font size.
    pub spacing: f32,
    pub fg: [f32; 4],
    pub dim: [f32; 4],
    pub link: [f32; 4],
    /// A note link with no target (the host says which, at paint).
    pub link_unresolved: [f32; 4],
    /// A `#tag`'s text.
    pub tag: [f32; 4],
    pub code_bg: [f32; 4],
    pub highlight_bg: [f32; 4],
    pub tag_bg: [f32; 4],
    /// A list property's item.
    pub pill_bg: [f32; 4],
    /// The bar down a block quote's side.
    pub quote_bar: [f32; 4],
    pub rule: [f32; 4],
    pub caret: [f32; 4],
    pub selection: [f32; 4],
}

impl EditorTheme {
    /// Obsidian-ish dark defaults at `size`, in the DE's sans and mono; the link, tag and
    /// quote bar colours are the theme's (`style.text.*`), read once here.
    pub fn new(size: f32) -> EditorTheme {
        let lin = crate::colors::to_linear;
        EditorTheme {
            body_font: "sans-serif".into(),
            mono_font: "monospace".into(),
            size,
            spacing: 1.55,
            fg: crate::colors::TEXT_FG,
            dim: crate::colors::TEXT_DIM,
            link: crate::colors::text_link_color(),
            link_unresolved: crate::colors::text_link_unresolved_color(),
            code_bg: [1.0, 1.0, 1.0, 0.06],
            highlight_bg: lin([1.0, 0.82, 0.0, 0.40]),
            tag: crate::colors::text_tag_color(),
            tag_bg: crate::colors::text_tag_background_color(),
            pill_bg: [1.0, 1.0, 1.0, 0.09],
            quote_bar: crate::colors::text_quote_bar_color(),
            rule: [1.0, 1.0, 1.0, 0.14],
            caret: lin([0.85, 0.85, 0.92, 1.0]),
            selection: lin([0.40, 0.45, 0.75, 0.45]),
        }
    }
}

/// Indent per list level, and the gutter a list marker sits in.
pub(super) fn indent(th: &EditorTheme) -> f32 {
    (th.size * 1.6).round()
}

pub(super) fn gutter(th: &EditorTheme) -> f32 {
    (th.size * 1.6).round()
}

pub(super) const QUOTE_STEP: f32 = 18.0;

/// Space either side of a pill's text, inside its background.
pub const PILL_PAD: f32 = 6.0;

/// Space between two pills.
pub(super) const PILL_GAP: f32 = 4.0;

/// The Properties table's key column, as the reading view sizes it.
pub fn prop_key_w(width: f32) -> f32 {
    140f32.min(width * 0.35).round()
}
