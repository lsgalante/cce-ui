//! Row and section geometry: the label layout, row heights, section boxes, the
//! rects every row's control is placed in, and the scrollbar's.

use super::*;

impl ParametersBg {

    /// What a row of type `t` spends of its control rect on everything but
    /// its track: a slider's readout and gap, plus a float3's axis column.
    /// Zero for the controls that have no track.
    pub(super) fn control_chrome(t: &str) -> f32 {
        if t.starts_with("slider") {
            crate::widget::input::slider::Slider::readout_chrome()
        } else if is_vec_row(t) {
            let ball = if Self::has_trackball(t) { Float3::trackball_chrome() } else { 0.0 };
            ball + crate::widget::display::float3::AXIS_W + crate::widget::input::slider::Slider::readout_chrome()
        } else {
            0.0
        }
    }

    /// Whether a row of type `t` takes the inline layout at all. Only the
    /// rows whose control would otherwise carry a label strip: toggles and
    /// buttons ARE their label, a ramp keeps its label band, sections and
    /// code rows have layouts of their own.
    pub(super) fn inline_kind(t: &str) -> bool {
        t.starts_with("slider")
            || is_vec_row(t)
            || t.starts_with("spinbox")
            || t.starts_with("choice")
            || is_text_row(t)
            || t.starts_with("color")
            || t == "rgb"
            || t == "rgba"
    }

    /// Whether row `i` is laid out with its label beside the control.
    pub(super) fn inline_row(&self, i: usize) -> bool {
        self.inline_labels && Self::inline_kind(&self.display_params[i].2)
    }

    /// The layout the rows allow: the preference, unless a label column
    /// would leave any visible row's track shorter than
    /// [`Self::MIN_INLINE_TRACK_W`], in which case the labels stack. One
    /// decision for the pane — a mix of inline and stacked rows reads as
    /// ragged — taken by its SHORTEST track, so a pane of spinboxes keeps
    /// its column at a width where a pane with a float3 has let it go. An
    /// unlaid pane (no rect yet) follows the preference; the first rect
    /// assignment decides.
    pub(super) fn decide_inline(&self) -> bool {
        if !self.inline_pref {
            return false;
        }
        if self.rect.width <= 0.0 {
            return true;
        }
        let row_w = (self.rect.width - 2.0 * ROW_X_INSET).max(0.0);
        let control_w = row_w - self.label_col_w_if(true);
        let hidden = self.hidden_rows();
        self.display_params
            .iter()
            .enumerate()
            .filter(|(i, p)| !hidden[*i] && Self::inline_kind(&p.2))
            .all(|(_, p)| control_w - Self::control_chrome(&p.2) >= Self::MIN_INLINE_TRACK_W)
    }

    /// Re-decide the layout for the current rect and, when it flips, move
    /// every label between the column and the controls. The widgets were
    /// built with or without a label, and the two layouts have different
    /// row heights, so a flip is a relabel and a re-layout, not a flag.
    pub(super) fn apply_label_layout(&mut self) {
        let want = self.decide_inline();
        if want != self.inline_labels {
            self.inline_labels = want;
            self.relabel_rows();
        }
    }

    /// Put each inline-kind row's label where the current layout wants it:
    /// on the control when stacked, off it (the pane draws the column) when
    /// inline. Toggles, buttons and ramps carry their label either way.
    pub(super) fn relabel_rows(&mut self) {
        let inline = self.inline_labels;
        for i in 0..self.display_params.len() {
            let (name, ty) = (self.display_params[i].0.clone(), self.display_params[i].2.clone());
            if !Self::inline_kind(&ty) {
                continue;
            }
            macro_rules! relabel {
                ($w:expr) => {
                    if let Some(w) = $w {
                        if inline { w.clear_label() } else { w.set_label(&name) }
                    }
                };
            }
            relabel!(&mut self.sliders[i]);
            relabel!(&mut self.float3s[i]);
            relabel!(&mut self.spinboxes[i]);
            relabel!(&mut self.texts[i]);
            relabel!(&mut self.colors[i]);
            // Only a choice row's dropdown carries the row label; a text
            // row's completion picker is a button inside the box.
            if ty.starts_with("choice:") {
                relabel!(&mut self.choices[i]);
            }
        }
    }

    /// The label strip an inline row no longer spends: the control was
    /// built unlabelled, so the row loses exactly the strip's height.
    pub(super) fn inline_strip(&self, i: usize) -> f32 {
        if self.inline_row(i) { crate::layout::control_label_strip() } else { 0.0 }
    }

    /// The inline label's font: the pane's own labels draw in the control
    /// label font (`Paint::widget_font`), so the column is measured in it —
    /// family AND size — or the widest label runs into its control.
    pub(super) fn inline_label_font() -> (String, f32) {
        crate::layout::control_label_font_parsed()
    }

    /// Width of the label column under the inline layout: the widest visible
    /// inline label plus the gap, clamped to a floor (so a pane of one-word
    /// labels still reads as a column) and to under half the row (so a long
    /// label truncates rather than squeezing the control out). Zero when
    /// nothing is inline.
    pub(super) fn label_col_w(&self) -> f32 {
        self.label_col_w_if(self.inline_labels)
    }

    /// [`Self::label_col_w`] under a given layout — what the column WOULD
    /// take, which is what [`Self::decide_inline`] asks before committing.
    pub(super) fn label_col_w_if(&self, inline: bool) -> f32 {
        if !inline {
            return 0.0;
        }
        let (family, size) = Self::inline_label_font();
        let hidden = self.hidden_rows();
        let widest = self
            .display_params
            .iter()
            .enumerate()
            .filter(|(i, p)| !hidden[*i] && Self::inline_kind(&p.2))
            .map(|(_, p)| crate::widget::display::measure_text_width(&p.0, &family, size))
            .fold(0.0f32, f32::max);
        if widest <= 0.0 {
            return 0.0;
        }
        let row_w = (self.rect.width - 2.0 * ROW_X_INSET).max(0.0);
        (widest + Self::LABEL_GAP).clamp(72.0f32.min(row_w * 0.45), row_w * 0.45)
    }

    /// Row `i`'s CONTROL rect: the row rect less the label column when the
    /// row is inline, the row rect itself otherwise.
    pub(super) fn control_rect(&self, r: (f32, f32, f32, f32), i: usize, lw: f32) -> (f32, f32, f32, f32) {
        if self.inline_row(i) { (r.0 + lw, r.1, (r.2 - lw).max(0.0), r.3) } else { r }
    }

    pub(super) fn row_height(&self, i: usize) -> f32 {
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
            let content_h = Self::CODE_TOP + (line_count as f32 * Self::CODE_LINE_H) + 12.0;
            content_h.max(200.0)
        } else if p.2 == "section" {
            24.0
        } else if p.2 == SEPARATOR {
            // A rule, its own pixel: the row gaps either side are the air.
            1.0
        } else if p.2 == "ramp" {
            // Label band + the ramp's graph and control strip. The control
            // strip and label band are fixed, so this whole increase grows the
            // curve plot (Ramp::graph_h = height − strip).
            260.0
        } else if is_vec_row(&p.2) {
            Float3::preferred_height_for(!self.inline_row(i), vec_row_n(&p.2))
        } else if p.2.starts_with("slider") {
            (38.0 - self.inline_strip(i)).max(22.0)
        } else if is_text_row(&p.2) || p.2.starts_with("spinbox") || p.2.starts_with("choice") {
            (42.0 - self.inline_strip(i)).max(24.0)
        } else if p.2.starts_with("color") || p.2 == "rgb" || p.2 == "rgba" {
            (40.0 - self.inline_strip(i)).max(24.0)
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
    pub(super) fn hidden_rows(&self) -> Vec<bool> {
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
    pub(super) fn section_title_box(&self, hdr: usize, r_hdr: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
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
    pub(super) fn first_visible_row_top(&self, hdr: usize) -> Option<f32> {
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
    pub(super) fn sections(&self) -> Vec<(usize, Option<(usize, usize)>)> {
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

    pub(super) fn section_boxes(&self) -> Vec<((f32, f32, f32, f32), Option<(f32, f32, f32, f32)>)> {
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

    /// Where the rows start, in absolute y — the origin `get_param_rects` lays out from.
    pub(super) fn rows_origin(&self) -> f32 {
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
    pub(super) fn scrollbar_w(&self) -> f32 {
        crate::layout::scrollbar_width() * 1.6
    }

    /// The scrollbar's left x: the bar rides the pane's CENTRE line, as
    /// every sink-behind bar in the DE does (cce-mail's list and body, the
    /// designer's dialog list) — over the rows, reserving no lane, in front
    /// only while raised. It sat a sixth of the width in from the right
    /// edge before 2026-09-21, which beside a centred bar read as off.
    pub(super) fn scrollbar_x(&self) -> f32 {
        self.rect.x + (self.rect.width - self.scrollbar_w()) * 0.5
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

    /// How far the bar's FORE copy has faded in, 0..=1: the host draws the
    /// idle copy before the plate every frame and the fore copy after the
    /// rows at this alpha, so the raise and the sink are a fade, not a flip.
    pub fn scrollbar_fade(&self) -> f32 {
        self.activity.fade()
    }

    /// Re-latch the shared hysteresis with this pane's inputs, returning whether it changed.
    pub(super) fn recompute_scrollbar_raised(&mut self) -> bool {
        let visible = self.scrollbar_visible();
        self.activity.recompute(visible, self.scrollbar_dragging)
    }

    pub(super) fn update_slider_rects(&mut self) {
        // Inline rows hand their control the row less the label column; the
        // row rects themselves (`get_param_rects`) stay the full row, which is
        // what the floors, the section boxes and hit routing measure against.
        let lw = self.label_col_w();
        let rects: Vec<(f32, f32, f32, f32)> = self
            .get_param_rects()
            .into_iter()
            .enumerate()
            .map(|(i, r)| self.control_rect(r, i, lw))
            .collect();
        let inline: Vec<bool> = (0..self.display_params.len()).map(|i| self.inline_row(i)).collect();
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
                    // The picker is the field's RIGHT END: a flush control
                    // plate over the whole band below the detached label,
                    // out to the field's outer edge on the top, right and
                    // bottom as a dropdown trigger's plate is, square where
                    // it meets the text box (whose well stops at the seam —
                    // `TextBox::joined_right`) and the box's radius on the
                    // outside, so the two read as one field. Until
                    // 2026-10-01 it was a button nested INSIDE the box's
                    // well, its face stopping at the base of the well's
                    // wall, so it never reached the edge a dropdown's does.
                    let label_top = if inline[i] { 0.0 } else { crate::layout::control_label_strip() };
                    let band_h = r.3 - label_top;
                    let rr = crate::layout::textbox_corner_radius();
                    // The picker is a trigger's arrow slot: PICK_W and a
                    // wall, the wall being the lip its face rises out of at
                    // the seam. Its arrow in its middle is then where every
                    // other trigger's arrow is, in its slot.
                    let pick_w = crate::widget::input::dropdown::arrow_slot(band_h);
                    d.set_rect(r.0 + r.2 - pick_w, r.1 + label_top, pick_w, band_h);
                    d.inner_mut().set_radii(Some((0.0, rr, rr, 0.0)));
                    d.inner_mut().center_arrow = true;
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
                // A textpick row's box ends where its picker begins.
                let joined = self.display_params[i].2.starts_with("textpick") && self.choices[i].is_some();
                tb.inner_mut().joined_right = joined;
                tb.set_rect(r.0, r.1, r.2, r.3);
                if joined {
                    // The box ends where the picker begins, at the seam.
                    let depth = crate::layout::bevel_width().min((r.3 - tb.label_strip()) * 0.2);
                    tb.set_rect(r.0, r.1, (r.2 - PICK_W - depth).max(0.0), r.3);
                }
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
    pub(super) fn refresh_scroll_metrics(&mut self) {
        // The label layout (inline column or stacked) is decided first, by
        // the tracks the rows would get: every geometry below depends on
        // it. Here rather than on rect assignment alone, because a section
        // collapsing or opening changes which rows are visible, and so
        // which is the shortest track and how wide the column is.
        self.apply_label_layout();
        self.content_h = self.get_total_content_height();
        let max_scroll = (self.content_h - self.rect.height).max(0.0);
        self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        self.update_slider_rects();
    }

    /// The dropdown rows' popover, if one is open — the widget's OWN popover surface
    /// ([`Paint::popover`]); the raw `children`'s popovers are the adapter's recursion.
    /// The ramp rows' field dropdowns count too.
    pub(super) fn choices_popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        for d in self.choices.iter().flatten() {
            if let Some(r) = d.popover_rect() {
                return Some(r);
            }
        }
        for rp in self.ramps.iter().flatten() {
            let ramp = rp.inner();
            if let Some(r) = ramp
                .preset_dropdown
                .popover_rect()
                .or_else(|| ramp.line_type_dropdown.popover_rect())
            {
                return Some(r);
            }
        }
        None
    }

    /// The full legacy popover reach (choices, then children) — hit-testing extends to it.
    pub(super) fn own_popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if let Some(r) = self.choices_popover_rect() {
            return Some(r);
        }
        None
    }
}
