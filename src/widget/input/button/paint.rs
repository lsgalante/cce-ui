//! What a `Button` draws: its face colour by kind and state, its control plate (raised, flush or
//! flat), and its icon and label.

use super::*;

impl Paint for Button {
    fn color(&self) -> [f32; 4] {
        if self.pressed || self.hovered {
            if let Some(hbg) = self.hover_bg {
                return hbg;
            }
        } else if let Some(bg) = self.bg {
            return bg;
        }
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed {
                    colors::button_press_color()
                } else if self.hovered {
                    colors::button_hover_color()
                } else {
                    colors::button_background_color()
                }
            }
            ButtonKind::MenuItem => {
                // Idle is fully transparent so the shared recess reads as one
                // continuous well; only the hovered row lifts out of it.
                if self.pressed {
                    colors::button_press_color()
                } else if self.hovered {
                    colors::button_hover_color()
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                }
            }
            ButtonKind::Reset => {
                if self.pressed {
                    colors::RESET_BTN_PRESS
                } else if self.hovered {
                    colors::RESET_BTN_HOVER
                } else {
                    colors::RESET_BTN_IDLE
                }
            }
            ButtonKind::ListRow => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.6] }
                    else if self.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.4] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.hovered { [0.20, 0.20, 0.25, 0.15] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
            ButtonKind::CopyIcon => {
                // Pressed and hovered wear the same wash.
                let lit = self.pressed || self.hovered;
                if self.selected {
                    if lit { [0.30, 0.52, 0.78, 0.5] } else { [0.20, 0.40, 0.65, 0.2] }
                } else if lit {
                    [0.20, 0.20, 0.25, 0.25]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                }
            }
        }
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::button_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        if self.kind == ButtonKind::ListRow {
            Some(crate::layout::list_font())
        } else {
            Some(crate::layout::button_font())
        }
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let radius = crate::layout::button_corner_radius();
        let color = self.color();

        // Relief style: a flush plate — its face level with the surface, its
        // edge a field run's (a well's fall, mirrored back up to the face). Transparent fills degrade to
        // edges-only inside the groove (an opaque hover_color fills the face).
        // List rows are exempt: they are transparent-until-hover/selected
        // surfaces, and the edges-only groove would stack a permanent carved
        // ring on every idle row of a list.
        if let Some(plate) = self.plate(rect) {
            ctx.control_plate(&plate);
        } else {
            // ListRow also skips the border idiom below: it draws the border
            // color as a FULL rect with the fill inset over it, which only
            // reads as a 1px ring when the fill is opaque — a row's
            // transparent idle fill left the whole row painted in the config
            // button border_color (an accidental coupling).
            // Keyboard focus reuses the border the button already draws, tinted with
            // the DE's existing focus-border colour — no new geometry, and nothing
            // changes for a button that is not focused. It overrides the ListRow
            // opt-out too: a focused row must show the ring, which is the whole point.
            let border_color = if self.focused {
                Some(colors::tree_border_focus_color())
            } else if self.kind == ButtonKind::ListRow || self.kind == ButtonKind::MenuItem {
                None
            } else {
                colors::button_border_color()
            };
            // Background (+ optional configured border), split by radius exactly as the legacy
            // `all_rounded_quads` (rounded) / `extra_quads` (square) overrides emitted it.
            if radius > 0.0 {
                if let Some(bc) = border_color {
                    ctx.rounded_rect(rect, radius, (true, true, true, true), bc);
                    ctx.rounded_rect(
                        Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: h - 2.0 },
                        (radius - 1.0).max(0.0),
                        (true, true, true, true),
                        color,
                    );
                } else if color[3].abs() > 0.001 {
                    ctx.rounded_rect(rect, radius, (true, true, true, true), color);
                }
            } else if let Some(bc) = border_color {
                ctx.quad(rect, bc);
                ctx.quad(Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: h - 2.0 }, color);
            } else if color[3].abs() > 0.001 {
                ctx.quad(rect, color);
            }
        }

        // Icon face: replaces the label. Geometry from `icon_rect` — see there
        // for why it is not inlined here.
        if let Some((image, rect, alpha)) = self.icon_rect(rect) {
            ctx.image(image, rect, alpha);
            return;
        }

        // Label, with per-kind justification/color (legacy `text_labels`).
        if let Some(ref label) = self.label {
            let (_, font_size) = self.font();
            let est_w = self.label_width(label);
            let color = if let Some(lc) = self.label_color {
                [(lc[0] * 255.0) as u8, (lc[1] * 255.0) as u8, (lc[2] * 255.0) as u8]
            } else {
                match self.kind {
                    ButtonKind::ListRow | ButtonKind::CopyIcon => {
                        if self.selected { [230, 230, 242] } else { [178, 178, 191] }
                    }
                    _ => colors::control_label_color_u8(),
                }
            };
            let justify = if self.kind == ButtonKind::ListRow {
                match crate::layout::list_justification() {
                    0 => Justification::Left,
                    2 => Justification::Right,
                    _ => Justification::Center,
                }
            } else {
                self.justify
            };
            let tx = match justify {
                Justification::Left => x + 8.0,
                Justification::Right => x + w - est_w - 8.0,
                Justification::Center => x + (w - est_w) / 2.0,
            };
            // A button is sized by its ROW, not by its label — `row_layout`
            // divides a section's width evenly — so a long label in a narrow
            // button makes every one of these go negative relative to the
            // plate: centring put a 26-character label 78px to the LEFT of its
            // own button, running out both sides over whatever sat beside it.
            // Clamp the start to the plate's text inset, and clip to the plate
            // itself rather than to that inset, so a label which merely grazes
            // the inset (the width here is an estimate) is not shaved for it.
            let tx = tx.max(x + 8.0);
            ctx.text_with(
                label.clone(),
                tx,
                crate::layout::align_text_y(y, h, font_size, 0.0),
                font_size,
                color,
                None,
                Some([x, y, x + w, y + h]),
            );
        }
    }
}
