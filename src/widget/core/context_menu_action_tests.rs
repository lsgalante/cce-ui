use super::context_menu::{self, ROW_H};
use crate::context::UiContext;
use crate::widget::{ContextAction, ElementState, MouseButton, Widget, WidgetHost};

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
