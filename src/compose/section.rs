//! A settings page's sections: where each stands on the page ([`PageFlow`]), and what
//! goes inside one ([`SectionContext`]).
//!
//! A page is a masonry of sections — the page split into as many columns as fit
//! `grid_min_col_width()`, at most the page's section count, each section dropped into the
//! shortest column. A section is drawn ONCE, in place: where it goes depends only on the
//! sections before it, never on its own height, so [`PageLayoutBuilder`] asks the flow for
//! the slot, lets the section draw its content there, and hands the height it came to back
//! to the flow.
//!
//! What goes inside a section is a [`Form`](super::Form): a box-model tree the page declares,
//! which the section lays out across its content box with `scene::layout` and paints
//! ([`SectionContext::form`], [`SectionContext::place`]).

use crate::context::UiContext;
use crate::layout::*;
use crate::scene::paint::{RenderTarget, SectionFrame};

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
    /// How far down the section's content reaches.
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
    /// Where the content box starts.
    pub content_start_y: f32,
}

impl<'a, P: RenderTarget> SectionContext<'a, P> {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;

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
            content_start_y,
        }
    }

    /// The section's content-box top edge: the body well's top (the tab's
    /// bottom) under relief styling, the outline's border line otherwise.
    pub fn well_top(&self) -> f32 {
        if self.relief_style {
            self.relief_tab.map(|t| t.1 + t.3).unwrap_or(self.top)
        } else {
            self.top + 7.0
        }
    }

    /// Horizontal inset of section CONTENT from the section's left edge. The section's border
    /// is drawn at `left + padding()` (see `finish`), so content clears the border by
    /// `padding() + 12`.
    pub fn content_margin(&self) -> f32 {
        2.0 * self.padding() + 12.0
    }

    /// Left edge of the content box.
    pub fn content_left(&self) -> f32 {
        self.left + self.content_margin()
    }

    /// Width of the content box — the section's width less the inset on both sides.
    pub fn content_width(&self) -> f32 {
        (self.cw - 2.0 * self.content_margin()).max(0.0)
    }

    /// A [`Form`](super::Form) across this section's content box, starting below whatever the
    /// section already holds (the control gap after it). Declare the contents into it, then
    /// hand it to [`place`](Self::place).
    pub fn form<'w>(&self) -> super::Form<'w, P>
    where
        P: 'w,
    {
        let mut y = self.content_y;
        if y > self.content_start_y {
            y += crate::layout::control_gap();
        }
        let pad = self.padding();
        super::Form::new(self.content_left(), y, self.content_width(), (self.left + pad, self.cw - 2.0 * pad))
    }

    /// Lay a [`Form`](super::Form) out, paint it, and move the section past it.
    pub fn place<'w>(&mut self, form: super::Form<'w, P>, ctx: &mut UiContext)
    where
        P: 'w,
    {
        let bottom = form.paint(&mut *self.pc, ctx);
        self.content_y = bottom;
    }

    /// How far below the last of its content the section's frame ends — what a page leaves
    /// under a list that fills the rest of the page.
    pub fn bottom_inset(&self) -> f32 {
        if self.relief_style { self.content_margin() } else { self.padding() + 12.0 + 8.0 }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::paint::PopoverCollector;

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
                let mut form = sec.form();
                form.column().draw(0.0, 40.0, false, |_, _, _| {});
                sec.place(form, &mut UiContext::new());
            });
        }
        assert_eq!(tops.len(), 2, "one call per section");
        assert!(tops[1] > tops[0] + 40.0, "{tops:?}");
    }
}
