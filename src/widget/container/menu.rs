//! Narrow-trait `MenuBar` (Phase 5o) — a titled bar of dropdown menus with an optional
//! context selector on the title. The menu buttons live in an EMBEDDED legacy [`ButtonStrip`]
//! (owned by value in the model, driven through `WidgetHost` calls — events reach it via
//! `EventCtx::ui`, and [`Layout::arrange_children`] parents it back to the adapter so its
//! parent-chain styling walks keep working). Dropdowns are painted through the popover hooks
//! ([`Paint::popover`] / [`Paint::draw_popover`]) and layered by [`Layout::z_order`]. The
//! title supports the designer's curved circular-pane mode. Focus is conditional, exactly like
//! legacy: the bar claims the global focus only while a dropdown is open or a menu selected
//! ([`Input::is_focused`] reports that state, ignoring the base flag).
//!
//! The standalone `Menu` widget that used to live here was DELETED in this migration: it had
//! zero constructors workspace-wide (dead code).
//!
//! Dropped with the migration: the `title_buf`/`curved_title_char_bufs`/`context_item_bufs`
//! glyphon caches — `get_text_items` always returned empty, so `prepare_text` built buffers
//! nothing ever read (an abandoned optimization). Also gone: the legacy `rect()` override's
//! vertical-mode dynamic height (`with_vertical` has no callers workspace-wide; the vertical
//! label/title geometry is kept for the strip's rotated mode, but the widget rect is the
//! assigned base rect).

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::display::TextLabel;
use crate::widget::{
    Adapted, ButtonStrip, WidgetHost, ElementState, Event, EventCtx, Input, Key, Layout,
    MenuController, MouseButton, NamedKey, PageSelector, Paint, DROPDOWN_ITEM_H,
};

pub struct MenuBar {
    pub visible: bool,
    pub network_opacity: f32,
    pub curved_circle: Option<(f32, f32, f32)>,
    pub blur: bool,
    pub color: Option<[f32; 4]>,
    /// Draw as a recess carved into the window backplate instead of as an opaque bar:
    /// no background fill of its own, just shaded edges, so the plate shows through.
    /// `color` is ignored while this is set — see [`Adapted::<MenuBar>::with_recess`].
    pub recessed: bool,
    pub title: String,
    pub menus: Adapted<ButtonStrip>,
    pub menu_items: Vec<String>,
    pub vertical_items: Vec<String>,
    pub menu_dropdowns: Vec<Vec<String>>,
    pub menu_dropdown_checked: Vec<Vec<Option<bool>>>,
    pub vertical: bool,
    pub focused: bool,
    pub z_level: i32,
    pub center_items: bool,
    pub title_pos: Option<(f32, f32)>,
    pub label: Option<String>,
    pub context_options: Vec<String>,
    pub context_selected: usize,
    pub context_dropdown_open: bool,
    pub context_just_changed: bool,
    pub context_hovered_item: Option<usize>,
    pub context_title_hovered: bool,
    pub right_align_title: bool,
    pub layout_dirty: bool,
    pub on_context_change_cb: Option<Box<dyn Fn(usize) + Send + Sync>>,
    pub on_menu_click_cb: Option<Box<dyn Fn(usize, usize) + Send + Sync>>,
    pub hovered_dropdown_item: Option<usize>,
    pub clicked_dropdown_item: Option<(usize, usize)>,
    last_arranged: Option<Rect>,
}

impl MenuBar {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Adapted<MenuBar> {
        let mut bar = Adapted::new(MenuBar {
            visible: true,
            network_opacity: 1.0,
            curved_circle: None,
            blur: false,
            color: None,
            recessed: false,
            title: String::new(),
            menus: Adapted::new(ButtonStrip::new(x, y, w, h).with_inherit_menubar_font(true)),
            menu_items: Vec::new(),
            vertical_items: Vec::new(),
            menu_dropdowns: Vec::new(),
            menu_dropdown_checked: Vec::new(),
            vertical: false,
            focused: false,
            z_level: 0,
            center_items: false,
            title_pos: None,
            label: None,
            context_options: Vec::new(),
            context_selected: 0,
            context_dropdown_open: false,
            context_just_changed: false,
            context_hovered_item: None,
            context_title_hovered: false,
            right_align_title: false,
            layout_dirty: true,
            on_context_change_cb: None,
            on_menu_click_cb: None,
            hovered_dropdown_item: None,
            clicked_dropdown_item: None,
            last_arranged: None,
        });
        WidgetHost::set_rect(&mut bar, x, y, w, h);
        bar
    }

    pub fn set_curved_circle(&mut self, circle: Option<(f32, f32, f32)>) {
        self.curved_circle = circle;
    }

    pub fn set_network_opacity(&mut self, opacity: f32) {
        self.network_opacity = opacity;
    }

    pub fn set_context_selected(&mut self, selected: usize) {
        self.context_selected = selected;
    }

    pub fn take_context_change(&mut self) -> Option<usize> {
        if self.context_just_changed {
            self.context_just_changed = false;
            Some(self.context_selected)
        } else {
            None
        }
    }

    fn display_title(&self) -> String {
        let mut display_title = self.title.clone();
        if !self.context_options.is_empty() {
            display_title.push_str(if self.vertical { "▼" } else { " ▼" });
        }
        display_title
    }

    pub fn title_rect(&self, rect: Rect) -> (f32, f32, f32, f32) {
        if self.title.is_empty() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);
        let padding_x = crate::layout::paginator_tab_padding_x();

        let mut display_title = self.title.clone();
        if !self.context_options.is_empty() {
            display_title.push_str(" ▼");
        }

        if self.curved_circle.is_some() {
            if let Some((tx, ty)) = self.title_pos {
                let title_w = display_title.len() as f32 * char_w + 24.0;
                (tx, ty, title_w, rect.height)
            } else {
                (rect.x, rect.y, display_title.len() as f32 * char_w + 24.0, rect.height)
            }
        } else if self.vertical {
            let mut cy = 16.0;
            if let Some(ref label) = self.label {
                let line_height = font_size * 1.2;
                let label_h = label.chars().count() as f32 * line_height;
                cy += label_h + 20.0;
            }
            let line_height = font_size * 1.2;
            let title_h = self.display_title().chars().count() as f32 * line_height;
            (rect.x, rect.y + cy, rect.width, title_h)
        } else {
            let mut start_x = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                total_width += display_title.len() as f32 * char_w + 24.0;
                for btn_label in &self.menus.buttons {
                    total_width += btn_label.len() as f32 * char_w + 2.0 * padding_x;
                }
                if rect.width > total_width {
                    start_x = (rect.width - total_width) / 2.0;
                }
            }
            let title_w = display_title.len() as f32 * char_w + 24.0;
            let tx = if self.right_align_title {
                rect.x + rect.width - title_w - 20.0
            } else {
                rect.x + start_x
            };
            (tx, rect.y, title_w, rect.height)
        }
    }

    pub fn context_popover_rect(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        if self.context_options.is_empty() || !self.context_dropdown_open {
            return None;
        }
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);

        let max_len = self.context_options.iter().map(|s| s.len()).max().unwrap_or(0);
        let dw = (max_len as f32 * char_w + 40.0).max(140.0);
        let dh = self.context_options.len() as f32 * DROPDOWN_ITEM_H;

        let tr = self.title_rect(rect);
        let dx = if self.vertical { tr.0 + tr.2 } else { tr.0 };
        let dy = if self.vertical { tr.1 } else { tr.1 + tr.3 };
        Some((dx, dy, dw, dh))
    }

    pub fn menu_dropdown_rect(&self) -> Option<(f32, f32, f32, f32)> {
        let menu_idx = self.menus.selected?;
        let items = self.menu_dropdowns.get(menu_idx)?;
        if items.is_empty() {
            return None;
        }
        let hr = self.menus.item_rect(menu_idx);
        let dh = items.len() as f32 * DROPDOWN_ITEM_H;
        let max_len = items.iter().map(|s| s.len()).max().unwrap_or(0);
        let font_setting = crate::layout::menubar_font();
        let (_, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let char_w = 7.5 * (font_size / 12.0);
        let dw = (max_len as f32 * char_w + 40.0).max(120.0);

        let dx = if self.vertical { hr.0 + hr.2 } else { hr.0 };
        let dy = if self.vertical { hr.1 } else { hr.1 + hr.3 };
        Some((dx, dy, dw, dh))
    }

    pub fn text_color(&self) -> [f32; 4] {
        crate::colors::menubar_tab_label_color()
    }

    pub fn is_blur_enabled(&self) -> bool {
        self.blur
    }

    fn update_menu_labels(&mut self) {
        let src = if self.vertical { &self.vertical_items } else { &self.menu_items };
        self.menus.buttons = src.clone();
        self.menus.generate_rotated_labels();
    }

    fn bg_color(&self) -> [f32; 4] {
        self.color.unwrap_or_else(|| colors::sidebar_bg_color())
    }

    /// Position the embedded strip inside `rect` — the legacy `set_rect` body, minus the
    /// parent clamping (that lives in [`Layout::adjust_rect`]) and the base assignment (the
    /// adapter's). Early-outs when the rect and content are unchanged, like legacy.
    fn layout_strip(&mut self, rect: Rect) {
        if self.last_arranged == Some(rect) && !self.layout_dirty {
            return;
        }
        self.layout_dirty = false;
        self.last_arranged = Some(rect);
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);

        let font_setting = crate::layout::menubar_font();
        let font_info = crate::layout::parse_font_string(&font_setting);
        let font_fam = font_info.0;
        let font_size = font_info.1.unwrap_or(12.0);

        if self.vertical {
            let mut cy = 16.0;
            if let Some(ref label) = self.label {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let label_h = label.chars().count() as f32 * line_height;
                cy += label_h + 20.0;
            }
            if !self.title.is_empty() {
                let font_size = 12.0;
                let line_height = font_size * 1.2;
                let title_h = self.display_title().chars().count() as f32 * line_height;
                cy += title_h + 36.0;
            }
            let menus_y = (y + cy).clamp(y, y + h);
            let menus_h = (h - cy).min(y + h - menus_y).max(0.0);
            self.menus.vertical = true;
            self.menus.set_rect(x, menus_y, w, menus_h);
        } else {
            let padding = crate::layout::button_padding();
            let spacing = crate::layout::button_strip_spacing();
            let mut cx = 8.0;
            if self.center_items {
                let mut total_width = 8.0;
                if !self.title.is_empty() && !self.right_align_title {
                    let mut display_title = self.title.clone();
                    if !self.context_options.is_empty() {
                        display_title.push_str(" ▼");
                    }
                    total_width += crate::widget::display::measure_text_width(&display_title, &font_fam, font_size) + 24.0;
                }
                let mut btn_strip_w = 0.0;
                for (i, btn_label) in self.menus.buttons.iter().enumerate() {
                    let text_w = crate::widget::display::measure_text_width(btn_label, &font_fam, font_size);
                    btn_strip_w += text_w + 2.0 * padding;
                    if i > 0 {
                        btn_strip_w += spacing;
                    }
                }
                total_width += btn_strip_w;
                if w > total_width {
                    cx = (w - total_width) / 2.0;
                }
            }
            if !self.title.is_empty() && !self.right_align_title {
                let mut display_title = self.title.clone();
                if !self.context_options.is_empty() {
                    display_title.push_str(" ▼");
                }
                cx += crate::widget::display::measure_text_width(&display_title, &font_fam, font_size) + 24.0;
            }
            let mut btn_strip_w = 0.0;
            for (i, btn_label) in self.menus.buttons.iter().enumerate() {
                let text_w = crate::widget::display::measure_text_width(btn_label, &font_fam, font_size);
                btn_strip_w += text_w + 2.0 * padding;
                if i > 0 {
                    btn_strip_w += spacing;
                }
            }
            let menus_x = (x + cx).clamp(x, x + w);
            let menus_w = btn_strip_w.min(x + w - menus_x);
            self.menus.vertical = false;
            self.menus.set_rect(menus_x, y, menus_w, h);
        }
    }

    /// Close every open dropdown and drop internal focus state — the state half of the legacy
    /// `unfocus` (the global-focus release is the caller's, via `EventCtx::release_focus`).
    fn close_all(&mut self) {
        self.focused = false;
        self.context_dropdown_open = false;
        self.context_hovered_item = None;
        self.hovered_dropdown_item = None;
        self.menus.inner_mut().set_selected(None);
    }

    /// The conditional focus claim of the legacy `focus()`: hold the global focus only while
    /// something is open.
    fn sync_focus(&mut self, ectx: &mut EventCtx) {
        if self.context_dropdown_open || self.menus.selected.is_some() {
            self.focused = true;
            ectx.request_focus();
        } else {
            self.focused = false;
            ectx.release_focus();
        }
    }
}

impl Adapted<MenuBar> {
    pub fn with_color(mut self, color: [f32; 4]) -> Self {
        self.color = Some(color);
        self
    }

    /// Drop the bar's own background and carve it into the window backplate instead, so
    /// the plate reads as recessed under the menu — a relief cut into the surface rather
    /// than a slab sitting on it. Shading follows the DE-wide `light_source_position` /
    /// `bevel_depth` config, inverted so the light-facing edges are the shadowed ones.
    pub fn with_recess(mut self, recessed: bool) -> Self {
        self.recessed = recessed;
        self
    }

    pub fn with_blur(mut self, blur: bool) -> Self {
        self.blur = blur;
        self
    }

    pub fn with_right_aligned_title(mut self, right: bool) -> Self {
        self.right_align_title = right;
        self
    }

    pub fn on_context_change<F: Fn(usize) + Send + Sync + 'static>(mut self, cb: F) -> Self {
        self.on_context_change_cb = Some(Box::new(cb));
        self
    }

    pub fn on_menu_click<F: Fn(usize, usize) + Send + Sync + 'static>(mut self, cb: F) -> Self {
        self.on_menu_click_cb = Some(Box::new(cb));
        self
    }

    pub fn with_context_options(mut self, options: Vec<String>, selected: usize) -> Self {
        self.context_options = options;
        self.context_selected = selected;
        self
    }

    pub fn with_center_items(mut self, center: bool) -> Self {
        self.center_items = center;
        self
    }

    pub fn with_title(mut self, title: &str) -> Self {
        self.title = title.to_string();
        self
    }

    pub fn with_item(mut self, label: &str, items: &[&str]) -> Self {
        self.menu_items.push(label.to_string());
        self.vertical_items.push(label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);
        self.menus.add_button(label);
        self
    }

    pub fn with_item_vh(mut self, horizontal_label: &str, vertical_label: &str, items: &[&str]) -> Self {
        self.menu_items.push(horizontal_label.to_string());
        self.vertical_items.push(vertical_label.to_string());
        self.menu_dropdowns.push(items.iter().map(|s| s.to_string()).collect());
        self.menu_dropdown_checked.push(vec![None; items.len()]);
        let label = if self.vertical { vertical_label } else { horizontal_label };
        self.menus.add_button(label);
        self
    }

    pub fn with_vertical(mut self, vertical: bool) -> Self {
        self.vertical = vertical;
        self.menus.vertical = vertical;
        self.update_menu_labels();
        self
    }

    pub fn with_z_index(mut self, z: i32) -> Self {
        self.z_level = z;
        self
    }
}

impl Layout for MenuBar {

    fn z_order(&self) -> i32 {
        self.z_level
    }

    fn arrange_children(&mut self, rect: Rect, _host: *mut (dyn WidgetHost + 'static)) {
        self.layout_strip(rect);
    }
}

impl Paint for MenuBar {
    fn color(&self) -> [f32; 4] {
        self.bg_color()
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        // Corners never round (the backplate-adjacency source is gone). The radius was the
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
        if self.recessed {
            // No background of our own: carve the backplate instead. The recess shading is
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
                let depth = crate::layout::bar_wall_width().min(rect.height * 0.6);
                ctx.recess_edges(rect, (0.0, 0.0, 0.0, 0.0), depth, (false, false, true, false));
            } else {
                // Inset from the plate edge: a real trough, walled all round, its corners
                // rounded by the roll itself.
                let depth = crate::layout::bar_wall_width().min(rect.height * 0.4);
                ctx.recess(rect, (depth, depth, depth, depth), depth);
            }
        } else {
            // Background: always the plain quad — the rounded-against-parent variant required a
            // backplate parent, which no longer exists.
            ctx.quad(rect, self.bg_color());
        }

        // Title highlight while the context dropdown is open / hovered.
        if !self.context_options.is_empty() {
            let tr = self.title_rect(rect);
            if self.context_dropdown_open {
                ctx.quad(Rect { x: tr.0, y: tr.1, width: tr.2, height: tr.3 }, colors::highlight_primary_color());
            } else if self.context_title_hovered {
                ctx.quad(Rect { x: tr.0, y: tr.1, width: tr.2, height: tr.3 }, colors::HIGHLIGHT_SECONDARY);
            }
        }

        // The embedded strip's geometry (it is not a tree child; its pixels are ours).
        for (qx, qy, qw, qh, qc) in self.menus.extra_quads() {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
        for (cx, cy, r, t, start, end, c) in self.menus.extra_arcs() {
            ctx.arc(cx, cy, r, t, start, end, c);
        }
        for (cx, cy, r, c) in self.menus.extra_circles() {
            ctx.circle(cx, cy, r, c);
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
                    ctx.text(char_str, x_pos, y_pos, font_size, [0x83, 0x83, 0x8a]);
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
                    ctx.text(l.text, l.x, l.y, l.font_size, l.color);
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
                    ctx.text(char_str, x_pos, y_pos, font_size, text_color);
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
            ctx.text(display_title, x_pos, text_y, font_size, text_color);
        }

        for l in self.menus.own_labels() {
            ctx.text(l.text, l.x, l.y, l.font_size, l.color);
        }
    }

    fn popover(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        self.context_popover_rect(rect).or_else(|| self.menu_dropdown_rect())
    }

    fn draw_popover(&self, rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        let label_color = self.text_color();
        let srgb = crate::colors::to_srgb(label_color);
        let color_f32 = [srgb[0], srgb[1], srgb[2], 1.0];
        let font = Paint::widget_font(self);

        if self.context_dropdown_open {
            if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
                let theme = colors::active_theme();
                pc.rect(theme.surface_border, dx, dy, dw, dh);
                pc.rect(theme.surface_bg, dx + 1.0, dy + 1.0, dw - 2.0, dh - 2.0);
                if let Some(di) = self.context_hovered_item {
                    let iy = dy + di as f32 * DROPDOWN_ITEM_H;
                    pc.rect(colors::PANEL_MENU_HOVER, dx + 2.0, iy + 2.0, dw - 4.0, DROPDOWN_ITEM_H - 4.0);
                }

                let bounds = Some([dx, dy, dx + dw, dy + dh]);
                for (i, option) in self.context_options.iter().enumerate() {
                    let is_selected = self.context_selected == i;
                    let prefix = if is_selected { "✓ " } else { "  " };
                    let text = format!("{}{}", prefix, option);
                    let iy = crate::layout::align_text_y(dy + i as f32 * DROPDOWN_ITEM_H, DROPDOWN_ITEM_H, 12.0, 0.0);
                    if let Some(ref f) = font {
                        pc.text_with_font_and_bounds(&text, dx + 8.0, iy, 12.0, color_f32, f, bounds);
                    } else {
                        pc.text_with_bounds(&text, dx + 8.0, iy, 12.0, color_f32, bounds);
                    }
                }
            }
        } else if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
            let theme = colors::active_theme();
            pc.rect(theme.surface_border, dx, dy, dw, dh);
            pc.rect(theme.surface_bg, dx + 1.0, dy + 1.0, dw - 2.0, dh - 2.0);
            if let Some(di) = self.hovered_dropdown_item {
                let iy = dy + di as f32 * DROPDOWN_ITEM_H;
                pc.rect(colors::PANEL_MENU_HOVER, dx + 2.0, iy + 2.0, dw - 4.0, DROPDOWN_ITEM_H - 4.0);
            }

            let bounds = Some([dx, dy, dx + dw, dy + dh]);
            if let Some(menu_idx) = self.menus.selected {
                if let Some(items) = self.menu_dropdowns.get(menu_idx) {
                    for (i, option) in items.iter().enumerate() {
                        let checked = self.menu_dropdown_checked.get(menu_idx)
                            .and_then(|menu| menu.get(i))
                            .and_then(|&v| v);
                        let prefix = match checked {
                            Some(true) => "✓ ",
                            Some(false) => "  ",
                            None => "",
                        };
                        let text = format!("{}{}", prefix, option);
                        let iy = crate::layout::align_text_y(dy + i as f32 * DROPDOWN_ITEM_H, DROPDOWN_ITEM_H, 12.0, 0.0);
                        if let Some(ref f) = font {
                            pc.text_with_font_and_bounds(&text, dx + 8.0, iy, 12.0, color_f32, f, bounds);
                        } else {
                            pc.text_with_bounds(&text, dx + 8.0, iy, 12.0, color_f32, bounds);
                        }
                    }
                }
            }
        }
    }
}

impl Input for MenuBar {
    fn blocks_backplate_drag(&self) -> bool {
        false
    }

    /// Legacy `mouse_input` saw every press: any press closes an open context dropdown, even
    /// outside the bar.
    fn gates_presses(&self) -> bool {
        false
    }

    fn hit(&self, rect: Rect, px: f32, py: f32) -> bool {
        if !self.visible {
            return false;
        }
        if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
            if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                return true;
            }
        }
        if px >= rect.x && px <= rect.x + rect.width && py >= rect.y && py <= rect.y + rect.height {
            return true;
        }
        // The strip may extend past the assigned rect (clamped layouts).
        let (sx, sy, sw, sh) = self.menus.rect();
        px >= sx && px <= sx + sw && py >= sy && py <= sy + sh
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.menus.set_modifiers(ctrl, shift, alt);
    }

    fn visibility_changed(&mut self, visible: bool) {
        self.visible = visible;
        self.menus.set_visible(visible);
        self.layout_dirty = true;
    }

    fn is_focused(&self, _base_focused: bool) -> bool {
        self.focused || self.context_dropdown_open || self.menus.selected.is_some()
    }

    fn set_selected(&mut self, selected: bool) {
        self.focused = selected;
        if !selected {
            self.menus.inner_mut().set_selected(None);
        }
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::PointerMove { x: px, y: py, .. } => {
                if !self.visible {
                    return false;
                }
                let (px, py) = (*px, *py);
                let rect = ectx.rect;
                self.layout_strip(rect);

                let mut changed = false;

                let old_title_hovered = self.context_title_hovered;
                self.context_title_hovered = false;
                if !self.context_options.is_empty() {
                    let tr = self.title_rect(rect);
                    if px >= tr.0 && px <= tr.0 + tr.2 && py >= tr.1 && py <= tr.1 + tr.3 {
                        self.context_title_hovered = true;
                    }
                }
                if old_title_hovered != self.context_title_hovered {
                    changed = true;
                }

                let old_hovered_item = self.context_hovered_item;
                self.context_hovered_item = None;
                if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                        if di < self.context_options.len() {
                            self.context_hovered_item = Some(di);
                        }
                    }
                }
                if old_hovered_item != self.context_hovered_item {
                    changed = true;
                }

                let old_hovered_dropdown = self.hovered_dropdown_item;
                self.hovered_dropdown_item = None;
                if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                        if let Some(menu_idx) = self.menus.selected {
                            if let Some(items) = self.menu_dropdowns.get(menu_idx) {
                                if di < items.len() {
                                    self.hovered_dropdown_item = Some(di);
                                }
                            }
                        }
                    }
                }
                if old_hovered_dropdown != self.hovered_dropdown_item {
                    changed = true;
                }

                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if self.menus.cursor_moved(px, py, ui) {
                        changed = true;
                    }
                }
                changed
            }
            Event::MouseButton { button, state, x: px, y: py, .. } => {
                if !self.visible {
                    return false;
                }
                if *button != MouseButton::Left {
                    return false;
                }
                let (px, py, state) = (*px, *py, *state);
                let rect = ectx.rect;
                self.layout_strip(rect);

                let mut changed = false;

                if let Some((dx, dy, dw, dh)) = self.menu_dropdown_rect() {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        if state == ElementState::Pressed {
                            let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                            if let Some(menu_idx) = self.menus.selected {
                                if let Some(items) = self.menu_dropdowns.get(menu_idx) {
                                    if di < items.len() {
                                        self.clicked_dropdown_item = Some((menu_idx, di));
                                        self.close_all();
                                        ectx.release_focus();
                                        return true;
                                    }
                                }
                            }
                        }
                        return true;
                    }
                }

                if let Some((dx, dy, dw, dh)) = self.context_popover_rect(rect) {
                    if px >= dx && px < dx + dw && py >= dy && py < dy + dh {
                        if state == ElementState::Pressed {
                            let di = ((py - dy) / DROPDOWN_ITEM_H) as usize;
                            if di < self.context_options.len() {
                                self.context_selected = di;
                                self.context_just_changed = true;
                                self.close_all();
                                ectx.release_focus();
                                if let Some(ref cb) = self.on_context_change_cb {
                                    cb(di);
                                }
                                return true;
                            }
                        }
                    }
                }

                if !self.context_options.is_empty() {
                    let tr = self.title_rect(rect);
                    if px >= tr.0 && px <= tr.0 + tr.2 && py >= tr.1 && py <= tr.1 + tr.3 {
                        if state == ElementState::Pressed {
                            if self.context_dropdown_open {
                                self.close_all();
                                ectx.release_focus();
                            } else {
                                self.menus.unfocus();
                                self.context_dropdown_open = true;
                                self.sync_focus(ectx);
                            }
                        }
                        return true;
                    }
                }

                if self.context_dropdown_open && state == ElementState::Pressed {
                    self.close_all();
                    ectx.release_focus();
                    changed = true;
                }

                let old_menu_selected = self.menus.selected;
                if let Some(ui) = ectx.ui.as_deref_mut() {
                    if self.menus.mouse_input(MouseButton::Left, state, px, py, ui) {
                        changed = true;
                        if self.menus.selected.is_some() && old_menu_selected != self.menus.selected {
                            self.sync_focus(ectx);
                        }
                    }
                }
                changed
            }
            Event::KeyInput(key_event) => {
                if key_event.state != ElementState::Pressed {
                    return false;
                }
                if self.context_dropdown_open {
                    match key_event.logical_key {
                        Key::Named(NamedKey::ArrowDown) => {
                            let current = self.context_hovered_item.unwrap_or(self.context_selected);
                            if current + 1 < self.context_options.len() {
                                self.context_hovered_item = Some(current + 1);
                            } else {
                                self.context_hovered_item = Some(0);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::ArrowUp) => {
                            let current = self.context_hovered_item.unwrap_or(self.context_selected);
                            if current > 0 {
                                self.context_hovered_item = Some(current - 1);
                            } else {
                                self.context_hovered_item = Some(self.context_options.len() - 1);
                            }
                            return true;
                        }
                        Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                            if let Some(idx) = self.context_hovered_item {
                                self.context_selected = idx;
                                self.context_just_changed = true;
                            }
                            self.close_all();
                            ectx.release_focus();
                            return true;
                        }
                        Key::Named(NamedKey::Escape) => {
                            self.close_all();
                            ectx.release_focus();
                            return true;
                        }
                        _ => {}
                    }
                }
                match ectx.ui.as_deref_mut() {
                    Some(ui) => self.menus.keyboard_input(key_event, ui),
                    None => false,
                }
            }
            // Hosts call `focus()`/`unfocus()` directly; legacy semantics: focus is claimed
            // conditionally (only while something is open), unfocus closes everything.
            Event::FocusIn => {
                self.sync_focus(ectx);
                true
            }
            Event::FocusOut => {
                self.close_all();
                ectx.release_focus();
                true
            }
            _ => false,
        }
    }

}

impl MenuController for MenuBar {
    fn menu_click(&mut self) -> Option<(usize, usize)> {
        let _ = self.menus.take_click();

        if let Some((menu_idx, item_idx)) = self.clicked_dropdown_item.take() {
            if let Some(ref cb) = self.on_menu_click_cb {
                cb(menu_idx, item_idx);
            }
            return Some((menu_idx, item_idx));
        }
        None
    }

    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize) {
        self.clicked_dropdown_item = Some((menu_idx, item_idx));
    }

    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        if let Some(menu) = self.menu_dropdown_checked.get_mut(menu_idx) {
            if item_idx < menu.len() {
                menu[item_idx] = Some(checked);
            }
        }
    }

    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        if menu_idx < self.menu_dropdowns.len() {
            self.menu_dropdowns[menu_idx] = items.to_vec();
            self.menu_dropdown_checked[menu_idx] = vec![Some(false); items.len()];
            self.layout_dirty = true;
        }
    }

    fn is_menu_bar(&self) -> bool {
        self.visible
    }

    fn is_menu_open(&self) -> bool {
        self.context_dropdown_open || self.menus.selected.is_some()
    }

    fn menu_items(&self) -> Vec<String> {
        self.menu_items.clone()
    }

    fn menu_item_checked(&self) -> Vec<Option<bool>> {
        self.menu_dropdown_checked.iter().flatten().copied().collect()
    }

    fn is_vertical(&self) -> bool {
        self.vertical
    }

    fn menu_names(&self) -> Vec<String> {
        self.menus.buttons.clone()
    }

    fn menu_items_list(&self) -> Vec<Vec<String>> {
        self.menu_dropdowns.clone()
    }

    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        self.menu_dropdown_checked.clone()
    }

    fn take_context_change(&mut self) -> Option<usize> {
        self.take_context_change()
    }

    fn set_context_selected(&mut self, selected: usize) {
        self.set_context_selected(selected);
    }

    fn set_center_items(&mut self, center: bool) {
        self.center_items = center;
    }

    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        if !self.visible {
            return None;
        }
        for i in 0..self.menus.buttons.len() {
            let r = self.menus.item_rect(i);
            if px >= r.0 && px < r.0 + r.2 && py >= r.1 && py < r.1 + r.3 {
                let title = self.menus.buttons[i].clone();
                let mut formatted_items = Vec::new();
                if let Some(items) = self.menu_dropdowns.get(i) {
                    for (item_idx, item) in items.iter().enumerate() {
                        let checked = self.menu_dropdown_checked.get(i)
                            .and_then(|menu| menu.get(item_idx))
                            .and_then(|&v| v);
                        let prefix = match checked {
                            Some(true) => "✓ ",
                            Some(false) => "  ",
                            None => "",
                        };
                        formatted_items.push(format!("{}{}", prefix, item));
                    }
                }
                return Some((i, title, formatted_items, r.0, r.1, r.2, r.3));
            }
        }
        None
    }
}

impl PageSelector for MenuBar {
    fn selected_page(&self) -> usize {
        self.menus.selected.unwrap_or(0)
    }

    fn set_selected_page(&mut self, page: usize) {
        self.menus.inner_mut().set_selected(Some(page));
    }

    fn sidebar_w(&self) -> f32 {
        let padding_x = crate::layout::paginator_tab_padding_x();
        let margin_x = 5.0;
        if self.vertical {
            (12.0 + 2.0 * padding_x).max(24.0) + 2.0 * margin_x
        } else {
            let items = &self.menu_items;
            let max_req_w = items.iter()
                .map(|p| p.len() as f32 * 7.5 + 2.0 * padding_x)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            max_req_w.max(24.0) + 2.0 * margin_x
        }
    }
}

unsafe impl Send for MenuBar {}
unsafe impl Sync for MenuBar {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::UiContext;

    fn bar() -> Adapted<MenuBar> {
        MenuBar::new(0.0, 0.0, 400.0, 24.0)
            .with_title("Test")
            .with_item("File", &["New", "Save"])
            .with_item("Edit", &["Undo"])
    }

    #[test]
    fn menu_open_click_and_controller_roundtrip() {
        let mut ctx = UiContext::new();
        let mut mb = bar();
        let (id, ptr) = (mb.id(), mb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut mb, 0.0, 0.0, 400.0, 24.0);

        // Click the "File" strip button (the strip commits selection on release): the dropdown
        // opens, the bar reports focused (conditional focus), and a popover rect exists.
        let (bx, by, bw, bh) = mb.menus.item_rect(0);
        assert!(bw > 0.0, "strip laid out");
        assert!(mb.mouse_input(MouseButton::Left, ElementState::Pressed, bx + bw / 2.0, by + bh / 2.0, &mut ctx));
        assert!(mb.mouse_input(MouseButton::Left, ElementState::Released, bx + bw / 2.0, by + bh / 2.0, &mut ctx));
        assert!(MenuController::is_menu_open(&*mb), "dropdown open");
        assert!(WidgetHost::focused(&mb, &ctx), "bar holds focus while open");
        let (dx, dy, _, _) = WidgetHost::popover_rect(&mb).expect("dropdown popover");

        // Click the second item ("Save"): menu_click reports (0, 1) and everything closes.
        assert!(mb.mouse_input(MouseButton::Left, ElementState::Pressed, dx + 10.0, dy + DROPDOWN_ITEM_H * 1.5, &mut ctx));
        assert_eq!(MenuController::menu_click(&mut *mb), Some((0, 1)));
        assert!(!MenuController::is_menu_open(&*mb));
        assert!(!WidgetHost::focused(&mb, &ctx), "focus released after the click");

        // The PageSelector capability is reached through the concrete adapter too.
        assert!(PageSelector::sidebar_w(&*mb) > 0.0);
    }

    #[test]
    fn hidden_menubar_reports_no_menu_and_rejects_hits() {
        let mut ctx = UiContext::new();
        let mut mb = bar();
        let (id, ptr) = (mb.id(), mb.as_ptr_mut());
        ctx.register_widget(id, ptr);
        WidgetHost::set_rect(&mut mb, 0.0, 0.0, 400.0, 24.0);

        WidgetHost::set_visible(&mut mb, false);
        assert!(!MenuController::is_menu_bar(&*mb), "hidden bar is not a menu bar");
        assert!(!WidgetHost::hit_test(&mb, 10.0, 10.0, &ctx));
        assert!(MenuController::get_menu_items_at(&*mb, 10.0, 10.0).is_none());
    }
}
