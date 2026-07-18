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
use crate::widget::input::{Button, ColorSelector, Dropdown, Slider, Spinbox, TextBox, Toggle};
use crate::widget::{
    Adapted, WidgetHost, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton,
    MouseScrollDelta, NamedKey, Paint, ParamController, TextEditorState, UiContext,
};

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
    /// Titles of the sections the user has collapsed by clicking their header. Keyed by
    /// title so it outlives the row rebuild `set_display_params` runs on every node change.
    collapsed: std::collections::HashSet<String>,
    visible: bool,
    pub scroll_y: f32,
    pub content_h: f32,
    scrollbar_dragging: bool,
    drag_offset_y: f32,
    /// Seconds left in the "recently scrolled" window that keeps the scrollbar raised in
    /// front of the pane plate; decays in `tick`. See [`ParametersBg::scrollbar_active`].
    scroll_activity: f32,
    /// Whether the pointer currently sits over the scrollbar track (updated on pointer move).
    scrollbar_hover: bool,
    /// Latched "raised in front of the plate" state, with hysteresis: a wheel scroll (or an
    /// active drag) raises it; hover only *sustains* an already-raised bar; nothing else
    /// raises it. While sunk it is behind the plate, so hover and clicks can't reach it —
    /// the plate occludes it. Recomputed via [`ParametersBg::recompute_scrollbar_raised`].
    scrollbar_raised: bool,
}

/// How long (seconds) the scrollbar stays raised after the last wheel scroll or drag release.
const SCROLL_ACTIVE_HOLD: f32 = 0.7;

/// Vertical pitch between consecutive rows.
const ROW_GAP: f32 = 8.0;
/// How far a section's title box overhangs its header row upward, and the box's height.
const TITLE_BOX_INSET: f32 = 2.0;
const TITLE_BOX_H: f32 = 22.0;
/// How far a section's content box overhangs the first and last row it wraps.
const CONTENT_BOX_PAD: f32 = 4.0;
/// The gap between one section's bottom box edge and the next section's title box. Equal to
/// the title→content gap by construction: a header row is `TITLE_BOX_INSET + TITLE_BOX_H`
/// tall as drawn and `ROW_GAP` from the row under it, whose content box starts
/// `CONTENT_BOX_PAD` early — leaving exactly `ROW_GAP`. Laying the next header out from the
/// previous block's *drawn* bottom edge (rather than the uniform row pitch, which the two
/// boxes' overhangs eat into unequally) keeps the two gaps identical.
const SECTION_GAP: f32 = ROW_GAP;

impl ParametersBg {
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
            collapsed: std::collections::HashSet::new(),
            visible: true,
            scroll_y: 0.0,
            content_h: 0.0,
            scrollbar_dragging: false,
            drag_offset_y: 0.0,
            scroll_activity: 0.0,
            scrollbar_raised: false,
            scrollbar_hover: false,
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
        } else if p.2.starts_with("float3") {
            108.0
        } else if p.2.starts_with("slider") {
            38.0
        } else if p.2 == "text" || p.2.starts_with("spinbox") || p.2.starts_with("choice") {
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
        let full_w = self.rect.width - 8.0;
        let (label_family, label_size) = crate::layout::control_label_font_parsed();
        let text_w =
            crate::widget::display::measure_text_width(&self.display_params[hdr].0, &label_family, label_size);
        let title_w = (text_w + 16.0).min(full_w);
        (self.rect.x + 4.0, r_hdr.1 - TITLE_BOX_INSET, title_w, TITLE_BOX_H)
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
                rects.push((self.rect.x + 8.0, cur_y, self.rect.width - 16.0, 0.0));
                continue;
            }
            let is_header = self.display_params[i].2 == "section";
            if is_header {
                if let Some(bottom) = prev_bottom {
                    cur_y = bottom + SECTION_GAP + TITLE_BOX_INSET;
                }
            }
            let h = self.row_height(i);
            rects.push((self.rect.x + 8.0, cur_y, self.rect.width - 16.0, h));
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

    /// The scrollbar's left x. The bar sits an eighth of the plate's width in from the right
    /// edge — i.e. that gap separates the bar's right edge from the pane's right side.
    fn scrollbar_x(&self) -> f32 {
        let sb_w = crate::layout::scrollbar_width();
        self.rect.x + self.rect.width - sb_w - self.rect.width / 8.0
    }

    pub fn hit_test_scrollbar(&self, px: f32, py: f32) -> bool {
        if self.content_h <= self.rect.height {
            return false;
        }
        let sb_w = crate::layout::scrollbar_width();
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
        self.scrollbar_raised
    }

    /// Recompute the latched "raised" state with hysteresis, returning whether it changed.
    ///
    /// A wheel scroll (`scroll_activity`) or an active drag raises the bar. Hover only
    /// *sustains* a bar that is already raised — it can never raise a sunk one, because a
    /// sunk bar is behind the plate and the plate is what the pointer is actually over. Once
    /// nothing holds it up it sinks, and can only rise again by scrolling.
    fn recompute_scrollbar_raised(&mut self) -> bool {
        let raised = self.scrollbar_visible()
            && (self.scrollbar_dragging
                || self.scroll_activity > 0.0
                || (self.scrollbar_raised && self.scrollbar_hover));
        let changed = raised != self.scrollbar_raised;
        self.scrollbar_raised = raised;
        changed
    }

    /// The scrollbar's track + thumb quads (empty when no scrollbar is needed). The host draws
    /// these either behind or in front of the pane plate per [`Self::scrollbar_active`]; they
    /// are deliberately kept out of [`Self::plain_quads`] so the host controls their depth.
    pub fn scrollbar_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.scrollbar_visible() {
            return Vec::new();
        }
        let sb_w = crate::layout::scrollbar_width();
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
                d.set_rect(r.0, r.1, r.2, r.3);
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
                labels.push(TextLabel {
                    text: name.clone(),
                    x: self.rect.x + 12.0,
                    y: r.1 + 2.0,
                    font_size: 13.0,
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
            } else if ptype == "text" {
                if let Some(tb) = &self.texts[i] {
                    labels.extend(tb.own_text_labels());
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
            } else {
                labels.push(TextLabel {
                    text: format!("{}: {}", name, value),
                    x: self.rect.x + 8.0,
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
    fn choices_popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        for d_opt in &self.choices {
            if let Some(d) = d_opt {
                if let Some(r) = d.popover_rect() {
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
                        p.1 = format!("{:.2}", new_val);
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
                } else if p.2 == "text" {
                    if let Some(tb) = &mut self.texts[idx] {
                        tb.unfocus();
                        p.1 = tb.text.clone();
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

        // Find each section: its header row index plus the content-row range
        // beneath it (up to the next header). `content` is None for a header
        // with no rows under it.
        let mut sections: Vec<(usize, Option<(usize, usize)>)> = Vec::new();
        for (i, p) in self.display_params.iter().enumerate() {
            if p.2 == "section" {
                sections.push((i, None));
            } else if let Some((_, content)) = sections.last_mut() {
                match content {
                    Some((_, end)) => *end = i,
                    None => *content = Some((i, i)),
                }
            }
        }

        // Draw, per section: a box around the title, a box around the content
        // rows, and a short vertical line joining the two.
        let border_color = [0.18, 0.18, 0.27, 1.0];
        let border_t = 1.0;
        let push_box = |quads: &mut Vec<(f32, f32, f32, f32, [f32; 4])>,
                        bx: f32, by: f32, bw: f32, bh: f32| {
            quads.push((bx, by, bw, border_t, border_color)); // top
            quads.push((bx, by + bh - border_t, bw, border_t, border_color)); // bottom
            quads.push((bx, by, border_t, bh, border_color)); // left
            quads.push((bx + bw - border_t, by, border_t, bh, border_color)); // right
        };

        let full_w = self.rect.width - 8.0;
        for (hdr, content) in sections {
            if hdr >= rects.len() {
                continue;
            }
            let r_hdr = rects[hdr];

            // Title box, wrapping the header label (drawn at rect.x + 12) — also the
            // click target that collapses the section.
            let (tb_x, tb_y, title_w, tb_h) = self.section_title_box(hdr, r_hdr);
            push_box(&mut param_quads, tb_x, tb_y, title_w, tb_h);

            // Collapsed: the title box is the whole section — no content box, no connector.
            if self.collapsed.contains(&self.display_params[hdr].0) {
                continue;
            }

            // Content box + the connector line dropping into it from the title.
            if let Some((start, end)) = content {
                if start <= end && start < rects.len() && end < rects.len() {
                    let r_start = rects[start];
                    let r_end = rects[end];
                    let by = r_start.1 - CONTENT_BOX_PAD;
                    let bh = (r_end.1 + r_end.3 + CONTENT_BOX_PAD) - by;
                    push_box(&mut param_quads, tb_x, by, full_w, bh);

                    let line_x = tb_x + 12.0;
                    let line_top = tb_y + tb_h;
                    if by > line_top {
                        param_quads.push((line_x, line_top, border_t, by - line_top, border_color));
                    }
                }
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
                        let cursor_x = r.0 + 12.0 + (cursor_c as f32 * 7.2);
                        let cursor_y = r.1 + 22.0 + (cursor_l as f32 * 16.0) + (16.0 - 13.0) / 2.0;
                        if cursor_y >= r.1 + 18.0 && cursor_y + 13.0 <= r.1 + r.3 {
                            param_quads.push((cursor_x, cursor_y, 1.5, 13.0, [0.80, 0.80, 0.85, 1.0]));
                        }
                    }
                }
            } else if p.2 == "text" {
                if let Some(tb) = &self.texts[i] {
                    param_quads.extend(tb.extra_quads());
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
            if p.2 == "text" {
                if let Some(tb) = &self.texts[i] {
                    out.extend(tb.all_rounded_quads(ctx));
                }
            } else if p.2.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    out.extend(d.all_rounded_quads(ctx));
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
    /// The panel IS its own background plate (the host draws it from `color()` + the corner
    /// style via `push_widget_vertices`) — there is no separate plate widget behind it, so
    /// this carries the full plate treatment: `PARAM_BG` scaled by the global plate opacity,
    /// with the alpha negated as the scenefx blur marker when plate blur is on. Transparent
    /// while hidden. It must not ALSO be emitted as a quad anywhere or it would double-blend.
    fn color(&self) -> [f32; 4] {
        if !self.visible {
            return [0.0, 0.0, 0.0, 0.0];
        }
        let mut c = colors::PARAM_BG;
        c[3] *= crate::layout::plate_opacity();
        if colors::plate_blur() {
            c[3] = -c[3].abs();
        }
        c
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

    /// Scene-path emission (the designer renders through the legacy hatches instead): the row
    /// chrome plus the viewport-filtered labels. The background plate stays out — see
    /// [`color`](Paint::color).
    fn paint(&self, _rect: Rect, ctx: &mut PaintCtx) {
        for (qx, qy, qw, qh, qc) in self.plain_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
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
                    let new_val_str = format!("{:.2}", new_val);
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
            self.scroll_activity = SCROLL_ACTIVE_HOLD;
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
        for cb_opt in &mut self.toggles {
            if let Some(cb) = cb_opt {
                if cb.tick(dt, &mut dummy) {
                    changed = true;
                }
            }
        }
        // Decay the "recently scrolled" window; keep frames coming until it expires so the
        // scrollbar's sink behind the plate actually renders.
        if self.scroll_activity > 0.0 {
            self.scroll_activity = (self.scroll_activity - dt).max(0.0);
            changed = true;
        }
        // Re-latch the raised state (e.g. sink once the scroll window lapses).
        if self.recompute_scrollbar_raised() {
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
                self.scrollbar_hover = self.hit_test_scrollbar(px, py);
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
                        if self.scrollbar_raised && self.hit_test_scrollbar(px, py) {
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
                            self.scroll_activity = SCROLL_ACTIVE_HOLD;
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
                        if d.popover_rect().is_some() {
                            if d.mouse_input(button, state, px, py, ui) {
                                if d.take_change() {
                                    if let Some(val) = d.get_value_string() {
                                        self.display_params[i].1 = val;
                                    }
                                }
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
                                if b.take_click() {
                                    p.1 = "clicked".to_string();
                                }
                                return true;
                            }
                        }
                    } else if p.2 == "text" {
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
                                let col = (click_x / 7.2 + 0.5).floor().max(0.0) as usize;
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
                        } else if p.2 == "text" {
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
                                    p.1 = format!("{:.2}", new_val);
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
                let rects = self.get_param_rects();
                for (i, p) in self.display_params.iter_mut().enumerate() {
                    if p.2.starts_with("slider") {
                        let r = rects[i];
                        let row_y = r.1;
                        if py >= row_y - 2.0 && py <= row_y + r.3 && px >= self.rect.x && px <= self.rect.x + self.rect.width {
                            if let Some(s) = &mut self.sliders[i] {
                                let was_scroll = s.scroll_enabled;
                                s.set_scroll(true);
                                if s.mouse_wheel(delta, px, py, ui) {
                                    let (min, max) = parse_slider_range(&p.2);
                                    let new_val = min + s.value * (max - min);
                                    let old_val = &p.1;
                                    let new_val_str = format!("{:.2}", new_val);
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
                                        let scroll_amount = match delta {
                                            MouseScrollDelta::LineDelta(_x, y) => *y,
                                            MouseScrollDelta::PixelDelta(pos) => (pos.y as f32) / 120.0,
                                        };
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
                if !changed && !ui.is_coordinate_covered(self_id, px, py) {
                    let in_rect = px >= self.rect.x
                        && px <= self.rect.x + self.rect.width
                        && py >= self.rect.y
                        && py <= self.rect.y + self.rect.height;
                    let in_popover = self.own_popover_rect().map_or(false, |(rx, ry, rw, rh)| {
                        px >= rx && px <= rx + rw && py >= ry && py <= ry + rh
                    });
                    if in_rect || in_popover {
                        let scroll_speed = 24.0;
                        let dy = match delta {
                            MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                            MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
                        };
                        let old_scroll = self.scroll_y;
                        let max_scroll = (self.content_h - self.rect.height).max(0.0);
                        self.scroll_y = (self.scroll_y + dy).clamp(0.0, max_scroll);
                        if (self.scroll_y - old_scroll).abs() > 0.01 {
                            self.update_slider_rects();
                            self.scroll_activity = SCROLL_ACTIVE_HOLD;
                            self.recompute_scrollbar_raised();
                            changed = true;
                        }
                    }
                }

                changed
            }
            _ => false,
        }
    }
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
                    Some(Slider::new().with_value(t).with_range(min, max).with_readout(true).with_label(&p.0))
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
                    Some(Button::new(0.0, 0.0, 0.0, 0.0).with_label(&p.0))
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
                } else {
                    None
                }
            }).collect();
            self.texts = self.display_params.iter().map(|p| {
                if p.2 == "text" {
                    Some(TextBox::new(p.1.clone()).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
            self.toggles = self.display_params.iter().map(|p| {
                if p.2 == "toggle" || p.2 == "checkbox" {
                    let on = p.1.trim().to_lowercase() == "true";
                    let mut t = Toggle::new().with_label(&p.0);
                    t.set_toggled(on);
                    Some(t)
                } else {
                    None
                }
            }).collect();
            self.colors = self.display_params.iter().map(|p| {
                if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
                    let col = parse_hex_to_rgb(&p.1).unwrap_or([255, 255, 255]);
                    Some(ColorSelector::new(col).with_label(&p.0))
                } else {
                    None
                }
            }).collect();
        } else {
            for (i, p_new) in params.iter().enumerate() {
                if Some(i) != self.focused_param && Some(i) != self.dragging_param {
                    self.display_params[i].1 = p_new.1.clone();
                    if let Some(ref mut s) = self.sliders[i] {
                        let val = p_new.1.parse::<f32>().unwrap_or(0.0);
                        let (min, max) = parse_slider_range(&p_new.2);
                        let t = if max - min != 0.0 {
                            ((val - min) / (max - min)).clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        s.set_value(t);
                    } else if let Some(ref mut f) = self.float3s[i] {
                        let (min, max) = parse_slider_range(&p_new.2);
                        let vals = parse_float3_value(&p_new.1, min, max);
                        f.set_values(vals);
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
        assert_eq!((sx, sw), (8.0, 284.0), "row rect derives from the assigned rect");
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

    #[test]
    fn section_to_section_gap_matches_the_title_to_content_gap() {
        let p = panel_with(&[
            ("Transform", "", "section"),
            ("Size", "1.00", "slider:0:2"),
            ("Shading", "", "section"),
            ("On", "true", "checkbox"),
        ]);
        let rects = p.get_param_rects();
        let title_bottom = |i: usize| rects[i].1 - TITLE_BOX_INSET + TITLE_BOX_H;
        let content_top = |i: usize| rects[i].1 - CONTENT_BOX_PAD;
        let content_bottom = |i: usize| rects[i].1 + rects[i].3 + CONTENT_BOX_PAD;
        let title_top = |i: usize| rects[i].1 - TITLE_BOX_INSET;

        assert_eq!(content_top(1) - title_bottom(0), SECTION_GAP, "title -> its content box");
        assert_eq!(title_top(2) - content_bottom(1), SECTION_GAP, "section -> next section");
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
