//! The well's geometry: border, padding, wall inset and text clip; content width; keeping the
//! caret in view.

use super::*;

impl TextBox {

    pub(super) fn border_width(&self) -> f32 {
        if self.multiline {
            crate::layout::textbox_multiline_border_width()
        } else {
            1.0
        }
    }

    /// The inset from the box's edge to its text — the caret, selection,
    /// hit-testing, wrap and scroll range all measure from it. 8px, or the
    /// well's floor when the relief wall reaches further in: the wall is
    /// `bevel_width` deep once the box is tall enough (`well`'s 20% cap), so a
    /// fixed 8px sat a multiline box's text on its wall.
    pub(super) fn pad(&self) -> f32 {
        8.0f32.max(self.wall_inset())
    }

    /// How far in from the box's edge its relief wall ends (the outline's own
    /// inset from the carve plus the wall's depth); zero with no relief.
    pub(super) fn wall_inset(&self) -> f32 {
        self.well().map_or(0.0, |f| f.rect.x - self.rect.x + f.depth)
    }

    /// The value text's clip, `[x1, y1, x2, y2]`, from the content rect (the
    /// box below its label strip): the well's floor, so scrolled text slides
    /// under the relief wall rather than over it. Only a multiline box is
    /// clipped in y: a single line is centred, never scrolls vertically, and a
    /// short box's floor is shallower than its line.
    pub(super) fn text_clip(&self, content: Rect) -> [f32; 4] {
        let inset = self.wall_inset();
        let inset_y = if self.multiline { inset } else { 0.0 };
        [
            content.x + inset,
            content.y + inset_y,
            content.x + content.width - inset,
            content.y + content.height - inset_y,
        ]
    }

    /// The widest line's drawn advance — shaped when recorded, else the
    /// chars × char_width estimate (the pre-shaping formula).
    pub(super) fn content_width(&self, lines: &[String]) -> f32 {
        let shaped = if self.multiline {
            self.line_glyph_positions
                .iter()
                // A line's width is its widest stop: the last in left-to-right text, the
                // first in right-to-left.
                .map(|l| l.iter().copied().fold(0.0f32, f32::max))
                .fold(0.0f32, f32::max)
        } else {
            self.total_text_width
        };
        if shaped > 0.0 {
            shaped
        } else {
            let max_line_len = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            max_line_len as f32 * self.char_width()
        }
    }

    pub fn clamp_scroll(&mut self) {
        let pad = self.pad();
        let line_height = self.line_height();
        let max_w = self.wrap_width(self.rect.width);
        let (lines, _) = if self.multiline {
            self.wrap_text(max_w)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            (vec![buffer.clone()], vec![(0, 0); buffer.chars().count() + 1])
        };

        if self.multiline {
            let content_h = lines.len() as f32 * line_height;
            let max_scroll = (content_h - (self.rect.height - 2.0 * pad)).max(0.0);
            self.scroll_y = self.scroll_y.clamp(0.0, max_scroll);
        } else {
            self.scroll_y = 0.0;
        }

        if !self.line_wrap_enabled() {
            let content_w = self.content_width(&lines);
            let max_scroll_x = (content_w - (self.rect.width - 2.0 * pad)).max(0.0);
            self.scroll_x = self.scroll_x.clamp(0.0, max_scroll_x);
        } else {
            self.scroll_x = 0.0;
        }
    }

    pub fn scroll_to_cursor(&mut self) {
        let pad = self.pad();
        let char_width = self.char_width();
        let line_height = self.line_height();
        let max_w = self.wrap_width(self.rect.width);
        let (_lines, index_map) = if self.multiline {
            self.wrap_text(max_w)
        } else {
            let buffer = if self.editing { &self.edit_buffer } else { &self.text };
            let mut m = Vec::new();
            for i in 0..=buffer.chars().count() {
                m.push((0, i));
            }
            (vec![buffer.clone()], m)
        };
        if index_map.is_empty() { return; }

        let cursor_idx = self.cursor_idx.min(index_map.len() - 1);
        let (line_idx, col_idx) = index_map[cursor_idx];

        let top = self.label_top();
        let viewport_w = self.rect.width - 2.0 * pad;
        let viewport_h = self.rect.height - top - 2.0 * pad;

        if self.multiline {
            let line_y = top + pad + (line_idx as f32 * line_height);
            if line_y < self.scroll_y + 10.0 {
                self.scroll_y = (line_y - 20.0).max(0.0);
            } else if line_y + line_height > self.scroll_y + viewport_h - 10.0 {
                self.scroll_y = (line_y + line_height - viewport_h + 20.0).max(0.0);
            }
        }

        if !self.line_wrap_enabled() {
            let cursor_x = if self.multiline {
                self.line_col_x(line_idx, col_idx)
            } else {
                self.glyph_positions
                    .get(col_idx)
                    .copied()
                    .unwrap_or(col_idx as f32 * char_width)
            };
            if cursor_x < self.scroll_x + 10.0 {
                self.scroll_x = (cursor_x - 20.0).max(0.0);
            } else if cursor_x + char_width > self.scroll_x + viewport_w - 10.0 {
                self.scroll_x = (cursor_x + char_width - viewport_w + 20.0).max(0.0);
            }
        }
        self.clamp_scroll();
    }

    /// The well this TextBox carves: a [`crate::scene::paint::Field`] that is
    /// all well, lit while editing — its radii top-left, top-right,
    /// bottom-right, bottom-left, the right two square when the box is
    /// [`Self::joined_right`] (its host draws the field that joins it to a
    /// run, `ParametersBg::fields`). `None` when it draws no relief at all
    /// (square-cornered legacy geometry, `control_relief` off, or a box that
    /// draws no background).
    ///
    /// The SINGLE source for that geometry: `paint` carves it here, and the
    /// flat-host bridge (`layout::render_widget`) relays that carve to a flat
    /// host as a [`relief_carve`](crate::scene::paint::RenderTarget::relief_carve).
    /// A second copy of this math in the bridge is exactly how the two would
    /// drift apart.
    pub fn well(&self) -> Option<crate::scene::paint::Field> {
        let radius = crate::layout::textbox_corner_radius();
        if radius <= 0.0 || !self.recessed() || !self.draw_bg_border {
            return None;
        }
        let top = self.label_top();
        let well = Rect {
            x: self.rect.x,
            y: self.rect.y + top,
            width: self.rect.width,
            height: self.rect.height - top,
        };
        let depth = crate::layout::bevel_width().min(well.height * 0.2);
        let right = if self.joined_right { 0.0 } else { radius };
        let (well, radii) = crate::layout::carve_inside(well, (radius, right, right, radius), depth);
        let tint = self.editing.then(|| {
            let hc = crate::color::highlight_primary_color();
            [hc[0], hc[1], hc[2]]
        });
        Some(crate::scene::paint::Field::well(well, radii, depth).with_tint(tint))
    }
}
