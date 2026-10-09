//! `ParametersBg` — the parameter pane (cce-designer's, and cce-files'): a scrollable column of
//! param rows — sliders, spinboxes, dropdowns, text boxes (with an optional completion picker),
//! toggles, colours, ramps, vector groups with an optional trackball, buttons, separators,
//! collapsible sections, and an inline code editor — each row's control owned by value in
//! parallel `Vec<Option<..>>` fields. Behaviour reference: `docs/widgets.md`, "Parameter pane".
//!
//! The model caches its laid-out rect via [`Layout::rect_assigned`]; all row geometry derives
//! from it. The pane's own background is drawn by the host from [`Paint::color`] /
//! [`Paint::corner_style`], not emitted here. Its text rides the per-label view
//! ([`Paint::serves_legacy_labels`]): each label clips to the viewport, but the code editor's
//! to the code box in monospace, which the one-font one-bounds prim bridge cannot express.
//!
//! The code row is an editor, not a text box: a line-number gutter, shift+arrow / shift+click
//! selection with ctrl+c / ctrl+x / ctrl+v through the toolkit clipboard, tab and shift+tab
//! indenting by four, Enter carrying the line's indentation (a level deeper after an opening
//! bracket), ctrl+z / ctrl+shift+z on a `History` of editor snapshots, and emacs chords. Edits go
//! to the BUFFER and reach the row's VALUE on ctrl+enter, Escape, or leaving the row — never per
//! keystroke, because a host that evaluates a script on every value change would fail on every
//! half-typed line. The border says which state the row is in (amber pending, blue applied, grey
//! at rest), and [`ParametersBg::set_code_error_line`] flags the line the host's last evaluation
//! failed on.
//!
//! The pane is split by concern:
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, its constants, construction, `impl Layout` |
//! | `rows` | what a row's type string says, and parsing a row's value |
//! | `geometry` | label layout, row heights, sections, control rects, the scrollbar |
//! | `paint` | the views the pane draws, and `impl Paint` |
//! | `input` | hover, typing, committing a row, and `impl Input` |
//! | `code` | the inline code editor |
//! | `controller` | `impl ParamController`: the host's reads and writes |

mod code;
mod controller;
mod geometry;
mod input;
mod paint;
mod rows;
#[cfg(test)]
mod tests;

pub use rows::SEPARATOR;
use code::*;
use rows::*;

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::display::{Float3, TextLabel};
use crate::scene::paint::Field;
use crate::widget::input::{Button, ColorSelector, Dropdown, Ramp, Slider, Spinbox, TextBox, Toggle};
use crate::widget::{Adapted, WidgetHost, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton,
    MouseScrollDelta, NamedKey, Paint, ParamController, TextEditorState, UiContext, WidgetHostExt};

/// Width of a textpick row's picker button: the right end of the field,
/// a flush control plate reaching the field's outer edge as a dropdown
/// trigger's does, the text box's well ending at the seam beside it.
const PICK_W: f32 = crate::widget::input::dropdown::ARROW_SLOT;

pub struct ParametersBg {
    /// A row VALUE changed inside `tick` (a picker stream folded into its
    /// row) — for the host's sync, which must not run on every tick that
    /// merely animated (a scroll glide reports change every frame; syncing
    /// the whole parameter list on each was the choppy params scroll,
    /// 2026-09-20). Drained by [`Self::take_tick_value_change`].
    tick_value_changed: bool,
    /// The view every trackball in the pane is seen from
    /// ([`Self::set_trackball_view`]); kept here so rows built later start
    /// from it.
    trackball_view: [[f32; 3]; 3],
    /// The configured preference (`layout::param_labels_inline`, the
    /// `param_label_layout` style key), re-read at every rebuild.
    inline_pref: bool,
    /// The label layout the rows are laid out under NOW: inline, the pane
    /// draws each label in a column beside an unlabelled control; stacked,
    /// the control carries its own label above itself. The preference,
    /// unless the pane is too narrow for a column: [`Self::decide_inline`]
    /// re-decides on every rect assignment and [`Self::relabel_rows`]
    /// moves the labels when it flips, so geometry and widgets agree.
    inline_labels: bool,
    rect: Rect,
    display_params: Vec<(String, String, String)>,
    dragging_param: Option<usize>,
    pub focused_param: Option<usize>,
    pub code_editor: Option<TextEditorState>,
    /// The code editor's undo history: one entry per edit, typing runs
    /// coalesced by group. Lives exactly as long as the editor does.
    code_history: crate::history::History<TextEditorState>,
    /// A line (0-based) the host has flagged as the site of an error in the
    /// code row's value — a script that failed to parse. Drawn as a band
    /// under that line while the value it was reported for still stands.
    code_error_line: Option<usize>,
    mouse_pos: Option<(f32, f32)>,
    /// The parameter row under the pointer: its LABEL lifts
    /// (`own_text_labels`) and its own carves light (`paint_ui`, the tint
    /// channel) — the pane's ONE hover rule, over every control kind. The controls keep their own hovers (a spinbox's +/- buttons, a
    /// slider's band), but under the DE relief style most of them draw none
    /// at all — the spinbox's row border, the well frames and the dropdown
    /// border are flat-style chrome the relief branches never paint — so
    /// which rows answered the pointer depended on the control and the
    /// style (2026-09-28: a slider lit, the spinbox beside it did not). Set
    /// with the controls' hover in `hover_controls`, so a scroll moves it
    /// too.
    ///
    /// A label, deliberately, and not a wash over the row. A row wash was
    /// tried the same day and failed twice, both times through the relief:
    /// painted before the wells it closed the pane plate's carve-grouping
    /// window and every well in the pane flipped to the overlay shading on
    /// any hover; painted after them, its 5% white composited in LINEAR
    /// light, which lifts a well's dark shade line from 18 to ~67 while
    /// the pane face moves 46 to 76 — the hovered well's outline paled and
    /// read as the well lighting up. Text stands clear of the relief, and
    /// the tinted carve is the relief's own way of lighting ONE rim.
    hover_row: Option<usize>,
    pub sliders: Vec<Option<Adapted<Slider>>>,
    pub float3s: Vec<Option<Adapted<Float3>>>,
    pub spinboxes: Vec<Option<Adapted<Spinbox>>>,
    pub buttons: Vec<Option<Adapted<Button>>>,
    pub choices: Vec<Option<Adapted<Dropdown>>>,
    pub texts: Vec<Option<Adapted<TextBox>>>,
    pub toggles: Vec<Option<Adapted<Toggle>>>,
    pub colors: Vec<Option<crate::widget::Adapted<ColorSelector>>>,
    /// Ramp-curve rows (`"ramp"` type; value = the ramp spec string). Painted
    /// scene-path through [`ParametersBg::paint_scene_rows`] — the legacy flat
    /// views can't carry the curve/key geometry.
    pub ramps: Vec<Option<Adapted<Ramp>>>,
    /// Titles of the sections the user has collapsed by clicking their header. Keyed by
    /// title so it outlives the row rebuild `set_display_params` runs on every node change.
    collapsed: std::collections::HashSet<String>,
    visible: bool,
    pub scroll_y: f32,
    pub content_h: f32,
    scrollbar_dragging: bool,
    drag_offset_y: f32,
    /// The raise/sink hysteresis (wheel/drag raises, hover sustains, the hold decays in
    /// `tick`) — the shared [`crate::widget::ScrollbarActivity`], which was extracted FROM
    /// this widget so every app's plate-straddling scrollbar behaves the same way.
    activity: crate::widget::ScrollbarActivity,
    /// Smooth-scroll driver behind `scroll_y` (see `ScrollRegion::motion`).
    scroll_motion: crate::widget::ScrollMotion,
    /// One code-editor column's shaped advance (monospace @12, the family/size
    /// the code rows draw in), recorded by [`Paint::prepare_text`]. The caret
    /// and click→column math read it; the hardcoded 7.2 px/col they used
    /// before drifted off the glyphs. 0.0 until the first shape.
    code_char_advance: f32,
}

/// The channel: the ONLY gap a control keeps from whatever its edge meets — the
/// neighboring control, or its section's wall. Controls pack edge-to-edge; the
/// reliefs on either side (control bevel, section wall) shade the channel into a
/// narrow 3D groove.
const CHANNEL: f32 = 3.0;

/// The section carve's wall width: the DE relief scaled by the section depth
/// multiplier (`style.container.section.depth`), capped against the row height
/// (safety) and the control channel — the channel cap scales WITH the
/// multiplier, so deepening sections is an explicit choice to let the roll
/// cross the groove.
fn section_carve_depth(h: f32) -> f32 {
    let sd = crate::layout::section_depth().max(0.0);
    (crate::layout::bevel_width() * sd).min(h * 0.2).min(CHANNEL * sd)
}
/// Vertical pitch between consecutive rows. Wider than the horizontal
/// channel on purpose: each label+control pair gets its own breathing room,
/// so rows read as separate entries rather than one packed stack. Was 8
/// until 2026-09-06; a control's flush seam and the next row's label sat
/// close enough to read as one block, so the pitch is now near two channels
/// past the label's own line height.
const ROW_GAP: f32 = 14.0;
/// How far a section's title box overhangs its header row upward, and the box's height.
const TITLE_BOX_INSET: f32 = 2.0;
const TITLE_BOX_H: f32 = 28.0;
/// How far a section's content box overhangs the first and last row it wraps —
/// one channel, so the rows abut the section's top/bottom walls too.
const CONTENT_BOX_PAD: f32 = CHANNEL;
/// The section outline's color, stroke width, and its corner radii: convex (outer) corners, and the
/// concave (inner) corners where the neck joins the title and content boxes.
const SECTION_BORDER_COLOR: [f32; 4] = [0.18, 0.18, 0.27, 1.0];
const SECTION_BORDER_T: f32 = 1.0;
const SECTION_R: f32 = 13.0;
/// The throat — the concave fillet where the tab's right side turns onto the content
/// body's top edge (the tab sits flush on the body; there is no connector neck).
const SECTION_THROAT_R: f32 = 5.0;
/// The relief carve's concave inside-corner radius at the tab throat
/// (`section_fillets`) — sized against SECTION_R so inside and outside
/// corners read as one family.
const SECTION_FILLET_R: f32 = 10.0;
/// Narrowest a title box may be: both its corners plus the throat fillet. Titles run
/// wider than this in practice; it only keeps the throat clear of the corners.
const SECTION_TITLE_MIN_W: f32 = 2.0 * SECTION_R + SECTION_THROAT_R;

/// The gap between one section's bottom box edge and the next section's title box —
/// deliberately much wider than the channel the rows pack on, so sections read as
/// separate blocks. Laid out from the previous block's *drawn* bottom edge (rather
/// than the uniform row pitch, which the two boxes' overhangs eat into unequally) so the
/// gap is exact.
const SECTION_GAP: f32 = 16.0;

/// Horizontal inset of a section's boxes (title tab and content body) from the pane
/// plate's sides — deliberately the wider of the two horizontal gaps, so sections
/// float clearly inside the plate.
const SECTION_MARGIN: f32 = 16.0;
/// Horizontal gap between a control row and its parent section's side walls —
/// one channel; controls abut the section's edge.
const CONTROL_INSET: f32 = CHANNEL;
/// A row's inset from the plate: the section margin plus the controls' inset within
/// the section, so bare rows above the first section align with wrapped ones.
const ROW_X_INSET: f32 = SECTION_MARGIN + CONTROL_INSET;


impl ParametersBg {
    /// The code box's line pitch, its text inset below the box top, and the
    /// gutter that carries line numbers: four columns of digits and a gap.
    const CODE_LINE_H: f32 = 16.0;
    const CODE_TOP: f32 = 22.0;
    const CODE_BOX_TOP: f32 = 18.0;
    const CODE_GUTTER_COLS: f32 = 4.0;
    const CODE_INDENT: &'static str = "    ";

    /// Gap between the label column and the control.
    const LABEL_GAP: f32 = 16.0;

    /// The pane's own row label, and the same label under the pointer: the
    /// dialog's row and selected-row label colours, so a hovered parameter
    /// reads as the dialog's hovered command does.
    const LABEL: [u8; 3] = [0xaa, 0xaa, 0xbb];
    const LABEL_HOVER: [u8; 3] = [0xf4, 0xf4, 0xfa];
    /// The hovered row's carve tint: the lifted label's own colour, so the
    /// lit rim and the label read as one cue. Through the tint channel the
    /// shader composites the rim's light in this colour at its focus gain
    /// and the shadow in a quarter of it — the carve lights, it is not
    /// washed over (a wash over a carve pales its dark shade line in linear
    /// blending; see `hover_row`).
    const HOVER_TINT: [f32; 3] = [0xf4 as f32 / 255.0, 0xf4 as f32 / 255.0, 0xfa as f32 / 255.0];

    pub fn new() -> Adapted<ParametersBg> {
        Adapted::new(ParametersBg {
            trackball_view: crate::widget::display::float3::IDENTITY_VIEW,
            inline_pref: crate::layout::param_labels_inline(),
            inline_labels: crate::layout::param_labels_inline(),
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            display_params: Vec::new(),
            dragging_param: None,
            focused_param: None,
            code_editor: None,
            code_history: crate::history::History::with_limit(200),
            code_error_line: None,
            mouse_pos: None,
            hover_row: None,
            sliders: Vec::new(),
            float3s: Vec::new(),
            spinboxes: Vec::new(),
            buttons: Vec::new(),
            choices: Vec::new(),
            texts: Vec::new(),
            toggles: Vec::new(),
            colors: Vec::new(),
            ramps: Vec::new(),
            collapsed: std::collections::HashSet::new(),
            visible: true,
            scroll_y: 0.0,
            content_h: 0.0,
            code_char_advance: 0.0,
            scrollbar_dragging: false,
            drag_offset_y: 0.0,
            activity: crate::widget::ScrollbarActivity::new(),
            scroll_motion: crate::widget::ScrollMotion::new(),
            tick_value_changed: false,
        })
    }

    /// Row `i`'s laid-out height, ignoring collapse (the row-type table).
    /// The shortest TRACK an inline row may leave its slider. Below it the
    /// pane stacks every label above its control instead. Measured on the
    /// track — what the slider is, to the eye and the hand — not on the
    /// control's rect, which also holds the readout and its gap (and a
    /// float3's axis column): until 2026-09-28 the rect was what was
    /// measured, so a slider's track shrank to 52 px, and a float3's to
    /// 36, before the labels moved. A control with no track (spinbox, text
    /// box, dropdown, colour) is measured whole.
    pub const MIN_INLINE_TRACK_W: f32 = 120.0;

    /// See every trackball in the pane from a host's camera
    /// (`Float3::set_view`): the view's right, up and toward-the-viewer
    /// axes, in the vectors' space. Returns whether anything changed, so a
    /// host knows to draw again. Kept for the rows built after it.
    pub fn set_trackball_view(&mut self, view: [[f32; 3]; 3]) -> bool {
        let mut probe = Float3::new();
        if !probe.set_view(view) {
            return false;
        }
        let view = probe.view();
        let moved = (0..3).any(|i| (0..3).any(|k| (view[i][k] - self.trackball_view[i][k]).abs() > 1e-5));
        if moved {
            self.trackball_view = view;
            for f in self.float3s.iter_mut().flatten() {
                f.set_view(view);
            }
        }
        moved
    }

    /// Whether a float3 row's type asks for the trackball: a fourth segment,
    /// `float3:lo:hi:trackball`.
    fn has_trackball(t: &str) -> bool {
        t.starts_with("float3:") && t.split(':').nth(3) == Some("trackball")
    }

    /// The scene-path companion to the legacy views: rows whose widgets paint
    /// prims NO flat tuple view can carry (the ramp rows' curve fill, key
    /// circles, and field controls). A host rendering this panel through the
    /// legacy hatches calls this with its own `PaintCtx` inside the pane's
    /// scroll clip, after the flat chrome — or the rows draw as bare labels.
    /// Whether a row value changed inside `tick` since the last call
    /// (see `tick_value_changed`); clears the flag.
    pub fn take_tick_value_change(&mut self) -> bool {
        std::mem::take(&mut self.tick_value_changed)
    }
}

impl Layout for ParametersBg {
    /// Ungated rect landing (the legacy `set_rect` head, before its visibility gate): cache the
    /// rect all row geometry derives from, then re-derive content height/scroll/row rects.
    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
        self.refresh_scroll_metrics();
    }

}
