//! Narrow-trait `Dropdown` (Phase 5p — first popover widget through `Paint::popover` /
//! `draw_popover`, the 5o surface). Detached-label control on the Slider convention (no rect
//! inflation; the label eats into the assigned rect), `Control::control_label`'s +4px inset via
//! `Layout::detached_label_inset`, side-label inset computed from the synced label.
//!
//! Parity notes (all legacy-faithful, verified against the pre-migration impl):
//! - `parent` stays a public, direct-write-only field: legacy `set_parent` never wrote it (the
//!   Element default only touched the tree — Ramp's dummy-ctx `set_parent` calls were silently
//!   discarded), so the Ramp popover clamp and the fade-blend parent color activate only for
//!   callers that assign the field, exactly as before. The backplate-concentric corner walk
//!   (which legacy ran over the ctx tree) instead starts from a separate pointer captured by
//!   `Layout::parent_changed` and hops legacy field-based `parent(&dummy)` impls — exact for
//!   the real consumer (cce-graph: Dropdown → Plate → Backplate, both field-based).
//! - The row-rect hit expansion (`base.row_x/row_w`) is dropped, consistent with every other
//!   migrated control: `Input::hit` tests the widget rect plus the open popover.
//! - `Layout::intrinsic_measure_width` (new hook) preserves the `auto_width` measure behavior
//!   (cce-system-settings sizes its page dropdown from `Element::measure`).

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::model::{Adapted, EventCtx, Input, Layout, Paint};
use crate::widget::{
    Control, Element, ElementState, Event, Key, MouseButton, NamedKey, UiContext,
};

/// Side-layout label inset — the legacy `Element::label_x_offset` default for non-exempt
/// widgets (Dropdown was never in the exempt list).
fn side_offset(label: &Option<String>) -> f32 {
    if crate::layout::control_label_layout() == "side" && label.is_some() {
        90.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone)]
pub struct Dropdown {
    pub options: Vec<String>,
    pub selected: usize,
    pub open: bool,
    pub(crate) hovered_item: Option<usize>,
    just_changed: bool,
    /// Legacy-faithful parent pointer: written ONLY by direct assignment (the Ramp-clamp unit
    /// test; no production writer). Read by the Ramp popover clamp and the fade-blend parent
    /// color, like legacy. NOT the corner-walk pointer — see `tracked_parent`.
    pub parent: Option<*mut (dyn Element + 'static)>,
    /// Captured by [`Layout::parent_changed`] whenever a container `set_parent`s this widget —
    /// the model-side stand-in for the ctx-tree head the legacy backplate corner walk started
    /// from (`paint` has no ctx to reach the real tree).
    tracked_parent: Option<*mut (dyn Element + 'static)>,
    pub font_family: String,
    pub custom_display_text: Option<String>,
    pub open_upward: Option<bool>,
    pub auto_width: bool,
    /// Synced copy of the control label ([`Paint::sync_label`]) — drives the side/detached
    /// offsets the base label geometry imposes on the widget's own geometry.
    label: Option<String>,
    /// Own hover flag, maintained from `MouseEnter`/`MouseLeave` (the adapter's hover
    /// bookkeeping hit-tests through [`Input::hit`], which includes the open popover — matching
    /// the legacy `on_cursor_moved` + popover-aware `hit_test` pair).
    hovered: bool,
    /// App-owned concentric frame (Phase 6s): `(rect, radius, corners)` of the rounded plate
    /// the dropdown sits in. When set, the corner adjustment uses it INSTEAD of walking for a
    /// `Backplate` ancestor — the hook that keeps the adjustment after an app dissolves its
    /// root Backplate (the walk finds nothing once the widget is parentless).
    corner_frame: Option<((f32, f32, f32, f32), f32, (bool, bool, bool, bool))>,
}

impl Dropdown {
    pub fn new(options: Vec<String>, selected: usize) -> Adapted<Dropdown> {
        Adapted::new(Dropdown {
            options,
            selected,
            open: false,
            hovered_item: None,
            just_changed: false,
            parent: None,
            tracked_parent: None,
            font_family: "sans-serif".to_string(),
            custom_display_text: None,
            open_upward: None,
            auto_width: false,
            label: None,
            hovered: false,
            corner_frame: None,
        })
    }

    /// Set (or clear) the app-owned concentric frame — see the `corner_frame` field docs.
    pub fn set_corner_frame(&mut self, frame: Option<((f32, f32, f32, f32), f32, (bool, bool, bool, bool))>) {
        self.corner_frame = frame;
    }

    pub fn take_change(&mut self) -> bool {
        let changed = self.just_changed;
        self.just_changed = false;
        changed
    }

    pub fn content_width(&self) -> f32 {
        let font_setting = crate::layout::control_label_font_detached();
        let (font_family, font_size_opt) = crate::layout::parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(12.0);
        let mut max_w = 0.0f32;
        for opt in &self.options {
            let opt_w = crate::widget::display::measure_text_width(opt, &font_family, font_size) + 32.0;
            if opt_w > max_w {
                max_w = opt_w;
            }
        }
        max_w
    }

    /// The detached-label strip height above the content rect — a replica of
    /// `Widget::label_offset` over the synced label (zero in side layout or unlabeled).
    fn label_top(&self) -> f32 {
        if crate::layout::control_label_layout() == "side" {
            return 0.0;
        }
        if self.label.is_some() {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            font_size + crate::layout::control_label_margin()
        } else {
            0.0
        }
    }

    fn popover_width(&self, content: Rect) -> f32 {
        content.width.max(self.content_width())
    }

    /// Popover geometry against the laid-out content rect — the legacy `get_popover_geom`,
    /// with the base-rect reads rewritten in content-rect terms (`base.y + base.h` ⇒
    /// `content.y + content.height`, `base.y + label_offset` ⇒ `content.y`).
    pub fn popover_geom(&self, content: Rect) -> (f32, f32, f32, f32) {
        let rw = self.popover_width(content);
        let rh = self.options.len() as f32 * 24.0;

        let base_y = content.y - self.label_top();
        let open_upward = self.open_upward.unwrap_or(base_y > 400.0);
        let label_x = side_offset(&self.label);

        let mut rx = content.x + label_x;
        let mut ry = if open_upward {
            content.y - rh
        } else {
            content.y + content.height
        };

        let mut is_ramp = false;
        if let Some(parent_ptr) = self.parent {
            is_ramp = unsafe { (*parent_ptr).as_any().is::<crate::widget::Ramp>() };
        }

        if is_ramp {
            if let Some(parent_ptr) = self.parent {
                let (px, py, pw_parent, ph_parent) = unsafe { (*parent_ptr).rect() };
                if pw_parent > 0.0 && ph_parent > 0.0 {
                    let dy_down = content.y + content.height;
                    let dy_up = content.y - rh;

                    if self.open_upward.is_none() {
                        if dy_down + rh > py + ph_parent && dy_up >= py {
                            ry = dy_up;
                        } else if dy_up < py && dy_down + rh <= py + ph_parent {
                            ry = dy_down;
                        }
                    }

                    // Clamp X to parent borders
                    if rx < px {
                        rx = px;
                    }
                    if rx + rw > px + pw_parent {
                        rx = px + pw_parent - rw;
                    }

                    // Clamp Y to parent borders
                    if ry < py {
                        ry = py;
                    }
                    if ry + rh > py + ph_parent {
                        ry = py + ph_parent - rh;
                    }
                }
            }
        }

        (rx, ry, rw, rh)
    }

    fn border_color(&self) -> [f32; 4] {
        if self.open {
            [0.30, 0.50, 0.32, 1.0]
        } else if self.hovered {
            let bc = colors::dropdown_border_color();
            [(bc[0] + 0.15).min(1.0), (bc[1] + 0.15).min(1.0), (bc[2] + 0.15).min(1.0), bc[3]]
        } else {
            colors::dropdown_border_color()
        }
    }

    /// The legacy backplate-ancestor lookup for concentric corners, walked from the tracked
    /// parent with a dummy ctx (field-based legacy `parent` impls answer; tree-only ones end
    /// the walk, so deep tree-linked chains lose the adjustment — flagged in the module docs).
    fn backplate_ancestor(&self) -> Option<*mut (dyn Element + 'static)> {
        let dummy = crate::context::UiContext::new();
        let mut curr = self.tracked_parent;
        while let Some(ptr) = curr {
            if unsafe { (*ptr).is_backplate() } {
                return Some(ptr);
            }
            curr = unsafe { (*ptr).parent(&dummy) };
        }
        None
    }

    /// Emit the border + background geometry — the legacy `all_rounded_quads` body (rounded,
    /// with the backplate-concentric corner adjustment) or `extra_quads` (plain) depending on
    /// the configured radius, byte-for-byte on the same content rect.
    fn paint_background(&self, content: Rect, ctx: &mut PaintCtx) {
        let label_x = side_offset(&self.label);
        let x = content.x + label_x;
        let w = content.width - label_x;
        let y = content.y;
        let visual_h = content.height;

        let mut bg_color = colors::dropdown_background_color();
        bg_color[3] = 1.0; // Force opaque background to prevent subpixel blending artifacts
        let border_color = self.border_color();

        let radius = crate::layout::dropdown_corner_radius();
        if radius <= 0.0 {
            ctx.quad(Rect { x, y, width: w, height: visual_h }, border_color);
            ctx.quad(
                Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: visual_h - 2.0 },
                bg_color,
            );
            return;
        }

        let inner_radius = (radius - 1.0).max(0.0);
        let mut adjusted = false;
        let mut outer_radii = [radius; 4];
        let mut inner_radii = [inner_radius; 4];

        let frame = self.corner_frame.or_else(|| {
            self.backplate_ancestor().map(|bp| unsafe {
                ((*bp).rect(), (*bp).corner_radius(), (*bp).rounded_corners())
            })
        });
        if let Some(((px, py, pw, ph), pr, (pr1, pr2, pr3, pr4))) = frame {
            let g_left = x - px;
            let g_top = y - py;
            let g_right = (px + pw) - (x + w);
            let g_bottom = (py + ph) - (y + visual_h);

            if pr1 && (g_left - g_top).abs() < 1.0 && g_left >= 0.0 {
                outer_radii[0] = (pr - g_left).max(0.0);
                inner_radii[0] = (outer_radii[0] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr2 && (g_right - g_top).abs() < 1.0 && g_right >= 0.0 {
                outer_radii[1] = (pr - g_right).max(0.0);
                inner_radii[1] = (outer_radii[1] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr3 && (g_right - g_bottom).abs() < 1.0 && g_right >= 0.0 {
                outer_radii[2] = (pr - g_right).max(0.0);
                inner_radii[2] = (outer_radii[2] - 1.0).max(0.0);
                adjusted = true;
            }
            if pr4 && (g_left - g_bottom).abs() < 1.0 && g_left >= 0.0 {
                outer_radii[3] = (pr - g_left).max(0.0);
                inner_radii[3] = (outer_radii[3] - 1.0).max(0.0);
                adjusted = true;
            }
        }

        if adjusted {
            for (qx, qy, qw, qh, qr, qc, corners) in crate::layout::partition_concentric_corners(
                x, y, w, visual_h, radius, outer_radii, border_color,
            ) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
            }
            for (qx, qy, qw, qh, qr, qc, corners) in crate::layout::partition_concentric_corners(
                x + 1.0, y + 1.0, w - 2.0, visual_h - 2.0, inner_radius, inner_radii, bg_color,
            ) {
                ctx.rounded_rect(Rect { x: qx, y: qy, width: qw, height: qh }, qr, corners, qc);
            }
        } else {
            let corners = (true, true, true, true);
            ctx.rounded_rect(Rect { x, y, width: w, height: visual_h }, radius, corners, border_color);
            ctx.rounded_rect(
                Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: visual_h - 2.0 },
                inner_radius,
                corners,
                bg_color,
            );
        }
    }

    /// Emit the selected-text (per-character fade against the right edge) and the ▼ arrow —
    /// the legacy `text_labels` body minus the control label (the adapter's base-label
    /// machinery draws that, with the +4px `detached_label_inset`).
    fn paint_text(&self, content: Rect, ctx: &mut PaintCtx) {
        let selected_text = if let Some(ref custom_text) = self.custom_display_text {
            custom_text.clone()
        } else {
            self.options.get(self.selected).cloned().unwrap_or_default()
        };

        let (font_family, font_size) = crate::layout::control_label_font_detached_parsed();
        let label_x = side_offset(&self.label);
        let x = content.x + label_x;
        let w = content.width - label_x;
        let start_x = x + 8.0;
        let right_limit = x + w - 28.0; // 10px margin before the arrow
        let fade_start_x = (right_limit - 24.0).max(start_x); // Fade out over the last 24px
        let text_y = crate::layout::center_text_y(content.y, content.height, font_size);
        let tc = colors::dropdown_text_color();
        let default_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        let bg_color = colors::dropdown_background_color();
        let mut parent_color = colors::page_color();
        if let Some(parent_ptr) = self.parent {
            unsafe {
                parent_color = (*parent_ptr).color();
            }
        }
        let alpha = 1.0; // The dropdown background is drawn fully opaque
        let bg_rgb = [
            ((parent_color[0] * (1.0 - alpha) + bg_color[0] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[1] * (1.0 - alpha) + bg_color[1] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
            ((parent_color[2] * (1.0 - alpha) + bg_color[2] * alpha) * 255.0).round().clamp(0.0, 255.0) as u8,
        ];

        let w_dummy = crate::widget::display::measure_text_width("M", &font_family, font_size);
        let chars: Vec<char> = selected_text.chars().collect();
        let n = chars.len();

        let is_monospace = {
            let w_i10 = crate::widget::display::measure_text_width("iiiiiiiiii", &font_family, font_size);
            let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", &font_family, font_size);
            (w_i10 - w_m10).abs() < 5.0
        };

        let cell_width = if is_monospace {
            let w_m10 = crate::widget::display::measure_text_width("mmmmmmmmmm", &font_family, font_size);
            let w_m20 = crate::widget::display::measure_text_width("mmmmmmmmmmmmmmmmmmmm", &font_family, font_size);
            ((w_m20 - w_m10) / 10.0).max(1.0)
        } else {
            0.0
        };

        let mut char_offsets = Vec::with_capacity(n);
        if is_monospace {
            for i in 0..n {
                char_offsets.push(i as f32 * cell_width);
            }
        } else {
            if n > 0 {
                char_offsets.push(0.0f32);
            }
            let mut prefix = String::new();
            for i in 1..n {
                prefix.push(chars[i - 1]);
                let measure_str = format!("{}M", prefix);
                let w_prefix_dummy = crate::widget::display::measure_text_width(&measure_str, &font_family, font_size);
                let offset = (w_prefix_dummy - w_dummy).max(0.0);
                char_offsets.push(offset);
            }
        }

        let total_advance = if n > 0 {
            if is_monospace {
                n as f32 * cell_width
            } else {
                let measure_str = format!("{}M", selected_text);
                (crate::widget::display::measure_text_width(&measure_str, &font_family, font_size) - w_dummy).max(0.0)
            }
        } else {
            0.0
        };

        // Draw and fade every character individually
        let mut prev_char_end = 0.0;
        for i in 0..n {
            let mut offset = char_offsets[i];
            if !is_monospace {
                if i > 0 {
                    offset = offset.max(prev_char_end + 1.0);
                }
            }
            let next_offset = if i < n - 1 { char_offsets[i + 1] } else { total_advance };
            let c_w = if is_monospace { cell_width } else { next_offset - offset };
            let cur_x = start_x + offset;

            if cur_x >= right_limit {
                break;
            }

            let char_mid_x = cur_x + c_w / 2.0;
            let mut skip_char = false;
            let text_end_x = start_x + total_advance;
            let color = if text_end_x > right_limit && char_mid_x > fade_start_x {
                let factor = ((char_mid_x - fade_start_x) / (right_limit - fade_start_x)).clamp(0.0, 1.0);
                if factor >= 0.9 {
                    skip_char = true;
                    default_color
                } else {
                    [
                        (default_color[0] as f32 + (bg_rgb[0] as f32 - default_color[0] as f32) * factor).round() as u8,
                        (default_color[1] as f32 + (bg_rgb[1] as f32 - default_color[1] as f32) * factor).round() as u8,
                        (default_color[2] as f32 + (bg_rgb[2] as f32 - default_color[2] as f32) * factor).round() as u8,
                    ]
                }
            } else {
                default_color
            };

            if !skip_char {
                ctx.text(chars[i].to_string(), cur_x, text_y, font_size, color);
                let c_w_ink = if is_monospace {
                    cell_width
                } else {
                    crate::widget::display::measure_text_width(&chars[i].to_string(), &font_family, font_size)
                };
                prev_char_end = offset + c_w_ink;
            }
        }

        ctx.text(
            "▼",
            x + w - 18.0,
            crate::layout::center_text_y(content.y, content.height, 10.0),
            10.0,
            [0x83, 0x83, 0x8a],
        );
    }

    /// Port of the legacy `keyboard_input` body.
    fn handle_key(&mut self, event: &crate::widget::KeyEvent) -> bool {
        if event.state != ElementState::Pressed {
            return false;
        }
        if !self.open {
            if let Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) = event.logical_key {
                self.open = true;
                let mut start_idx = self.selected;
                if start_idx < self.options.len() && self.options[start_idx] == "-" {
                    for i in 0..self.options.len() {
                        if self.options[i] != "-" {
                            start_idx = i;
                            break;
                        }
                    }
                }
                self.hovered_item = Some(start_idx);
                return true;
            }
            return false;
        }

        match event.logical_key {
            Key::Named(NamedKey::ArrowDown) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut next = (current + 1) % self.options.len();
                for _ in 0..self.options.len() {
                    if self.options[next] != "-" {
                        self.hovered_item = Some(next);
                        break;
                    }
                    next = (next + 1) % self.options.len();
                }
                true
            }
            Key::Named(NamedKey::ArrowUp) => {
                let current = self.hovered_item.unwrap_or(self.selected);
                let mut prev = if current == 0 { self.options.len() - 1 } else { current - 1 };
                for _ in 0..self.options.len() {
                    if self.options[prev] != "-" {
                        self.hovered_item = Some(prev);
                        break;
                    }
                    prev = if prev == 0 { self.options.len() - 1 } else { prev - 1 };
                }
                true
            }
            Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                if let Some(idx) = self.hovered_item {
                    if idx < self.options.len() && self.options[idx] != "-" {
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                        self.open = false;
                    }
                }
                true
            }
            Key::Named(NamedKey::Escape) => {
                self.open = false;
                true
            }
            _ => false,
        }
    }
}

impl Adapted<Dropdown> {
    pub fn with_custom_display_text(mut self, text: &str) -> Self {
        self.custom_display_text = Some(text.to_string());
        self
    }

    pub fn with_font_family(mut self, font_family: &str) -> Self {
        self.font_family = font_family.to_string();
        self
    }

    pub fn with_open_upward(mut self, open_upward: bool) -> Self {
        self.open_upward = Some(open_upward);
        self
    }

    pub fn with_auto_width(mut self, auto_width: bool) -> Self {
        self.auto_width = auto_width;
        self
    }

    /// Popover geometry from the widget's laid-out rect — the legacy inherent
    /// `get_popover_geom` shape, for callers that hold the wrapper.
    pub fn get_popover_geom(&self) -> (f32, f32, f32, f32) {
        let (x, y, w, h) = Element::rect(self);
        let top = self.inner().label_top();
        self.inner().popover_geom(Rect { x, y: y + top, width: w, height: h - top })
    }
}

impl Layout for Dropdown {
    fn layout_ignore(&self) -> bool {
        true
    }

    fn z_order(&self) -> i32 {
        if self.open {
            100
        } else {
            0
        }
    }

    fn inflates_label_rect(&self) -> bool {
        false
    }

    fn detached_label_inset(&self) -> f32 {
        4.0
    }

    /// Content size for the scene layout engine (Phase 2b): wide enough for the widest option
    /// (via `content_width`, which already includes the arrow/padding inset), at the configured
    /// dropdown height — so the control doesn't resize as the selection changes.
    fn intrinsic_size(&self) -> Option<Size> {
        Some(Size::new(self.content_width(), crate::layout::dropdown_height()))
    }

    fn intrinsic_measure_width(&self) -> bool {
        self.auto_width
    }

    fn parent_changed(&mut self, parent: Option<*mut (dyn Element + 'static)>) {
        self.tracked_parent = parent;
    }
}

impl Paint for Dropdown {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::dropdown_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            Some((r, (false, false, false, false)))
        }
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::control_label_font_detached())
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        self.paint_background(rect, ctx);
        self.paint_text(rect, ctx);
    }

    fn popover(&self, rect: Rect) -> Option<(f32, f32, f32, f32)> {
        if self.open {
            Some(self.popover_geom(rect))
        } else {
            None
        }
    }

    fn draw_popover(&self, rect: Rect, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.open {
            return;
        }

        let (rx, ry, rw, rh) = self.popover_geom(rect);

        // 1. Soft layered drop shadows
        pc.rect([0.02, 0.02, 0.05, 0.15], rx + 1.0, ry + 1.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.08], rx + 3.0, ry + 3.0, rw, rh);
        pc.rect([0.02, 0.02, 0.05, 0.04], rx + 5.0, ry + 5.0, rw, rh);

        let theme = colors::active_theme();

        // 2. High-contrast premium outer border
        pc.rect(theme.surface_border, rx, ry, rw, rh);

        // 3. Frosted glass background
        pc.rect(theme.surface_bg, rx + 1.0, ry + 1.0, rw - 2.0, rh - 2.0); // bg

        if let Some(h_idx) = self.hovered_item {
            let iy = ry + h_idx as f32 * 24.0;
            // 4. Vibrantly colored translucent selection highlight
            pc.rect(theme.primary_accent, rx + 2.0, iy + 2.0, rw - 4.0, 20.0);
        }

        for (idx, opt) in self.options.iter().enumerate() {
            let iy = crate::layout::align_text_y(ry + idx as f32 * 24.0, 24.0, 12.0, 0.0);

            if opt == "-" {
                pc.rect(theme.surface_border, rx + 8.0, ry + idx as f32 * 24.0 + 11.5, rw - 16.0, 1.0);
                continue;
            }

            let text_color = if self.hovered_item == Some(idx) {
                [0xff, 0xff, 0xff]
            } else if self.selected == idx {
                [0x3a, 0x9a, 0xff]
            } else {
                [0xcc, 0xcc, 0xd4]
            };

            let color_f32 = [
                text_color[0] as f32 / 255.0,
                text_color[1] as f32 / 255.0,
                text_color[2] as f32 / 255.0,
                1.0,
            ];

            let bounds = Some([rx, ry, rx + rw, ry + rh]);
            let font = crate::layout::control_label_font_detached();
            pc.text_with_font_and_bounds(opt, rx + 8.0, iy, 12.0, color_f32, &font, bounds);
        }
    }
}

impl Input for Dropdown {
    /// The legacy geometric test: the widget rect (edges inclusive), extended to the open
    /// popover. `rect` is the full base rect (label strip included), as legacy `hit_test` used.
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        if rect.width <= 0.0 || rect.height <= 0.0 {
            return false;
        }
        let hit_trigger =
            x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height;
        if self.open {
            let top = self.label_top();
            let content = Rect { x: rect.x, y: rect.y + top, width: rect.width, height: rect.height - top };
            let (rx, ry, rw, rh) = self.popover_geom(content);
            let hit_popover = x >= rx && x <= rx + rw && y >= ry && y <= ry + rh;
            hit_trigger || hit_popover
        } else {
            hit_trigger
        }
    }

    fn opens_context_menu(&self) -> bool {
        true
    }

    /// Ungated presses (legacy `mouse_input` saw every press): an open dropdown must close on
    /// an outside click it would otherwise never learn about.
    fn gates_presses(&self) -> bool {
        false
    }

    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                x: px,
                y: py,
                ..
            } => {
                let content = ectx.rect;
                let top = self.label_top();
                let (bx, by, bw, bh) = (content.x, content.y - top, content.width, content.height + top);
                let (rx, ry, rw, rh) = self.popover_geom(content);

                let inside_trigger = *px >= bx && *px <= bx + bw && *py >= by && *py <= by + bh;
                let inside_popover =
                    self.open && *px >= rx && *px <= rx + rw && *py >= ry && *py <= ry + rh;

                if inside_popover {
                    let idx = ((py - ry) / 24.0) as usize;
                    if idx < self.options.len() {
                        if self.options[idx] == "-" {
                            return true;
                        }
                        if self.selected != idx || self.custom_display_text.is_some() {
                            self.selected = idx;
                            self.just_changed = true;
                        }
                    }
                    self.open = false;
                    return true;
                }

                if inside_trigger {
                    self.open = !self.open;
                    if self.open {
                        // Legacy `focus()` claimed only the global slot.
                        ectx.request_focus();
                    }
                    return true;
                }

                if self.open {
                    self.open = false;
                    return true;
                }

                false
            }
            Event::PointerMove { x: px, y: py, .. } => {
                // The popover-item half of the legacy `on_cursor_moved`; the trigger-hover half
                // is the adapter's bookkeeping (MouseEnter/MouseLeave below).
                let was_hovered_item = self.hovered_item;
                self.hovered_item = None;
                if self.open {
                    let (rx, ry, rw, rh) = self.popover_geom(ectx.rect);
                    if *px >= rx && *px <= rx + rw && *py >= ry && *py <= ry + rh {
                        let idx = ((py - ry) / 24.0) as usize;
                        if idx < self.options.len() && self.options[idx] != "-" {
                            self.hovered_item = Some(idx);
                        }
                    }
                }
                self.hovered_item != was_hovered_item
            }
            Event::MouseEnter => {
                self.hovered = true;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            Event::KeyInput(key_event) => self.handle_key(key_event),
            Event::FocusIn => {
                // Legacy `focus()` claimed the global focus slot on every direct call
                // (test-interface focuses the ramp's preset dropdown this way).
                ectx.request_focus();
                false
            }
            Event::FocusOut => {
                // Legacy `unfocus` closed the dropdown.
                self.open = false;
                false
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        self.take_change()
    }

    fn take_change(&mut self) -> bool {
        self.take_change()
    }

    fn value(&self) -> i32 {
        self.selected as i32
    }

    fn value_string(&self) -> Option<String> {
        self.options.get(self.selected).cloned()
    }

    fn set_value_string(&mut self, val: &str) -> bool {
        let val_trimmed = val.trim();
        for (idx, opt) in self.options.iter().enumerate() {
            if opt.eq_ignore_ascii_case(val_trimmed) {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        if let Ok(idx) = val_trimmed.parse::<usize>() {
            if idx < self.options.len() {
                if self.selected != idx {
                    self.selected = idx;
                    self.just_changed = true;
                    return true;
                }
                return false;
            }
        }
        false
    }
}

unsafe impl Send for Dropdown {}
unsafe impl Sync for Dropdown {}

impl Control for Adapted<Dropdown> {
    fn set_label(&mut self, label: &str) {
        Adapted::set_label(self, label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::LayoutConstraints;

    #[test]
    fn test_dropdown_widget_interaction() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["Option A".to_string(), "Option B".to_string(), "Option C".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // 1. Initial State
        assert!(!dd.open);
        assert_eq!(dd.selected, 0);

        // 2. Click trigger area opens dropdown
        let input_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(input_changed);
        assert!(dd.open);

        // 3. Hovering options inside popover
        // Popover starts at y = 10 + 24 = 34. Options are of height 24 each.
        // Hover option B at y = 34 + 24 + 12 = 70.0
        let move_changed = dd.on_cursor_moved(50.0, 70.0, &mut dummy);
        assert!(move_changed);
        assert_eq!(dd.hovered_item, Some(1));

        // 4. Click option B selects it and closes dropdown
        let select_changed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
        assert!(select_changed);
        assert!(!dd.open);
        assert_eq!(dd.selected, 1);
        assert!(dd.take_change());
    }

    #[test]
    fn test_dropdown_context_menu_with_config() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["Option A".to_string(), "Option B".to_string()];
        let mut dd = Dropdown::new(options, 0).with_config("path/to/config.json", "some_key");
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        assert!(!crate::widget::context_menu::is_visible());

        // Right click dropdown
        let handled = dd.mouse_input(MouseButton::Right, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(handled);

        assert!(crate::widget::context_menu::is_visible());
        let menu_options = crate::widget::context_menu::options();
        assert!(menu_options.len() >= 3);
        assert_eq!(menu_options[1], "File: path/to/config.json");
        assert_eq!(menu_options[2], "Key: some_key");

        crate::widget::context_menu::hide();
        assert!(!crate::widget::context_menu::is_visible());
    }

    #[test]
    fn test_dropdown_separators() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec![
            "Option A".to_string(),
            "-".to_string(),
            "Option B".to_string(),
        ];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        // Open dropdown
        dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(dd.open);

        // Hover over separator at index 1 at y = 34 + 24 + 12 = 70.0
        dd.on_cursor_moved(50.0, 70.0, &mut dummy);
        assert_eq!(dd.hovered_item, None); // Separator should not be hovered

        // Click separator at index 1
        let clicked = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 70.0, &mut dummy);
        assert!(clicked);
        assert!(dd.open); // Dropdown should remain open
        assert_eq!(dd.selected, 0); // Selection should not change

        // Hover over Option B at index 2 at y = 34 + 48 + 12 = 94.0
        dd.on_cursor_moved(50.0, 94.0, &mut dummy);
        assert_eq!(dd.hovered_item, Some(2));

        // Keyboard arrow up from index 2 should skip separator (index 1) and go to index 0
        let key_up = crate::widget::KeyEvent {
            state: ElementState::Pressed,
            logical_key: Key::Named(NamedKey::ArrowUp),
            text: None,
            repeat: false,
            ctrl: false,
            shift: false,
        };
        dd.keyboard_input(&key_up, &mut dummy);
        assert_eq!(dd.hovered_item, Some(0));
    }

    #[test]
    fn test_dropdown_ramp_parent_constraints() {
        let mut ramp = crate::widget::Ramp::new();
        // Set the rect of parent Ramp
        ramp.set_rect(20.0, 20.0, 410.0, 260.0);

        let options = vec![
            "Option 1".to_string(),
            "Option 2".to_string(),
            "Option 3".to_string(),
            "Option 4".to_string(),
            "Option 5".to_string(),
            "Option 6".to_string(),
        ];
        let mut dd = Dropdown::new(options, 0).with_label("Preset");
        dd.set_rect(30.0, 125.0, 110.0, 20.0);

        // Link the dropdown parent pointer to the Ramp (the legacy direct-write path)
        dd.parent = Some(&mut ramp as *mut crate::widget::Ramp as *mut (dyn crate::widget::Element + 'static));

        // Compute geometry
        let (rx, ry, rw, rh) = dd.get_popover_geom();

        // Validate coordinates stay inside the parent Ramp bounds: x in [20, 430], y in [20, 280]
        assert!(rx >= 20.0, "rx {} should be >= 20.0", rx);
        assert!(rx + rw <= 430.0, "rx + rw {} should be <= 430.0", rx + rw);
        assert!(ry >= 20.0, "ry {} should be >= 20.0", ry);
        assert!(ry + rh <= 280.0, "ry + rh {} should be <= 280.0", ry + rh);
    }

    #[test]
    fn test_dropdown_label_fade_out() {
        let options = vec!["This is a very long option name that will exceed the dropdown width".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0); // very narrow dropdown

        let labels = dd.own_text_labels();
        // Labels are individual characters of selected_text, then the ▼ arrow (prim order).
        assert!(labels.len() > 2);

        // The last character label (excluding the arrow) should be faded (i.e. not the default color)
        let last_char_idx = labels.len() - 2;
        let first_char = &labels[0];
        let last_char = &labels[last_char_idx];

        let tc = colors::dropdown_text_color();
        let expected_color = [
            (colors::linear_to_srgb(tc[0]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[1]) * 255.0).round() as u8,
            (colors::linear_to_srgb(tc[2]) * 255.0).round() as u8,
        ];
        assert_eq!(first_char.color, expected_color);
        assert_ne!(last_char.color, expected_color); // color has shifted towards background
    }

    #[test]
    fn test_dropdown_auto_width() {
        let dummy = crate::context::UiContext::new();
        let options = vec!["Short".to_string(), "A much longer option name".to_string()];
        let mut dd = Dropdown::new(options, 0).with_auto_width(true);
        dd.set_rect(10.0, 10.0, 50.0, 24.0);

        let size = dd.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
        assert!(size.width > 50.0, "Measured auto-width {} should be greater than original width 50.0", size.width);

        let dd_no_auto = Dropdown::new(vec!["Short".to_string(), "A much longer option name".to_string()], 0);
        let size_no_auto = dd_no_auto.measure(LayoutConstraints::new(0.0, 500.0, 24.0, 24.0), &dummy);
        assert_eq!(size_no_auto.width, 0.0);
    }

    #[test]
    fn intrinsic_size_fits_widest_option() {
        let wide = Dropdown::new(
            vec!["Short".to_string(), "A much longer option name".to_string()],
            0,
        );
        let size = Layout::intrinsic_size(wide.inner()).expect("dropdown reports intrinsic size");
        assert!(size.width >= wide.content_width(), "width fits the widest option");
        assert_eq!(size.height, crate::layout::dropdown_height());

        let narrow = Dropdown::new(vec!["Hi".to_string()], 0);
        assert!(
            size.width > Layout::intrinsic_size(narrow.inner()).unwrap().width,
            "more/longer options measure wider",
        );
    }

    /// Migration additions: popover routing through the adapter (`Element::popover_rect` /
    /// `render_popover`), outside-press close, and Escape via routed key events.
    #[test]
    fn popover_reaches_hosts_through_the_adapter() {
        let mut dummy = crate::context::UiContext::new();
        let options = vec!["A".to_string(), "B".to_string()];
        let mut dd = Dropdown::new(options, 0);
        dd.set_rect(10.0, 10.0, 100.0, 24.0);

        assert!(Element::popover_rect(&dd).is_none(), "closed dropdown registers no popover");

        dd.mouse_input(MouseButton::Left, ElementState::Pressed, 50.0, 20.0, &mut dummy);
        assert!(dd.open);
        let (rx, ry, rw, rh) = Element::popover_rect(&dd).expect("open dropdown registers its popover");
        assert_eq!((rx, ry), (10.0, 34.0), "popover opens under the trigger");
        assert!(rw >= 100.0 && rh == 48.0);

        // An outside press closes it (ungated presses — `gates_presses` is false).
        let closed = dd.mouse_input(MouseButton::Left, ElementState::Pressed, 500.0, 500.0, &mut dummy);
        assert!(closed);
        assert!(!dd.open);
        assert!(!dd.take_change(), "outside close does not report a change");
    }
}
