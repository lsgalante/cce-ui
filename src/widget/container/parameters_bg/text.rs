//! The pane's text and glyphs: every row's labels and its controls' text (a long value cut at its
//! tail with an ellipsis), and the glyphs its controls draw, which the pane paints itself.

use super::*;

impl ParametersBg {
    pub(super) fn own_text_labels(&self) -> Vec<TextLabel> {
        let rects = self.get_param_rects();
        let hidden = self.hidden_rows();
        let mut labels = Vec::new();
        let lw = self.label_col_w();
        let (family, size) = Self::inline_label_font();
        for (i, (name, value, ptype)) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            let r = rects[i];
            if self.inline_row(i) {
                // The pane's own label, in the column beside an unlabelled
                // control: vertically centred on the row, tail-truncated to
                // the column — by the measure the column is sized with, so
                // a label it was sized to hold is never cut (it was cut to
                // as many chars as the column holds M's).
                labels.push(TextLabel {
                    text: fit_tail(name, lw - Self::LABEL_GAP, &family, size),
                    x: r.0,
                    y: r.1 + (r.3 - size) * 0.5,
                    font_size: size,
                    color: if self.hover_row == Some(i) { Self::LABEL_HOVER } else { Self::LABEL },
                });
            }
            if ptype.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    labels.extend(s.own_text_labels());
                }
            } else if is_vec_row(ptype) {
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
                // One label PER LINE, at the same pitch the cursor math uses
                // (`plain_quads`' cursor_y) — a single multi-line label would depend on
                // the consumer's buffer line-height matching that pitch, and never
                // exactly did. A line number sits in the gutter before each.
                let text_x = self.code_text_x(r);
                for (line_i, line) in val_text.split('\n').enumerate() {
                    let y = r.1 + Self::CODE_TOP + line_i as f32 * Self::CODE_LINE_H;
                    let number = format!("{:>3}", line_i + 1);
                    labels.push(TextLabel {
                        text: number,
                        x: r.0 + 12.0,
                        y,
                        font_size: 12.0,
                        color: if self.code_error_line == Some(line_i) { [0xff, 0x80, 0x70] } else { [0x66, 0x66, 0x78] },
                    });
                    if line.is_empty() {
                        continue;
                    }
                    labels.push(TextLabel {
                        text: line.to_string(),
                        x: text_x,
                        y,
                        font_size: 12.0,
                        color: [0xee, 0xee, 0xf0],
                    });
                }
                if self.focused_param == Some(i) && self.code_is_dirty() {
                    labels.push(TextLabel {
                        text: "ctrl+enter applies".to_string(),
                        x: r.0 + r.2 - 12.0 - 18.0 * self.code_col_w(),
                        y: r.1 + r.3 - 15.0,
                        font_size: 12.0,
                        color: [0xd8, 0xa0, 0x50],
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
            } else if ptype == SEPARATOR {
                // A rule says nothing.
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

    /// The hosted controls' GLYPHS — a dropdown's or a picker's arrow, a
    /// spinbox's −/+ — as `(image, rect, alpha)`, row by row as
    /// [`own_text_labels`](Self::own_text_labels) collects their text. The
    /// pane paints its controls' chrome itself and never runs their
    /// `Paint::paint` into the frame, so a symbol a control draws reaches
    /// the pane only through here. Ramps are not asked: they paint whole
    /// through `paint_scene_rows`.
    pub(super) fn child_glyphs(&self) -> Vec<(u32, Rect, f32)> {
        let hidden = self.hidden_rows();
        let mut glyphs = Vec::new();
        for (i, (_, _, ptype)) in self.display_params.iter().enumerate() {
            if hidden[i] {
                continue;
            }
            if ptype.starts_with("slider") {
                if let Some(s) = &self.sliders[i] {
                    glyphs.extend(s.own_glyphs());
                }
            } else if is_vec_row(ptype) {
                if let Some(f) = &self.float3s[i] {
                    glyphs.extend(f.own_glyphs());
                }
            } else if ptype.starts_with("spinbox") {
                if let Some(sb) = &self.spinboxes[i] {
                    glyphs.extend(sb.own_glyphs());
                }
            } else if is_text_row(ptype) {
                if let Some(tb) = &self.texts[i] {
                    glyphs.extend(tb.own_glyphs());
                }
                if let Some(d) = &self.choices[i] {
                    glyphs.extend(d.own_glyphs());
                }
            } else if ptype.starts_with("choice") {
                if let Some(d) = &self.choices[i] {
                    glyphs.extend(d.own_glyphs());
                }
            } else if ptype == "button" {
                if let Some(b) = &self.buttons[i] {
                    glyphs.extend(b.own_glyphs());
                }
            } else if ptype == "toggle" || ptype == "checkbox" {
                if let Some(cb) = &self.toggles[i] {
                    glyphs.extend(cb.own_glyphs());
                }
            } else if ptype.starts_with("color") || ptype == "rgb" || ptype == "rgba" {
                if let Some(c) = &self.colors[i] {
                    glyphs.extend(c.own_glyphs());
                }
            }
        }
        glyphs
    }

    /// Paint [`child_glyphs`](Self::child_glyphs), over the chrome.
    pub(super) fn paint_child_glyphs(&self, ctx: &mut PaintCtx) {
        for (image, rect, alpha) in self.child_glyphs() {
            ctx.image(image, rect, alpha);
        }
    }
}

/// `text` whole if it measures within `avail` px, else its longest head
/// with "..." that does.
pub(super) fn fit_tail(text: &str, avail: f32, family: &str, size: f32) -> String {
    use crate::widget::display::{measure_text_width, truncate_tail};
    let fits = |t: &str| measure_text_width(t, family, size) <= avail;
    if fits(text) {
        return text.to_string();
    }
    (0..text.chars().count())
        .rev()
        .map(|n| truncate_tail(text, n))
        .find(|t| fits(t))
        .unwrap_or_default()
}
