//! `MenuBar`: a titled bar of dropdown menus with an optional context selector on the title. The
//! menu buttons are a [`ButtonStrip`] the bar owns by value and drives directly, laid out by
//! [`Layout::arrange_children`]; events reach the context through `EventCtx::ui`. Dropdowns are
//! painted through the popover hooks ([`Paint::popover`] / [`Paint::draw_popover`]) and layered
//! by [`Layout::z_order`]. The title supports the designer's curved circular-pane mode. Focus is
//! conditional: the bar reports itself focused only while it holds the focus, a dropdown is open
//! or a menu is selected ([`Input::is_focused`]).
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the struct, its geometry (title, popovers, the strip's layout), construction and builders, `impl Layout` |
//! | `paint` | `impl Paint`: the bar, the title, the strip and the open dropdowns |
//! | `input` | `impl Input`: hits, presses, keys and the pointer over the bar and its dropdowns |
//! | `controller` | `MenuController` and `PageSelector`, what a host drives the bar through |

mod controller;
mod input;
mod paint;
#[cfg(test)]
mod tests;

use crate::color;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::display::TextLabel;
use crate::widget::{Adapted, ButtonStrip, WidgetHost, ElementState, Event, EventCtx, Input, Key, Layout, MenuController, MouseButton, NamedKey, PageSelector, Paint, DROPDOWN_ITEM_H, WidgetHostExt};

pub struct MenuBar {
    pub visible: bool,
    pub network_opacity: f32,
    pub curved_circle: Option<(f32, f32, f32)>,
    pub blur: bool,
    pub color: Option<[f32; 4]>,
    /// Draw as a recess carved into the window root plate instead of as an opaque bar:
    /// no background fill of its own, just shaded edges, so the plate shows through.
    /// `color` is ignored while this is set — see [`Adapted::<MenuBar>::with_recess`].
    pub recessed: Option<bool>,
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
    /// The style in force: the per-widget override (`with_recess`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn recessed(&self) -> bool {
        self.recessed.unwrap_or_else(crate::layout::control_relief)
    }

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Adapted<MenuBar> {
        let mut bar = Adapted::new(MenuBar {
            visible: true,
            network_opacity: 1.0,
            curved_circle: None,
            blur: false,
            color: None,
            recessed: None,
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
        crate::color::menubar_tab_label_color()
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
        self.color.unwrap_or_else(color::sidebar_bg_color)
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

    /// Drop the bar's own background and carve it into the window root plate instead, so
    /// the plate reads as recessed under the menu — a relief cut into the surface rather
    /// than a slab sitting on it. Shading follows the DE-wide `light_source_position` /
    /// `bevel_depth` config, inverted so the light-facing edges are the shadowed ones.
    pub fn with_recess(mut self, recessed: bool) -> Self {
        self.recessed = Some(recessed);
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

    fn arrange_children(&mut self, rect: Rect) {
        self.layout_strip(rect);
    }
}

unsafe impl Send for MenuBar {}

unsafe impl Sync for MenuBar {}
