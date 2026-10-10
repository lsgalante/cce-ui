//! `Breadcrumb`: a path as a run of segments on one plate, parted by seams that lean like a
//! slash; when the path is longer than the rect holds, its leading segments are elided behind a
//! marker. A click navigates to a segment, a right press opens the context menu on it, and the
//! keyboard walks the segments (Enter navigates). A host drives it through its
//! [`PathController`] impl (the designer reaches it as `&mut dyn PathController`).
//!
//! Segment geometry (hit zones, the hover wash, each segment's text) is derived from the paint
//! rect in one place; the right press records the clicked segment *before* opening the shared
//! context menu via [`EventCtx::open_context_menu`], so the menu's header shows that segment's
//! path.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, its constants, construction and setters, the path, `impl Layout`, `PathController` |
//! | `geometry` | the visible segments and their elision, hit zones, the plate band, the run box and seams |
//! | `paint` | `impl Paint` |
//! | `input` | `impl Input`: clicks, the right press, the keyboard, context actions |

mod geometry;
mod input;
mod paint;
#[cfg(test)]
mod focus_tests;
#[cfg(test)]
mod tests;

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
    /// dropdown's flush plate: the run's face level with the surface, its
    /// edge a field run's (`PaintCtx::inset_plate`), as the dropdown
    /// trigger and the buttons wear. `true` swaps it for a boss — the same
    /// silhouette raised out of the surface, for hosts whose breadcrumb
    /// floats in front of its plate (the designer's network editor) rather
    /// than sitting inset into a toolbar. Flat (non-relief) styling and the
    /// seams are identical in both stances.
    pub raised: bool,
    /// Keyboard focus (FocusIn / FocusOut): the run's rim lights and a cursor
    /// segment (`focus_seg`, a logical index) wears the wash; the arrows walk
    /// the visible segments, Enter / Space navigate to the cursor's.
    focused: bool,
    focus_seg: Option<usize>,
}

impl Breadcrumb {
    pub fn new() -> Adapted<Breadcrumb> {
        Adapted::new(Breadcrumb {
            path: Vec::new(),
            hovered: false,
            hovered_seg: None,
            clicked_seg: None,
            right_clicked_seg: None,
            focused: false,
            focus_seg: None,
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

    /// The side of the folded-segments marker glyph at font size `size`.
    fn marker_side(size: f32) -> f32 {
        (size * 0.9).round()
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

    fn bg_color(&self) -> [f32; 4] {
        let c = crate::color::breadcrumb_bg_color();
        [c[0], c[1], c[2], self.network_opacity]
    }
}

impl Layout for Breadcrumb {
    /// The segments are button plates, so the bar is one button row tall.
    fn intrinsic_size(&self) -> Option<crate::scene::layout::Size> {
        Some(crate::scene::layout::Size::new(0.0, crate::layout::button_height()))
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
