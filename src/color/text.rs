//! Text colours outside a control: the link colour (`style.text.link`) and the tag colour
//! (`style.text.tag`). The unresolved link and the tag's wash are derived from them, not set.

use super::*;

/// The built-in link colour, as a theme states it (sRGB): a violet on the dark pages.
pub(super) const DEFAULT_LINK_SRGB: [f32; 4] = [0.66, 0.55, 0.98, 1.0];

/// The built-in tag colour (sRGB): the link's violet, set apart so a theme can part them.
pub(super) const DEFAULT_TAG_SRGB: [f32; 4] = [0.66, 0.55, 0.98, 1.0];

/// The opacity of the wash behind a tag, as a share of the tag colour's own.
const TAG_WASH_ALPHA: f32 = 0.15;

/// How an unresolved link differs from a link, in OKLab: this much of the lightness and this
/// much of the chroma, the hue kept. With the default link it lands within 1/255 of
/// `[0.50, 0.44, 0.70]`, the colour the reading view drew before the key existed.
const UNRESOLVED_LIGHTNESS: f32 = 0.82;
const UNRESOLVED_CHROMA: f32 = 0.64;

pub(super) static TEXT_LINK_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TEXT_LINK_COLOR, |s| &mut s.color.TEXT_LINK_COLOR);

pub(super) static TEXT_TAG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TEXT_TAG_COLOR, |s| &mut s.color.TEXT_TAG_COLOR);

/// The colour of a link in text (linear): a URL, a note link that resolves, the link glyph.
pub fn text_link_color() -> [f32; 4] {
    style_read(&TEXT_LINK_COLOR)
}

pub fn set_text_link_color(c: [f32; 4]) {
    style_write(&TEXT_LINK_COLOR, c);
}

/// The colour of a note link with no target (linear): the link colour, darker and greyer, so
/// a theme that sets `style.text.link` moves both.
pub fn text_link_unresolved_color() -> [f32; 4] {
    unresolved_from(text_link_color())
}

/// The colour of a `#tag`'s text (linear).
pub fn text_tag_color() -> [f32; 4] {
    style_read(&TEXT_TAG_COLOR)
}

pub fn set_text_tag_color(c: [f32; 4]) {
    style_write(&TEXT_TAG_COLOR, c);
}

/// The wash behind a `#tag` (linear): the tag colour, faint, so a theme that sets
/// `style.text.tag` moves both.
pub fn text_tag_background_color() -> [f32; 4] {
    let t = text_tag_color();
    [t[0], t[1], t[2], t[3] * TAG_WASH_ALPHA]
}

fn unresolved_from(link: [f32; 4]) -> [f32; 4] {
    let [l, a, b] = linear_srgb_to_oklab([link[0], link[1], link[2]]);
    let rgb = oklab_to_linear_srgb([l * UNRESOLVED_LIGHTNESS, a * UNRESOLVED_CHROMA, b * UNRESOLVED_CHROMA]);
    [rgb[0].clamp(0.0, 1.0), rgb[1].clamp(0.0, 1.0), rgb[2].clamp(0.0, 1.0), link[3]]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The derived default is the hand-picked colour it replaces, to a level.
    #[test]
    fn the_default_unresolved_link_is_the_old_one() {
        let got = to_srgb(unresolved_from(to_linear(DEFAULT_LINK_SRGB)));
        for (g, want) in got.iter().zip([0.50, 0.44, 0.70, 1.0]) {
            assert!((g - want).abs() <= 1.5 / 255.0, "{got:?}");
        }
    }

    /// `style.text.link` sets the link and moves the unresolved link with it.
    #[test]
    fn the_link_key_sets_both_link_colours() {
        let _lock = test_color_state_lock();
        let before = text_link_unresolved_color();
        reload_colors("style {\n text {\n link \"#ff0000\"\n }\n}\n");
        let (link, unresolved) = (text_link_color(), text_link_unresolved_color());
        if let Ok(mut lock) = TEXT_LINK_COLOR.write() {
            *lock = to_linear(DEFAULT_LINK_SRGB);
        }
        reload_colors("");
        assert_eq!(link, [1.0, 0.0, 0.0, 1.0]);
        assert_ne!(unresolved, before);
        assert!(unresolved[0] < link[0] && unresolved[0] > unresolved[2], "{unresolved:?}");
    }

    /// `style.text.tag` sets the tag and its wash, and leaves the link alone; the default wash
    /// is the one the reading view drew before the key existed.
    #[test]
    fn the_tag_key_sets_the_tag_and_its_wash() {
        let _lock = test_color_state_lock();
        assert_eq!(text_tag_background_color(), to_linear([0.66, 0.55, 0.98, 0.15]));
        let link = text_link_color();
        reload_colors("style {\n text {\n tag \"#00ff00\"\n }\n}\n");
        let (tag, wash, after) = (text_tag_color(), text_tag_background_color(), text_link_color());
        if let Ok(mut lock) = TEXT_TAG_COLOR.write() {
            *lock = to_linear(DEFAULT_TAG_SRGB);
        }
        reload_colors("");
        assert_eq!(tag, [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(wash, [0.0, 1.0, 0.0, TAG_WASH_ALPHA]);
        assert_eq!(after, link);
    }
}
