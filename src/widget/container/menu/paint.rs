//! What a `MenuBar` draws: its background (or recess), the title (straight or curved), the menu
//! strip, and the open dropdowns through the popover hooks.

use super::*;

impl Paint for MenuBar {
    fn color(&self) -> [f32; 4] {
        self.bg_color()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        // Corners never round (the root plate-adjacency source is gone). The radius was the
        // parent's, read through a stored pointer — but nothing ever set_parent's a MenuBar,
        // so 0.0 is what production always read (6bd: the dead pointer field is gone).
        Some((0.0, (false, false, false, false)))
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::menubar_font())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
        self.layout_dirty = true;
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        if self.recessed() {
            // No background of our own: carve the root plate instead. The recess shading is
            // a light/shadow overlay, so whatever the plate painted here (fill, rim
            // gradient, blur) shows through modulated.
            // `depth` is the roll-off width in px (the shading amplitude is separate: the
            // renderer applies `bevel_depth` itself), capped so a deep DE-wide setting can
            // never swallow a short bar — the two walls would meet in the middle and the
            // flat floor would vanish.
            if rect.x <= 0.5 && rect.y <= 0.5 {
                // Flush with the plate's top-left: the bar is a plateau one step down, not a
                // trough, so its only wall is the one facing the content. The other three
                // sides are the plate's outer edge, where the plate's own roll already lives
                // — carving there too would cut a second lip into the same pixels. One wall
                // straddling the boundary intrudes only half its width, so the cap is looser
                // than the trough's.
                // The wall stays inside the bar (`layout::carve_inside`).
                let depth = crate::layout::bar_wall_width().min(rect.height * 0.6);
                let bar = Rect { height: rect.height - depth * 0.5, ..rect };
                ctx.recess_edges(bar, (0.0, 0.0, 0.0, 0.0), depth, (false, false, true, false));
            } else {
                // Inset from the plate edge: a real trough, walled all round, its corners
                // rounded by the roll itself.
                let depth = crate::layout::bar_wall_width().min(rect.height * 0.4);
                let (well, radii) = crate::layout::carve_inside(rect, (depth, depth, depth, depth), depth);
                ctx.recess(well, radii, depth);
            }
        } else {
            // Background: always the plain quad — the rounded-against-parent variant required a
            // root plate parent, which no longer exists.
            ctx.quad(rect, self.bg_color());
        }

        // Dropdown-trigger chrome shared by the context title and the menu
        // buttons on horizontal bars: the DE-wide closed-dropdown look — a
        // flush plate with a field run's edge and a transparent face
        // (Dropdown::paint_background's raised path) on a band-inset rect, with
        // the state fill rounded to sit inside it. The vertical and curved
        // modes keep their plain quads — their geometry is exotic and gets no
        // trough.
        let flat_modes = self.vertical || self.curved_circle.is_some();
        let trough_chrome = |ctx: &mut PaintCtx, r: (f32, f32, f32, f32), fill: Option<[f32; 4]>| {
            let trough_h = DROPDOWN_ITEM_H.min((r.3 - 6.0).max(8.0));
            let trough = Rect { x: r.0, y: r.1 + (r.3 - trough_h) / 2.0, width: r.2, height: trough_h };
            let radius = crate::layout::dropdown_corner_radius();
            let depth = crate::layout::bevel_width().min(trough_h * 0.2);
            let (trough, radii) = crate::layout::carve_inside(trough, (radius, radius, radius, radius), depth);
            ctx.inset_plate(trough, radii, None, depth);
            if let Some(c) = fill {
                ctx.rounded_rect(trough, radius, (true, true, true, true), c);
            }
        };

        // Context-dropdown trigger (the bar's folder/pane selector).
        if !self.context_options.is_empty() {
            let tr = self.title_rect(rect);
            if flat_modes {
                if self.context_dropdown_open {
                    ctx.quad(Rect { x: tr.0, y: tr.1, width: tr.2, height: tr.3 }, colors::highlight_primary_color());
                } else if self.context_title_hovered {
                    ctx.quad(Rect { x: tr.0, y: tr.1, width: tr.2, height: tr.3 }, colors::HIGHLIGHT_SECONDARY);
                }
            } else {
                let fill = if self.context_dropdown_open {
                    Some(colors::highlight_primary_color())
                } else if self.context_title_hovered {
                    Some(colors::HIGHLIGHT_SECONDARY)
                } else {
                    None
                };
                trough_chrome(ctx, tr, fill);
            }
        }

        // The embedded strip's geometry (it is not a tree child; its pixels are
        // ours). Horizontal bars restyle the menu buttons as dropdown triggers
        // too: each gets its own trough (side-inset so adjacent troughs keep
        // separate groove rings), and the strip's full-height square state
        // quads are replaced by the trigger fills — open matches the context
        // dropdown's open tint rather than the strip's legacy palette.
        let strip = crate::widget::shown_prims(&self.menus);
        if flat_modes {
            for prim in &strip {
                if let crate::scene::paint::Prim::Quad { rect, color } = prim {
                    ctx.quad(*rect, *color);
                }
            }
        } else {
            for i in 0..self.menus.buttons.len() {
                let r = self.menus.item_rect(i);
                let r = (r.0 + 3.0, r.1, (r.2 - 6.0).max(8.0), r.3);
                let fill = if Some(i) == self.menus.selected {
                    Some(colors::highlight_primary_color())
                } else if Some(i) == self.menus.pressed_idx {
                    Some(colors::BUTTON_PRESS)
                } else if Some(i) == self.menus.hovered_idx {
                    Some(colors::HIGHLIGHT_SECONDARY)
                } else {
                    None
                };
                trough_chrome(ctx, r, fill);
            }
        }
        for prim in strip {
            match prim {
                crate::scene::paint::Prim::Arc { cx, cy, radius, thickness, start, end, color } => {
                    ctx.arc(cx, cy, radius, thickness, start, end, color);
                }
                crate::scene::paint::Prim::Circle { cx, cy, radius, color } => ctx.circle(cx, cy, radius, color),
                _ => {}
            }
        }

        // Text: the sidebar label (vertical), the title (curved / vertical / horizontal), and
        // the strip's button labels — the legacy `text_labels` body.
        let label_color = self.text_color();
        let srgb = crate::colors::to_srgb(label_color);
        let text_color = [
            (srgb[0] * 255.0) as u8,
            (srgb[1] * 255.0) as u8,
            (srgb[2] * 255.0) as u8,
        ];
        // The title's context arrow: the `chevron-down` glyph in the title's
        // colour, standing where the " ▼" it replaced stood (the title's
        // width is still measured with it, so nothing moves). A label at
        // (x, y) sized `fs` puts it there.
        let arrow_tint = [srgb[0], srgb[1], srgb[2], 1.0];
        let arrow_at = |ctx: &mut PaintCtx, x: f32, y: f32, fs: f32| {
            let side = (fs * 0.6).round();
            ctx.icon("chevron-down", Rect { x, y: y + 0.5 * (fs - side), width: side, height: side }, arrow_tint);
        };
        let padding_x = crate::layout::paginator_tab_padding_x();

        if let Some(ref label) = self.label {
            if self.vertical {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let start_y = rect.y + 16.0;
                for (i, c) in label.chars().enumerate() {
                    let char_str = c.to_string();
                    let char_w = crate::widget::display::measure_text(&char_str, font_size);
                    let x_pos = rect.x + (rect.width - char_w) / 2.0;
                    let y_pos = start_y + i as f32 * line_height;
                    ctx.text_with(char_str, x_pos, y_pos, font_size, [0x83, 0x83, 0x8a], None,
                        Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]));
                }
            }
        }

        let font_setting = crate::layout::menubar_font();
        let (font_fam, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        let mut display_title = self.title.clone();
        if !self.context_options.is_empty() {
            display_title.push_str(" ▼");
        }

        if let Some((ccx, ccy, ccr)) = self.curved_circle {
            let r_mid = ccr - rect.height / 2.0;
            let mut total_width = 8.0;
            if !self.title.is_empty() {
                total_width += display_title.len() as f32 * char_w + 24.0;
            }
            for btn_label in &self.menus.buttons {
                total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
            }
            let total_angular_width = total_width / r_mid;
            let start_angle = 1.5 * std::f32::consts::PI - total_angular_width / 2.0;
            let current_angle = start_angle;

            if !self.title.is_empty() {
                let title_w = display_title.len() as f32 * char_w + 24.0;
                let dtheta_title = title_w / r_mid;
                for l in TextLabel::curved_layout(
                    &display_title,
                    ccx, ccy, r_mid,
                    current_angle, current_angle + dtheta_title,
                    font_size,
                    text_color,
                ) {
                    if l.text == "▼" {
                        arrow_at(ctx, l.x, l.y, l.font_size);
                        continue;
                    }
                    ctx.text_with(l.text, l.x, l.y, l.font_size, l.color, None,
                        Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]));
                }
            }
        } else if self.vertical {
            if !self.title.is_empty() {
                let mut start_y = rect.y + 16.0;
                if let Some(ref label) = self.label {
                    let font_size = 12.0;
                    let line_height = font_size * 1.2;
                    let label_h = label.chars().count() as f32 * line_height;
                    start_y += label_h + 20.0;
                }
                let line_height = font_size * 1.2;
                let char_w = crate::widget::display::measure_text("o", font_size);
                let x_pos = rect.x + (rect.width - char_w) / 2.0;
                for (i, c) in self.display_title().chars().enumerate() {
                    let char_str = c.to_string();
                    let y_pos = start_y + i as f32 * line_height;
                    if c == '▼' {
                        arrow_at(ctx, x_pos, y_pos, font_size);
                        continue;
                    }
                    ctx.text_with(char_str, x_pos, y_pos, font_size, text_color, None,
                        Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]));
                }
            }
        } else if !self.title.is_empty() {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.right_align_title {
                    total_width += display_title.len() as f32 * char_w + 24.0;
                }
                for btn_label in &self.menus.buttons {
                    total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
                }
                if rect.width > total_width {
                    start_x = (rect.width - total_width) / 2.0;
                }
            }
            let text_y = crate::layout::align_text_y(rect.y, rect.height, font_size, 0.0);
            let x_pos = if self.right_align_title {
                let title_w = crate::widget::display::measure_text_width(&display_title, &font_fam, font_size) + 24.0;
                rect.x + rect.width - title_w - 20.0
            } else {
                rect.x + start_x
            };
            ctx.text_with(self.title.clone(), x_pos, text_y, font_size, text_color, None,
                Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]));
            if !self.context_options.is_empty() {
                let lead = format!("{} ", self.title);
                let ax = x_pos + crate::widget::display::measure_text_width(&lead, &font_fam, font_size);
                arrow_at(ctx, ax, text_y, font_size);
            }
        }

        // Every word this widget draws is bounded by the widget. A menu's
        // POPOVER is a separate pass with its own rect, so bounding the bar
        // here does not clip an open menu.
        let bar = Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]);
        for l in self.menus.own_labels() {
            ctx.text_with(l.text, l.x, l.y, l.font_size, l.color, None, bar);
        }
    }

    fn popover(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.context_popover_rect(rect).or_else(|| self.menu_dropdown_rect())
    }

    fn draw_popover(&self, rect: Rect, pc: &mut dyn crate::scene::paint::RenderTarget) {
        let label_color = self.text_color();
        let srgb = crate::colors::to_srgb(label_color);
        let color_f32 = [srgb[0], srgb[1], srgb[2], 1.0];
        let font = Paint::widget_font(self);

        // Both dropdown flavors paint in the Dropdown widget's popover idiom
        // (layered soft shadows, surface border/bg, accent hover, blue
        // selected) so menubar menus read as the DE's normal dropdowns.
        let draw_panel = |pc: &mut dyn crate::scene::paint::RenderTarget, dx: f32, dy: f32, dw: f32, dh: f32, hovered: Option<usize>| {
            let theme = colors::active_theme();
            pc.rect([0.02, 0.02, 0.05, 0.15], dx + 1.0, dy + 1.0, dw, dh);
            pc.rect([0.02, 0.02, 0.05, 0.08], dx + 3.0, dy + 3.0, dw, dh);
            pc.rect([0.02, 0.02, 0.05, 0.04], dx + 5.0, dy + 5.0, dw, dh);
            pc.rect(theme.surface_border, dx, dy, dw, dh);
            // Frosted, like the Dropdown popover and the context menu: the
            // popover material (`Material::popover`), encoded for the
            // colour-typed flat path — menus show what is beneath them
            // blurred and tinted, not covered.
            let bg = crate::scene::Material::popover(theme.surface_bg).fill(crate::scene::PlateRole::Nested);
            pc.rect(bg, dx + 1.0, dy + 1.0, dw - 2.0, dh - 2.0);
            if let Some(di) = hovered {
                let iy = dy + di as f32 * DROPDOWN_ITEM_H;
                pc.rect(theme.primary_accent, dx + 2.0, iy + 2.0, dw - 4.0, DROPDOWN_ITEM_H - 4.0);
            }
        };
        let item_color = |hovered: bool, selected: bool| -> [f32; 4] {
            let c: [u8; 3] = if hovered {
                [0xff, 0xff, 0xff]
            } else if selected {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };
            [c[0] as f32 / 255.0, c[1] as f32 / 255.0, c[2] as f32 / 255.0, 1.0]
        };
        let _ = color_f32;

        if self.context_dropdown_open {
            if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
                draw_panel(pc, dx, dy, dw, dh, self.context_hovered_item);
                let bounds = Some([dx, dy, dx + dw, dy + dh]);
                for (i, option) in self.context_options.iter().enumerate() {
                    let color = item_color(self.context_hovered_item == Some(i), self.context_selected == i);
                    let iy = crate::layout::align_text_y(dy + i as f32 * DROPDOWN_ITEM_H, DROPDOWN_ITEM_H, 12.0, 0.0);
                    if let Some(ref f) = font {
                        pc.text_with_font_and_bounds(option, dx + 8.0, iy, 12.0, color, f, bounds);
                    } else {
                        pc.text_with_bounds(option, dx + 8.0, iy, 12.0, color, bounds);
                    }
                }
            }
        } else if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
            draw_panel(pc, dx, dy, dw, dh, self.hovered_dropdown_item);
            let bounds = Some([dx, dy, dx + dw, dy + dh]);
            if let Some(menu_idx) = self.menus.selected {
                if let Some(items) = self.menu_dropdowns.get(menu_idx) {
                    for (i, option) in items.iter().enumerate() {
                        let checked = self.menu_dropdown_checked.get(menu_idx)
                            .and_then(|menu| menu.get(i))
                            .and_then(|&v| v);
                        // A checkable item keeps a mark's room before its
                        // text; a checked one wears the `check` glyph in it.
                        const MARK: f32 = 9.0;
                        let indent = if checked.is_some() { MARK + 6.0 } else { 0.0 };
                        let color = item_color(self.hovered_dropdown_item == Some(i), checked == Some(true));
                        let iy = crate::layout::align_text_y(dy + i as f32 * DROPDOWN_ITEM_H, DROPDOWN_ITEM_H, 12.0, 0.0);
                        if checked == Some(true) {
                            let my = dy + i as f32 * DROPDOWN_ITEM_H + 0.5 * (DROPDOWN_ITEM_H - MARK);
                            pc.icon("check", Rect { x: dx + 8.0, y: my, width: MARK, height: MARK }, color);
                        }
                        if let Some(ref f) = font {
                            pc.text_with_font_and_bounds(option, dx + 8.0 + indent, iy, 12.0, color, f, bounds);
                        } else {
                            pc.text_with_bounds(option, dx + 8.0 + indent, iy, 12.0, color, bounds);
                        }
                    }
                }
            }
        }
    }
}
