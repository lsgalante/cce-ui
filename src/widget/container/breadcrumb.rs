//! Narrow-trait `Breadcrumb` (Phase 5k) — the first controller widget across: its
//! [`PathController`] impl is reached through the concrete `Adapted<Breadcrumb>` by deref
//! (cce-designer's `path_mut` downcasts the roster entry; Phase 6aw). Segment
//! geometry (hit zones, hover overlay, per-segment text) is derived from the paint rect in one
//! place; the right-press records the clicked segment *before* opening the shared context menu
//! via [`EventCtx::open_context_menu`], so the menu header shows that segment's path.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint, PathController,
};

/// Point size the breadcrumb text is painted at; also the size measured for segment layout so
/// the two stay in lockstep.
const BREADCRUMB_FONT_SIZE: f32 = 12.0;

/// Gap between segment buttons. Zero: the segments ABUT, and the boundary
/// between two of them is a single slanted seam (see [`SEG_SLANT`]) rather than
/// a strip of the well floor showing through.
const SEG_GAP: f32 = 0.0;

/// Lean of a seam, as horizontal run per unit of height — the boundary's top is
/// this fraction of the plate height to the RIGHT of its bottom, so it reads as
/// a "/" cut between two segments. 0.36 ≈ 20°, the slope of a "/" glyph in the
/// mono faces the breadcrumb is set in.
const SEG_SLANT: f32 = 0.36;

/// Horizontal text inset inside each segment button — the dropdown's own
/// label inset (`start_x = x + 8.0` in `Dropdown::paint_text`), so the two
/// controls share a text rhythm. Must still clear the seam's lean at the
/// plate's top and bottom edges (±`SEG_SLANT * h / 2` about mid-height —
/// 4.3px at the 24px control height), not just sit beside the text.
const SEG_PAD_X: f32 = 8.0;

/// A segment as actually laid out for painting/hit-testing: its text, its BUTTON BOX's left
/// edge and width (text sits `SEG_PAD_X` in), and logical index in
/// [`Breadcrumb::virtual_segs`] (`None` for the leading "…" ellipsis marker).
struct VisibleSeg {
    text: String,
    x: f32,
    w: f32,
    logical: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct Breadcrumb {
    pub path: Vec<String>,
    hovered: bool,
    hovered_seg: Option<usize>,
    clicked_seg: Option<usize>,
    pub right_clicked_seg: Option<usize>,
    pub network_opacity: f32,
    /// Relief stance of the segment run. `false` (the default) is the
    /// dropdown-mirror trough: the run sits flush, sunk into the surface
    /// behind a valley seam. `true` swaps the trough for a boss — the same
    /// silhouette raised out of the surface, for hosts whose breadcrumb
    /// floats in front of its plate (the designer's network editor) rather
    /// than sitting inset into a toolbar. Flat (non-relief) styling and the
    /// seams are identical in both stances.
    pub raised: bool,
}

impl Breadcrumb {
    pub fn new() -> Adapted<Breadcrumb> {
        Adapted::new(Breadcrumb {
            path: Vec::new(),
            hovered: false,
            hovered_seg: None,
            clicked_seg: None,
            right_clicked_seg: None,
            network_opacity: 1.0,
            raised: false,
        })
    }

    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    /// See [`Breadcrumb::raised`].
    pub fn set_raised(&mut self, raised: bool) {
        self.raised = raised;
    }

    pub fn path_to_seg(&self, idx: usize) -> String {
        let mut path_str = "/".to_string();
        for (i, s) in self.path.iter().enumerate() {
            if i + 1 > idx {
                break;
            }
            if path_str != "/" {
                path_str.push('/');
            }
            path_str.push_str(s);
        }
        path_str
    }

    /// The displayed segments: a root "/" then each path component. No trailing slashes —
    /// each segment renders as its own button, so the boxes are the separators.
    fn virtual_segs(&self) -> Vec<String> {
        let mut segs = vec!["/".to_string()];
        for s in &self.path {
            segs.push(s.clone());
        }
        segs
    }

    /// The breadcrumb font split into (family, size). The configured font string carries both
    /// (e.g. "Berkeley Mono 14"); the render path parses the size out of it and shapes at that
    /// size, so layout must measure with the SAME family and size — passing the whole string
    /// as a family name (which no font matches) or measuring at a different size mis-sizes
    /// every segment and makes them overlap or gap.
    fn font_and_size() -> (String, f32) {
        let (family, size) = crate::layout::parse_font_string(&crate::layout::breadcrumb_font());
        (family, size.unwrap_or(BREADCRUMB_FONT_SIZE))
    }

    /// The segments to actually paint, each with its BUTTON BOX left edge/width and its
    /// *logical* index (position in [`virtual_segs`]; `None` marks the leading "…" ellipsis).
    /// This is the one source for hit-testing, the hover overlay, the plates, and the text
    /// run.
    ///
    /// Each segment is its own button: box width = the segment's measured ink width plus
    /// `SEG_PAD_X` each side, boxes separated by `SEG_GAP`. (The old abutting-text layout
    /// measured cumulative prefixes so glyph side bearings cancelled; per-box padding
    /// absorbs the bearings instead, so per-segment measurement is enough.)
    ///
    /// When the full run is wider than the container, leading segments are dropped and
    /// replaced with a "…" marker button, so the trailing (current) segments stay visible.
    /// The last segment is always kept.
    fn visible_segs(&self, rect: Rect) -> Vec<VisibleSeg> {
        let all = self.virtual_segs();
        let avail = (rect.width - 2.0 * Self::SEG_INSET).max(0.0);

        let (font, size) = Self::font_and_size();
        let box_w =
            |s: &str| crate::widget::display::measure_text_width(s, &font, size) + 2.0 * SEG_PAD_X;

        // Lay a list of (text, logical index) out left-to-right from the widget's left edge,
        // one button box per segment.
        let place = |items: Vec<(String, Option<usize>)>| -> Vec<VisibleSeg> {
            let mut x = rect.x + Self::SEG_INSET;
            items
                .into_iter()
                .map(|(text, logical)| {
                    let w = box_w(&text);
                    let vs = VisibleSeg { text, x, w, logical };
                    x += w + SEG_GAP;
                    vs
                })
                .collect()
        };

        let full: f32 = all.iter().map(|s| box_w(s)).sum::<f32>()
            + SEG_GAP * all.len().saturating_sub(1) as f32;

        if full <= avail || all.len() <= 1 {
            return place(all.into_iter().enumerate().map(|(i, s)| (s, Some(i))).collect());
        }

        // Keep the last segment, then add trailing segments while the "…" marker button plus
        // the kept run still fits; finally prepend the marker and restore left-to-right order.
        let ell = "…".to_string();
        let mut used = box_w(&ell);
        let mut kept: Vec<(String, Option<usize>)> = Vec::new();
        for i in (0..all.len()).rev() {
            let w = SEG_GAP + box_w(&all[i]);
            if !kept.is_empty() && used + w > avail {
                break;
            }
            used += w;
            kept.push((all[i].clone(), Some(i)));
        }
        kept.push((ell, None));
        kept.reverse();
        place(kept)
    }

    /// The segment under `px` at height `py`. The interior boundaries LEAN
    /// (see [`SEG_SLANT`]), so the hit zones are parallelograms, not columns —
    /// testing x alone would put the top-left corner of a segment in its
    /// neighbor, exactly where the seam is drawn furthest from the nominal edge.
    ///
    /// The parallelograms are bounded vertically by the plate band. That bound
    /// is what makes this usable as [`Input::hit`]: `py` otherwise enters only
    /// through `lean`, which slants the seams without ever rejecting a point,
    /// so every segment would claim the full-height column beneath it.
    fn seg_at(&self, rect: Rect, px: f32, py: f32) -> Option<usize> {
        let segs = self.visible_segs(rect);
        let (py0, ph) = Self::plate_band(rect);
        if py < py0 || py >= py0 + ph {
            return None;
        }
        let mid = py0 + ph * 0.5;
        // Only interior edges lean; the run's two outer ends stay upright.
        let lean = |i: usize| -> f32 {
            if i == 0 || i >= segs.len() { 0.0 } else { SEG_SLANT * (mid - py) }
        };
        for (i, s) in segs.iter().enumerate() {
            let left = s.x + lean(i);
            let right = s.x + s.w + lean(i + 1);
            if px >= left && px < right {
                return s.logical;
            }
        }
        None
    }

    /// Inset between the widget rect and the segment plate. ZERO since the
    /// dropdown restyle: the plate fills the control box exactly as the
    /// dropdown's flush face does (its groove ring is carved OUTSIDE the box,
    /// widget and dropdown alike), so any inset here would render the
    /// breadcrumb a shorter control than the dropdown beside it. Kept as a
    /// named constant because the layout, hit zones and tests all share it.
    const SEG_INSET: f32 = 0.0;

    /// Face opacity of the raised run, applied over the configured dropdown
    /// fill's own alpha. Translucent on purpose: the floating stance pairs it
    /// with the blur-behind frost, so the face reads as glass over the
    /// content beneath rather than a solid chip.
    pub const RAISED_FACE_OPACITY: f32 = 0.5;

    /// Width of a seam's flat floor in px. Zero would meet the two walls in a
    /// perfect V; a hair of floor keeps the crease from aliasing into a dotted
    /// line as the seam's subpixel position drifts with the path text. Public
    /// because flat-path hosts engrave the seams themselves — see [`seams`].
    ///
    /// [`seams`]: Breadcrumb::seams
    pub const SEAM_WIDTH: f32 = 0.75;

    /// The plate band: (y, height). Full control height since the dropdown
    /// restyle (SEG_INSET = 0) — the seams, hover tint and hit zones all
    /// span the plate.
    fn plate_band(rect: Rect) -> (f32, f32) {
        (rect.y + Self::SEG_INSET, (rect.height - 2.0 * Self::SEG_INSET).max(0.0))
    }

    /// The ONE plate the whole segment run shares — (x, y, w, h), the full
    /// control height — or `None` when nothing is laid out.
    ///
    /// The run is a single plate, not a plate per segment: with the segments
    /// abutting, per-segment plates would put a boss wall falling and another
    /// rising within a pixel of each other at every boundary, which stacks two
    /// lighting evaluations and reads far hotter than one seam (the same reason
    /// `Prim::Ridge` exists). The divisions are engraved instead — see [`seams`].
    ///
    /// For flat-path hosts (cce-files) that mirror the relief app-side — the
    /// render_widget geometry path drops relief prims, same as the dropdown's.
    ///
    /// [`seams`]: Breadcrumb::seams
    pub fn run_box(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        let segs = self.visible_segs(rect);
        let first = segs.first()?;
        let last = segs.last()?;
        let (y, h) = Self::plate_band(rect);
        Some((first.x, y, last.x + last.w - first.x, h))
    }

    /// The seam between each pair of abutting segments, as (top, bottom) line
    /// endpoints. Each leans right at the top by [`SEG_SLANT`], so it reads as a
    /// "/" between the two names. Only interior boundaries appear here — the
    /// run's outer ends are the plate's own upright edges.
    pub fn seams(&self, rect: Rect) -> Vec<((f32, f32), (f32, f32))> {
        let segs = self.visible_segs(rect);
        let (y, h) = Self::plate_band(rect);
        let run = SEG_SLANT * h * 0.5;
        segs.iter()
            .skip(1)
            .map(|s| ((s.x + run, y), (s.x - run, y + h)))
            .collect()
    }

    fn bg_color(&self) -> [f32; 4] {
        let c = crate::color::breadcrumb_bg_color();
        [c[0], c[1], c[2], self.network_opacity]
    }
}

impl Layout for Breadcrumb {}

impl Paint for Breadcrumb {
    fn color(&self) -> [f32; 4] {
        // No whole-widget fill in either style: each segment draws its own
        // button plate in paint().
        [0.0; 4]
    }

    fn widget_font(&self) -> Option<String> {
        let font = crate::layout::breadcrumb_font();
        if font.is_empty() {
            None
        } else {
            Some(font)
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        // The segment run is ONE flush inset plate hugging its content — the
        // dropdown trigger's exact relief (`Dropdown::paint_background`'s
        // raised style: groove ring sunk around the control, its lip rolling
        // back up, face level with the window plate) — divided into segments
        // by seams engraved across it at a "/" lean. The old full-width
        // recessed well is gone: right of the run there is plain window
        // surface now, just as there is around the dropdown.
        // ONE knob with the dropdown (style.control.dropdown.corner_radius):
        // the two controls share a silhouette by construction, not by two
        // numbers happening to agree.
        let radius = crate::layout::dropdown_corner_radius();
        let relief = crate::layout::control_relief();

        let segs = self.visible_segs(rect);
        if let Some((rx, ry, rw, rh)) = self.run_box(rect) {
            let run_rect = Rect { x: rx, y: ry, width: rw, height: rh };
            let r = radius.min(rh * 0.5);
            if relief {
                let depth = crate::layout::bevel_width().min(rh * 0.2);
                // The face comes from the DROPDOWN's fill knob, not one of
                // the breadcrumb's own: the two controls sit side by side on a
                // toolbar and must read as the same material under any config.
                // A transparent configured fill is the dropdown's degraded
                // form — edges only, the window plate showing through as the
                // face, which is what the boss run always did here; an opaque
                // one makes both controls that color. Mirrors
                // `Dropdown::paint_background`'s `face` exactly.
                let raw_bg = crate::color::dropdown_background_color();
                let face = if raw_bg[3] > 0.001 {
                    let mut c = raw_bg;
                    c[3] = 1.0;
                    c
                } else {
                    [0.0; 4]
                };
                if self.raised {
                    // The floating stance: the run rises out of the surface as
                    // ONE beveled plate — fill and raised roll in a single
                    // lighting pass. The face is deliberately translucent
                    // ([`Self::RAISED_FACE_OPACITY`] over the configured fill)
                    // and ALWAYS frosted (negative alpha, the blur-behind
                    // sentinel): a floating part shows what is under it, and
                    // at this translucency the frost is what keeps the names
                    // legible over live content beneath. A transparent
                    // configured fill keeps the boss degradation: edges only,
                    // the surface as the face.
                    if face[3] > 0.001 {
                        let mut c = face;
                        c[3] = -(c[3] * Self::RAISED_FACE_OPACITY);
                        ctx.bevel(run_rect, (r, r, r, r), c, depth);
                    } else {
                        ctx.boss(run_rect, (r, r, r, r), depth);
                    }
                } else {
                    ctx.inset_plate(run_rect, (r, r, r, r), face, depth);
                }
                for (a, b) in self.seams(rect) {
                    ctx.groove(a, b, Self::SEAM_WIDTH, depth, run_rect);
                }
            } else {
                ctx.rounded_rect(run_rect, r, (true, true, true, true), self.bg_color());
                for (a, b) in self.seams(rect) {
                    ctx.vector(a.0, a.1, b.0, b.1, 1.0, [0.0, 0.0, 0.0, 0.25], crate::scene::paint::Cap::Flat);
                }
            }
        }
        // Hover wash: the WHOLE segment silhouette — flush to the slanted
        // seams, and around the run's rounded end arcs on the first/last
        // segment. No sheared primitive exists, so the wash is BANDED: one
        // thin quad per logical pixel row, each row's edges sampled from the
        // same seam-lean and corner-arc math the seams and run box use.
        // ~24 plain Quads, hover-only — and Quads survive the flat hosts'
        // rounded-quad bridge, so cce-files' mirror gets the same shape.
        if let Some(hovered) = self.hovered_seg {
            if let (Some((sx0, sw)), Some((rx, ry, rw, rh))) = (
                segs.iter().find(|s| s.logical == Some(hovered)).map(|s| (s.x, s.w)),
                self.run_box(rect),
            ) {
                let (hy, hh) = Self::plate_band(rect);
                let run = SEG_SLANT * hh * 0.5;
                let first = segs.first().map(|s| s.x) == Some(sx0);
                let last = segs.last().map(|s| s.x + s.w) == Some(sx0 + sw);
                let rr = crate::layout::dropdown_corner_radius().min(rh * 0.5);
                // The rounded end's horizontal inset at height yc.
                let arc = |yc: f32| -> f32 {
                    let dy = if yc < ry + rr {
                        rr - (yc - ry)
                    } else if yc > ry + rh - rr {
                        yc - (ry + rh - rr)
                    } else {
                        return 0.0;
                    };
                    rr - (rr * rr - dy * dy).max(0.0).sqrt()
                };
                let wash = [1.0, 1.0, 1.0, 0.06];
                let mut y = hy;
                while y < hy + hh {
                    let bh = 1.0f32.min(hy + hh - y);
                    let yc = y + bh * 0.5;
                    let t = ((yc - hy) / hh).clamp(0.0, 1.0);
                    let lean = run * (1.0 - 2.0 * t);
                    let l = if first { rx + arc(yc) } else { sx0 + lean };
                    let r_edge = if last { rx + rw - arc(yc) } else { sx0 + sw + lean };
                    if r_edge > l {
                        ctx.quad(Rect { x: l, y, width: r_edge - l, height: bh }, wash);
                    }
                    y += bh;
                }
            }
        }

        // The current directory (last logical segment) is drawn brightly; everything else,
        // including the "…" ellipsis marker, is dimmed. Paint at the configured font size so
        // the glyph run matches the widths `visible_segs` measured (and thus its x positions).
        let (_, size) = Self::font_and_size();
        let last_logical = self.virtual_segs().len().saturating_sub(1);
        for vs in segs {
            let color =
                if vs.logical == Some(last_logical) { [0xcc, 0xcc, 0xd4] } else { [0x88, 0x88, 0x99] };
            ctx.text(
                vs.text,
                vs.x + SEG_PAD_X,
                crate::layout::center_text_y(rect.y, rect.height, size),
                size,
                color,
            );
        }
    }
}

impl Input for Breadcrumb {
    /// The widget claims only its segment run, not its laid-out strip: hosts
    /// float the breadcrumb over live content (the designer's graph runs
    /// underneath), and presses on the strip's empty remainder must fall
    /// through to what's beneath. Right-clicks sharpen with it — the
    /// copy-path menu opens over the run, the content's own menu elsewhere.
    fn hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        self.seg_at(rect, px, py).is_some()
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                let r = ectx.rect;
                let was = self.hovered;
                self.hovered =
                    *px >= r.x && *px <= r.x + r.width && *py >= r.y && *py <= r.y + r.height;
                let old = self.hovered_seg;
                self.hovered_seg = if self.hovered { self.seg_at(r, *px, *py) } else { None };
                was != self.hovered || old != self.hovered_seg
            }
            Event::MouseLeave => {
                let changed = self.hovered || self.hovered_seg.is_some();
                self.hovered = false;
                self.hovered_seg = None;
                changed
            }
            Event::MouseButton {
                button: MouseButton::Right,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                // Record the segment first: the shared menu's header reads it (via the
                // `as_any` downcast in `UiContext::handle_right_click`) to title itself with
                // that segment's path, and "Copy Path" copies it.
                self.right_clicked_seg = self.seg_at(ectx.rect, *px, *py);
                ectx.open_context_menu(*px, *py);
                true
            }
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                if let Some(i) = self.seg_at(ectx.rect, *px, *py) {
                    if i < self.path.len() {
                        self.clicked_seg = Some(i);
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }


    fn context_action(&mut self, action: crate::widget::ContextAction) -> bool {
        if action != crate::widget::ContextAction::CopyPath {
            return false;
        }
        let idx = self.right_clicked_seg.unwrap_or(self.path.len());
        let path_str = self.path_to_seg(idx);
        crate::widget::clipboard::copy_to_clipboard(&path_str);
        true
    }
}

impl PathController for Breadcrumb {
    fn set_path(&mut self, segments: &[String]) {
        self.path = segments.to_vec();
    }
    fn path_click(&mut self) -> Option<usize> {
        self.clicked_seg.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;
    use crate::widget::WidgetHost;

    /// Center x of the visible segment with the given logical index, derived from the
    /// widget's own layout so the tests don't depend on the font's exact metrics.
    fn seg_center_x(breadcrumb: &Breadcrumb, rect: Rect, logical: usize) -> f32 {
        let s = breadcrumb
            .visible_segs(rect)
            .into_iter()
            .find(|s| s.logical == Some(logical))
            .expect("segment visible");
        s.x + s.w / 2.0
    }

    #[test]
    fn test_breadcrumb_clicks() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        // Set coordinates: x=10.0, y=20.0, w=300.0, h=24.0
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let ctx = UiContext::new();

        // Let's test hit_test
        assert!(breadcrumb.hit_test(15.0, 25.0, &ctx));

        // Segments abut (no inter-segment gap) and are sized by real font measurement, so
        // click coordinates are taken from the widget's own layout rather than hardcoded.

        // Click in segment 0 (/) — through the adapter's direct-dispatch mouse_input, the
        // same entry cce-files drives.
        let mut ui_ctx = UiContext::new();
        let x0 = seg_center_x(&breadcrumb, rect, 0);
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x0, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(0));

        // Click in segment 1 (home/)
        let x1 = seg_center_x(&breadcrumb, rect, 1);
        assert!(breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x1, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), Some(1));

        // Click in segment 2 (lsgalante/) — the last segment is the current dir, not a link.
        let x2 = seg_center_x(&breadcrumb, rect, 2);
        assert!(!breadcrumb.mouse_input(crate::widget::MouseButton::Left, crate::widget::ElementState::Pressed, x2, 25.0, &mut ui_ctx));
        assert_eq!(breadcrumb.path_click(), None);
    }

    #[test]
    fn test_breadcrumb_right_clicks() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let mut ui_ctx = UiContext::new();

        // Right click segment 1 (home/)
        let x1 = seg_center_x(&breadcrumb, rect, 1);
        let handled = breadcrumb.mouse_input(crate::widget::MouseButton::Right, crate::widget::ElementState::Pressed, x1, 25.0, &mut ui_ctx);
        assert!(handled);
        assert_eq!(breadcrumb.right_clicked_seg, Some(1));

        // Test path_to_seg
        assert_eq!(breadcrumb.path_to_seg(0), "/");
        assert_eq!(breadcrumb.path_to_seg(1), "/home");
        assert_eq!(breadcrumb.path_to_seg(2), "/home/lsgalante");
    }

    #[test]
    fn long_path_elides_leading_segments() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&[
            "home".to_string(),
            "lsgalante".to_string(),
            "Dropbox".to_string(),
            "cce".to_string(),
            "cce-ui".to_string(),
        ]);
        // Narrow container: the full path can't fit, so leading segments get dropped.
        breadcrumb.set_rect(0.0, 0.0, 160.0, 24.0);

        let segs = breadcrumb.visible_segs(Rect { x: 0.0, y: 0.0, width: 160.0, height: 24.0 });

        // First visible segment is the "…" ellipsis marker (no logical index → not a link).
        assert_eq!(segs.first().map(|s| s.text.as_str()), Some("…"));
        assert_eq!(segs.first().and_then(|s| s.logical), None);

        // The current directory (last logical segment) is always visible.
        let last_logical = breadcrumb.path.len(); // "/" is index 0, so path.len() == last idx
        assert_eq!(segs.last().and_then(|s| s.logical), Some(last_logical));
        assert_eq!(segs.last().map(|s| s.text.as_str()), Some("cce-ui"));

        // The run hugs the well's bevel on the left and never crosses the
        // mirrored limit on the right — a segment may reach that edge, but it
        // stops flush against it rather than running under the bevel.
        assert_eq!(segs[0].x, Breadcrumb::SEG_INSET);
        for s in &segs {
            assert!(
                s.x + s.w <= 160.0 - Breadcrumb::SEG_INSET + 0.01,
                "segment {:?} crosses the right inset",
                s.text
            );
        }

        // A kept trailing segment still hit-tests to its original logical index, so clicking
        // it navigates to the correct path.
        let visible_seg = segs.iter().rev().nth(1).unwrap();
        let hit = breadcrumb.seg_at(
            Rect { x: 0.0, y: 0.0, width: 160.0, height: 24.0 },
            visible_seg.x + 6.0,
            12.0,
        );
        assert_eq!(hit, visible_seg.logical);
    }

    #[test]
    fn short_path_is_not_elided() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        breadcrumb.set_rect(0.0, 0.0, 300.0, 24.0);

        let segs = breadcrumb.visible_segs(Rect { x: 0.0, y: 0.0, width: 300.0, height: 24.0 });
        // Root + two components, no ellipsis.
        assert_eq!(segs.len(), 3);
        assert!(segs.iter().all(|s| s.logical.is_some()));
        assert_eq!(segs[0].text, "/");
    }

    /// The seams lean like "/" — top edge to the RIGHT of the bottom — and there
    /// is exactly one per interior boundary, sitting on the shared edge at
    /// mid-height. The run plate spans all of them.
    #[test]
    fn seams_lean_right_at_the_top() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let segs = breadcrumb.visible_segs(rect);
        let seams = breadcrumb.seams(rect);
        // Root + two components ⇒ two interior boundaries.
        assert_eq!(seams.len(), 2);
        assert_eq!(seams.len(), segs.len() - 1);

        for (i, ((tx, ty), (bx, by))) in seams.iter().enumerate() {
            assert!(tx > bx, "seam {i} must lean right at the top");
            assert!(ty < by, "seam {i} top must be above its bottom");
            // Centered on the boundary it divides.
            let edge = segs[i + 1].x;
            assert!(((tx + bx) * 0.5 - edge).abs() < 0.01);
        }

        // One plate under the lot, spanning first edge to last.
        let (rx, _, rw, _) = breadcrumb.run_box(rect).expect("run laid out");
        assert_eq!(rx, segs[0].x);
        assert!((rx + rw - (segs[2].x + segs[2].w)).abs() < 0.01);
    }

    /// A point in a segment's top-left corner belongs to the segment on the
    /// LEFT: the seam has leaned right there, so the boundary is no longer the
    /// nominal edge. This is what an x-only hit test got wrong.
    #[test]
    fn hit_test_follows_the_seam_lean() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let segs = breadcrumb.visible_segs(rect);
        let (y, h) = Breadcrumb::plate_band(rect);
        let edge = segs[1].x; // boundary between "/" and "home"
        let lean = SEG_SLANT * h * 0.5;
        assert!(lean > 1.0, "the test needs a lean wide enough to probe");

        // Just right of the nominal edge, at the TOP: still segment 0.
        assert_eq!(breadcrumb.seg_at(rect, edge + lean * 0.5, y + 0.5), Some(0));
        // The same x at the BOTTOM, where the seam has leaned left: segment 1.
        assert_eq!(breadcrumb.seg_at(rect, edge + lean * 0.5, y + h - 0.5), Some(1));
        // At mid-height the seam sits on the nominal edge.
        assert_eq!(breadcrumb.seg_at(rect, edge + 0.5, y + h * 0.5), Some(1));
        assert_eq!(breadcrumb.seg_at(rect, edge - 0.5, y + h * 0.5), Some(0));
    }

    /// A segment claims its plate, not the full-height column under it. `hit()`
    /// delegates to `seg_at`, so a missing vertical bound there hands the widget
    /// every press sharing an x with the run — which is how a host's own content
    /// menu (cce-files' file rows) lost its right-click to the copy-path menu.
    #[test]
    fn hit_test_stops_at_the_plate_band() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string(), "lsgalante".to_string()]);
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let (y, h) = Breadcrumb::plate_band(rect);
        let x = seg_center_x(&breadcrumb, rect, 1);

        // Inside the band the segment answers, at the top and bottom edges too.
        assert_eq!(breadcrumb.seg_at(rect, x, y + h * 0.5), Some(1));
        assert_eq!(breadcrumb.seg_at(rect, x, y), Some(1));
        assert_eq!(breadcrumb.seg_at(rect, x, y + h - 0.5), Some(1));

        // Above and below it, nothing — however far the seams have leaned.
        assert_eq!(breadcrumb.seg_at(rect, x, y - 0.5), None);
        assert_eq!(breadcrumb.seg_at(rect, x, y + h), None);
        assert_eq!(breadcrumb.seg_at(rect, x, y + 400.0), None);

        // And the same bound through the `hit_test` hosts actually call.
        let ctx = UiContext::new();
        assert!(breadcrumb.hit_test(x, y + h * 0.5, &ctx));
        assert!(!breadcrumb.hit_test(x, y + 400.0, &ctx));
    }

    /// The run's outer ends stay upright — only edges that face another segment
    /// lean, so the first segment's left edge is a plain vertical boundary.
    #[test]
    fn outer_ends_do_not_lean() {
        let mut breadcrumb = Breadcrumb::new();
        breadcrumb.set_path(&["home".to_string()]);
        let rect = Rect { x: 10.0, y: 20.0, width: 300.0, height: 24.0 };
        breadcrumb.set_rect(rect.x, rect.y, rect.width, rect.height);

        let segs = breadcrumb.visible_segs(rect);
        let (y, h) = Breadcrumb::plate_band(rect);
        let left = segs[0].x;
        let right = segs[1].x + segs[1].w;

        for py in [y + 0.5, y + h * 0.5, y + h - 0.5] {
            assert_eq!(breadcrumb.seg_at(rect, left + 0.5, py), Some(0));
            assert_eq!(breadcrumb.seg_at(rect, left - 0.5, py), None);
            assert_eq!(breadcrumb.seg_at(rect, right - 0.5, py), Some(1));
            assert_eq!(breadcrumb.seg_at(rect, right + 0.5, py), None);
        }
    }

    #[test]
    fn path_controller_reachable_through_element() {
        let mut breadcrumb = Breadcrumb::new();
        PathController::set_path(&mut *breadcrumb, &["a".to_string()]);
        assert_eq!(breadcrumb.path, vec!["a".to_string()]);
    }
}
