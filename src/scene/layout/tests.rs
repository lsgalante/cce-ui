use super::*;

#[test]
fn contains_is_half_open() {
    let r = Rect { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
    assert!(r.contains(10.0, 20.0), "the top-left corner is inside");
    assert!(r.contains(109.9, 69.9));
    assert!(!r.contains(110.0, 40.0), "the right edge is outside");
    assert!(!r.contains(50.0, 70.0), "the bottom edge is outside");
    assert!(!r.contains(9.9, 40.0) && !r.contains(50.0, 19.9));
    // Neighbours sharing an edge split it: the point goes to one only.
    let next = Rect { x: 110.0, ..r };
    assert!(!r.contains(110.0, 40.0) && next.contains(110.0, 40.0));
    assert!(!Rect::ZERO.contains(0.0, 0.0), "an empty rect contains nothing");
}

#[test]
fn fit_contain_letterboxes_and_centers() {
    let b = Rect { x: 10.0, y: 20.0, width: 100.0, height: 50.0 };
    // 200x100 source, scale limited by both axes equally -> 100x50 fill
    let r = fit_rect(200, 100, b, FitMode::Contain { max_upscale: 4.0 });
    assert_eq!((r.x, r.y, r.width, r.height), (10.0, 20.0, 100.0, 50.0));
    // tall source letterboxes horizontally: scale = 50/200 -> 25x50
    let r = fit_rect(100, 200, b, FitMode::Contain { max_upscale: 4.0 });
    assert_eq!((r.width, r.height), (25.0, 50.0));
    assert_eq!(r.x, 10.0 + (100.0 - 25.0) * 0.5);
    assert_eq!(r.y, 20.0);
}

#[test]
fn fit_contain_caps_upscale_but_downscales_freely() {
    let b = Rect { x: 0.0, y: 0.0, width: 400.0, height: 400.0 };
    // small source: would need 8x, capped at 4x, centered
    let r = fit_rect(50, 50, b, FitMode::Contain { max_upscale: 4.0 });
    assert_eq!((r.width, r.height), (200.0, 200.0));
    assert_eq!((r.x, r.y), (100.0, 100.0));
    // large source downscales with no floor (the old .max(1.0) bug)
    let r = fit_rect(800, 800, b, FitMode::Contain { max_upscale: 4.0 });
    assert_eq!((r.width, r.height), (400.0, 400.0));
}

#[test]
fn fit_degenerate_inputs() {
    let b = Rect { x: 0.0, y: 0.0, width: 100.0, height: 100.0 };
    let r = fit_rect(0, 50, b, FitMode::Contain { max_upscale: 4.0 });
    assert_eq!((r.width, r.height), (0.0, 0.0));
    let r = fit_rect(10, 10, b, FitMode::Stretch);
    assert_eq!((r.width, r.height), (100.0, 100.0));
}

fn leaf(arena: &mut Arena<LayoutBox>, w: f32, h: f32) -> NodeId {
    arena.insert(LayoutBox::leaf(Style::default(), Size::new(w, h)))
}

fn rect_of(arena: &Arena<LayoutBox>, id: NodeId) -> Rect {
    arena.value(id).unwrap().rect
}

#[test]
fn row_places_children_left_to_right_with_gap() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::row().gap(5.0)));
    let a = leaf(&mut arena, 10.0, 10.0);
    let b = leaf(&mut arena, 20.0, 10.0);
    arena.append_child(root, a);
    arena.append_child(root, b);

    compute_layout(&mut arena, root, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&arena, b), Rect { x: 15.0, y: 0.0, width: 20.0, height: 10.0 });
}

#[test]
fn padding_offsets_content() {
    let mut arena = Arena::new();
    let mut s = Style::row();
    s.padding = Edges { left: 5.0, right: 0.0, top: 7.0, bottom: 0.0 };
    let root = arena.insert(LayoutBox::container(s));
    let a = leaf(&mut arena, 10.0, 10.0);
    arena.append_child(root, a);

    compute_layout(&mut arena, root, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a), Rect { x: 5.0, y: 7.0, width: 10.0, height: 10.0 });
}

#[test]
fn grow_distributes_free_space_by_weight() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::row()));
    let a = arena.insert(LayoutBox::leaf(Style::default().grow(1.0), Size::new(10.0, 10.0)));
    let b = arena.insert(LayoutBox::leaf(Style::default().grow(3.0), Size::new(10.0, 10.0)));
    arena.append_child(root, a);
    arena.append_child(root, b);

    compute_layout(&mut arena, root, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a).width, 30.0);
    assert_eq!(rect_of(&arena, b).width, 70.0);
    assert_eq!(rect_of(&arena, b).x, 30.0);
}

#[test]
fn shrink_absorbs_overflow_by_weight() {
    // Two 60-wide leaves in 100px, both shrink 1 => 20px deficit split evenly => 50 each.
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::row()));
    let a = arena.insert(LayoutBox::leaf(Style::default().shrink(1.0), Size::new(60.0, 10.0)));
    let b = arena.insert(LayoutBox::leaf(Style::default().shrink(1.0), Size::new(60.0, 10.0)));
    arena.append_child(root, a);
    arena.append_child(root, b);

    compute_layout(&mut arena, root, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a).width, 50.0);
    assert_eq!(rect_of(&arena, b).width, 50.0);
    assert_eq!(rect_of(&arena, b).x, 50.0);
}

#[test]
fn main_align_center_and_end() {
    let mut arena = Arena::new();
    let root_c = arena.insert(LayoutBox::container(Style::row().main_align(MainAlign::Center)));
    let a = leaf(&mut arena, 20.0, 10.0);
    arena.append_child(root_c, a);
    compute_layout(&mut arena, root_c, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a).x, 40.0);

    let mut arena2 = Arena::new();
    let root_e = arena2.insert(LayoutBox::container(Style::row().main_align(MainAlign::End)));
    let b = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(20.0, 10.0)));
    arena2.append_child(root_e, b);
    compute_layout(&mut arena2, root_e, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena2, b).x, 80.0);
}

#[test]
fn space_between_pushes_children_to_edges() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::row().main_align(MainAlign::SpaceBetween)));
    let a = leaf(&mut arena, 10.0, 10.0);
    let b = leaf(&mut arena, 10.0, 10.0);
    arena.append_child(root, a);
    arena.append_child(root, b);

    compute_layout(&mut arena, root, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a).x, 0.0);
    assert_eq!(rect_of(&arena, b).x, 90.0);
}

#[test]
fn cross_align_center_and_stretch() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::row().cross_align(CrossAlign::Center)));
    let a = leaf(&mut arena, 10.0, 10.0);
    arena.append_child(root, a);
    compute_layout(&mut arena, root, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena, a).y, 20.0);

    let mut arena2 = Arena::new();
    let root2 = arena2.insert(LayoutBox::container(Style::row().cross_align(CrossAlign::Stretch)));
    let b = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(10.0, 10.0)));
    arena2.append_child(root2, b);
    compute_layout(&mut arena2, root2, Size::new(100.0, 50.0));
    assert_eq!(rect_of(&arena2, b).height, 50.0);
    assert_eq!(rect_of(&arena2, b).y, 0.0);
}

#[test]
fn auto_container_measures_to_content() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::column().gap(4.0)));
    let a = leaf(&mut arena, 10.0, 10.0);
    let b = leaf(&mut arena, 10.0, 10.0);
    arena.append_child(root, a);
    arena.append_child(root, b);

    let m = measure(&mut arena, root);
    assert_eq!(m, Size::new(10.0, 24.0));
}

#[test]
fn fixed_length_overrides_content_and_clamps() {
    let mut arena = Arena::new();
    let s = Style::column().width(Length::Fixed(200.0));
    let root = arena.insert(LayoutBox::container(s));
    let a = leaf(&mut arena, 10.0, 10.0);
    arena.append_child(root, a);

    let m = measure(&mut arena, root);
    assert_eq!(m.width, 200.0);
    assert_eq!(m.height, 10.0);
}

#[test]
fn nested_containers_lay_out_recursively() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::row().gap(0.0)));
    let inner = arena.insert(LayoutBox::container(Style::column().gap(2.0)));
    let c1 = leaf(&mut arena, 10.0, 10.0);
    let c2 = leaf(&mut arena, 10.0, 10.0);
    let sibling = leaf(&mut arena, 5.0, 5.0);
    arena.append_child(root, inner);
    arena.append_child(root, sibling);
    arena.append_child(inner, c1);
    arena.append_child(inner, c2);

    compute_layout(&mut arena, root, Size::new(100.0, 100.0));
    assert_eq!(rect_of(&arena, inner), Rect { x: 0.0, y: 0.0, width: 10.0, height: 22.0 });
    assert_eq!(rect_of(&arena, c1), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&arena, c2), Rect { x: 0.0, y: 12.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&arena, sibling).x, 10.0);
}

#[test]
fn stack_overlays_children_and_aligns_per_axis() {
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::stack()));
    let a = leaf(&mut arena, 10.0, 10.0);
    let b = leaf(&mut arena, 30.0, 20.0);
    arena.append_child(root, a);
    arena.append_child(root, b);
    compute_layout(&mut arena, root, Size::new(100.0, 100.0));
    // Start/Start: both at the content origin, at their own sizes.
    assert_eq!(rect_of(&arena, a), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&arena, b), Rect { x: 0.0, y: 0.0, width: 30.0, height: 20.0 });

    // Centered on both axes.
    let mut arena2 = Arena::new();
    let root2 = arena2.insert(LayoutBox::container(
        Style::stack().main_align(MainAlign::Center).cross_align(CrossAlign::Center),
    ));
    let c = arena2.insert(LayoutBox::leaf(Style::default(), Size::new(10.0, 10.0)));
    arena2.append_child(root2, c);
    compute_layout(&mut arena2, root2, Size::new(100.0, 100.0));
    assert_eq!(rect_of(&arena2, c), Rect { x: 45.0, y: 45.0, width: 10.0, height: 10.0 });
}

#[test]
fn grid_flows_children_by_columns() {
    // 3 leaves (10x10) in a 2-col grid, gaps 5/5.
    let mut arena = Arena::new();
    let root = arena.insert(LayoutBox::container(Style::grid(2, 5.0, 5.0)));
    let a = leaf(&mut arena, 10.0, 10.0);
    let b = leaf(&mut arena, 10.0, 10.0);
    let c = leaf(&mut arena, 10.0, 10.0);
    arena.append_child(root, a);
    arena.append_child(root, b);
    arena.append_child(root, c);

    // measured: 2 cols * 10 + 5 = 25 wide; 2 rows * 10 + 5 = 25 tall.
    let m = measure(&mut arena, root);
    assert_eq!(m, Size::new(25.0, 25.0));

    compute_layout(&mut arena, root, Size::new(200.0, 200.0));
    assert_eq!(rect_of(&arena, a), Rect { x: 0.0, y: 0.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&arena, b), Rect { x: 15.0, y: 0.0, width: 10.0, height: 10.0 });
    assert_eq!(rect_of(&arena, c), Rect { x: 0.0, y: 15.0, width: 10.0, height: 10.0 });
}
