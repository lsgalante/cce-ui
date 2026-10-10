//! Shaping a buffer — a single run, or laid out in a box — through the cache.

use super::*;

/// The shaping mode for one text run: ASCII-only text in a MONOSPACED face
/// shapes `Basic`, everything else `Advanced`.
///
/// `Basic` bypasses OpenType substitution and positioning, and for ASCII in a
/// mono face that is exactly right: a mono font's ligatures are the one thing
/// `Advanced` adds there, and they break the grid — Chivo Mono's `liga`
/// squeezes f+i into a single-advance ﬁ glyph, which is why the bar's window
/// titles rendered "file" with a cramped fi — while mono faces carry no
/// kerning to lose. Proportional faces keep `Advanced` (their kerning and
/// ligatures are wanted — a font preview must not misrepresent the face), and
/// any non-ASCII text keeps real shaping (combining marks, emoji, complex
/// scripts) whatever the face.
pub fn shaping_for(fs: &FontSystem, text: &str, family: &cosmic_text::Family) -> cosmic_text::Shaping {
    if text.is_ascii() {
        if let cosmic_text::Family::Name(name) = family {
            if family_is_monospaced(fs, name) {
                return cosmic_text::Shaping::Basic;
            }
        }
    }
    cosmic_text::Shaping::Advanced
}

pub fn get_text_buffer(fs: &mut FontSystem, text: &str, size: f32, font: Option<&str>) -> Buffer {
    get_text_buffer_attrs(fs, text, size, font, crate::scene::paint::TextAttrs::default())
}

/// [`get_text_buffer`] plus shaping attributes (italic / weight) — the backend's shape entry
/// for `Prim::Text` prims that carry [`TextAttrs`](crate::scene::paint::TextAttrs) (the font picker's style-variant previews).
pub fn get_text_buffer_attrs(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
) -> Buffer {
    Buffer::clone(&shared_text_buffer(fs, text, size, font, text_attrs))
}

/// [`get_text_buffer_attrs`] without the copy: the cached buffer itself,
/// shared. What the frame and the toolkit's own measuring use — a `Buffer`
/// owns every shaped line and glyph, so the clone the public functions hand
/// out costs as much as the text is long. Shaped at the current window's
/// scale; a measurement that divides by a scale it read itself uses
/// [`shared_text_buffer_at`] instead.
pub(crate) fn shared_text_buffer(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
) -> Rc<Buffer> {
    shared_text_buffer_at(fs, text, size, font, text_attrs, crate::scale::scale_factor())
}

/// [`shared_text_buffer`] shaped at `scale` (physical px per logical px) rather than at a
/// scale read here. A measurement turns the buffer's physical positions into logical ones by
/// dividing by a scale; it reads that scale ONCE and passes it here, so the buffer and the
/// division agree. Two reads disagree whenever the scale changes between them (a worker
/// thread, on no window, reads the process-wide value, which any window may set), and the
/// offsets come out multiplied by their ratio.
pub(crate) fn shared_text_buffer_at(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    scale: f32,
) -> Rc<Buffer> {
    let (family_name, font_size) = match font {
        Some(font_str) => {
            let (family, parsed_size) = crate::layout::split_font_string(font_str);
            (Some(family), parsed_size.unwrap_or(size))
        }
        None => (None, size),
    };

    let physical_size = font_size * scale;
    let is_vertical = vertical_text().is_some();
    let key = BufferKey {
        size_milli: (physical_size * 1000.0).round() as u32,
        font: family_name,
        is_vertical,
        attrs: text_attrs,
        scale_bits: scale.to_bits(),
        layout: None,
    };
    if let Some((buf, _)) = buffer_cache_get(text, &key) {
        return buf;
    }
    // Before any family resolves: a face alias must be in this database first.
    sync_font_ops(fs);

    let line_height = if is_vertical {
        physical_size * 1.05
    } else {
        physical_size * 1.0
    };
    let metrics = Metrics::new(physical_size, line_height);
    let mut buf = Buffer::new(fs, metrics);
    let mut attrs = Attrs::new();

    let (sans_fallback, serif_fallback, mono_fallback, _) = crate::layout::read_preferred_fonts();

    let resolved_storage = family_name.and_then(|font_name| match font_name {
        "monospace" if !mono_fallback.is_empty() => find_cased_family(fs, &mono_fallback),
        "sans-serif" if !sans_fallback.is_empty() => find_cased_family(fs, &sans_fallback),
        "serif" if !serif_fallback.is_empty() => find_cased_family(fs, &serif_fallback),
        _ => None,
    });

    let resolved_sans = if !sans_fallback.is_empty() {
        find_cased_family(fs, &sans_fallback)
    } else {
        None
    };

    let family = if let Some(font_family) = family_name {
        match font_family {
            "monospace" => {
                if !mono_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::Name(crate::layout::get_system_monospace_font())
                    }
                } else {
                    cosmic_text::Family::Name(crate::layout::get_system_monospace_font())
                }
            }
            "sans-serif" => {
                if !sans_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::SansSerif
                    }
                } else {
                    cosmic_text::Family::SansSerif
                }
            }
            "serif" => {
                if !serif_fallback.is_empty() {
                    if let Some(ref cased) = resolved_storage {
                        cosmic_text::Family::Name(cased)
                    } else {
                        cosmic_text::Family::Serif
                    }
                } else {
                    cosmic_text::Family::Serif
                }
            }
            name => cosmic_text::Family::Name(name),
        }
    } else {
        if !sans_fallback.is_empty() {
            if let Some(ref cased) = resolved_sans {
                cosmic_text::Family::Name(cased)
            } else {
                cosmic_text::Family::SansSerif
            }
        } else {
            cosmic_text::Family::SansSerif
        }
    };
    attrs = attrs.family(family);
    if text_attrs.italic {
        attrs = attrs.style(cosmic_text::Style::Italic);
    }
    if let Some(w) = text_attrs.weight {
        attrs = attrs.weight(cosmic_text::Weight(w));
    }
    if let Some(s) = text_attrs.stretch {
        attrs = attrs.stretch(stretch_from_width_class(s));
    }
    let attrs = snap_to_family_face(fs, attrs);
    let shaping = shaping_for(fs, text, &family);
    buf.set_text(fs, text, attrs, shaping);
    buf.shape_until_scroll(fs, true);

    let buf = Rc::new(buf);
    buffer_cache_put(text, &key, Rc::clone(&buf), 0.0);
    buf
}

/// Shape a boxed [`Prim::Text`](crate::scene::paint::Prim::Text) (word-wrap + alignment) and return `(buffer, vertical_offset)`.
/// Starts from [`get_text_buffer_attrs`]'s single run for all the family resolution, then
/// re-lays it out: a 1.4 line-height (the placed-text convention), the wrap width, per-line
/// horizontal alignment, and re-shapes. The vertical offset positions the shaped block inside
/// the box per `align_v`. Cached beside the single runs, keyed by the box as well.
pub fn get_text_buffer_laid_out(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    layout: crate::scene::paint::TextLayout,
) -> (Buffer, f32) {
    let (buf, voff) = shared_laid_out_buffer(fs, text, size, font, text_attrs, layout);
    (Buffer::clone(&buf), voff)
}

/// [`get_text_buffer_laid_out`] without the copy, as [`shared_text_buffer`] is to
/// [`get_text_buffer_attrs`]. Until 2026-10-04 a boxed text was re-shaped from scratch
/// every frame it was drawn; the box is part of the key now.
pub(crate) fn shared_laid_out_buffer(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    layout: crate::scene::paint::TextLayout,
) -> (Rc<Buffer>, f32) {
    use crate::scene::paint::{AlignH, AlignV};
    let scale = crate::scale::scale_factor();

    // The font string may override the size ("family:size") — mirror shared_text_buffer.
    let (family, font_size) = match font {
        Some(font_str) => {
            let (family, parsed_size) = crate::layout::split_font_string(font_str);
            (Some(family), parsed_size.unwrap_or(size))
        }
        None => (None, size),
    };
    let physical_size = font_size * scale;
    let key = BufferKey {
        size_milli: (physical_size * 1000.0).round() as u32,
        font: family,
        is_vertical: vertical_text().is_some(),
        attrs: text_attrs,
        scale_bits: scale.to_bits(),
        layout: Some(layout),
    };
    if let Some(hit) = buffer_cache_get(text, &key) {
        return hit;
    }

    // Resolved family + attrs come from the single run; this copy is ours to re-lay-out.
    let mut buf = Buffer::clone(&shared_text_buffer_at(fs, text, size, font, text_attrs, scale));
    let line_height = physical_size * 1.4;
    buf.set_metrics(fs, Metrics::new(physical_size, line_height));
    buf.set_size(fs, layout.wrap_width.map(|w| w * scale), Some(layout.box_height * scale));

    let align = match layout.align_h {
        AlignH::Left => cosmic_text::Align::Left,
        AlignH::Center => cosmic_text::Align::Center,
        AlignH::Right => cosmic_text::Align::Right,
    };
    for line in &mut buf.lines {
        line.set_align(Some(align));
    }
    buf.shape_until_scroll(fs, true);

    // Vertical offset (logical) from the shaped run count, matching the legacy per-app math.
    let runs = buf.layout_runs().count();
    let total_h = runs as f32 * font_size * 1.4;
    let voff = match layout.align_v {
        AlignV::Top => 0.0,
        AlignV::Middle => ((layout.box_height - total_h) / 2.0).max(0.0),
        AlignV::Bottom => (layout.box_height - total_h).max(0.0),
    };
    let buf = Rc::new(buf);
    buffer_cache_put(text, &key, Rc::clone(&buf), voff);
    (buf, voff)
}
