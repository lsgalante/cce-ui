//! `cce-relief` — the relief-material control interface. A SHAPE from the
//! `scene::paint` relief family (recess, boss, ridge, trough, groove, fillet,
//! plate edge roll, and the composed inset plate) is shown as a lit
//! CROSS-SECTION of the actual edge, and the curve its walls are cut with is
//! shaped by three semantic sliders (Shoulder / Base / Bias) instead of a
//! free-form ramp.
//!
//! Named for the family, not for one member: `scene::paint`'s `Prim` doc
//! reserves "bevel" for the `Bevel` prim and the shared edge treatment, and
//! calls the family relief primitives — which is also what `control_relief`
//! gates and what the `relief` config node is called. This was `cce-bevel`
//! until the shape picker landed and made the mismatch untenable.
//!
//! Under the section runs the SHADING STRIP: the same shape put through
//! `scene::relief_shade`, the shader's own arithmetic in Rust, composited one
//! carve at a time in emission order. The section says what the shape is; the
//! strip says what it will look like. A composed shape gets one pass per carve
//! there, which is how a stack whose geometry looks reasonable can still read
//! hot.
//!
//! Every edit applies live to this process (the
//! popup's own plate, wells, and buttons ARE the preview) and logs the
//! sampled spec to stdout; Save persists to `~/.config/cce/config.kdl`
//! (`style.surface.relief`) so every cce app starts with the material —
//! or, with `--config <path>`, to that file instead (a per-app override
//! like cce-designer's), which also seeds the knobs/depth/width on open.
//!
//! `--key <dotted.key>` edits a single `(relief)` VALUE in place instead
//! (`cce_ui::relief_spec::ReliefSpec` — width/depth/wall knobs/wall
//! profile folded into one string), e.g.
//! `cce-relief --key style.surface.desktop.line_relief` for the desktop
//! grid's lines. Seeds come from that key, edits still preview live, and
//! Save rewrites only that key — the DE-wide material is untouched. The
//! edge section is not part of a `(relief)` value; a feature material has
//! one wall curve.
//!
//! The curve family is the two-exponent rational ease
//! `h(w) = w^a / (w^a + (1-w)^b)` over a bias pre-warp `w = v^g` — monotone,
//! endpoint-exact, with the shoulder (a) and base fillet (b) shaped
//! independently. Slider midpoints give a=b=2, g=1: the analytic smoothstep.
//! Curves are sampled into ramp-spec keys, so the config format and the
//! DE-wide loader are unchanged — free-form specs from cce-designer or a
//! hand-edited config still load everywhere.

use cce_ui::engine::{Application, EngineState, LogicalPosition, LogicalSize, WindowSettings};
use cce_ui::layout::RELIEF_PROFILE_IDENTITY_SPEC as IDENTITY_SPEC;
use cce_ui::scene::layout::Rect;
use cce_ui::scene::paint::{Cap, DisplayList, PaintCtx};
use cce_ui::scene::relief_shade::{self, CarveMode};
use cce_ui::widget::{
    Adapted, Button, Dropdown, ElementState, Event, KeyEvent, MouseButton, MouseScrollDelta,
    Slider, WidgetHost, WidgetId,
};
use wayland_client::QueueHandle;

const HEADER_FONT_SIZE: f32 = 13.0;
const HEADER_COLOR: [u8; 3] = [0x9a, 0x9a, 0xa4];

/// Knob ranges: depth (how hard the light falls across the wall) and width
/// (how far the wall runs, logical px). Defaults per `layout::bevel_depth` /
/// `bevel_width`.
const DEPTH_RANGE: (f32, f32) = (0.0, 0.6);
const WIDTH_RANGE: (f32, f32) = (2.0, 24.0);

/// Sample count for the spec written to config — enough that the 32-slot
/// renderer LUT sees the curve, few enough that the config line stays sane.
const SPEC_SAMPLES: usize = 17;

/// Cutaway metrics, shared by `draw_section` and the layout's shrink-wrap:
/// inner margin, the axis gutters (depth numbers left of the slab's cut
/// face, run numbers under its underside), the plateau/floor minimum band,
/// and the slab's underside room.
const CUT_MARGIN: f32 = 8.0;
const GUTTER_L: f32 = 34.0;
const GUTTER_B: f32 = 16.0;
const MIN_BAND: f32 = 36.0;
const UNDERSIDE: f32 = 18.0;
/// Height of the shading strip under the section — the band that shows what
/// the shape will actually LOOK like, as opposed to what it is.
const SHADE_STRIP_H: f32 = 14.0;

/// The height this window WANTS at a given width: every fixed row, plus the
/// cutaway at its natural (shrink-wrapped) height.
///
/// The window is CONTENT-SIZED. There is nothing here to drag to a different
/// shape — the rows are fixed and the cutaway shrink-wraps its section — so a
/// user-chosen height only ever adds dark space or squeezes the section's wall
/// to a sliver, because the proportional axes are height-bound. Kept as one
/// function so `settings()` asks for the same number the layout will use.
///
/// (The compositor may still restore a saved size over this; see the Utility
/// window-mode proposal. Until then the request is only a request.)
fn content_height(width: f32) -> f32 {
    let pad = cce_ui::layout::backplate_padding();
    let w = (width - 2.0 * pad).max(0.0);
    let gap = 14.0;
    let strip = {
        let (_, fsize) = cce_ui::layout::control_label_font_detached_parsed();
        fsize + cce_ui::layout::control_label_margin()
    };
    let knob_h = 22.0 + strip;
    let button_h = 26.0;
    let status_h = HEADER_FONT_SIZE + 4.0;
    let fixed = knob_h + gap
        + 5.0 * (knob_h + gap)
        + button_h + 8.0 + status_h + 2.0 * gap;
    let natural = (w - 2.0 * CUT_MARGIN - GUTTER_L - 2.0 * MIN_BAND)
        + 2.0 * CUT_MARGIN
        + UNDERSIDE
        + GUTTER_B
        + 2.0 * SHADE_STRIP_H
        + 6.0
        + GUTTER_B;
    2.0 * pad + fixed + natural
}

#[derive(Debug, Clone)]
enum BevelMsg {
    Exit,
}

/// Which of the two profile curves a shape's walls are drawn with — the thing
/// the knobs actually edit. Several shapes share one curve (every box carve and
/// both straddling shapes are the Wall curve), so the picker selects a SHAPE and
/// the curve follows; the caption says which one you are editing.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Curve {
    Wall,
    Edge,
}

/// Which wall of the rect the section is taken through.
///
/// Not cosmetic. A carve's shading depends on the angle between the wall's
/// outward normal and the DE light, so ONE profile reads four different ways
/// around a control — the reason a seam's two rims never match (measured on
/// cce-files' pane gap: 187 grey one side, 125 the other, same carve).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    const ALL: [Edge; 4] = [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom];

    fn label(self) -> &'static str {
        match self {
            Edge::Left => "Left wall",
            Edge::Right => "Right wall",
            Edge::Top => "Top wall",
            Edge::Bottom => "Bottom wall",
        }
    }

    /// The SDF gradient at the wall: the unit vector pointing OUT of the carve.
    fn facing(self) -> [f32; 2] {
        match self {
            Edge::Left => [-1.0, 0.0],
            Edge::Right => [1.0, 0.0],
            Edge::Top => [0.0, -1.0],
            Edge::Bottom => [0.0, 1.0],
        }
    }

    /// Does the wall run horizontally? Then its shading varies DOWN the band
    /// rather than along it, and both bands must be sampled that way to stay
    /// comparable to each other.
    fn horizontal_wall(self) -> bool {
        matches!(self, Edge::Top | Edge::Bottom)
    }
}

/// A relief shape, as a cross-section. This is the `Prim` family from
/// `scene::paint` — the point of the picker is that the section shows the shape
/// you will actually emit, not an idealised wall standing in for all of them.
///
/// Two of these are COMPOSED rather than primitive, and they are the reason the
/// picker exists: `InsetPlate` is what `PaintCtx::inset_plate` emits today (one
/// `Trough`), and `InsetStacked` is what it emitted before cce-ui@`35f5183` (a
/// `Recess` on an outset rect plus a `Boss` on the rect). Put them side by side
/// and the old one's fault is visible as geometry: same dip depth, 1.5× the run,
/// and a flat floor where the single evaluation comes to a point.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Shape {
    Recess,
    Boss,
    EdgeRoll,
    Ridge,
    Trough,
    Groove,
    Fillet,
    InsetPlate,
    InsetStacked,
}

impl Shape {
    const ALL: [Shape; 9] = [
        Shape::Recess,
        Shape::Boss,
        Shape::EdgeRoll,
        Shape::Ridge,
        Shape::Trough,
        Shape::Groove,
        Shape::Fillet,
        Shape::InsetPlate,
        Shape::InsetStacked,
    ];

    fn label(self) -> &'static str {
        match self {
            Shape::Recess => "Recess — carve, interior one step down",
            Shape::Boss => "Boss — plateau, interior one step up",
            Shape::EdgeRoll => "Plate edge — perimeter roll to the silhouette",
            Shape::Ridge => "Ridge — raised crest on the boundary",
            Shape::Trough => "Trough — sunken valley on the boundary",
            Shape::Groove => "Groove — slab valley (width 0 = a V)",
            Shape::Fillet => "Fillet — concave inside corner",
            Shape::InsetPlate => "Inset plate — flush control seam",
            Shape::InsetStacked => "Inset plate, STACKED (pre-35f5183)",
        }
    }

    /// The curve whose knobs shape this section.
    fn curve(self) -> Curve {
        match self {
            Shape::EdgeRoll => Curve::Edge,
            _ => Curve::Wall,
        }
    }

    /// Does the section end in a floor (material continues), or in air (past a
    /// silhouette)? Only the plate's edge roll ends in air.
    fn has_floor(self) -> bool {
        self != Shape::EdgeRoll
    }

    /// The run the shape's walls occupy, in WALL WIDTHS. Everything but the
    /// stacked inset is one wall wide; the stack is 1.5 because its two walls
    /// only overlap by half.
    fn run(self) -> f32 {
        match self {
            Shape::InsetStacked => 1.5,
            Shape::Groove => 1.0 + GROOVE_FLOOR,
            _ => 1.0,
        }
    }

    /// Surface height at run position `t` (in wall widths, 0 at the first wall's
    /// start). Units: 1.0 = one full wall drop DOWN into the material, negative
    /// = up out of it, 0 = the surrounding surface. `None` is air.
    fn height(self, p: &ProfileKnobs, t: f32) -> Option<f32> {
        // The profile's height curve, clamped to its plateaus.
        let h = |u: f32| -> f32 {
            if u <= 0.0 {
                0.0
            } else if u >= 1.0 {
                1.0
            } else {
                p.eval(u)
            }
        };
        // A bump straddling the run: up the first half, back down the second,
        // amplitude halved — the shader's ridge/trough construction, where one
        // profile evaluation on the folded coordinate yields both walls.
        let bump = |t: f32| -> f32 {
            let w = (2.0 * t).min(2.0 - 2.0 * t).clamp(0.0, 1.0);
            0.5 * h(w)
        };
        Some(match self {
            Shape::Recess => h(t),
            Shape::Boss => -h(t),
            Shape::EdgeRoll => {
                if t > 1.0 {
                    return None;
                }
                h(t)
            }
            Shape::Ridge => -bump(t),
            Shape::Trough | Shape::InsetPlate => bump(t),
            Shape::Groove => {
                // The trough with a flat floor of GROOVE_FLOOR spliced in at the
                // bottom — `width` in `Prim::Groove`, which is 0 for a pure V.
                if t < 0.5 {
                    bump(t)
                } else if t < 0.5 + GROOVE_FLOOR {
                    0.5 * h(1.0)
                } else {
                    bump(t - GROOVE_FLOOR)
                }
            }
            // Quarter arc, concave: the inside corner a box SDF cannot express.
            Shape::Fillet => {
                let u = t.clamp(0.0, 1.0);
                1.0 - (1.0 - u * u).max(0.0).sqrt()
            }
            // The composed stack: a recess wall stepping DOWN over [0,1] and a
            // boss wall stepping UP over [0.5,1.5]. They overlap across the
            // middle half, which is where the old code shaded twice.
            Shape::InsetStacked => h(t) - h(t - 0.5),
        })
    }

    /// The carve(s) this shape emits, as (mode, run start, run end). The
    /// shading strip composites these IN ORDER, exactly as the renderer
    /// composites one prim's cover quad over another's — which is the whole
    /// reason a stacked shape can read hot while its geometry looks fine.
    ///
    /// The plate's edge roll returns nothing: it is the shader's plate branch
    /// (fill + roll + CSG features), not the free-carve branch this models, so
    /// the strip says so rather than inventing a number.
    fn walls(self) -> Vec<(CarveMode, f32, f32)> {
        match self {
            Shape::Recess => vec![(CarveMode::Recess, 0.0, 1.0)],
            Shape::Boss => vec![(CarveMode::Boss, 0.0, 1.0)],
            Shape::Ridge => vec![(CarveMode::Ridge, 0.0, 1.0)],
            Shape::Trough | Shape::InsetPlate => vec![(CarveMode::Trough, 0.0, 1.0)],
            // The groove rejoins the free-carve path as a recess on |distance to
            // the line| - halfwidth; across the section that is a trough spread
            // over the wider run.
            Shape::Groove => vec![(CarveMode::Trough, 0.0, 1.0 + GROOVE_FLOOR)],
            // The fillet rejoins the shared path as its flat equivalent.
            Shape::Fillet => vec![(CarveMode::Recess, 0.0, 1.0)],
            Shape::EdgeRoll => Vec::new(),
            // The pre-35f5183 pair, in emission order.
            Shape::InsetStacked => {
                vec![(CarveMode::Recess, 0.0, 1.0), (CarveMode::Boss, 0.5, 1.5)]
            }
        }
    }

    /// For a composed shape, the run interval where TWO walls are live at once
    /// — the band that gets two lighting evaluations instead of one, and so the
    /// band that reads hot. `None` for a primitive shape, which has one wall
    /// everywhere by construction.
    ///
    /// Drawn as a band rather than as the two contributing curves on purpose:
    /// plotted raw, the second wall's own excursion runs a full drop the other
    /// way and leaves the section entirely, which forces the view to zoom out
    /// far enough to shrink the composite you actually came to look at.
    fn overlap(self) -> Option<(f32, f32)> {
        match self {
            Shape::InsetStacked => Some((0.5, 1.0)),
            _ => None,
        }
    }
}

/// Flat floor spliced into the `Groove` section, in wall widths — `Prim::Groove`'s
/// `width`, shown non-zero so the knob's effect is visible (0 would render as a V
/// identical to `Trough`, whose difference is the slab SDF, not the section).
const GROOVE_FLOOR: f32 = 0.45;

/// One profile section's shape state: the three knob sliders plus whether
/// the profile has diverged from the analytic default.
struct ProfileKnobs {
    shoulder: Adapted<Slider>,
    base: Adapted<Slider>,
    bias: Adapted<Slider>,
    /// False until a knob moves or config installed a real (non-identity)
    /// profile for this section: the DE renders its analytic profile and
    /// Save writes the identity sentinel.
    custom: bool,
    /// Last spec applied+logged.
    last_spec: String,
}

impl ProfileKnobs {
    /// `installed` is whether the live material actually carries a custom
    /// LUT for this profile (`layout::*_profile_slopes().is_some()` — the
    /// shader's own condition). It is NOT the same as "config carried saved
    /// knobs": Save writes the knob triples as a ride-along even for an
    /// untouched section (so the editor reopens where it was left), while
    /// writing the identity SPEC — which installs nothing. Seeding `custom`
    /// from the knobs' presence made the prediction follow the knob curve
    /// while the renderer ran analytic. The wall curve hid it (the knob
    /// midpoints ARE the analytic smoothstep); the roll exposed it (the knob
    /// family is nothing like the superellipse quadrant) — and a Save from
    /// that state would have installed a smoothstep roll DE-wide unasked.
    fn new(seed: Option<(f32, f32, f32)>, installed: bool) -> Self {
        let (s, b, c) = seed.unwrap_or((0.5, 0.5, 0.5));
        let knob = |v: f32, label: &str| {
            Slider::new()
                .with_label(label)
                .with_value(v.clamp(0.0, 1.0))
                .with_scroll(true)
                .with_band(true)
        };
        let mut this = Self {
            shoulder: knob(s, "Shoulder"),
            base: knob(b, "Base"),
            bias: knob(c, "Bias"),
            custom: installed,
            last_spec: String::new(),
        };
        this.last_spec = if this.custom { this.spec() } else { IDENTITY_SPEC.to_string() };
        this
    }

    fn values(&self) -> (f32, f32, f32) {
        (self.shoulder.inner().value(), self.base.inner().value(), self.bias.inner().value())
    }

    /// The section's height curve `h(v)` — the shared `(bevel)` curve family
    /// (`cce_ui::widget::bevel_ease`; the BevelPreview swatch draws the same).
    fn eval(&self, v: f32) -> f32 {
        let (s, b, c) = self.values();
        cce_ui::widget::bevel_ease(s, b, c, v)
    }

    /// The curve sampled as linear ramp-spec keys — what the renderer LUT
    /// and the config carry.
    fn keys(&self) -> Vec<(f32, f32)> {
        (0..SPEC_SAMPLES)
            .map(|i| {
                let v = i as f32 / (SPEC_SAMPLES - 1) as f32;
                (v, self.eval(v))
            })
            .collect()
    }

    fn spec(&self) -> String {
        cce_ui::widget::format_ramp_spec(&self.keys(), false)
    }

    fn take_change(&mut self) -> bool {
        // Bitwise-or on purpose: every slider's flag must drain.
        self.shoulder.take_change() | self.base.take_change() | self.bias.take_change()
    }

    fn set_defaults(&mut self) {
        self.shoulder.set_value(0.5);
        self.base.set_value(0.5);
        self.bias.set_value(0.5);
        self.custom = false;
        self.last_spec = IDENTITY_SPEC.to_string();
    }
}

struct BevelPopup {
    /// Which SHAPE the section shows; the curve it edits follows from it.
    profile_dropdown: Adapted<Dropdown>,
    /// Which wall of the rect the section is through — see [`Edge`].
    edge_dropdown: Adapted<Dropdown>,
    /// The carve wall — what `carve_slope` renders on every
    /// recess/boss/ridge in the DE.
    wall: ProfileKnobs,
    /// The plate perimeter roll — `roll_slope`'s descent profile.
    edge: ProfileKnobs,
    depth_slider: Adapted<Slider>,
    width_slider: Adapted<Slider>,
    save_button: Adapted<Button>,
    reset_button: Adapted<Button>,
    /// The window plate's alpha — seeded from this app's own config
    /// (`~/.config/cce/cce-relief/config.kdl`, `window { opacity }`, falling
    /// back to the pre-rename `cce-bevel` path), falling
    /// back to the DE backplate opacity. Edited on the file directly, the
    /// DE way — no dedicated control.
    plate_opacity: f32,
    /// Status line under the buttons: what the last save/reset did.
    status: String,
    /// Save/seed target: `--config <path>` retargets the editor at a
    /// specific config file (a per-app override like cce-designer's), else
    /// the shared config.kdl. Knob/depth/width seeds prefer this file.
    config_path: std::path::PathBuf,
    /// `--key <dotted.key>`: edit a single `(relief)` VALUE in place —
    /// Save serializes width/depth/wall knobs/wall profile into that one
    /// key instead of the `style.surface.relief.*` material keys, and the
    /// seeds come from it. The edge section still previews but is not part
    /// of a `(relief)` value (a feature material has one wall curve).
    target_key: Option<String>,
    /// Short label for a retargeted config ("cce-designer"), shown in the
    /// title and status so it's obvious which material is being edited.
    target_label: Option<String>,
    ui_context: cce_ui::context::UiContext,
    width: u32,
    height: u32,
    scale_factor: f64,
    needs_rebuild: bool,
    registered: bool,
    status_pos: (f32, f32),
    cut_rect: Rect,
}

/// Parse a saved "shoulder,base,bias" knob triple — the `(bevel)` value
/// format, shared with the BevelPreview swatch.
use cce_ui::widget::parse_bevel_knobs as parse_knobs;

/// Draw one profile as a lit cutaway: the material slab (plate color) inside
/// a dark opening, its surface stroked with segment lighting from the DE's
/// light azimuth. `has_floor` distinguishes the carve (wall meets a floor
/// inside the material) from the roll (the surface drops to the silhouette
/// and the material simply ends — air beyond the edge).
fn draw_section(pc: &mut PaintCtx, rect: Rect, profile: &ProfileKnobs, shape: Shape, edge: Edge) {
    let has_floor = shape.has_floor();
    let radius = 6.0f32;
    let radii = (radius, radius, radius, radius);
    pc.rounded_rect(rect, radius, (true, true, true, true), [0.08, 0.08, 0.10, 1.0]);

    // The section geometry: a flat band, the shape's walls over `run` wall
    // widths, then a closing band. PROPORTIONAL AXES: one unit of run is one
    // unit of drop in pixels, whatever the shape or the window — so a 45°
    // chamfer draws at 45°, and a shape whose run is 1.5 wall widths draws
    // half again as wide as one that runs 1. That comparison is the whole
    // point of putting the composed shapes in here, so the scale must not be
    // renormalised per shape.
    let m = CUT_MARGIN;
    let x_l = rect.x + m + GUTTER_L;
    let x_r = rect.x + rect.width - m;
    let avail_w = x_r - x_l;
    // The shading strip owns a reserved band along the bottom of the opening,
    // and the SECTION lays out in what is left. Reserving it up front is what
    // keeps the slab from expanding over it — the slab grows to the bottom of
    // whatever area it is given.
    let strip_band = 2.0 * SHADE_STRIP_H + 6.0;
    let sec_h = rect.height - strip_band;
    // stroke + slab-underside room, plus the bottom gutter
    let avail_h = sec_h - 2.0 * m - UNDERSIDE - GUTTER_B;

    let run = shape.run();
    // Vertical extent of THIS shape, sampled — a ridge lives above the surface,
    // a recess below, a trough only half a drop down.
    let (mut h_lo, mut h_hi) = (0.0f32, 0.0f32);
    for i in 0..=64 {
        let t = run * i as f32 / 64.0;
        if let Some(hv) = shape.height(profile, t) {
            h_lo = h_lo.min(hv);
            h_hi = h_hi.max(hv);
        }
    }
    let h_span = (h_hi - h_lo).max(0.25);

    // One scale for both axes (see above), the binding constraint whichever it is.
    let unit = ((avail_w - 2.0 * MIN_BAND) / run)
        .min(avail_h / h_span)
        .max(16.0);
    let wall_w = run * unit;
    let leftover = (avail_w - wall_w).max(2.0 * MIN_BAND);
    let plateau_frac = if has_floor { 0.45 } else { 0.62 };
    let plateau_w = leftover * plateau_frac;
    // y of h = 0 (the surrounding surface), placed so the whole excursion fits.
    let drawn_h = h_span * unit;
    let y_zero = rect.y
        + ((sec_h - drawn_h - UNDERSIDE - GUTTER_B) / 2.0).max(m)
        - h_lo * unit;
    let y_top = y_zero + h_lo * unit;
    let y_bot = y_zero + h_hi * unit;
    let x0 = x_l + plateau_w;
    let x1 = x0 + wall_w;

    // The axes: faint gridlines over the shape's run × depth domain, drawn
    // before the slab so the material occludes them (grid in the void only),
    // with the numbers in the gutters. x is run in wall widths, y is depth in
    // drops — 0 at the surrounding surface, positive down into the material.
    let grid = [0.25f32, 0.25, 0.28, 0.6];
    let num_color = [0x84u8, 0x84, 0x92];
    let mut gh = (h_lo / 0.25).round() * 0.25;
    while gh <= h_hi + 1e-3 {
        let gy = y_zero + gh * unit;
        pc.quad(Rect { x: x0, y: gy, width: wall_w, height: 1.0 }, grid);
        pc.text_with(
            format!("{gh:.2}"),
            rect.x + m + 2.0,
            gy - 5.0,
            10.0,
            num_color,
            Some("monospace".to_string()),
            None,
        );
        gh += 0.25;
    }
    let mut gt = 0.0f32;
    while gt <= run + 1e-3 {
        let gx = x0 + gt * unit;
        pc.quad(Rect { x: gx, y: y_top, width: 1.0, height: (y_bot - y_top).max(1.0) }, grid);
        gt += 0.25;
    }

    // A Right or Bottom wall genuinely draws MIRRORED: a wall's outward normal
    // always points at its plateau, so "plateau on the left" IS the left/top
    // wall. Flipping the run keeps the drawing, the predicted band and the real
    // swatch all describing the same physical wall — without it the prediction
    // and the swatch disagree by a reflection, which reads as a shading bug.
    let flip = matches!(edge, Edge::Right | Edge::Bottom);
    let t_of = |x: f32| -> f32 {
        if flip { (x1 - x) / unit } else { (x - x0) / unit }
    };

    // Surface height at a section x.
    let surface_y = |x: f32| -> Option<f32> {
        let inside = if flip { x >= x0 } else { x <= x1 };
        let outside = if flip { x >= x1 } else { x <= x0 };
        if outside {
            Some(y_zero)
        } else if inside {
            shape
                .height(profile, t_of(x))
                .map(|hv| y_zero + hv * unit)
        } else if has_floor {
            shape.height(profile, run).map(|hv| y_zero + hv * unit)
        } else {
            None // past the silhouette: air
        }
    };

    // A composed shape's double-shaded band: where two walls are live at once.
    // Drawn under the slab so the material still occludes it, like the grid.
    if let Some((ot0, ot1)) = shape.overlap() {
        pc.quad(
            Rect {
                x: x0 + ot0 * unit,
                y: y_top,
                width: (ot1 - ot0) * unit,
                height: (y_bot - y_top).max(1.0),
            },
            [0.55, 0.30, 0.30, 0.30],
        );
    }

    // The slab: the plate material itself, filled from the surface down to
    // the cut's bottom edge. Columns share exact edges (opaque fill, but the
    // ramp-fill rule keeps seams clean under AA).
    let mut slab = cce_ui::color::page_low_color();
    slab = [slab[0] * 1.25 + 0.03, slab[1] * 1.25 + 0.03, slab[2] * 1.25 + 0.03, 1.0];
    // A fixed slab thickness under the lowest surface, so vertical centering
    // doesn't grow a bottomless block of material.
    let slab_bot = (y_bot + 16.0).min(rect.y + sec_h - 8.0);
    let step = 2.0f32;
    let mut x = x_l;
    while x < x_r {
        let xm = (x + step / 2.0).min(x_r);
        if let Some(sy) = surface_y(xm) {
            let w = step.min(x_r - x);
            pc.quad(Rect { x, y: sy, width: w, height: (slab_bot - sy).max(0.0) }, slab);
        }
        x += step;
    }

    // Run numbers under the slab's underside, in wall widths — so a 1.5-wide
    // shape reads "…1.25 1.50" and its extra run is a number, not just a
    // feeling.
    let mut rt = 0.0f32;
    while rt <= run + 1e-3 {
        pc.text_with(
            format!("{rt:.2}"),
            (if flip { x1 - rt * unit } else { x0 + rt * unit }) - 11.0,
            slab_bot + 3.0,
            10.0,
            num_color,
            Some("monospace".to_string()),
            None,
        );
        rt += 0.25;
    }

    // THE SHADING STRIP: what the shader will actually put on screen along
    // this section, as opposed to the geometry drawn above it.
    //
    // Each of the shape's carves is evaluated with the real model and
    // composited in emission order onto the surface colour, so a shape that
    // emits two overlapping walls gets two passes here exactly as it would in
    // the frame. That is the difference the geometry cannot show: the stacked
    // inset's dip is only half again as deep as the trough's, but its strip is
    // visibly hotter, because the overlap region is lit twice.
    let strip_h = SHADE_STRIP_H;
    let strip_y = rect.y + rect.height - strip_band + 2.0;
    {
        let walls = shape.walls();
        let light = relief_shade::light_vector();
        let mat = relief_shade::Material::from_style();
        // Follow the knobs ONLY when a custom profile is actually installed.
        // Until one is, the shader runs its analytic branch, and predicting
        // from the knob curve instead quietly disagrees with it. The carve case
        // hides this — the knob midpoints ARE the analytic smoothstep — but the
        // roll's analytic form is a superellipse quadrant, nothing like the
        // knob family, and there the two differ by ~20 grey levels.
        let custom = profile.custom;
        let knob_slope = |v: f32| -> f32 {
            let d = 1.0 / 32.0;
            let (a, b) = ((v - d * 0.5).clamp(0.0, 1.0), (v + d * 0.5).clamp(0.0, 1.0));
            let taper = (v.min(1.0 - v) * 32.0 * 0.667).clamp(0.0, 1.0);
            if b <= a { 0.0 } else { (profile.eval(b) - profile.eval(a)) / (b - a) * taper }
        };
        let slope_at = |v: f32| -> f32 {
            if custom { knob_slope(v) } else { relief_shade::analytic_carve_slope(v) }
        };
        // The DE's own plate colour, NOT a swatch grey. The composite is
        // asymmetric — brightening screens toward white, darkening multiplies
        // toward black — so which lobe dominates depends on how light the
        // surface under it is, and it INVERTS between a dark plate and a light
        // one. Drawn over the wrong base the strip reverses the very thing you
        // came to judge: on this plate a wall's bright side out-measures its
        // dark side about 4:1, and over a pale swatch it reads the other way.
        let plate = cce_ui::color::page_low_color();
        let surface = [plate[0], plate[1], plate[2]];
        // A HORIZONTAL wall's shading varies down the band, not along it: the
        // run axis is y there, so the band becomes rows of one colour rather
        // than columns. Both bands do this, so they still compare to each
        // other — and at 14px the vertical version is roughly life size, since
        // a real wall is 4.8-9.3 logical px.
        let horiz = edge.horizontal_wall();
        let step = 1.0f32;
        let (scan_lo, scan_hi) = if horiz { (strip_y, strip_y + strip_h) } else { (x_l, x_r) };
        let mut x = scan_lo;
        while x < scan_hi {
            let t = if horiz {
                // One wall across the band's height, centred like the swatch's.
                // Bottom mirrors for the same reason Right does.
                let raw = (x - (strip_y + strip_h * 0.5)) / strip_h + 0.5;
                if flip { 1.0 - raw } else { raw }
            } else {
                t_of(x)
            };
            // The plate's edge roll is the OTHER shader branch: it emits its
            // own material rather than an overlay, and it ends at a silhouette
            // rather than a floor, so past f = 1 there is nothing to draw.
            if matches!(shape, Shape::EdgeRoll) {
                let roll_slope_at = |f: f32| -> f32 {
                    if !custom {
                        return relief_shade::analytic_roll_slope(f);
                    }
                    let d = 1.0 / 32.0;
                    let (a, b) = ((f - d * 0.5).clamp(0.0, 1.0), (f + d * 0.5).clamp(0.0, 1.0));
                    // Taper at the FACE end only; the silhouette keeps whatever
                    // slope the curve was drawn ending on (roll_slope's `win`).
                    let taper = (f * 32.0 * 0.667).clamp(0.0, 1.0);
                    if b <= a { 0.0 } else { (profile.eval(b) - profile.eval(a)) / (b - a) * taper }
                };
                // Past the silhouette (f > 1) there is nothing; INSIDE the
                // face (f < 0) there is the plate's own colour, untouched —
                // that is the whole point of expressing plate shading relative
                // to the flat face. Drawing nothing there, as the first cut
                // did, leaves the band empty over most of its length and looks
                // like the model failing.
                //
                // The section draws this shape with the face on the plateau
                // side and AIR past the wall — so the outward normal at the
                // drawn silhouette points along +x, which is the direction fed
                // to the model here.
                let c = if t < 0.0 {
                    Some(surface)
                } else {
                    relief_shade::plate_surface(surface, t, [1.0, 0.0], &roll_slope_at, light, &mat)
                };
                if let Some(c) = c {
                    let band = if horiz {
                        Rect { x: x_l, y: x, width: x_r - x_l, height: step.min(scan_hi - x) }
                    } else {
                        Rect { x, y: strip_y, width: step.min(scan_hi - x), height: strip_h }
                    };
                    pc.quad(band, [c[0], c[1], c[2], 1.0]);
                }
                x += step;
                continue;
            }
            let mut c = surface;
            for (mode, w0, w1) in &walls {
                let u = if horiz { t } else { (t - w0) / (w1 - w0) };
                if !(-0.02..=1.02).contains(&u) {
                    continue;
                }
                // Facing: the SDF gradient at this wall, pointing out of the
                // carve. THIS is what the edge selector changes, and it is the
                // whole of the difference between the four walls.
                let v = relief_shade::carve_shade(*mode, u, edge.facing(), &slope_at, light, &mat);
                c = relief_shade::composite(c, v);
            }
            let band = if horiz {
                Rect { x: x_l, y: x, width: x_r - x_l, height: step.min(scan_hi - x) }
            } else {
                Rect { x, y: strip_y, width: step.min(scan_hi - x), height: strip_h }
            };
            pc.quad(band, [c[0], c[1], c[2], 1.0]);
            x += step;
        }
        // THE LIVE SWATCH, directly under the prediction and on the same
        // x-scale: the shape emitted as REAL prims, through the same PaintCtx
        // as everything else, so the renderer draws it with the actual shader.
        //
        // The two bands are the anti-drift device with teeth. A shared constant
        // and a parsing test say the numbers agree; these say the PICTURES do.
        // Any divergence between them is either a bug in relief_shade or a
        // change in the shader that relief_shade has not tracked, and it shows
        // up as a visible seam between the bands rather than as silence.
        //
        // Alignment: a carve's wall straddles its box edge by ±depth/2, so
        // emitting at depth = `unit` with the edge at the section's own wall
        // centre puts the real wall over the predicted one, column for column.
        let swatch_y = strip_y + strip_h + 2.0;
        let edge_x = x0 + unit * 0.5;
        // Tall and wide: only the LEFT wall is meant to land in the band, so
        // the box's other three edges are pushed well outside it. The clip is
        // what keeps the cover quad — which inflates past the box — off the
        // section above and the sliders below.
        pc.clip(
            Rect { x: x_l, y: swatch_y, width: x_r - x_l, height: strip_h },
            |pc| {
                // OPAQUE: page_low_color carries the plate's own alpha, and
                // over the near-black opening that lands ~4 grey levels below
                // the predicted band, which then reads as a constant model
                // error it is not. The two bands must differ ONLY by shading.
                // A carve needs a surface to cut into; a PLATE brings its own
                // fill and ends at a silhouette. Backing the plate with a full
                // band of its own colour paints over the air past that
                // silhouette, so the band reads as uniform material and the
                // roll vanishes — the swatch has to be left empty for it.
                if !matches!(shape, Shape::EdgeRoll) {
                    pc.quad(
                        Rect { x: x_l, y: swatch_y, width: x_r - x_l, height: strip_h },
                        [plate[0], plate[1], plate[2], 1.0],
                    );
                }
                // The box is placed so the SELECTED wall is the one crossing
                // the band, and its other three edges are pushed far outside
                // it. For a horizontal wall the box spans the band's width and
                // its top/bottom edge sits at the band's mid-height, so the
                // wall runs across the band at depth = strip_h.
                let (tall, d_sw) = match edge {
                    Edge::Left => (
                        Rect { x: edge_x, y: swatch_y - 400.0, width: 4000.0, height: strip_h + 800.0 },
                        unit,
                    ),
                    Edge::Right => (
                        Rect { x: edge_x - 4000.0, y: swatch_y - 400.0, width: 4000.0, height: strip_h + 800.0 },
                        unit,
                    ),
                    Edge::Top => (
                        Rect { x: x_l - 400.0, y: swatch_y + strip_h * 0.5, width: (x_r - x_l) + 800.0, height: 4000.0 },
                        strip_h,
                    ),
                    Edge::Bottom => (
                        Rect { x: x_l - 400.0, y: swatch_y + strip_h * 0.5 - 4000.0, width: (x_r - x_l) + 800.0, height: 4000.0 },
                        strip_h,
                    ),
                };
                let unit = d_sw;
                let sq = (0.0, 0.0, 0.0, 0.0);
                match shape {
                    Shape::Recess | Shape::Fillet => pc.recess(tall, sq, unit),
                    Shape::Boss => pc.boss(tall, sq, unit),
                    Shape::Ridge => pc.ridge(tall, sq, unit),
                    Shape::Trough => pc.trough(tall, sq, unit),
                    Shape::InsetPlate => pc.inset_plate(tall, sq, [0.0; 4], unit),
                    Shape::Groove => {
                        let cx = x0 + unit * (0.5 + GROOVE_FLOOR * 0.5);
                        pc.groove(
                            (cx, swatch_y - 400.0),
                            (cx, swatch_y + strip_h + 400.0),
                            GROOVE_FLOOR * unit,
                            unit,
                            tall,
                        );
                    }
                    // The pair as inset_plate used to emit it, in order.
                    Shape::InsetStacked => {
                        let g = unit * 0.5;
                        let inner = Rect { x: edge_x + g, ..tall };
                        pc.recess(
                            Rect { x: inner.x - g, y: inner.y, width: inner.width + 2.0 * g, height: inner.height },
                            sq,
                            unit,
                        );
                        pc.boss(inner, sq, unit);
                    }
                    // A plate's roll runs INWARD from its silhouette, where a
                    // carve's wall straddles its edge — so this box is placed
                    // by its silhouette at the section's x1, not by a wall
                    // centre at edge_x. Half a roll of misalignment otherwise.
                    Shape::EdgeRoll => pc.plate(
                        Rect { x: x1 - 4000.0, y: swatch_y - 400.0, width: 4000.0, height: strip_h + 800.0 },
                        sq,
                        [plate[0], plate[1], plate[2], 1.0],
                        unit,
                    ),
                }
            },
        );

        if walls.is_empty() {
            pc.text_with(
                "".to_string(),
                x_l + 4.0,
                strip_y + 1.0,
                10.0,
                num_color,
                Some("monospace".to_string()),
                None,
            );
        }
    }

    // The surface stroke, lit per segment: outward normal (material below)
    // against the DE light azimuth — the same light the real walls shade by.
    let az = cce_ui::layout::light_source_position();
    let (lx, ly) = (az.cos(), -az.sin());
    let base = [0.60f32, 0.65, 0.74];
    let n_seg = 56usize;
    let seg_end = if has_floor { x_r } else { x1 };
    let mut prev = (x_l, surface_y(x_l).unwrap_or(y_top));
    for i in 1..=n_seg {
        let x = x_l + (seg_end - x_l) * i as f32 / n_seg as f32;
        let Some(y) = surface_y(x) else { break };
        let (dx, dy) = (x - prev.0, y - prev.1);
        let len = (dx * dx + dy * dy).sqrt().max(1e-3);
        let (nx, ny) = (dy / len, -dx / len);
        let lit = (nx * lx + ny * ly) * 0.35;
        let c = [
            (base[0] + lit).clamp(0.0, 1.0),
            (base[1] + lit).clamp(0.0, 1.0),
            (base[2] + lit).clamp(0.0, 1.0),
            1.0,
        ];
        pc.vector(prev.0, prev.1, x, y, 2.5, c, Cap::Round);
        prev = (x, y);
    }
    // The roll's cut face: a dimmer vertical edge closing the slab at the
    // silhouette.
    if !has_floor {
        pc.vector(x1, y_bot, x1, slab_bot, 2.0, [0.36, 0.39, 0.46, 1.0], Cap::Round);
    }

    // The opening's rim, drawn last so its shading falls over the slab edges.
    let depth = cce_ui::layout::bevel_width().min(rect.height * 0.2);
    pc.recess(rect, radii, depth);
}

impl BevelPopup {
    /// The selected shape. The dropdown index is the ONLY source; read it
    /// through here so layout and paint cannot disagree about it.
    fn active_shape(&self) -> Shape {
        Shape::ALL
            .get(self.profile_dropdown.selected)
            .copied()
            .unwrap_or(Shape::Recess)
    }

    /// The wall the section is taken through.
    fn active_edge(&self) -> Edge {
        Edge::ALL.get(self.edge_dropdown.selected).copied().unwrap_or(Edge::Left)
    }

    /// The curve the knobs are editing for the selected shape.
    fn active_curve(&self) -> Curve {
        self.active_shape().curve()
    }

    fn root_ids(&self) -> [WidgetId; 12] {
        [
            self.profile_dropdown.id(),
            self.edge_dropdown.id(),
            self.wall.shoulder.id(),
            self.wall.base.id(),
            self.wall.bias.id(),
            self.edge.shoulder.id(),
            self.edge.base.id(),
            self.edge.bias.id(),
            self.depth_slider.id(),
            self.width_slider.id(),
            self.save_button.id(),
            self.reset_button.id(),
        ]
    }

    fn roots(&mut self) -> [*mut (dyn WidgetHost + 'static); 12] {
        [
            self.profile_dropdown.as_ptr_mut(),
            self.edge_dropdown.as_ptr_mut(),
            self.wall.shoulder.as_ptr_mut(),
            self.wall.base.as_ptr_mut(),
            self.wall.bias.as_ptr_mut(),
            self.edge.shoulder.as_ptr_mut(),
            self.edge.base.as_ptr_mut(),
            self.edge.bias.as_ptr_mut(),
            self.depth_slider.as_ptr_mut(),
            self.width_slider.as_ptr_mut(),
            self.save_button.as_ptr_mut(),
            self.reset_button.as_ptr_mut(),
        ]
    }

    /// `take_*` plumbing after any routed dispatch — state-gated, so it does
    /// not matter which propagate call consumed the event.
    fn drain_widget_changes(&mut self) {
        if self.edge_dropdown.take_change() {
            self.needs_rebuild = true;
        }
        if self.profile_dropdown.take_change() {
            // Switch which profile the section shows — re-arrange parks the
            // other set's knobs off-screen.
            self.needs_rebuild = true;
        }
        if self.wall.take_change() {
            self.wall.custom = true;
            let keys = self.wall.keys();
            cce_ui::layout::set_bevel_profile_keys(&keys, false);
            let spec = self.wall.spec();
            println!("wall {spec}");
            self.wall.last_spec = spec;
            self.needs_rebuild = true;
        }
        if self.edge.take_change() {
            self.edge.custom = true;
            let keys = self.edge.keys();
            cce_ui::layout::set_roll_profile_keys(&keys, false);
            let spec = self.edge.spec();
            println!("edge {spec}");
            self.edge.last_spec = spec;
            self.needs_rebuild = true;
        }
        if self.depth_slider.take_change() {
            let v = self.depth_slider.inner().get_scaled_value();
            if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
                reg.set_float("bevel_depth", v);
            }
            println!("depth {v:.3}");
            self.needs_rebuild = true;
        }
        if self.width_slider.take_change() {
            let v = self.width_slider.inner().get_scaled_value();
            if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
                reg.set_float("bevel_width", v);
            }
            println!("width {v:.2}");
            self.needs_rebuild = true;
        }
        if self.save_button.take_click() {
            self.save_to_config();
            self.needs_rebuild = true;
        }
        if self.reset_button.take_click() {
            self.reset_live();
            self.needs_rebuild = true;
        }
    }

    /// Persist the current material to the shared config
    /// (`style.surface.relief` — the same keys every app reads at startup).
    /// Untouched sections write the identity sentinel (= analytic); the knob
    /// triples ride along so this editor reopens where you left it.
    fn save_to_config(&mut self) {
        let p = self.config_path.to_string_lossy().into_owned();
        // `--key` mode: the whole material folds into ONE `(relief)` value
        // at that key — width, depth, the wall curve, and the knob triple
        // behind it (so reopening with --key seeds these sliders). The edge
        // section is not part of a feature material; an untouched analytic
        // wall writes no profile at all.
        if let Some(key) = self.target_key.clone() {
            let spec = cce_ui::relief_spec::ReliefSpec {
                width: self.width_slider.inner().get_scaled_value(),
                depth: Some(self.depth_slider.inner().get_scaled_value()),
                knobs: Some(self.wall.values()),
                profile: self.wall.custom.then(|| self.wall.last_spec.clone()),
            };
            let ok = cce_ui::config::write_config_value_typed(
                &p,
                &key,
                &spec.serialize(),
                "style",
                Some("relief"),
            );
            self.status = if ok {
                println!("saved {key} -> {p}");
                format!("Saved — {key} holds this material.")
            } else {
                "Save FAILED — see config.kdl permissions.".to_string()
            };
            return;
        }
        let depth = format!("{:.3}", self.depth_slider.inner().get_scaled_value());
        let width = format!("{:.2}", self.width_slider.inner().get_scaled_value());
        let knob_str = |k: &ProfileKnobs| {
            let (s, b, c) = k.values();
            format!("{s:.3},{b:.3},{c:.3}")
        };
        let w = &mut |key: &str, value: &str| {
            cce_ui::config::write_config_value(&p, key, value, "style")
        };
        // The knob keys carry the (bevel) type explicitly, so a config that
        // never had them gains the annotation (and its editors' previews).
        let wb = |key: &str, value: &str| {
            cce_ui::config::write_config_value_typed(&p, key, value, "style", Some("bevel"))
        };
        let ok = w("style.surface.relief.depth", &depth)
            & w("style.surface.relief.width", &width)
            & w("style.surface.relief.profile", &self.wall.last_spec)
            & w("style.surface.relief.edge_profile", &self.edge.last_spec)
            & wb("style.surface.relief.profile_knobs", &knob_str(&self.wall))
            & wb("style.surface.relief.edge_knobs", &knob_str(&self.edge));
        self.status = if ok {
            println!("saved {p}");
            "Saved — apps pick the material up on start.".to_string()
        } else {
            "Save FAILED — see config.kdl permissions.".to_string()
        };
    }

    /// Back to the analytic material, live only (Save persists it): knobs to
    /// their midpoints, both profiles cleared, default depth/width.
    fn reset_live(&mut self) {
        self.wall.set_defaults();
        self.edge.set_defaults();
        cce_ui::layout::clear_bevel_profile();
        cce_ui::layout::clear_roll_profile();
        if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
            reg.set_float("bevel_depth", 0.15);
            reg.set_float("bevel_width", 9.3);
        }
        let (dmin, dmax) = DEPTH_RANGE;
        self.depth_slider.set_value((0.15 - dmin) / (dmax - dmin));
        let (wmin, wmax) = WIDTH_RANGE;
        self.width_slider.set_value((9.3 - wmin) / (wmax - wmin));
        self.status = "Reset to the analytic profiles (unsaved).".to_string();
        println!("reset");
    }
}

impl Application for BevelPopup {
    type Message = BevelMsg;

    fn new(
        _qh: &QueueHandle<EngineState<Self>>,
        _sender: calloop::channel::Sender<Self::Message>,
    ) -> Self {
        cce_ui::scale::set_scale_factor(1.0);
        // Force the lazy config load BEFORE reading the registry: the knob
        // strings are read directly (no getter wraps them), so nothing else
        // has triggered it yet this early in startup.
        cce_ui::layout::lazy_init_style_registry();

        // `--config <path>`: retarget Save (and the seeds below) at a
        // specific config file instead of the shared config.kdl.
        let shared_path = cce_ui::config::get_config_path();
        let mut config_path = shared_path.clone();
        let args: Vec<String> = std::env::args().collect();
        let mut target_key: Option<String> = None;
        let mut i = 1;
        while i < args.len() {
            if args[i] == "--config" && i + 1 < args.len() {
                config_path = std::path::PathBuf::from(&args[i + 1]);
                i += 1;
            } else if args[i] == "--key" && i + 1 < args.len() {
                target_key = Some(args[i + 1].clone());
                i += 1;
            }
            i += 1;
        }
        let target_label = (config_path != shared_path).then(|| {
            // An app override (~/.config/cce/<app>/config.kdl) reads best as
            // the app name; anything else as the file name.
            let parent = config_path.parent().and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned());
            match parent {
                Some(dir) if dir.starts_with("cce-") => dir,
                _ => config_path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| config_path.display().to_string()),
            }
        });

        // Seeds prefer the target file's own relief keys, falling back to
        // the DE-wide registry for anything it lacks.
        let target_relief = target_label
            .is_some()
            .then(|| std::fs::read_to_string(&config_path).ok())
            .flatten()
            .map(|c| cce_ui::config::parse_kdl_to_json(&c))
            .and_then(|v| v.pointer("/style/surface/relief").cloned());
        let rel_str = |k: &str| {
            target_relief.as_ref().and_then(|r| r.get(k)).and_then(|v| v.as_str().map(String::from))
        };
        let rel_f32 = |k: &str| {
            target_relief.as_ref().and_then(|r| r.get(k)).and_then(|v| v.as_f64()).map(|f| f as f32)
        };
        // `--key` seeds: the single `(relief)` value at that key wins over
        // both the target file's material keys and the registry. Installing
        // it live BEFORE the knob structs are built means the preview shows
        // the key's material from the first frame, and the wall's
        // `installed` flag reads the truth from the registry as usual.
        let key_spec = target_key.as_ref().and_then(|k| {
            std::fs::read_to_string(&config_path)
                .ok()
                .map(|c| cce_ui::config::parse_kdl_to_json(&c))
                .and_then(|v| v.pointer(&format!("/{}", k.replace('.', "/"))).cloned())
                .and_then(|v| v.as_str().map(String::from))
                .and_then(|s| cce_ui::relief_spec::ReliefSpec::parse(&s))
        });
        if let Some(ks) = &key_spec {
            if let Ok(mut reg) = cce_ui::layout::get_style_registry().write() {
                reg.set_float("bevel_width", ks.width);
                if let Some(d) = ks.depth {
                    reg.set_float("bevel_depth", d);
                }
            }
            cce_ui::layout::install_wall_profile_spec(ks.profile.as_deref());
        }
        let (wall_seed, edge_seed) = {
            let reg = cce_ui::layout::get_style_registry().read().unwrap();
            (
                key_spec
                    .as_ref()
                    .and_then(|s| s.knobs)
                    .or_else(|| rel_str("profile_knobs").as_deref().and_then(parse_knobs))
                    .or_else(|| reg.get_string("bevel_profile_knobs").as_deref().and_then(parse_knobs)),
                rel_str("edge_knobs")
                    .as_deref()
                    .and_then(parse_knobs)
                    .or_else(|| reg.get_string("roll_profile_knobs").as_deref().and_then(parse_knobs)),
            )
        };

        let depth = key_spec
            .as_ref()
            .and_then(|s| s.depth)
            .or_else(|| rel_f32("depth"))
            .unwrap_or_else(cce_ui::layout::bevel_depth);
        let width = key_spec
            .as_ref()
            .map(|s| s.width)
            .or_else(|| rel_f32("width"))
            .unwrap_or_else(cce_ui::layout::bevel_width);
        // A key target labels the window by the key, not the file.
        let target_label = match &target_key {
            Some(k) => {
                let parts: Vec<&str> = k.split('.').collect();
                Some(parts[parts.len().saturating_sub(2)..].join("."))
            }
            None => target_label,
        };
        let (dmin, dmax) = DEPTH_RANGE;
        let (wmin, wmax) = WIDTH_RANGE;
        // The persisted per-app plate opacity, falling back to the DE look.
        // New path first, then the pre-rename one, so an existing opacity
        // setting keeps working without a migration step.
        let plate_opacity = std::fs::read_to_string(cce_ui::config::get_app_config_path("cce-relief"))
            .or_else(|_| std::fs::read_to_string(cce_ui::config::get_app_config_path("cce-bevel")))
            .ok()
            .map(|c| cce_ui::config::parse_kdl_to_json(&c))
            .and_then(|v| {
                v.get("window")
                    .and_then(|w| w.get("opacity"))
                    .and_then(|o| o.as_f64())
                    .map(|f| (f as f32).clamp(0.0, 1.0))
            })
            .unwrap_or_else(|| cce_ui::color::active_backplate_opacity());
        Self {
            profile_dropdown: Dropdown::new(
                Shape::ALL.iter().map(|s| s.label().to_string()).collect(),
                0,
            )
            .with_label("Shape"),
            edge_dropdown: Dropdown::new(
                Edge::ALL.iter().map(|e| e.label().to_string()).collect(),
                0,
            )
            .with_label("Edge"),
            wall: ProfileKnobs::new(wall_seed, cce_ui::layout::bevel_profile_slopes().is_some()),
            edge: ProfileKnobs::new(edge_seed, cce_ui::layout::roll_profile_slopes().is_some()),
            depth_slider: Slider::new()
                .with_label("Depth")
                .with_range(dmin, dmax)
                .with_value(((depth - dmin) / (dmax - dmin)).clamp(0.0, 1.0))
                .with_readout(true)
                .with_decimals(2)
                .with_scroll(true)
                .with_band(true),
            width_slider: Slider::new()
                .with_label("Width")
                .with_range(wmin, wmax)
                .with_value(((width - wmin) / (wmax - wmin)).clamp(0.0, 1.0))
                .with_readout(true)
                .with_decimals(1)
                .with_scroll(true)
                .with_band(true),
            save_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Save"),
            reset_button: Button::new(0.0, 0.0, 0.0, 0.0).with_label("Reset"),
            plate_opacity,
            status: match (&target_key, &target_label) {
                (Some(_), Some(l)) => format!("Edits apply live; Save writes the {l} key."),
                (None, Some(l)) => format!("Edits apply live; Save writes {l}'s config."),
                _ => "Edits apply live; Save writes config.kdl.".to_string(),
            },
            config_path,
            target_key,
            target_label,
            ui_context: cce_ui::context::UiContext::new(),
            width: 520,
            height: 480,
            scale_factor: 1.0,
            needs_rebuild: true,
            registered: false,
            status_pos: (0.0, 0.0),
            cut_rect: Rect::ZERO,
        }
    }

    fn settings(&self) -> WindowSettings {
        WindowSettings {
            title: match &self.target_label {
                Some(l) => format!("Relief — {l}"),
                None => "Relief".to_string(),
            },
            app_id: "cce-relief".to_string(),
            width: 520,
            // Asked for, not guessed: the exact height the content occupies at
            // this width (see `content_height`).
            height: content_height(520.0).round() as u32,
            fullscreen: false,
            // The floor is the same content height — this window has no useful
            // smaller shape, and shrinking it only eats the cutaway.
            min_size: Some((440, content_height(440.0).round() as u32)),
        }
    }

    /// The motivating case for the mode: this window's shape IS
    /// `content_height`, so nothing — not a drag, not a remembered size —
    /// should ever dictate a different one.
    fn utility(&self) -> bool {
        true
    }

    fn update(&mut self, msg: Self::Message, _needs_rebuild: &mut bool, exit: &mut bool) {
        match msg {
            BevelMsg::Exit => *exit = true,
        }
    }

    fn tick(&mut self, dt: f32, needs_rebuild: &mut bool) {
        if self.ui_context.tick(dt) {
            self.drain_widget_changes();
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn display_list(&mut self, size: LogicalSize, scale: f64) -> Option<DisplayList> {
        if !self.registered {
            self.registered = true;
            let self_ptr = self as *mut Self;
            unsafe {
                for w in (*self_ptr).roots() {
                    let id = (*w).base().id();
                    self.ui_context.register_widget(id, w);
                }
            }
        }

        let size_changed = self.width != size.width as u32
            || self.height != size.height as u32
            || self.scale_factor != scale;
        if self.needs_rebuild || size_changed {
            self.width = size.width as u32;
            self.height = size.height as u32;
            self.scale_factor = scale;
            cce_ui::scale::set_scale_factor(scale as f32);

            // Manual column layout: the profile selector, ONE cutaway, the
            // selected profile's knobs, then the global rows. The unselected
            // profile's knobs park off-screen.
            let pad = cce_ui::layout::backplate_padding();
            let x = pad;
            let w = (self.width as f32 - 2.0 * pad).max(0.0);
            let gap = 14.0;
            let strip = {
                let (_, fsize) = cce_ui::layout::control_label_font_detached_parsed();
                fsize + cce_ui::layout::control_label_margin()
            };
            let knob_h = 22.0 + strip;
            let button_h = 26.0;
            let status_h = HEADER_FONT_SIZE + 4.0;
            // Rows above/below the cutaway: the selector row, then FIVE
            // stacked sliders, then buttons and status. Stacked rather than
            // gridded because a slider's label and readout want the full width
            // — three to a row truncated both, and the two-wide Depth/Width row
            // set a different rhythm again for no reason.
            let fixed = knob_h + gap                      // selector row
                + 5.0 * (knob_h + gap)                    // Shoulder..Width
                + button_h + 8.0 + status_h + 2.0 * gap;  // buttons + status
            // The cutaway absorbs spare height — but only up to its NATURAL
            // height for this width (the proportional square domain plus
            // gutters), so the opening shrink-wraps the section instead of
            // floating it in dark space.
            let natural = (w - 2.0 * CUT_MARGIN - GUTTER_L - 2.0 * MIN_BAND)
                + 2.0 * CUT_MARGIN
                + UNDERSIDE
                + GUTTER_B
                + 2.0 * SHADE_STRIP_H
                + 6.0
                + GUTTER_B;
            let cut_h = (self.height as f32 - 2.0 * pad - fixed).min(natural).max(90.0);

            let knob_row = |k: &mut ProfileKnobs, x: f32, y: f32| {
                k.shoulder.set_rect(x, y, w, knob_h);
                k.base.set_rect(x, y + (knob_h + gap), w, knob_h);
                k.bias.set_rect(x, y + 2.0 * (knob_h + gap), w, knob_h);
            };
            let park = |k: &mut ProfileKnobs| {
                k.shoulder.set_rect(-1000.0, -1000.0, 0.0, 0.0);
                k.base.set_rect(-1000.0, -1000.0, 0.0, 0.0);
                k.bias.set_rect(-1000.0, -1000.0, 0.0, 0.0);
            };

            let mut y = pad;
            // Shape takes the row's left portion, Edge the rest — one row,
            // because the cutaway is what should get the spare height.
            let edge_w = (w * 0.32).max(120.0).min(w * 0.5);
            let shape_w = (w - edge_w - gap).max(140.0);
            self.profile_dropdown.set_rect(x, y, shape_w, knob_h);
            self.edge_dropdown.set_rect(x + shape_w + gap, y, edge_w, knob_h);
            y += knob_h + gap;
            self.cut_rect = Rect { x, y, width: w, height: cut_h };
            y += cut_h + gap;
            // Keyed off the SHAPE's curve, never the dropdown index — several
            // shapes share the Wall curve, and this has to agree with the paint
            // side's `wall_active` or the row is laid out for one set and drawn
            // from the other, which parks every knob off-screen and looks like
            // the sliders vanished.
            if self.active_curve() == Curve::Wall {
                knob_row(&mut self.wall, x, y);
                park(&mut self.edge);
            } else {
                knob_row(&mut self.edge, x, y);
                park(&mut self.wall);
            }
            y += 3.0 * (knob_h + gap);
            self.depth_slider.set_rect(x, y, w, knob_h);
            y += knob_h + gap;
            self.width_slider.set_rect(x, y, w, knob_h);
            y += knob_h + gap;
            self.save_button.set_rect(x, y, 96.0, button_h);
            self.reset_button.set_rect(x + 96.0 + 12.0, y, 96.0, button_h);
            y += button_h + 8.0;
            self.status_pos = (x, y);

            self.needs_rebuild = false;
            self.ui_context.rebuild_spatial_grid();
        }

        // BOTH selectors, not just the shape one. A dropdown whose popover is
        // neither registered nor rendered still OPENS on a press — it just
        // opens invisibly, so its items cannot be hit and the widget reads as
        // completely dead. That is what adding the Edge selector looked like
        // until this list grew: paint, layout, registration and routing were
        // all correct and the thing still did nothing.
        self.ui_context.clear_popovers();
        if self.profile_dropdown.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.profile_dropdown);
        }
        if self.edge_dropdown.popover_rect().is_some() {
            self.ui_context.register_popover(&mut self.edge_dropdown);
        }

        let mut pc = PaintCtx::new();
        let (w, h) = (self.width as f32, self.height as f32);

        // The window plate at full opacity on purpose: its rolled perimeter
        // previews the edge profile, and the wells/buttons preview the wall
        // profile — the popup is its own material sample.
        let mut plate = cce_ui::color::page_low_color();
        plate[3] = self.plate_opacity;
        let radius = cce_ui::colors::backplate_corner_radius();
        let bevel = cce_ui::layout::bevel_width();
        pc.plate(
            Rect { x: 0.0, y: 0.0, width: w, height: h },
            (radius, radius, radius, radius),
            plate,
            bevel,
        );

        pc.text_with(
            self.status.clone(),
            self.status_pos.0,
            self.status_pos.1,
            HEADER_FONT_SIZE,
            HEADER_COLOR,
            Some("monospace".to_string()),
            None,
        );

        let shape = self.active_shape();
        let wall_active = shape.curve() == Curve::Wall;
        let active = if wall_active { &self.wall } else { &self.edge };
        draw_section(&mut pc, self.cut_rect, active, shape, self.active_edge());

        let knobs = if wall_active { &self.wall } else { &self.edge };
        for s in [
            &knobs.shoulder,
            &knobs.base,
            &knobs.bias,
            &self.depth_slider,
            &self.width_slider,
        ] {
            cce_ui::scene::painter::paint_root_into(&self.ui_context, s, &mut pc);
        }
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.edge_dropdown, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.profile_dropdown, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.save_button, &mut pc);
        cce_ui::scene::painter::paint_root_into(&self.ui_context, &self.reset_button, &mut pc);

        // The selector's popover, drawn into the frame on top of everything
        // below it (its labels carry the popover rect as bounds).
        if self.profile_dropdown.popover_rect().is_some() {
            // PaintCtx is a RenderTarget: the popover draws its real prims (the
            // dropdown's expanded inset-plate surface) with its own bounds.
            self.profile_dropdown.render_popover(&mut pc);
        }
        if self.edge_dropdown.popover_rect().is_some() {
            self.edge_dropdown.render_popover(&mut pc);
        }

        // The shared context menu (slider Copy/Paste), last, on top.
        if self.ui_context.is_context_menu_visible() {
            for (qx, qy, qw, qh, c) in self.ui_context.context_menu_quads() {
                pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, c);
            }
            let (mx, my, mw, mh) = (
                cce_ui::widget::context_menu::x(),
                cce_ui::widget::context_menu::y(),
                cce_ui::widget::context_menu::w(),
                cce_ui::widget::context_menu::h(),
            );
            let bounds = Some([mx, my, mx + mw, my + mh]);
            for l in self.ui_context.context_menu_labels() {
                pc.text_with(l.text, l.x, l.y, l.font_size, l.color, None, bounds);
            }
        }

        Some(pc.finish())
    }

    fn display_list_text(&self) -> bool {
        true
    }

    fn ui_context(&self) -> Option<&cce_ui::context::UiContext> {
        Some(&self.ui_context)
    }

    // Engine-driven animation frames for the dropdown expand/contract.
    fn ui_context_mut(&mut self) -> Option<&mut cce_ui::context::UiContext> {
        Some(&mut self.ui_context)
    }

    fn is_movable_backplate_at(&self, px: f32, py: f32) -> bool {
        self.ui_context.drag_allowed_at(px, py)
    }

    fn clear_color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn handle_pointer_move(&mut self, pos: LogicalPosition, needs_rebuild: &mut bool) {
        if self.ui_context.cursor_moved_context_menu(pos.x, pos.y) {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        let ev = Event::PointerMove { x: pos.x, y: pos.y, local_x: pos.x, local_y: pos.y };
        let mut changed = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                changed = true;
            }
        }
        self.drain_widget_changes();
        if changed || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn handle_mouse_input(
        &mut self,
        button: MouseButton,
        state: ElementState,
        pos: LogicalPosition,
        needs_rebuild: &mut bool,
    ) -> Option<Self::Message> {
        // The open context menu owns the press (item dispatch / dismiss).
        if self.ui_context.mouse_input_context_menu(button, state, pos.x, pos.y) {
            self.drain_widget_changes();
            *needs_rebuild = true;
            self.needs_rebuild = true;
            return None;
        }
        let ev = Event::MouseButton {
            button,
            state,
            x: pos.x,
            y: pos.y,
            local_x: pos.x,
            local_y: pos.y,
        };
        let mut changed = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                changed = true;
            }
        }
        self.drain_widget_changes();
        if changed || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        None
    }

    fn handle_mouse_wheel(
        &mut self,
        delta: &MouseScrollDelta,
        pos: LogicalPosition,
        needs_rebuild: &mut bool,
    ) {
        let ev = Event::MouseWheel {
            delta: delta.clone(),
            x: pos.x,
            y: pos.y,
            local_x: pos.x,
            local_y: pos.y,
        };
        let mut changed = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                changed = true;
            }
        }
        self.drain_widget_changes();
        if changed || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
    }

    fn handle_key_input(
        &mut self,
        event: &KeyEvent,
        needs_rebuild: &mut bool,
    ) -> Option<Self::Message> {
        use cce_ui::widget::{Key, NamedKey};
        if event.state == ElementState::Pressed {
            // Escape exits — unless the context menu is up (the toolkit-wide
            // Escape-dismiss should win the first press).
            if event.logical_key == Key::Named(NamedKey::Escape)
                && !self.ui_context.is_context_menu_visible()
            {
                return Some(BevelMsg::Exit);
            }
            if event.ctrl {
                if let Key::Character(ref c) = event.logical_key {
                    if c == "q" {
                        return Some(BevelMsg::Exit);
                    }
                }
            }
        }
        let ev = Event::KeyInput(event.clone());
        let mut handled = false;
        for root in self.root_ids() {
            if self.ui_context.propagate_event(&ev, root) {
                handled = true;
                break;
            }
        }
        self.drain_widget_changes();
        if handled || self.needs_rebuild {
            *needs_rebuild = true;
            self.needs_rebuild = true;
        }
        None
    }
}

fn main() {
    cce_ui::engine::run::<BevelPopup>();
}
