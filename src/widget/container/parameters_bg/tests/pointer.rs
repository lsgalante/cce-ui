//! Scrolling, hover, and which control a wheel or finger gesture belongs to.

use super::*;

#[test]
fn scroll_wheel_scrolls_when_content_overflows() {
    let mut ctx = UiContext::new();
    let rows: Vec<(String, String, String)> = (0..30)
        .map(|i| (format!("P{i}"), "1.00".to_string(), "slider:0:2".to_string()))
        .collect();
    let mut p = ParametersBg::new();
    ParamController::set_display_params(&mut *p, &rows);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 200.0);
    assert!(p.content_h > 200.0);
    assert!(crate::widget::WidgetHostExt::is_scrollable(&p));
    // Wheel over the panel body but off every slider row's x-span is impossible (rows are
    // full-width), so scroll via the region below the last visible row: use a y between
    // rows (the 2px slack above a row) — simplest is the bottom padding strip.
    let before = p.scroll_y;
    p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), 150.0, 199.0, &mut ctx);
    // Either a slider consumed it (value change) or the panel scrolled; both mark change.
    // The panel-scroll path must work when no slider is under the pointer:
    p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), 2.0, 2.0, &mut ctx);
    assert!(p.scroll_y >= before, "scroll never decreases on a downward wheel");
}

/// A scroll moves the rows under a still pointer, and the hover follows
/// the rows: the control that was under the pointer goes dark and the one
/// now there lights, without a pointer motion. The pointer's last
/// position is what the pane re-hovers from, so the wheel's own position
/// (over the label column, where the pane takes it) need not be it.
#[test]
fn a_scroll_re_hovers_the_control_under_a_still_pointer() {
    let mut ctx = UiContext::new();
    let rows: Vec<(String, String, String)> = (0..30)
        .map(|i| (format!("P{i}"), "1.00".to_string(), "slider:0:2".to_string()))
        .collect();
    let mut p = ParametersBg::new();
    ParamController::set_display_params(&mut *p, &rows);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 200.0);
    let hovered = |p: &Adapted<ParametersBg>| -> Vec<usize> {
        p.sliders.iter().enumerate().filter(|(_, s)| s.as_ref().is_some_and(|s| s.inner().hovered())).map(|(i, _)| i).collect()
    };
    assert!(hovered(&p).is_empty());

    // Over the first row's control.
    let r0 = p.get_param_rects()[0];
    let (px, py) = (250.0, r0.1 + r0.3 * 0.5);
    p.on_cursor_moved(px, py, &mut ctx);
    assert_eq!(hovered(&p), vec![0], "the slider under the pointer hovers");
    assert_eq!(p.hover_row, Some(0), "and the pane lifts its label");

    // A wheel over the label column scrolls the pane: a notch moves the
    // TARGET and the rows glide there over the following ticks, so the
    // re-hover that matters is the tick's. The pointer's stored position
    // has not moved, but the rows under it have.
    ctx.scroll_gesture_new = true;
    let before = p.scroll_y;
    p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), 2.0, py, &mut ctx);
    for _ in 0..120 {
        WidgetHost::tick(&mut p, 1.0 / 60.0, &mut ctx);
    }
    assert!(p.scroll_y > before, "the pane scrolled: {} -> {}", before, p.scroll_y);
    let now = p
        .get_param_rects()
        .iter()
        .position(|r| py >= r.1 && py <= r.1 + r.3)
        .expect("a row under the pointer after the scroll");
    assert_ne!(now, 0, "a different row is under the pointer");
    assert_eq!(hovered(&p), vec![now], "the hover followed the rows, without a motion");
    assert_eq!(p.hover_row, Some(now), "the label lift followed too");

    // Off the pane, nothing is hovered.
    p.on_cursor_moved(-9999.0, -9999.0, &mut ctx);
    assert!(hovered(&p).is_empty());
    assert_eq!(p.hover_row, None);
}

/// The hover cue is the row's LABEL and the row's OWN carves lit through
/// the tint channel — and nothing else in the pane's paint changes with
/// it. A hover adds no geometry: it can neither close the pane plate's
/// carve-grouping window (a wash before the wells did, and every well
/// flipped shading on any hover) nor composite over a well's shade lines
/// (a wash after them did, and the hovered well's outline paled). Both
/// shipped for part of 2026-09-28. The other rows' carves keep their
/// untinted, grouped shading.
#[test]
fn a_hover_lifts_the_label_and_lights_only_its_own_carves() {
    use crate::scene::paint::Prim;
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Name", "x", "text"), ("Count", "3", "spinbox:0:10"), ("Size", "0.5", "slider:0:1")]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 400.0);
    let rect = Rect { x: 0.0, y: 0.0, width: 300.0, height: 400.0 };
    let prims = |p: &Adapted<ParametersBg>, ctx: &UiContext| {
        let mut pc = crate::scene::paint::PaintCtx::new();
        Paint::paint_ui(&**p, ctx, rect, &mut pc);
        pc.finish().items.into_iter().map(|i| i.prim).collect::<Vec<_>>()
    };
    let label_of = |p: &Adapted<ParametersBg>, name: &str| {
        p.own_text_labels().into_iter().find(|l| l.text == name).map(|l| l.color).expect("a row label")
    };
    // A carve's rect and tint, whatever its kind.
    let carve = |prim: &Prim| match prim {
        Prim::Recess { rect, tint, .. }
        | Prim::Boss { rect, tint, .. }
        | Prim::Trough { rect, tint, .. }
        | Prim::Field { rect, tint, .. } => Some((*rect, *tint)),
        _ => None,
    };
    let at_rest = prims(&p, &ctx);
    assert!(at_rest.iter().all(|pr| carve(pr).is_none_or(|(_, t)| t.is_none())), "nothing is tinted at rest");
    assert_eq!(label_of(&p, "Count"), ParametersBg::LABEL);

    let (x, y, w, h) = p.get_param_rects()[1];
    p.on_cursor_moved(x + w * 0.5, y + h * 0.5, &mut ctx);
    assert_eq!(p.hover_row, Some(1));
    assert_eq!(label_of(&p, "Count"), ParametersBg::LABEL_HOVER, "the hovered row's label lifts");
    assert_eq!(label_of(&p, "Name"), ParametersBg::LABEL, "and only that row's");

    let hovered = prims(&p, &ctx);
    assert_eq!(hovered.len(), at_rest.len(), "a hover adds no geometry");
    let mut lit = 0;
    for (a, b) in at_rest.iter().zip(&hovered) {
        if a == b {
            continue;
        }
        let ((ra, ta), (rb, tb)) = (carve(a).expect("only carves change"), carve(b).expect("only carves change"));
        assert_eq!(ra, rb, "a carve keeps its place");
        assert_eq!((ta, tb), (None, Some(ParametersBg::HOVER_TINT)), "the change is the hover tint");
        assert!(rb.y >= y - 0.5 && rb.y + rb.height <= y + h + 0.5, "and only inside the hovered row: {rb:?}");
        lit += 1;
    }
    assert!(lit > 0, "the hovered row's carves light");
}

/// The label lift is the pane's rule, not the control's: a spinbox row,
/// which draws no hover of its own under the relief style, lifts exactly
/// as a slider row does, and a section header never does.
#[test]
fn every_control_kind_hovers_by_its_row() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[
        ("Shape", "", "section"),
        ("Count", "3", "spinbox:0:10"),
        ("Mode", "A", "choice:A,B"),
        ("Size", "0.5", "slider:0:1"),
    ]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 400.0);
    let rects = p.get_param_rects();
    for i in 1..4 {
        let (x, y, w, h) = rects[i];
        p.on_cursor_moved(x + w * 0.5, y + h * 0.5, &mut ctx);
        assert_eq!(p.hover_row, Some(i), "row {i} under the pointer");
    }
    let (x, y, w, h) = rects[0];
    p.on_cursor_moved(x + w * 0.5, y + h * 0.5, &mut ctx);
    assert_eq!(p.hover_row, None, "a header is not a hoverable row");
}

/// A gesture the pane acquired stays the pane's: rows travelling under
/// the pointer mid-gesture must not hand the wheel to the slider that
/// arrives there (the Alt+D settings-tab leak, 2026-09-20). A NEW gesture
/// over the same slider still adjusts it.
#[test]
fn pane_owned_scroll_gesture_is_not_captured_by_a_slider_sliding_under_the_pointer() {
    let rows: Vec<(String, String, String)> = (0..30)
        .map(|i| (format!("P{i}"), "1.00".to_string(), "slider:0:2".to_string()))
        .collect();
    let fresh = || {
        let mut p = ParametersBg::new();
        ParamController::set_display_params(&mut *p, &rows);
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 200.0);
        p
    };
    // A point on a slider's band: row 1's rect, below the label strip.
    let band_point = |p: &Adapted<ParametersBg>| {
        let r = p.get_param_rects()[1];
        (r.0 + r.2 * 0.5, r.1 + r.3 * 0.7)
    };
    let values = |p: &Adapted<ParametersBg>| -> Vec<String> { p.display_params.iter().map(|d| d.1.clone()).collect() };

    // Control: a new gesture ON the band adjusts the slider (the point is a real hit).
    let mut ctx = UiContext::new();
    let mut p = fresh();
    let (bx, by) = band_point(&p);
    ctx.scroll_gesture_new = true;
    ctx.scroll_initiate_widget_id = None;
    p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), bx, by, &mut ctx);
    assert_ne!(values(&p)[1], "1.00", "a new gesture on the band adjusts the slider");

    // The case: the gesture starts on the pane (dead space), the pane owns it...
    let mut ctx = UiContext::new();
    let mut p = fresh();
    ctx.scroll_gesture_new = true;
    ctx.scroll_initiate_widget_id = None;
    p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), 2.0, 2.0, &mut ctx);
    // (The scroll itself is animated by scroll_motion over later ticks, so
    // ownership — set only on the pane-scroll path — is the witness.)
    assert_eq!(ctx.scroll_initiate_widget_id, Some(p.base().id()), "the pane owns the gesture");
    // ...and the same gesture continuing over a band adjusts nothing.
    let (bx, by) = band_point(&p);
    ctx.scroll_gesture_new = false;
    p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, -3.0), bx, by, &mut ctx);
    assert!(values(&p).iter().all(|v| v == "1.00"), "no slider took the pane's gesture: {:?}", values(&p));
    assert_eq!(ctx.scroll_initiate_widget_id, Some(p.base().id()), "the pane still owns it");
}

/// A trackpad gesture belongs to what it BEGINS on: over a slider's band
/// it turns the slider — in a pane that overflows and in one that fits
/// alike — and keeps turning it as the pointer drifts; beginning on the
/// label column it scrolls the pane, and stays the pane's as bands pass
/// under the pointer. A float3's rows are three such bands. (Until
/// 2026-09-28 every finger gesture was the pane's, from anywhere, and a
/// slider could be turned by a wheel notch but not by a trackpad.)
#[test]
fn a_finger_gesture_belongs_to_the_control_it_begins_on() {
    use crate::widget::{scroll_motion::set_scroll_phase, Position, ScrollPhase};
    let rows = |n: usize| -> Vec<(String, String, String)> {
        (0..n).map(|i| (format!("P{i}"), "1.00".to_string(), "slider:0:2".to_string())).collect()
    };
    let panel = |n: usize, h: f32| {
        let mut p = ParametersBg::new();
        ParamController::set_display_params(&mut *p, &rows(n));
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, h);
        p
    };
    // A point on row 1's band: the slider's own rect, below its label strip.
    let band_point = |p: &Adapted<ParametersBg>| {
        let s = p.sliders[1].as_ref().unwrap();
        let (sx, sy, sw, sh) = s.rect();
        let ty = s.label_strip();
        (sx + sw * 0.3, sy + ty + (sh - ty) * 0.5)
    };
    let values = |p: &Adapted<ParametersBg>| -> Vec<String> { p.display_params.iter().map(|d| d.1.clone()).collect() };
    let finger = |dy: f64| MouseScrollDelta::PixelDelta(Position { x: 0.0, y: dy });

    for (n, h) in [(30, 200.0), (3, 600.0)] {
        // Beginning ON the band: the slider turns, and owns the gesture.
        let mut ctx = UiContext::new();
        let mut p = panel(n, h);
        let (bx, by) = band_point(&p);
        ctx.scroll_gesture_new = true;
        set_scroll_phase(ScrollPhase::Finger);
        assert!(p.mouse_wheel(&finger(-60.0), bx, by, &mut ctx));
        let slider_id = p.sliders[1].as_ref().unwrap().base().id();
        assert_ne!(values(&p)[1], "1.00", "a finger gesture on the band turns the slider ({n} rows)");
        assert_eq!(ctx.scroll_initiate_widget_id, Some(slider_id), "the slider owns the gesture");
        assert_eq!(p.scroll_y, 0.0, "the pane did not scroll");
        // Still its gesture with the pointer drifted onto the next band.
        let turned = values(&p)[1].clone();
        let (_, sy2, _, sh2) = p.sliders[2].as_ref().unwrap().rect();
        ctx.scroll_gesture_new = false;
        assert!(p.mouse_wheel(&finger(-60.0), bx, sy2 + sh2 * 0.7, &mut ctx));
        assert_ne!(values(&p)[1], turned, "latched: the first slider keeps turning");
        assert_eq!(values(&p)[2], "1.00", "the band under the drifted pointer holds");
    }

    // Beginning on the label column: the pane scrolls and owns the
    // gesture, and a band passing under the pointer adjusts nothing.
    let mut ctx = UiContext::new();
    let mut p = panel(30, 200.0);
    let (_, by) = band_point(&p);
    ctx.scroll_gesture_new = true;
    set_scroll_phase(ScrollPhase::Finger);
    p.mouse_wheel(&finger(-30.0), ROW_X_INSET + 2.0, by, &mut ctx);
    assert_eq!(ctx.scroll_initiate_widget_id, Some(p.base().id()), "the pane took the gesture");
    assert!(p.scroll_y > 0.0, "the pane scrolled (finger tracks 1:1): {}", p.scroll_y);
    let (bx, by) = band_point(&p);
    ctx.scroll_gesture_new = false;
    p.mouse_wheel(&finger(-60.0), bx, by, &mut ctx);
    assert!(values(&p).iter().all(|v| v == "1.00"), "a pane-owned gesture turns nothing: {:?}", values(&p));

    // A float3's rows are three bands: a finger gesture on the Y row
    // turns Y alone.
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Name", "x", "text"), ("Offset", "0.00:0.00:0.00", "float3:-1:1")]);
    let f = p.float3s[1].as_ref().unwrap();
    let (rx, ry, rw, rh) = f.get_row_rects()[1];
    let chrome = crate::widget::input::slider::Slider::readout_chrome();
    let (bx, by) = (rx + (rw - chrome) * 0.5, ry + rh * 0.5);
    ctx.scroll_gesture_new = true;
    set_scroll_phase(ScrollPhase::Finger);
    assert!(p.mouse_wheel(&finger(-120.0), bx, by, &mut ctx));
    let parts: Vec<f32> = p.display_params[1].1.split(':').map(|v| v.parse().unwrap()).collect();
    assert_eq!((parts[0], parts[2]), (0.0, 0.0), "X and Z hold: {parts:?}");
    assert_ne!(parts[1], 0.0, "Y turned: {parts:?}");
    assert_eq!(p.scroll_y, 0.0);
    set_scroll_phase(ScrollPhase::Wheel);
}

/// The same rule on a spinbox, whose zone is its row: a gesture that
/// begins on a spinbox row steps the spinbox — a finger's fractional notches accumulating
/// into whole steps — and stays the spinbox's, while a gesture the pane
/// acquired still scrolls past it. A wheel notch on the row steps it too,
/// however the scroll factor scales the notch.
#[test]
fn a_gesture_beginning_on_a_spinbox_row_steps_it_trackpad_included() {
    use crate::widget::{scroll_motion::set_scroll_phase, Position, ScrollPhase};
    let rows = |n: usize| -> Vec<(String, String, String)> {
        (0..n)
            .map(|i| if i == 1 { ("Count".to_string(), "10".to_string(), "spinbox:0:100:1".to_string()) } else { (format!("P{i}"), "1.00".to_string(), "slider:0:2".to_string()) })
            .collect()
    };
    let panel = |n: usize, h: f32| {
        let mut p = ParametersBg::new();
        ParamController::set_display_params(&mut *p, &rows(n));
        WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, h);
        p
    };
    let row_point = |p: &Adapted<ParametersBg>| {
        let r = p.get_param_rects()[1];
        (r.0 + r.2 * 0.3, r.1 + r.3 * 0.5)
    };
    let count = |p: &Adapted<ParametersBg>| p.display_params[1].1.clone();

    // A finger gesture beginning on the row: 60 px of travel is one step,
    // the spinbox owns the gesture, and the pane does not scroll.
    let mut ctx = UiContext::new();
    let mut p = panel(30, 200.0);
    let (x, y) = row_point(&p);
    ctx.scroll_gesture_new = true;
    set_scroll_phase(ScrollPhase::Finger);
    assert!(p.mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: 0.0, y: 30.0 }), x, y, &mut ctx));
    assert_eq!(count(&p), "10", "half a notch: no step yet");
    let sb_id = p.spinboxes[1].as_ref().unwrap().base().id();
    assert_eq!(ctx.scroll_initiate_widget_id, Some(sb_id), "the spinbox owns the gesture");
    ctx.scroll_gesture_new = false;
    assert!(p.mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: 0.0, y: 30.0 }), x, y, &mut ctx));
    assert_eq!(count(&p), "11", "the second half completes the notch");
    assert_eq!(p.scroll_y, 0.0, "the pane did not scroll");
    // Still its gesture when the pointer drifts onto the row below.
    let below = p.get_param_rects()[2];
    assert!(p.mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: 0.0, y: -120.0 }), x, below.1 + below.3 * 0.5, &mut ctx));
    assert_eq!(count(&p), "9", "two notches down, latched");
    assert_eq!(p.display_params[2].1, "1.00", "the slider under the drifted pointer holds");

    // A gesture the pane acquired scrolls past the row without stepping it.
    let mut ctx = UiContext::new();
    let mut p = panel(30, 200.0);
    ctx.scroll_gesture_new = true;
    set_scroll_phase(ScrollPhase::Finger);
    p.mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: 0.0, y: -30.0 }), 2.0, 2.0, &mut ctx);
    assert_eq!(ctx.scroll_initiate_widget_id, Some(p.base().id()), "the pane took the gesture");
    let (x, y) = row_point(&p);
    ctx.scroll_gesture_new = false;
    p.mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: 0.0, y: -120.0 }), x, y, &mut ctx);
    assert_eq!(count(&p), "10", "a pane-owned gesture never steps a spinbox");
    assert_eq!(ctx.scroll_initiate_widget_id, Some(p.base().id()));

    // A scaled wheel notch (scroll_factor 0.5) still steps, over two notches.
    let mut ctx = UiContext::new();
    let mut p = panel(30, 200.0);
    let (x, y) = row_point(&p);
    ctx.scroll_gesture_new = true;
    set_scroll_phase(ScrollPhase::Wheel);
    assert!(p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 0.5), x, y, &mut ctx));
    assert_eq!(count(&p), "10");
    ctx.scroll_gesture_new = false;
    assert!(p.mouse_wheel(&MouseScrollDelta::LineDelta(0.0, 0.5), x, y, &mut ctx));
    assert_eq!(count(&p), "11", "a half-notch factor is not truncated to nothing");
}
