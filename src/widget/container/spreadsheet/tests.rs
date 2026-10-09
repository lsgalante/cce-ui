use super::*;
use crate::context::UiContext;
use crate::widget::WidgetHost;

fn filled(rows: usize) -> Adapted<Spreadsheet> {
    let mut s = Spreadsheet::new();
    s.set_visible(true);
    WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0); // viewport: 100 = ~4 rows of 24
    let data: Vec<Vec<String>> =
        (0..rows).map(|i| vec![format!("r{i}"), format!("v{i}")]).collect();
    SpreadsheetController::set_spreadsheet_data(&mut *s, vec!["a".into(), "b".into()], data);
    s
}

fn wide(cols: usize) -> Adapted<Spreadsheet> {
    let mut s = Spreadsheet::new();
    s.set_visible(true);
    WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0);
    let headers: Vec<String> = (0..cols).map(|i| format!("c{i}")).collect();
    let rows = vec![(0..cols).map(|i| format!("v{i}")).collect::<Vec<String>>(); 2];
    SpreadsheetController::set_spreadsheet_data(&mut *s, headers, rows);
    s
}

/// A row half scrolled out at either edge of the body draws its text cut
/// at the band — it used to draw none until it was wholly inside.
#[test]
fn a_half_scrolled_row_draws_its_text_cut_at_the_body() {
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    let mut s = filled(20);
    s.scroll_y = ROW_H * 0.5;
    let mut pc = crate::scene::paint::PaintCtx::new();
    Paint::paint(&*s, rect, &mut pc);
    let items = pc.finish().items;
    let cell = |want: &str| {
        items.iter().find_map(|item| match &item.prim {
            crate::scene::paint::Prim::Text { text, bounds, .. } if text == want => *bounds,
            _ => None,
        })
    };
    let body_top = HEADER_H;
    let body_bottom = rect.height;
    // r0 spans -12..12 of the body; r4 runs 12 px past its bottom.
    let top = cell("r0").expect("the row under the header draws its text");
    assert_eq!((top[1], top[3]), (body_top, body_top + ROW_H * 0.5), "cut at the header");
    let bottom = cell("r4").expect("the row off the bottom draws its text");
    assert_eq!(bottom[3], body_bottom, "cut at the pane's bottom");
    assert!(bottom[1] < bottom[3]);
    assert!(cell("r5").is_none(), "a row wholly out of the body draws nothing");
}

/// Each column is as wide as its content — its header with room for the
/// sort glyph, or its widest cell — and not a share of the pane: a
/// narrow table leaves the rest of a wide pane empty, a wide one scrolls.
#[test]
fn columns_fit_their_content_and_overflow_scrolls() {
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    let cw = Spreadsheet::char_w();
    let mut s = Spreadsheet::new();
    s.set_visible(true);
    WidgetHost::set_rect(&mut s, 0.0, 0.0, 200.0, 124.0);
    SpreadsheetController::set_spreadsheet_columns(
        &mut *s,
        vec!["#".into(), "a_long_header_name".into(), "x".into()],
        vec![
            SheetColumn::Int((0..1000).collect()),
            SheetColumn::Int(vec![0, 1]),
            SheetColumn::Float { values: vec![-1.5, 12.25], decimals: 4 },
        ],
    );
    let edges = (*s).col_edges();
    let w = |c: usize| edges[c + 1] - edges[c];
    // "999" is three; "#" and its sort room three too.
    assert!((w(0) - (3.0 * cw + CELL_PAD)).abs() < 0.01);
    // The header, not the 0 and 1, sizes the second.
    assert!((w(1) - ((18 + SORT_MARK_CHARS) as f32 * cw + CELL_PAD)).abs() < 0.01);
    // "-1.5000" and "12.2500" are seven.
    assert!((w(2) - (7.0 * cw + CELL_PAD)).abs() < 0.01);
    let g = (*s).hgeom(rect).expect("a run wider than 200 px scrolls");
    assert!((g.max_scroll - (edges[3] - 200.0)).abs() < 0.01);

    let fits = wide(2);
    assert!((*fits).hgeom(rect).is_none(), "2 short columns fit, no h-scroll");
    assert!(*(*fits).col_edges().last().unwrap() < 200.0, "and do not stretch to the pane");
}

/// A column's widest cell, from its values: a whole number's least or
/// greatest, a float's sign and integer digits plus its decimals, and
/// the non-finite spellings.
#[test]
fn a_columns_widest_cell_is_worked_out_from_its_values() {
    assert_eq!(SheetColumn::Int(vec![-12, 5, 300]).max_chars(), 3);
    assert_eq!(SheetColumn::Int(vec![-1200, 5]).max_chars(), 5);
    assert_eq!(SheetColumn::Float { values: vec![0.5, -0.25], decimals: 4 }.max_chars(), 7);
    assert_eq!(SheetColumn::Float { values: vec![0.5, f32::NEG_INFINITY], decimals: 1 }.max_chars(), 4);
    assert_eq!(SheetColumn::Float { values: vec![f32::NAN], decimals: 4 }.max_chars(), 3);
    assert_eq!(SheetColumn::Text(vec!["ab".into(), "abcd".into()]).max_chars(), 4);
    assert_eq!(SheetColumn::Int(vec![]).max_chars(), 0);
    for col in [SheetColumn::Int(vec![-7, 42]), SheetColumn::Float { values: vec![-3.25, 120.0], decimals: 3 }] {
        let written = (0..col.len()).map(|r| col.cell(r).chars().count()).max().unwrap();
        assert_eq!(col.max_chars(), written, "as `cell` writes them");
    }
}

/// The sort hit-test must look up columns through the scrolled origin, or
/// clicking a header would sort the column that USED to be under the pointer.
#[test]
fn header_hit_test_tracks_horizontal_scroll() {
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    let mut s = wide(6);
    assert_eq!((*s).header_col_at(10.0, 5.0, rect), Some(0));
    s.scroll_x = (*s).col_edges()[1];
    assert_eq!((*s).header_col_at(10.0, 5.0, rect), Some(1));
}

/// A horizontal wheel feeds hscroll velocity, tick integrates and decays it,
/// and the scroll clamps inside the overflow.
#[test]
fn horizontal_wheel_integrates_and_decays_through_tick() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(wide(6));

    let wheel = Event::MouseWheel {
        delta: MouseScrollDelta::LineDelta(-2.0, 0.0),
        x: 50.0,
        y: 60.0,
        local_x: 50.0,
        local_y: 60.0,
    };
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&wheel, ctx)).unwrap(), "in-rect horizontal wheel consumed");
    assert!(ctx.lend_h(s, |w, ctx| WidgetHost::tick(w, 0.016, ctx)).unwrap(), "first tick moves the h-scroll");
    let mut guard = 0;
    while ctx.lend_h(s, |w, ctx| WidgetHost::tick(w, 0.016, ctx)).unwrap() {
        guard += 1;
        assert!(guard < 1000, "h-inertia must decay to a stop");
    }
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    let max = (*ctx[s]).hgeom(rect).unwrap().max_scroll;
    assert!(ctx[s].scroll_x >= 0.0 && ctx[s].scroll_x <= max, "h-scroll stays clamped");
    assert!(ctx[s].scroll_x > 0.0, "negative dx scrolled the columns (ScrollRegion sign convention)");

    // A pane whose columns fit ignores horizontal wheels.
    let fits = ctx.insert(wide(2));
    assert!(!ctx.lend_h(fits, |w, ctx| w.handle_event(&wheel, ctx)).unwrap(), "no overflow, wheel passes through");
}

#[test]
fn wheel_velocity_integrates_and_decays_through_tick() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(filled(50));

    // A wheel over the body feeds velocity (negated delta, the ScrollRegion
    // convention: a negative line delta scrolls the view down)…
    let wheel = Event::MouseWheel {
        delta: MouseScrollDelta::LineDelta(0.0, -2.0),
        x: 50.0,
        y: 60.0,
        local_x: 50.0,
        local_y: 60.0,
    };
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&wheel, ctx)).unwrap(), "in-rect wheel consumed");

    // …which tick integrates into scroll movement and decays to a stop.
    assert!(ctx.lend_h(s, |w, ctx| WidgetHost::tick(w, 0.016, ctx)).unwrap(), "first tick moves the scroll");
    let mut guard = 0;
    while ctx.lend_h(s, |w, ctx| WidgetHost::tick(w, 0.016, ctx)).unwrap() {
        guard += 1;
        assert!(guard < 1000, "inertia must decay to a stop");
    }

    // Hidden spreadsheets are not hittable, so the wheel passes through.
    ctx[s].set_visible(false);
    assert!(!ctx.lend_h(s, |w, ctx| w.handle_event(&wheel, ctx)).unwrap(), "hidden widget ignores wheel");
}

#[test]
fn scrollbar_drag_and_keys_move_the_scroll() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(filled(50));
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };

    // content 1200, viewport 100 -> overflowing, so the host may drag it.
    assert!(ctx[s].draggable());

    // Sunk behind the plate, the bar takes no press.
    ctx[s].drag_begin(100.0, 80.0);
    assert!(!ctx[s].is_dragging(), "a sunk bar is not grabbed");

    // Raised, a press on its track (the pane'ctx[s] centre line) jumps the
    // thumb and engages the drag.
    ctx[s].inner_mut().activity.bump();
    ctx[s].inner_mut().recompute_bars(rect);
    ctx[s].drag_begin(100.0, 80.0);
    assert!(ctx[s].is_dragging());
    assert!(ctx[s].drag_update(100.0, 90.0), "thumb drag scrolls");
    let dragged_to = ctx[s].inner().geom(rect).unwrap().scroll;
    assert!(dragged_to > 0.0);
    ctx[s].drag_end();
    assert!(!ctx[s].is_dragging());

    // A body press (left of the scrollbar) engages no drag.
    ctx[s].drag_begin(50.0, 60.0);
    assert!(!ctx[s].is_dragging(), "body press is not a scrollbar drag");

    // End key jumps to max; Home returns to zero. (Keys route via keyboard_input.)
    let end = crate::widget::KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Named(NamedKey::End),
        text: None,
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    assert!(ctx.lend_h(s, |w, ctx| w.keyboard_input(&end, ctx)).unwrap());
    // The key glides: run the motion out before reading the offset.
    for _ in 0..1000 {
        if !Input::tick(&mut *ctx[s].inner_mut(), 1.0 / 60.0, rect) {
            break;
        }
    }
    let g = ctx[s].inner().geom(rect).unwrap();
    assert_eq!(g.scroll, g.max_scroll);

    // Hidden: the focused-widget keyboard path must not consume keys.
    ctx[s].set_visible(false);
    assert!(!ctx.lend_h(s, |w, ctx| w.keyboard_input(&end, ctx)).unwrap(), "hidden widget ignores keys");
}

/// The middle of column `c`'s header — columns are as wide as their
/// content, so a press is placed by the edges, not by a share of the pane.
fn mid(s: &Adapted<Spreadsheet>, c: usize) -> f32 {
    let e = s.inner().col_edges();
    (e[c] + e[c + 1]) * 0.5
}

fn header_click(x: f32) -> Event {
    Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x,
        y: 10.0,
        local_x: x,
        local_y: 10.0,
    }
}

#[test]
fn header_click_cycles_ascending_descending_natural() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(Spreadsheet::new());
    ctx[s].set_visible(true);
    WidgetHost::set_rect(&mut ctx[s], 0.0, 0.0, 200.0, 124.0);
    // Numeric strings out of lexicographic order: "10" must sort after "9".
    let rows = vec![
        vec!["10".to_string(), "b".to_string()],
        vec!["9".to_string(), "c".to_string()],
        vec!["2".to_string(), "a".to_string()],
    ];
    SpreadsheetController::set_spreadsheet_data(&mut *ctx[s], vec!["n".into(), "s".into()], rows);
    assert_eq!(ctx[s].inner().order, vec![0, 1, 2], "unsorted = natural order");

    // Click 1 on column 0: ascending, numeric.
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 0)), ctx)).unwrap());
    assert_eq!(ctx[s].inner().sort, Some((0, true)));
    assert_eq!(ctx[s].inner().order, vec![2, 1, 0], "2 < 9 < 10 numerically");

    // Click 2: descending.
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 0)), ctx)).unwrap());
    assert_eq!(ctx[s].inner().sort, Some((0, false)));
    assert_eq!(ctx[s].inner().order, vec![0, 1, 2]);

    // Click 3: back to natural order.
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 0)), ctx)).unwrap());
    assert_eq!(ctx[s].inner().sort, None);
    assert_eq!(ctx[s].inner().order, vec![0, 1, 2]);

    // Column 1 (lexicographic), then a body click changes nothing.
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 1)), ctx)).unwrap());
    assert_eq!(ctx[s].inner().order, vec![2, 0, 1], "a < b < c");
    let body = Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x: 50.0,
        y: 60.0,
        local_x: 50.0,
        local_y: 60.0,
    };
    ctx.lend_h(s, |w, ctx| w.handle_event(&body, ctx)).unwrap();
    assert_eq!(ctx[s].inner().sort, Some((1, true)), "body press is not a sort");
}

fn body_click(y: f32) -> Event {
    Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x: 50.0,
        y,
        local_x: 50.0,
        local_y: y,
    }
}

/// A press selects a row, ctrl toggles one, shift extends from the last
/// pressed; the rows are named by their place in the DATA, so a sort
/// moves the highlight and not what is selected; and a press on the
/// scrollbar is not a press on a row.
#[test]
fn rows_select_alone_toggled_and_in_runs() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(filled(50));
    // Rows are 24 tall under a 24 header: row k spans 24 + 24k.
    let row_y = |k: usize| 24.0 + 24.0 * k as f32 + 12.0;

    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(1)), ctx)).unwrap());
    assert_eq!(ctx[s].inner().selected_rows(), vec![1]);
    assert!(ctx[s].inner_mut().take_selection_change());
    assert!(!ctx[s].inner_mut().take_selection_change(), "taken once");

    // Plain press elsewhere replaces it.
    ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(2)), ctx)).unwrap();
    assert_eq!(ctx[s].inner().selected_rows(), vec![2]);

    // Ctrl adds and removes.
    crate::widget::WidgetHostExt::set_modifiers(&mut ctx[s], true, false, false);
    ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(0)), ctx)).unwrap();
    assert_eq!(ctx[s].inner().selected_rows(), vec![0, 2]);
    ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(2)), ctx)).unwrap();
    assert_eq!(ctx[s].inner().selected_rows(), vec![0]);

    // Shift runs from the last row pressed without it (row 2, the ctrl
    // press) to this one.
    crate::widget::WidgetHostExt::set_modifiers(&mut ctx[s], false, true, false);
    ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(0)), ctx)).unwrap();
    assert_eq!(ctx[s].inner().selected_rows(), vec![0, 1, 2]);

    // A plain press on the one selected row clears it.
    crate::widget::WidgetHostExt::set_modifiers(&mut ctx[s], false, false, false);
    ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(3)), ctx)).unwrap();
    ctx.lend_h(s, |w, ctx| w.handle_event(&body_click(row_y(3)), ctx)).unwrap();
    assert!(ctx[s].inner().selected_rows().is_empty());

    // A RAISED scrollbar'ctx[s] lane is the drag surface'ctx[s].
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    ctx[s].inner_mut().activity.bump();
    ctx[s].inner_mut().recompute_bars(rect);
    let press = Event::MouseButton {
        button: MouseButton::Left,
        state: ElementState::Pressed,
        x: 100.0,
        y: row_y(1),
        local_x: 100.0,
        local_y: row_y(1),
    };
    ctx.lend_h(s, |w, ctx| w.handle_event(&press, ctx));
    assert!(ctx[s].inner().selected_rows().is_empty(), "a scrollbar press selects nothing");

    // Under a sort the row pressed is the row SHOWN there, and a shift
    // run is the rows shown between.
    let t = ctx.insert(Spreadsheet::new());
    ctx[t].set_visible(true);
    WidgetHost::set_rect(&mut ctx[t], 0.0, 0.0, 200.0, 124.0);
    let rows = vec![
        vec!["10".to_string(), "b".to_string()],
        vec!["9".to_string(), "c".to_string()],
        vec!["2".to_string(), "a".to_string()],
    ];
    SpreadsheetController::set_spreadsheet_data(&mut *ctx[t], vec!["n".into(), "s".into()], rows.clone());
    ctx.lend_h(t, |w, ctx| w.handle_event(&header_click(mid(&*w, 0)), ctx)).unwrap(); // ascending: 2, 9, 10 = rows 2, 1, 0
    ctx.lend_h(t, |w, ctx| w.handle_event(&body_click(row_y(0)), ctx)).unwrap();
    assert_eq!(ctx[t].inner().selected_rows(), vec![2], "the first row shown is the data's third");
    crate::widget::WidgetHostExt::set_modifiers(&mut ctx[t], false, true, false);
    ctx.lend_h(t, |w, ctx| w.handle_event(&body_click(row_y(1)), ctx)).unwrap();
    assert_eq!(ctx[t].inner().selected_rows(), vec![1, 2]);

    // A refresh keeps what is selected, less what the table lost.
    SpreadsheetController::set_spreadsheet_data(&mut *ctx[t], vec!["n".into(), "s".into()], rows[..2].to_vec());
    assert_eq!(ctx[t].inner().selected_rows(), vec![1]);
    ctx[t].inner_mut().set_selected_rows(&[]);
    assert!(ctx[t].inner().selected_rows().is_empty());
    assert!(ctx[t].inner_mut().take_selection_change());
}

/// The two bars cross at the middle of the body; they idle behind the
/// plate, where a press on them is the row's; a scroll brings them to the
/// fore, a pointer over a raised bar holds it there past the hold, and
/// with nothing holding them they sink again. Hover alone never raises.
#[test]
fn the_scrollbars_cross_at_the_body_and_sink_until_scrolled() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(Spreadsheet::new());
    ctx[s].set_visible(true);
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    WidgetHost::set_rect(&mut ctx[s], rect.x, rect.y, rect.width, rect.height);
    let headers: Vec<String> = (0..6).map(|i| format!("c{i}")).collect();
    let rows: Vec<Vec<String>> = (0..50).map(|r| (0..6).map(|c| format!("{r}.{c}")).collect()).collect();
    SpreadsheetController::set_spreadsheet_data(&mut *ctx[s], headers, rows);

    // A cross: the vertical bar centred on the width, the horizontal on
    // the body'ctx[s] height, each spanning its axis less the inset.
    let v = ctx[s].inner().geom(rect).expect("50 rows overflow");
    let h = ctx[s].inner().hgeom(rect).expect("6 floored columns overflow");
    let body_mid = HEADER_H + (rect.height - HEADER_H) * 0.5;
    assert!((v.bar_x + v.bar_w * 0.5 - rect.width * 0.5).abs() < 0.01, "vertical bar on the centre line");
    assert!((h.bar_y + h.bar_h * 0.5 - body_mid).abs() < 0.01, "horizontal bar on the body's centre line");
    assert_eq!((v.track_y, v.track_h), (HEADER_H + TRACK_INSET, rect.height - HEADER_H - 2.0 * TRACK_INSET));
    assert_eq!((h.track_x, h.track_w), (TRACK_INSET, rect.width - 2.0 * TRACK_INSET));

    let at = |x: f32, y: f32| Event::PointerMove { x, y, local_x: x, local_y: y };
    let mid = (rect.width * 0.5, body_mid);

    // Sunk: hovering raises nothing, and a press on the middle of the
    // cross is a press on the row there.
    assert!(!ctx[s].inner().scrollbars_raised());
    ctx.lend_h(s, |w, ctx| w.handle_event(&at(mid.0, mid.1), ctx)).unwrap();
    assert!(!ctx[s].inner().scrollbars_raised(), "hover never raises a sunk bar");
    assert!(ctx[s].inner().body_row_at(mid.0, mid.1, rect).is_some(), "a sunk bar's lane is the row's");

    // A scroll raises both; the lane is the bars' now, and the fore copy
    // fades in over the next frames.
    let wheel = Event::MouseWheel {
        delta: MouseScrollDelta::LineDelta(0.0, -1.0),
        x: 30.0,
        y: 40.0,
        local_x: 30.0,
        local_y: 40.0,
    };
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&wheel, ctx)).unwrap());
    assert!(ctx[s].inner().scrollbars_raised(), "a scroll raises the bars");
    assert!(ctx[s].inner().body_row_at(mid.0, mid.1, rect).is_none(), "a raised bar's lane is the drag's");
    assert!(ctx[s].inner().body_row_at(mid.0, mid.1 + 30.0, rect).is_none(), "the vertical bar off the middle too");
    for _ in 0..20 {
        Input::tick(&mut *ctx[s].inner_mut(), 0.016, rect);
    }
    assert_eq!(ctx[s].inner().scrollbar_fade(), 1.0, "faded all the way in");

    // The pointer on a bar holds it up long past the hold…
    ctx.lend_h(s, |w, ctx| w.handle_event(&at(mid.0, mid.1 + 30.0), ctx)).unwrap();
    for _ in 0..200 {
        Input::tick(&mut *ctx[s].inner_mut(), 0.016, rect);
    }
    assert!(ctx[s].inner().scrollbars_raised(), "hovered, a raised bar stays in front");

    // …and off it, the bars sink once the hold runs out, and fade away.
    ctx.lend_h(s, |w, ctx| w.handle_event(&at(30.0, 40.0), ctx)).unwrap();
    let mut guard = 0;
    while Input::tick(&mut *ctx[s].inner_mut(), 0.016, rect) {
        guard += 1;
        assert!(guard < 1000, "the sink settles");
    }
    assert!(!ctx[s].inner().scrollbars_raised(), "unheld, the bars sink");
    assert_eq!(ctx[s].inner().scrollbar_fade(), 0.0);

    // Painted: nothing of the bars while sunk (the host draws that copy
    // behind its plate), four pills while raised.
    let pills = |s: &Adapted<Spreadsheet>| {
        let mut pc = crate::scene::paint::PaintCtx::new();
        Paint::paint(&**s, rect, &mut pc);
        pc.finish().items.iter().filter(|i| matches!(i.prim, crate::scene::paint::Prim::RoundedRect { .. })).count()
    };
    assert_eq!(pills(&ctx[s]), 0, "a sunk bar is not painted over the cells");
    ctx.lend_h(s, |w, ctx| w.handle_event(&wheel, ctx)).unwrap();
    for _ in 0..20 {
        Input::tick(&mut *ctx[s].inner_mut(), 0.016, rect);
    }
    assert_eq!(pills(&ctx[s]), 4, "two tracks and two thumbs");
}

/// A table large enough to work its columns' widths out on several
/// threads gets the widths one thread works out.
#[test]
fn a_large_tables_widths_are_worked_out_in_parallel_alike() {
    let n = 60_000;
    let columns: Vec<SheetColumn> = (0..6)
        .map(|c| {
            if c % 2 == 0 {
                SheetColumn::Float { values: (0..n).map(|r| ((r * 7919 + c * 13) % 100_003) as f32 * if c == 2 { -0.37 } else { 0.011 }).collect(), decimals: 4 }
            } else {
                SheetColumn::Int((0..n as i64).map(|r| r * (c as i64) - 1000).collect())
            }
        })
        .collect();
    let headers: Vec<String> = (0..6).map(|c| format!("c{c}")).collect();
    let want: Vec<usize> = headers.iter().zip(&columns).map(|(h, c)| (h.chars().count() + SORT_MARK_CHARS).max(c.max_chars())).collect();
    let mut s = Spreadsheet::new();
    SpreadsheetController::set_spreadsheet_columns(&mut *s, headers, columns);
    assert_eq!(s.inner().col_chars, want);
}

/// A column's width in characters is its widest cell's, numbers by
/// their range: over negatives, positives, values that round to zero
/// either side, both zeros, the non-finite spellings, and integers.
#[test]
fn max_chars_is_the_widest_cell() {
    let widest = |c: &SheetColumn| (0..c.len()).map(|r| c.cell(r).chars().count()).max().unwrap_or(0);
    let floats = [
        vec![0.0, -0.0],
        vec![-0.0, 0.0],
        vec![1.5, -0.00001, 2.0],
        vec![123.456, -9.99996, 0.5],
        vec![-1234.5, 99999.0, 0.0],
        vec![f32::NAN, 1.0],
        vec![f32::NEG_INFINITY, 0.25],
        vec![f32::INFINITY],
        vec![],
    ];
    for values in floats {
        for decimals in [0, 2, 4] {
            let c = SheetColumn::Float { values: values.clone(), decimals };
            assert_eq!(c.max_chars(), widest(&c), "{values:?} at {decimals}");
        }
    }
    for ints in [vec![], vec![0], vec![-7, 3, 1200], vec![i64::MIN, 5], vec![-1, -100]] {
        let c = SheetColumn::Int(ints.clone());
        assert_eq!(c.max_chars(), widest(&c), "{ints:?}");
    }
}

/// A table of columns: the cells on screen are written as they are
/// painted — to a float column's decimals, an integer as one — and only
/// those; a sort compares the values, so 10 sorts after 9 without a
/// cell being parsed, and an integer column likewise.
#[test]
fn a_column_table_is_written_as_painted_and_sorts_by_value() {
    let rect = Rect { x: 0.0, y: 0.0, width: 200.0, height: 124.0 };
    let mut ctx = UiContext::new();
    let s = ctx.insert(Spreadsheet::new());
    ctx[s].set_visible(true);
    WidgetHost::set_rect(&mut ctx[s], 0.0, 0.0, 200.0, 124.0);
    let n = 1000;
    SpreadsheetController::set_spreadsheet_columns(
        &mut *ctx[s],
        vec!["i".into(), "x".into()],
        vec![
            SheetColumn::Int((0..n as i64).collect()),
            SheetColumn::Float { values: (0..n).map(|r| ((r * 7919) % n) as f32 * 0.5 - 3.25).collect(), decimals: 4 },
        ],
    );
    assert_eq!(ctx[s].inner().row_count, n);
    let texts = |s: &Adapted<Spreadsheet>| -> Vec<String> {
        let mut pc = crate::scene::paint::PaintCtx::new();
        Paint::paint(&**s, rect, &mut pc);
        pc.finish()
            .items
            .iter()
            .filter_map(|item| match &item.prim {
                crate::scene::paint::Prim::Text { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    let shown = texts(&ctx[s]);
    assert!(shown.contains(&"0".to_string()) && shown.contains(&"-3.2500".to_string()), "{shown:?}");
    assert!(!shown.contains(&"999".to_string()), "a row off screen is not written");

    // Sorted by x, ascending: the least value first, by value.
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 1)), ctx)).unwrap());
    let order = ctx[s].inner().order.clone();
    let x = |r: usize| ((r * 7919) % n) as f32 * 0.5 - 3.25;
    assert!(order.windows(2).all(|w| x(w[0]) <= x(w[1])));
    // And by i, descending (two more clicks on the first header).
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 0)), ctx)).unwrap());
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 0)), ctx)).unwrap());
    assert_eq!(ctx[s].inner().order[0], n - 1, "999 sorts after 99 and 100");
    assert_eq!(SheetColumn::Float { values: vec![1.0 / 3.0], decimals: 2 }.cell(0), "0.33");
    assert_eq!(SheetColumn::Int(vec![-4]).cell(0), "-4");
    assert_eq!(SheetColumn::Int(vec![]).cell(3), "", "past the end is empty");
}

#[test]
fn data_refresh_reapplies_sort_and_column_shrink_clears_it() {
    let mut ctx = UiContext::new();
    let s = ctx.insert(Spreadsheet::new());
    ctx[s].set_visible(true);
    WidgetHost::set_rect(&mut ctx[s], 0.0, 0.0, 200.0, 124.0);
    SpreadsheetController::set_spreadsheet_data(
        &mut *ctx[s],
        vec!["a".into(), "b".into()],
        vec![vec!["1".into(), "x".into()], vec!["2".into(), "y".into()]],
    );
    assert!(ctx.lend_h(s, |w, ctx| w.handle_event(&header_click(mid(&*w, 1)), ctx)).unwrap()); // sort col 1 asc

    // A refresh with new rows keeps the sort and re-derives the order.
    SpreadsheetController::set_spreadsheet_data(
        &mut *ctx[s],
        vec!["a".into(), "b".into()],
        vec![vec!["1".into(), "z".into()], vec!["2".into(), "w".into()]],
    );
    assert_eq!(ctx[s].inner().sort, Some((1, true)));
    assert_eq!(ctx[s].inner().order, vec![1, 0], "w < z");

    // A refresh that drops the sorted column clears the sort.
    SpreadsheetController::set_spreadsheet_data(
        &mut *ctx[s],
        vec!["a".into()],
        vec![vec!["1".into()], vec!["2".into()]],
    );
    assert_eq!(ctx[s].inner().sort, None);
    assert_eq!(ctx[s].inner().order, vec![0, 1]);
}
