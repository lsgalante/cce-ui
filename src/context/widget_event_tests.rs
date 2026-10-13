//! `UiContext::take_events`: what the sweep reports, in what order, and that it drains what it
//! reports.

use super::*;
use crate::widget::{
    Adapted, Button, Checkbox, EventCtx, Input, KeyEvent, KeybindRecorder, Layout, Paint, Slider,
    TextBox, Toggle, WidgetEvent,
};

fn button_event(state: ElementState, x: f32, y: f32) -> Event {
    Event::MouseButton { button: MouseButton::Left, state, x, y, local_x: x, local_y: y }
}

/// A press then a release at `(x, y)`, routed from `root`.
fn click(ctx: &mut UiContext, root: WidgetId, x: f32, y: f32) {
    ctx.propagate_event(&button_event(ElementState::Pressed, x, y), root);
    ctx.propagate_event(&button_event(ElementState::Released, x, y), root);
}

/// A routed click is swept once as `Clicked`, and the sweep drained it: the per-handle poll
/// finds nothing after it, nor does a second sweep.
#[test]
fn a_routed_click_is_swept_once() {
    let mut ctx = UiContext::new();
    let b = ctx.insert(Button::new(10.0, 10.0, 80.0, 24.0).with_label("Go"));
    click(&mut ctx, b.id(), 20.0, 20.0);
    assert_eq!(ctx.take_events(), vec![(b.id(), WidgetEvent::Clicked)]);
    assert!(!ctx[b].take_click(), "the sweep drained the flag");
    assert!(ctx.take_events().is_empty(), "and reports it once");
}

/// A value the user changes is swept as `Changed`: a check box's press (which is also its
/// click), a toggle's (one flag behind both polls: a change) and a slider's drag.
#[test]
fn a_value_change_is_swept_as_changed() {
    let mut ctx = UiContext::new();
    let cb = ctx.insert(Checkbox::new());
    WidgetHost::set_rect(&mut ctx[cb], 0.0, 0.0, 20.0, 20.0);
    ctx.propagate_event(&button_event(ElementState::Pressed, 10.0, 10.0), cb.id());
    assert_eq!(ctx.take_events(), vec![(cb.id(), WidgetEvent::Clicked), (cb.id(), WidgetEvent::Changed)]);
    assert!(!ctx[cb].take_change());

    let t = ctx.insert(Toggle::new());
    WidgetHost::set_rect(&mut ctx[t], 0.0, 100.0, 60.0, 30.0);
    ctx.propagate_event(&button_event(ElementState::Pressed, 30.0, 115.0), t.id());
    assert!(ctx[t].toggled());
    assert_eq!(ctx.take_events(), vec![(t.id(), WidgetEvent::Changed)]);

    let sl = ctx.insert(Slider::new().with_value(0.5));
    WidgetHost::set_rect(&mut ctx[sl], 0.0, 40.0, 100.0, 20.0);
    ctx.propagate_event(&button_event(ElementState::Pressed, 50.0, 50.0), sl.id());
    assert!(ctx[sl].drag_update(80.0, 50.0));
    ctx[sl].drag_end();
    assert_eq!(ctx.take_events(), vec![(sl.id(), WidgetEvent::Changed)]);
}

/// A text box commits its edit on `FocusOut`; one from a direct `unfocus()` — no routing
/// context — still reaches the next sweep.
#[test]
fn a_commit_from_a_direct_unfocus_is_swept() {
    let mut ctx = UiContext::new();
    let tb = ctx.insert(TextBox::new("old".to_string()));
    ctx[tb].set_rect(0.0, 0.0, 200.0, 30.0);
    ctx[tb].focus();
    assert!(ctx[tb].editing);
    ctx[tb].edit_buffer = "new".to_string();
    assert!(ctx.take_events().is_empty(), "nothing is committed while it edits");
    ctx[tb].unfocus();
    assert_eq!(ctx.take_events(), vec![(tb.id(), WidgetEvent::Changed)]);
    assert_eq!(ctx[tb].text, "new");
}

/// A chord a `KeybindRecorder` records reaches the sweep and the per-handle poll: its
/// `Input` impl forwards its own drain.
#[test]
fn a_recorded_chord_is_swept() {
    let mut ctx = UiContext::new();
    let k = ctx.insert(KeybindRecorder::new("ctrl+a".to_string()));
    WidgetHost::set_rect(&mut ctx[k], 0.0, 0.0, 120.0, 24.0);
    click(&mut ctx, k.id(), 10.0, 10.0);
    let key = KeyEvent {
        state: ElementState::Pressed,
        logical_key: Key::Character("b".to_string()),
        text: Some("b".to_string()),
        repeat: false,
        ctrl: false,
        shift: false,
        alt: false,
    };
    ctx.lend_h(k, |w, ctx| w.keyboard_input(&key, ctx));
    assert_eq!(ctx[k].value, "b");
    assert_eq!(ctx.take_events(), vec![(k.id(), WidgetEvent::Changed)]);

    click(&mut ctx, k.id(), 10.0, 10.0);
    ctx.lend_h(k, |w, ctx| w.keyboard_input(&key, ctx));
    assert!(ctx[k].take_change(), "the per-handle poll sees it too");
}

/// A widget of the app's own that emits: on a press, on a tick and on a direct `unfocus`,
/// and it also raises a change flag on the press.
#[derive(Default)]
struct Emitter {
    changed: bool,
    ticked: bool,
}
impl Layout for Emitter {}
impl Paint for Emitter {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }
}
impl Input for Emitter {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { state: ElementState::Pressed, .. } => {
                ectx.emit(WidgetEvent::Clicked);
                self.changed = true;
                true
            }
            Event::FocusOut => {
                ectx.emit(WidgetEvent::Clicked);
                false
            }
            _ => false,
        }
    }
    fn wants_tick(&self) -> bool {
        true
    }
    fn tick_ctx(&mut self, _dt: f32, ectx: &mut EventCtx) -> bool {
        if !std::mem::replace(&mut self.ticked, true) {
            ectx.emit(WidgetEvent::Changed);
        }
        false
    }
    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.changed)
    }
}

/// What a widget emits arrives in the order it emitted it — across routed, ticked and
/// direct calls — and ahead of its flag-derived events, though the flag went up first.
#[test]
fn emitted_events_arrive_in_order_before_the_flags() {
    let mut ctx = UiContext::new();
    let e = ctx.insert(Adapted::new(Emitter::default()));
    WidgetHost::set_rect(&mut ctx[e], 0.0, 0.0, 40.0, 40.0);
    ctx.propagate_event(&button_event(ElementState::Pressed, 10.0, 10.0), e.id());
    ctx.tick(0.016);
    ctx[e].unfocus();
    let id = e.id();
    assert_eq!(
        ctx.take_events(),
        vec![
            (id, WidgetEvent::Clicked),
            (id, WidgetEvent::Changed),
            (id, WidgetEvent::Clicked),
            (id, WidgetEvent::Changed),
        ],
        "the press's, the tick's and the unfocus's emits, then the press's change flag",
    );
    assert!(ctx.take_events().is_empty());
}

/// A window nobody touched reports nothing, and widgets in it are swept in id order.
#[test]
fn an_idle_context_sweeps_nothing_and_the_order_is_by_id() {
    let mut ctx = UiContext::new();
    assert!(ctx.take_events().is_empty(), "an empty context");
    let b = ctx.insert(Button::new(0.0, 0.0, 80.0, 24.0).with_label("A"));
    let cb = ctx.insert(Checkbox::new());
    let tb = ctx.insert(TextBox::new("x".to_string()));
    let _ = ctx.insert(Slider::new());
    assert!(ctx.take_events().is_empty(), "widgets nobody touched");

    // Raised out of id order; swept in it.
    ctx[tb].set_value("y");
    ctx[cb].set_value_string("true");
    WidgetHost::set_rect(&mut ctx[b], 0.0, 0.0, 80.0, 24.0);
    click(&mut ctx, b.id(), 10.0, 10.0);
    let ids: Vec<WidgetId> = ctx.take_events().into_iter().map(|(id, _)| id).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
    assert_eq!(ids, vec![b.id(), cb.id(), tb.id()]);
}

/// Embedded children are swept under their own ids: a tree list's search box. The list
/// still applies a search whose change the sweep took first.
#[test]
fn an_embedded_childs_change_is_swept_and_its_composite_still_sees_it() {
    let mut tree = crate::widget::TreeList::new();
    tree.set_rect(0.0, 0.0, 380.0, 400.0);
    tree.set_flat_keys(vec![
        ("style.corner_radius".to_string(), serde_json::Value::from(8)),
        ("input.accel_profile".to_string(), serde_json::Value::from("flat")),
    ]);
    let mut ctx = UiContext::new();
    let t = ctx.insert(tree);
    let sb = ctx[t].search_box.handle().expect("the search box is the context's");
    ctx[sb].set_value("corner");
    assert_eq!(ctx.take_events(), vec![(sb.id(), WidgetEvent::Changed)]);
    ctx.tick(0.016);
    let leaves: Vec<String> = ctx[t]
        .items
        .iter()
        .filter_map(|item| match item {
            crate::widget::TreeElement::Leaf { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(leaves, vec!["corner_radius".to_string()], "the search applied");
}
