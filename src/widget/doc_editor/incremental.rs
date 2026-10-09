//! The incremental bookkeeping: which lines show raw (the caret's, widened to a fenced block or
//! the frontmatter), estimated and laid-out line heights, keeping layouts in step with the buffer's
//! changes, and the line at a y.

use super::*;

impl DocEditor {
    /// The lines shown raw: the caret's (or the selection's), widened to
    /// a fenced code block or the frontmatter the caret is in, so its
    /// fences (or its raw YAML) show.
    pub(super) fn active_range(&self) -> (usize, usize) {
        let (mut a, mut b) = match self.buf.selection() {
            Some((a, b)) => (a.line, b.line),
            None => (self.buf.caret.line, self.buf.caret.line),
        };
        // The frontmatter shows raw while the caret is anywhere in it, as
        // a fenced block does; elsewhere it is the Properties table.
        if let Some((x, _)) = preview::frontmatter_block(&self.ctx, a) {
            a = x;
        }
        if let Some((_, y)) = preview::frontmatter_block(&self.ctx, b) {
            b = y;
        }
        if let Some((x, _)) = preview::fenced_block(&self.ctx, a) {
            a = x;
        }
        if let Some((_, y)) = preview::fenced_block(&self.ctx, b) {
            b = y;
        }
        (a, b)
    }

    pub(super) fn is_active(&self, i: usize, act: (usize, usize)) -> bool {
        !self.preview || (i >= act.0 && i <= act.1)
    }

    pub(super) fn estimate(&self, i: usize) -> f32 {
        let row = (self.theme.size * self.theme.spacing).ceil();
        let chars = self.buf.line(i).chars().count() as f32;
        let per_row = (self.width / (self.theme.size * 0.5)).max(10.0);
        row * (chars / per_row).ceil().max(1.0)
    }

    /// Bring layouts, contexts and heights in line with the buffer.
    pub(super) fn sync(&mut self) {
        for c in self.buf.take_changes() {
            if c.removed == usize::MAX {
                self.layouts = vec![None; self.buf.line_count()];
                self.heights = vec![0.0; self.buf.line_count()];
                self.ctx.clear();
                continue;
            }
            let end = (c.first + c.removed).min(self.layouts.len());
            self.layouts.splice(c.first..end, std::iter::repeat_with(|| None).take(c.inserted));
            self.heights.splice(c.first..end, std::iter::repeat_n(0.0, c.inserted));
            self.tops_dirty = true;
        }
        let n = self.buf.line_count();
        self.layouts.resize_with(n, || None);
        self.heights.resize(n, 0.0);
        let ctx = preview::contexts(self.buf.lines());
        if ctx.len() == self.ctx.len() {
            for i in 0..n {
                if ctx[i] != self.ctx[i] {
                    self.layouts[i] = None;
                }
            }
        } else {
            self.layouts.fill(None);
        }
        self.ctx = ctx;
        let props = preview::properties(self.buf.lines(), &self.ctx);
        for (i, p) in props.iter().enumerate() {
            if self.props.get(i) != Some(p) {
                self.layouts[i] = None;
            }
        }
        self.props = props;
        let act = self.active_range();
        if act != self.shown_active {
            let (a, b) = self.shown_active;
            for i in [a, b, act.0, act.1] {
                if i < n {
                    self.layouts[i] = None;
                }
            }
            // A selection's middle lines too.
            for i in act.0.min(n)..=act.1.min(n.saturating_sub(1)) {
                self.layouts[i] = None;
            }
            if a != usize::MAX {
                for i in a.min(n)..=b.min(n.saturating_sub(1)) {
                    self.layouts[i] = None;
                }
            }
            self.shown_active = act;
        }
        for i in 0..n {
            let h = match &self.layouts[i] {
                Some(l) => l.height,
                None if self.heights[i] > 0.0 => self.heights[i],
                None => self.estimate(i),
            };
            if (h - self.heights[i]).abs() > 0.01 {
                self.heights[i] = h;
                self.tops_dirty = true;
            }
        }
        self.retop();
    }

    pub(super) fn retop(&mut self) {
        if !self.tops_dirty && self.tops.len() == self.heights.len() + 1 {
            return;
        }
        self.tops.clear();
        self.tops.reserve(self.heights.len() + 1);
        let mut y = 0.0;
        for h in &self.heights {
            self.tops.push(y);
            y += h;
        }
        self.tops.push(y);
        self.tops_dirty = false;
    }

    pub(super) fn ensure(&mut self, i: usize) {
        if i >= self.layouts.len() || self.layouts[i].is_some() {
            return;
        }
        let act = self.shown_active;
        let active = self.is_active(i, act);
        // The caret line with a composition in it, laid out as typed.
        let composed = self.composition.as_ref().filter(|(at, _)| at.line == i).map(|(at, p)| {
            let mut t = self.buf.line(i).to_string();
            t.insert_str(at.col.min(t.len()), &p.text);
            t
        });
        let text = composed.as_deref().unwrap_or(self.buf.line(i));
        let line = match self.props.get(i) {
            Some(Some(p)) if !active => {
                let key = match p {
                    Prop::Item { key_line: Some(k), .. } => preview::prop_key(self.buf.line(*k)),
                    _ => None,
                };
                preview::style_property(text, p, key)
            }
            _ => preview::style_line(text, self.ctx[i], active),
        };
        let mut l = layout::layout_line(text, &line, active, self.width, &self.theme, &mut self.measure);
        if self.preview && self.ctx[i] == Context::Normal {
            if let Some((target, want_w, want_h)) = preview::standalone_embed(text) {
                if let Some(img) = self.images.as_ref().and_then(|f| f(&target)) {
                    let (w, h) = img.fit(want_w, want_h, self.width);
                    l = if active { l.with_image_below(&target, w, h) } else { LineLayout::image(&target, w, h, l.text_size) };
                }
            } else if text.contains("![") {
                // Embeds inside the text: their pictures in a row below it.
                let width = self.width;
                let found: Vec<(String, f32, f32)> = preview::inline_embeds(text)
                    .into_iter()
                    .filter_map(|(target, want_w, want_h)| {
                        let img = self.images.as_ref().and_then(|f| f(&target))?;
                        let (w, h) = img.fit(want_w, want_h, width);
                        Some((target, w, h))
                    })
                    .collect();
                l = l.with_images_below(&found, width);
            }
        }
        if (l.height - self.heights[i]).abs() > 0.01 {
            self.heights[i] = l.height;
            self.tops_dirty = true;
        }
        self.layouts[i] = Some(l);
    }

    pub(super) fn line_at_y(&self, y: f32) -> usize {
        let n = self.heights.len();
        match self.tops[..n].binary_search_by(|t| t.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Less)) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => (i - 1).min(n - 1),
        }
    }

    pub fn content_height(&self) -> f32 {
        self.tops.last().copied().unwrap_or(0.0) + 2.0 * self.pad
    }

    pub(super) fn max_scroll(&self) -> f32 {
        (self.content_height() - self.viewport.height).max(0.0)
    }
}
