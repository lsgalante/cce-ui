//! What the tree list draws: its well (radius, corners, border), the embedded fields, the rows
//! with their chevrons, values and swatches, and `impl Paint`.

use super::*;

impl Paint for TreeList {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::tree_font())
    }

    // The field widgets are the context's but not tree-linked (self-routing, see
    // register_embedded_children); their pixels come from `paint_ui`'s child pass — the
    // walk must not descend either.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    /// Shape the fields the tree still holds; once they are the context's, the runner
    /// shapes them with every other widget it has.
    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem, _rect: Rect) {
        if let Some(f) = self.search_box.here_mut() {
            f.prepare_text(fs);
        }
        if let Some(b) = self.add_key_btn.here_mut() {
            b.prepare_text(fs);
        }
        if self.add_key_popover_open {
            if let Some(f) = self.add_key_popover_box.here_mut() {
                f.prepare_text(fs);
            }
        }
        if self.editing_key_idx.is_some() {
            if let Some(f) = self.edit_box.here_mut() {
                f.prepare_text(fs);
            }
        }
    
    }

    /// The whole tree — container border/background, search/header chrome, virtualized
    /// rows (backgrounds, separators, color previews, button pills), the scrollbar, the
    /// row/header labels, and the field children (search box, add-key button, the add-key
    /// popover box while open, the inline rename editor while editing). Ported verbatim
    /// from the legacy `all_rounded_quads` rounded branch + `subtree_fonted_labels`;
    /// children paint through their own adapters (dummy ctx — none of their paint reads it).
    fn paint(&self, rect: Rect, pc: &mut PaintCtx) {
        let (r1, r2, r3, r4) = self.tree_corners();
        let mut quads: Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> = Vec::new();
        let radius = self.tree_radius();
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        
        let opacity = crate::layout::tree_opacity();
        let apply_opacity = |mut c: [f32; 4]| -> [f32; 4] {
            c[3] *= opacity;
            c
        };

        // 0. The scrollbar's idle copy, BEFORE the plate: sunk behind it, it
        // shows dimly through the translucent fill and takes no press. The
        // fore copy fades in over the rows (below) while a scroll holds it.
        self.scroll_box.paint_scrollbar_pills(pc, 1.0);

        // 1. Draw container border and background. A true fill + border ring
        // (not the legacy full-rect border punched out by the background quad,
        // which read as a whole-pane border_color wash once the background
        // went transparent).
        if let Some((border_color, thickness)) = self.tree_border() {
            let rr = |on: bool| if on { radius } else { 0.0 };
            pc.border(
                Rect { x, y, width: w, height: h },
                (rr(r1), rr(r2), rr(r3), rr(r4)),
                apply_opacity(crate::color::tree_background_color()),
                apply_opacity(border_color),
                thickness,
            );
        } else {
            quads.push((x, y, w, h, radius, apply_opacity(crate::color::tree_background_color()), (r1, r2, r3, r4)));
        }

        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;

        // Draw Header border (no background fill — the header sits directly on
        // the pane surface, like the rows).
        let header_h = 26.0;
        let header_border_color = [0.18, 0.18, 0.22, 1.0];

        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;

        // Separator line below header
        quads.push((list_left + 1.0, y + offset_y + header_h - 1.0, list_width - 2.0, 1.0, 0.0, apply_opacity(header_border_color), (false, false, false, false)));
        
        // Vertical separators inside header
        quads.push((list_left + 180.0, y + offset_y + 1.0, 1.0, header_h - 2.0, 0.0, apply_opacity(header_border_color), (false, false, false, false)));
        quads.push((list_left + 235.0, y + offset_y + 1.0, 1.0, header_h - 2.0, 0.0, apply_opacity(header_border_color), (false, false, false, false)));


        // 2. Draw items (row backgrounds, separator lines, color previews)
        let list_left = self.scroll_box.base.x;
        let list_width = self.scroll_box.base.w;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        for (i, item) in self.items.iter().enumerate() {
            let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
            if row_y + self.item_height < list_top || row_y > list_bottom {
                continue;
            }
            
            let draw_y = row_y.max(list_top);
            let draw_bottom = (row_y + self.item_height).min(list_bottom);
            let draw_h = draw_bottom - draw_y;
            if draw_h <= 0.0 { continue; }
            
            let bg_color = match item {
                TreeElement::Section { .. } => {
                    if Some(i) == self.hovered_row_idx {
                        crate::color::tree_section_bg_hover_color()
                    } else {
                        crate::color::tree_section_bg_color()
                    }
                }
                TreeElement::Leaf { original_idx, .. } => {
                    if Some(*original_idx) == self.selected_key_idx {
                        crate::color::tree_leaf_bg_selected_color()
                    } else if Some(i) == self.hovered_row_idx {
                        crate::color::tree_leaf_bg_hover_color()
                    } else if i % 2 == 0 {
                        crate::color::tree_leaf_bg_even_color()
                    } else {
                        crate::color::tree_leaf_bg_odd_color()
                    }
                }
            };
            
            let row_r1 = false;
            let row_r2 = false;
            let mut row_r3 = false;
            let mut row_r4 = false;
            let row_radius = radius - 1.0;

            if draw_bottom >= list_bottom - radius {
                row_r3 = r3;
                row_r4 = r4;
            }

            quads.push((list_left + 1.0, draw_y, list_width - 2.0, draw_h, row_radius, apply_opacity(bg_color), (row_r1, row_r2, row_r3, row_r4)));
            
            if let TreeElement::Leaf { ref val, original_idx, .. } = item {
                let separator_color = crate::color::tree_separator_color();
                quads.push((list_left + 180.0, draw_y, 1.0, draw_h, 0.0, apply_opacity(separator_color), (false, false, false, false)));
                quads.push((list_left + 235.0, draw_y, 1.0, draw_h, 0.0, apply_opacity(separator_color), (false, false, false, false)));

                let mut is_button = false;
                if let Some(Some(ref anno)) = self.annotations.get(*original_idx) {
                    if anno == "button" || anno.starts_with("button:") {
                        is_button = true;
                    }
                }

                if Some(*original_idx) != self.selected_key_idx {
                    if is_button {
                        let btn_x = list_left + 245.0;
                        let btn_y = row_y + 1.0;
                        let btn_bottom = (row_y + 27.0).min(list_bottom);
                        let btn_draw_y = btn_y.max(list_top);
                        let btn_draw_h = btn_bottom - btn_draw_y;
                        if btn_draw_h > 0.0 {
                            let btn_bg = [0.10, 0.29, 0.33, 0.65]; // theme button color
                            quads.push((btn_x, btn_draw_y, 125.0, btn_draw_h, 4.0, apply_opacity(btn_bg), (true, true, true, true)));
                        }
                    } else if let serde_json::Value::String(s) = val {
                        if s.starts_with('#') {
                            if let Some(rgba) = parse_hex_f32(s) {
                                let preview_x = list_left + 245.0;
                                let preview_y = row_y + 4.0;
                                let preview_bottom = (row_y + 20.0).min(list_bottom);
                                let preview_draw_y = preview_y.max(list_top);
                                let preview_draw_h = preview_bottom - preview_draw_y;
                                if preview_draw_h > 0.0 {
                                    // Checkerboard pattern
                                    let grid_size = 8.0;
                                    quads.push((preview_x, preview_draw_y, 16.0, preview_draw_h, 0.0, [1.0, 1.0, 1.0, 1.0], (false, false, false, false)));
                                    let cols = (16.0f32 / grid_size).ceil() as i32;
                                    let rows = (preview_draw_h / grid_size).ceil() as i32;
                                    for r in 0..rows {
                                        for c in 0..cols {
                                            if (r + c) % 2 == 1 {
                                                let qx = preview_x + c as f32 * grid_size;
                                                let qy = preview_draw_y + r as f32 * grid_size;
                                                let qw = grid_size.min(preview_x + 16.0 - qx);
                                                let qh = grid_size.min(preview_draw_y + preview_draw_h - qy);
                                                if qw > 0.0 && qh > 0.0 {
                                                    quads.push((qx, qy, qw, qh, 0.0, [0.8, 0.8, 0.8, 1.0], (false, false, false, false)));
                                                }
                                            }
                                        }
                                    }
                                    quads.push((preview_x, preview_draw_y, 16.0, preview_draw_h, 0.0, rgba, (false, false, false, false)));
                                }
                            }
                        }
                    }
                }
            }

            if row_y + self.item_height <= list_bottom {
                let mut sep_r3 = false;
                let mut sep_r4 = false;
                if row_y + self.item_height >= list_bottom - radius {
                    sep_r3 = r3;
                    sep_r4 = r4;
                }
                quads.push((list_left + 1.0, row_y + self.item_height - 1.0, list_width - 2.0, 1.0, row_radius, apply_opacity([0.13, 0.13, 0.17, 1.0]), (false, false, sep_r3, sep_r4)));
            }
        }

        for (qx, qy, qw, qh, qr, qc, qcorners) in quads {
            if qr > 0.1 {
                pc.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, qcorners, qc);
            } else {
                pc.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
            }
        }

        // Recessed well like a text box or list: the tree floor sits below the
        // pane surface, its wall carved over the bg and row quads above. Focus
        // lights the rim in the highlight accent instead of washing the tree
        // (the retired legacy_focus_highlight overlay).
        if crate::layout::control_relief() {
            let depth = crate::layout::bevel_width().min(h * 0.2);
            let (well, radii) = crate::layout::carve_inside(Rect { x, y, width: w, height: h }, (radius, radius, radius, radius), depth);
            if self.focused {
                let hc = crate::color::highlight_primary_color();
                pc.recess_tinted(well, radii, depth, [hc[0], hc[1], hc[2]]);
            } else {
                pc.recess(well, radii, depth);
            }
        }

        // The scrollbar's fore copy, over the rows and the well's wall, at
        // the activity's fade.
        self.scroll_box.paint_scrollbar_pills(pc, self.scroll_box.scrollbar_fade());

        // Row/header labels with the legacy header/list viewport bounds.
        let font = Some(crate::layout::tree_font());
        let (x, y, w, _h) = (rect.x, rect.y, rect.width, rect.height);
        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;
        let header_h = 26.0;
        // Labels and chevrons paint over the recessed well's wall (the recess
        // is shading-only and already drawn), so cut them at the wall's inner
        // edge — content slides under the bevel instead of sitting on it. The
        // top stays at the viewport: the wall there is behind the search/header
        // strip, outside the scroll area.
        let wall = if crate::layout::control_relief() {
            crate::layout::bevel_width().min(rect.height * 0.2)
        } else {
            0.0
        };
        let list_bounds = Some([
            self.scroll_box.base.x + wall,
            self.scroll_box.viewport_y,
            self.scroll_box.base.x + self.scroll_box.base.w - wall,
            self.scroll_box.viewport_y + self.scroll_box.viewport_h - wall,
        ]);
        let header_bounds = Some([x, y + offset_y, x + w, y + offset_y + header_h]);
        for (idx, (l, col_max_x)) in self.own_labels().into_iter().enumerate() {
            let mut b = if idx < 3 { header_bounds } else { list_bounds };
            if let (Some(bb), Some(mx)) = (b.as_mut(), col_max_x) {
                bb[2] = bb[2].min(mx);
            }
            pc.text_with(l.text, l.x, l.y, l.font_size, l.color, font.clone(), b);
        }

        // Section chevrons: image icons in the slot own_labels leaves open,
        // clipped like the row text — to the list viewport shrunk by the
        // well wall, so arrows are cut off by the bevel, never drawn on it.
        {
            let (_, tree_font_size) = crate::layout::tree_font_parsed();
            let list_left = self.scroll_box.base.x;
            let list_top = self.scroll_box.viewport_y;
            let list_bottom = list_top + self.scroll_box.viewport_h;
            let viewport = Rect {
                x: list_left + wall,
                y: list_top,
                width: self.scroll_box.base.w - 2.0 * wall,
                height: self.scroll_box.viewport_h - wall,
            };
            pc.clip(viewport, |pc| {
                for (i, item) in self.items.iter().enumerate() {
                    let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
                    if row_y + self.item_height < list_top || row_y > list_bottom {
                        continue;
                    }
                    if let TreeElement::Section { indent, collapsed, .. } = item {
                        if self.editing_key_idx == Some(i) {
                            continue;
                        }
                        if let Some((id, _, _)) = Self::chevron_icon(*collapsed) {
                            let s = tree_font_size;
                            pc.image(id, Rect {
                                x: list_left + 8.0 + *indent as f32 * 12.0,
                                y: row_y + 7.0,
                                width: s,
                                height: s,
                            }, 1.0);
                        }
                    }
                }
            });
        }

        // Field children the tree still holds (no context: `paint_ui` paints the context's).
        let dummy = UiContext::new();
        self.paint_fields(&dummy, pc, true);
    }

    /// The tree, then its fields wherever they are — the context's through `ui`.
    fn paint_ui(&self, ui: &UiContext, rect: Rect, pc: &mut PaintCtx) {
        self.paint(rect, pc);
        self.paint_fields(ui, pc, false);
    }

    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        if self.add_key_popover_open {
            Some(self.popover_rect_geom())
        } else {
            None
        }
    
    }

    fn draw_popover(&self, _rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.add_key_popover_open { return; }
        
        let (rx, ry, rw, rh) = self.popover_rect_geom();
        
        // 1. Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], rx + 1.0, ry + 1.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.08], rx + 3.0, ry + 3.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.04], rx + 5.0, ry + 5.0, rw, rh);

        let theme = crate::color::active_theme();

        // 2. High-contrast premium outer border
        pc.rect(theme.surface_border, rx, ry, rw, rh);
        
        // 3. Frosted glass background
        pc.rect(theme.surface_bg, rx + 1.0, ry + 1.0, rw - 2.0, rh - 2.0); // bg
    
    }
}

pub(super) fn parse_hex_f32(s: &str) -> Option<[f32; 4]> {
    crate::color::parse_hex_rgba_linear(s)
}

impl TreeList {
    /// Row/header labels plus each label's column clip: the x where its
    /// column ends (None = only the shared list/header bounds apply). Key and
    /// Type cells clip at their separators so text can't bleed into the next
    /// column; sections span the whole row.
    /// The section-row chevron (cce-icons), cached per size by `upload_icon`;
    /// `None` when the icon set is missing (the slot is left empty).
    pub(super) fn chevron_icon(collapsed: bool) -> Option<(u32, u32, u32)> {
        crate::upload_icon(if collapsed { "chevron-right" } else { "chevron-down" }, 32)
    }

    pub(crate) fn own_labels(&self) -> Vec<(TextLabel, Option<f32>)> {
        let f32_to_rgb = |c: [f32; 4]| -> [u8; 3] {
            [
                (crate::color::linear_to_srgb(c[0]) * 255.0).round() as u8,
                (crate::color::linear_to_srgb(c[1]) * 255.0).round() as u8,
                (crate::color::linear_to_srgb(c[2]) * 255.0).round() as u8,
            ]
        };

        let (_, tree_font_size) = crate::layout::tree_font_parsed();
        let header_font_size = (tree_font_size - 1.0).max(8.0);

        let mut labels = Vec::new();
        let list_left = self.scroll_box.base.x;
        let list_top = self.scroll_box.viewport_y;
        let list_bottom = self.scroll_box.viewport_y + self.scroll_box.viewport_h;

        let search_margin_y = 6.0;
        let search_h = 26.0;
        let offset_y = search_h + 2.0 * search_margin_y;

        labels.push((TextLabel {
            text: "Key".to_string(),
            x: list_left + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        }, None));
        labels.push((TextLabel {
            text: "Type".to_string(),
            x: list_left + 180.0 + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        }, None));
        labels.push((TextLabel {
            text: "Value".to_string(),
            x: list_left + 235.0 + 8.0,
            y: self.base.y + offset_y + 6.0,
            font_size: header_font_size,
            color: [200, 200, 210],
        }, None));

        for (i, item) in self.items.iter().enumerate() {
            let row_y = list_top + i as f32 * self.item_height - self.scroll_box.scroll_y;
            if row_y + self.item_height < list_top || row_y > list_bottom {
                continue;
            }

            match item {
                TreeElement::Section { name, indent, .. } => {
                    if self.editing_key_idx != Some(i) {
                        let sx = list_left + 8.0 + *indent as f32 * 12.0;
                        // The chevron glyph (cce-icons) stands in the slot
                        // this leaves open — paint() draws it. A machine
                        // without the icon set shows the slot empty: no
                        // symbol is drawn as a character.
                        let (text, tx) = (name.clone(), sx + tree_font_size + 6.0);
                        labels.push((TextLabel {
                            text,
                            x: tx,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color: f32_to_rgb(crate::color::tree_section_text_color()),
                        }, None));
                    }
                }
                TreeElement::Leaf { name, indent, val, original_idx, .. } => {
                    // A unit-suffixed string (`"2mm"`, what `(mm)2.0` reads
                    // as) shows as the length it is — `2 mm` — not a quoted
                    // string; its unit is its type below.
                    let len = val.as_str().and_then(crate::units::Len::parse);
                    let val_str = match len {
                        Some(l) => format!("{} {}", crate::units::fmt_num(l.value), l.unit.suffix()),
                        None => serde_json::to_string(val).unwrap_or_default(),
                    };
                    // Generous shaping cap only — the column bounds clip the
                    // visible text at the list edge. (char-based: the old
                    // byte slice could panic on multibyte text.)
                    let display_val = if val_str.chars().count() > 120 {
                        let cut: String = val_str.chars().take(117).collect();
                        format!("{}...", cut)
                    } else {
                        val_str
                    };

                    let color = if Some(*original_idx) == self.selected_key_idx {
                        f32_to_rgb(crate::color::tree_leaf_text_selected_color())
                    } else {
                        f32_to_rgb(crate::color::tree_leaf_text_color())
                    };

                    if self.editing_key_idx != Some(i) {
                        labels.push((TextLabel {
                            text: name.clone(),
                            x: list_left + 8.0 + *indent as f32 * 12.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color,
                        }, Some(list_left + 178.0)));
                    }

                    let val_ty = match val {
                        serde_json::Value::Bool(_) => Some("bool"),
                        serde_json::Value::Number(num) => {
                            if num.is_f64() {
                                Some("f64")
                            } else {
                                Some("i64")
                            }
                        }
                        serde_json::Value::String(s) => {
                            if s.starts_with('#') {
                                let s_clean = s.trim_start_matches('#');
                                if s_clean.len() == 8 {
                                    Some("rgba")
                                } else {
                                    Some("rgb")
                                }
                            } else if name == "key" || name == "keybind" || name == "shortcut" || name == "open_search" || name == "close_search" || name == "delete" || name.ends_with("_key") || name.ends_with(".key") || name.ends_with(".keybind") || name.ends_with(".shortcut") || name.ends_with(".open_search") || name.ends_with(".close_search") || name.ends_with("_delete") || name.ends_with(".delete") {
                                Some("keybind")
                            } else if name == "font" || name.ends_with("_font") || name.ends_with(".font") {
                                Some("font")
                            } else { len.map(|l| l.unit.suffix()) }
                        }
                        _ => None,
                    };
                    let mut display_ty = val_ty.map(|s| s.to_string());
                    if let Some(Some(ref anno)) = self.annotations.get(*original_idx) {
                        if anno.starts_with("menu:") {
                            display_ty = Some("menu".to_string());
                        } else if anno == "button" || anno.starts_with("button:") {
                            display_ty = Some("button".to_string());
                        } else {
                            display_ty = Some(anno.clone());
                        }
                    } else if display_ty.is_none() {
                        if let serde_json::Value::String(_) = val {
                            display_ty = Some("string".to_string());
                        }
                    }

                    if let Some(ty) = display_ty {
                        let ty_text = format!("({})", ty);
                        labels.push((TextLabel {
                            text: ty_text,
                            x: list_left + 190.0,
                            y: row_y + 6.0,
                            font_size: tree_font_size,
                            color: f32_to_rgb(crate::color::tree_type_text_color()),
                        }, Some(list_left + 233.0)));
                    }

                    if Some(*original_idx) != self.selected_key_idx {
                        let mut is_button = false;
                        if let Some(Some(ref anno)) = self.annotations.get(*original_idx) {
                            if anno == "button" || anno.starts_with("button:") {
                                is_button = true;
                            }
                        }

                        let is_color = if let serde_json::Value::String(s) = val {
                            s.starts_with('#')
                        } else {
                            false
                        };
                        
                        let label_x = if is_color {
                            list_left + 267.0
                        } else {
                            list_left + 245.0
                        };

                        if is_button {
                            labels.push((TextLabel {
                                text: display_val,
                                x: list_left + 245.0 + 8.0,
                                y: row_y + 6.0,
                                font_size: tree_font_size,
                                color: [240, 240, 245],
                            }, None));
                        } else {
                            labels.push((TextLabel {
                                text: display_val,
                                x: label_x,
                                y: row_y + 6.0,
                                font_size: tree_font_size,
                                color: f32_to_rgb(crate::color::tree_value_text_color()),
                            }, None));
                        }
                    }
                }
            }
        }
        labels
    }

}

impl TreeList {

    /// The field children, in the legacy children() order: those held here (`held`) or
    /// those in `ui` (not `held`).
    pub(super) fn paint_fields(&self, ui: &UiContext, pc: &mut PaintCtx, held: bool) {
        macro_rules! field {
            ($f:expr) => {
                if $f.is_attached() != held {
                    $f.get(ui).paint_self(ui, pc);
                }
            };
        }
        field!(self.search_box);
        field!(self.add_key_btn);
        if self.add_key_popover_open {
            field!(self.add_key_popover_box);
        }
        if self.editing_key_idx.is_some() {
            field!(self.edit_box);
        }
    }
    pub(super) fn tree_radius(&self) -> f32 {
        crate::layout::tree_corner_radius()
    }

    pub(super) fn tree_corners(&self) -> (bool, bool, bool, bool) {
        (true, true, true, true)
    }

    pub(super) fn tree_border(&self) -> Option<([f32; 4], f32)> {
        if self.scroll_box.show_border {
            Some((crate::color::tree_border_color(), 1.0))
        } else {
            None
        }
    }
}
