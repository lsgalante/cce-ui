//! Text shaping for the runner: the shaped-buffer cache, family resolution,
//! the display list's text prims gathered for the glyph pass, and the
//! popover-occlusion clamp. Platform-neutral — nothing here knows the window
//! system; moved out of `window_runner` so another shell can share it.

use cosmic_text::{FontSystem, Buffer, Attrs, Metrics};
use crate::widget::TextItem;
use crate::draw::TextSpan;

#[derive(Hash, PartialEq, Eq, Clone)]
struct BufferCacheKey {
    text: String,
    size_milli: u32,
    font: Option<String>,
    is_vertical: bool,
    attrs: crate::scene::paint::TextAttrs,
}

#[derive(Clone)]
struct CachedBuffer {
    buffer: Buffer,
    last_accessed: web_time::Instant,
}

std::thread_local! {
    static BUFFER_CACHE: std::cell::RefCell<std::collections::HashMap<BufferCacheKey, CachedBuffer>> = std::cell::RefCell::new(std::collections::HashMap::new());
}

fn find_cased_family(fs: &FontSystem, name: &str) -> Option<String> {
    let lower_name = name.to_lowercase();
    for face in fs.db().faces() {
        for (family, _) in &face.families {
            if family.to_lowercase() == lower_name {
                return Some(family.clone());
            }
        }
    }
    None
}

thread_local! {
    /// Family name → is-monospaced, resolved once per family from fontdb's
    /// face metadata (the post table's isFixedPitch, as fontdb records it).
    static MONO_FAMILY_CACHE: std::cell::RefCell<std::collections::HashMap<String, bool>> =
        std::cell::RefCell::new(std::collections::HashMap::new());
}

fn family_is_monospaced(fs: &FontSystem, name: &str) -> bool {
    MONO_FAMILY_CACHE.with(|cache| {
        if let Some(&mono) = cache.borrow().get(name) {
            return mono;
        }
        let lower = name.to_lowercase();
        let mono = fs
            .db()
            .faces()
            .find(|face| face.families.iter().any(|(f, _)| f.to_lowercase() == lower))
            .map(|face| face.monospaced)
            .unwrap_or(false);
        cache.borrow_mut().insert(name.to_string(), mono);
        mono
    })
}

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
/// for `Prim::Text` prims that carry [`TextAttrs`] (the font picker's style-variant previews).
pub fn get_text_buffer_attrs(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
) -> Buffer {
    let scale = crate::scale::scale_factor();
    let mut font_size = size;
    let mut family_name = None;

    if let Some(font_str) = font {
        let (parsed_family, parsed_size) = crate::layout::parse_font_string(font_str);
        if let Some(ps) = parsed_size {
            font_size = ps;
        }
        family_name = Some(parsed_family);
    }

    let physical_size = font_size * scale;
    let size_key = (physical_size * 1000.0).round() as u32;
    let is_vertical = crate::IS_VERTICAL.load(std::sync::atomic::Ordering::Relaxed);
    let key = BufferCacheKey {
        text: text.to_string(),
        size_milli: size_key,
        font: family_name.clone(),
        is_vertical,
        attrs: text_attrs,
    };

    let cached = BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(cached_item) = cache.get_mut(&key) {
            cached_item.last_accessed = web_time::Instant::now();
            Some(cached_item.buffer.clone())
        } else {
            None
        }
    });

    if let Some(buf) = cached {
        return buf;
    }

    let line_height = if is_vertical {
        physical_size * 1.05
    } else {
        physical_size * 1.0
    };
    let metrics = Metrics::new(physical_size, line_height);
    let mut buf = Buffer::new(fs, metrics);
    let mut attrs = Attrs::new();

    let (sans_fallback, serif_fallback, mono_fallback, _) = crate::layout::read_preferred_fonts();

    let resolved_storage = family_name.as_deref().and_then(|font_name| match font_name {
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

    let family = if let Some(ref font_family) = family_name {
        match font_family.as_str() {
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
    let shaping = shaping_for(fs, text, &family);
    buf.set_text(fs, text, attrs, shaping);
    buf.shape_until_scroll(fs, true);

    BUFFER_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.len() >= 2000 {
            let mut items: Vec<(BufferCacheKey, web_time::Instant)> = cache
                .iter()
                .map(|(k, v)| (k.clone(), v.last_accessed))
                .collect();
            items.sort_by_key(|&(_, time)| time);
            for (k, _) in items.iter().take(100) {
                cache.remove(k);
            }
        }
        cache.insert(key, CachedBuffer {
            buffer: buf.clone(),
            last_accessed: web_time::Instant::now(),
        });
    });

    buf
}

/// Byte-offset → x mapping of single-line `text`, shaped exactly as the renderer draws it —
/// same buffer cache as the draw, so this is a lookup when the text is already on screen.
/// Returns ascending `(byte_idx, x)` pairs (one per cluster start, logical px, relative to
/// the text origin), terminated by `(text.len(), total_advance)`. This is the correct
/// source for caret placement and click→cursor mapping in hand-rolled text fields:
/// `measure_text_width` reports SVG-rasterized inked extent through fontdb's family
/// resolution, which disagrees with cosmic-text's advance and can even resolve a
/// different face — a caret placed with it drifts off the drawn glyphs.
pub fn shaped_cluster_offsets(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
) -> Vec<(usize, f32)> {
    let scale = crate::scale::scale_factor().max(1.0);
    let buffer = get_text_buffer(fs, text, size, font);
    let mut out: Vec<(usize, f32)> = Vec::new();
    let mut total: f32 = 0.0;
    for (start, x, w) in normalized_glyph_starts(&buffer, text) {
        if out.last().map_or(true, |&(b, _)| b != start) {
            out.push((start, x / scale));
        }
        total = total.max((x + w) / scale);
    }
    out.push((text.len(), total));
    out
}

/// Every glyph of `buffer`'s layout runs as `(start_byte, x, w)` (physical px),
/// with `start` normalized to be text-relative.
///
/// Exists because cosmic-text 0.12's `Shaping::Basic` path (`shape_skip`) emits
/// `LayoutGlyph::start` relative to the shape SPAN — it resets to 0 at every
/// word — while the Advanced path emits line-relative starts. `shaping_for`
/// picks Basic exactly for ASCII text in a monospace family (the DE's default
/// control font), so any multi-word value hit the bug: offsets keyed by those
/// starts collide on the low columns and the caret/selection walk off the
/// glyphs. A reset can ONLY come from that path, which shapes strictly one
/// glyph per char in logical order — so when one is seen, byte starts are
/// rebuilt by walking the text's chars. `text` must be the single line the
/// buffer was shaped from.
pub(crate) fn normalized_glyph_starts(buffer: &Buffer, text: &str) -> Vec<(usize, f32, f32)> {
    let mut glyphs: Vec<(usize, f32, f32)> = Vec::new();
    let mut monotonic = true;
    let mut prev = 0usize;
    for run in buffer.layout_runs() {
        for g in run.glyphs {
            if g.start < prev {
                monotonic = false;
            }
            prev = g.start;
            glyphs.push((g.start, g.x, g.w));
        }
    }
    if !monotonic {
        let mut starts = text.char_indices().map(|(i, _)| i);
        for g in glyphs.iter_mut() {
            g.0 = starts.next().unwrap_or(text.len());
        }
    }
    glyphs
}

/// Shape a boxed [`Prim::Text`] (word-wrap + alignment) and return `(buffer, vertical_offset)`.
/// Reuses [`get_text_buffer_attrs`] for all the family resolution — that returns a *clone* of the
/// cached single-run buffer, so re-applying metrics/size/align here does not touch the cache — then
/// re-lays-it-out: a 1.4 line-height (the placed-text convention), the wrap width, per-line
/// horizontal alignment, and re-shapes. The vertical offset positions the shaped block inside the
/// box per `align_v`. Uncached by construction (each box may differ in width/align).
pub fn get_text_buffer_laid_out(
    fs: &mut FontSystem,
    text: &str,
    size: f32,
    font: Option<&str>,
    text_attrs: crate::scene::paint::TextAttrs,
    layout: crate::scene::paint::TextLayout,
) -> (Buffer, f32) {
    use crate::scene::paint::{AlignH, AlignV};
    let scale = crate::scale::scale_factor();

    // Resolved family + attrs come for free (a cache clone we are free to mutate).
    let mut buf = get_text_buffer_attrs(fs, text, size, font, text_attrs);

    // The font string may override the size ("family:size") — mirror get_text_buffer_attrs.
    let mut font_size = size;
    if let Some(font_str) = font {
        if let (_, Some(ps)) = crate::layout::parse_font_string(font_str) {
            font_size = ps;
        }
    }
    let physical_size = font_size * scale;
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
    (buf, voff)
}

/// A text item's clip rect in physical pixels. This was `glyphon::TextBounds` — the one
/// glyphon-owned type cce-ui ever used, everything else being a cosmic-text re-export — so
/// it is defined here now that the dependency is cosmic-text directly. Same plain
/// four-`i32` layout; it is only an intermediate on the way to `TextSpan::bounds`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// The display list's Text prims, shaped through the shared buffer cache and
/// held for the glyph pass (the [`TextSpan`]s built by [`dl_text_spans`] borrow
/// these). Clip = the paint walk's item clip ∩ the prim's own bounds, in
/// logical space. Shared by the window's frame and the context-menu popup's.
pub(crate) fn collect_dl_text(fs: &mut FontSystem, dl: &crate::scene::paint::DisplayList, out: &mut Vec<TextItem>) {
    for item in &dl.items {
        if let crate::scene::paint::Prim::Text { text, x, y, font_size, color, alpha, font, bounds, attrs, layout } = &item.prim {
            let clip = item.clip.map(|c| [c.x, c.y, c.x + c.width, c.y + c.height]);
            let merged = match (clip, *bounds) {
                (Some(a), Some(b)) => Some([a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])]),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            // Boxed text (wrap/align) shapes uncached and shifts down by the vertical
            // offset; ordinary labels take the shared cached buffer.
            let (buffer, y_off) = match layout {
                Some(l) => get_text_buffer_laid_out(fs, text, *font_size, font.as_deref(), *attrs, *l),
                None => (get_text_buffer_attrs(fs, text, *font_size, font.as_deref(), *attrs), 0.0),
            };
            out.push(TextItem {
                buffer,
                x: *x,
                y: *y + y_off,
                color: cosmic_text::Color::rgba(
                    color[0],
                    color[1],
                    color[2],
                    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8,
                ),
                bounds: merged,
                clip_circle: item.clip_circle,
                clip_rrect: item.clip_rrect,
            });
        }
    }
}

/// The glyph pass's spans for `items`: each clamped to the surface and its
/// own bounds, then by the popover-occlusion clamp against `overlays`.
pub(crate) fn dl_text_spans<'a>(
    items: &'a [TextItem],
    scale_f32: f32,
    bounds: TextBounds,
    overlays: &[(f32, f32, f32, f32)],
) -> Vec<TextSpan<'a>> {
    let mut spans: Vec<TextSpan<'a>> = Vec::new();
    for ti in items {
        let mut item_bounds = if let Some([l, t, r, b]) = ti.bounds {
            TextBounds {
                left: ((l * scale_f32).round() as i32).clamp(0, bounds.right),
                top: ((t * scale_f32).round() as i32).clamp(0, bounds.bottom),
                right: ((r * scale_f32).round() as i32).clamp(0, bounds.right),
                bottom: ((b * scale_f32).round() as i32).clamp(0, bounds.bottom),
            }
        } else {
            bounds
        };
        popover_occlusion_clamp(overlays, ti, scale_f32, &mut item_bounds);
        spans.push(TextSpan {
            buffer: &ti.buffer,
            left: (ti.x * scale_f32).round(),
            top: (ti.y * scale_f32).round(),
            // Buffers are shaped at physical size (get_text_buffer_attrs).
            scale: 1.0,
            bounds: Some([
                item_bounds.left,
                item_bounds.top,
                item_bounds.right,
                item_bounds.bottom,
            ]),
            default_color: [
                ti.color.r() as f32 / 255.0,
                ti.color.g() as f32 / 255.0,
                ti.color.b() as f32 / 255.0,
                ti.color.a() as f32 / 255.0,
            ],
            rotation: None,
            // Circle wins when both are set (the circular pane's innermost clip);
            // otherwise a rounded-rect clip rides as center+radius with extents.
            clip_circle: match (ti.clip_circle, ti.clip_rrect) {
                (Some(c), _) => [c[0] * scale_f32, c[1] * scale_f32, c[2] * scale_f32],
                (None, Some(rr)) => [rr[0] * scale_f32, rr[1] * scale_f32, rr[4] * scale_f32],
                (None, None) => [0.0; 3],
            },
            clip_extents: match (ti.clip_circle, ti.clip_rrect) {
                (None, Some(rr)) => [rr[2] * scale_f32, rr[3] * scale_f32],
                _ => [0.0; 2],
            },
        });
    }
    spans
}

/// The popover-occlusion clamp shared by the default [`Application::text_areas`] mapping and
/// the display-list text path: clip a text item's bounds so it does not bleed through an open
/// popover's plate. A text item whose own bounds coincide with a popover rect IS that popover's
/// text and is left alone; anything else that intersects gets clamped horizontally toward
/// whichever side of the popover it starts on.
/// Clamp a text item's bounds away from the registered popover rects it
/// runs under, so page text does not bleed through a floating plate.
///
/// A text item BELONGS to a popover when it carries exactly that popover's
/// rect as its bounds (the convention every popover's own labels follow),
/// and it is then clamped only against the popovers registered AFTER its
/// own — `overlay_rects` is in stacking order, the shared context menu
/// last. Before 2026-09-22 a popover's text was exempt from its own rect
/// alone and clamped against every other, so a context menu opened over a
/// modal dialog had its labels clipped by the dialog it was drawn on top
/// of, and showed as a plate with no legible entries.
fn popover_occlusion_clamp(
    overlay_rects: &[(f32, f32, f32, f32)],
    ti: &TextItem,
    scale_f32: f32,
    item_bounds: &mut TextBounds,
) {
    let owner = ti.bounds.and_then(|[l, t, r, b]| {
        overlay_rects.iter().position(|&(ox, oy, ow, oh)| {
            (l - ox).abs() < 1.0
                && (t - oy).abs() < 1.0
                && (r - (ox + ow)).abs() < 1.0
                && (b - (oy + oh)).abs() < 1.0
        })
    });
    let first_above = owner.map_or(0, |k| k + 1);
    for &(ox, oy, ow, oh) in &overlay_rects[first_above..] {
        let ol = (ox * scale_f32).round() as i32;
        let ot = (oy * scale_f32).round() as i32;
        let or = ((ox + ow) * scale_f32).round() as i32;
        let ob = ((oy + oh) * scale_f32).round() as i32;

        let tx_pixel = ti.x * scale_f32;
        let ty_pixel = ti.y * scale_f32;

        let mut text_w = 0.0f32;
        let mut run_count = 0;
        for run in ti.buffer.layout_runs() {
            text_w = text_w.max(run.line_w);
            run_count += 1;
        }
        let text_h = run_count as f32 * ti.buffer.metrics().line_height;

        let actual_left = tx_pixel;
        let actual_right = tx_pixel + text_w;
        let actual_top = ty_pixel;
        let actual_bottom = ty_pixel + text_h;

        if actual_left < or as f32
            && actual_right > ol as f32
            && actual_top < ob as f32
            && actual_bottom > ot as f32
        {
            if tx_pixel < ol as f32 {
                item_bounds.right = item_bounds.right.min(ol);
            } else {
                item_bounds.left = item_bounds.left.max(or);
            }
        }
    }
}
