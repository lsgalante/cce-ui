use super::context_menu::{self, ROW_H};
use crate::context::UiContext;
use crate::widget::{ContextAction, ElementState, MouseButton, Widget, WidgetHost, WidgetId};

/// A widget that remembers the last action a menu ran on it.
struct Recorder {
    base: Widget,
    got: Option<ContextAction>,
}
impl crate::widget::Input for Recorder {
    fn context_action(&mut self, action: ContextAction) -> bool {
        self.got = Some(action);
        true
    }
}
impl WidgetHost for Recorder {
    crate::impl_widget_base!(Recorder);
    fn input_model_mut(&mut self) -> &mut dyn crate::widget::Input {
        self
    }
}

fn press_row(ctx: &mut UiContext, idx: usize) {
    let (x, y) = (context_menu::x() + 10.0, context_menu::row_y(idx) + ROW_H * 0.5);
    context_menu::mouse_input(MouseButton::Left, ElementState::Pressed, x, y, Some(ctx));
}

#[test]
fn a_row_runs_its_action_whatever_its_label_says() {
    let mut ctx = UiContext::new();
    let w = ctx.insert(Recorder { base: Widget::new(), got: None });
    let id = w.id();

    // A label the English table has never seen: only the row's action can say what it is.
    context_menu::show(0.0, 0.0, vec!["[Recorder]".into(), "Kopieren".into()], 1, id);
    context_menu::set_row_actions(vec![None, Some(ContextAction::Copy)]);
    press_row(&mut ctx, 1);
    assert_eq!(ctx[w].got, Some(ContextAction::Copy));

    // A row with an action and an English label that names ANOTHER: the action wins.
    ctx[w].got = None;
    context_menu::show(0.0, 0.0, vec!["[Recorder]".into(), "Paste".into()], 1, id);
    context_menu::set_row_actions(vec![None, Some(ContextAction::SelectAll)]);
    press_row(&mut ctx, 1);
    assert_eq!(ctx[w].got, Some(ContextAction::SelectAll));

    // A menu built without actions still works in English, through the fallback.
    ctx[w].got = None;
    context_menu::show(0.0, 0.0, vec!["[Recorder]".into(), "Paste".into()], 1, id);
    press_row(&mut ctx, 1);
    assert_eq!(ctx[w].got, Some(ContextAction::Paste));
}

#[test]
fn the_toolkits_own_menus_carry_their_actions() {
    let mut ctx = UiContext::new();
    let tb = ctx.insert(crate::widget::TextBox::new(String::new()).with_label("Name"));
    ctx.lend_h(tb, |w, ctx| ctx.handle_right_click(w, 5.0, 5.0));
    let options = context_menu::options();
    let header = context_menu::header_count();
    assert!(options.len() > header, "a text box's menu has rows");
    for (i, label) in options.iter().enumerate().skip(header) {
        assert!(context_menu::row_action(i).is_some(), "row {label:?} has no action of its own");
    }
    context_menu::hide();
}

/// A disabled row stands among the rows that run but is none of them: dim like a header (its
/// mark too), never highlighted by the pointer or the keyboard, and a press on it runs nothing
/// and only closes the menu. A new `show` enables every row again.
#[test]
fn a_disabled_row_is_dim_and_runs_nothing() {
    let mut ctx = UiContext::new();
    let w = ctx.insert(Recorder { base: Widget::new(), got: None });
    let rows = ["[Page]", "Back", "✓ Forward", "-", "Reload"].map(String::from).to_vec();
    context_menu::show(100.0, 50.0, rows.clone(), 1, w.id());
    context_menu::set_row_actions(vec![None, Some(ContextAction::Undo), Some(ContextAction::Redo), None, Some(ContextAction::Copy)]);
    context_menu::set_rows_disabled(vec![false, true]);
    context_menu::set_row_disabled(2);
    assert!(context_menu::row_disabled(1) && context_menu::row_disabled(2) && !context_menu::row_disabled(4));

    // The keyboard and the pointer pass over them.
    assert_eq!(context_menu::step_hovered(1), Some(4), "the header, both disabled rows and the separator are stepped over");
    assert_eq!(context_menu::step_hovered(-1), Some(4), "and nothing above it runs");
    context_menu::set_hovered_item(Some(1));
    assert_eq!(context_menu::hovered_item(), None, "a disabled row cannot be highlighted");
    let (x, y) = (context_menu::x() + 10.0, context_menu::row_y(2) + ROW_H * 0.5);
    context_menu::cursor_moved(x, y);
    assert_eq!(context_menu::hovered_item(), None, "nor hovered");

    // Dim, as the header is; the row that runs is not.
    context_menu::with_state(|m| {
        let m = m.borrow();
        let labels = m.labels();
        assert_eq!(labels[1].color, labels[0].color, "Back is drawn as the header is");
        assert_eq!(labels[2].color, labels[0].color);
        assert_ne!(labels[3].color, labels[0].color, "Reload runs and is not dim");
        let check = m.glyphs().into_iter().find(|g| g.name == "check").unwrap();
        assert_eq!(check.color, crate::color::TEXT_DIM, "a disabled row's mark is dim too");
    });

    // A press runs nothing, and closes the menu as a press on a header does.
    press_row(&mut ctx, 1);
    assert!(!context_menu::is_visible());
    assert_eq!(ctx[w].got, None);
    assert_eq!(context_menu::take_picked(), None, "nor is it the host's");
    assert!(!context_menu::activate(2, Some(&mut ctx)), "nothing runs it");

    context_menu::show(100.0, 50.0, rows, 1, w.id());
    assert!(!context_menu::row_disabled(1), "a new show enables every row");
    context_menu::hide();
}

/// A row with no action of its own is the host's: a press on it, Enter on its highlight
/// (`activate_hovered`) or a reader's Click (`activate`) records it with the generation of
/// the `show` it belongs to, once, and closes the menu. A row with an action runs it and is
/// not the host's; a header, a separator or the padding is neither; a page row turns.
#[test]
fn a_row_the_host_owns_is_picked_by_a_press_enter_or_a_reader() {
    let mut ctx = UiContext::new();
    let w = ctx.insert(Recorder { base: Widget::new(), got: None });
    let open = || {
        let rows = ["[Recorder]", "Open", "Copy", "Kopieren", "-", "More"].map(String::from).to_vec();
        context_menu::show(100.0, 50.0, rows, 1, w.id());
        context_menu::set_row_actions(vec![None, None, None, Some(ContextAction::Copy)]);
        context_menu::set_row_page(5);
        context_menu::generation()
    };

    let generation = open();
    press_row(&mut ctx, 1);
    assert!(!context_menu::is_visible());
    assert_eq!(context_menu::take_picked(), Some((generation, 1)));
    assert_eq!(context_menu::take_picked(), None, "taken once");
    assert_eq!(ctx[w].got, None, "the toolkit ran nothing for it");

    // Its own action: run, not picked.
    open();
    press_row(&mut ctx, 3);
    assert_eq!((ctx[w].got, context_menu::take_picked()), (Some(ContextAction::Copy), None));

    // No action of its own but an English label the fallback knows: the fallback still
    // runs it on the target, and it is the host's all the same.
    ctx[w].got = None;
    let generation = open();
    press_row(&mut ctx, 2);
    assert_eq!((ctx[w].got, context_menu::take_picked()), (Some(ContextAction::Copy), Some((generation, 2))));

    // A header, a separator, the padding: only closed.
    for at in [0, 4] {
        open();
        press_row(&mut ctx, at);
        assert!(!context_menu::is_visible());
        assert_eq!(context_menu::take_picked(), None, "row {at} is nobody's");
    }
    open();
    context_menu::mouse_input(MouseButton::Left, ElementState::Pressed, context_menu::x() + 10.0, context_menu::y() + 2.0, None);
    assert_eq!(context_menu::take_picked(), None, "the padding is plate, not a row");

    // Enter on the highlight, with no context at all: the host's row is picked all the same.
    let generation = open();
    assert!(!context_menu::activate_hovered(None), "nothing highlighted, nothing run");
    assert!(context_menu::is_visible());
    assert_eq!(context_menu::step_hovered(1), Some(1));
    assert!(context_menu::activate_hovered(None));
    assert!(!context_menu::is_visible());
    assert_eq!(context_menu::take_picked(), Some((generation, 1)));

    // A page row turns and stays up; nothing is picked.
    open();
    assert!(context_menu::activate(5, Some(&mut ctx)));
    assert!(context_menu::is_visible());
    assert_eq!(context_menu::take_turn(), Some(context_menu::PageTurn::Into(5)));
    assert_eq!(context_menu::take_picked(), None);

    // An untaken pick does not outlive the next show.
    open();
    press_row(&mut ctx, 1);
    open();
    assert_eq!(context_menu::take_picked(), None);
    context_menu::hide();
}

/// A reader's Click reaches a row wherever it is drawn: on a menu cut short, a row scrolled
/// out of view is picked as one in view is.
#[test]
fn a_row_scrolled_out_of_view_is_run_by_index() {
    let rows: Vec<String> = (0..20).map(|i| format!("Row {i}")).collect();
    context_menu::show(100.0, 50.0, rows, 0, WidgetId(0));
    context_menu::place(100.0, 50.0, 120.0);
    let generation = context_menu::generation();
    assert!(context_menu::row_y(15) > context_menu::y() + context_menu::h(), "drawn below the plate");
    assert!(context_menu::activate(15, None));
    assert_eq!(context_menu::take_picked(), Some((generation, 15)));
}
