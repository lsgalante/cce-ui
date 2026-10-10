//! What each row type builds and draws, and how the pane lays out its labels.

use super::*;

/// The textpick text-row variant: a TextBox AND a menu-button Dropdown
/// share the row — the picker takes a right-edge sliver, its options come
/// from the type string, and a plain text row builds no picker.
/// A textpick row's picker centres its arrow, and every other dropdown
/// row's arrow stands in the same column: the picker is a trigger's
/// arrow slot (`dropdown::arrow_slot`).
#[test]
fn a_pickers_arrow_lines_up_with_a_dropdowns() {
    let p = panel_with(&[
        ("Name", "mass", "textpick:Norm,UV,Pos,Col"),
        ("Type", "Float", "choice:Float,Float3,Int"),
    ]);
    let arrow = |i: usize| {
        let d = p.choices[i].as_ref().unwrap();
        let (x, y, w, h) = d.rect();
        d.inner().arrow_rect(crate::scene::layout::Rect { x, y, width: w, height: h }).x
    };
    assert!((arrow(0) - arrow(1)).abs() < 1e-3, "picker {} vs dropdown {}", arrow(0), arrow(1));
}

/// The pane draws its controls' glyphs: a dropdown's and a picker's
/// arrow, a spinbox's minus and plus. The pane paints its rows' chrome
/// itself and took only their TEXT, so when the symbols became glyphs
/// (2026-10-05) every control in the pane lost them. Skipped where the
/// icon set is not checked out (no glyph uploads, nothing to name).
#[test]
fn the_pane_draws_its_controls_glyphs() {
    if !std::path::Path::new(&crate::icons_dir()).join("chevron-down.svg").is_file() {
        eprintln!("skipped: no icon set");
        return;
    }
    let p = panel_with(&[
        ("Name", "mass", "textpick:Norm,UV,Pos,Col"),
        ("Type", "Float", "choice:Float,Float3,Int"),
        ("Count", "3", "spinbox:0:10:1"),
    ]);
    let ui = UiContext::new();
    let mut pc = PaintCtx::new();
    let r = Rect { x: 0.0, y: 0.0, width: 300.0, height: 400.0 };
    Paint::paint_ui(&*p, &ui, r, &mut pc);
    let names: Vec<String> = pc
        .finish()
        .items
        .into_iter()
        .filter_map(|item| match item.prim {
            crate::scene::paint::Prim::Image { image, .. } => crate::icon_source(image).map(|(n, _, _)| n),
            _ => None,
        })
        .collect();
    let count = |n: &str| names.iter().filter(|m| *m == n).count();
    assert_eq!(count("chevron-down"), 2, "the picker's and the dropdown's arrows: {names:?}");
    assert_eq!(count("minus"), 1, "the spinbox's minus: {names:?}");
    assert_eq!(count("plus"), 1, "the spinbox's plus: {names:?}");
}

#[test]
fn textpick_rows_carry_a_picker() {
    let p = panel_with(&[
        ("Attribute Name", "mass", "textpick:Norm,UV,Pos,Col"),
        ("Plain", "x", "text"),
        ("Empty", "y", "textpick:"),
    ]);
    assert!(p.texts[0].is_some(), "textpick keeps its TextBox");
    let d = p.choices[0].as_ref().expect("textpick builds the picker");
    assert_eq!(d.options, ["Norm", "UV", "Pos", "Col"]);
    assert!(d.custom_display_text.is_some(), "menu-button mode");
    assert!(p.choices[1].is_none(), "plain text has no picker");
    assert!(p.texts[2].is_some() && p.choices[2].is_none(), "no options, no picker");

    // Layout: the picker is the field's right end — the box stops at
    // the seam, square there, and the picker reaches the field's outer
    // edge on the right, top and bottom, as a dropdown trigger does.
    let (tx, ty, tw, th) = p.texts[0].as_ref().unwrap().rect();
    let (dx, dy, dw, dh) = p.choices[0].as_ref().unwrap().rect();
    // The box ends where the picker begins, at the seam; the picker
    // is a wall wider than PICK_W, the wall being the lip its face
    // rises out of at the seam, so the face is PICK_W wide.
    let label = p.texts[0].as_ref().unwrap().label_strip();
    let depth = crate::layout::bevel_width().min((th - label) * 0.2);
    assert!((dw - (PICK_W + depth)).abs() < 1e-3, "the picker is PICK_W and a wall: {dw}");
    assert!((dx - (tx + tw)).abs() < 1e-3, "the picker begins where the box ends: {dx} vs {}", tx + tw);
    assert!(d.center_arrow, "the picker's arrow stands in its middle, its padding even");
    assert_eq!((dy, dy + dh), (ty + label, ty + th), "the picker spans the field's band, edge to edge");
    assert!(p.texts[0].as_ref().unwrap().inner().joined_right, "the box is square at the seam");
    assert!(!p.texts[1].as_ref().unwrap().inner().joined_right, "a plain text row is not");
    // ONE field: one outline round the box and the picker, its seam at
    // the picker — not a well and a trough each turning its own corner
    // there.
    let rr = crate::layout::textbox_corner_radius();
    let fields = p.inner().fields();
    assert_eq!(fields.len(), 1, "{fields:?}");
    let f = fields[0].rect;
    assert_eq!((f.x, f.y, f.x + f.width, f.y + f.height), (tx, dy, dx + dw, dy + dh), "the field spans box and picker");
    assert_eq!(fields[0].radii, (rr, rr, rr, rr), "its own radius at all four corners");
    assert_eq!(fields[0].run_span(), Some((dx, dx + dw)), "the run is the picker, to the field's end");
    assert!(p.inner().reliefs().iter().all(|r| !(r.0 == tx && r.2 == tw)), "nor the box a well of its own");

    // Both text variants lay out at the same row height.
    assert_eq!(p.inner().row_height(0), p.inner().row_height(1));

    // The menu anchors off the WHOLE field: the popover spans at least
    // the box width and hangs below it, not off the button sliver.
    let anchor = p.choices[0].as_ref().unwrap().popover_anchor.expect("anchor set");
    assert_eq!(anchor.x, tx);
    assert!((anchor.width - (dx + dw - tx)).abs() < 1e-3, "the whole field");
    let d = p.choices[0].as_ref().unwrap();
    let (px_, py_, pw, _ph) = d.popover_geom(crate::scene::layout::Rect {
        x: dx, y: dy, width: dw, height: dh,
    });
    assert_eq!(px_, tx, "menu left-aligns with the box");
    assert!(pw >= tw, "menu at least as wide as the box");
    assert!(py_ >= ty + th - 1.0, "menu hangs below the box");
}

/// A dropdown trigger's edge is a field's run: its own paint and the
/// pane's re-emission both draw a field that is all run, so its edge
/// matches the picker at the end of a text row and the -/+ run of a
/// spinbox. Never a trough, whose outer half differed.
#[test]
fn a_dropdown_trigger_wears_the_runs_edge() {
    use crate::scene::paint::{PaintCtx, Prim};
    if !crate::layout::control_relief() {
        return;
    }
    let p = panel_with(&[("Mode", "b", "choice:a,b,c")]);
    let d = p.choices[0].as_ref().expect("a dropdown");
    let (x, y, w, h) = d.rect();
    let ty = d.label_strip();
    let fields = p.inner().fields();
    assert_eq!(fields.len(), 1, "{fields:?}");
    assert_eq!(fields[0].rect, Rect { x, y: y + ty, width: w, height: h - ty }, "on the trigger's band");
    assert!(!fields[0].has_well(), "all run, no well");
    let mut pc = PaintCtx::new();
    Paint::paint(d.inner(), Rect { x, y: y + ty, width: w, height: h - ty }, &mut pc);
    let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
    assert!(prims.iter().any(|p| matches!(p, Prim::Field { rect, split, .. } if *split < rect.x - 100.0)), "{prims:?}");
    assert!(!prims.iter().any(|p| matches!(p, Prim::Trough { .. })), "no trough");
}

#[test]
fn param_controller_roundtrip_and_row_widgets() {
    let p = panel_with(&[
        ("Size", "1.00", "slider:0:2"),
        ("Mode", "b", "choice:a,b,c"),
        ("On", "true", "checkbox"),
    ]);
    assert_eq!(ParamController::node_params(&*p).len(), 3);
    assert!(p.sliders[0].is_some() && p.choices[1].is_some() && p.toggles[2].is_some());
    // Rows were laid out from the cached rect: the slider's control rect
    // is the row less the label column when the labels are inline (the
    // default), the whole row when they are stacked.
    let lw = p.inner().label_col_w();
    if p.inner().inline_labels {
        assert!(lw > 0.0, "inline labels reserve a column");
        let labels = p.inner().own_text_labels();
        let size = labels.iter().find(|l| l.text == "Size").expect("the pane draws the inline label");
        assert_eq!(size.x, ROW_X_INSET, "the label sits at the row's left edge");
    } else {
        assert_eq!(lw, 0.0);
    }
    let (sx, _, sw, _) = p.sliders[0].as_ref().unwrap().rect();
    assert_eq!(
        (sx, sw),
        (ROW_X_INSET + lw, 300.0 - 2.0 * ROW_X_INSET - lw),
        "control rect derives from the assigned rect and the label column"
    );
}

/// A pane too narrow for a label column stacks its labels above the
/// controls, and takes them back into the column when it widens: the
/// controls are relabelled, the column goes, the rows grow by the label
/// strip. The threshold is the control's width, not the pane's.
#[test]
fn a_narrow_pane_stacks_its_labels_and_a_wide_one_puts_them_back() {
    let mut p = panel_with(&[("Size", "1.00", "slider:0:2"), ("Mode", "b", "choice:a,b,c"), ("On", "true", "checkbox")]);
    if !p.inner().inline_pref {
        return; // a stacked preference has nothing to re-flow
    }
    let strip = crate::layout::control_label_strip();
    // The COLUMN label sits exactly at the row's left edge; a control's
    // own detached label is inset from the control's edge, so the two
    // are told apart by x.
    let labels_in_column = |p: &Adapted<ParametersBg>| {
        p.inner().own_text_labels().iter().any(|l| l.text == "Size" && l.x == ROW_X_INSET)
    };

    // 300 wide: inline. The slider carries no label; the pane draws it.
    assert!(p.inner().inline_labels);
    let col = p.inner().label_col_w();
    assert!(col > 0.0);
    assert!(labels_in_column(&p));
    assert!(p.sliders[0].as_ref().unwrap().base().label.is_none());
    let (_, _, sw_inline, _) = p.sliders[0].as_ref().unwrap().rect();
    let h_inline = p.get_param_rects()[0].3;
    // The threshold is the slider's TRACK: its rect less the readout.
    let chrome = crate::widget::input::slider::Slider::readout_chrome();
    assert!(sw_inline - chrome >= ParametersBg::MIN_INLINE_TRACK_W, "the fixture starts with room: {sw_inline}");

    // Narrow it until the TRACK would fall under the minimum: stacked,
    // though the control's rect is still well over it.
    let narrow = 2.0 * ROW_X_INSET + col + chrome + ParametersBg::MIN_INLINE_TRACK_W - 1.0;
    WidgetHost::set_rect(&mut p, 0.0, 0.0, narrow, 400.0);
    assert!(!p.inner().inline_labels, "under the minimum the labels stack");
    assert_eq!(p.inner().label_col_w(), 0.0, "no column");
    assert!(!labels_in_column(&p), "the pane draws no column label");
    assert_eq!(p.sliders[0].as_ref().unwrap().base().label.as_deref(), Some("Size"), "the slider carries its label");
    assert_eq!(p.choices[1].as_ref().unwrap().base().label.as_deref(), Some("Mode"));
    let (sx, _, sw, _) = p.sliders[0].as_ref().unwrap().rect();
    assert_eq!((sx, sw), (ROW_X_INSET, narrow - 2.0 * ROW_X_INSET), "the control spans the row");
    // The row grows for the label band — by the strip, less the floor
    // the inline row's height table clamps at.
    let h_stacked = p.get_param_rects()[0].3;
    assert!(h_stacked > h_inline && h_stacked <= h_inline + strip, "inline {h_inline}, stacked {h_stacked}, strip {strip}");
    // The toggle carried its label all along.
    assert_eq!(p.toggles[2].as_ref().unwrap().base().label.as_deref(), Some("On"));

    // One pixel wider than the minimum: inline again, labels off the controls.
    WidgetHost::set_rect(&mut p, 0.0, 0.0, narrow + 1.0, 400.0);
    assert!(p.inner().inline_labels, "at the minimum the column comes back");
    assert!(labels_in_column(&p));
    assert!(p.sliders[0].as_ref().unwrap().base().label.is_none(), "the label left the slider");
    assert!(p.choices[1].as_ref().unwrap().base().label.is_none());
    assert_eq!(p.get_param_rects()[0].3, h_inline);
    let (sx, _, sw, _) = p.sliders[0].as_ref().unwrap().rect();
    assert_eq!((sx, sw), (ROW_X_INSET + col, narrow + 1.0 - 2.0 * ROW_X_INSET - col));

    // A rebuild under a narrow rect builds stacked from the start.
    let rows: Vec<(String, String, String)> = vec![("Size".into(), "1.00".into(), "slider:0:2".into()), ("Width".into(), "2.00".into(), "slider:0:2".into())];
    WidgetHost::set_rect(&mut p, 0.0, 0.0, narrow, 400.0);
    ParamController::set_display_params(&mut *p, &rows);
    assert!(!p.inner().inline_labels);
    assert_eq!(p.sliders[1].as_ref().unwrap().base().label.as_deref(), Some("Width"));

    // The shortest track decides. At one width: a pane of controls with
    // no track keeps its column, a slider's readout costs it the
    // column, and a float3's axis letters cost it sooner still.
    let at = |rows: &[(&str, &str, &str)], width: f32| {
        let mut p = panel_with(rows);
        WidgetHost::set_rect(&mut p, 0.0, 0.0, width, 400.0);
        p.inner().inline_labels
    };
    let spin = [("Size", "3", "spinbox:0:10")];
    let slider = [("Size", "1.00", "slider:0:2")];
    let float3 = [("Size", "0:0:0", "float3:-1:1")];
    let base = 2.0 * ROW_X_INSET + col + ParametersBg::MIN_INLINE_TRACK_W;
    assert!(at(&spin, base), "a spinbox is measured whole");
    assert!(!at(&slider, base), "a slider at that width has a track {chrome} short");
    assert!(at(&slider, base + chrome));
    assert!(!at(&float3, base + chrome), "a float3 spends an axis column too");
    assert!(at(&float3, base + chrome + crate::widget::display::float3::AXIS_W));

    // Collapsing the section that holds the only slider re-decides: the
    // rows left visible have no track, and the column comes back.
    let mut p = panel_with(&[("Count", "3", "spinbox:0:10"), ("Shape", "", "section"), ("Size", "1.00", "slider:0:2")]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, base, 400.0);
    assert!(!p.inner().inline_labels, "the slider's track is the shortest");
    p.inner_mut().set_section_collapsed("Shape", true);
    assert!(p.inner().inline_labels, "with the slider hidden the spinbox decides");
}

/// A `separator` row is a hairline a pixel tall between its neighbours,
/// the row gap either side, drawn as a rule and never hovered.
#[test]
fn a_separator_row_is_a_rule_between_rows() {
    let p = panel_with(&[("Input", "a", "text"), ("", "", SEPARATOR), ("Size", "1.00", "slider:0:2")]);
    let mut p = p;
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 400.0, 400.0);
    let rects = p.get_param_rects();
    assert_eq!(rects[1].3, 1.0, "a pixel tall");
    assert!((rects[1].1 - (rects[0].1 + rects[0].3 + ROW_GAP)).abs() < 1e-3, "the row gap above");
    assert!((rects[2].1 - (rects[1].1 + 1.0 + ROW_GAP)).abs() < 1e-3, "and below");
    let rule = p.plain_quads().into_iter().find(|q| q.3 == 1.0 && (q.1 - rects[1].1).abs() < 1e-3);
    assert!(rule.is_some_and(|q| q.2 > 300.0), "a rule across the row");
    assert_eq!(p.hoverable_row_at(rects[1].0 + 50.0, rects[1].1 + 0.5), None, "nothing to hover");
    let near = |l: &TextLabel| (l.y - rects[1].1).abs() < 6.0;
    assert!(!p.own_text_labels().iter().any(near), "and no label: a fallback `name: value` once drew ':'");
}

/// A `soft` slider or float row builds its sliders with a soft range,
/// and the pane writes back the value the slider holds — past the
/// row's declared range once a typed value has widened it — rather
/// than re-reading the slider's fraction over the declared range.
#[test]
fn a_soft_row_writes_back_what_its_slider_holds() {
    let mut p = panel_with(&[("V", "1.00", "slider:-10:10:2:soft"), ("W", "1:2", "float2:-10:10:soft"), ("H", "1.00", "slider:-10:10")]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 500.0, 400.0);
    assert!(is_soft_row("slider:-10:10:2:soft") && is_soft_row("float3:-1:1:trackball:soft"));
    assert!(!is_soft_row("slider:-10:10") && !is_soft_row("float3:-1:1:trackball"));
    assert_eq!(slider_decimals("slider:-10:10:2:soft"), 2);
    {
        let s = p.sliders[0].as_mut().unwrap();
        s.set_range(-10.0, 500.0);
        s.set_scaled_value(500.0);
    }
    p.inner_mut().focused_param = Some(0);
    p.inner_mut().commit_and_unfocus();
    assert_eq!(p.display_params[0].1, "500.00");
    assert_eq!(p.display_params[2].1, "1.00", "the hard row is untouched");
}

/// `float2` and `float4` rows are the float3 group with two or four
/// rows (X Y, X Y Z W): that many sliders over the row's range, laid out
/// and sized for that many, read from and written back as that many
/// components — and no trackball, which a direction of three numbers
/// is the only thing to have.
#[test]
fn float2_and_float4_rows_are_the_group_with_two_or_four_sliders() {
    let mut p = panel_with(&[
        ("Two", "1.00:-2.00", "float2:-10:10"),
        ("Four", "1:2:3:4", "float4:-10:10:trackball"),
        ("Three", "0:0:0", "float3:-10:10"),
    ]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 500.0, 600.0);
    let two = p.float3s[0].as_ref().expect("a float2 row is a group");
    let four = p.float3s[1].as_ref().expect("a float4 row is a group");
    assert_eq!((two.components(), four.components()), (2, 4));
    assert_eq!(two.get_row_rects().len(), 2);
    assert_eq!(four.get_row_rects().len(), 4);
    assert!(!four.has_trackball(), "four numbers have no ball");
    assert_eq!(two.value_string(), "1.00:-2.00");
    assert_eq!(four.value_string(), "1.00:2.00:3.00:4.00");
    assert!(
        Float3::preferred_height_for(false, 4) > Float3::preferred_height_for(false, 3)
            && Float3::preferred_height_for(false, 3) > Float3::preferred_height_for(false, 2)
    );
    assert_eq!(Float3::preferred_height_for(false, 3), Float3::preferred_height(false));

    // A new value for the row reaches the group, and a group whose
    // row changed width takes the new width.
    let rows: Vec<(String, String, String)> = vec![
        ("Two".into(), "3:4".into(), "float2:-10:10".into()),
        ("Four".into(), "5:6:7".into(), "float3:-10:10".into()),
        ("Three".into(), "0:0:0".into(), "float3:-10:10".into()),
    ];
    ParamController::set_display_params(&mut *p, &rows);
    assert_eq!(p.float3s[0].as_ref().unwrap().value_string(), "3.00:4.00");
    let f = p.float3s[1].as_ref().unwrap();
    assert_eq!((f.components(), f.value_string()), (3, "5.00:6.00:7.00".to_string()));
}

/// A `float3:lo:hi:trackball` row builds its group with the ball, counts
/// the ball as chrome when the label layout is decided, paints it
/// through the scene path, and a press-and-drag on it through the
/// pane's own drag protocol turns the row's value.
#[test]
fn a_trackball_row_turns_its_value_through_the_pane() {
    use crate::scene::paint::Prim;
    let mut p = panel_with(&[("Name", "x", "text"), ("Pull", "0.000:0.000:2.000", "float3:-10:10:trackball"), ("Plain", "0:0:0", "float3:-10:10")]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 500.0, 400.0);
    assert!(p.float3s[1].as_ref().unwrap().has_trackball());
    assert!(!p.float3s[2].as_ref().unwrap().has_trackball());
    assert_eq!(
        ParametersBg::control_chrome("float3:-10:10:trackball") - ParametersBg::control_chrome("float3:-10:10"),
        Float3::trackball_chrome(),
        "the ball is chrome: the tracks are what is left of it"
    );

    let mut pc = crate::scene::paint::PaintCtx::new();
    p.inner().paint_scene_rows(&mut pc);
    let spheres = pc.finish().items.into_iter().filter(|i| matches!(i.prim, Prim::Sphere { .. })).count();
    assert_eq!(spheres, 1, "one ball, for the one row that asks");

    let (cx, cy, r) = p.float3s[1].as_ref().unwrap().ball_circle().unwrap();
    let rect = Rect { x: 0.0, y: 0.0, width: 500.0, height: 400.0 };
    Input::drag_begin(p.inner_mut(), cx, cy, rect);
    assert!(Input::is_dragging(p.inner()), "the pane is dragging the row");
    assert!(Input::drag_update(p.inner_mut(), cx + r * std::f32::consts::FRAC_PI_2, cy, rect));
    Input::drag_end(p.inner_mut());
    let v: Vec<f32> = p.display_params[1].1.split(':').map(|c| c.parse().unwrap()).collect();
    assert!((v[0] - 2.0).abs() < 2e-3 && v[1].abs() < 2e-3 && v[2].abs() < 2e-3, "a quarter turn right: {v:?}");
    assert_eq!(p.display_params[2].1, "0:0:0", "the plain row is untouched");

    // The pane's camera reaches every ball it has, and the ones built
    // after it; the same view again changes nothing.
    let camera = [[0.0, 0.0, -1.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
    assert!(p.inner_mut().set_trackball_view(camera));
    assert_eq!(p.float3s[1].as_ref().unwrap().view(), camera);
    assert!(!p.inner_mut().set_trackball_view(camera), "unchanged");
    let rebuilt: Vec<(String, String, String)> = vec![("Other".into(), "0.000:0.000:1.000".into(), "float3:-10:10:trackball".into())];
    ParamController::set_display_params(&mut *p, &rebuilt);
    assert_eq!(p.float3s[0].as_ref().unwrap().view(), camera, "a rebuilt row starts from the pane's view");

    // A trackpad gesture beginning on the ball rolls it — the pane does
    // not scroll — and one beginning on the label column is the pane's.
    use crate::widget::{scroll_motion::set_scroll_phase, Position, ScrollPhase};
    let rows: Vec<(String, String, String)> = (0..12)
        .map(|i| (format!("P{i}"), "0.000:0.000:2.000".to_string(), "float3:-10:10:trackball".to_string()))
        .collect();
    let mut p = ParametersBg::new();
    ParamController::set_display_params(&mut *p, &rows);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 500.0, 300.0);
    assert!(p.content_h > 300.0, "the pane overflows, so it has somewhere to scroll");
    let (cx, cy, _) = p.float3s[1].as_ref().unwrap().ball_circle().unwrap();
    let mut ctx = UiContext::new();
    ctx.scroll_gesture_new = true;
    set_scroll_phase(ScrollPhase::Finger);
    assert!(p.mouse_wheel(&MouseScrollDelta::PixelDelta(Position { x: 60.0, y: 0.0 }), cx, cy, &mut ctx));
    let v: Vec<f32> = p.display_params[1].1.split(':').map(|c| c.parse().unwrap()).collect();
    assert!(v[0] > 0.4 && v[1].abs() < 2e-3, "rolled right by a notch: {v:?}");
    assert_eq!(ctx.scroll_initiate_widget_id, Some(p.float3s[1].as_ref().unwrap().ball_id()));
    assert_eq!(p.scroll_y, 0.0, "the pane did not scroll");
    assert_eq!(p.display_params[0].1, "0.000:0.000:2.000", "the row above is untouched");
    set_scroll_phase(ScrollPhase::Wheel);
}

#[test]
fn ramp_row_builds_from_spec_and_edits_serialize_back() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Bevel Profile", "smooth;0.000:0.000,1.000:1.000", "ramp")]);
    let rp = p.ramps[0].as_ref().expect("ramp row builds a Ramp");
    assert_eq!(rp.inner().keys.len(), 2);
    assert!(rp.inner().smooth());

    // A press inside the curve area adds a key, and the row value carries
    // the re-serialized spec (what hosts poll and persist).
    let (rx, ry, rw, _) = rp.rect();
    p.mouse_input(MouseButton::Left, ElementState::Pressed, rx + rw * 0.5, ry + 40.0, &mut ctx);
    assert_eq!(p.ramps[0].as_ref().unwrap().inner().keys.len(), 3);
    let val = &ParamController::node_params(&*p)[0].1;
    assert_eq!(val.split(',').count(), 3, "spec re-serialized: {val}");
    assert!(val.starts_with("smooth;"));
}

#[test]
fn toggle_click_commits_value_and_unfocus_commits_editor() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("On", "false", "checkbox")]);
    let (cx, cy, _, ch) = p.toggles[0].as_ref().unwrap().rect();
    // Click the toggle row (presses are ungated for this widget; the panel consumes
    // every left press, so the return is true either way — assert the value flip).
    p.mouse_input(MouseButton::Left, ElementState::Pressed, cx + 6.0, cy + ch / 2.0, &mut ctx);
    p.mouse_input(MouseButton::Left, ElementState::Released, cx + 6.0, cy + ch / 2.0, &mut ctx);
    assert_eq!(ParamController::node_params(&*p)[0].1, "true");

    // Code editor: focus it via a click, type, then unfocus commits the buffer.
    let mut p = panel_with(&[("Src", "let x = 1;", "code")]);
    let rects = p.get_param_rects();
    let r = rects[0];
    p.mouse_input(MouseButton::Left, ElementState::Pressed, r.0 + 20.0, r.1 + 30.0, &mut ctx);
    assert_eq!(p.focused_param, Some(0), "code row focused");
    assert!(p.code_editor.is_some());
    p.code_editor.as_mut().unwrap().insert_text("y");
    WidgetHost::unfocus(&mut p);
    assert_eq!(p.focused_param, None);
    assert!(p.code_editor.is_none());
    assert!(ParamController::node_params(&*p)[0].1.contains('y'), "editor buffer committed on unfocus");
}

/// An inline label is cut only when it does not fit its column, by the
/// measure the column is sized with. It used to be cut to as many
/// characters as the column holds M's: in a proportional face a label
/// of narrow letters is far narrower than that many M's, so a label the
/// column was sized to hold lost its tail. On a runner whose fallback
/// face drew "M" a pixel wider, "Count" became "C..." in a column with
/// 30 px to spare.
#[test]
fn an_inline_label_is_cut_only_when_it_does_not_fit() {
    use crate::widget::display::measure_text_width;
    let narrow = "iiiiiiiiiiii";
    let long = "A parameter name far too long for any label column of this pane";
    let mut p = panel_with(&[(narrow, "x", "text"), ("Count", "3", "spinbox:0:10"), (long, "y", "text")]);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 400.0);
    let (family, size) = ParametersBg::inline_label_font();
    let avail = p.label_col_w() - ParametersBg::LABEL_GAP;
    assert!(avail > 0.0, "the pane lays its labels out inline");
    let texts: Vec<String> = p.own_text_labels().into_iter().map(|l| l.text).collect();
    for name in [narrow, "Count"] {
        if measure_text_width(name, &family, size) <= avail {
            assert!(texts.iter().any(|t| t == name), "{name:?} fits its {avail} px column and is drawn whole: {texts:?}");
        }
    }
    assert!(measure_text_width(long, &family, size) > avail, "the long label cannot fit");
    let cut = texts.iter().find(|t| t.starts_with("A p") && t.ends_with("...")).expect("the long label is cut");
    assert!(measure_text_width(cut, &family, size) <= avail, "and what is left fits: {cut:?}");
}
