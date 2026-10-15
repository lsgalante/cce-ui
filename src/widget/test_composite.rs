//! `Composite`, the toolkit's own tests' stand-in for a widget made of parts it never
//! registers (the ramps): one keyboard stop whose two fields take the keyboard through
//! `UiContext::claim_field_focus`, and accessibility items for a key it selects and for
//! each field. It records the keys and the focus changes it is handed.

use crate::a11y::A11yItem;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Event, EventCtx, FocusRole, Input, Key, Layout, Paint, WidgetId};

pub(crate) struct Composite {
    /// Its parts' ids, never registered: a position slider and a line dropdown.
    pub fields: [WidgetId; 2],
    /// Every key it was handed, in order.
    pub keys: Vec<Key>,
    /// "in" / "out" for every FocusIn / FocusOut, in order.
    pub focus_events: Vec<&'static str>,
    /// Whether the keyboard is on it (or a field of its), as its FocusIn / FocusOut say.
    pub focused: bool,
    pub key_selected: bool,
    /// The position slider's value.
    pub position: f64,
    /// What `keeps_tab` answers: forward, then backward.
    pub keeps: (bool, bool),
}

impl Composite {
    pub(crate) fn new() -> Adapted<Composite> {
        let fresh = || WidgetId(crate::widget::NEXT_WIDGET_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
        Adapted::new(Composite {
            fields: [fresh(), fresh()],
            keys: Vec::new(),
            focus_events: Vec::new(),
            focused: false,
            key_selected: false,
            position: 0.25,
            keeps: (false, false),
        })
    }
}

impl Layout for Composite {}

impl Paint for Composite {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }

    fn paint(&self, _rect: Rect, _ctx: &mut PaintCtx) {}
}

impl Input for Composite {
    fn focus_role(&self) -> FocusRole {
        FocusRole::Well
    }

    fn keeps_tab(&self, reverse: bool) -> bool {
        if reverse { self.keeps.1 } else { self.keeps.0 }
    }

    fn a11y_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Group)
    }

    fn a11y_items(&self, rect: Rect) -> Vec<A11yItem> {
        use accesskit::Role;
        vec![
            A11yItem {
                toggled: Some(self.key_selected),
                focused: self.focused && self.key_selected,
                ..A11yItem::new(Role::RadioButton, "Key 1 of 1", rect)
            },
            A11yItem {
                value: Some(format!("{:.2}", self.position)),
                range: Some((0.0, 1.0, 0.02)),
                field: Some(self.fields[0]),
                ..A11yItem::new(Role::Slider, "Position", rect)
            },
            A11yItem { value: Some("Smooth".into()), field: Some(self.fields[1]), ..A11yItem::new(Role::ComboBox, "Line", rect) },
        ]
    }

    fn a11y_select_item(&mut self, idx: usize) -> bool {
        idx == 0 && !std::mem::replace(&mut self.key_selected, true)
    }

    fn a11y_set_item_value(&mut self, idx: usize, value: f64) -> bool {
        if idx != 1 {
            return false;
        }
        self.position = value.clamp(0.0, 1.0);
        true
    }

    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::KeyInput(k) => {
                self.keys.push(k.logical_key.clone());
                true
            }
            Event::FocusIn => {
                self.focus_events.push("in");
                self.focused = true;
                true
            }
            Event::FocusOut => {
                self.focus_events.push("out");
                self.focused = false;
                true
            }
            _ => false,
        }
    }
}
