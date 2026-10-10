use super::*;

/// Coming to the fore is a fade, not a flip: the latch moves at once (so
/// input never waits) while the drawn opacity ramps, in and back out.
#[test]
fn scrollbar_fade_ramps_instead_of_flipping() {
    let mut a = ScrollbarActivity::new();
    a.bump();
    // dt = SCROLL_FADE_SECS / 3, so one tick is a third of the way in.
    let dt = SCROLL_FADE_SECS / 3.0;
    a.tick(dt, true, false);
    assert!(a.raised(), "the latch flips immediately");
    assert!(a.fade() > 0.0 && a.fade() < 1.0, "part-way faded in, got {}", a.fade());
    for _ in 0..3 {
        a.tick(dt, true, false);
    }
    assert_eq!(a.fade(), 1.0, "fully in after the fade duration");

    // Let the post-scroll hold expire: the latch drops, then the fade
    // runs back out rather than vanishing with it.
    let ticks = (SCROLL_ACTIVE_HOLD / dt).ceil() as i32 + 1;
    for _ in 0..ticks {
        a.tick(dt, true, false);
    }
    assert!(!a.raised(), "hold expired");
    assert!(a.fade() < 1.0 && a.fade() >= 0.0, "fading out, got {}", a.fade());
    for _ in 0..4 {
        a.tick(dt, true, false);
    }
    assert_eq!(a.fade(), 0.0, "fully out");
}

fn region() -> ScrollRegion {
    // item_height clamps to list_font + 14, so pick one comfortably above any config.
    let mut r = ScrollRegion::new(40.0, 4.0);
    r.set_rect(10.0, 20.0, 200.0, 100.0);
    r
}

/// The left edge of a sink-behind [`region`]'s centred vertical bar.
fn centred_bar_x() -> f32 {
    10.0 + (200.0 - crate::layout::centred_scrollbar_width()) * 0.5
}

/// Run the glide out (a no-op with smoothing off in the test host's config).
fn settle(r: &mut ScrollRegion) {
    let mut n = 0;
    while r.is_animating() && n < 1000 {
        r.tick(1.0 / 60.0);
        n += 1;
    }
}

#[test]
fn wheel_scrolls_and_clamps() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0); // content_h = 444 > 100
    assert!(r.wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 50.0, 50.0));
    settle(&mut r);
    assert_eq!(r.scroll_y, 48.0);
    assert!(!r.wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 500.0, 50.0)); // miss
    r.wheel(&MouseScrollDelta::LineDelta(0.0, -100.0), 50.0, 50.0);
    settle(&mut r);
    assert_eq!(r.scroll_y, 344.0); // clamped to max_scroll
}

#[test]
fn virtualization_matches_list_math() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0);
    r.set_scroll_y(0.0);
    // Row 0 at viewport_y + 0*(44) + 4 = 24; fits (24 + 40 <= 121).
    assert_eq!(r.get_item_draw_y(0, 4.0), Some(24.0));
    // Row 2 at 20 + 92 - 0 = 112: extends past the viewport bottom (121) but
    // still intersects it — returned so the caller draws it cut by the clip.
    assert_eq!(r.get_item_draw_y(2, 4.0), Some(112.0));
    // Row 3 at 20 + 136 = 156: fully below the viewport → culled.
    assert!(r.get_item_draw_y(3, 4.0).is_none());
}

#[test]
fn virtualization_keeps_partial_row_at_top() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0);
    // Scrolled so row 0 (virtual 4..44) is half above the viewport top:
    // draw_y = 20 + 4 - 24 = 0 < viewport_y, but its bottom (40) intersects.
    r.set_scroll_y(24.0);
    assert_eq!(r.get_item_draw_y(0, 4.0), Some(0.0));
    // A row whose bottom ends above the viewport top would be culled; with
    // this geometry row 0 always intersects, so scroll far and check row 0.
    r.set_scroll_y(80.0);
    assert!(r.get_item_draw_y(0, 4.0).is_none());
}

#[test]
fn raw_shim_shares_the_intersection_contract() {
    let mut r = region();
    r.update_bounds_raw(444.0, 20.0, 100.0);
    r.set_scroll_y(50.0);
    // draw_y = 20 + 10 - 50 = -20; bottom = 4 < 19 → fully above, culled.
    assert!(r.get_draw_y(10.0, 24.0).is_none());
    // draw_y = 20 + 40 - 50 = 10: straddles the top edge → returned.
    assert_eq!(r.get_draw_y(40.0, 24.0), Some(10.0));
    // draw_y = 20 + 60 - 50 = 30: fully inside.
    assert_eq!(r.get_draw_y(60.0, 24.0), Some(30.0));
}

#[test]
fn press_focuses_and_grabs_only_scrollbar() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0);
    // Press in the rows area: focused, not dragging, falls through.
    assert!(!r.press(50.0, 50.0));
    assert!(r.focused && !r.dragging);
    // Press on the scrollbar strip (x + w - sb_w - 4 ± 4): consumed.
    let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0;
    assert!(r.press(sb_x + 1.0, 50.0));
    assert!(r.dragging);
    assert!(r.release());
    // Press outside: unfocuses.
    assert!(!r.press(500.0, 500.0));
    assert!(!r.focused);
}

#[test]
fn horizontal_scroll_is_opt_in_and_clamps() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0);
    // No content width declared: x wheel deltas change nothing and the
    // vertical-only behavior (including the y component) is untouched.
    assert!(!r.wheel(&MouseScrollDelta::LineDelta(-2.0, 0.0), 50.0, 50.0));
    assert_eq!(r.scroll_x, 0.0);
    assert!(!r.h_scroll_active());

    // Content wider than the 200px box: x deltas pan and clamp.
    r.set_content_w(500.0);
    assert!(r.h_scroll_active());
    assert!(r.wheel(&MouseScrollDelta::LineDelta(-2.0, 0.0), 50.0, 50.0));
    settle(&mut r);
    assert_eq!(r.scroll_x, 48.0);
    r.wheel(&MouseScrollDelta::LineDelta(-100.0, 0.0), 50.0, 50.0);
    settle(&mut r);
    assert_eq!(r.scroll_x, 300.0); // max = 500 - 200
    r.wheel(&MouseScrollDelta::LineDelta(100.0, 0.0), 50.0, 50.0);
    settle(&mut r);
    assert_eq!(r.scroll_x, 0.0);
}

#[test]
fn h_thumb_press_grabs_and_releases() {
    let mut r = region();
    r.update_bounds(2, 20.0, 100.0); // no vertical overflow
    r.set_content_w(500.0);
    // The bottom strip: y + h - sb_w - 4, thumb starts at track_x.
    let sb_y = 20.0 + 100.0 - crate::layout::scrollbar_width() - 4.0;
    assert!(r.press(20.0, sb_y + 1.0));
    // Drag right: scroll_x follows.
    assert!(r.cursor_moved(120.0, sb_y + 1.0));
    assert!(r.scroll_x > 0.0);
    assert!(r.release());
    // A rows-area press still falls through (no h-bar hit).
    assert!(!r.press(50.0, 50.0));
}

#[test]
fn edge_inset_moves_the_bar_off_the_edge() {
    let mut r = region().with_edge_inset(20.0);
    r.update_bounds(10, 20.0, 100.0);
    // Bar right edge sits edge_inset in from the region's right edge; the
    // old 4px position no longer hits.
    let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 20.0;
    assert!(r.press(sb_x + 1.0, 50.0));
    assert!(r.release());
    let old_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0 + 1.0;
    assert!(!r.press(old_x + 4.1, 50.0)); // past the ±4 slop of the inset bar
}

#[test]
fn sink_behind_gates_input_until_a_scroll_raises() {
    let mut r = region().with_sink_behind(true);
    r.update_bounds(10, 20.0, 100.0);
    assert!(!r.scrollbar_raised());
    // Sunk: a press on the bar strip falls through (the plate occludes it).
    let sb_x = centred_bar_x();
    assert!(!r.press(sb_x + 1.0, 50.0));
    assert!(!r.dragging);
    // A wheel scroll raises it in the same frame…
    assert!(r.wheel(&MouseScrollDelta::LineDelta(0.0, -2.0), 50.0, 50.0));
    assert!(r.scrollbar_raised());
    // …and now the bar takes the grab.
    assert!(r.press(sb_x + 1.0, 50.0));
    assert!(r.dragging);
    assert!(r.release());
    // The release refreshed the hold: still raised, and the hold keeps the
    // repaint signal up so the frame loop keeps ticking toward the sink.
    assert!(r.scrollbar_raised());
    assert!(r.tick(0.3)); // holding → keep frames coming
    assert!(r.scrollbar_raised());
    assert!(r.tick(SCROLL_ACTIVE_HOLD)); // hold lapses → sink flip reported
    assert!(!r.scrollbar_raised());
    assert!(!r.tick(0.016)); // settled sunk: quiet again
}

#[test]
fn hover_sustains_but_never_raises() {
    let mut r = region().with_sink_behind(true);
    r.update_bounds(10, 20.0, 100.0);
    let sb_x = centred_bar_x();
    // Hovering the sunk bar's strip does not raise it.
    r.cursor_moved(sb_x + 1.0, 50.0);
    assert!(!r.tick(0.016));
    assert!(!r.scrollbar_raised());
    // Raise by scrolling (and let the glide land, so the ticks below
    // measure only the raise/sink state), hover it, and let the hold
    // lapse: hover sustains.
    r.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 50.0);
    settle(&mut r);
    r.cursor_moved(sb_x + 1.0, 50.0);
    r.tick(SCROLL_ACTIVE_HOLD + 0.1); // hold lapses, hover keeps it raised
    assert!(r.scrollbar_raised());
    assert!(!r.tick(0.016)); // sustained by hover alone: no repaint churn
    // Pointer leaves: the next tick sinks it.
    r.cursor_moved(50.0, 50.0);
    assert!(r.tick(0.016));
    assert!(!r.scrollbar_raised());
}

#[test]
fn tuple_emission_layers_by_raised_state() {
    let mut r = region().with_sink_behind(true);
    r.update_bounds(10, 20.0, 100.0);
    // Sunk: bar quads come UNDER the bg (push_quads emits bar then bg, the
    // raised-layer call is quiet).
    let mut under = Vec::new();
    r.push_quads(&mut under);
    assert_eq!(under.len(), 3); // track + thumb + bg
    assert_eq!(under[2].2, 200.0); // last quad is the full-width bg fill
    let mut over = Vec::new();
    r.push_scrollbar_quads(&mut over);
    assert!(over.is_empty());
    // Raised: bg alone below, bar above.
    r.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 50.0);
    let mut under = Vec::new();
    r.push_quads(&mut under);
    assert_eq!(under.len(), 1);
    let mut over = Vec::new();
    r.push_scrollbar_quads(&mut over);
    assert_eq!(over.len(), 2);
    // A non-sink region keeps the legacy shape: bg alone, bar always.
    let mut plain = region();
    plain.update_bounds(10, 20.0, 100.0);
    let (mut under, mut over) = (Vec::new(), Vec::new());
    plain.push_quads(&mut under);
    plain.push_scrollbar_quads(&mut over);
    assert_eq!((under.len(), over.len()), (1, 2));
}

/// A sink-behind region's bars ride the centre lines and cross there:
/// the vertical bar down the region's middle, the horizontal one across
/// the viewport's, the horizontal track running the full width rather
/// than stopping short of a corner. Sunk, a press on either lane is a
/// press on the rows; raised, it is the bar's.
#[test]
fn sink_behind_bars_cross_at_the_centre() {
    let mut r = region().with_sink_behind(true).with_edge_inset(30.0);
    r.update_bounds(10, 20.0, 100.0);
    r.set_content_w(500.0);
    let (sb_x, _, sb_w, _, _, _) = r.scrollbar_geom();
    assert!((sb_x + sb_w * 0.5 - (10.0 + 100.0)).abs() < 0.01, "vertical bar on the centre line, edge_inset ignored");
    assert_eq!(sb_w, crate::layout::centred_scrollbar_width());
    let (track_x, sb_y, track_w, sb_h, _, _) = r.h_scrollbar_geom();
    assert!((sb_y + sb_h * 0.5 - (20.0 + 50.0)).abs() < 0.01, "horizontal bar on the viewport's centre line");
    assert_eq!((track_x, track_w), (14.0, 192.0), "the horizontal track crosses, reserving no corner");

    // Sunk: the middle of the list is the rows'.
    assert!(!r.press(110.0, 50.0));
    assert!(!r.press(40.0, 70.0));
    assert!(!r.dragging);
    // Raised by a scroll, the cross takes the press.
    r.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 50.0);
    assert!(r.press(110.0, 40.0));
    assert!(r.release());
    assert!(r.press(40.0, 70.0));
    assert!(r.release());

    // A plain region keeps its edge bars.
    let mut plain = region();
    plain.update_bounds(10, 20.0, 100.0);
    let (sb_x, _, sb_w, _, _, _) = plain.scrollbar_geom();
    assert_eq!(sb_x + sb_w, 10.0 + 200.0 - 4.0);
}

/// Records the alpha of every rect a paint emits.
#[derive(Default)]
struct Alphas(Vec<f32>);
impl crate::scene::paint::RenderTarget for Alphas {
    fn rect(&mut self, color: [f32; 4], _x: f32, _y: f32, _w: f32, _h: f32) {
        self.0.push(color[3]);
    }
    fn text(&mut self, _: &str, _: f32, _: f32, _: f32, _: [f32; 4]) {}
}

/// A sink-behind region's `push_prims` draws the frame and the idle copy
/// under its bg, never the fore copy, which would land under the rows the
/// host draws next; `push_scrollbar_fore` draws that, at the fade, and
/// nothing while the bar is sunk. A plain region's bar is still in
/// `push_prims`, and its fore call is quiet.
#[test]
fn the_fore_copy_is_drawn_after_the_rows() {
    let mut r = region().with_sink_behind(true);
    r.update_bounds(10, 20.0, 100.0);
    let mut prims = Alphas::default();
    r.push_prims(&mut prims);
    // border, idle track + thumb, bg — and no fore copy.
    assert_eq!(prims.0.len(), 4);
    let mut fore = Alphas::default();
    r.push_scrollbar_fore(&mut fore);
    assert!(fore.0.is_empty(), "sunk: no fore copy");
    r.wheel(&MouseScrollDelta::LineDelta(0.0, -1.0), 50.0, 50.0);
    r.tick(SCROLL_FADE_SECS * 0.5);
    let mut prims = Alphas::default();
    r.push_prims(&mut prims);
    assert_eq!(prims.0.len(), 4, "raised, the idle copy still lies under the bg");
    let mut fore = Alphas::default();
    r.push_scrollbar_fore(&mut fore);
    assert_eq!(fore.0.len(), 2);
    let track = crate::color::scrollbar_track_color()[3];
    assert!(fore.0[0] < track, "half faded in: {} of {}", fore.0[0], track);

    let mut plain = region();
    plain.update_bounds(10, 20.0, 100.0);
    let mut prims = Alphas::default();
    plain.push_prims(&mut prims);
    assert_eq!(prims.0.len(), 4, "border, bg, track, thumb");
    let mut fore = Alphas::default();
    plain.push_scrollbar_fore(&mut fore);
    assert!(fore.0.is_empty());
}

#[test]
fn non_sink_regions_are_unchanged() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0);
    assert!(r.scrollbar_raised()); // always interactive
    assert!(!r.tick(1.0)); // tick is a no-op
    let sb_x = 10.0 + 200.0 - crate::layout::scrollbar_width() - 4.0;
    assert!(r.press(sb_x + 1.0, 50.0));
}

#[test]
fn keyboard_is_hover_or_focus_scoped() {
    let mut r = region();
    r.update_bounds(10, 20.0, 100.0);
    let down = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::ArrowDown),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(!r.keyboard(&down)); // neither hovered nor focused
    r.cursor_moved(50.0, 50.0);
    assert!(r.hovered);
    assert!(r.keyboard(&down));
    settle(&mut r);
    assert_eq!(r.scroll_y, 24.0);
}
