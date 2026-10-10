//! `RadioGroup` — one of several options, chosen (`docs/rfc-accessibility-locale.md`,
//! phase 3).
//!
//! Each option is the check box's well at a full corner (the DE's superellipse, so a
//! squircle rather than a disc) with a lit bead, a sphere, standing in the chosen one, and
//! its label after it — in a column, or a row with [`Adapted<RadioGroup>::with_horizontal`].
//! The bead is round where the check box's plate is square, which is what tells the two
//! apart at a glance (a flat host, which carries no sphere, draws it as a disc).
//!
//! **The keyboard.** The group is ONE stop in the Tab walk, a plate, as every set of
//! exclusive choices is; the arrows move the choice (Down / Right the next, Up / Left the one
//! before, wrapping; Home / End the ends), and the choice follows them — there is no option
//! that has focus without being chosen. A press on an option chooses it.
//!
//! **A screen reader** sees a radio group whose children are radio buttons, the chosen one
//! checked and focused while the group is, each one clickable
//! ([`Input::a11y_items`] / [`Input::a11y_select_item`]).

use crate::scene::layout::{Rect, Size};
use crate::scene::paint::{Field, PaintCtx};
use crate::widget::{ElementState, Event, Key, MouseButton, NamedKey};
use crate::widget::model::EventCtx;
use crate::widget::{Adapted, Input, Layout, Paint};

/// The gap between an option's box and its label.
const LABEL_GAP: f32 = 8.0;
/// The chosen option's bead, as a share of its well's side.
const BEAD_SHARE: f32 = 0.42;
/// The bead's body (linear): lighter than the well it stands in.
const BEAD: [f32; 4] = [0.42, 0.45, 0.56, 1.0];

#[derive(Debug, Clone)]
pub struct RadioGroup {
    options: Vec<String>,
    selected: usize,
    horizontal: bool,
    focused: bool,
    just_changed: bool,
}

impl RadioGroup {
    /// A column of `options`, the first chosen.
    pub fn new<S: Into<String>>(options: impl IntoIterator<Item = S>) -> Adapted<RadioGroup> {
        Adapted::new(RadioGroup {
            options: options.into_iter().map(Into::into).collect(),
            selected: 0,
            horizontal: false,
            focused: false,
            just_changed: false,
        })
    }

    pub fn options(&self) -> &[String] {
        &self.options
    }

    /// The chosen option's index.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Choose option `idx` (clamped). Not reported as a change: the host set it.
    pub fn set_selected(&mut self, idx: usize) {
        self.selected = idx.min(self.options.len().saturating_sub(1));
    }

    /// Replace the options, keeping the choice where it still exists.
    pub fn set_options<S: Into<String>>(&mut self, options: impl IntoIterator<Item = S>) {
        self.options = options.into_iter().map(Into::into).collect();
        self.set_selected(self.selected);
    }

    /// One option's height: a toggle's, the check box's side.
    fn row_h() -> f32 {
        crate::layout::toggle_height()
    }

    fn gap() -> f32 {
        crate::layout::control_gap()
    }

    fn label_width(label: &str) -> f32 {
        let (fam, size) = crate::layout::control_label_font_parsed();
        crate::widget::display::measure_text_width(label, &fam, size)
    }

    /// Each option's rect (its box and its label) in `rect`.
    pub fn option_rects(&self, rect: Rect) -> Vec<Rect> {
        let (h, gap) = (Self::row_h(), Self::gap());
        let mut x = rect.x;
        self.options
            .iter()
            .enumerate()
            .map(|(i, label)| {
                if self.horizontal {
                    let w = h + LABEL_GAP + Self::label_width(label);
                    let r = Rect { x, y: rect.y, width: w, height: h };
                    x += w + gap * 2.0;
                    r
                } else {
                    Rect { x: rect.x, y: rect.y + i as f32 * (h + gap), width: rect.width, height: h }
                }
            })
            .collect()
    }

    /// The option under (`px`, `py`).
    fn option_at(&self, rect: Rect, px: f32, py: f32) -> Option<usize> {
        self.option_rects(rect)
            .iter()
            .position(|r| px >= r.x && px <= r.x + r.width && py >= r.y && py <= r.y + r.height)
    }

    /// An option's box: the check box's well, round.
    pub fn ring(option: Rect) -> Field {
        let side = option.height.max(1.0);
        let square = Rect { x: option.x, y: option.y, width: side, height: side };
        let depth = crate::layout::bevel_width().min(side * 0.2);
        let r = side * 0.5;
        let (outline, _) = crate::layout::carve_inside(square, (r, r, r, r), depth);
        // Round at the carved outline's own size, whatever the carve did to the corner.
        let r = outline.width.min(outline.height) * 0.5;
        Field::well(outline, (r, r, r, r), depth)
    }

    fn choose(&mut self, idx: usize) -> bool {
        if idx >= self.options.len() || idx == self.selected {
            return false;
        }
        self.selected = idx;
        self.just_changed = true;
        true
    }
}

impl Adapted<RadioGroup> {
    /// Lay the options out in a row instead of a column.
    pub fn with_horizontal(mut self, horizontal: bool) -> Self {
        self.inner_mut().horizontal = horizontal;
        self
    }

    pub fn with_selected(mut self, idx: usize) -> Self {
        self.inner_mut().set_selected(idx);
        self
    }
}

impl Layout for RadioGroup {
    fn inline_label(&self) -> bool {
        true
    }

    fn intrinsic_size(&self) -> Option<Size> {
        let (h, gap, n) = (Self::row_h(), Self::gap(), self.options.len() as f32);
        if self.horizontal {
            let w: f32 = self.option_rects(Rect { x: 0.0, y: 0.0, width: 0.0, height: h }).iter().map(|r| r.width).sum::<f32>()
                + gap * 2.0 * (n - 1.0).max(0.0);
            Some(Size::new(w, h))
        } else {
            Some(Size::new(0.0, n * h + (n - 1.0).max(0.0) * gap))
        }
    }
}

impl Paint for RadioGroup {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (_, font_size) = crate::layout::control_label_font_parsed();
        for (i, (option, label)) in self.option_rects(rect).into_iter().zip(&self.options).enumerate() {
            let chosen = i == self.selected;
            let lit = chosen && self.focused;
            let ring = Self::ring(option).with_tint(lit.then(crate::scene::paint::ControlPlate::focus_tint));
            super::checkbox::paint_box(ctx, &ring, None, false, lit);
            if chosen {
                let o = ring.rect;
                let (cx, cy, r) = (o.x + o.width * 0.5, o.y + o.height * 0.5, o.width.min(o.height) * BEAD_SHARE * 0.5);
                // The disc first, for a flat host that drops the sphere; the sphere covers it.
                ctx.circle(cx, cy, r, BEAD);
                ctx.sphere(cx, cy, r, &crate::scene::material::Material::from_fill(BEAD));
            }
            let ty = crate::layout::align_text_y(option.y, option.height, font_size, 0.0);
            let lx = option.x + option.height + LABEL_GAP;
            ctx.text_with(
                label.clone(),
                lx,
                ty,
                font_size,
                crate::color::control_label_color_for_state(false, lit),
                None,
                Some([option.x, option.y, rect.x + rect.width.max(option.width), option.y + option.height]),
            );
        }
    }
}

impl Input for RadioGroup {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }

    fn a11y_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::RadioGroup)
    }

    fn a11y_items(&self, rect: Rect) -> Vec<crate::a11y::A11yItem> {
        self.option_rects(rect)
            .into_iter()
            .zip(&self.options)
            .enumerate()
            .map(|(i, (r, label))| crate::a11y::A11yItem {
                role: accesskit::Role::RadioButton,
                label: label.clone(),
                rect: r,
                toggled: Some(i == self.selected),
                focused: self.focused && i == self.selected,
            })
            .collect()
    }

    fn a11y_select_item(&mut self, idx: usize) -> bool {
        self.choose(idx)
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, .. } => {
                // Already hit-gated by the adapter; a press between options chooses nothing.
                match self.option_at(ectx.rect, *x, *y) {
                    Some(i) => {
                        self.choose(i);
                        true
                    }
                    None => false,
                }
            }
            Event::FocusIn => {
                self.focused = true;
                true
            }
            Event::FocusOut => {
                self.focused = false;
                true
            }
            Event::KeyInput(key_event) => {
                if !self.focused || key_event.state != ElementState::Pressed || self.options.is_empty() {
                    return false;
                }
                let n = self.options.len();
                let target = match key_event.logical_key {
                    Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowRight) => (self.selected + 1) % n,
                    Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowLeft) => (self.selected + n - 1) % n,
                    Key::Named(NamedKey::Home) => 0,
                    Key::Named(NamedKey::End) => n - 1,
                    // Space / Enter: the focused option is already the chosen one.
                    Key::Named(NamedKey::Space) | Key::Named(NamedKey::Enter) => return true,
                    _ => return false,
                };
                self.choose(target);
                true
            }
            _ => false,
        }
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    /// The chosen option's label.
    fn value_string(&self) -> Option<String> {
        self.options.get(self.selected).cloned()
    }

    /// Choose by label, or by index.
    fn set_value_string(&mut self, val: &str) -> bool {
        let val = val.trim();
        let idx = self.options.iter().position(|o| o == val).or_else(|| val.parse::<usize>().ok());
        idx.is_some_and(|i| self.choose(i))
    }

    fn value(&self) -> i32 {
        self.selected as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{UiContext, WidgetHost};

    fn key(k: NamedKey) -> Event {
        Event::KeyInput(crate::widget::KeyEvent {
            logical_key: Key::Named(k),
            state: ElementState::Pressed,
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
            alt: false,
        })
    }

    /// The arrows move the choice, wrapping, and only while focused; a press chooses the
    /// option under it.
    #[test]
    fn the_arrows_and_a_press_choose_an_option() {
        let mut ctx = UiContext::new();
        let mut g = RadioGroup::new(["Small", "Medium", "Large"]);
        WidgetHost::set_rect(&mut g, 10.0, 10.0, 200.0, 200.0);
        assert!(!g.handle_event(&key(NamedKey::ArrowDown), &mut ctx), "unfocused: not its key");
        g.handle_event(&Event::FocusIn, &mut ctx);
        g.handle_event(&key(NamedKey::ArrowDown), &mut ctx);
        assert_eq!(g.inner().selected(), 1);
        assert!(g.take_change());
        g.handle_event(&key(NamedKey::ArrowUp), &mut ctx);
        g.handle_event(&key(NamedKey::ArrowUp), &mut ctx);
        assert_eq!(g.inner().selected(), 2, "wraps to the last");
        g.handle_event(&key(NamedKey::Home), &mut ctx);
        assert_eq!(g.inner().selected(), 0);

        let third = g.inner().option_rects(Rect { x: 10.0, y: 10.0, width: 200.0, height: 200.0 })[2];
        let (px, py) = (third.x + 4.0, third.y + third.height * 0.5);
        g.handle_event(
            &Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x: px, y: py, local_x: px, local_y: py },
            &mut ctx,
        );
        assert_eq!(g.inner().selected(), 2, "a press on the third chooses it");
        assert_eq!(g.get_value_string().as_deref(), Some("Large"));
    }

    /// A reader sees a radio group of radio buttons, the chosen one checked; a click on one
    /// chooses it.
    #[test]
    fn a_reader_sees_and_chooses_radio_buttons() {
        let mut g = RadioGroup::new(["Small", "Medium"]);
        WidgetHost::set_rect(&mut g, 0.0, 0.0, 200.0, 80.0);
        let items = g.a11y_items();
        assert_eq!(items.len(), 2);
        assert_eq!((items[0].role, items[0].label.as_str(), items[0].toggled), (accesskit::Role::RadioButton, "Small", Some(true)));
        assert_eq!(items[1].toggled, Some(false));
        assert!(g.a11y_select_item(1));
        assert_eq!(g.inner().selected(), 1);
        assert!(g.take_change());
        assert!(!g.a11y_select_item(1), "already chosen: no change");
    }

    /// Every option's box is round: the check box's well at a full corner.
    #[test]
    fn an_option_is_a_round_well() {
        let ring = RadioGroup::ring(Rect { x: 0.0, y: 0.0, width: 200.0, height: 24.0 });
        assert!((ring.radii.0 * 2.0 - ring.rect.width).abs() < 1e-3 && ring.rect.width == ring.rect.height);
    }
}
