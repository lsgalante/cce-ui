//! `Spreadsheet`: a read-only table with a header row, zebra rows, selectable rows (a press
//! selects one, ctrl toggles one, shift extends from the last pressed — see
//! [`Spreadsheet::press_row`]), column dividers, and an inertially scrolled body: wheel input
//! feeds a velocity that [`Input::tick`] integrates and decays each frame, the scrollbar thumb is
//! host-drag-driven through the drag surface, and arrow/page/home/end keys jump the scroll. The
//! scroll geometry (content and viewport heights, thumb position) is derived in one place
//! ([`ScrollGeom`]). [`SpreadsheetController`] rides the `Input` capability hooks.
//!
//! **The two scrollbars cross at the body's centre**: the vertical bar rides the pane's vertical
//! centre line and the horizontal one the body's horizontal centre line, as the params pane's and
//! the designer dialog's bars ride theirs — over the cells, reserving no lane. They idle BEHIND
//! the pane's plate ([`ScrollbarActivity`]): a scroll, a key or a thumb drag raises them in
//! front, a pointer over a raised bar keeps it there, and once nothing holds them for the hold
//! window they sink again. Sunk they take no input, so a press on their lane reaches the row
//! beneath. The fore copy is painted by [`Paint::paint`] at the activity's fade; the copy behind
//! the plate is the HOST's to draw, before the plate, through [`Spreadsheet::paint_scrollbars`] —
//! the plate is the host's too.
//!
//! **The table is COLUMNS of values, formatted as they are painted** ([`SheetColumn`],
//! [`SpreadsheetController::set_spreadsheet_columns`]): a column of numbers is the numbers, a
//! cell is written only when it is drawn, and a sort compares the values rather than parsing
//! text. That is what lets a host refill the table every frame (the designer's, during a
//! simulation's playback) at the cost of a copy of its values. `set_spreadsheet_data` (rows of
//! strings) still works: its cells become text columns.
//!
//! The `PARAM_BG` background is NOT emitted here: the host draws every widget's background
//! itself from `color()` + `corner_style()` (`append_widget_plate`), and `PARAM_BG` is
//! translucent — emitting it again would double-blend. This widget's own geometry starts at the
//! header strip.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, its constants and scroll geometries, construction, column widths, taking a table, `SpreadsheetController` |
//! | `column` | [`SheetColumn`]: a column's values, its cells, its widest cell, comparing rows |
//! | `scroll` | the vertical and horizontal scroll geometry, the scrollbars (hit, raise, paint), thumb drags |
//! | `rows` | the header and body hit tests, sorting, row selection |
//! | `paint` | `impl Paint` |
//! | `input` | `impl Input`: events, keys, the wheel, drags, the tick |

mod column;
mod input;
mod paint;
mod rows;
mod scroll;
#[cfg(test)]
mod tests;

pub use column::SheetColumn;

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::scroll_motion::{scroll_settings, Bounds, ScrollMotion};
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Key, Layout, MouseButton, MouseScrollDelta,
    NamedKey, Paint, ScrollbarActivity, SpreadsheetController,
};

const HEADER_H: f32 = 24.0;

const ROW_H: f32 = 24.0;

/// How far a track stops short of each end of what it spans, as every toolkit bar's does.
const TRACK_INSET: f32 = 4.0;

/// How far either side of a bar a press or the pointer still counts as on it.
const BAR_SLOP: f32 = 4.0;

/// A column's width beyond its text: the 8 px a label stands in from the
/// column's left edge, and the 4 px gutter it is clamped short of the divider
/// by, with room to spare.
const CELL_PAD: f32 = 16.0;

/// Room after a header's name for the sort glyph, in characters — kept
/// whether or not the column is sorted, so sorting moves no column.
const SORT_MARK_CHARS: usize = 2;

/// The narrowest a column is, in characters, so a header is a target.
const MIN_COL_CHARS: usize = 3;

pub struct Spreadsheet {
    hovered: bool,
    headers: Vec<String>,
    /// The table, a column a header ([`SheetColumn`]); `row_count` rows.
    columns: Vec<SheetColumn>,
    row_count: usize,
    /// Display order into the rows — the identity permutation unless `sort` is set.
    /// Rebuilt by `apply_sort` at both mutation sites (header click, data refresh),
    /// so `order.len() == row_count` always holds.
    order: Vec<usize>,
    /// Active sort: `(column, ascending)`. A header click cycles
    /// ascending → descending → natural order (matching the source data).
    sort: Option<(usize, bool)>,
    /// Header cell the pointer is over — hover tint, like the Processes page's
    /// sortable headers.
    header_hover_col: Option<usize>,
    /// Each column's width in characters: its header with room for the
    /// sort glyph, or its widest cell, whichever is wider (`set_columns`).
    col_chars: Vec<usize>,
    scroll_y: f32,
    dragging_scrollbar: bool,
    drag_offset_y: f32,
    scroll_x: f32,
    /// Smooth-scroll driver behind `scroll_y`/`scroll_x`: wheel notches
    /// glide, trackpad flicks coast (the widget's former velocity model,
    /// now the toolkit-wide one).
    motion: ScrollMotion,
    dragging_hscrollbar: bool,
    drag_offset_x: f32,
    /// Whether the two bars are raised in front of the plate or sunk behind
    /// it — one latch for both, since they are one cross.
    activity: ScrollbarActivity,
    /// The selected rows, as indices into `rows` — the SOURCE order, so a
    /// sort moves a selected row's place in the pane and not what it names.
    selected: std::collections::BTreeSet<usize>,
    /// The row a shift press extends from: the last one pressed without it.
    anchor: Option<usize>,
    /// Set by whatever changed `selected`, taken by the host.
    selection_changed: bool,
    /// The modifiers the host pushed in ahead of the press.
    ctrl: bool,
    shift: bool,
}

/// Scroll/scrollbar geometry for one (row count, rect) pair. Present only when the content
/// overflows the viewport — every scroll behavior is gated on that, as in legacy.
struct ScrollGeom {
    visible_h: f32,
    max_scroll: f32,
    /// `scroll_y` clamped to the current bounds (data changes can leave the raw value stale).
    scroll: f32,
    /// The bar's left edge: it is centred on the pane's width.
    bar_x: f32,
    bar_w: f32,
    track_y: f32,
    track_h: f32,
    thumb_h: f32,
    thumb_y: f32,
    track_range: f32,
}

/// [`ScrollGeom`]'s horizontal twin, for the bottom scrollbar. Present only
/// when the column run overflows the pane width.
struct HScrollGeom {
    max_scroll: f32,
    /// `scroll_x` clamped to the current bounds.
    scroll: f32,
    track_x: f32,
    track_w: f32,
    /// The bar's top edge: it is centred on the body's height.
    bar_y: f32,
    bar_h: f32,
    thumb_w: f32,
    thumb_x: f32,
    track_range: f32,
}

impl Spreadsheet {
    pub fn new() -> Adapted<Spreadsheet> {
        let mut s = Adapted::new(Spreadsheet {
            hovered: false,
            headers: Vec::new(),
            columns: Vec::new(),
            row_count: 0,
            order: Vec::new(),
            sort: None,
            header_hover_col: None,
            col_chars: Vec::new(),
            scroll_y: 0.0,
            dragging_scrollbar: false,
            drag_offset_y: 0.0,
            scroll_x: 0.0,
            motion: ScrollMotion::new(),
            dragging_hscrollbar: false,
            drag_offset_x: 0.0,
            activity: ScrollbarActivity::new(),
            selected: std::collections::BTreeSet::new(),
            anchor: None,
            selection_changed: false,
            ctrl: false,
            shift: false,
        });
        // The spreadsheet pane starts hidden (the designer toggles it in later).
        crate::widget::WidgetHost::set_visible(&mut s, false);
        s
    }

    /// One character's width in the label font, which is monospace.
    fn char_w() -> f32 {
        let fam = crate::layout::control_label_font();
        (crate::widget::display::measure_text_width("0123456789", &fam, 12.0) / 10.0).max(1.0)
    }

    /// The columns' edges from the table's left, `n + 1` of them: each
    /// column is as wide as its content (`col_chars`) — its header and sort
    /// glyph, or its widest cell — and no wider. The table does not stretch
    /// to the pane: what is left of a wide pane is left empty, and a table
    /// wider than the pane scrolls. (Until 2026-10-07 every column was an
    /// even share of the pane, floored at 76 px, so a point index or a
    /// group's 0 and 1 took as much room as a four-decimal float.)
    fn col_edges(&self) -> Vec<f32> {
        let cw = Self::char_w();
        let mut edges = Vec::with_capacity(self.headers.len() + 1);
        let mut x = 0.0;
        edges.push(x);
        for i in 0..self.headers.len() {
            let chars = self.col_chars.get(i).copied().unwrap_or(0).max(MIN_COL_CHARS);
            x += chars as f32 * cw + CELL_PAD;
            edges.push(x);
        }
        edges
    }

    /// How wide the table is: its columns, each as wide as its content. A
    /// host sizes the pane by it — the designer's plate is as wide as the
    /// table, up to the room it has (since 2026-10-07).
    pub fn content_width(&self) -> f32 {
        self.col_edges().last().copied().unwrap_or(0.0)
    }
}

impl Layout for Spreadsheet {}

impl Spreadsheet {
    /// Take a table: its headers, its columns and how many rows they hold.
    fn set_columns(&mut self, headers: Vec<String>, columns: Vec<SheetColumn>, rows: usize) {
        // Each column's width, from its header and its values: a refill
        // is a scan of the values it already copies.
        // The columns' widths are independent, so a large table works
        // them out a share of columns a thread (since 2026-10-07: at a
        // million values this pass was most of what a refill cost).
        let cells: Vec<usize> = {
            let values: usize = columns.iter().map(SheetColumn::len).sum();
            let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(columns.len());
            if values < 200_000 || threads < 2 {
                columns.iter().map(SheetColumn::max_chars).collect()
            } else {
                let per = columns.len().div_ceil(threads);
                std::thread::scope(|scope| {
                    let pieces: Vec<_> = columns
                        .chunks(per)
                        .map(|share| scope.spawn(move || share.iter().map(SheetColumn::max_chars).collect::<Vec<_>>()))
                        .collect();
                    pieces.into_iter().flat_map(|p| p.join().expect("a column's width")).collect()
                })
            }
        };
        self.col_chars = headers
            .iter()
            .enumerate()
            .map(|(i, h)| (h.chars().count() + SORT_MARK_CHARS).max(cells.get(i).copied().unwrap_or(0)))
            .collect();
        self.headers = headers;
        self.columns = columns;
        self.row_count = rows;
        // Re-derive the display order so an active sort survives a data refresh
        // (the designer re-sets the whole table on selection/param changes).
        self.apply_sort();
        // The raw scroll may now exceed the new content; every consumer clamps through
        // `geom()`, and the next scroll write re-clamps it for real.
        //
        // The selection is of rows by index, and stands across a refresh:
        // the designer re-sets the table on every frame of a playback, and
        // the rows selected are still the elements they were. What a
        // shorter table no longer has goes.
        let n = self.row_count;
        let kept = self.selected.len();
        self.selected.retain(|&r| r < n);
        if self.selected.len() != kept {
            self.selection_changed = true;
        }
        if self.anchor.is_some_and(|a| a >= n) {
            self.anchor = None;
        }
    }
}

impl SpreadsheetController for Spreadsheet {
    fn set_spreadsheet_data(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        // Rows of text become text columns, a cell a row; a short row's
        // missing cells are empty.
        let width = headers.len();
        let mut columns: Vec<Vec<String>> = (0..width).map(|_| Vec::with_capacity(rows.len())).collect();
        for row in &rows {
            for (c, column) in columns.iter_mut().enumerate() {
                column.push(row.get(c).cloned().unwrap_or_default());
            }
        }
        self.set_columns(headers, columns.into_iter().map(SheetColumn::Text).collect(), rows.len());
    }

    fn set_spreadsheet_columns(&mut self, headers: Vec<String>, columns: Vec<SheetColumn>) {
        let rows = columns.iter().map(SheetColumn::len).max().unwrap_or(0);
        self.set_columns(headers, columns, rows);
    }

    fn selected_rows(&self) -> Vec<usize> {
        self.selected.iter().copied().collect()
    }

    fn set_selected_rows(&mut self, rows: &[usize]) {
        let n = self.row_count;
        let next: std::collections::BTreeSet<usize> = rows.iter().copied().filter(|&r| r < n).collect();
        if next != self.selected {
            self.selected = next;
            self.selection_changed = true;
        }
        if self.anchor.is_some_and(|a| !self.selected.contains(&a)) {
            self.anchor = None;
        }
    }

    fn take_selection_change(&mut self) -> bool {
        std::mem::take(&mut self.selection_changed)
    }
}
