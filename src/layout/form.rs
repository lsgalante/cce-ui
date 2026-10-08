//! A section's contents as a box-model tree: declared first, solved by `scene::layout`, then
//! painted where the solver put each piece.
//!
//! [`SectionContext`](super::SectionContext) frames a section (its title tab, its well) and
//! used to place what goes in it with a cursor: every row was handed its y and width, and a
//! page added insets of its own on top. A [`Form`] replaces the cursor. A page asks the section
//! for one ([`SectionContext::form`]), declares its contents into it — retained widgets, text
//! lines, rows of cells, blocks it paints itself — and hands it back
//! ([`SectionContext::place`]), which lays the tree out across the section's content box,
//! paints each piece in the order it was declared, and moves the section past it.
//!
//! The spacing is the ladder's, never the page's: a form is a [`Style::controls_column`],
//! a row a [`Style::controls_row`], both spaced by `control_gap`. A page states sizes only
//! where a piece has a size of its own (a list's height, a button's width).

use crate::scene::arena::{Arena, NodeId};
use crate::scene::layout::{arrange, measure, CrossAlign, LayoutBox, Length, Rect, Size, Style};
use crate::widget::{UiContext, WidgetHost, WidgetHostExt};

use super::{render_widget, RenderTarget};

/// What a piece paints once it is placed: the target, its rect, the widget context.
pub type Draw<'w, P> = Box<dyn FnOnce(&mut P, Rect, &mut UiContext) + 'w>;

/// The tree a page declares a section's contents into. See the module docs.
pub struct Form<'w, P> {
    arena: Arena<LayoutBox>,
    root: NodeId,
    /// Where the content box starts and how wide it is.
    x: f32,
    y: f32,
    width: f32,
    /// A height to fill: set by [`Form::fill_height`], so a growing piece (a list that takes
    /// the rest of the page) has room to grow into.
    height: Option<f32>,
    /// The row a widget lights when hovered or focused: the section's span, not the cell's.
    row_span: (f32, f32),
    pieces: Vec<(NodeId, Draw<'w, P>)>,
}

impl<'w, P: RenderTarget + 'w> Form<'w, P> {
    /// A form across a content box at `(x, y)`, `width` wide; widgets light `row_span`.
    pub fn new(x: f32, y: f32, width: f32, row_span: (f32, f32)) -> Self {
        let mut arena = Arena::new();
        let root = arena.insert(LayoutBox::container(
            Style::controls_column().cross_align(CrossAlign::Stretch).width(Length::Fixed(width)),
        ));
        Form { arena, root, x, y, width, height: None, row_span, pieces: Vec::new() }
    }

    /// The content box's width — what a page wraps text to.
    pub fn width(&self) -> f32 {
        self.width
    }

    /// Where the form starts.
    pub fn top(&self) -> f32 {
        self.y
    }

    /// Give the form this height to fill, so a piece declared with
    /// [`Group::fill`] takes what the rest leave of it.
    pub fn fill_height(&mut self, height: f32) {
        self.height = Some(height.max(0.0));
    }

    /// The form's top-level column.
    pub fn column(&mut self) -> Group<'_, 'w, P> {
        let node = self.root;
        Group { form: self, node, axis_row: false }
    }

    /// Lay the tree out and paint every piece in the order it was declared. Returns the
    /// bottom of what was placed.
    pub fn paint(mut self, pc: &mut P, ctx: &mut UiContext) -> f32 {
        let measured = measure(&mut self.arena, self.root);
        let height = self.height.unwrap_or(measured.height);
        arrange(&mut self.arena, self.root, Rect { x: self.x, y: self.y, width: self.width, height });
        for (node, draw) in self.pieces {
            let rect = self.arena.value(node).map(|b| b.rect).unwrap_or(Rect::ZERO);
            draw(pc, rect, ctx);
        }
        self.y + height
    }

    fn add(&mut self, parent: NodeId, style: Style, size: Size, draw: Option<Draw<'w, P>>) -> NodeId {
        let node = self.arena.insert(LayoutBox::leaf(style, size));
        self.arena.append_child(parent, node);
        if let Some(draw) = draw {
            self.pieces.push((node, draw));
        }
        node
    }
}

/// A column or a row of a [`Form`], what pieces are declared into.
pub struct Group<'f, 'w, P> {
    form: &'f mut Form<'w, P>,
    node: NodeId,
    axis_row: bool,
}

impl<'f, 'w, P: RenderTarget + 'w> Group<'f, 'w, P> {
    /// The leaf style for a piece of this group: across a column it stretches to the width;
    /// along a row it takes its own width and grows only when asked.
    fn leaf_style(&self, grow: bool) -> Style {
        if grow { Style::default().grow(1.0) } else { Style::default() }
    }

    /// The span a widget lights when hovered or focused: down a column, the section's row;
    /// in a row, its own cell (`None`).
    fn row_span(&self) -> Option<(f32, f32)> {
        if self.axis_row { None } else { Some(self.form.row_span) }
    }

    /// A retained widget at its own height (its preferred height, or `fallback`, plus its
    /// detached label). In a row it shares the row's width with the other growing cells.
    pub fn widget<T: WidgetHost + 'static>(&mut self, w: &'w mut T, fallback: f32) -> &mut Self {
        let h = w.preferred_height().unwrap_or(fallback) + w.label_strip();
        let style = self.leaf_style(self.axis_row);
        let span = self.row_span();
        let draw: Draw<'w, P> = Box::new(move |pc, r, ctx| {
            let (x, width) = span.unwrap_or((r.x, r.width));
            w.set_row_rect(x, width);
            render_widget(pc, w, r.x, r.y, r.width, r.height, ctx);
        });
        self.form.add(self.node, style, Size::new(0.0, h), Some(draw));
        self
    }

    /// A retained widget at a width of its own (a toggle that is not as wide as its row), not
    /// growing. Its height is as for [`widget`](Self::widget).
    pub fn widget_w<T: WidgetHost + 'static>(&mut self, w: &'w mut T, width: f32, fallback: f32) -> &mut Self {
        let h = w.preferred_height().unwrap_or(fallback) + w.label_strip();
        let span = self.row_span();
        let draw: Draw<'w, P> = Box::new(move |pc, r, ctx| {
            let (x, width) = span.unwrap_or((r.x, r.width));
            w.set_row_rect(x, width);
            render_widget(pc, w, r.x, r.y, r.width, r.height, ctx);
        });
        self.form.add(self.node, Style::default(), Size::new(width, h), Some(draw));
        self
    }

    /// A piece the page paints itself, `w` × `h` (in a column the width is the column's;
    /// in a row `w` is the cell's width, and it grows into the row's slack when `grow`).
    pub fn draw(&mut self, w: f32, h: f32, grow: bool, draw: impl FnOnce(&mut P, Rect, &mut UiContext) + 'w) -> &mut Self {
        let style = self.leaf_style(grow);
        self.form.add(self.node, style, Size::new(w, h), Some(Box::new(draw)));
        self
    }

    /// A piece that takes the height the rest of the form leaves (see
    /// [`Form::fill_height`]), at least `min_h`.
    pub fn fill(&mut self, min_h: f32, draw: impl FnOnce(&mut P, Rect, &mut UiContext) + 'w) -> &mut Self {
        let style = Style { min_height: min_h, ..Style::default() }.grow(1.0);
        self.form.add(self.node, style, Size::new(0.0, min_h), Some(Box::new(draw)));
        self
    }

    /// A line of text in the default face, as tall as a section's text line.
    pub fn text(&mut self, text: impl Into<String>, size: f32, color: [f32; 4]) -> &mut Self {
        let text = text.into();
        let w = text_width(&text, size);
        self.draw(w, line_height(size), false, move |pc, r, _| {
            pc.text_with_bounds(&text, r.x, r.y, size, color, Some([r.x, r.y - size, r.x + r.width, r.y + 2.0 * size]));
        })
    }

    /// A line of text that takes the slack of its row (or the width of its column) and is cut
    /// at the edge of what it was given — a value beside a label that may run long.
    pub fn text_fill(&mut self, text: impl Into<String>, size: f32, color: [f32; 4]) -> &mut Self {
        let text = text.into();
        self.draw(0.0, line_height(size), true, move |pc, r, _| {
            pc.text_with_bounds(&text, r.x, r.y, size, color, Some([r.x, r.y - size, r.x + r.width, r.y + 2.0 * size]));
        })
    }

    /// Lines of text, one under the next, as one piece.
    pub fn lines(&mut self, lines: Vec<String>, size: f32, color: [f32; 4]) -> &mut Self {
        let w = lines.iter().map(|l| text_width(l, size)).fold(0.0, f32::max);
        let h = line_height(size) * lines.len() as f32;
        self.draw(w, h, false, move |pc, r, _| {
            for (i, line) in lines.iter().enumerate() {
                let y = r.y + i as f32 * line_height(size);
                pc.text_with_bounds(line, r.x, y, size, color, Some([r.x, y - size, r.x + r.width, y + 2.0 * size]));
            }
        })
    }

    /// A one-pixel rule across the group, parting its zones.
    pub fn rule(&mut self, color: [f32; 4]) -> &mut Self {
        self.draw(0.0, 1.0, false, move |pc, r, _| pc.rect(color, r.x, r.y, r.width, 1.0))
    }

    /// Empty room of the given extent (along a row, a width; down a column, a height),
    /// growing into the slack when `grow` — what pushes a row's later cells to its end.
    pub fn space(&mut self, extent: f32, grow: bool) -> &mut Self {
        let size = if self.axis_row { Size::new(extent, 0.0) } else { Size::new(0.0, extent) };
        let style = self.leaf_style(grow);
        self.form.add(self.node, style, size, None);
        self
    }

    /// A row of cells, the control gap between them, centred on each other.
    pub fn row(&mut self, build: impl FnOnce(&mut Group<'_, 'w, P>)) -> &mut Self {
        self.group(Style::controls_row().cross_align(CrossAlign::Center), true, build)
    }

    /// Lines of text one under the next with no gap between them, stretched across: a block
    /// of text, not a column of controls.
    pub fn block(&mut self, build: impl FnOnce(&mut Group<'_, 'w, P>)) -> &mut Self {
        self.group(Style::column().cross_align(CrossAlign::Stretch), false, build)
    }

    /// A column of pieces, the control gap between them, stretched across.
    pub fn column(&mut self, build: impl FnOnce(&mut Group<'_, 'w, P>)) -> &mut Self {
        self.group(Style::controls_column().cross_align(CrossAlign::Stretch), false, build)
    }

    /// A nested group with a style of its own (a grid, a tighter gap).
    pub fn group(&mut self, style: Style, row: bool, build: impl FnOnce(&mut Group<'_, 'w, P>)) -> &mut Self {
        let node = self.form.arena.insert(LayoutBox::container(style));
        self.form.arena.append_child(self.node, node);
        let mut g = Group { form: self.form, node, axis_row: row };
        build(&mut g);
        self
    }

    /// The width of the form this group is in.
    pub fn form_width(&self) -> f32 {
        self.form.width
    }
}

/// How tall a section's text line is at `size`: the size and the 4 px a line has always had
/// below it.
pub fn line_height(size: f32) -> f32 {
    size + 4.0
}

/// The width `text` takes at `size` in the default face, as the renderer shapes it.
pub fn text_width(text: &str, size: f32) -> f32 {
    crate::geometry_font_system()
        .lock()
        .ok()
        .and_then(|mut fs| crate::backend::text::shaped_cluster_offsets(&mut fs, text, size, None).last().map(|&(_, total)| total))
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    /// A target that keeps the bounds every text was given.
    #[derive(Default)]
    struct Texts {
        bounds: Vec<(String, Option<[f32; 4]>)>,
    }
    impl RenderTarget for Texts {
        fn rect(&mut self, _: [f32; 4], _: f32, _: f32, _: f32, _: f32) {}
        fn text(&mut self, content: &str, _: f32, _: f32, _: f32, _: [f32; 4]) {
            self.bounds.push((content.to_string(), None));
        }
        fn text_with_bounds(&mut self, content: &str, _: f32, _: f32, _: f32, _: [f32; 4], bounds: Option<[f32; 4]>) {
            self.bounds.push((content.to_string(), bounds));
        }
    }

    type Seen = Rc<RefCell<Vec<Rect>>>;

    /// A piece of the given size that records where it was put.
    fn piece<'w>(g: &mut Group<'_, 'w, Texts>, seen: &Seen, w: f32, h: f32, grow: bool) {
        let seen = seen.clone();
        g.draw(w, h, grow, move |_, r, _| seen.borrow_mut().push(r));
    }

    /// A form spans its content box, and its pieces stand one control gap apart, in the
    /// order they were declared; the section moves past the last.
    #[test]
    fn a_form_stacks_its_pieces_a_control_gap_apart_across_the_box() {
        let gap = crate::layout::control_gap();
        let seen: Seen = Rc::default();
        let mut form: Form<'_, Texts> = Form::new(10.0, 20.0, 300.0, (0.0, 320.0));
        {
            let mut col = form.column();
            piece(&mut col, &seen, 0.0, 30.0, false);
            piece(&mut col, &seen, 0.0, 12.0, false);
        }
        let bottom = form.paint(&mut Texts::default(), &mut UiContext::new());
        let r = seen.borrow();
        assert_eq!((r[0].x, r[0].y, r[0].width, r[0].height), (10.0, 20.0, 300.0, 30.0));
        assert_eq!((r[1].x, r[1].y, r[1].width), (10.0, 20.0 + 30.0 + gap, 300.0));
        assert_eq!(bottom, 20.0 + 30.0 + gap + 12.0);
    }

    /// Cells that grow share a row's slack equally on top of what each asked for — a row of
    /// buttons sized to their labels — and the row is the form's width, gaps included.
    #[test]
    fn a_rows_growing_cells_share_its_slack() {
        let gap = crate::layout::control_gap();
        let seen: Seen = Rc::default();
        let mut form: Form<'_, Texts> = Form::new(0.0, 0.0, 400.0, (0.0, 400.0));
        form.column().row(|r| {
            piece(r, &seen, 50.0, 20.0, true);
            piece(r, &seen, 100.0, 20.0, true);
        });
        form.paint(&mut Texts::default(), &mut UiContext::new());
        let r = seen.borrow();
        let extra = (400.0 - gap - 150.0) / 2.0;
        assert!((r[0].width - (50.0 + extra)).abs() < 1e-3, "{r:?}");
        assert!((r[1].width - (100.0 + extra)).abs() < 1e-3, "{r:?}");
        assert!((r[1].x + r[1].width - 400.0).abs() < 1e-3, "the row ends at the form's edge");
    }

    /// Given a height to fill, a fill piece takes what the other pieces leave of it.
    #[test]
    fn a_fill_piece_takes_the_rest_of_the_height() {
        let gap = crate::layout::control_gap();
        let seen: Seen = Rc::default();
        let mut form: Form<'_, Texts> = Form::new(0.0, 0.0, 200.0, (0.0, 200.0));
        form.fill_height(300.0);
        {
            let mut col = form.column();
            piece(&mut col, &seen, 0.0, 40.0, false);
            let s = seen.clone();
            col.fill(10.0, move |_, r, _| s.borrow_mut().push(r));
        }
        let bottom = form.paint(&mut Texts::default(), &mut UiContext::new());
        let r = seen.borrow();
        assert!((r[1].height - (300.0 - 40.0 - gap)).abs() < 1e-3, "{r:?}");
        assert_eq!(bottom, 300.0);
    }

    /// Text is cut where its piece ends: a line in a column at the content box's edge, a value
    /// at its cell's.
    #[test]
    fn text_is_cut_at_the_edge_of_its_piece() {
        let mut form: Form<'_, Texts> = Form::new(10.0, 0.0, 200.0, (0.0, 200.0));
        {
            let mut col = form.column();
            col.text("a line far wider than the box it was put in", 12.0, [1.0; 4]);
            col.row(|r| {
                r.draw(60.0, 16.0, false, |_, _, _| {});
                r.text_fill("a value", 12.0, [1.0; 4]);
            });
        }
        let mut out = Texts::default();
        form.paint(&mut out, &mut UiContext::new());
        for (text, bounds) in &out.bounds {
            let right = bounds.expect("bounded")[2];
            assert!((right - 210.0).abs() < 1e-3, "{text}: {bounds:?}");
        }
    }
}
