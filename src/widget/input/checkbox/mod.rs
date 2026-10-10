//! `Checkbox` and `Toggle`, the two on/off controls. Both are fields (`scene::paint::Field`): a
//! check box is a square well, with a square plate standing in it when checked; a toggle is a
//! field whose run glides from one end to the other. Both are inline-label widgets: they paint
//! their own label (its colour following hover and focus) inside their rect, so they track
//! `hovered` / `focused` themselves from the `MouseEnter` / `MouseLeave` / `FocusIn` / `FocusOut`
//! events the adapter forwards.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the box both share (`paint_box`, also the radio group's), the value parser, `Checkbox` whole |
//! | `toggle` | `Toggle` whole |

mod toggle;
#[cfg(test)]
mod tests;

pub use toggle::Toggle;

use crate::color;
use crate::scene::layout::Rect;
use crate::scene::paint::{Field, PaintCtx};
use crate::widget::{
    Adapted, ElementState, Event, EventCtx, Input, Justification, Layout, MouseButton, Paint,
};

/// How much of a checked box's side its plate takes, standing square in the
/// middle of the well — enough to read as a plate, little enough to leave the
/// well showing all round it, which is what tells it from an empty box. (All
/// run, a box filled flush, was tried first: at a control's size its outline
/// is the empty well's, and the two states were hard to tell apart. Then the
/// toggle's run, half the width and the field's whole height: in a square box
/// that is a tall bar, and a ticked box read as having narrowed.)
const PLATE_SHARE: f32 = 0.6;

/// Paint a check box — its well ([`Checkbox::box_field`]) and, checked, the
/// plate in it ([`Checkbox::box_plate`]): both fields with relief on; off,
/// as the toggle's — a well is its frame, the hairline every well falls back
/// to (lit by `hovered` / `focused`), and the plate a lit face in it.
pub(crate) fn paint_box(ctx: &mut PaintCtx, well: &Field, plate: Option<&Field>, hovered: bool, focused: bool) {
    if crate::layout::control_relief() {
        ctx.field(well);
        if let Some(plate) = plate {
            ctx.field(plate);
        }
        return;
    }
    let bw = crate::layout::toggle_border_width().max(1.0);
    ctx.border(well.rect, well.radii, [0.0; 4], color::well_frame_color(hovered, focused), bw);
    if let Some(plate) = plate {
        let (face, face_r) = inset(plate.rect, plate.radii.0, plate.depth * 0.5);
        let lit = (0.16 * (crate::layout::bevel_depth() / 0.15)).clamp(0.0, 0.5);
        ctx.rounded_rect(face, face_r, (true, true, true, true), [1.0, 1.0, 1.0, lit]);
    }
}

/// A rect shrunk by `g` on every side, its uniform corner radius shrunk to
/// match so the inner silhouette stays concentric with the outer one.
fn inset(rect: Rect, radius: f32, g: f32) -> (Rect, f32) {
    (
        Rect {
            x: rect.x + g,
            y: rect.y + g,
            width: (rect.width - 2.0 * g).max(0.0),
            height: (rect.height - 2.0 * g).max(0.0),
        },
        (radius - g).max(0.0),
    )
}

fn parse_bool(val: &str) -> Option<bool> {
    match val.trim().to_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Some(true),
        "false" | "0" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// A check box: a square well ([`Field::well`]) with an optional label to
/// its right — empty unchecked; checked, a flush square plate standing in the
/// middle of it ([`Field::run`] on `PLATE_SHARE` of its side), the well
/// showing all round. Not the toggle's run with no travel, though it was: a
/// run spans its field's whole height, so in a square it is a tall bar.
/// Standalone, the box is the largest square in the rect, centred.
///
/// A check drawn inline in text — a list row (cce-list), a markdown task
/// item, the doc editor — is the same box, through [`Checkbox::paint_inline`].
/// Until 2026-10-02 the widget and all of those drew a ring-and-dot mark.
pub struct Checkbox {
    checked: bool,
    just_clicked: bool,
    pub just_changed: bool,
    label: Option<String>,
    hovered: bool,
    focused: bool,
}

impl Checkbox {
    /// Half the side of an inline box ([`Checkbox::paint_inline`]) at a
    /// list row's size: a 14px square, the size of the round mark it
    /// replaced.
    pub const INLINE_HALF: f32 = 7.0;

    /// The box's well: a square as tall as the control (at most a toggle's
    /// height) at the rect's left when labelled, the largest square in the
    /// rect, centred, when not ([`Checkbox::box_field`]). Lit while focused.
    pub fn field(&self, rect: Rect) -> Field {
        let side = if self.label.is_some() {
            rect.height.min(crate::layout::toggle_height())
        } else {
            rect.width.min(rect.height)
        }
        .max(1.0);
        let x = if self.label.is_some() { rect.x } else { rect.x + (rect.width - side) * 0.5 };
        let square = Rect { x, y: rect.y + (rect.height - side) * 0.5, width: side, height: side };
        Self::box_field(square).with_tint(self.focused.then(crate::scene::paint::ControlPlate::focus_tint))
    }

    /// A check box's well on `square` — its footprint — carved inside it
    /// through [`crate::layout::carve_inside`] as every field is. With
    /// [`Checkbox::box_plate`], the ONE description the widget and every
    /// inline box share.
    ///
    /// The corner is the toggle's in PROPORTION — its radius over its height
    /// — so a box as tall as a toggle has the toggle's corner exactly, and a
    /// 14px box in a line of text is a rounded square rather than a disc,
    /// which the toggle's radius taken whole would make it.
    pub fn box_field(square: Rect) -> Field {
        let side = square.width.min(square.height).max(1.0);
        let r = (crate::layout::toggle_corner_radius() * side / crate::layout::toggle_height().max(1.0)).min(side * 0.5);
        let depth = crate::layout::bevel_width().min(side * 0.2);
        let (outline, radii) = crate::layout::carve_inside(square, (r, r, r, r), depth);
        Field::well(outline, radii, depth)
    }

    /// A checked box's plate in `well` ([`Checkbox::box_field`]): a square
    /// `PLATE_SHARE` of its side, centred, all run — its corner the well's
    /// in proportion, so it is the box again, smaller.
    pub fn box_plate(well: &Field) -> Field {
        let o = well.rect;
        let side = (o.width.min(o.height) * PLATE_SHARE).max(1.0);
        let rect = Rect { x: o.x + (o.width - side) * 0.5, y: o.y + (o.height - side) * 0.5, width: side, height: side };
        let r = well.radii.0 * side / o.width.max(1.0);
        Field::run(rect, (r, r, r, r), well.depth.min(side * 0.2))
    }

    /// Paint a check box centred on (`cx`, `cy`), `half` its half-side, for
    /// a host that draws one inline — cce-list's rows, a markdown task item,
    /// the doc editor — where the widget would be a whole control. The same
    /// box the widget draws ([`Checkbox::box_field`], [`Checkbox::box_plate`]),
    /// relief on or off.
    pub fn paint_inline(ctx: &mut PaintCtx, cx: f32, cy: f32, half: f32, checked: bool) {
        let square = Rect { x: cx - half, y: cy - half, width: 2.0 * half, height: 2.0 * half };
        let well = Self::box_field(square);
        paint_box(ctx, &well, checked.then(|| Self::box_plate(&well)).as_ref(), false, false);
    }

    pub fn new() -> Adapted<Checkbox> {
        Adapted::new(Checkbox {
            checked: false,
            just_clicked: false,
            just_changed: false,
            label: None,
            hovered: false,
            focused: false,
        })
    }

    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    pub fn checked(&self) -> bool {
        self.checked
    }

    /// Hover state, also settable directly for immediate-mode hosts that do their own
    /// hit-testing instead of routing `MouseEnter`/`MouseLeave` (json_layout).
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }
}

impl Layout for Checkbox {
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for Checkbox {
    fn color(&self) -> [f32; 4] {
        // The box is carved; the widget itself has no background.
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        // The box leads and the label follows, a list row's reading order.
        let field = self.field(rect);
        let plate = self.checked.then(|| Self::box_plate(&field));
        paint_box(ctx, &field, plate.as_ref(), self.hovered, self.focused);
        if let Some(ref label) = self.label {
            let (_, font_size) = crate::layout::control_label_font_parsed();
            let ty = crate::layout::align_text_y(y, h, font_size, 0.0);
            // From the box's footprint, not its carved outline.
            let box_right = field.rect.x + field.rect.width + field.depth * 0.5;
            ctx.text_with(
                label.clone(),
                box_right + 8.0,
                ty,
                font_size,
                color::control_label_color_for_state(self.hovered, self.focused),
                None,
                // The label is caller text and the box is caller-sized; a
                // control has no business drawing past its own rect.
                Some([x, y, x + w, y + h]),
            );
        }
    }
}

impl Input for Checkbox {
    fn focus_role(&self) -> crate::widget::FocusRole {
        crate::widget::FocusRole::Plate
    }
    fn on_event(&mut self, event: &Event, _ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                // Already hit-gated by the adapter.
                self.checked = !self.checked;
                self.just_clicked = true;
                self.just_changed = true;
                true
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            Event::KeyInput(key_event) => {
                // A focused plate is pressed by Enter / Space, as a Button is.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    crate::widget::Key::Named(crate::widget::NamedKey::Enter)
                    | crate::widget::Key::Named(crate::widget::NamedKey::Space) => {
                        self.checked = !self.checked;
                        self.just_clicked = true;
                        self.just_changed = true;
                        true
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_clicked)
    }

    fn take_change(&mut self) -> bool {
        std::mem::take(&mut self.just_changed)
    }

    fn value_string(&self) -> Option<String> {
        Some(self.checked.to_string())
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let Some(new_checked) = parse_bool(val) else { return false };
        if self.checked != new_checked {
            self.checked = new_checked;
            self.just_changed = true;
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        if self.checked { 1 } else { 0 }
    }
}
