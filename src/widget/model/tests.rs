/// A subtree painter's own text passes through verbatim, and the adapter still
/// draws the detached label above its content — the widget cannot, it does not
/// know about the label.
#[test]
fn a_subtree_painters_detached_label_is_drawn() {
    use crate::scene::paint::PaintCtx;
    use crate::widget::{TreeList, WidgetHost};
    let ui = crate::context::UiContext::new();
    let mut tree = TreeList::new().with_label("TreeList");
    let strip = tree.label_strip();
    assert!(strip > 0.0);
    tree.set_rect(0.0, 0.0, 300.0, 200.0 + strip);
    let mut pc = PaintCtx::new();
    WidgetHost::paint_self(&tree, &ui, &mut pc);
    let texts: Vec<(String, f32)> = pc
        .finish()
        .items
        .into_iter()
        .filter_map(|it| match it.prim {
            crate::scene::paint::Prim::Text { text, y, .. } => Some((text, y)),
            _ => None,
        })
        .collect();
    let label = texts.iter().find(|(t, _)| t == "TreeList").expect("the detached label is drawn");
    assert_eq!(label.1, 0.0, "on the strip above the content");
    assert!(texts.iter().any(|(t, _)| t == "Key"), "the tree's own header text still passes through");
}

use super::*;
use crate::widget::PathController;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::Prim;
use crate::scene::painter::paint_tree;
use crate::widget::UiContext;

/// A leaf that only knows the two narrow concerns — no `WidgetHost` in sight: it reports an
/// intrinsic size ([`Layout`]) and a color ([`Paint`]).
struct Dot {
    color: [f32; 4],
    size: Size,
}
impl Layout for Dot {
    fn intrinsic_size(&self) -> Option<Size> {
        Some(self.size)
    }
}
impl Paint for Dot {
    fn color(&self) -> [f32; 4] {
        self.color
    }
}
impl Input for Dot {}

/// A narrow container: it drives a column layout ([`Layout`]) and paints nothing.
struct Col;
impl Layout for Col {}
impl Paint for Col {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }
}
impl Input for Col {}

fn rect_of(w: &dyn WidgetHost) -> Rect {
    let (x, y, w, h) = w.rect();
    Rect { x, y, width: w, height: h }
}

/// One rhythm for labeled controls: the preferred height is the CONTENT height
/// for every kind (ProgressBar, Slider, Spinbox, Dropdown), and `layout` places that
/// content at the origin with the label strip hanging above it — so a strategy
/// placing content boxes lines mixed controls up by content, neither squashes a
/// track to the label's leftovers nor lets a label spill into the gap below.
#[test]
fn labeled_controls_land_their_content_at_the_origin_with_the_label_above() {
    use crate::widget::{Dropdown, LayoutConstraints, Point, ProgressBar, Slider, Spinbox};
    let mut ctx = UiContext::new();
    let mut slider = Slider::new().with_label("Gain");
    let mut spinbox = Spinbox::new(1, 0, 9, 1).with_label("Count");
    let mut dropdown = Dropdown::new(vec!["a".into()], 0).with_label("Pick");
    let mut bar = ProgressBar::new(0.5).with_label("Load");
    let strip = slider.base.label_offset();
    assert!(strip > 0.0, "a detached label has a strip above the content");

    for (name, w, content) in [
        ("slider", &mut slider as &mut dyn WidgetHost, crate::layout::slider_height()),
        ("spinbox", &mut spinbox, crate::layout::spinbox_height()),
        ("dropdown", &mut dropdown, crate::layout::dropdown_height()),
        ("progress bar", &mut bar, crate::layout::progressbar_height()),
    ] {
        let pref = w.preferred_height().expect(name);
        assert!((pref - content).abs() < 0.01, "{name}: preferred {pref} is the content height {content}");
        assert!((w.label_strip() - strip).abs() < 0.01, "{name}: one label strip");
        w.layout(Point { x: 0.0, y: 100.0 }, LayoutConstraints::new(100.0, 100.0, pref, pref), &mut ctx);
        let (_, top, _, landed) = w.rect();
        assert!((top - (100.0 - strip)).abs() < 0.01, "{name}: the label hangs above the origin (top {top})");
        assert!((landed - (content + strip)).abs() < 0.01, "{name}: occupied {landed} = content + strip");
        let painted = landed - w.label_strip();
        assert!((painted - content).abs() < 0.01, "{name}: content {painted}, wanted {content}");
    }
}

#[test]
fn narrow_widget_lays_out_and_paints_through_the_adapter() {
    // A pure narrow-trait widget tree (Col + two Dots), wrapped in `Adapted`, is laid out by
    // the existing bridge and painted by the existing painter — proving a widget that never
    // touches `WidgetHost` participates in both live passes.
    let mut ctx = UiContext::new();
    let root = ctx.insert(Adapted::new(Col));
    let a = ctx.insert(Adapted::new(Dot { color: [1.0, 0.0, 0.0, 1.0], size: Size::new(10.0, 10.0) }));
    let b = ctx.insert(Adapted::new(Dot { color: [0.0, 1.0, 0.0, 1.0], size: Size::new(10.0, 20.0) }));
    ctx.link_ids(root.id(), a.id());
    ctx.link_ids(root.id(), b.id());

    // Layout by hand (the Phase-2b bridge is gone; apps drive the solver directly) —
    // the same column-of-two placement the bridge used to compute.
    ctx[root].set_rect(0.0, 0.0, 100.0, 100.0);
    ctx[a].set_rect(0.0, 0.0, 10.0, 10.0);
    ctx[b].set_rect(0.0, 14.0, 10.0, 20.0);
    assert_eq!(rect_of(&ctx[a]), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&ctx[b]), Rect { x: 0.0, y: 14.0, width: 10.0, height: 20.0 });

    // Paint: each Dot's `Paint::paint` default emits one quad at its laid-out rect, in colour.
    let list = paint_tree(&ctx, &ctx[root]);
    let quads: Vec<_> = list
        .items
        .iter()
        .filter_map(|it| match it.prim {
            Prim::Quad { rect, color } => Some((rect, color)),
            _ => None,
        })
        .collect();
    assert!(
        quads.iter().any(|(r, c)| *r == Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 } && c[0] == 1.0),
        "red Dot painted at its laid-out rect: {quads:?}",
    );
    assert!(
        quads.iter().any(|(r, c)| *r == Rect { x: 0.0, y: 14.0, width: 10.0, height: 20.0 } && c[1] == 1.0),
        "green Dot painted at its laid-out rect: {quads:?}",
    );
}

/// A narrow interactive widget: counts left-clicks and records hover transitions — all
/// through [`Input::on_event`], never touching `WidgetHost`.
struct Clicker {
    clicks: u32,
    entered: u32,
    left: u32,
}
impl Layout for Clicker {}
impl Paint for Clicker {
    fn color(&self) -> [f32; 4] {
        [0.5, 0.5, 0.5, 1.0]
    }
}
impl Input for Clicker {
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        use crate::widget::{ElementState, MouseButton};
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                self.clicks += 1;
                true
            }
            Event::MouseEnter => {
                self.entered += 1;
                false
            }
            Event::MouseLeave => {
                self.left += 1;
                false
            }
            _ => false,
        }
    }
}

#[test]
fn narrow_widget_receives_routed_events_through_the_adapter() {
    use crate::widget::{ElementState, MouseButton};
    let mut ctx = UiContext::new();
    let w = ctx.insert(Adapted::new(Clicker { clicks: 0, entered: 0, left: 0 }));
    let id = w.id();
    ctx[w].set_rect(10.0, 10.0, 40.0, 20.0);

    let click_at = |x: f32, y: f32| Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x,
        y,
        local_x: x,
        local_y: y,
    };

    // A click inside the rect is hit-gated in, consumed, and counted.
    assert!(ctx.propagate_event(&click_at(20.0, 15.0), id), "in-rect click is consumed");
    // A click outside never reaches on_event (the adapter's hit gate rejects it).
    assert!(!ctx.propagate_event(&click_at(200.0, 200.0), id), "out-of-rect click passes through");
    assert_eq!(ctx[w].inner().clicks, 1, "only the in-rect click was counted");

    // Hover: moving inside synthesizes MouseEnter (via the legacy bookkeeping the adapter
    // preserves) and sets the base hover flag; moving away synthesizes MouseLeave.
    ctx.propagate_event(&Event::PointerMove { x: 20.0, y: 15.0, local_x: 20.0, local_y: 15.0 }, id);
    assert_eq!(ctx[w].inner().entered, 1, "MouseEnter reached on_event");
    assert!(ctx[w].base().hovered, "base hover flag set through the adapter");
    ctx.propagate_event(&Event::PointerMove { x: 200.0, y: 200.0, local_x: 200.0, local_y: 200.0 }, id);
    assert_eq!(ctx[w].inner().left, 1, "MouseLeave reached on_event");
    assert!(!ctx[w].base().hovered, "base hover flag cleared");
}

/// A narrow widget that is also a controller: the controller trait is reached through the
/// concrete `Adapted<W>` by deref (Phase 6aw -- the `WidgetHost::as_*_controller` discovery
/// hooks are deleted).
struct Crumbs {
    segs: Vec<String>,
    clicked: Option<usize>,
}
impl Layout for Crumbs {}
impl Paint for Crumbs {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }
}
impl Input for Crumbs {
}
impl PathController for Crumbs {
    fn set_path(&mut self, segments: &[String]) {
        self.segs = segments.to_vec();
    }
    fn path_click(&mut self) -> Option<usize> {
        self.clicked.take()
    }
}

#[test]
fn controller_capability_reached_through_the_concrete_adapter() {
    let mut w = Box::new(Adapted::new(Crumbs { segs: Vec::new(), clicked: Some(2) }));

    // The controller trait is reached by deref through the concrete Adapted<W>...
    PathController::set_path(&mut **w, &["home".to_string(), "user".to_string()]);
    assert_eq!(PathController::path_click(&mut **w), Some(2));

    // ...and lands on the same state the concrete widget sees.
    assert_eq!(w.inner().segs, vec!["home".to_string(), "user".to_string()]);
    assert_eq!(w.inner().clicked, None, "path_click drained through the deref");
}
/// Phase 6: the paint walk's text prims carry the widget's font and clip rect (what the
/// display-list text path renders), not the bare `Paint::paint` text.
#[test]
fn paint_walk_text_carries_font_and_bounds() {
    struct Tag;
    impl Layout for Tag {}
    impl Paint for Tag {
        fn color(&self) -> [f32; 4] {
            [0.0; 4]
        }
        fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
            ctx.text("hi", rect.x + 2.0, rect.y + 2.0, 12.0, [1, 2, 3]);
        }
        fn widget_font(&self) -> Option<String> {
            Some("Mono:12".into())
        }
        fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
            Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
        }
    }
    impl Input for Tag {}

    let mut ctx = UiContext::new();
    let w = ctx.insert(Adapted::new(Tag));
    ctx[w].set_rect(10.0, 20.0, 100.0, 30.0);

    let list = paint_tree(&ctx, &ctx[w]);
    let texts: Vec<_> = list
        .items
        .iter()
        .filter_map(|it| match &it.prim {
            Prim::Text { text, font, bounds, .. } => Some((text.clone(), font.clone(), *bounds)),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 1, "one text prim, no plain duplicate");
    assert_eq!(texts[0].0, "hi");
    assert_eq!(texts[0].1.as_deref(), Some("Mono:12"), "widget_font attached");
    assert_eq!(texts[0].2, Some([10.0, 20.0, 110.0, 50.0]), "text_bounds attached");
}

/// `text_bounds` clips the content text only: the detached label sits in the
/// strip above the content rect, so the content's clip would cut it away.
#[test]
fn text_bounds_leave_the_detached_label_unclipped() {
    struct Tag;
    impl Layout for Tag {}
    impl Paint for Tag {
        fn color(&self) -> [f32; 4] {
            [0.0; 4]
        }
        fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
            ctx.text("hi", rect.x + 2.0, rect.y + 2.0, 12.0, [1, 2, 3]);
        }
        fn text_bounds(&self, rect: Rect) -> Option<[f32; 4]> {
            Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height])
        }
    }
    impl Input for Tag {}

    let mut ctx = UiContext::new();
    let w = ctx.insert(Adapted::new(Tag).with_label("Name"));
    ctx[w].set_rect(10.0, 20.0, 100.0, 60.0);

    let list = paint_tree(&ctx, &ctx[w]);
    let bounds_of = |want: &str| {
        list.items.iter().find_map(|it| match &it.prim {
            Prim::Text { text, bounds, .. } if text == want => Some(*bounds),
            _ => None,
        })
    };
    assert!(matches!(bounds_of("hi"), Some(Some(_))), "content text is clipped");
    assert_eq!(bounds_of("Name"), Some(None), "the label is not");
}
