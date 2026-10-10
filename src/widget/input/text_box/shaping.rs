//! Measuring and wrapping: the value's per-glyph advances (cached per shaping key), wrap
//! by shaped width, and the mapping between x, columns and buffer indices.

use super::*;

impl TextBox {

    pub(super) fn map_x_to_idx(&self, click_x: f32) -> usize {
        let pad = self.pad();
        let relative_x = click_x - (self.rect.x + pad) + self.scroll_x;
        if self.glyph_positions.is_empty() {
            let char_width = self.char_width();
            return ((relative_x / char_width).round() as isize)
                .max(0)
                .min(self.edit_buffer.chars().count() as isize) as usize;
        }

        let mut closest_idx = 0;
        let mut min_diff = f32::MAX;
        for (i, &pos) in self.glyph_positions.iter().enumerate() {
            let diff = (pos - relative_x).abs();
            if diff < min_diff {
                min_diff = diff;
                closest_idx = i;
            }
        }
        // When the box is empty, `prepare_text` shapes the PLACEHOLDER into
        // `glyph_positions`, so the nearest-glyph snap above can land on a
        // placeholder column. Clamp to the real text: the placeholder is
        // painted, not caret-addressable.
        let text_len = if self.editing { self.edit_buffer.chars().count() } else { self.text.chars().count() };
        closest_idx.min(text_len)
    }

    /// The x offset of `col` on wrapped line `line`, from the shaped per-line
    /// offsets when recorded, else the uniform-grid estimate.
    pub(super) fn line_col_x(&self, line: usize, col: usize) -> f32 {
        self.line_glyph_positions
            .get(line)
            .and_then(|l| l.get(col).copied())
            .unwrap_or_else(|| col as f32 * self.char_width())
    }

    /// An x offset (relative to the text origin) → nearest column on wrapped
    /// line `line`, from the shaped offsets when recorded.
    pub(super) fn line_x_to_col(&self, line: usize, relative_x: f32) -> usize {
        let Some(offsets) = self.line_glyph_positions.get(line).filter(|l| !l.is_empty()) else {
            return ((relative_x / self.char_width()).round() as isize).max(0) as usize;
        };
        let mut closest = 0;
        let mut min_diff = f32::MAX;
        for (i, &pos) in offsets.iter().enumerate() {
            let diff = (pos - relative_x).abs();
            if diff < min_diff {
                min_diff = diff;
                closest = i;
            }
        }
        closest
    }

    /// The font the value text is drawn in — AND measured in: `prepare_text`
    /// (whose per-glyph positions place the caret, the selection and
    /// click-to-column) shapes with exactly this, and `paint` emits it. It is
    /// [`Paint::text_font`], because that is what actually draws the text:
    /// `Adapted::paint_self` drops the Text prims `paint` emits and redraws
    /// the value through its own-labels bridge in `text_font()` — the box's
    /// family when customized (cce-text-editor's monospace editor), else the
    /// configured control font string.
    ///
    /// Until 2026-09-25 the measurement shaped `font_family` instead (or
    /// `monospace` for a password), a different face from the drawn one, so
    /// the caret drifted off the text a little more with every character —
    /// two dots short after twelve in the login greeter's password box.
    pub fn value_font(&self) -> Option<String> {
        <Self as Paint>::text_font(self)
    }

    /// The shaping half of `prepare_text`: `glyph_positions` and
    /// `total_text_width` for the whole display text, and, multiline,
    /// `line_glyph_positions` per wrapped line as the paint draws them.
    pub(super) fn shape_columns(&mut self, fs: &mut cosmic_text::FontSystem, key: &PrepKey) {
        let scale = f32::from_bits(key.scale_bits);
        let font_fam = key.font.as_deref();
        let attrs = key.attrs;
        let shared = |fs: &mut cosmic_text::FontSystem, text: &str, size: f32| {
            crate::backend::text::shared_text_buffer_at(fs, text, size, font_fam, attrs, scale)
        };

        let render_text = if key.password {
            "•".repeat(key.text.chars().count())
        } else {
            key.text.clone()
        };

        // A multiline box reads only the per-line offsets; shaping the whole
        // document as one buffer would be its most expensive and least used step.
        self.line_glyph_positions.clear();
        if let Some(bits) = key.wrap {
            // Measure the wrapped text's advances with the paint's font system first, so
            // the wrap below never reaches for the shared one (which may be this one).
            let src = if self.editing { self.edit_buffer.clone() } else { self.text.clone() };
            self.cache_advances(fs, &src, scale);
            let (lines, map) = self.wrap_text(f32::from_bits(bits));
            // Each wrapped line's paragraph, and whether that paragraph is right to left: a
            // paragraph's direction is its first strong character's, for all its lines.
            let para_rtl: Vec<bool> = src.split('\n').map(crate::backend::text::paragraph_rtl).collect();
            let mut para_of_line = vec![0usize; lines.len()];
            let mut para = 0usize;
            let mut seen = vec![false; lines.len()];
            for (ci, ch) in src.chars().enumerate() {
                let line = map[ci].0.min(lines.len() - 1);
                if !seen[line] {
                    seen[line] = true;
                    para_of_line[line] = para;
                }
                if ch == '\n' {
                    para += 1;
                }
            }
            for (line, s) in seen.iter().enumerate() {
                if !s {
                    para_of_line[line] = para;
                }
            }
            let room = f32::from_bits(key.room_bits);
            self.line_shift.clear();
            self.line_runs.clear();
            for (li, line) in lines.iter().enumerate() {
                let line_buffer = shared(fs, line, self.font_size);
                let run = crate::backend::text::shaped_run(&line_buffer, line, scale);
                let rtl = para_rtl.get(para_of_line[li]).copied().unwrap_or(false);
                let shift = if rtl { (room - run.width).max(0.0) } else { 0.0 };
                self.line_glyph_positions.push(run.stops.iter().map(|s| s.1 + shift).collect());
                self.line_shift.push(shift);
                self.line_runs.push(run);
            }
            self.glyph_positions = vec![0.0; render_text.chars().count() + 1];
            self.total_text_width = 0.0;
            return;
        }

        let buffer = shared(fs, &render_text, self.font_size);
        let run = crate::backend::text::shaped_run(&buffer, &render_text, scale);
        let total_w = run.width;
        // A right-to-left line that fits is set against the right. One that does not
        // overflows to the LEFT, its start being at the right: shown and not being edited,
        // it is scrolled to its right end, where a left-to-right line shows its left (until
        // 2026-10-08 it showed its left end too, which is the END of a right-to-left line).
        // While editing, the caret is followed (`scroll_to_cursor`) as for any line.
        let room = f32::from_bits(key.room_bits);
        let rtl = crate::backend::text::paragraph_rtl(&render_text);
        let shift = if rtl && total_w < room { room - total_w } else { 0.0 };
        self.glyph_positions = run.stops.iter().map(|s| s.1 + shift).collect();
        self.glyph_shift = shift;
        self.glyph_run = Some(run);
        self.total_text_width = total_w;
        if rtl && total_w > room && !self.editing && key.wrap.is_none() && !self.multiline {
            self.scroll_x = total_w - room;
        }
    }

    pub fn char_width(&self) -> f32 {
        if self.shaped_char_advance > 0.0 {
            self.shaped_char_advance
        } else {
            crate::widget::display::measure_text_width("M", &self.font_family, self.font_size)
        }
    }

    pub fn line_height(&self) -> f32 {
        self.font_size * 1.333
    }

    /// The width a multiline box wraps its lines at, for a box `outer_w` wide: its width
    /// less the padding, or no limit with wrapping off.
    pub fn wrap_width(&self, outer_w: f32) -> f32 {
        if self.line_wrap_enabled() {
            (outer_w - 2.0 * self.pad()).max(1.0)
        } else {
            f32::INFINITY
        }
    }

    pub(super) fn advance_key(&self, text: &str, scale: f32) -> AdvanceKey {
        AdvanceKey {
            text: text.to_string(),
            font_size_bits: self.font_size.to_bits(),
            font: self.value_font(),
            attrs: self.font_attrs,
            scale_bits: scale.to_bits(),
        }
    }

    /// Each char's shaped advance in `text` (see the `advances` field), shaped with `fs` at
    /// `scale` — the buffer and the division by it take the one value.
    pub(super) fn measure_advances(&self, fs: &mut cosmic_text::FontSystem, text: &str, scale: f32) -> Vec<f32> {
        let font = self.value_font();
        let mut out = Vec::with_capacity(text.chars().count());
        for (pi, para) in text.split('\n').enumerate() {
            if pi > 0 {
                out.push(0.0); // the newline
            }
            let shown = if self.is_password { "•".repeat(para.chars().count()) } else { para.to_string() };
            let buf = crate::backend::text::shared_text_buffer_at(fs, &shown, self.font_size, font.as_deref(), self.font_attrs, scale);
            let run = crate::backend::text::shaped_run(&buf, &shown, scale);
            let mut adv = vec![0.0f32; shown.chars().count()];
            let char_at: std::collections::HashMap<usize, usize> =
                shown.char_indices().enumerate().map(|(ci, (b, _))| (b, ci)).collect();
            for c in &run.clusters {
                if let Some(&ci) = char_at.get(&c.start) {
                    adv[ci] += c.x1 - c.x0;
                }
            }
            out.extend(adv);
        }
        out
    }

    /// Measure `text`'s advances at `scale` with `fs` into the cache, unless it already
    /// holds them.
    pub(super) fn cache_advances(&self, fs: &mut cosmic_text::FontSystem, text: &str, scale: f32) {
        let key = self.advance_key(text, scale);
        if self.advances.borrow().as_ref().is_some_and(|(k, _)| *k == key) {
            return;
        }
        let adv = self.measure_advances(fs, text, scale);
        *self.advances.borrow_mut() = Some((key, adv));
    }

    /// `text`'s char advances: the cache, else measured with the shared geometry font
    /// system, else — that one held, by this thread further up the stack or by another —
    /// with a font system of this thread's own, from the same font set. Always measured,
    /// never a grid: two wraps of one text must agree.
    pub(super) fn char_advances(&self, text: &str) -> Vec<f32> {
        thread_local! {
            static OWN_FS: std::cell::RefCell<Option<cosmic_text::FontSystem>> = const { std::cell::RefCell::new(None) };
        }
        let scale = crate::scale::scale_factor();
        let key = self.advance_key(text, scale);
        if let Some((k, adv)) = self.advances.borrow().as_ref() {
            if *k == key {
                return adv.clone();
            }
        }
        let adv = match crate::geometry_font_system().try_lock() {
            Ok(mut fs) => self.measure_advances(&mut fs, text, scale),
            Err(_) => OWN_FS.with(|own| {
                let mut own = own.borrow_mut();
                let fs = own.get_or_insert_with(crate::create_font_system);
                self.measure_advances(fs, text, scale)
            }),
        };
        *self.advances.borrow_mut() = Some((key, adv.clone()));
        adv
    }

    /// The box's text wrapped at `max_width` px ([`TextBox::wrap_width`]): its lines, and
    /// where each char lands as (line, column). Words move whole to the next line; a word
    /// wider than the line breaks where it overflows. Widths are the shaped advances, so a
    /// proportional face and two-column CJK wrap where they are drawn (until 2026-10-08 it
    /// counted chars against one monospace advance).
    pub fn wrap_text(&self, max_width: f32) -> (Vec<String>, Vec<(usize, usize)>) {
        let text_src = if self.editing { &self.edit_buffer } else { &self.text };
        self.wrap_str(text_src, max_width)
    }

    /// [`TextBox::wrap_text`] for any text in the box's face (its placeholder).
    pub fn wrap_str(&self, text_src: &str, max_width: f32) -> (Vec<String>, Vec<(usize, usize)>) {
        let chars: Vec<char> = text_src.chars().collect();
        let mut lines = Vec::new();
        let mut current_line = Vec::new();
        let mut index_map = vec![(0, 0); chars.len() + 1];

        if !self.line_wrap_enabled() || !max_width.is_finite() {
            let mut i = 0;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\n' {
                    index_map[i] = (lines.len(), current_line.len());
                    lines.push(current_line.iter().collect::<String>());
                    current_line.clear();
                } else {
                    current_line.push(ch);
                    index_map[i] = (lines.len(), current_line.len() - 1);
                }
                i += 1;
            }
            index_map[chars.len()] = (lines.len(), current_line.len());
            lines.push(current_line.iter().collect::<String>());
            return (lines, index_map);
        }

        let adv = self.char_advances(text_src);
        let max_width = max_width.max(1.0);
        // The current line's width: the advances of the chars it holds.
        let mut line_w = 0.0f32;

        let mut i = 0;
        while i < chars.len() {
            let ch = chars[i];

            if ch == '\n' {
                index_map[i] = (lines.len(), current_line.len());
                lines.push(current_line.iter().collect::<String>());
                current_line.clear();
                line_w = 0.0;
                i += 1;
                continue;
            }

            current_line.push(ch);
            line_w += adv.get(i).copied().unwrap_or(0.0);
            index_map[i] = (lines.len(), current_line.len() - 1);

            // A space may hang past the edge, as it is drawn there; anything else that
            // overflows breaks the line after its last space, else before itself.
            if line_w > max_width + 0.5 && current_line.len() > 1 && !ch.is_whitespace() {
                let split = match current_line.iter().rposition(|c| c.is_whitespace()) {
                    Some(s_idx) => s_idx + 1,
                    None => current_line.len() - 1,
                };
                let line_to_push: Vec<char> = current_line[..split].to_vec();
                let remaining: Vec<char> = current_line[split..].to_vec();

                let line_idx = lines.len();
                lines.push(line_to_push.iter().collect::<String>());

                current_line = remaining;
                let start_orig = i + 1 - current_line.len();
                line_w = 0.0;
                for c_idx in 0..current_line.len() {
                    index_map[start_orig + c_idx] = (line_idx + 1, c_idx);
                    line_w += adv.get(start_orig + c_idx).copied().unwrap_or(0.0);
                }
            }
            i += 1;
        }

        index_map[chars.len()] = (lines.len(), current_line.len());
        lines.push(current_line.iter().collect::<String>());

        (lines, index_map)
    }

    pub fn map_2d_to_1d(&self, index_map: &[(usize, usize)], target_line: usize, target_col: usize, max_line_idx: usize) -> usize {
        let line = target_line.min(max_line_idx);
        let mut best_idx = 0;
        let mut best_dist = usize::MAX;

        for (i, &(l, c)) in index_map.iter().enumerate() {
            if l == line {
                let dist = (c as isize - target_col as isize).unsigned_abs();
                if dist < best_dist {
                    best_dist = dist;
                    best_idx = i;
                }
            }
        }
        best_idx
    }

    /// Map a press/drag position to a buffer index — the shared body of the legacy
    /// `mouse_input` press arm and `drag_update`.
    pub(super) fn position_to_idx(&self, px: f32, py: f32) -> usize {
        let pad = self.pad();
        let top = self.label_top();
        if self.multiline {
            let line_height = self.line_height();
            let max_w = self.wrap_width(self.rect.width);
            let (lines, index_map) = self.wrap_text(max_w);
            let click_line = (((py - (self.rect.y + top + pad) + self.scroll_y) / line_height).floor() as isize).max(0) as usize;
            let rel_x = px - (self.rect.x + pad) + self.scroll_x;
            let click_col = self.line_x_to_col(click_line.min(lines.len() - 1), rel_x);
            self.map_2d_to_1d(&index_map, click_line, click_col, lines.len() - 1)
        } else {
            self.map_x_to_idx(px)
        }
    }
}
