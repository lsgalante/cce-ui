use super::*;
use super::code::get_cursor_line_col;
use crate::context::UiContext;

fn panel_with(params: &[(&str, &str, &str)]) -> Adapted<ParametersBg> {
    let mut p = ParametersBg::new();
    let params: Vec<(String, String, String)> = params
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    ParamController::set_display_params(&mut *p, &params);
    WidgetHost::set_rect(&mut p, 0.0, 0.0, 300.0, 400.0);
    p
}

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

fn key(k: Key, ctrl: bool, shift: bool) -> Event {
    Event::KeyInput(crate::widget::KeyEvent { state: ElementState::Pressed, logical_key: k, text: None, repeat: false, ctrl, shift, alt: false })
}

fn chr(c: &str) -> Event {
    key(Key::Character(c.into()), false, false)
}

fn ctrl(c: &str) -> Event {
    key(Key::Character(c.into()), true, false)
}

fn named(k: NamedKey) -> Event {
    key(Key::Named(k), false, false)
}

fn shift_named(k: NamedKey) -> Event {
    key(Key::Named(k), false, true)
}

/// A code row focused by a click at its first character.
fn code_panel(src: &str) -> (Adapted<ParametersBg>, UiContext) {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Src", src, "code")]);
    let r = p.get_param_rects()[0];
    let x = p.code_text_x(r);
    p.mouse_input(MouseButton::Left, ElementState::Pressed, x, r.1 + ParametersBg::CODE_TOP + 2.0, &mut ctx);
    assert_eq!(p.focused_param, Some(0));
    (p, ctx)
}

fn send(p: &mut Adapted<ParametersBg>, ctx: &mut UiContext, ev: Event) -> bool {
    WidgetHost::handle_event(p, &ev, ctx)
}

/// Typing edits the buffer and leaves the VALUE alone until ctrl+enter
/// applies it — a script would otherwise re-run on every keystroke — and
/// the row says so: dirty while they differ, clean once applied.
#[test]
fn code_edits_apply_on_ctrl_enter_not_per_keystroke() {
    let (mut p, mut ctx) = code_panel("let x = 1;");
    assert!(!p.code_is_dirty());
    send(&mut p, &mut ctx, chr("/"));
    send(&mut p, &mut ctx, chr("/"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "//let x = 1;");
    assert_eq!(ParamController::node_params(&*p)[0].1, "let x = 1;", "the value waits");
    assert!(p.code_is_dirty());
    send(&mut p, &mut ctx, key(Key::Named(NamedKey::Enter), true, false));
    assert_eq!(ParamController::node_params(&*p)[0].1, "//let x = 1;", "ctrl+enter applies");
    assert!(!p.code_is_dirty());
    assert_eq!(p.focused_param, Some(0), "and keeps editing");
    // Escape applies too, and leaves the row.
    send(&mut p, &mut ctx, chr("!"));
    send(&mut p, &mut ctx, named(NamedKey::Escape));
    assert_eq!(ParamController::node_params(&*p)[0].1, "//!let x = 1;");
    assert_eq!(p.focused_param, None);
}

/// Tab indents, shift+tab dedents, and Enter carries the indentation —
/// one level deeper after an opening brace.
#[test]
fn code_editor_indents_like_an_editor() {
    let (mut p, mut ctx) = code_panel("");
    send(&mut p, &mut ctx, chr("i"));
    send(&mut p, &mut ctx, chr("f"));
    send(&mut p, &mut ctx, chr(" "));
    send(&mut p, &mut ctx, chr("{"));
    send(&mut p, &mut ctx, named(NamedKey::Enter));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    ", "a brace opens a level");
    send(&mut p, &mut ctx, chr("x"));
    send(&mut p, &mut ctx, named(NamedKey::Enter));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    x\n    ", "the level carries");
    send(&mut p, &mut ctx, shift_named(NamedKey::Tab));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    x\n", "shift+tab dedents the line");
    send(&mut p, &mut ctx, chr("}"));
    send(&mut p, &mut ctx, named(NamedKey::Tab));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "if {\n    x\n}    ", "tab inserts a level");
    assert!(send(&mut p, &mut ctx, named(NamedKey::Tab)), "tab is the editor's: it does not fall through to the host");
}

/// shift+arrows select, typing replaces the selection, ctrl+z undoes a
/// typing run as one step and ctrl+shift+z redoes it.
#[test]
fn code_editor_selects_and_undoes() {
    let (mut p, mut ctx) = code_panel("abc\ndef");
    send(&mut p, &mut ctx, named(NamedKey::End));
    send(&mut p, &mut ctx, shift_named(NamedKey::ArrowDown));
    let e = p.code_editor.as_ref().unwrap();
    assert_eq!(e.selected_text().as_deref(), Some("\ndef"), "shift+down from the end of line 1 selects to the same column of line 2");
    send(&mut p, &mut ctx, chr("Z"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcZ", "typing replaces the selection");
    send(&mut p, &mut ctx, chr("Y"));
    send(&mut p, &mut ctx, chr("X"));
    send(&mut p, &mut ctx, ctrl("z"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abc\ndef", "one typing run, one undo");
    send(&mut p, &mut ctx, key(Key::Character("z".into()), true, true));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcZYX", "and one redo");
    send(&mut p, &mut ctx, key(Key::Character("a".into()), true, true));
    assert_eq!(p.code_editor.as_ref().unwrap().selected_text().as_deref(), Some("abcZYX"), "ctrl+shift+a selects all");
    assert_eq!(ParamController::node_params(&*p)[0].1, "abc\ndef", "none of it applied yet");
}

/// The host flags an error line and the gutter number turns red for it;
/// a click lands the caret past the gutter, on the column it named.
#[test]
fn code_row_flags_an_error_line_and_clicks_land_past_the_gutter() {
    let (mut p, mut ctx) = code_panel("one\ntwo\nthree");
    p.set_code_error_line(Some(1));
    let labels = p.own_text_labels();
    let two = labels.iter().find(|l| l.text == "  2").expect("a gutter number for line 2");
    assert_eq!(two.color, [0xff, 0x80, 0x70]);
    let one = labels.iter().find(|l| l.text == "  1").unwrap();
    assert_eq!(one.color, [0x66, 0x66, 0x78]);
    let r = p.get_param_rects()[0];
    let x = p.code_text_x(r) + 2.0 * p.code_col_w();
    p.mouse_input(MouseButton::Left, ElementState::Pressed, x, r.1 + ParametersBg::CODE_TOP + 2.0 * ParametersBg::CODE_LINE_H + 2.0, &mut ctx);
    let e = p.code_editor.as_ref().unwrap();
    assert_eq!(get_cursor_line_col(&e.buffer, e.cursor_idx), (2, 2), "line 3, column 2");
    p.set_code_error_line(None);
    assert!(p.own_text_labels().iter().all(|l| l.color != [0xff, 0x80, 0x70]));
}

/// The context actions reach the editor: undo through the runner's
/// routing is the editor's own history, and select-all selects the
/// buffer. Outside an edit the pane declines them.
#[test]
fn code_editor_answers_context_actions_while_editing() {
    use crate::widget::ContextAction;
    let (mut p, mut ctx) = code_panel("abc");
    send(&mut p, &mut ctx, named(NamedKey::End));
    send(&mut p, &mut ctx, chr("d"));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcd");
    assert!(Input::context_action(&mut *p, ContextAction::Undo));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abc", "undo through the context action");
    assert!(Input::context_action(&mut *p, ContextAction::Redo));
    assert_eq!(p.code_editor.as_ref().unwrap().buffer, "abcd");
    assert!(Input::context_action(&mut *p, ContextAction::SelectAll));
    assert_eq!(p.code_editor.as_ref().unwrap().selected_text().as_deref(), Some("abcd"));
    WidgetHost::unfocus(&mut p);
    assert!(!Input::context_action(&mut *p, ContextAction::Undo), "nothing to act on once the editor is closed");
}

#[test]
fn clicking_a_section_title_collapses_its_rows() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[
        ("Transform", "", "section"),
        ("Size", "1.00", "slider:0:2"),
        ("Shading", "", "section"),
        ("On", "true", "checkbox"),
    ]);
    let expanded_h = p.get_total_content_height();
    let below_before = p.get_param_rects()[2].1;

    // Press the first section's title box.
    let r_hdr = p.get_param_rects()[0];
    let (bx, by, _, bh) = p.section_title_box(0, r_hdr);
    p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 4.0, by + bh / 2.0, &mut ctx);

    assert!(p.section_collapsed("Transform"));
    assert_eq!(p.get_param_rects()[1].3, 0.0, "the collapsed section's row has no height");
    assert!(p.get_param_rects()[2].1 < below_before, "the next section moves up");
    assert!(p.get_total_content_height() < expanded_h);
    // The row's chrome and label are gone; the header's stay.
    assert!(!p.own_text_labels().iter().any(|l| l.text.contains("Size")));
    assert!(p.own_text_labels().iter().any(|l| l.text.contains("Transform")));

    // Clicking it again restores the section.
    let r_hdr = p.get_param_rects()[0];
    let (bx, by, _, bh) = p.section_title_box(0, r_hdr);
    p.mouse_input(MouseButton::Left, ElementState::Pressed, bx + 4.0, by + bh / 2.0, &mut ctx);
    assert!(!p.section_collapsed("Transform"));
    assert_eq!(p.get_total_content_height(), expanded_h);
}

/// Both ends of a straight run, and of an arc, as points on the path.
fn run_ends(r: &(f32, f32, f32, f32)) -> [(f32, f32); 2] {
    let (x, y, w, h) = *r;
    if w > h {
        [(x, y), (x + w, y)]
    } else {
        [(x, y), (x, y + h)]
    }
}
fn arc_ends(a: &(f32, f32, f32, f32, f32)) -> [(f32, f32); 2] {
    let (cx, cy, r, a0, a1) = *a;
    [
        (cx + r * a0.cos(), cy + r * a0.sin()),
        (cx + r * a1.cos(), cy + r * a1.sin()),
    ]
}

#[test]
fn section_outline_is_one_continuous_path() {
    let p = panel_with(&[("Transform", "", "section"), ("Size", "1.00", "slider:0:2")]);
    let (title, content) = p.section_boxes().into_iter().next().expect("one section");
    let content = content.expect("the section has a content box");
    let (runs, arcs) = p.section_outline(title, content.into());

    // The tab sits flush on the body — its bottom edge is the body's top edge.
    assert_eq!(title.1 + title.3, content.1, "the tab fuses to the body");
    // Folder-tab shape: the tab's two top corners, the concave throat where its right
    // side turns onto the body's top edge, and the body's three remaining corners
    // (its top-LEFT is the tab's left side running straight through).
    assert_eq!(arcs.len(), 6);
    let (tab_right, body_top) = (title.0 + title.2, content.1);
    let throats = arcs
        .iter()
        .filter(|(cx, cy, ..)| *cx > tab_right - 0.01 && *cy < body_top)
        .count();
    assert_eq!(throats, 1, "the throat — centred out in the pocket right of the tab");

    // Every corner hands off to a straight run — no arc dangles. (Within a stroke width:
    // runs and arcs are anchored on opposite ink sides at the concave corner.)
    let ends: Vec<(f32, f32)> = runs.iter().flat_map(run_ends).collect();
    for arc in &arcs {
        for (ax, ay) in arc_ends(arc) {
            let nearest = ends
                .iter()
                .map(|(x, y)| ((x - ax).powi(2) + (y - ay).powi(2)).sqrt())
                .fold(f32::INFINITY, f32::min);
            assert!(nearest <= SECTION_BORDER_T + 0.01, "corner at ({ax}, {ay}) dangles: {nearest}");
        }
    }

    // The tab's bottom edge is open (no run along it), and the body's top edge runs
    // only right of the throat.
    let tab_bottom = title.1 + title.3 - SECTION_BORDER_T;
    let bottom_runs = runs
        .iter()
        .filter(|(_, y, w, _)| (*y - tab_bottom).abs() < 0.01 && *w > SECTION_BORDER_T)
        .count();
    assert_eq!(bottom_runs, 0, "the tab opens onto the body");
    let top_runs: Vec<&(f32, f32, f32, f32)> = runs
        .iter()
        .filter(|(_, y, w, _)| (*y - body_top).abs() < 0.01 && *w > SECTION_BORDER_T)
        .collect();
    assert_eq!(top_runs.len(), 1, "the body's top edge starts past the tab");
    assert!(top_runs[0].0 >= tab_right, "…right of the throat");

    // The left edge is ONE straight run from the tab's top corner to the body's
    // bottom corner.
    let left_runs: Vec<&(f32, f32, f32, f32)> = runs
        .iter()
        .filter(|(x, _, _, h)| (*x - title.0).abs() < 0.01 && *h > 0.0)
        .collect();
    assert_eq!(left_runs.len(), 1, "tab + body share one left side");
    assert!((left_runs[0].1 - (title.1 + SECTION_R)).abs() < 0.01);
    assert!((left_runs[0].1 + left_runs[0].3 - (content.1 + content.3 - SECTION_R)).abs() < 0.01);
}

#[test]
fn a_collapsed_section_outline_closes_on_itself() {
    let mut p = panel_with(&[("Transform", "", "section"), ("Size", "1.00", "slider:0:2")]);
    p.set_section_collapsed("Transform", true);
    let (title, content) = p.section_boxes().into_iter().next().expect("one section");
    assert!(content.is_none(), "nothing to wrap below a collapsed header");
    let (runs, arcs) = p.section_outline(title, None);
    assert_eq!(arcs.len(), 4, "a plain rounded rect");
    assert_eq!(runs.len(), 4);
}

#[test]
fn rows_pack_on_the_channel_and_sections_separate_wider() {
    let p = panel_with(&[
        ("Transform", "", "section"),
        ("Size", "1.00", "slider:0:2"),
        ("Shading", "", "section"),
        ("On", "true", "checkbox"),
    ]);
    let rects = p.get_param_rects();
    let content_bottom = |i: usize| rects[i].1 + rects[i].3 + CONTENT_BOX_PAD;
    let title_top = |i: usize| rects[i].1 - TITLE_BOX_INSET;

    // Inside a section everything packs on the channel: each tab sits flush on its
    // content box, and the rows keep one channel from the box's walls.
    for (title, content) in p.section_boxes() {
        let content = content.expect("both sections have content");
        assert_eq!(title.1 + title.3, content.1, "tab flush on its body");
    }
    let (_, content0) = p.section_boxes()[0];
    let content0 = content0.unwrap();
    assert_eq!(rects[1].1 - content0.1, CHANNEL, "row -> its box's top wall");
    assert_eq!(rects[1].0 - content0.0, CHANNEL, "row -> its box's side wall");

    // Section to section stays far wider, so the blocks still read apart.
    assert_eq!(title_top(2) - content_bottom(1), SECTION_GAP, "section -> next section");
    const { assert!(SECTION_GAP > 2.0 * CHANNEL, "sections separate wider than any channel") };
}

#[test]
fn a_collapsed_section_keeps_the_same_gap_to_the_next_one() {
    // With no content box under it, the collapsed section's bottom edge is its own title
    // box — a fixed row-pitch bump would leave a double gap here.
    let mut p = panel_with(&[
        ("Transform", "", "section"),
        ("Size", "1.00", "slider:0:2"),
        ("Shading", "", "section"),
        ("On", "true", "checkbox"),
    ]);
    p.set_section_collapsed("Transform", true);
    let rects = p.get_param_rects();
    let collapsed_bottom = rects[0].1 - TITLE_BOX_INSET + TITLE_BOX_H;
    let next_title_top = rects[2].1 - TITLE_BOX_INSET;
    assert_eq!(next_title_top - collapsed_bottom, SECTION_GAP);
}

#[test]
fn collapse_survives_a_param_rebuild_and_swallows_row_clicks() {
    let mut ctx = UiContext::new();
    let mut p = panel_with(&[("Shading", "", "section"), ("On", "false", "checkbox")]);
    p.set_section_collapsed("Shading", true);

    // A click where the toggle used to sit must not reach it.
    let (cx, cy, _, ch) = p.toggles[1].as_ref().unwrap().rect();
    p.mouse_input(MouseButton::Left, ElementState::Pressed, cx + 6.0, cy + ch / 2.0, &mut ctx);
    p.mouse_input(MouseButton::Left, ElementState::Released, cx + 6.0, cy + ch / 2.0, &mut ctx);
    assert_eq!(ParamController::node_params(&*p)[1].1, "false", "hidden row ignores clicks");

    // The host re-syncs the panel (node change): collapse is keyed by title, so it holds.
    let params: Vec<(String, String, String)> = [("Shading", "", "section"), ("On", "false", "checkbox"), ("Extra", "1", "int")]
        .iter()
        .map(|(a, b, c)| (a.to_string(), b.to_string(), c.to_string()))
        .collect();
    ParamController::set_display_params(&mut *p, &params);
    assert!(p.section_collapsed("Shading"));
    assert_eq!(p.get_param_rects()[1].3, 0.0);
}

#[test]
fn the_row_chrome_is_the_panes_and_the_plate_the_hosts() {
    let ctx = UiContext::new();
    let p = panel_with(&[("Size", "1.00", "slider:0:2")]);
    // The pane's plain quads carry the row chrome (clipped), including the slider
    // background it reads via rect()+color()...
    let plain = p.plain_quads();
    assert!(!plain.is_empty(), "row chrome in the pane's plain quads");
    // ...but NOT the panel's own PARAM_BG plate (the host draws that from color()).
    let (x, y, w, h) = WidgetHost::rect(&p);
    assert!(
        !plain.iter().any(|q| (q.0, q.1, q.2, q.3) == (x, y, w, h)),
        "panel bg plate is the host's, not the pane's"
    );
    // Per-label hatch: the walk's text prims carry the widget font and viewport bounds.
    let mut scratch = crate::scene::paint::PaintCtx::new();
    crate::scene::painter::append_widget_text(&ctx, &p, &mut scratch);
    let labels: Vec<_> = scratch
        .finish()
        .items
        .into_iter()
        .filter_map(|item| match item.prim {
            crate::scene::paint::Prim::Text { font, bounds, .. } => Some((font, bounds)),
            _ => None,
        })
        .collect();
    assert!(!labels.is_empty());
    assert!(labels.iter().all(|(font, bounds)| font.is_some() && bounds.is_some()));
}

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
