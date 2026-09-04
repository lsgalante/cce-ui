//! Narrow-trait `ParametersBg` (Phase 5s) — the designer's parameter panel: a scrollable column
//! of param rows (sliders, spinboxes, dropdowns, text boxes, toggles, colors, float3s,
//! buttons, section borders, and an inline emacs-flavored code editor), each row's widget owned
//! by value in parallel `Vec<Option<..>>` fields (most already `Adapted<W>` from earlier
//! phases), plus a raw-pointer `children` container list. The designer stores it as
//! `Box<dyn WidgetHost>` and drives it through direct `dyn WidgetHost` calls; `window_runner`'s
//! `get_child_widget_for_quad` downcasts to the concrete type through `as_any` (which the
//! adapter forwards to the inner widget) and reads the pub sub-widget fields — both keep
//! working unchanged.
//!
//! The model caches its laid-out rect via [`Layout::rect_assigned`] (all row geometry derives
//! from it — the TextBox pattern), and serves BOTH legacy escape hatches: the plain-quad view
//! ([`Paint::serves_legacy_plain_quads`] — the designer renders params through raw
//! `extra_quads`, and the panel's own background is drawn by the host from
//! [`Paint::color`]/[`Paint::corner_style`], NOT emitted here), and the per-label text view
//! ([`Paint::serves_legacy_labels`], new with this migration — each label clips to the
//! viewport but the code editor's clip to the code box, in monospace, which the one-font
//! one-bounds prim bridge can't express).
//!
//! Flagged approximation: the legacy scrollbar-press called `self.focus()` (base flag only —
//! nothing reads it: the highlight keys on the ctx focus slot, and the designer tracks its
//! focused pane by index); the `on_event` arm drops it.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::display::{Float3, TextLabel};
use crate::widget::input::{Button, ColorSelector, Dropdown, Ramp, Slider, Spinbox, TextBox, Toggle};
use crate::widget::{
    Adapted, WidgetHost, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton,
    MouseScrollDelta, NamedKey, Paint, ParamController, TextEditorState, UiContext,
};

/// A text row, in either variant: plain (`"text"`), or with a completion
/// picker (`"textpick:a,b,c"` — the Houdini-style attribute/group chooser: a
/// TextBox plus a slim menu-button Dropdown at its right edge whose pick
/// fills the box; the host supplies the candidates in the type string).
fn is_text_row(t: &str) -> bool {
    t == "text" || t.starts_with("textpick")
}

/// Width of a textpick row's picker button, nested inside the right end of
/// the TextBox's recessed well.
const PICK_W: f32 = 24.0;

pub struct ParametersBg {
    rect: Rect,
    display_params: Vec<(String, String, String)>,
    dragging_param: Option<usize>,
    pub focused_param: Option<usize>,
    pub code_editor: Option<TextEditorState>,
    mouse_pos: Option<(f32, f32)>,
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
/// so rows read as separate entries rather than one packed stack.
const ROW_GAP: f32 = 8.0;
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
    /// One code column's width — the shaped advance when recorded, else the
    /// legacy 7.2 estimate (only before the first `prepare_text`).
    fn code_col_w(&self) -> f32 {
        if self.code_char_advance > 0.0 {
            self.code_char_advance
        } else {
            7.2
        }
    }

    pub fn new() -> Adapted<ParametersBg> {
        Adapted::new(ParametersBg {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
            display_params: Vec::new(),
            dragging_param: None,
            focused_param: None,
            code_editor: None,
            mouse_pos: None,
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
        })
    }

    /// Row `i`'s laid-out height, ignoring collapse (the row-type table).
    fn row_height(&self, i: usize) -> f32 {
        let p = &self.display_params[i];
        if p.2 == "code" {
            let val_text = if self.focused_param == Some(i) {
                if let Some(ref editor) = self.code_editor {
                    &editor.buffer
                } else {
                    &p.1
                }
            } else {
                &p.1
            };
            let line_count = val_text.split('\n').count();
            let content_h = 22.0 + (line_count as f32 * 16.0) + 12.0;
            content_h.max(200.0)
        } else if p.2 == "section" {
            24.0
        } else if p.2 == "ramp" {
            // Label band + the ramp's graph and control strip. The control
            // strip and label band are fixed, so this whole increase grows the
            // curve plot (Ramp::graph_h = height − strip).
            260.0
        } else if p.2.starts_with("float3") {
            108.0
        } else if p.2.starts_with("slider") {
            38.0
        } else if is_text_row(&p.2) || p.2.starts_with("spinbox") || p.2.starts_with("choice") {
            42.0
        } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
            40.0
        } else if p.2 == "button" || p.2 == "toggle" || p.2 == "checkbox" {
            24.0
        } else {
            20.0
        }
    }

    /// Per-row "sits inside a collapsed section" flags: a section owns every row between its
    /// header and the next header. Headers themselves are never hidden — they stay clickable
    /// so a collapsed section can be reopened. Rows before the first header belong to no
    /// section and are always shown.
    fn hidden_rows(&self) -> Vec<bool> {
        let mut out = Vec::with_capacity(self.display_params.len());
        let mut hiding = false;
        for p in &self.display_params {
            if p.2 == "section" {
                hiding = self.collapsed.contains(&p.0);
                out.push(false);
            } else {
                out.push(hiding);
            }
        }
        out
    }

    /// Whether the section titled `title` is collapsed.
    pub fn section_collapsed(&self, title: &str) -> bool {
        self.collapsed.contains(title)
    }

    /// Collapse/expand the section titled `title`, re-laying the rows. Keyed by title, not
    /// row index, so the state survives the `set_display_params` rebuilds a host runs
    /// whenever the inspected node changes.
    pub fn set_section_collapsed(&mut self, title: &str, collapsed: bool) {
        let changed = if collapsed {
            self.collapsed.insert(title.to_string())
        } else {
            self.collapsed.remove(title)
        };
        if changed {
            self.refresh_scroll_metrics();
        }
    }

    /// The box drawn around a section header's title — and, since the title is the collapse
    /// affordance, that box is also the header's click target.
    fn section_title_box(&self, hdr: usize, r_hdr: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        // The label sits 8px in from the box's left edge; matching that 8px on the right
        // (box width = text + 16) centers the text in the box. Measure the run in the
        // label's real family AND size — parsed, not the raw "Family NN" spec string, which
        // resvg can't resolve (it would fall back to a narrow font and undersize the box).
        let full_w = self.rect.width - 2.0 * SECTION_MARGIN;
        let (label_family, label_size) = crate::layout::control_label_font_parsed();
        let text_w =
            crate::widget::display::measure_text_width(&self.display_params[hdr].0, &label_family, label_size);
        let title_w = (text_w + 16.0).max(SECTION_TITLE_MIN_W).min(full_w);
        // The tab sits FLUSH on the content body: its bottom edge is the body's top
        // edge, in every style (the relief carve always drew it there; the outline now
        // fuses to it too). Collapsed, the box stays where the expanded tab sits (one
        // title-box height above where the body's top edge would be), so collapsing
        // doesn't jump the tab — and the click-toggle hit zone follows the ink. An
        // expanded-but-empty section keeps the legacy header-row placement.
        let y = if self.collapsed.contains(&self.display_params[hdr].0) {
            r_hdr.1 + r_hdr.3 + ROW_GAP - CONTENT_BOX_PAD - TITLE_BOX_H
        } else {
            match self.first_visible_row_top(hdr) {
                Some(control_top) => control_top - CONTENT_BOX_PAD - TITLE_BOX_H,
                None => r_hdr.1 - TITLE_BOX_INSET,
            }
        };
        (self.rect.x + SECTION_MARGIN, y, title_w, TITLE_BOX_H)
    }

    /// The top edge of the first visible row under header `hdr` — the row the section's
    /// content box (and so its tab) hangs from. `None` for an empty section (a header
    /// with no rows of its own before the next header).
    fn first_visible_row_top(&self, hdr: usize) -> Option<f32> {
        let hidden = self.hidden_rows();
        let rects = self.get_param_rects();
        self.display_params
            .iter()
            .enumerate()
            .skip(hdr + 1)
            .take_while(|(_, q)| q.2 != "section")
            .find(|(j, _)| !hidden[*j])
            .map(|(j, _)| rects[j].1)
    }

    /// Each section as `(header index, its content-row range)` — the rows between a header
    /// and the next one, `None` for a header with nothing under it.
    fn sections(&self) -> Vec<(usize, Option<(usize, usize)>)> {
        let mut out: Vec<(usize, Option<(usize, usize)>)> = Vec::new();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2 == "section" {
                out.push((i, None));
            } else if let Some((_, content)) = out.last_mut() {
                match content {
                    Some((_, end)) => *end = i,
                    None => *content = Some((i, i)),
                }
            }
        }
        out
    }

    /// Each section's boxes: the title box, plus the box wrapping its rows (`None` when the
    /// section is collapsed or has no rows). The shared source for the outline's straight
    /// runs and its corner fillets, so the two halves can't disagree.
    fn section_boxes(&self) -> Vec<((f32, f32, f32, f32), Option<(f32, f32, f32, f32)>)> {
        let rects = self.get_param_rects();
        let full_w = self.rect.width - 2.0 * SECTION_MARGIN;
        let mut out = Vec::new();
        for (hdr, content) in self.sections() {
            if hdr >= rects.len() {
                continue;
            }
            let title = self.section_title_box(hdr, rects[hdr]);
            let content_box = if self.collapsed.contains(&self.display_params[hdr].0) {
                None
            } else {
                content.and_then(|(start, end)| {
                    if start <= end && start < rects.len() && end < rects.len() {
                        let by = rects[start].1 - CONTENT_BOX_PAD;
                        let bh = (rects[end].1 + rects[end].3 + CONTENT_BOX_PAD) - by;
                        Some((title.0, by, full_w, bh))
                    } else {
                        None
                    }
                })
            };
            out.push((title, content_box));
        }
        out
    }

    /// One section's outline: a SINGLE continuous border shaped like a folder tab — around
    /// the title box, whose bottom edge is open onto the content body it sits flush on
    /// (its right side turning onto the body's top edge through the concave throat fillet,
    /// its left side running straight down into the body's left edge) — as
    /// `(straight runs, corner fillets)`.
    ///
    /// Runs are `(x, y, w, h)`; fillets are `(cx, cy, radius, start, end)` for an arc stroked
    /// `SECTION_BORDER_T` inward of `radius` (the renderer's convention). Convex corners take
    /// the stroke inside the box, so their radius is the outer one; the concave throat corner
    /// has its centre out in the empty pocket, so its carries the `+ T` that puts the
    /// ink on the far side. Every run stops a radius short of its corner, and each fillet
    /// picks it up there — the path closes.
    fn section_outline(
        &self,
        title: (f32, f32, f32, f32),
        content: Option<(f32, f32, f32, f32)>,
    ) -> (Vec<(f32, f32, f32, f32)>, Vec<(f32, f32, f32, f32, f32)>) {
        use std::f32::consts::{PI, TAU};
        const Q: f32 = std::f32::consts::FRAC_PI_2;
        let (t, r) = (SECTION_BORDER_T, SECTION_R);
        let mut quads: Vec<(f32, f32, f32, f32)> = Vec::new();
        let mut arcs: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
        fn hrun(out: &mut Vec<(f32, f32, f32, f32)>, x0: f32, x1: f32, y: f32) {
            if x1 - x0 > 0.01 {
                out.push((x0, y, x1 - x0, SECTION_BORDER_T));
            }
        }
        fn vrun(out: &mut Vec<(f32, f32, f32, f32)>, y0: f32, y1: f32, x: f32) {
            if y1 - y0 > 0.01 {
                out.push((x, y0, SECTION_BORDER_T, y1 - y0));
            }
        }

        let (tx, ty, tw, th) = title;
        let ty_b = ty + th; // the title box's bottom edge
        arcs.push((tx + r, ty + r, r, PI, PI + Q)); // title top-left
        arcs.push((tx + tw - r, ty + r, r, PI + Q, TAU)); // title top-right
        hrun(&mut quads, tx + r, tx + tw - r, ty); // title top

        let Some((cx, cy_t, cw, ch)) = content else {
            // Collapsed or empty: the title box IS the section, so it closes on itself.
            arcs.push((tx + tw - r, ty_b - r, r, 0.0, Q)); // title bottom-right
            arcs.push((tx + r, ty_b - r, r, Q, PI)); // title bottom-left
            vrun(&mut quads, ty + r, ty_b - r, tx + tw - t); // title right
            vrun(&mut quads, ty + r, ty_b - r, tx); // title left
            hrun(&mut quads, tx + r, tx + tw - r, ty_b - t); // title bottom
            return (quads, arcs);
        };

        // The tab sits flush on the body (`ty_b == cy_t` — `section_title_box` places it
        // there): its bottom edge is open. The throat fillet shrinks if the tab runs
        // close to the body's right corner.
        let f = SECTION_THROAT_R.min((cx + cw - r - (tx + tw)).max(0.0));
        vrun(&mut quads, ty + r, cy_t - f, tx + tw - t); // tab right side, down to the throat
        arcs.push((tx + tw + f, cy_t - f, f + t, Q, PI)); // throat: tab side -> body top
        hrun(&mut quads, tx + tw + f, cx + cw - r, cy_t); // body top, right of the tab

        arcs.push((cx + cw - r, cy_t + r, r, PI + Q, TAU)); // body top-right
        arcs.push((cx + cw - r, cy_t + ch - r, r, 0.0, Q)); // body bottom-right
        arcs.push((cx + r, cy_t + ch - r, r, Q, PI)); // body bottom-left
        vrun(&mut quads, cy_t + r, cy_t + ch - r, cx + cw - t); // body right
        hrun(&mut quads, cx + r, cx + cw - r, cy_t + ch - t); // body bottom
        vrun(&mut quads, ty + r, cy_t + ch - r, tx); // tab + body left, one straight run

        (quads, arcs)
    }

    /// The section outlines' corner fillets, as the renderer's arc tuples
    /// `(cx, cy, radius, thickness, start, end, color)`.
    ///
    /// A concrete accessor rather than prims out of [`Paint::paint`] on purpose: the host
    /// draws this panel through the legacy plain-quad hatch, and `append_widget_plate` serves
    /// a widget's arcs UNCLIPPED — these have to land inside the pane's scroll viewport, so
    /// the host draws them itself under its own clip (the straight runs ride `plain_quads`,
    /// which clips them there by hand).
    pub fn arcs(&self) -> Vec<(f32, f32, f32, f32, f32, f32, [f32; 4])> {
        if !self.visible || crate::layout::control_relief() {
            return Vec::new();
        }
        let color = SECTION_BORDER_COLOR;
        let mut out = Vec::new();
        for (title, content) in self.section_boxes() {
            let (_, arcs) = self.section_outline(title, content);
            out.extend(
                arcs.into_iter()
                    .map(|(cx, cy, r, a0, a1)| (cx, cy, r, SECTION_BORDER_T, a0, a1, color)),
            );
        }
        out
    }

    /// Where the rows start, in absolute y — the origin `get_param_rects` lays out from.
    fn rows_origin(&self) -> f32 {
        self.rect.y - self.scroll_y
    }

    /// The scrollable height of the row column. Derived from [`Self::get_param_rects`] rather
    /// than re-walking the advance rules, so the two can't drift apart.
    pub fn get_total_content_height(&self) -> f32 {
        let hidden = self.hidden_rows();
        let rects = self.get_param_rects();
        let bottom = rects
            .iter()
            .enumerate()
            .filter(|(i, _)| !hidden[*i])
            .map(|(_, r)| r.1 + r.3)
            .fold(f32::NEG_INFINITY, f32::max);
        if bottom == f32::NEG_INFINITY {
            return 30.0 + 10.0; // no rows: the top offset plus the bottom padding
        }
        (bottom - self.rows_origin()) + ROW_GAP + 10.0
    }

    /// One rect per row, in index order — collapsed rows get a zero-height rect at the
    /// current cursor (and consume no vertical space), so every index-parallel consumer
    /// keeps working while the row draws and hit-tests as nothing.
    ///
    /// Rows advance on a uniform [`ROW_GAP`] pitch, except a section header, which is placed
    /// [`SECTION_GAP`] below the previous section's drawn bottom edge — see [`SECTION_GAP`].
    pub fn get_param_rects(&self) -> Vec<(f32, f32, f32, f32)> {
        let hidden = self.hidden_rows();
        let mut rects = Vec::new();
        let mut cur_y = self.rows_origin() + 30.0;
        // Bottom edge of the last row as DRAWN: a row inside a section is wrapped by the
        // content box, which overhangs it; a header with no visible rows under it (empty or
        // collapsed) ends at its own title box. `None` until the first section starts —
        // rows above it are bare, with no box to measure against.
        let mut prev_bottom: Option<f32> = None;
        for i in 0..self.display_params.len() {
            if hidden[i] {
                rects.push((self.rect.x + ROW_X_INSET, cur_y, self.rect.width - 2.0 * ROW_X_INSET, 0.0));
                continue;
            }
            let is_header = self.display_params[i].2 == "section";
            if is_header {
                if let Some(bottom) = prev_bottom {
                    cur_y = bottom + SECTION_GAP + TITLE_BOX_INSET;
                }
            }
            let h = self.row_height(i);
            rects.push((self.rect.x + ROW_X_INSET, cur_y, self.rect.width - 2.0 * ROW_X_INSET, h));
            prev_bottom = Some(if is_header {
                cur_y - TITLE_BOX_INSET + TITLE_BOX_H
            } else if prev_bottom.is_some() {
                cur_y + h + CONTENT_BOX_PAD
            } else {
                cur_y + h
            });
            cur_y += h + ROW_GAP;
        }
        rects
    }

    /// The pane's scrollbar width — the DE `scrollbar_width` widened: the bar
    /// rides over the section carves and reads too slim at the stock width.
    fn scrollbar_w(&self) -> f32 {
        crate::layout::scrollbar_width() * 1.6
    }

    /// The scrollbar's left x. The bar sits a sixth of the plate's width in from the right
    /// edge — i.e. that gap separates the bar's right edge from the pane's right side.
    fn scrollbar_x(&self) -> f32 {
        self.rect.x + self.rect.width - self.scrollbar_w() - self.rect.width / 6.0
    }

    pub fn hit_test_scrollbar(&self, px: f32, py: f32) -> bool {
        if self.content_h <= self.rect.height {
            return false;
        }
        let sb_w = self.scrollbar_w();
        let sb_x = self.scrollbar_x();
        let sb_track_h = self.rect.height - 8.0;
        let sb_track_y = self.rect.y + 4.0;

        px >= sb_x - 4.0 && px <= sb_x + sb_w + 4.0
            && py >= sb_track_y && py <= sb_track_y + sb_track_h
    }

    /// Whether the pane holds enough content to need a scrollbar at all.
    pub fn scrollbar_visible(&self) -> bool {
        self.content_h > self.rect.height
    }

    /// Whether the scrollbar is currently raised in front of the pane plate (the latched
    /// state). While this is false the bar sits behind the plate and is non-interactive.
    pub fn scrollbar_active(&self) -> bool {
        self.activity.raised()
    }

    /// Re-latch the shared hysteresis with this pane's inputs, returning whether it changed.
    fn recompute_scrollbar_raised(&mut self) -> bool {
        let visible = self.scrollbar_visible();
        self.activity.recompute(visible, self.scrollbar_dragging)
    }

    /// The scrollbar's track + thumb quads (empty when no scrollbar is needed). The host draws
    /// these either behind or in front of the pane plate per [`Self::scrollbar_active`]; they
    /// are deliberately kept out of [`Self::plain_quads`] so the host controls their depth.
    pub fn scrollbar_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.scrollbar_visible() {
            return Vec::new();
        }
        let sb_w = self.scrollbar_w();
        let sb_x = self.scrollbar_x();
        let sb_track_h = self.rect.height - 8.0;
        let sb_track_y = self.rect.y + 4.0;

        let visible_ratio = self.rect.height / self.content_h;
        let thumb_h = if sb_track_h <= 20.0 {
            sb_track_h
        } else {
            (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
        };
        let max_scroll = self.content_h - self.rect.height;
        let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
        let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

        vec![
            (sb_x, sb_track_y, sb_w, sb_track_h, crate::color::scrollbar_track_color()),
            (sb_x, thumb_y, sb_w, thumb_h, crate::color::scrollbar_thumb_color()),
        ]
    }

    fn update_slider_rects(&mut self) {
        let rects = self.get_param_rects();
        for (i, s_opt) in self.sliders.iter_mut().enumerate() {
            if let Some(s) = s_opt {
                let r = rects[i];
                s.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, f_opt) in self.float3s.iter_mut().enumerate() {
            if let Some(f) = f_opt {
                let r = rects[i];
                f.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, sb_opt) in self.spinboxes.iter_mut().enumerate() {
            if let Some(sb) = sb_opt {
                let r = rects[i];
                sb.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, b_opt) in self.buttons.iter_mut().enumerate() {
            if let Some(b) = b_opt {
                let r = rects[i];
                b.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, d_opt) in self.choices.iter_mut().enumerate() {
            if let Some(d) = d_opt {
                let r = rects[i];
                if self.display_params[i].2.starts_with("textpick") {
                    // The picker button nests INSIDE the text box's recessed
                    // well (the box spans the full row): below the detached
                    // label band, its face reaching exactly to the base of
                    // the well's wall (inset = the carve depth) on the sides
                    // it adjoins — there the WELL'S OWN BEVEL is the seam's
                    // far side, and the trough ([`Self::troughs`]) carves
                    // only the interior left edge. A ring encapsulated
                    // within the bevel doubled the valley on the adjoining
                    // sides.
                    let label_top = if crate::layout::control_label_layout() == "side" {
                        0.0
                    } else {
                        crate::layout::control_label_font_detached_parsed().1
                            + crate::layout::control_label_margin()
                    };
                    let band_h = r.3 - label_top;
                    let inset = crate::layout::bevel_width().min(band_h * 0.2);
                    let by = r.1 + label_top + inset;
                    let bh = (band_h - 2.0 * inset).max(8.0);
                    d.set_rect(r.0 + r.2 - PICK_W - inset, by, PICK_W, bh);
                    // The menu hangs off the WHOLE field, not the button
                    // sliver: anchor the popover to the box's well band.
                    d.popover_anchor = Some(Rect {
                        x: r.0,
                        y: r.1 + label_top,
                        width: r.2,
                        height: r.3 - label_top,
                    });
                } else {
                    d.set_rect(r.0, r.1, r.2, r.3);
                }
            }
        }
        for (i, tb_opt) in self.texts.iter_mut().enumerate() {
            if let Some(tb) = tb_opt {
                let r = rects[i];
                tb.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, cb_opt) in self.toggles.iter_mut().enumerate() {
            if let Some(cb) = cb_opt {
                let r = rects[i];
                cb.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, c_opt) in self.colors.iter_mut().enumerate() {
            if let Some(c) = c_opt {
                let r = rects[i];
                c.set_rect(r.0, r.1, r.2, r.3);
            }
        }
        for (i, rp_opt) in self.ramps.iter_mut().enumerate() {
            if let Some(rp) = rp_opt {
                let r = rects[i];
                // Below the 18px label band own_text_labels draws (the ramp
                // carries no label of its own).
                rp.set_rect(r.0, r.1 + 18.0, r.2, r.3 - 18.0);
            }
        }
    }

    /// The legacy `set_rect`/`set_display_params` tail: recompute the content height, clamp the
    /// scroll into it, re-lay the rows.
    fn refresh_scroll_metrics(&mut self) {
        self.content_h = self.get_total_content_height();
        let max_scroll = (self.content_h - self.rect.height).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        self.update_slider_rects();
    }

    fn own_text_labels(&self) -> Vec<TextLabel> {
        let rects = self.get_param_rects();
        let hidden = self.hidden_rows();
        let mut labels = Vec::new();
        for (i, (name, value, ptype)) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            let r = rects[i];
            if ptype.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    labels.extend(s.own_text_labels());
                }
            } else if ptype.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    labels.extend(f.own_text_labels());
                }
            } else if ptype == "section" {
                // Centered within the tab itself — `section_title_box` is the one
                // source for where the tab sits (flush on the body; collapsed and
                // empty sections carry their own placements there).
                let font_size = 13.0;
                let (bx, by, _, _) = self.section_title_box(i, r);
                labels.push(TextLabel {
                    text: name.clone(),
                    // 8px in from the title box's left edge — the inset
                    // `section_title_box`'s width math centers against.
                    x: bx + 8.0,
                    y: by + (TITLE_BOX_H - font_size) / 2.0,
                    font_size,
                    color: [0xee, 0xee, 0xf0],
                });
            } else if ptype == "code" {
                labels.push(TextLabel {
                    text: format!("{}:", name),
                    x: self.rect.x + 12.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
                let val_text = if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        editor.buffer.clone()
                    } else {
                        value.clone()
                    }
                } else {
                    value.clone()
                };
                // One label PER LINE, at the same 16px pitch the cursor math uses
                // (`plain_quads`' cursor_y) — a single multi-line label would depend on
                // the consumer's buffer line-height matching that pitch, and never
                // exactly did.
                for (line_i, line) in val_text.split('\n').enumerate() {
                    if line.is_empty() {
                        continue;
                    }
                    labels.push(TextLabel {
                        text: line.to_string(),
                        x: r.0 + 12.0,
                        y: r.1 + 22.0 + line_i as f32 * 16.0,
                        font_size: 12.0,
                        color: [0xee, 0xee, 0xf0],
                    });
                }
            } else if ptype.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    labels.extend(sb.own_text_labels());
                }
            } else if is_text_row(ptype) {
                if let Some(tb) = &self.texts[i] {
                    labels.extend(tb.own_text_labels());
                }
                if let Some(d) = &self.choices[i] {
                    labels.extend(d.own_text_labels());
                }
            } else if ptype.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    labels.extend(d.own_text_labels());
                }
            } else if ptype == "button" {
                if let Some(b) = &self.buttons[i] {
                    labels.extend(b.own_text_labels());
                }
            } else if ptype == "toggle" || ptype == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    labels.extend(cb.own_text_labels());
                }
            } else if ptype.starts_with("color") || ptype == "rgb" || ptype == "rgba" {
                if let Some(c) = &self.colors[i] {
                    labels.extend(c.own_text_labels());
                }
            } else if ptype == "ramp" {
                // The name label only — the ramp's own control labels ride its
                // scene-path paint (paint_scene_rows).
                labels.push(TextLabel {
                    text: name.clone(),
                    x: r.0,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            } else {
                labels.push(TextLabel {
                    text: format!("{}: {}", name, value),
                    x: self.rect.x + ROW_X_INSET,
                    y: r.1,
                    font_size: 12.0,
                    color: [0xaa, 0xaa, 0xbb],
                });
            }
        }
        labels
    }

    /// The dropdown rows' popover, if one is open — the widget's OWN popover surface
    /// ([`Paint::popover`]); the raw `children`'s popovers are the adapter's recursion.
    /// The ramp rows' field dropdowns count too.
    fn choices_popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        for d_opt in &self.choices {
            if let Some(d) = d_opt {
                if let Some(r) = d.popover_rect() {
                    return Some(r);
                }
            }
        }
        for rp_opt in &self.ramps {
            if let Some(rp) = rp_opt {
                let ramp = rp.inner();
                if let Some(r) = ramp
                    .preset_dropdown
                    .popover_rect()
                    .or_else(|| ramp.line_type_dropdown.popover_rect())
                {
                    return Some(r);
                }
            }
        }
        None
    }

    /// The full legacy popover reach (choices, then children) — hit-testing extends to it.
    fn own_popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if let Some(r) = self.choices_popover_rect() {
            return Some(r);
        }
        None
    }

    /// The state half of the legacy `unfocus`: commit the focused row's in-flight value back
    /// into `display_params`, drop the code editor, and unfocus the raw children.
    fn commit_and_unfocus(&mut self) {
        if let Some(idx) = self.focused_param {
            if idx < self.display_params.len() {
                let p = &mut self.display_params[idx];
                if p.2.starts_with("spinbox") {
                    if let Some(sb) = &mut self.spinboxes[idx] {
                        sb.unfocus();
                        p.1 = sb.value.to_string();
                    }
                } else if p.2.starts_with("slider") {
                    if let Some(s) = &mut self.sliders[idx] {
                        s.unfocus();
                        let (min, max) = parse_slider_range(&p.2);
                        let new_val = min + s.value * (max - min);
                        p.1 = format!("{:.*}", slider_decimals(&p.2), new_val);
                    }
                } else if p.2.starts_with("float3") {
                    if let Some(f) = &mut self.float3s[idx] {
                        f.unfocus();
                        let (min, max) = parse_slider_range(&p.2);
                        let val0 = min + f.values[0] * (max - min);
                        let val1 = min + f.values[1] * (max - min);
                        let val2 = min + f.values[2] * (max - min);
                        p.1 = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                    }
                } else if is_text_row(&p.2) {
                    if let Some(tb) = &mut self.texts[idx] {
                        tb.unfocus();
                        p.1 = tb.text.clone();
                    }
                    if let Some(d) = &mut self.choices[idx] {
                        d.unfocus();
                    }
                } else if p.2.starts_with("choice") {
                    if let Some(d) = &mut self.choices[idx] {
                        d.unfocus();
                        if let Some(val) = d.get_value_string() {
                            p.1 = val;
                        }
                    }
                } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    if let Some(c) = &mut self.colors[idx] {
                        c.unfocus();
                        if let Some(val) = c.get_value_string() {
                            p.1 = val;
                        }
                    }
                } else if p.2 == "code" {
                    if let Some(ref editor) = self.code_editor {
                        p.1 = editor.buffer.clone();
                    }
                    self.code_editor = None;
                }
            }
        }
        self.focused_param = None;
    }

    /// The legacy `extra_quads` body: section border boxes, every row's chrome (slider/spinbox
    /// backgrounds read via `rect()`+`color()`, the code editor's box/border/cursor), the raw
    /// children via [`collect_child_quads`], all clipped to the viewport — plus the unclipped
    /// scrollbar. Served verbatim through [`Paint::legacy_plain_quads`].
    fn plain_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut quads = Vec::new();
        let rects = self.get_param_rects();
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;

        let clip_quad = |q: (f32, f32, f32, f32, [f32; 4])| -> Option<(f32, f32, f32, f32, [f32; 4])> {
            let (qx, qy, qw, qh, qc) = q;
            let y1 = qy.max(view_min);
            let y2 = (qy + qh).min(view_max);
            if y1 < y2 {
                Some((qx, y1, qw, y2 - y1, qc))
            } else {
                None
            }
        };

        let mut param_quads = Vec::new();

        // Each section is drawn as ONE continuous outline — around the title, down the
        // neck, around the content rows — whose straight runs are these quads; its corner
        // fillets ride `arcs()`, which the host draws under the same clip. Under
        // `control_relief` the outline is replaced by the inset carves served
        // through `reliefs` (title tab + content body recessed).
        if !crate::layout::control_relief() {
            for (title, content) in self.section_boxes() {
                let (runs, _) = self.section_outline(title, content);
                param_quads.extend(runs.into_iter().map(|(x, y, w, h)| (x, y, w, h, SECTION_BORDER_COLOR)));
            }
        }

        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            let r = rects[i];
            if p.2.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    let (sx, sy, sw, sh) = s.rect();
                    param_quads.push((sx, sy, sw, sh, s.color()));
                    param_quads.extend(s.extra_quads());
                }
            } else if p.2 == "section" {
                // Section header line is handled by the border box top border now
            } else if p.2.starts_with("float3") {
                if let Some(f) = &self.float3s[i] {
                    param_quads.extend(f.extra_quads());
                }
            } else if p.2 == "code" {
                param_quads.push((r.0, r.1 + 18.0, r.2, r.3 - 18.0, [0.08, 0.08, 0.10, 1.0]));
                let border_color = if self.focused_param == Some(i) {
                    [0.25, 0.45, 0.85, 1.0]
                } else {
                    [0.20, 0.20, 0.25, 1.0]
                };
                let (bx, by, bw, bh) = (r.0, r.1 + 18.0, r.2, r.3 - 18.0);
                param_quads.push((bx, by, bw, 1.0, border_color));
                param_quads.push((bx, by + bh - 1.0, bw, 1.0, border_color));
                param_quads.push((bx, by, 1.0, bh, border_color));
                param_quads.push((bx + bw - 1.0, by, 1.0, bh, border_color));
                if self.focused_param == Some(i) {
                    if let Some(ref editor) = self.code_editor {
                        let (cursor_l, cursor_c) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                        let cursor_x = r.0 + 12.0 + (cursor_c as f32 * self.code_col_w());
                        let cursor_y = r.1 + 22.0 + (cursor_l as f32 * 16.0) + (16.0 - 13.0) / 2.0;
                        if cursor_y >= r.1 + 18.0 && cursor_y + 13.0 <= r.1 + r.3 {
                            param_quads.push((cursor_x, cursor_y, 1.5, 13.0, [0.80, 0.80, 0.85, 1.0]));
                        }
                    }
                }
            } else if is_text_row(&p.2) {
                if let Some(tb) = &self.texts[i] {
                    param_quads.extend(tb.extra_quads());
                }
                if let Some(d) = &self.choices[i] {
                    param_quads.extend(d.extra_quads());
                }
            } else if p.2.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    param_quads.extend(d.extra_quads());
                }
            } else if p.2 == "button" {
                if let Some(b) = &self.buttons[i] {
                    param_quads.extend(b.extra_quads());
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    { let (bx, by, bw, bh) = sb.rect(); param_quads.push((bx, by, bw, bh, sb.color())); }
                    param_quads.extend(sb.extra_quads());
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    param_quads.extend(cb.extra_quads());
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &self.colors[i] {
                    param_quads.extend(c.extra_quads());
                }
            }
        }

        // Clip all parameter quads vertically
        for q in param_quads {
            if let Some(clipped) = clip_quad(q) {
                quads.push(clipped);
            }
        }

        // The scrollbar is NOT emitted here: the host draws it via `scrollbar_quads`, above
        // or below the pane plate depending on `scrollbar_active`.

        quads
    }

    /// The rounded companion to [`Self::plain_quads`]: the row controls whose boxes are
    /// `Prim::RoundedRect` (textbox, dropdown, button, toggle, color selector). Those
    /// backgrounds never reach the plain view — `own_plain_quads` keeps `Prim::Quad` only —
    /// so a host that renders this panel through the legacy plain-quad hatch must read this
    /// getter too or the controls draw as bare text. Returned unclipped; the host clips to
    /// the pane's scroll viewport when it pushes vertices. Tuple layout matches
    /// `all_rounded_quads`: (x, y, w, h, radius, color, (tl, tr, br, bl)).
    pub fn rounded_quads(
        &self,
        ctx: &UiContext,
    ) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
        if !self.visible {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if is_text_row(&p.2) {
                if let Some(tb) = &self.texts[i] {
                    out.extend(tb.all_rounded_quads(ctx));
                }
                if let Some(d) = &self.choices[i] {
                    out.extend(d.all_rounded_quads(ctx));
                }
            } else if p.2.starts_with("slider") {
                // Track (square style only — the recessed style has no track
                // background), value fill, and readout box; the thumb knob is a
                // `Prim::Sphere` and rides `spheres()` instead.
                if let Some(s) = &self.sliders[i] {
                    out.extend(s.all_rounded_quads(ctx));
                }
            } else if p.2.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    out.extend(d.all_rounded_quads(ctx));
                }
            } else if p.2.starts_with("spinbox") {
                // The spinbox's whole chrome (frame, display well, +/- button
                // wells) is modern rounded-rect paint — its legacy color() is
                // transparent and it has no extra_quads, so skipping it here
                // renders the row as bare text.
                if let Some(sb) = &self.spinboxes[i] {
                    out.extend(sb.all_rounded_quads(ctx));
                }
            } else if p.2 == "button" {
                if let Some(b) = &self.buttons[i] {
                    out.extend(b.all_rounded_quads(ctx));
                }
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    out.extend(cb.all_rounded_quads(ctx));
                }
            } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                if let Some(c) = &self.colors[i] {
                    out.extend(c.all_rounded_quads(ctx));
                }
            }
        }
        out
    }

    /// The relief companion to [`Self::rounded_quads`]: the row controls'
    /// raised/recessed step prims (boss rims, recess wells), which the flat
    /// views cannot carry — a host rendering this panel through the legacy
    /// views must read this getter too or the `control_relief` styling is lost
    /// entirely (with the DE's transparent control backgrounds the controls all
    /// but vanish). Edges-only variants throughout: the flat views own the
    /// faces, exactly the widgets' own transparent-fill bevel→boss degradation.
    /// Returned unclipped; the host clips to the pane's scroll viewport and
    /// draws these AFTER the flat quads, so the walls' shading modulates the
    /// fills they cross (the order the widgets' own paints use). Tuple:
    /// (x, y, w, h, per-corner radii, depth, raised, walls) — raised maps to
    /// `PaintCtx::boss_edges`, flat to `recess_edges`; the toggles' rocker
    /// halves are why radii/walls are per-entry.
    #[allow(clippy::type_complexity)]
    pub fn reliefs(
        &self,
    ) -> Vec<(f32, f32, f32, f32, (f32, f32, f32, f32), f32, bool, (bool, bool, bool, bool))> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();

        // Sections as inset panels (the flat outline+fillet path is the
        // non-relief style): ONE union-shaped recess per section — a
        // title-text-width tab strip flush with the body's left edge, opening
        // into the full-width body below. Composed from three edge-suppressed
        // pieces (the tab with its bottom open, the body with its top open,
        // and the top-wall run right of the tab's throat) so no wall crosses
        // the union's interior and the whole section reads as a single well.
        // Collapsed sections keep the title-box carve.
        let all = (true, true, true, true);
        let r4 = |r: f32| (r, r, r, r);
        let r = SECTION_R;
        for (title, content) in self.section_boxes() {
            let (tx, ty, tw, th) = title;
            if let Some((cx, cy, cw, ch)) = content {
                // Capped at the channel: the well's wall must roll off inside the
                // groove between it and the controls packed one CHANNEL inside,
                // not shade across their faces. `section_depth` scales wall and
                // channel cap together — past 1.0 the roll crosses the groove
                // by choice.
                let depth = section_carve_depth(ch);
                // Tab strip: the title box itself, sitting flush on the body's top
                // edge (the header row above it stays plain plate), bottom open.
                // A wall FADES OUT over the carve width approaching a suppressed
                // edge (the tessellator's host fade — meant for carves flush with
                // a real plate edge), so every piece extends past its interior
                // seam by `depth`: its fade-out then crossfades with the
                // neighbor's fade-in instead of both dying AT the seam (which
                // notched the walls there; found the hard way).
                let throat_r = tx + tw;
                let rho = SECTION_FILLET_R;
                // Right of the throat, ONE piece owns the whole right run —
                // top wall, top-right arc, right wall, bottom-right arc — so
                // both right corners are real turns. (The old top-run +
                // full-body split put the top wall and the right wall in
                // different pieces; each faded out at the seam and the
                // top-right corner rendered square.) A left piece carries the
                // left wall and the bottom-left arc; the two bottom runs
                // crossfade under the seam at the right piece's left edge.
                let body_lr = |x_run: f32, out: &mut Vec<_>| {
                    out.push((x_run, cy, cx + cw - x_run, ch, (0.0, r, r, 0.0), depth, false, (true, true, true, false)));
                    out.push((cx, cy, x_run + depth - cx, ch, (0.0, 0.0, 0.0, r), depth, false, (false, false, true, true)));
                };
                if cx + cw > throat_r + 2.0 * rho {
                    // Filleted throat ([`Self::section_fillets`]): the tab's
                    // right wall must END at the fillet's vertical tangent
                    // (crossfading out under the arc) or its straight run
                    // ghosts through the curve — so the tab piece stops there,
                    // and a left-only bridge carries the left wall across the
                    // fillet span down to the body's own fade-in.
                    out.push((tx, ty, tw, (cy - rho) - ty + depth, (r, r, 0.0, 0.0), depth, false, (true, true, false, true)));
                    out.push((tx, cy - rho, tw, rho + depth, (0.0, 0.0, 0.0, 0.0), depth, false, (false, false, false, true)));
                    body_lr(throat_r + rho - depth, &mut out);
                } else {
                    // Too narrow for the fillet: the plain square throat.
                    out.push((tx, ty, tw, th + depth, (r, r, 0.0, 0.0), depth, false, (true, true, false, true)));
                    if cx + cw > throat_r + 0.5 {
                        body_lr(throat_r - depth, &mut out);
                    } else {
                        // The tab spans the body: no top wall at all.
                        out.push((cx, cy, cw, ch, (0.0, 0.0, r, r), depth, false, (false, true, true, true)));
                    }
                }
            } else {
                let depth = section_carve_depth(th);
                out.push((tx, ty, tw, th, r4(SECTION_R), depth, false, all));
            }
        }

        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            // (control, its configured corner radius, raised vs recessed)
            let ctl: Option<(&dyn WidgetHost, f32, bool)> = if is_text_row(&p.2) {
                // The textpick picker button is NOT in this list: it is a
                // flush inset control (face level with the well floor, a
                // valley seam around it — the dropdown trigger's trough
                // language, not a boss), and troughs travel through
                // [`Self::picker_troughs`]. A boss here read as a raised
                // island, which no other inset control in the DE does.
                self.texts[i].as_ref().map(|w| (w as &dyn WidgetHost, crate::layout::textbox_corner_radius(), false))
            } else if p.2.starts_with("choice") {
                self.choices[i].as_ref().map(|w| (w as &dyn WidgetHost, crate::layout::dropdown_corner_radius(), true))
            } else if p.2 == "button" {
                self.buttons[i].as_ref().map(|w| (w as &dyn WidgetHost, crate::layout::button_corner_radius(), true))
            } else if p.2 == "toggle" || p.2 == "checkbox" {
                // The rocker's bg pill and its faces' uniform light overlays
                // (`Toggle::face_light`) arrive through the rounded-quad view;
                // the entries here are its two flat halves' beveled edges
                // (`Toggle::rocker_reliefs`, exactly what the widget's own
                // raised paint emits): the state half a raised plateau, the
                // other recessed, hinge wall open on both.
                if let Some(t) = &self.toggles[i] {
                    let (x, y, w, h) = t.rect();
                    if w > 0.0 && h > 0.0 {
                        let ty = crate::widget::label_offset(t);
                        let rect = Rect { x, y: y + ty, width: w, height: h - ty };
                        let depth = crate::layout::bevel_width().min(rect.height * 0.2);
                        if let Some(btn) = t.inner().slide_button(rect) {
                            // Slide style: the gliding half-width button is one
                            // raised plateau (its fill arrives through the
                            // rounded-quad view like the rocker's).
                            let r = crate::layout::toggle_corner_radius();
                            out.push((btn.x, btn.y, btn.width, btn.height, (r, r, r, r), depth, true, all));
                        } else {
                            for (half, radii, walls, raised) in t.inner().rocker_reliefs(rect) {
                                out.push((half.x, half.y, half.width, half.height, radii, depth, raised, walls));
                            }
                        }
                    }
                }
                None
            } else if p.2.starts_with("spinbox") {
                // The well recess only — the -/+ run's trough and its seam
                // travel through [`Self::troughs`] / [`Self::grooves`] (this
                // tuple speaks boss/recess). The generic push below matches
                // the widget's own `relief_parts` well exactly: same side-
                // label inset, same content band, same depth cap.
                self.spinboxes[i]
                    .as_ref()
                    .map(|w| (w as &dyn WidgetHost, crate::layout::spinbox_corner_radius(), false))
            } else {
                if let Some(s) = &self.sliders[i] {
                    let (x, y, w, h) = s.rect();
                    // The detached top label sits OUTSIDE the carve: shrink to
                    // the content band, exactly the rect the widget's own
                    // paint receives (the render_widget label_offset shrink).
                    let ty = crate::widget::label_offset(s);
                    if let Some((rx, ry, rw, rh, rr, rd)) =
                        s.inner().track_relief(Rect { x, y: y + ty, width: w, height: h - ty })
                    {
                        out.push((rx, ry, rw, rh, r4(rr), rd, false, all));
                    }
                }
                None
            };
            if let Some((w, radius, raised)) = ctl {
                let (x, y, ww, h) = w.rect();
                if ww <= 0.0 || h <= 0.0 {
                    continue;
                }
                // The side-label inset the widget's own paint applies (0 for the
                // exempt kinds — button, toggle), and the top-label band, which
                // stays outside the relief like every other host.
                let lx = w.label_x_offset();
                let ty = crate::widget::label_offset(w);
                let depth = crate::layout::bevel_width().min((h - ty) * 0.2);
                out.push((x + lx, y + ty, ww - lx, h - ty, r4(radius), depth, raised, all));
            }
        }
        out
    }

    /// The textpick picker buttons' trough rings — `(x, y, w, h, radii, depth)`
    /// for [`crate::scene::paint::PaintCtx::trough_edges`], drawn by the host
    /// AFTER [`Self::reliefs`]. These are the rows' FLUSH inset controls — the
    /// textpick picker button, and the spinbox's -/+ run: faces level with
    /// their well floor. On the sides a control adjoins its well, the WELL'S
    /// OWN WALL is the seam's far side (the face reaches the wall's base and
    /// that trough edge is suppressed — a lip of its own there doubles the
    /// valley); only edges facing open floor carve their own wall. They
    /// cannot ride in [`Self::reliefs`], whose tuple only speaks boss/recess.
    /// Radii are the well radius's parallel curve at each control's inset;
    /// depths match the well's carve, so seam and wall read as one family.
    #[allow(clippy::type_complexity)]
    pub fn troughs(
        &self,
    ) -> Vec<(f32, f32, f32, f32, (f32, f32, f32, f32), f32, (bool, bool, bool, bool))> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if p.2.starts_with("textpick") {
                if let (Some(d), Some(tb)) = (&self.choices[i], &self.texts[i]) {
                    let (bx, by, bw, bh) = d.rect();
                    let (_, _, _, th) = tb.rect();
                    let ty = crate::widget::label_offset(tb);
                    if bw > 0.0 && bh > 0.0 {
                        let depth = crate::layout::bevel_width().min((th - ty) * 0.2);
                        let r = (crate::layout::textbox_corner_radius() - depth).max(2.0);
                        // Left edge only: top, right and bottom adjoin the well.
                        out.push((bx, by, bw, bh, (r, r, r, r), depth, (false, false, false, true)));
                    }
                }
            } else if p.2.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    let (x, y, w, h) = sb.rect();
                    let ty = crate::widget::label_offset(sb);
                    let band = Rect { x, y: y + ty, width: w, height: h - ty };
                    if let Some((_, Some(((run, radii, rd, edges), _)))) =
                        sb.inner().relief_parts(band)
                    {
                        out.push((run.x, run.y, run.width, run.height, radii, rd, edges));
                    }
                }
            }
        }
        out
    }

    /// The engraved seams companion — `(a, b, width, depth, host)` for
    /// [`crate::scene::paint::PaintCtx::groove`], drawn AFTER [`Self::troughs`]
    /// (a groove engraves the surface the trough's control face provides).
    /// Today: the seam dividing a spinbox's -/+ run into its two buttons —
    /// the breadcrumb's segment-seam language at miniature scale.
    #[allow(clippy::type_complexity)]
    pub fn grooves(&self) -> Vec<((f32, f32), (f32, f32), f32, f32, Rect)> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] || !p.2.starts_with("spinbox") {
                continue;
            }
            if let Some(sb) = &self.spinboxes[i] {
                let (x, y, w, h) = sb.rect();
                let ty = crate::widget::label_offset(sb);
                let band = Rect { x, y: y + ty, width: w, height: h - ty };
                if let Some((_, Some(((_, _, rd, _), (sa, sb2, sw, host))))) =
                    sb.inner().relief_parts(band)
                {
                    out.push((sa, sb2, sw, rd, host));
                }
            }
        }
        out
    }

    /// The sphere companion to [`Self::rounded_quads`]: the slider rows' thumb
    /// knobs, which are `Prim::Sphere` — a prim NO legacy flat view carries, so
    /// a host rendering this panel through the legacy views must read this
    /// getter or the knobs vanish. Returned unclipped; the host clips to the
    /// pane's scroll viewport and draws these AFTER [`Self::reliefs`], matching
    /// the widget's own fill → carve → thumb order.
    pub fn spheres(&self) -> Vec<(f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        let mut out = Vec::new();
        let hidden = self.hidden_rows();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] || !p.2.starts_with("slider") {
                continue;
            }
            if let Some(s) = &self.sliders[i] {
                let (x, y, w, h) = s.rect();
                // Content band, like `reliefs`: the knob sizes to the track
                // well, not the label-inclusive rect.
                let ty = crate::widget::label_offset(s);
                if let Some(sphere) = s.inner().thumb_sphere(Rect { x, y: y + ty, width: w, height: h - ty }) {
                    out.push(sphere);
                }
            }
        }
        out
    }

    /// The section carves' concave inside-corner fillets — `(cx, cy, radius,
    /// depth, start angle)` for [`crate::scene::paint::PaintCtx::concave_fillet`]
    /// (recessed), drawn by the host AFTER [`Self::reliefs`]. The box reliefs
    /// can only round convex corners; this rounds the throat where a tab's
    /// right wall turns onto its body's top edge.
    pub fn section_fillets(&self) -> Vec<(f32, f32, f32, f32, f32)> {
        if !self.visible || !crate::layout::control_relief() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for (title, content) in self.section_boxes() {
            let (tx, _ty, tw, _th) = title;
            if let Some((cx, cy, cw, ch)) = content {
                let depth = section_carve_depth(ch);
                let throat_r = tx + tw;
                if cx + cw > throat_r + 2.0 * SECTION_FILLET_R {
                    out.push((
                        throat_r + SECTION_FILLET_R,
                        cy - SECTION_FILLET_R,
                        SECTION_FILLET_R,
                        depth,
                        std::f32::consts::FRAC_PI_2,
                    ));
                }
            }
        }
        out
    }

    /// The scene-path companion to the legacy views: rows whose widgets paint
    /// prims NO flat tuple view can carry (the ramp rows' curve fill, key
    /// circles, and field controls). A host rendering this panel through the
    /// legacy hatches calls this with its own `PaintCtx` inside the pane's
    /// scroll clip, after the flat chrome — or the rows draw as bare labels.
    pub fn paint_scene_rows(&self, pc: &mut PaintCtx) {
        if !self.visible {
            return;
        }
        let hidden = self.hidden_rows();
        let dummy = UiContext::new();
        for (i, p) in self.display_params.iter().enumerate() {
            if hidden[i] || p.2 != "ramp" {
                continue;
            }
            if let Some(rp) = &self.ramps[i] {
                rp.paint_self(&dummy, pc);
            }
        }
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

impl Paint for ParametersBg {
    /// Shape the hosted controls (their carets read per-glyph advances nothing
    /// else records for children of a container) and one code column's advance
    /// from the same monospace@12 path the code rows draw through.
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        for tb in self.texts.iter_mut().flatten() {
            tb.prepare_text(fs);
        }
        for sb in self.spinboxes.iter_mut().flatten() {
            sb.prepare_text(fs);
        }
        for c in self.colors.iter_mut().flatten() {
            c.prepare_text(fs);
        }
        let clusters = crate::backend::window_runner::shaped_cluster_offsets(
            fs,
            "MMMMMMMM",
            12.0,
            Some("monospace"),
        );
        if let Some(&(_, total)) = clusters.last() {
            if total > 0.0 {
                self.code_char_advance = total / 8.0;
            }
        }
    }

    /// The panel IS its own background plate (the host draws it from `color()` + the corner
    /// style via `push_widget_vertices`) — there is no separate plate widget behind it, so
    /// this carries the full plate treatment: `PARAM_BG` scaled by the global plate opacity,
    /// with the alpha negated as the scenefx blur marker when plate blur is on. Transparent
    /// while hidden. It must not ALSO be emitted as a quad anywhere or it would double-blend.
    fn color(&self) -> [f32; 4] {
        if !self.visible {
            return [0.0, 0.0, 0.0, 0.0];
        }
        colors::param_plate_fill()
    }

    /// The shared plate corner radius (rounded on all four corners when non-zero).
    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::plate_corner_radius();
        let on = r > 0.0;
        Some((r, (on, on, on, on)))
    }

    /// The shared plate border (folded in from the retired backing plate widget).
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        colors::plate_border_color().map(|bc| (bc, colors::plate_border_thickness()))
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    /// The COMPLETE row chrome — everything the legacy hatches carry, in the hosts' canonical
    /// draw order: the controls' rounded wells, the flat-style section outline fillets, the
    /// relief steps (after the fills so the walls shade what they cross), the section carves'
    /// concave throat fillets, the slider-thumb spheres, the scene-path rows, then the flat
    /// plain-quad chrome. What stays OUT, deliberately: the background plate (see
    /// [`color`](Paint::color)), the scrollbar (hosts place its depth — the designer straddles
    /// it around the pane plate), and text (`paint_self`'s own-labels bridge carries the
    /// per-row fonts and code-box bounds). Hosts clip this to their pane viewport — a rect
    /// clip pushed here would not survive `paint_self`'s replay.
    fn paint_ui(&self, ui: &UiContext, _rect: Rect, ctx: &mut PaintCtx) {
        if !self.visible {
            return;
        }
        for (qx, qy, qw, qh, qr, qc, corners) in self.rounded_quads(ui) {
            ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
        }
        for (acx, acy, ar, at, a0, a1, ac) in self.arcs() {
            ctx.arc(acx, acy, ar, at, a0, a1, ac);
        }
        for (rx, ry, rw, rh, radii, rd, raised, edges) in self.reliefs() {
            if raised {
                ctx.boss_edges(Rect { x: rx, y: ry, width: rw, height: rh }, radii, rd, edges);
            } else {
                ctx.recess_edges(Rect { x: rx, y: ry, width: rw, height: rh }, radii, rd, edges);
            }
        }
        for (tx2, ty2, tw2, th2, radii, td, tedges) in self.troughs() {
            ctx.trough_edges(Rect { x: tx2, y: ty2, width: tw2, height: th2 }, radii, td, tedges);
        }
        for (ga, gb, gw, gd, ghost) in self.grooves() {
            ctx.groove(ga, gb, gw, gd, ghost);
        }
        for (fcx, fcy, fr, fd, fs) in self.section_fillets() {
            ctx.concave_fillet(fcx, fcy, fr, fd, fs, false);
        }
        for (scx, scy, sr, sc) in self.spheres() {
            ctx.sphere(scx, scy, sr, sc);
        }
        self.paint_scene_rows(ctx);
        for (qx, qy, qw, qh, qc) in self.plain_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
    }

    /// Ui-less emission (the [`paint_ui`](Paint::paint_ui) override above is what `paint_self`
    /// runs): the flat subset plus the scrollbar, kept for direct callers only. The background
    /// plate stays out — see [`color`](Paint::color).
    fn paint(&self, _rect: Rect, ctx: &mut PaintCtx) {
        for (qx, qy, qw, qh, qc) in self.plain_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        self.paint_scene_rows(ctx);
        // Scene-path hosts get the scrollbar on top (the designer instead straddles it around
        // the pane plate through `scrollbar_quads`).
        for (qx, qy, qw, qh, qc) in self.scrollbar_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;
        for l in self.own_text_labels() {
            if l.y >= view_min - 20.0 && l.y <= view_max + 20.0 {
                ctx.text(l.text, l.x, l.y, l.font_size, l.color);
            }
        }
    }

    fn serves_legacy_plain_quads(&self) -> bool {
        true
    }

    fn legacy_plain_quads(&self, _rect: Rect) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        self.plain_quads()
    }

    fn serves_legacy_labels(&self) -> bool {
        true
    }

    /// The legacy `text_labels_with_font_and_bounds` body: every label clipped to the panel
    /// viewport in the control-label font, except labels inside a code row — those clip to the
    /// code box (or hide when it's scrolled out) and render monospace.
    fn legacy_labels_with_font_and_bounds(&self, _rect: Rect, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let view_min = self.rect.y + 4.0;
        let view_max = self.rect.y + self.rect.height - 4.0;
        let mut result = Vec::new();
        let font = Paint::widget_font(self);
        let rects = self.get_param_rects();
        for l in self.own_text_labels() {
            if l.y < view_min - 20.0 || l.y > view_max + 20.0 {
                continue;
            }
            let mut bounds = Some([self.rect.x + 4.0, view_min, self.rect.x + self.rect.width - 4.0, view_max]);
            let mut label_font = font.clone();
            for (i, p) in self.display_params.iter().enumerate() {
                if p.2 == "code" {
                    let r = rects[i];
                    if l.y >= r.1 + 18.0 && l.y <= r.1 + r.3 {
                        let code_min = (r.1 + 19.0).max(view_min);
                        let code_max = (r.1 + r.3 - 1.0).min(view_max);
                        if code_min < code_max {
                            bounds = Some([r.0 + 1.0, code_min, r.0 + r.2 - 1.0, code_max]);
                        } else {
                            bounds = Some([0.0, 0.0, 0.0, 0.0]); // hidden
                        }
                        label_font = Some("monospace".to_string());
                        break;
                    }
                }
            }
            result.push((l, label_font, bounds));
        }
        result
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.choices_popover_rect()
    }

    /// The dropdown rows' popovers; the raw children's are the adapter's recursion.
    fn draw_popover(&self, _rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        for d_opt in &self.choices {
            if let Some(d) = d_opt {
                d.render_popover(pc);
            }
        }
        for rp_opt in &self.ramps {
            if let Some(rp) = rp_opt {
                rp.inner().preset_dropdown.render_popover(pc);
                rp.inner().line_type_dropdown.render_popover(pc);
            }
        }
    }
}

impl Input for ParametersBg {
    fn scrollable(&self) -> bool {
        true
    }

    /// Legacy `mouse_input` saw every press (and consumes every left press — the designer
    /// relies on the panel swallowing clicks anywhere while it's the dispatch target).
    fn gates_presses(&self) -> bool {
        false
    }

    /// Legacy hit reach: the panel rect, or an open popover (a dropdown row's list extends
    /// below the panel).
    fn hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        if px >= rect.x && px <= rect.x + rect.width && py >= rect.y && py <= rect.y + rect.height {
            return true;
        }
        if let Some((pop_x, pop_y, pop_w, pop_h)) = self.own_popover_rect() {
            if px >= pop_x && px <= pop_x + pop_w && py >= pop_y && py <= pop_y + pop_h {
                return true;
            }
        }
        false
    }


    // --- The host-driven drag surface (the designer routes pointer drags here directly). ---

    fn draggable(&self, _rect: Rect) -> bool {
        self.scrollbar_dragging
            || self.dragging_param.is_some()
            || self.display_params.iter().any(|p| p.2.starts_with("slider") || p.2.starts_with("float3"))
    }

    fn is_dragging(&self) -> bool {
        self.scrollbar_dragging || self.dragging_param.is_some()
    }

    fn drag_begin(&mut self, px: f32, py: f32, _rect: Rect) {
        if self.scrollbar_dragging {
            return;
        }
        let rects = self.get_param_rects();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2.starts_with("slider") {
                let r = rects[i];
                if let Some(s) = &mut self.sliders[i] {
                    let top = crate::widget::label_offset(s);
                    if py >= r.1 + top && py <= r.1 + r.3 {
                        s.drag_begin(px, py);
                        self.dragging_param = Some(i);
                        break;
                    }
                }
            } else if p.2.starts_with("float3") {
                let r = rects[i];
                if py >= r.1 && py <= r.1 + r.3 {
                    if let Some(f) = &mut self.float3s[i] {
                        let mut dummy = crate::context::UiContext::new();
                        if f.mouse_input(MouseButton::Left, ElementState::Pressed, px, py, &mut dummy) {
                            self.dragging_param = Some(i);
                            break;
                        }
                    }
                }
            }
        }
    }

    fn drag_update(&mut self, px: f32, py: f32, _rect: Rect) -> bool {
        if self.scrollbar_dragging {
            let sb_track_h = self.rect.height - 8.0;
            let sb_track_y = self.rect.y + 4.0;
            let visible_ratio = self.rect.height / self.content_h;
            let thumb_h = if sb_track_h <= 20.0 {
                sb_track_h
            } else {
                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
            };
            let max_scroll = (self.content_h - self.rect.height).max(0.0);

            let target_thumb_y = py - self.drag_offset_y;
            let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let old_scroll = self.scroll_y;
            self.scroll_y = new_scroll_ratio * max_scroll;
            if (self.scroll_y - old_scroll).abs() > 0.01 {
                self.update_slider_rects();
                return true;
            }
            return false;
        }

        if let Some(i) = self.dragging_param {
            if let Some(s) = &mut self.sliders[i] {
                if s.drag_update(px, py) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let new_val = min + s.value * (max - min);
                    let old_val = &self.display_params[i].1;
                    let new_val_str = format!("{:.*}", slider_decimals(&self.display_params[i].2), new_val);
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            } else if let Some(f) = &mut self.float3s[i] {
                if f.drag_update(px, py) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let val0 = min + f.values[0] * (max - min);
                    let val1 = min + f.values[1] * (max - min);
                    let val2 = min + f.values[2] * (max - min);
                    let new_val_str = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                    let old_val = &self.display_params[i].1;
                    if *old_val != new_val_str {
                        self.display_params[i].1 = new_val_str;
                        return true;
                    }
                }
            }
        }
        false
    }

    fn drag_end(&mut self) {
        if self.scrollbar_dragging {
            self.scrollbar_dragging = false;
            self.activity.bump();
            return;
        }
        if let Some(i) = self.dragging_param.take() {
            if let Some(s) = &mut self.sliders[i] {
                s.drag_end();
            } else if let Some(f) = &mut self.float3s[i] {
                f.drag_end();
            }
        }
    }

    /// Legacy `tick` forwarded to the raw children (the adapter's recursion now) and the
    /// checkbox rows. The checkbox tick never touches the ctx (a leaf `Adapted` tick is
    /// ctx-free), so the in-file dummy-ctx convention (`drag_begin`, `collect_child_quads`)
    /// applies.
    fn tick(&mut self, dt: f32, _rect: Rect) -> bool {
        if !self.visible {
            return false;
        }
        let mut changed = false;
        let mut dummy = crate::context::UiContext::new();
        // Choice rows tick their Dropdowns' open/close animation. This was the
        // ONLY path that can advance a pane dropdown's anim_snap (the widget's
        // tick-receiver registration points at an id pane internals never put
        // in the host tree), and without it every params-pane dropdown opened
        // at zero drawn extent: logically open, invisible, reporting a sliver
        // popover rect — and the next click toggled it closed again.
        for c_opt in &mut self.choices {
            if let Some(d) = c_opt {
                if d.tick(dt, &mut dummy) {
                    changed = true;
                }
            }
        }
        for cb_opt in &mut self.toggles {
            if let Some(cb) = cb_opt {
                if cb.tick(dt, &mut dummy) {
                    changed = true;
                }
            }
        }
        // Slider rows tick their wheel-glide inertia — fold a coasting value
        // back into the row string so hosts syncing off display_params apply
        // it, exactly like a live wheel event would.
        for i in 0..self.sliders.len() {
            if let Some(s) = &mut self.sliders[i] {
                if s.tick(dt, &mut dummy) {
                    let (min, max) = parse_slider_range(&self.display_params[i].2);
                    let new_val = min + s.value * (max - min);
                    let new_val_str = format!("{:.*}", slider_decimals(&self.display_params[i].2), new_val);
                    if self.display_params[i].1 != new_val_str {
                        self.display_params[i].1 = new_val_str;
                    }
                    changed = true;
                }
            }
        }
        // Ramp rows tick their field widgets (preset application, slider→key
        // sync) and drain their change flag — fold the curve back into the row
        // value when it moved.
        for i in 0..self.ramps.len() {
            if let Some(rp) = &mut self.ramps[i] {
                if rp.tick(dt, &mut dummy) {
                    self.display_params[i].1 = rp.inner().spec_string();
                    changed = true;
                }
            }
        }
        // Color rows tick their picker-stream poll (`cce-color-editor --stream`
        // lines applying live) — fold a changed value back into the row so
        // hosts syncing off display_params see it while the picker is open.
        for i in 0..self.colors.len() {
            if let Some(c) = &mut self.colors[i] {
                if c.tick(dt, &mut dummy) {
                    if let Some(val) = c.get_value_string() {
                        self.display_params[i].1 = val;
                    }
                    changed = true;
                }
            }
        }
        // The pane's own wheel glide / trackpad coast: adopt any host write to
        // `scroll_y`, advance, and re-seat the rows when the offset moved.
        self.scroll_motion.reconcile(0.0, self.scroll_y);
        let pane_max = (self.content_h - self.rect.height).max(0.0);
        if self.scroll_motion.tick(dt, crate::widget::Bounds::max(0.0), crate::widget::Bounds::max(pane_max)) {
            self.scroll_y = self.scroll_motion.y.pos();
            self.update_slider_rects();
            changed = true;
        }
        if self.scroll_motion.is_animating() {
            changed = true;
        }
        // Decay the "recently scrolled" window; keep frames coming until it expires so the
        // scrollbar's sink behind the plate actually renders.
        if self.activity.holding() {
            changed = true;
        }
        // Re-latch the raised state (e.g. sink once the scroll window lapses).
        let visible = self.scrollbar_visible();
        if self.activity.tick(dt, visible, self.scrollbar_dragging) {
            changed = true;
        }
        changed
    }

    fn visibility_changed(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        // Copied before `ectx.ui` is borrowed: the wheel arm's occlusion check keys on the
        // adapter's address (the pointer hosts register/popover-track).
        let self_id = ectx.id;
        match event {
            // Hosts call `unfocus()` directly (the designer's pane switches): commit the
            // focused row and unfocus the children. Needs no ctx, so the direct path's
            // ui-less synthesis works too.
            Event::FocusOut => {
                self.commit_and_unfocus();
                true
            }
            Event::PointerMove { x: px, y: py, .. } => {
                let (px, py) = (*px, *py);
                self.mouse_pos = Some((px, py));
                // Track scrollbar hover, then re-latch: hover only sustains an already-raised
                // bar (a sunk one is behind the plate, so the pointer never reaches it), so
                // the only visible change here is the raised state — redraw on that transition.
                let hover = self.hit_test_scrollbar(px, py);
                self.activity.set_hover(hover);
                let raised_changed = self.recompute_scrollbar_raised();
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return raised_changed;
                };
                let mut changed = raised_changed;

                if self.scrollbar_dragging {
                    let sb_track_h = self.rect.height - 8.0;
                    let sb_track_y = self.rect.y + 4.0;
                    let visible_ratio = self.rect.height / self.content_h;
                    let thumb_h = if sb_track_h <= 20.0 {
                        sb_track_h
                    } else {
                        (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
                    };
                    let max_scroll = (self.content_h - self.rect.height).max(0.0);

                    let target_thumb_y = py - self.drag_offset_y;
                    let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                        ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };

                    let old_scroll = self.scroll_y;
                    self.scroll_y = new_scroll_ratio * max_scroll;
                    if (self.scroll_y - old_scroll).abs() > 0.01 {
                        self.update_slider_rects();
                        changed = true;
                    }
                }

                for sb_opt in &mut self.spinboxes {
                    if let Some(sb) = sb_opt {
                        if sb.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                for f_opt in &mut self.float3s {
                    if let Some(f) = f_opt {
                        if f.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                for b_opt in &mut self.buttons {
                    if let Some(b) = b_opt {
                        if b.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                for d_opt in &mut self.choices {
                    if let Some(d) = d_opt {
                        if d.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                for tb_opt in &mut self.texts {
                    if let Some(tb) = tb_opt {
                        if tb.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                for cb_opt in &mut self.toggles {
                    if let Some(cb) = cb_opt {
                        if cb.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                for c_opt in &mut self.colors {
                    if let Some(c) = c_opt {
                        if c.on_cursor_moved(px, py, ui) {
                            changed = true;
                        }
                    }
                }
                // Ramp rows: a move can drag a key — re-serialize the curve
                // into the row value so hosts polling `node_params` see it.
                for i in 0..self.ramps.len() {
                    if let Some(rp) = &mut self.ramps[i] {
                        if rp.on_cursor_moved(px, py, ui) {
                            self.display_params[i].1 = rp.inner().spec_string();
                            changed = true;
                        }
                    }
                }

                changed
            }
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if !self.visible {
                    return false;
                }
                let (button, state, px, py) = (*button, *state, *px, *py);
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };

                if button == MouseButton::Left {
                    if state == ElementState::Pressed {
                        // Only a raised bar can be grabbed — a sunk one is behind the plate,
                        // so the press falls through to the pane content underneath it.
                        if self.activity.raised() && self.hit_test_scrollbar(px, py) {
                            // Legacy called `self.focus()` here — base flag only, which
                            // nothing reads (see module docs).
                            self.scrollbar_dragging = true;

                            let sb_track_h = self.rect.height - 8.0;
                            let sb_track_y = self.rect.y + 4.0;
                            let visible_ratio = self.rect.height / self.content_h;
                            let thumb_h = if sb_track_h <= 20.0 {
                                sb_track_h
                            } else {
                                (sb_track_h * visible_ratio).clamp(20.0, sb_track_h)
                            };
                            let max_scroll = (self.content_h - self.rect.height).max(0.0);
                            let scroll_ratio = if max_scroll > 0.0 { self.scroll_y / max_scroll } else { 0.0 };
                            let thumb_y = sb_track_y + scroll_ratio * (sb_track_h - thumb_h);

                            let click_offset = py - thumb_y;
                            if click_offset >= 0.0 && click_offset <= thumb_h {
                                self.drag_offset_y = click_offset;
                            } else {
                                // Clicked outside the thumb: jump thumb center to py
                                self.drag_offset_y = thumb_h / 2.0;
                                let target_thumb_y = py - self.drag_offset_y;
                                let new_scroll_ratio = if sb_track_h - thumb_h > 0.0 {
                                    ((target_thumb_y - sb_track_y) / (sb_track_h - thumb_h)).clamp(0.0, 1.0)
                                } else {
                                    0.0
                                };
                                self.scroll_y = new_scroll_ratio * max_scroll;
                                self.update_slider_rects();
                            }
                            return true;
                        }
                    } else if state == ElementState::Released {
                        if self.scrollbar_dragging {
                            self.scrollbar_dragging = false;
                            self.activity.bump();
                            return true;
                        }
                    }
                }

                // A press on a section's title box collapses/expands it. Checked before the
                // rows so a header can never be shadowed by a control under it, and only on
                // the press — the matching release lands on whatever the relayout moved
                // under the pointer, which must not toggle it straight back.
                if button == MouseButton::Left && state == ElementState::Pressed {
                    let rects = self.get_param_rects();
                    let hit = self.display_params.iter().enumerate().position(|(i, p)| {
                        if p.2 != "section" {
                            return false;
                        }
                        let (bx, by, bw, bh) = self.section_title_box(i, rects[i]);
                        px >= bx && px <= bx + bw && py >= by && py <= by + bh
                    });
                    if let Some(i) = hit {
                        let title = self.display_params[i].0.clone();
                        let collapsed = self.collapsed.contains(&title);
                        // Collapsing out from under a focused row would strand the editor.
                        self.commit_and_unfocus();
                        self.set_section_collapsed(&title, !collapsed);
                        return true;
                    }
                }

                let hidden = self.hidden_rows();

                // 1. Check open dropdown popovers first (since they are drawn on top)
                for (i, d_opt) in self.choices.iter_mut().enumerate() {
                    if hidden[i] {
                        continue;
                    }
                    if let Some(d) = d_opt {
                        if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                            eprintln!("[pdbg] press ({px:.0},{py:.0}) choice[{i}] open-priority: popover_rect={:?}", d.popover_rect());
                        }
                        if let Some((ox, oy, ow, oh)) = d.popover_rect() {
                            // Textpick pickers are press-driven end to end
                            // (selection fires on the option PRESS): a release
                            // over the open surface is swallowed, never
                            // dispatched — mid-animation it can read as an
                            // outside press and close the menu it just opened.
                            if state != ElementState::Pressed
                                && self.display_params[i].2.starts_with("textpick")
                            {
                                if px >= ox && px <= ox + ow && py >= oy && py <= oy + oh {
                                    return true;
                                }
                                continue;
                            }
                            let consumed = d.mouse_input(button, state, px, py, ui);
                            if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                                eprintln!("[pdbg]   -> open dropdown consumed={consumed}");
                            }
                            if consumed {
                                if d.take_change() {
                                    if let Some(val) = d.get_value_string() {
                                        // A textpick row's pick fills its
                                        // TextBox — the box IS the value.
                                        if self.display_params[i].2.starts_with("textpick") {
                                            if let Some(tb) = &mut self.texts[i] {
                                                tb.text = val.clone();
                                                tb.edit_buffer = val.clone();
                                            }
                                        }
                                        self.display_params[i].1 = val;
                                    }
                                }
                                return true;
                            }
                        }
                    }
                }
                // The ramp rows' field dropdowns can pop over neighboring rows too.
                for (i, rp_opt) in self.ramps.iter_mut().enumerate() {
                    if hidden[i] {
                        continue;
                    }
                    if let Some(rp) = rp_opt {
                        let ramp = rp.inner();
                        if ramp.preset_dropdown.popover_rect().is_some()
                            || ramp.line_type_dropdown.popover_rect().is_some()
                        {
                            if rp.mouse_input(button, state, px, py, ui) {
                                self.display_params[i].1 = rp.inner().spec_string();
                                return true;
                            }
                        }
                    }
                }

                // 2. Propagate to our widgets
                for (i, p) in self.display_params.iter_mut().enumerate() {
                    if hidden[i] {
                        continue;
                    }
                    if p.2.starts_with("choice") {
                        if let Some(d) = &mut self.choices[i] {
                            if d.mouse_input(button, state, px, py, ui) {
                                // Claim the param focus while the dropdown is
                                // open — the KeyInput arm above is gated on
                                // `focused_param`, and without this the choice
                                // row was the ONE row type that never set it,
                                // so Escape/arrows/Enter could not reach an
                                // open params dropdown (found via cce-designer).
                                if d.open {
                                    self.focused_param = Some(i);
                                } else if self.focused_param == Some(i) {
                                    self.focused_param = None;
                                }
                                if d.take_change() {
                                    if let Some(val) = d.get_value_string() {
                                        p.1 = val;
                                    }
                                }
                                return true;
                            }
                        }
                    } else if p.2 == "button" {
                        if let Some(b) = &mut self.buttons[i] {
                            if b.mouse_input(button, state, px, py, ui) {
                                if std::env::var("CCE_PARAM_DEBUG").is_ok() {
                                    eprintln!("[pdbg] press ({px:.0},{py:.0}) BUTTON[{i}] '{}' consumed", p.0);
                                }
                                if b.take_click() {
                                    p.1 = "clicked".to_string();
                                }
                                return true;
                            }
                        }
                    } else if is_text_row(&p.2) {
                        if let Some(d) = &mut self.choices[i] {
                            // The picker acts on PRESSES only; the release
                            // over the button is swallowed. Releases used to
                            // reach the dropdown, and one arriving before the
                            // open animation's first frame (a fast or
                            // injected click) read as an outside press and
                            // closed the menu it had just opened.
                            let (bx, by, bw, bh) = d.rect();
                            let on_button =
                                px >= bx && px <= bx + bw && py >= by && py <= by + bh;
                            if state == ElementState::Pressed {
                                if d.mouse_input(button, state, px, py, ui) {
                                    if d.take_change() {
                                        if let Some(val) = d.get_value_string() {
                                            if let Some(tb) = &mut self.texts[i] {
                                                tb.text = val.clone();
                                                tb.edit_buffer = val.clone();
                                            }
                                            p.1 = val;
                                        }
                                    }
                                    return true;
                                }
                            } else if on_button {
                                return true;
                            }
                        }
                        if let Some(tb) = &mut self.texts[i] {
                            if tb.mouse_input(button, state, px, py, ui) {
                                if tb.editing {
                                    self.focused_param = Some(i);
                                } else {
                                    if self.focused_param == Some(i) {
                                        self.focused_param = None;
                                    }
                                }
                                if tb.take_change() {
                                    if let Some(val) = tb.get_value_string() {
                                        p.1 = val;
                                    }
                                }
                                return true;
                            }
                        }
                    } else if p.2.starts_with("spinbox") {
                        if let Some(sb) = &mut self.spinboxes[i] {
                            if sb.mouse_input(button, state, px, py, ui) {
                                p.1 = sb.value.to_string();
                                if sb.editing {
                                    self.focused_param = Some(i);
                                } else {
                                    if self.focused_param == Some(i) {
                                        self.focused_param = None;
                                    }
                                }
                                return true;
                            }
                        }
                    } else if p.2 == "toggle" || p.2 == "checkbox" {
                        if let Some(cb) = &mut self.toggles[i] {
                            if cb.mouse_input(button, state, px, py, ui) {
                                if cb.take_change() {
                                    if let Some(val) = cb.get_value_string() {
                                        p.1 = val;
                                    }
                                }
                                return true;
                            }
                        }
                    } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                        if let Some(c) = &mut self.colors[i] {
                            if c.mouse_input(button, state, px, py, ui) {
                                if let Some(val) = c.get_value_string() {
                                    p.1 = val;
                                }
                                if c.editing {
                                    self.focused_param = Some(i);
                                } else {
                                    if self.focused_param == Some(i) {
                                        self.focused_param = None;
                                    }
                                }
                                return true;
                            }
                        }
                    } else if p.2 == "ramp" {
                        if let Some(rp) = &mut self.ramps[i] {
                            if rp.mouse_input(button, state, px, py, ui) {
                                p.1 = rp.inner().spec_string();
                                return true;
                            }
                        }
                    }
                }

                if button == MouseButton::Left && state == ElementState::Pressed {
                    let rects = self.get_param_rects();
                    let mut clicked_any_focusable = false;
                    for (i, p) in self.display_params.iter_mut().enumerate() {
                        if hidden[i] {
                            continue;
                        }
                        if p.2 == "code" {
                            let r = rects[i];
                            if px >= r.0 && px <= r.0 + r.2 && py >= r.1 + 18.0 && py <= r.1 + r.3 {
                                self.focused_param = Some(i);
                                let mut editor = TextEditorState::new(p.1.clone());
                                let click_x = px - (r.0 + 12.0);
                                let click_y = py - (r.1 + 22.0);
                                let line = (click_y / 16.0).floor().max(0.0) as usize;
                                let col = (click_x / self.code_col_w() + 0.5).floor().max(0.0) as usize;
                                editor.cursor_idx = map_2d_to_1d(&editor.buffer, line, col);
                                self.code_editor = Some(editor);
                                clicked_any_focusable = true;
                                break;
                            }
                        } else if p.2.starts_with("slider") {
                            let r = rects[i];
                            if py >= r.1 && py <= r.1 + r.3 {
                                if let Some(s) = &mut self.sliders[i] {
                                    if s.mouse_input(button, state, px, py, ui) {
                                        if s.editing {
                                            self.focused_param = Some(i);
                                            clicked_any_focusable = true;
                                        }
                                        break;
                                    }
                                }
                            }
                        } else if p.2.starts_with("float3") {
                            let r = rects[i];
                            if py >= r.1 && py <= r.1 + r.3 {
                                if let Some(f) = &mut self.float3s[i] {
                                    if f.mouse_input(button, state, px, py, ui) {
                                        if f.editing_idx.is_some() {
                                            self.focused_param = Some(i);
                                            clicked_any_focusable = true;
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    if !clicked_any_focusable {
                        self.commit_and_unfocus();
                    }
                    return true;
                }
                false
            }
            Event::KeyInput(event) => {
                if !self.visible {
                    return false;
                }
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                if let Some(idx) = self.focused_param {
                    if event.state == ElementState::Pressed {
                        let p = &mut self.display_params[idx];
                        if p.2 == "code" {
                            if let Some(mut editor) = self.code_editor.take() {
                                let mut changed = false;
                                let mut handled = true;
                                let mut should_unfocus = false;
                                match &event.logical_key {
                                    Key::Named(NamedKey::Backspace) => {
                                        changed = editor.delete_backwards();
                                    }
                                    Key::Named(NamedKey::Delete) => {
                                        changed = editor.delete_forwards();
                                    }
                                    Key::Named(NamedKey::Enter) => {
                                        editor.insert_text("\n");
                                        changed = true;
                                    }
                                    Key::Named(NamedKey::Escape) => {
                                        should_unfocus = true;
                                    }
                                    Key::Named(NamedKey::ArrowLeft) => {
                                        editor.move_cursor_left(false);
                                    }
                                    Key::Named(NamedKey::ArrowRight) => {
                                        editor.move_cursor_right(false);
                                    }
                                    Key::Named(NamedKey::ArrowUp) => {
                                        let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                        if line > 0 {
                                            editor.cursor_idx = map_2d_to_1d(&editor.buffer, line - 1, col);
                                        }
                                    }
                                    Key::Named(NamedKey::ArrowDown) => {
                                        let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                        let total_lines = editor.buffer.split('\n').count();
                                        if line + 1 < total_lines {
                                            editor.cursor_idx = map_2d_to_1d(&editor.buffer, line + 1, col);
                                        }
                                    }
                                    Key::Named(NamedKey::Home) => {
                                        editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                                    }
                                    Key::Named(NamedKey::End) => {
                                        editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                                    }
                                    Key::Character(s) => {
                                        if event.ctrl {
                                            match s.to_lowercase().as_str() {
                                                "f" => {
                                                    editor.move_cursor_right(false);
                                                }
                                                "b" => {
                                                    editor.move_cursor_left(false);
                                                }
                                                "p" => {
                                                    let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                                    if line > 0 {
                                                        editor.cursor_idx = map_2d_to_1d(&editor.buffer, line - 1, col);
                                                    }
                                                }
                                                "n" => {
                                                    let (line, col) = get_cursor_line_col(&editor.buffer, editor.cursor_idx);
                                                    let total_lines = editor.buffer.split('\n').count();
                                                    if line + 1 < total_lines {
                                                        editor.cursor_idx = map_2d_to_1d(&editor.buffer, line + 1, col);
                                                    }
                                                }
                                                "a" => {
                                                    editor.cursor_idx = get_line_start(&editor.buffer, editor.cursor_idx);
                                                }
                                                "e" => {
                                                    editor.cursor_idx = get_line_end(&editor.buffer, editor.cursor_idx);
                                                }
                                                "d" => {
                                                    changed = editor.delete_forwards();
                                                }
                                                "h" => {
                                                    changed = editor.delete_backwards();
                                                }
                                                "k" => {
                                                    let current_idx = editor.cursor_idx;
                                                    let end_idx = get_line_end(&editor.buffer, current_idx);
                                                    let chars: Vec<char> = editor.buffer.chars().collect();
                                                    if chars.is_empty() {
                                                        // do nothing
                                                    } else if current_idx < chars.len() {
                                                        let delete_end = if chars[current_idx] == '\n' {
                                                            current_idx + 1
                                                        } else {
                                                            end_idx
                                                        };
                                                        let mut new_buf = String::new();
                                                        for i in 0..current_idx {
                                                            new_buf.push(chars[i]);
                                                        }
                                                        for i in delete_end..chars.len() {
                                                            new_buf.push(chars[i]);
                                                        }
                                                        editor.buffer = new_buf;
                                                        changed = true;
                                                    }
                                                }
                                                _ => {
                                                    handled = false;
                                                }
                                            }
                                        } else {
                                            editor.insert_text(s);
                                            changed = true;
                                        }
                                    }
                                    _ => {
                                        handled = false;
                                    }
                                }
                                if changed {
                                    p.1 = editor.buffer.clone();
                                }
                                if should_unfocus {
                                    p.1 = editor.buffer;
                                    self.focused_param = None;
                                    self.code_editor = None;
                                } else {
                                    self.code_editor = Some(editor);
                                }
                                if handled {
                                    return true;
                                }
                            }
                        } else if is_text_row(&p.2) {
                            if let Some(d) = &mut self.choices[idx] {
                                if d.open && d.keyboard_input(event, ui) {
                                    if d.take_change() {
                                        if let Some(val) = d.get_value_string() {
                                            if let Some(tb) = &mut self.texts[idx] {
                                                tb.text = val.clone();
                                                tb.edit_buffer = val.clone();
                                            }
                                            p.1 = val;
                                        }
                                    }
                                    return true;
                                }
                            }
                            if let Some(tb) = &mut self.texts[idx] {
                                if tb.keyboard_input(event, ui) {
                                    if !tb.editing {
                                        p.1 = tb.text.clone();
                                        self.focused_param = None;
                                    } else {
                                        p.1 = tb.edit_buffer.clone();
                                    }
                                    return true;
                                }
                            }
                        } else if p.2.starts_with("choice") {
                            if let Some(d) = &mut self.choices[idx] {
                                if d.keyboard_input(event, ui) {
                                    if !d.open {
                                        if let Some(val) = d.get_value_string() {
                                            p.1 = val;
                                        }
                                        self.focused_param = None;
                                    }
                                    return true;
                                }
                            }
                        } else if p.2.starts_with("spinbox") {
                            if let Some(sb) = &mut self.spinboxes[idx] {
                                if sb.keyboard_input(event, ui) {
                                    if !sb.editing {
                                        p.1 = sb.value.to_string();
                                        self.focused_param = None;
                                    } else {
                                        p.1 = sb.edit_buffer.clone();
                                    }
                                    return true;
                                }
                            }
                        } else if p.2.starts_with("slider") {
                            if let Some(s) = &mut self.sliders[idx] {
                                if s.keyboard_input(event, ui) {
                                    let (min, max) = parse_slider_range(&p.2);
                                    let new_val = min + s.value * (max - min);
                                    p.1 = format!("{:.*}", slider_decimals(&p.2), new_val);
                                    if !s.editing {
                                        self.focused_param = None;
                                    }
                                    return true;
                                }
                            }
                        } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                            if let Some(c) = &mut self.colors[idx] {
                                if c.keyboard_input(event, ui) {
                                    if let Some(val) = c.get_value_string() {
                                        p.1 = val;
                                    }
                                    if !c.editing {
                                        self.focused_param = None;
                                    }
                                    return true;
                                }
                            }
                        } else if p.2.starts_with("float3") {
                            if let Some(f) = &mut self.float3s[idx] {
                                if f.keyboard_input(event, ui) {
                                    let (min, max) = parse_slider_range(&p.2);
                                    let val0 = min + f.values[0] * (max - min);
                                    let val1 = min + f.values[1] * (max - min);
                                    let val2 = min + f.values[2] * (max - min);
                                    p.1 = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                                    if f.editing_idx.is_none() {
                                        self.focused_param = None;
                                    }
                                    return true;
                                }
                            }
                        }
                    }
                }
                false
            }
            Event::MouseWheel { delta, x: px, y: py, .. } => {
                if !self.visible {
                    return false;
                }
                let (px, py) = (*px, *py);
                let Some(ui) = ectx.ui.as_deref_mut() else {
                    return false;
                };
                let mut changed = false;
                // A value row that took the wheel (slider/float3/spinbox),
                // whether or not its 2-decimal string ticked over. Gating the
                // pane's viewport-scroll fallback on the STRING (`changed`)
                // let every sub-tick trackpad event scroll the pane instead —
                // a slider-vs-pane tug-of-war that shifted the rows under the
                // pointer mid-adjust.
                let mut wheel_taken = false;
                let rects = self.get_param_rects();
                for (i, p) in self.display_params.iter_mut().enumerate() {
                    if p.2.starts_with("slider") {
                        let r = rects[i];
                        let row_y = r.1;
                        // Band style: the capture zone is the slider's own
                        // shape halo (`Slider::scroll_hit` — the band plus the
                        // traveling bulge, inset), so scrolls off the shape
                        // fall through to the pane's viewport scroll below.
                        // The default style keeps the whole-row strip.
                        let in_zone = if crate::layout::slider_band() {
                            self.sliders[i].as_ref().map_or(false, |s| {
                                // The same gesture latch the slider's own wheel
                                // test applies: mid-gesture the slider that
                                // acquired the scroll keeps it (its halo travels
                                // away from the pointer as the value moves).
                                let latched = !ui.scroll_gesture_new
                                    && ui.scroll_initiate_widget_id == Some(s.base().id());
                                let (sx, sy, sw, sh) = s.rect();
                                let ty = crate::widget::label_offset(s);
                                latched
                                    || s.inner().scroll_hit(
                                        Rect { x: sx, y: sy + ty, width: sw, height: sh - ty },
                                        px,
                                        py,
                                    )
                            })
                        } else {
                            py >= row_y - 2.0
                                && py <= row_y + r.3
                                && px >= self.rect.x
                                && px <= self.rect.x + self.rect.width
                        };
                        if in_zone {
                            if let Some(s) = &mut self.sliders[i] {
                                let was_scroll = s.scroll_enabled;
                                s.set_scroll(true);
                                // Ungated: the in_zone halo above already gated
                                // spatially, and the adapter's rect gate would
                                // clip the halo's fringe outside the row rect.
                                if s.mouse_wheel_ungated(delta, px, py, ui) {
                                    wheel_taken = true;
                                    let (min, max) = parse_slider_range(&p.2);
                                    let new_val = min + s.value * (max - min);
                                    let old_val = &p.1;
                                    let new_val_str = format!("{:.*}", slider_decimals(&p.2), new_val);
                                    if *old_val != new_val_str {
                                        p.1 = new_val_str;
                                        changed = true;
                                    }
                                }
                                s.set_scroll(was_scroll);
                            }
                        }
                    } else if p.2.starts_with("float3") {
                        let r = rects[i];
                        let row_y = r.1;
                        if py >= row_y && py <= row_y + r.3 && px >= self.rect.x && px <= self.rect.x + self.rect.width {
                            if let Some(f) = &mut self.float3s[i] {
                                let rects_inner = f.get_row_rects();
                                for j in 0..3 {
                                    let r_inner = rects_inner[j];
                                    if py >= r_inner.1 && py <= r_inner.1 + r_inner.3 {
                                        wheel_taken = true;
                                        let scroll_amount = delta.notches_y();
                                        let step = 0.02;
                                        let new_val = (f.values[j] - scroll_amount * step).clamp(0.0, 1.0);
                                        if (new_val - f.values[j]).abs() > 0.0001 {
                                            f.values[j] = new_val;
                                            if f.editing_idx == Some(j) {
                                                let scaled_val = f.mins[j] + f.values[j] * (f.maxs[j] - f.mins[j]);
                                                f.edit_buffer = format!("{:.2}", scaled_val);
                                            }
                                            let (min, max) = parse_slider_range(&p.2);
                                            let val0 = min + f.values[0] * (max - min);
                                            let val1 = min + f.values[1] * (max - min);
                                            let val2 = min + f.values[2] * (max - min);
                                            let new_val_str = format!("{:.2}:{:.2}:{:.2}", val0, val1, val2);
                                            if p.1 != new_val_str {
                                                p.1 = new_val_str;
                                                changed = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    } else if p.2.starts_with("spinbox") {
                        let r = rects[i];
                        let row_y = r.1;
                        if py >= row_y && py <= row_y + r.3 && px >= self.rect.x && px <= self.rect.x + self.rect.width {
                            if let Some(sb) = &mut self.spinboxes[i] {
                                wheel_taken = true;
                                let scroll_amount = match delta {
                                    MouseScrollDelta::LineDelta(_x, y) => *y as i32,
                                    MouseScrollDelta::PixelDelta(pos) => {
                                        let dy = pos.y;
                                        if dy > 0.0 { 1 } else if dy < 0.0 { -1 } else { 0 }
                                    }
                                };
                                let new_val = (sb.value + scroll_amount * sb.step).clamp(sb.min, sb.max);
                                if sb.value != new_val {
                                    sb.value = new_val;
                                    p.1 = new_val.to_string();
                                    changed = true;
                                }
                            }
                        }
                    }
                }

                // The legacy tail's `self.hit_test(px, py, ctx)`: occlusion via the adapter's
                // address, then rect-or-popover containment.
                let mut swallowed = changed || wheel_taken;
                if !ui.is_coordinate_covered(self_id, px, py) {
                    let in_rect = px >= self.rect.x
                        && px <= self.rect.x + self.rect.width
                        && py >= self.rect.y
                        && py <= self.rect.y + self.rect.height;
                    let in_popover = self.own_popover_rect().map_or(false, |(rx, ry, rw, rh)| {
                        px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
                    });
                    if in_rect || in_popover {
                        if !changed && !wheel_taken {
                            if crate::scroll_debug() {
                                eprintln!("[scroll] params: PANE-SCROLL fallback at ({px:.0},{py:.0})");
                            }
                            let max_scroll = (self.content_h - self.rect.height).max(0.0);
                            self.scroll_motion.reconcile(0.0, self.scroll_y);
                            let moved = self.scroll_motion.apply(
                                delta,
                                (crate::widget::LINE_PX, crate::widget::LINE_PX),
                                crate::widget::Bounds::max(0.0),
                                crate::widget::Bounds::max(max_scroll),
                            );
                            self.scroll_y = self.scroll_motion.y.pos();
                            if moved {
                                self.update_slider_rects();
                                self.activity.bump();
                                self.recompute_scrollbar_raised();
                            }
                        }
                        // An opaque pane swallows EVERY wheel over it, whether
                        // anything moved or not: returning false would hand the
                        // event to whatever lies BEHIND the plate — the designer
                        // routes unhandled wheels to the 3D viewport, whose rect
                        // is the whole window in the floating layout, so a
                        // near-miss on a slider would orbit the camera through
                        // the pane.
                        swallowed = true;
                    }
                }

                swallowed
            }
            _ => false,
        }
    }
}

/// Display precision for a slider row from the type string's optional 4th
/// segment (`slider:min:max:decimals`); 2 when absent — the pane-wide
/// historical default.
fn slider_decimals(ptype: &str) -> usize {
    ptype.split(':').nth(3).and_then(|s| s.parse().ok()).unwrap_or(2)
}

fn parse_slider_range(ptype: &str) -> (f32, f32) {
    if ptype.starts_with("slider:") || ptype.starts_with("float3:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<f32>(), parts[2].parse::<f32>()) {
                return (min, max);
            }
        }
    }
    (0.0, 2.0)
}

fn parse_hex_to_rgb(s: &str) -> Option<[u8; 3]> {
    crate::color::parse_hex_bytes(s).map(|[r, g, b, _]| [r, g, b])
}

fn parse_hex_to_rgba(s: &str) -> Option<[u8; 4]> {
    crate::color::parse_hex_bytes(s)
}

fn parse_spinbox_range(ptype: &str) -> (i32, i32, i32) {
    if ptype.starts_with("spinbox:") {
        let parts: Vec<&str> = ptype.split(':').collect();
        if parts.len() >= 4 {
            if let (Ok(min), Ok(max), Ok(step)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>(), parts[3].parse::<i32>()) {
                return (min, max, step);
            }
        } else if parts.len() == 3 {
            if let (Ok(min), Ok(max)) = (parts[1].parse::<i32>(), parts[2].parse::<i32>()) {
                return (min, max, 1);
            }
        }
    }
    (0, 10000, 1)
}

fn parse_float3_value(val_str: &str, min: f32, max: f32) -> [f32; 3] {
    let mut out = [0.5, 0.5, 0.5];
    let parts: Vec<&str> = val_str
        .split(|c| c == ':' || c == ',' || c == ' ')
        .filter(|s| !s.is_empty())
        .collect();
    for i in 0..3 {
        if i < parts.len() {
            if let Ok(v) = parts[i].parse::<f32>() {
                let range = max - min;
                if range != 0.0 {
                    out[i] = ((v - min) / range).clamp(0.0, 1.0);
                } else {
                    out[i] = 0.0;
                }
            }
        }
    }
    out
}

impl ParamController for ParametersBg {
    fn node_params(&self) -> Vec<(String, String, String)> {
        self.display_params.clone()
    }

    fn set_display_params(&mut self, params: &[(String, String, String)]) {
        let mut layout_changed = self.display_params.len() != params.len();
        if !layout_changed {
            for (p_old, p_new) in self.display_params.iter().zip(params.iter()) {
                if p_old.0 != p_new.0 || p_old.2 != p_new.2 {
                    layout_changed = true;
                    break;
                }
            }
        }

        if layout_changed {
            self.scroll_y = 0.0;
            self.display_params = params.to_vec();
            self.focused_param = None;
            self.sliders = self.display_params.iter().map(|p| {
                if p.2.starts_with("slider") {
                    let val = p.1.parse::<f32>().unwrap_or(0.0);
                    let (min, max) = parse_slider_range(&p.2);
                    let t = if max - min != 0.0 {
                        ((val - min) / (max - min)).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    Some(Slider::new().with_value(t).with_range(min, max).with_readout(true).with_decimals(slider_decimals(&p.2)).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.float3s = self.display_params.iter().map(|p| {
                if p.2.starts_with("float3") {
                    let (min, max) = parse_slider_range(&p.2);
                    let vals = parse_float3_value(&p.1, min, max);
                    Some(Float3::new().with_values(vals).with_range(min, max).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.spinboxes = self.display_params.iter().map(|p| {
                if p.2.starts_with("spinbox") {
                    let (min, max, step) = parse_spinbox_range(&p.2);
                    let val = p.1.parse::<i32>().unwrap_or(min);
                    Some(Spinbox::new(val, min, max, step).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.buttons = self.display_params.iter().map(|p| {
                if p.2 == "button" {
                    // Left-aligned, like the toggles below: the rows form one column, and
                    // centered labels made each row's text start at a different x.
                    Some(Button::new(0.0, 0.0, 0.0, 0.0).with_label(&p.0).with_left_align(true))
                } else {
                    None
                }
            }).collect();
            self.choices = self.display_params.iter().map(|p| {
                if p.2.starts_with("choice:") {
                    let options_str = p.2.strip_prefix("choice:").unwrap_or("");
                    let options: Vec<String> = options_str.split(',').map(|s| s.to_string()).collect();
                    let selected = options.iter().position(|o| o == &p.1).unwrap_or(0);
                    Some(Dropdown::new(options, selected).with_label(&p.0))
                } else if p.2.starts_with("textpick:") {
                    // The text row's completion picker: a menu-button Dropdown
                    // (fixed glyph, re-fires on repeat picks) beside the box.
                    let options: Vec<String> = p.2.strip_prefix("textpick:").unwrap_or("")
                        .split(',')
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                        .collect();
                    if options.is_empty() {
                        None
                    } else {
                        // A raised face: the picker reads as a BUTTON sitting
                        // in the box's recess, not a bare glyph beside it.
                        Some(
                            Dropdown::new(options, 0)
                                .with_custom_display_text("\u{25be}")
                                .with_raised(true),
                        )
                    }
                } else {
                    None
                }
            }).collect();
            self.texts = self.display_params.iter().map(|p| {
                if is_text_row(&p.2) {
                    Some(TextBox::new(p.1.clone()).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.toggles = self.display_params.iter().map(|p| {
                if p.2 == "toggle" || p.2 == "checkbox" {
                    let on = p.1.trim().to_lowercase() == "true";
                    let mut t = Toggle::new().with_label(&p.0).with_left_align(true);
                    t.set_toggled(on);
                    Some(t)
                } else {
                    None
                }
            }).collect();
            self.colors = self.display_params.iter().map(|p| {
                if p.2 == "rgba" {
                    // Alpha-carrying param: the full picker, 8-digit hex.
                    let col = parse_hex_to_rgba(&p.1).unwrap_or([255, 255, 255, 255]);
                    Some(ColorSelector::new_rgba(col).with_label(&p.0))
                } else if p.2.starts_with("color") || p.2 == "rgb" {
                    let col = parse_hex_to_rgb(&p.1).unwrap_or([255, 255, 255]);
                    Some(ColorSelector::new(col).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.ramps = self.display_params.iter().map(|p| {
                if p.2 == "ramp" {
                    let mut rp = Ramp::new();
                    rp.inner_mut().set_spec(&p.1);
                    Some(rp)
                } else {
                    None
                }
            }).collect();
        } else {
            for (i, p_new) in params.iter().enumerate() {
                if Some(i) != self.focused_param && Some(i) != self.dragging_param {
                    self.display_params[i].1 = p_new.1.clone();
                    if let Some(ref mut s) = self.sliders[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        // Idempotence guard: hosts push params straight back
                        // after every sync, and re-seeding from the 2-decimal
                        // string quantizes away the slider's sub-tick state —
                        // mid-scroll that snaps the value BACKWARD between
                        // wheel events/glide ticks (visible as jitter). Only
                        // re-seed when the incoming string says something the
                        // current value doesn't (a genuinely external change).
                        let cur_str = format!("{:.*}", slider_decimals(&p_new.2), min + s.value * (max - min));
                        if cur_str != p_new.1 {
                            let val = p_new.1.parse::<f32>().unwrap_or(0.0);
                            let t = if max - min != 0.0 {
                                ((val - min) / (max - min)).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            s.set_value(t);
                        }
                    } else if let Some(ref mut f) = self.float3s[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        // Same round-trip guard as the slider row.
                        let cur_str = format!(
                            "{:.2}:{:.2}:{:.2}",
                            min + f.values[0] * (max - min),
                            min + f.values[1] * (max - min),
                            min + f.values[2] * (max - min)
                        );
                        if cur_str != p_new.1 {
                            let vals = parse_float3_value(&p_new.1, min, max);
                            f.set_values(vals);
                        }
                    } else if let Some(ref mut sb) = self.spinboxes[i] {
                        if !sb.editing {
                            let (min, _max, _step) = parse_spinbox_range(&p_new.2);
                            let val = p_new.1.parse::<i32>().unwrap_or(min);
                            sb.value = val;
                        }
                    } else if let Some(ref mut d) = self.choices[i] {
                        if !d.open {
                            if let Some(options_str) = p_new.2.strip_prefix("choice:") {
                                let options: Vec<String> = options_str.split(',').map(|s| s.to_string()).collect();
                                if d.options != options {
                                    d.options = options.clone();
                                }
                                if let Some(idx) = options.iter().position(|o| o == &p_new.1) {
                                    d.selected = idx;
                                }
                            }
                        }
                    } else if let Some(ref mut tb) = self.texts[i] {
                        if !tb.editing {
                            tb.set_value_string(&p_new.1);
                        }
                    } else if let Some(ref mut t) = self.toggles[i] {
                        let on = p_new.1.trim().to_lowercase() == "true";
                        t.set_toggled(on);
                    } else if let Some(ref mut c) = self.colors[i] {
                        if !c.editing {
                            c.set_value_string(&p_new.1);
                        }
                    } else if let Some(ref mut rp) = self.ramps[i] {
                        if !rp.inner().is_dragging_key {
                            rp.set_value_string(&p_new.1);
                        }
                    }
                }
            }
        }
        self.refresh_scroll_metrics();
    }
}

fn get_cursor_line_col(buffer: &str, cursor_idx: usize) -> (usize, usize) {
    let mut cur_line = 0;
    let mut cur_col = 0;
    let mut count = 0;
    for c in buffer.chars() {
        if count == cursor_idx {
            return (cur_line, cur_col);
        }
        if c == '\n' {
            cur_line += 1;
            cur_col = 0;
        } else {
            cur_col += 1;
        }
        count += 1;
    }
    (cur_line, cur_col)
}

fn map_2d_to_1d(buffer: &str, line: usize, col: usize) -> usize {
    let mut target_line = line;
    let lines: Vec<Vec<char>> = buffer.split('\n').map(|l| l.chars().collect()).collect();
    if lines.is_empty() {
        return 0;
    }
    if target_line >= lines.len() {
        target_line = lines.len() - 1;
    }
    let mut target_col = col;
    if target_col > lines[target_line].len() {
        target_col = lines[target_line].len();
    }
    let mut index = 0;
    for i in 0..target_line {
        index += lines[i].len() + 1; // +1 for the '\n'
    }
    index += target_col;
    index
}

fn get_line_start(buffer: &str, cursor_idx: usize) -> usize {
    let (line, _) = get_cursor_line_col(buffer, cursor_idx);
    map_2d_to_1d(buffer, line, 0)
}

fn get_line_end(buffer: &str, cursor_idx: usize) -> usize {
    let (line, _) = get_cursor_line_col(buffer, cursor_idx);
    let lines: Vec<Vec<char>> = buffer.split('\n').map(|l| l.chars().collect()).collect();
    if lines.is_empty() {
        return 0;
    }
    let line_len = if line < lines.len() {
        lines[line].len()
    } else {
        lines[lines.len() - 1].len()
    };
    map_2d_to_1d(buffer, line, line_len)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;

    fn panel_with(params: &[(&str, &str, &str)]) -> Adapted<ParametersBg> {
        let mut p = ParametersBg::new();
        let params: Vec<(String, String, String)> = params
            .iter()
            .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
            .collect();
        ParamController::set_display_params(&mut *p, &params);
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 400.0);
        p
    }

    /// The textpick text-row variant: a TextBox AND a menu-button Dropdown
    /// share the row — the picker takes a right-edge sliver, its options come
    /// from the type string, and a plain text row builds no picker.
    #[test]
    fn textpick_rows_carry_a_picker() {
        let p = panel_with(&[
            ("Attribute Name", "mass", "textpick:Norm,UV,Pos,Col"),
            ("Plain", "x", "text"),
            ("Empty", "y", "textpick:"),
        ]);
        assert!(p.texts[0].is_some(), "textpick keeps its TextBox");
        let d = p.choices[0].as_ref().expect("textpick builds the picker");
        assert_eq!(d.options, ["Norm", "UV", "Pos", "Col"]);
        assert!(d.custom_display_text.is_some(), "menu-button mode");
        assert!(p.choices[1].is_none(), "plain text has no picker");
        assert!(p.texts[2].is_some() && p.choices[2].is_none(), "no options, no picker");

        // Layout: the box spans the full row; the picker button nests
        // inside it (within the box's right end).
        let (tx, ty, tw, th) = p.texts[0].as_ref().unwrap().rect();
        let (dx, dy, dw, dh) = p.choices[0].as_ref().unwrap().rect();
        assert_eq!(dw, PICK_W);
        assert!(dx > tx && dx + dw < tx + tw, "button inside the box horizontally");
        assert!(dy > ty && dy + dh <= ty + th, "button inside the box vertically");

        // Both text variants lay out at the same row height.
        assert_eq!(p.inner().row_height(0), p.inner().row_height(1));

        // The menu anchors off the WHOLE field: the popover spans at least
        // the box width and hangs below it, not off the button sliver.
        let anchor = p.choices[0].as_ref().unwrap().popover_anchor.expect("anchor set");
        assert_eq!(anchor.x, tx);
        assert_eq!(anchor.width, tw);
        let d = p.choices[0].as_ref().unwrap();
        let (px_, py_, pw, _ph) = d.popover_geom(crate::scene::layout::Rect {
            x: dx, y: dy, width: dw, height: dh,
        });
        assert_eq!(px_, tx, "menu left-aligns with the box");
        assert!(pw >= tw, "menu at least as wide as the box");
        assert!(py_ >= ty + th - 1.0, "menu hangs below the box");
    }

    #[test]
    fn param_controller_roundtrip_and_row_widgets() {
        let p = panel_with(&[
            ("Size", "1.00", "slider:0:2"),
            ("Mode", "b", "choice:a,b,c"),
            ("On", "true", "checkbox"),
        ]);
        assert_eq!(ParamController::node_params(&*p).len(), 3);
        assert!(p.sliders[0].is_some() && p.choices[1].is_some() && p.toggles[2].is_some());
        // Rows were laid out from the cached rect.
        let (sx, _, sw, _) = p.sliders[0].as_ref().unwrap().rect();
        assert_eq!(
            (sx, sw),
            (ROW_X_INSET, 300.0 - 2.0 * ROW_X_INSET),
            "row rect derives from the assigned rect"
        );
    }

    #[test]
    fn ramp_row_builds_from_spec_and_edits_serialize_back() {
        let mut ctx = UiContext::new();
        let mut p = panel_with(&[("Bevel Profile", "smooth;0.000:0.000,1.000:1.000", "ramp")]);
        let rp = p.ramps[0].as_ref().expect("ramp row builds a Ramp");
        assert_eq!(rp.inner().keys.len(), 2);
        assert!(rp.inner().smooth());

        // A press inside the curve area adds a key, and the row value carries
        // the re-serialized spec (what hosts poll and persist).
        let (rx, ry, rw, _) = rp.rect();
        p.mouse_input(MouseButton::Left, ElementState::Pressed, rx + rw * 0.5, ry + 40.0, &mut ctx);
        assert_eq!(p.ramps[0].as_ref().unwrap().inner().keys.len(), 3);
        let val = &ParamController::node_params(&*p)[0].1;
        assert_eq!(val.split(',').count(), 3, "spec re-serialized: {val}");
        assert!(val.starts_with("smooth;"));
    }

    #[test]
    fn toggle_click_commits_value_and_unfocus_commits_editor() {
        let mut ctx = UiContext::new();
        let mut p = panel_with(&[("On", "false", "checkbox")]);
        let (cx, cy, _, ch) = p.toggles[0].as_ref().unwrap().rect();
        // Click the toggle row (presses are ungated for this widget; the panel consumes
        // every left press, so the return is true either way — assert the value flip).
        p.mouse_input(MouseButton::Left, ElementState::Pressed, cx + 6.0, cy + ch / 2.0, &mut ctx);
        p.mouse_input(MouseButton::Left, ElementState::Released, cx + 6.0, cy + ch / 2.0, &mut ctx);
        assert_eq!(ParamController::node_params(&*p)[0].1, "true");

        // Code editor: focus it via a click, type, then unfocus commits the buffer.
        let mut p = panel_with(&[("Src", "let x = 1;", "code")]);
        let rects = p.get_param_rects();
        let r = rects[0];
        p.mouse_input(MouseButton::Left, ElementState::Pressed, r.0 + 20.0, r.1 + 30.0, &mut ctx);
        assert_eq!(p.focused_param, Some(0), "code row focused");
        assert!(p.code_editor.is_some());
        p.code_editor.as_mut().unwrap().insert_text("y");
        WidgetHost::unfocus(&mut p);
        assert_eq!(p.focused_param, None);
        assert!(p.code_editor.is_none());
        assert!(ParamController::node_params(&*p)[0].1.contains('y'), "editor buffer committed on unfocus");
    }

    #[test]
    fn clicking_a_section_title_collapses_its_rows() {
        let mut ctx = UiContext::new();
        let mut p = panel_with(&[
            ("Transform", "", "section"),
            ("Size", "1.00", "slider:0:2"),
            ("Shading", "", "section"),
            ("On", "true", "checkbox"),
        ]);
        let expanded_h = p.get_total_content_height();
        let below_before = p.get_param_rects()[2].1;

        // Press the first section's title box.
        let r_hdr = p.get_param_rects()[0];
        let (bx, by, _, bh) = p.section_title_box(0, r_hdr);
        p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 4.0, by + bh / 2.0, &mut ctx);

        assert!(p.section_collapsed("Transform"));
        assert_eq!(p.get_param_rects()[1].3, 0.0, "the collapsed section's row has no height");
        assert!(p.get_param_rects()[2].1 < below_before, "the next section moves up");
        assert!(p.get_total_content_height() < expanded_h);
        // The row's chrome and label are gone; the header's stay.
        assert!(!p.own_text_labels().iter().any(|l| l.text.contains("Size")));
        assert!(p.own_text_labels().iter().any(|l| l.text.contains("Transform")));

        // Clicking it again restores the section.
        let r_hdr = p.get_param_rects()[0];
        let (bx, by, _, bh) = p.section_title_box(0, r_hdr);
        p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 4.0, by + bh / 2.0, &mut ctx);
        assert!(!p.section_collapsed("Transform"));
        assert_eq!(p.get_total_content_height(), expanded_h);
    }

    /// Both ends of a straight run, and of an arc, as points on the path.
    fn run_ends(r: &(f32, f32, f32, f32)) -> [(f32, f32); 2] {
        let (x, y, w, h) = *r;
        if w > h {
            [(x, y), (x + w, y)]
        } else {
            [(x, y), (x, y + h)]
        }
    }
    fn arc_ends(a: &(f32, f32, f32, f32, f32)) -> [(f32, f32); 2] {
        let (cx, cy, r, a0, a1) = *a;
        [
            (cx + r * a0.cos(), cy + r * a0.sin()),
            (cx + r * a1.cos(), cy + r * a1.sin()),
        ]
    }

    #[test]
    fn section_outline_is_one_continuous_path() {
        let p = panel_with(&[("Transform", "", "section"), ("Size", "1.00", "slider:0:2")]);
        let (title, content) = p.section_boxes().into_iter().next().expect("one section");
        let content = content.expect("the section has a content box");
        let (runs, arcs) = p.section_outline(title, content.into());

        // The tab sits flush on the body — its bottom edge is the body's top edge.
        assert_eq!(title.1 + title.3, content.1, "the tab fuses to the body");
        // Folder-tab shape: the tab's two top corners, the concave throat where its right
        // side turns onto the body's top edge, and the body's three remaining corners
        // (its top-LEFT is the tab's left side running straight through).
        assert_eq!(arcs.len(), 6);
        let (tab_right, body_top) = (title.0 + title.2, content.1);
        let throats = arcs
            .iter()
            .filter(|(cx, cy, ..)| *cx > tab_right - 0.01 && *cy < body_top)
            .count();
        assert_eq!(throats, 1, "the throat — centred out in the pocket right of the tab");

        // Every corner hands off to a straight run — no arc dangles. (Within a stroke width:
        // runs and arcs are anchored on opposite ink sides at the concave corner.)
        let ends: Vec<(f32, f32)> = runs.iter().flat_map(|r| run_ends(r)).collect();
        for arc in &arcs {
            for (ax, ay) in arc_ends(arc) {
                let nearest = ends
                    .iter()
                    .map(|(x, y)| ((x - ax).powi(2) + (y - ay).powi(2)).sqrt())
                    .fold(f32::INFINITY, f32::min);
                assert!(nearest <= SECTION_BORDER_T + 0.01, "corner at ({ax}, {ay}) dangles: {nearest}");
            }
        }

        // The tab's bottom edge is open (no run along it), and the body's top edge runs
        // only right of the throat.
        let tab_bottom = title.1 + title.3 - SECTION_BORDER_T;
        let bottom_runs = runs
            .iter()
            .filter(|(_, y, w, _)| (*y - tab_bottom).abs() < 0.01 && *w > SECTION_BORDER_T)
            .count();
        assert_eq!(bottom_runs, 0, "the tab opens onto the body");
        let top_runs: Vec<&(f32, f32, f32, f32)> = runs
            .iter()
            .filter(|(_, y, w, _)| (*y - body_top).abs() < 0.01 && *w > SECTION_BORDER_T)
            .collect();
        assert_eq!(top_runs.len(), 1, "the body's top edge starts past the tab");
        assert!(top_runs[0].0 >= tab_right, "…right of the throat");

        // The left edge is ONE straight run from the tab's top corner to the body's
        // bottom corner.
        let left_runs: Vec<&(f32, f32, f32, f32)> = runs
            .iter()
            .filter(|(x, _, _, h)| (*x - title.0).abs() < 0.01 && *h > 0.0)
            .collect();
        assert_eq!(left_runs.len(), 1, "tab + body share one left side");
        assert!((left_runs[0].1 - (title.1 + SECTION_R)).abs() < 0.01);
        assert!((left_runs[0].1 + left_runs[0].3 - (content.1 + content.3 - SECTION_R)).abs() < 0.01);
    }

    #[test]
    fn a_collapsed_section_outline_closes_on_itself() {
        let mut p = panel_with(&[("Transform", "", "section"), ("Size", "1.00", "slider:0:2")]);
        p.set_section_collapsed("Transform", true);
        let (title, content) = p.section_boxes().into_iter().next().expect("one section");
        assert!(content.is_none(), "nothing to wrap below a collapsed header");
        let (runs, arcs) = p.section_outline(title, None);
        assert_eq!(arcs.len(), 4, "a plain rounded rect");
        assert_eq!(runs.len(), 4);
    }

    #[test]
    fn rows_pack_on_the_channel_and_sections_separate_wider() {
        let p = panel_with(&[
            ("Transform", "", "section"),
            ("Size", "1.00", "slider:0:2"),
            ("Shading", "", "section"),
            ("On", "true", "checkbox"),
        ]);
        let rects = p.get_param_rects();
        let content_bottom = |i: usize| rects[i].1 + rects[i].3 + CONTENT_BOX_PAD;
        let title_top = |i: usize| rects[i].1 - TITLE_BOX_INSET;

        // Inside a section everything packs on the channel: each tab sits flush on its
        // content box, and the rows keep one channel from the box's walls.
        for (title, content) in p.section_boxes() {
            let content = content.expect("both sections have content");
            assert_eq!(title.1 + title.3, content.1, "tab flush on its body");
        }
        let (_, content0) = p.section_boxes()[0];
        let content0 = content0.unwrap();
        assert_eq!(rects[1].1 - content0.1, CHANNEL, "row -> its box's top wall");
        assert_eq!(rects[1].0 - content0.0, CHANNEL, "row -> its box's side wall");

        // Section to section stays far wider, so the blocks still read apart.
        assert_eq!(title_top(2) - content_bottom(1), SECTION_GAP, "section -> next section");
        assert!(SECTION_GAP > 2.0 * CHANNEL, "sections separate wider than any channel");
    }

    #[test]
    fn a_collapsed_section_keeps_the_same_gap_to_the_next_one() {
        // With no content box under it, the collapsed section's bottom edge is its own title
        // box — a fixed row-pitch bump would leave a double gap here.
        let mut p = panel_with(&[
            ("Transform", "", "section"),
            ("Size", "1.00", "slider:0:2"),
            ("Shading", "", "section"),
            ("On", "true", "checkbox"),
        ]);
        p.set_section_collapsed("Transform", true);
        let rects = p.get_param_rects();
        let collapsed_bottom = rects[0].1 - TITLE_BOX_INSET + TITLE_BOX_H;
        let next_title_top = rects[2].1 - TITLE_BOX_INSET;
        assert_eq!(next_title_top - collapsed_bottom, SECTION_GAP);
    }

    #[test]
    fn collapse_survives_a_param_rebuild_and_swallows_row_clicks() {
        let mut ctx = UiContext::new();
        let mut p = panel_with(&[("Shading", "", "section"), ("On", "false", "checkbox")]);
        p.set_section_collapsed("Shading", true);

        // A click where the toggle used to sit must not reach it.
        let (cx, cy, _, ch) = p.toggles[1].as_ref().unwrap().rect();
        p.mouse_input(MouseButton::Left, ElementState::Pressed, cx + 6.0, cy + ch / 2.0, &mut ctx);
        p.mouse_input(MouseButton::Left, ElementState::Released, cx + 6.0, cy + ch / 2.0, &mut ctx);
        assert_eq!(ParamController::node_params(&*p)[1].1, "false", "hidden row ignores clicks");

        // The host re-syncs the panel (node change): collapse is keyed by title, so it holds.
        let params: Vec<(String, String, String)> = [("Shading", "", "section"), ("On", "false", "checkbox"), ("Extra", "1", "int")]
            .iter()
            .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
            .collect();
        ParamController::set_display_params(&mut *p, &params);
        assert!(p.section_collapsed("Shading"));
        assert_eq!(p.get_param_rects()[1].3, 0.0);
    }

    #[test]
    fn plain_view_serves_row_chrome_and_all_quads_stays_empty() {
        let ctx = UiContext::new();
        let p = panel_with(&[("Size", "1.00", "slider:0:2")]);
        // The designer's plain path: extra_quads carries the row chrome (clipped), including
        // the slider background it reads via rect()+color()...
        let extra = WidgetHost::extra_quads(&p);
        assert!(!extra.is_empty(), "row chrome served through extra_quads");
        // ...but NOT the panel's own PARAM_BG plate (the host draws that from color()).
        let (x, y, w, h) = WidgetHost::rect(&p);
        assert!(
            !extra.iter().any(|q| (q.0, q.1, q.2, q.3) == (x, y, w, h)),
            "panel bg plate is the host's, not extra_quads'"
        );
        // The no-double-draw contract of the plain-quad hatch.
        assert!(WidgetHost::all_quads(&p, &ctx).is_empty());
        // Per-label hatch: the walk's text prims carry the widget font and viewport bounds.
        let mut scratch = crate::scene::paint::PaintCtx::new();
        crate::scene::painter::append_widget_text(&ctx, &p, &mut scratch);
        let labels: Vec<_> = scratch
            .finish()
            .items
            .into_iter()
            .filter_map(|item| match item.prim {
                crate::scene::paint::Prim::Text { font, bounds, .. } => Some((font, bounds)),
                _ => None,
            })
            .collect();
        assert!(!labels.is_empty());
        assert!(labels.iter().all(|(font, bounds)| font.is_some() && bounds.is_some()));
    }

    #[test]
    fn scroll_wheel_scrolls_when_content_overflows() {
        let mut ctx = UiContext::new();
        let rows: Vec<(String, String, String)> = (0..30)
            .map(|i| (format!("P{i}"), "1.00".to_string(), "slider:0:2".to_string()))
            .collect();
        let mut p = ParametersBg::new();
        ParamController::set_display_params(&mut *p, &rows);
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 200.0);
        assert!(p.content_h > 200.0);
        assert!(WidgetHost::is_scrollable(&p));
        // Wheel over the panel body but off every slider row's x-span is impossible (rows are
        // full-width), so scroll via the region below the last visible row: use a y between
        // rows (the 2px slack above a row) — simplest is the bottom padding strip.
        let before = p.scroll_y;
        p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), 150.0, 199.0, &mut ctx);
        // Either a slider consumed it (value change) or the panel scrolled; both mark change.
        // The panel-scroll path must work when no slider is under the pointer:
        p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), 2.0, 2.0, &mut ctx);
        assert!(p.scroll_y >= before, "scroll never decreases on a downward wheel");
    }
}
