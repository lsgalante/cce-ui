//! A settings page's sections: where each stands on the page ([`PageFlow`]), and what
//! goes inside one ([`SectionContext`]).
//!
//! A page is a masonry of sections — the page split into as many columns as fit
//! `grid_min_col_width()`, at most the page's section count, each section dropped into the
//! shortest column. A section is drawn ONCE, in place: where it goes depends only on the
//! sections before it, never on its own height, so [`PageLayoutBuilder`] asks the flow for
//! the slot, lets the section draw its content there, and hands the height it came to back
//! to the flow. Until 2026-10-08 every section was drawn twice — once into a throwaway
//! target to measure it, then for real — through the legacy `LayoutStrategy`, whose
//! `allocate` took the height first; the flow's choices are the same, so the pages are too.
//!
//! What a section draws is immediate-mode: [`SectionContext`] keeps a cursor down its content
//! box and a one- or two-column grid for the widgets that do not span it. Its geometry is the
//! settings app's, and documented on each placer.

use crate::widget::WidgetHostExt;
use super::*;
use crate::widget::WidgetHost;
use crate::context::UiContext;

fn estimate_label_width_helper(label: &str, font_size: f32, font_fam: &str) -> f32 {
    let fam_lower = font_fam.to_lowercase();
    let is_mono = fam_lower.contains("mono") || fam_lower.contains("courier") || fam_lower == "monospace";
    if is_mono {
        label.chars().count() as f32 * font_size * 0.60
    } else {
        let mut width = 0.0;
        for c in label.chars() {
            let factor = match c {
                'i' | 'l' | 'I' | ' ' | '.' | ',' | '!' | ';' | ':' | '\'' | '"' | '(' | ')' | '[' | ']' | '-' => 0.30,
                'f' | 'j' | 't' => 0.35,
                'r' | 's' | 'c' | 'z' => 0.50,
                'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'k' | 'n' | 'o' | 'p' | 'q' | 'u' | 'v' | 'x' | 'y' => 0.60,
                'm' | 'w' | 'M' | 'W' | '&' | '@' | 'O' | 'Q' | 'G' => 0.85,
                'A' | 'B' | 'C' | 'D' | 'H' | 'N' | 'U' | 'V' | 'X' | 'Y' => 0.75,
                'E' | 'F' | 'K' | 'L' | 'P' | 'R' | 'S' | 'T' | 'Z' | 'J' => 0.68,
                '0'..='9' => 0.60,
                _ => 0.60,
            };
            width += factor * font_size;
        }
        width
    }
}

/// A section's column grid, and the flow's: the column lefts, their width, and how far down
/// each is filled.
#[derive(Debug, Clone)]
pub struct Grid {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub col_width: f32,
    pub gap: f32,
    pub col_heights: Vec<f32>,
    pub col_lefts: Vec<f32>,
}

impl Grid {
    pub fn new(left: f32, top: f32, width: f32, min_col_width: f32, gap: f32, count: usize) -> Self {
        let total_gap = gap * (count - 1) as f32;
        let col_width = if count > 0 {
            (width - total_gap).max(0.0) / count as f32
        } else {
            min_col_width
        };
        let left_offset = 0.0;

        let mut col_lefts = Vec::with_capacity(count);
        let col_heights = vec![top; count];
        for i in 0..count {
            col_lefts.push(left + left_offset + i as f32 * (col_width + gap));
        }

        Self {
            left,
            top,
            width,
            col_width,
            gap,
            col_heights,
            col_lefts,
        }
    }

    pub fn next_column(&self) -> usize {
        let mut min_idx = 0;
        let mut min_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] < min_h {
                min_h = self.col_heights[i];
                min_idx = i;
            }
        }
        min_idx
    }

    pub fn max_height(&self) -> f32 {
        let mut max_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] > max_h {
                max_h = self.col_heights[i];
            }
        }
        max_h
    }
}

/// Where a page's sections stand (see the module docs).
#[derive(Debug, Clone, Default)]
pub struct PageFlow {
    grid: Option<Grid>,
    sections: Option<usize>,
}

/// Where [`PageFlow::place`] put a section, for [`PageFlow::commit`] to fill once its height
/// is known.
#[derive(Debug, Clone, Copy)]
pub struct Slot {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    span: SlotSpan,
}

#[derive(Debug, Clone, Copy)]
enum SlotSpan {
    /// Every column: they all end below it.
    All,
    /// One column.
    One(usize),
    /// `n` columns from `first`.
    Run { first: usize, n: usize },
    /// The flow was never laid over a page.
    Nowhere,
}

impl PageFlow {
    pub fn new() -> Self {
        Self::default()
    }

    /// Lay the flow over the page's content rect. The column count is what fits
    /// `grid_min_col_width()` with `grid_gap()` between, capped by the section count.
    pub fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        let min_col_width = grid_min_col_width();
        let gap = grid_gap();
        let max_cols = ((width + gap) / (min_col_width + gap)).floor().max(1.0) as usize;
        let count = if let Some(n) = self.sections {
            n.min(max_cols).max(1)
        } else {
            max_cols
        };
        self.grid = Some(Grid::new(left, top, width, min_col_width, gap, count));
    }

    /// At most this many columns: a page with fewer sections than fit across does not leave
    /// columns empty. Takes effect at the next [`init`](Self::init).
    pub fn set_section_count(&mut self, count: usize) {
        self.sections = Some(count);
    }

    /// One column's width, once laid over a page.
    pub fn column_width(&self) -> Option<f32> {
        self.grid.as_ref().map(|g| g.col_width)
    }

    /// The gap between sections, both ways.
    pub fn gap(&self) -> f32 {
        grid_gap()
    }

    /// Where a section `width` wide goes: as many columns as it is wide (rounded), at the
    /// left of the run whose lowest fill is highest-up, just below that fill. A section as
    /// wide as the page goes below everything.
    pub fn place(&self, width: f32) -> Slot {
        let Some(grid) = self.grid.as_ref() else {
            return Slot { x: 0.0, y: 0.0, w: 0.0, span: SlotSpan::Nowhere };
        };
        let num_cols = grid.col_heights.len();
        let n = (((width + grid.gap) / (grid.col_width + grid.gap)).round() as usize).clamp(1, num_cols);
        if n >= num_cols {
            Slot { x: grid.left, y: grid.max_height(), w: grid.width, span: SlotSpan::All }
        } else if n == 1 {
            let col = grid.next_column();
            Slot { x: grid.col_lefts[col], y: grid.col_heights[col], w: grid.col_width, span: SlotSpan::One(col) }
        } else {
            let mut first = 0;
            let mut lowest = f32::MAX;
            for c in 0..=(num_cols - n) {
                let fill = grid.col_heights[c..c + n].iter().fold(0.0f32, |a, &h| a.max(h));
                if fill < lowest {
                    lowest = fill;
                    first = c;
                }
            }
            let w = n as f32 * grid.col_width + (n - 1) as f32 * grid.gap;
            Slot { x: grid.col_lefts[first], y: lowest, w, span: SlotSpan::Run { first, n } }
        }
    }

    /// The section placed at `slot` came to `height`: its columns now end a gap below it.
    pub fn commit(&mut self, slot: Slot, height: f32) {
        let Some(grid) = self.grid.as_mut() else { return };
        match slot.span {
            SlotSpan::All => grid.col_heights.fill(slot.y + height + grid.gap),
            SlotSpan::One(col) => grid.col_heights[col] += height + grid.gap,
            SlotSpan::Run { first, n } => {
                for h in &mut grid.col_heights[first..first + n] {
                    *h = slot.y + height + grid.gap;
                }
            }
            SlotSpan::Nowhere => {}
        }
    }
}

/// Vertical slack for a section's content clip. The clip exists to stop
/// content escaping its section SIDEWAYS, which is the axis a section's width
/// actually fixes; a section's height is only known once its content has been
/// placed, so bounding that axis too would risk cutting content off rather
/// than keeping it in. Deliberately far larger than any section.
const SECTION_CLIP_SLACK: f32 = 100_000.0;

/// A page's sections, placed by a [`PageFlow`] and drawn once each.
pub struct PageLayoutBuilder<'a, P> {
    pub flow: &'a mut PageFlow,
    pub cx: f32,
    pub cy: f32,
    pub cw: f32,
    pub ch: f32,
    pub section_width: f32,
    pub idx: usize,
    _phantom: std::marker::PhantomData<P>,
}

impl<'a, P: RenderTarget> PageLayoutBuilder<'a, P> {
    /// Lay `flow` over the page's content rect. `section_width` is a section's width when the
    /// flow has none to offer (it always does once laid over a page).
    pub fn new(flow: &'a mut PageFlow, cx: f32, cy: f32, cw: f32, ch: f32, section_width: f32) -> Self {
        flow.init(cx, cy, cw, ch);
        Self { flow, cx, cy, cw, ch, section_width, idx: 0, _phantom: std::marker::PhantomData }
    }

    /// At most `count` columns (see [`PageFlow::set_section_count`]).
    pub fn with_section_count(self, count: usize) -> Self {
        self.flow.set_section_count(count);
        self.flow.init(self.cx, self.cy, self.cw, self.ch);
        self
    }

    /// A section one column wide.
    pub fn add_section<F>(&mut self, final_pc: &mut P, label: &str, focused: bool, render_fn: F)
    where
        F: FnOnce(&mut SectionContext<'_, P>),
    {
        let width = self.flow.column_width().unwrap_or(self.section_width);
        self.add_section_with_width(final_pc, width, label, focused, render_fn);
    }

    /// A section `width` wide: as many columns as that rounds to.
    pub fn add_section_with_width<F>(&mut self, final_pc: &mut P, width: f32, label: &str, focused: bool, render_fn: F)
    where
        F: FnOnce(&mut SectionContext<'_, P>),
    {
        let slot = self.flow.place(width);
        let mut sec = SectionContext::new(final_pc, slot.x, slot.y, slot.w, label, focused, false);
        let (clip_x, clip_w) = (sec.content_left(), sec.content_width());
        sec.pc.push_clip_rect(clip_x, slot.y - SECTION_CLIP_SLACK, clip_w, 2.0 * SECTION_CLIP_SLACK);
        render_fn(&mut sec);
        sec.pc.pop_clip_rect();
        let bottom = sec.finish();
        self.flow.commit(slot, bottom - slot.y);
        self.idx += 1;
    }

    /// A section `span` columns wide.
    pub fn add_section_spanned<F>(&mut self, final_pc: &mut P, label: &str, span: usize, focused: bool, render_fn: F)
    where
        F: FnOnce(&mut SectionContext<'_, P>),
    {
        let col_width = self.flow.column_width().unwrap_or(self.section_width);
        let gap = self.flow.gap();
        let width = span as f32 * col_width + (span - 1) as f32 * gap;
        self.add_section_with_width(final_pc, width, label, focused, render_fn);
    }
}

pub struct SectionContext<'a, P> {
    pub pc: &'a mut P,
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub label_x: f32,
    /// Whether the host renders sections as sunken wells (`section_relief_style`).
    pub relief_style: bool,
    /// The title tab box (x, y, w, h) when the host's relief styling laid the
    /// label out left-aligned — `finish` offers it with the section carve.
    /// None under relief styling means a label-less section: the well carves
    /// tabless, flush with the allocation top.
    pub relief_tab: Option<(f32, f32, f32, f32)>,
    pub focused: bool,
    pub is_child: bool,
    pub grid: Grid,
    pub last_col: usize,
    pub content_start_y: f32,
    pub row_gap: f32,
}

impl<'a, P: RenderTarget> SectionContext<'a, P> {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        section_padding()
    }

    pub fn new(pc: &'a mut P, left: f32, top: f32, cw: f32, label: &str, focused: bool, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let relief_style = pc.section_relief_style();
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else if relief_style {
            // Sunken style: the title sits in a tab flush with the well's left
            // edge (the designer look), not centered on the border.
            left + 12.0
        } else {
            left + (cw - label_width) / 2.0
        };
        // Under relief styling the well fills the whole allocated rect (so the
        // page's gaps and margins are the visual gaps) — the tab tops the
        // allocation and the label centers inside it.
        let label_y = if relief_style { top + 4.0 } else { top };
        pc.text_with_font(label, label_x, label_y, font_size, font_color, &font_fam);

        // The tab wraps the label, flush on the well's top-left corner; clamped
        // so off-default child alignments can't push it outside the well.
        let relief_tab = if relief_style && label_width > 0.0 {
            let tab_x = (label_x - 12.0).max(left);
            Some((tab_x, top, label_width + 24.0, font_size + 10.0))
        } else {
            None
        };

        let pad = section_padding();
        let margin_x = 2.0 * pad + 12.0;
        let usable_w = (cw - 2.0 * margin_x).max(1.0);
        let min_col_width = 130.0;
        let gap = 8.0;
        let max_cols = if is_child {
            1
        } else {
            ((usable_w + gap) / (min_col_width + gap)).floor().clamp(1.0, 2.0) as usize
        };
        // Under relief styling the content stands off the well's top wall by
        // the same inset it keeps from the side walls (`margin_x`, which is
        // also what `finish` leaves below it), so a well reads as one even
        // frame. The outline style's `pad + 19` predates the title tab: under
        // relief the tab alone is `font_size + 10` tall, which left the first
        // line touching the well's top wall at the default padding while the
        // sides kept 20px and more.
        let content_start_y = if relief_style {
            relief_tab.map(|t| t.1 + t.3).unwrap_or(top) + margin_x
        } else {
            top + pad + 19.0
        };
        let grid = Grid::new(left + margin_x, content_start_y, usable_w, min_col_width, gap, max_cols);

        Self {
            pc,
            left,
            top,
            content_y: content_start_y,
            cw,
            label_width,
            label_x,
            relief_style,
            relief_tab,
            focused,
            is_child,
            grid,
            last_col: usize::MAX,
            content_start_y,
            row_gap: Self::DEFAULT_ROW_GAP,
        }
    }

    pub fn with_row_gap(mut self, gap: f32) -> Self {
        self.row_gap = gap;
        self
    }

    /// The section's content-box top edge: the body well's top (the tab's
    /// bottom) under relief styling, the outline's border line otherwise. Lets
    /// a page place content at an exact inset from the well's walls.
    pub fn well_top(&self) -> f32 {
        if self.relief_style {
            self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top)
        } else {
            self.top + 7.0
        }
    }

    pub fn set_row_gap(&mut self, gap: f32) {
        self.row_gap = gap;
    }

    /// Horizontal inset of section CONTENT from the section's left edge.
    ///
    /// One number, used by every content placer in here — `row_layout`, the
    /// column `Grid`, `widget`, `VStack` and `ax` (so `text`) — because they
    /// share a section and have to line up inside it. The section's border is
    /// drawn at `left + padding()` (see `finish`), so content clears the
    /// border by `padding() + 12`.
    pub fn content_margin(&self) -> f32 {
        2.0 * self.padding() + 12.0
    }

    /// Left edge of the content box: where a row, a widget or a `text(_, 12.0,
    /// ..)` starts.
    pub fn content_left(&self) -> f32 {
        self.left + self.content_margin()
    }

    /// Width of the content box — the section's width less the inset on both
    /// sides. Nothing a section draws should extend past `content_left() +
    /// content_width()`.
    pub fn content_width(&self) -> f32 {
        (self.cw - 2.0 * self.content_margin()).max(0.0)
    }

    /// `x_off` px into the content box's coordinate space, where 12.0 is the
    /// content's own left edge — the offset 46 of the ~55 call sites in the
    /// tree already pass, and the one that lines text up with the buttons and
    /// widgets beside it.
    ///
    /// This used to add `padding()` only when `x_off >= 12.0`, which made the
    /// mapping DISCONTINUOUS: asking for 11 instead of 12 moved the text 5px
    /// LEFT rather than 1px, and silently dropped it out of alignment with
    /// every row in the same section. `cce-mail` and two others sit on the
    /// wrong side of that cliff today.
    pub fn ax(&self, x_off: f32) -> f32 {
        self.left + 2.0 * self.padding() + x_off
    }

    pub fn ay(&self) -> f32 {
        self.content_y
    }

    pub fn spacing(&mut self, dy: f32) {
        if self.grid.col_heights.len() >= 2 {
            if self.last_col == usize::MAX {
                for h in &mut self.grid.col_heights {
                    *h += dy;
                }
            } else if self.last_col < self.grid.col_heights.len() {
                self.grid.col_heights[self.last_col] += dy;
            }
            self.content_y = self.grid.max_height();
        } else {
            self.content_y += dy;
            for h in &mut self.grid.col_heights {
                *h += dy;
            }
        }
    }

    pub fn text(&mut self, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let mut y = self.content_y + y_off;
        if y > self.content_start_y {
            y += self.row_gap;
        }
        let x = self.ax(x_off);
        // Bound it to the content box. A section's text was drawn unbounded,
        // so a string wider than its section simply kept going — over the
        // border, over whatever sat to the right, and off the window (the
        // settings app's GPU names did all three). Rows and widgets have
        // always been sized to the section; text was the one thing that could
        // leave it. The vertical band is generous on purpose: it is the
        // horizontal overrun that has to be cut, and a tight band would
        // shave descenders.
        let right = self.content_left() + self.content_width();
        let bounds = if right > x {
            Some([x, y - font_size, right, y + 2.0 * font_size])
        } else {
            None
        };
        self.pc.text_with_bounds(text, x, y, font_size, color, bounds);
        let new_bottom = y + font_size + 4.0;
        self.content_y = new_bottom;
        self.grid.col_heights.fill(new_bottom);
    }

    pub fn widget<T: WidgetHost + 'static>(&mut self, w: &mut T, _x_off: f32, _ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        let top_room = w.label_strip();
        let total_h = wh + top_room;

        let name = w.type_name();
        let span_full = name == "Trackpad"
            || name == "Canvas"
            || name == "UsageBar"
            || name == "ProgressBar"
            || name == "ButtonStrip"
            || name == "Spreadsheet"
            || name == "Graph";

        if span_full {
            let margin_x = 2.0 * pad + 12.0;
            let x = self.left + margin_x;
            let clamped_w = (self.cw - 2.0 * margin_x).max(0.0);
            let mut max_h = self.grid.max_height().max(self.content_y);
            if max_h > self.content_start_y {
                max_h += self.row_gap;
            }
            let y = max_h;

            w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
            render_widget(self.pc, w, x, y, clamped_w, total_h, ctx);

            let new_bottom = y + total_h;
            self.content_y = new_bottom;
            self.grid.col_heights.fill(new_bottom);
        } else {
            let max_h = self.grid.max_height();
            if self.content_y > max_h {
                self.grid.col_heights.fill(self.content_y);
            }

            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let mut y = self.grid.col_heights[col];
            if y > self.content_start_y {
                y += self.row_gap;
            }

            let aligned_x = x;
            let aligned_w = self.grid.col_width;

            w.set_row_rect(aligned_x, aligned_w);
            render_widget(self.pc, w, aligned_x, y, aligned_w, total_h, ctx);
            self.grid.col_heights[col] = y + total_h;
            self.content_y = self.grid.max_height();
        }
    }

    pub fn widget_full<T: WidgetHost + 'static>(&mut self, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off); // cw - 40.0
        self.widget(w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self) {
        let pad = self.padding();
        let x = self.ax(pad);
        let max_h = self.grid.max_height().max(self.content_y);
        let y = max_h;
        self.pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y = max_h + 8.0;
        self.grid.col_heights.fill(self.content_y);
    }

    pub fn rect(&mut self, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        let max_h = self.grid.max_height().max(self.content_y);
        self.pc.rect(color, self.ax(x_off), max_h, w, h);
        self.content_y = max_h + h;
        self.grid.col_heights.fill(self.content_y);
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = self.content_margin();
        let usable_w = self.content_width();
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    /// A row whose columns are sized to what goes IN them: each gets the width
    /// it asked for in `needs`, and whatever is left over is shared equally.
    ///
    /// `row_layout` splits a row evenly and knows nothing about content, so it
    /// hands "Reboot" and "Hibernate" the same width — one floats in slack
    /// while the other is cut off, which is what a row of mismatched labels
    /// looks like. Sharing the SLACK equally rather than sizing proportionally
    /// is deliberate: proportional widths would make a two-character label a
    /// sliver, where what is wanted is "everyone fits, then everyone gets the
    /// same bonus".
    ///
    /// When the needs do not fit, every column is scaled by the same factor, so
    /// the row still cannot overflow its section and the shortfall is shared
    /// rather than landing entirely on the last column.
    pub fn row_layout_for(&self, needs: &[f32], gap: f32) -> Vec<(f32, f32)> {
        let count = needs.len();
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let room = (self.content_width() - total_gap).max(0.0);
        let total_need: f32 = needs.iter().map(|n| n.max(0.0)).sum();

        let widths: Vec<f32> = if total_need <= room {
            let extra = (room - total_need) / count as f32;
            needs.iter().map(|n| n.max(0.0) + extra).collect()
        } else if total_need > 0.0 {
            let scale = room / total_need;
            needs.iter().map(|n| n.max(0.0) * scale).collect()
        } else {
            vec![room / count as f32; count]
        };

        let mut cols = Vec::with_capacity(count);
        let mut x = self.content_left();
        for w in widths {
            cols.push((x, w));
            x += w + gap;
        }
        cols
    }


    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let max_h = self.grid.max_height().max(self.content_y);
        self.grid.col_heights.fill(max_h);
        self.content_y = max_h;

        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;

        self.grid.col_heights.fill(self.content_y);
    }

    pub fn vstack(&mut self, spacing: f32) -> VStack<'_, 'a, P> {
        VStack {
            context: self,
            spacing,
        }
    }

    pub fn add_section<F>(&mut self, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let pad = self.padding();
        let (left, top, cw, is_side_by_side) = if self.grid.col_heights.len() >= 2 {
            let col = self.grid.next_column();
            self.last_col = col;
            let x = self.grid.col_lefts[col];
            let mut y = self.grid.col_heights[col];
            if y > self.content_start_y {
                y += self.row_gap;
            }
            (x, y, self.grid.col_width, true)
        } else {
            let left = self.ax(0.0) + pad;
            let mut max_h = self.grid.max_height().max(self.content_y);
            if max_h > self.content_start_y {
                max_h += self.row_gap;
            }
            let top = max_h;
            let cw = self.cw - 2.0 * pad;
            (left, top, cw, false)
        };

        let mut sub_ctx = SectionContext::new(self.pc, left, top, cw, label, focused, true);
        render_fn(&mut sub_ctx);
        let new_bottom = sub_ctx.finish();

        if is_side_by_side {
            let col = self.last_col;
            self.grid.col_heights[col] = new_bottom;
            self.content_y = self.grid.max_height();
        } else {
            self.content_y = new_bottom;
            self.grid.col_heights.fill(new_bottom);
        }
    }

    pub fn finish(self) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if self.focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if self.focused {
                [0.30, 0.50, 0.32, 1.0] // Focused green
            } else {
                [0.25, 0.25, 0.35, 1.0] // Default gray
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;
        // Under relief the well's walls are the allocation's edges, so the
        // floor below the content matches the inset beside and above it (see
        // `new`). The outline frame sits `pad` in from its allocation and
        // keeps its own slack.
        let extra_bottom = if self.relief_style { self.content_margin() } else { pad + 12.0 };
        let bottom = y + h + extra_bottom;

        if self.relief_style {
            // Sunken style: the well spans the full allocated rect — body top
            // edge at the tab's bottom (the tab is flush ON the body, the
            // designer union shape; label-less sections carve tabless from the
            // allocation top), walls on the allocation's edges, and no
            // trailing slack so the layout gap IS the visual gap.
            let body_y = self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top);
            let frame = SectionFrame {
                x: self.left,
                y: body_y,
                w: self.cw,
                h: bottom - body_y,
                tab: self.relief_tab,
                focused: self.focused,
                is_child: self.is_child,
            };
            if self.pc.section_relief(&frame) {
                return self.content_y + extra_bottom;
            }
        }

        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = self.label_x;
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                self.pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                self.pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            self.pc.rect(border, left_edge, y, w, 1.0);
        }

        self.pc.rect(border, x, y + h + extra_bottom, w, 1.0);
        self.pc.rect(border, x, y, 1.0, h + extra_bottom);
        self.pc.rect(border, x + w - 1.0, y, 1.0, h + extra_bottom);
        self.content_y + extra_bottom + 8.0
    }
}


pub struct VStack<'b, 'a, P> {
    pub context: &'b mut SectionContext<'a, P>,
    pub spacing: f32,
}

impl<'b, 'a, P: RenderTarget> VStack<'b, 'a, P> {
    pub fn add_widget<T: WidgetHost + 'static>(&mut self, w: &mut T, _ww: f32, wh: f32, ctx: &mut UiContext) {
        let pad = self.context.padding();
        let margin_x = 2.0 * pad + 12.0;
        let x = self.context.left + margin_x;
        let clamped_w = (self.context.cw - 2.0 * margin_x).max(0.0);

        let mut max_h = self.context.grid.max_height().max(self.context.content_y);
        if max_h > self.context.content_start_y {
            max_h += self.context.row_gap;
        }
        let y = max_h;

        let pref_h = w.preferred_height().unwrap_or(wh);
        let top_room = w.label_strip();
        let total_h = pref_h + top_room;

        w.set_row_rect(self.context.left + pad, self.context.cw - 2.0 * pad);
        render_widget(self.context.pc, w, x, y, clamped_w, total_h, ctx);

        let new_bottom = y + total_h;
        self.context.content_y = new_bottom;
        self.context.grid.col_heights.fill(new_bottom);
        self.context.spacing(self.spacing);
    }

    /// [`add_row`](Self::add_row) with per-column widths from `needs` — see
    /// [`SectionContext::row_layout_for`]. For a row of buttons, `needs` is
    /// each label's measured width plus the plate's own inset.
    pub fn add_row_for<F>(&mut self, needs: &[f32], gap: f32, h: f32, mut f: F)
    where
        F: FnMut(&mut SectionContext<'a, P>, usize, f32, f32),
    {
        let max_h = self.context.grid.max_height().max(self.context.content_y);
        self.context.grid.col_heights.fill(max_h);
        self.context.content_y = max_h;

        let cols = self.context.row_layout_for(needs, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            self.context.content_y = max_h;
            f(self.context, i, x, w);
        }

        let new_bottom = max_h + h;
        self.context.content_y = new_bottom;
        self.context.grid.col_heights.fill(new_bottom);
        self.context.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(&mut SectionContext<'a, P>, usize, f32, f32),
    {
        let max_h = self.context.grid.max_height().max(self.context.content_y);
        self.context.grid.col_heights.fill(max_h);
        self.context.content_y = max_h;

        let cols = self.context.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            self.context.content_y = max_h;
            f(self.context, i, x, w);
        }

        let new_bottom = max_h + h;
        self.context.content_y = new_bottom;
        self.context.grid.col_heights.fill(new_bottom);
        self.context.spacing(self.spacing);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a section goes depends on the sections before it and never on its own height:
    /// one column wide it takes the shortest column, as wide as the page it goes below
    /// everything, and a run takes the columns whose lowest fill is highest up.
    #[test]
    fn a_section_is_placed_before_it_is_drawn() {
        let (min, gap) = (grid_min_col_width(), grid_gap());
        let width = 3.0 * min + 2.0 * gap + 1.0;
        let mut flow = PageFlow::new();
        flow.init(10.0, 20.0, width, 800.0);
        let col = flow.column_width().unwrap();

        let a = flow.place(col);
        flow.commit(a, 100.0);
        let b = flow.place(col);
        flow.commit(b, 50.0);
        let c = flow.place(col);
        flow.commit(c, 70.0);
        assert_eq!((a.x, a.y), (10.0, 20.0));
        assert_eq!((b.y, c.y), (20.0, 20.0), "the first three stand side by side");
        assert!(a.x < b.x && b.x < c.x);

        let d = flow.place(col);
        assert_eq!((d.x, d.y), (b.x, 20.0 + 50.0 + gap), "under the shortest column");
        flow.commit(d, 10.0);

        // Columns 1 and 2 reach 20+50+gap+10+gap and 20+70+gap; 0 and 1 reach further.
        let pair = flow.place(2.0 * col + gap);
        assert_eq!((pair.x, pair.y), (b.x, (20.0 + 50.0 + gap + 10.0 + gap).max(20.0 + 70.0 + gap)));
        assert_eq!(pair.w, 2.0 * col + gap);
        flow.commit(pair, 30.0);

        let wide = flow.place(width);
        assert_eq!((wide.x, wide.w), (10.0, width));
        assert_eq!(wide.y, pair.y + 30.0 + gap, "below everything");
    }

    /// A page with fewer sections than fit across leaves no column empty.
    #[test]
    fn the_section_count_caps_the_columns() {
        let width = 3.0 * grid_min_col_width() + 2.0 * grid_gap() + 1.0;
        let mut flow = PageFlow::new();
        flow.set_section_count(1);
        flow.init(0.0, 0.0, width, 400.0);
        assert_eq!(flow.column_width(), Some(width));
    }

    /// The builder draws each section once, in place, and the next one starts below it.
    #[test]
    fn each_section_is_drawn_once() {
        let mut flow = PageFlow::new();
        let mut pc = PopoverCollector::new();
        let mut builder = PageLayoutBuilder::new(&mut flow, 0.0, 0.0, 600.0, 400.0, 300.0).with_section_count(1);
        let mut tops = Vec::new();
        for _ in 0..2 {
            builder.add_section(&mut pc, "Section", false, |sec| {
                tops.push(sec.top);
                sec.spacing(40.0);
            });
        }
        assert_eq!(tops.len(), 2, "one call per section");
        assert!(tops[1] > tops[0] + 40.0, "{tops:?}");
    }
}
