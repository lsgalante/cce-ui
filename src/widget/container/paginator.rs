use crate::colors;
use crate::widget::*;
use crate::widget::input::get_font_db;
use crate::widget::display::TextLabel;
use super::plate::Plate;
use super::menu::MenuBar;

pub struct Paginator {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    hovered: bool,
    pub sidebar_menu: MenuBar,
    pub plates: Vec<Plate>,
    pub selected_page: usize,
    pub page_changed: bool,
    pub sidebar_label: Option<String>,
    pub tab_text_quads: Vec<Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub sidebar_scroll_y: f32,
    pub scale_factor: f32,
    pub sidebar_mode: bool,
    pub page_hidden: bool,
    pub sidebar_w: f32,
    pub pages: Vec<String>,
    pub tabs_at_top: bool,
    pub tab_y_offset: f32,
    pub tabs_rotated: bool,
    pub hovered_tab: Option<usize>,
    pub pressed_tab: Option<usize>,
    pub target_y: f32,
    pub target_x: f32,
    pub current_y: Option<f32>,
    pub current_x: Option<f32>,
    parent: Option<*mut (dyn Element + 'static)>,
    pub context_options: Vec<String>,
    pub context_selected: usize,
    pub context_just_changed: bool,
    pub tab_quads_cache: std::collections::HashMap<String, Vec<(f32, f32, f32, f32, [f32; 4])>>,
    pub on_page_changed_cb: Option<Box<dyn Fn(usize) + Send + Sync>>,
}

impl Paginator {
    pub fn sidebar_w(&self) -> f32 {
        let padding_x = crate::layout::paginator_tab_padding_x();
        let margin_x = crate::layout::paginator_tab_margin_x();
        if self.tabs_rotated {
            (12.0 + 2.0 * padding_x).max(24.0) + 2.0 * margin_x
        } else {
            let max_tab_req_w = self.pages.iter()
                .map(|p| p.len() as f32 * 7.5 + 2.0 * padding_x)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .unwrap_or(0.0);
            max_tab_req_w.max(24.0) + 2.0 * margin_x
        }
    }

    pub fn update_sidebar_w(&mut self) {
        self.sidebar_w = self.sidebar_w();
    }

    pub fn new(sidebar_w: f32, pages: Vec<String>) -> Self {
        let num_pages = pages.len();
        
        let mut pag = Self {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
            hovered: false,
            sidebar_menu: MenuBar::new(0.0, 0.0, sidebar_w, 0.0).with_vertical(true),
            plates: Vec::new(),
            selected_page: 0,
            page_changed: false,
            sidebar_label: None,
            tab_text_quads: Vec::new(),
            sidebar_scroll_y: 0.0,
            scale_factor: 1.0,
            sidebar_mode: true,
            page_hidden: false,
            sidebar_w,
            pages: pages.clone(),
            tabs_at_top: false,
            tab_y_offset: 10.0,
            tabs_rotated: true,
            hovered_tab: None,
            pressed_tab: None,
            target_y: 10.0,
            target_x: 0.0,
            current_y: Some(10.0),
            current_x: Some(0.0),
            parent: None,
            context_options: Vec::new(),
            context_selected: 0,
            context_just_changed: false,
            tab_quads_cache: std::collections::HashMap::new(),
            on_page_changed_cb: None,
        };

        for page in &pages {
            pag.sidebar_menu = pag.sidebar_menu.with_item(page, &[]);
        }

        let mut plates = Vec::new();
        for _ in 0..num_pages {
            let mut plate = Plate::new(0.0, 0.0, 0.0, 0.0).with_draggable(false);
            plate.visible = false;
            plates.push(plate);
        }
        pag.plates = plates;

        if num_pages > 0 {
            pag.plates[0].visible = true;
            pag.sidebar_menu.menus.set_selected(Some(0));
        }

        pag.update_sidebar_w();
        pag.update_target_pos();
        pag
    }

    pub fn tab_rect(&self, idx: usize) -> (f32, f32, f32, f32) {
        if idx >= self.pages.len() {
            return (0.0, 0.0, 0.0, 0.0);
        }
        let margin_x = crate::layout::paginator_tab_margin_x();
        let margin_y = crate::layout::paginator_tab_margin_y();
        let padding_y = crate::layout::paginator_tab_padding_y();
        if self.tabs_at_top {
            let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
            let tab_h = 2.0 * padding_y + 12.0;
            (self.x + idx as f32 * tab_w, self.y, tab_w, tab_h)
        } else {
            let (tab_w, tab_h) = self.vertical_tab_size();
            let spacing = margin_y;
            (
                self.x + margin_x,
                self.y + self.tab_y_offset + self.sidebar_label_height() + idx as f32 * (tab_h + spacing) - self.sidebar_scroll_y,
                tab_w,
                tab_h,
            )
        }
    }

    pub fn tab_size(&self, idx: usize) -> (f32, f32) {
        let r = self.tab_rect(idx);
        (r.2, r.3)
    }

    pub fn with_column_layout(mut self, enabled: bool) -> Self {
        for plate in &mut self.plates {
            plate.column_layout = enabled;
        }
        self
    }

    pub fn with_sidebar_mode(mut self, enabled: bool) -> Self {
        self.sidebar_mode = enabled;
        self
    }

    pub fn with_sidebar_label(mut self, label: &str) -> Self {
        self.sidebar_label = Some(label.to_string());
        if !self.context_options.is_empty() {
            self.sidebar_menu.title = label.to_string();
            self.sidebar_menu.label = None;
        } else {
            self.sidebar_menu.label = Some(label.to_string());
            self.sidebar_menu.title = String::new();
        }
        self
    }

    pub fn with_context_options(mut self, options: Vec<String>, selected: usize) -> Self {
        self.context_options = options.clone();
        self.context_selected = selected;
        self.sidebar_menu = self.sidebar_menu.with_context_options(options, selected);
        self
    }

    pub fn set_context_selected(&mut self, selected: usize) {
        self.context_selected = selected;
        self.sidebar_menu.set_context_selected(selected);
    }

    pub fn sidebar_label_height(&self) -> f32 {
        if let Some(ref label) = self.sidebar_label {
            if self.tabs_rotated {
                let font_size = 11.0;
                let line_height = font_size * 1.2;
                label.chars().count() as f32 * line_height + 20.0
            } else {
                24.0
            }
        } else {
            0.0
        }
    }

    pub fn is_page_hidden(&self) -> bool {
        self.page_hidden
    }

    pub fn set_page_hidden(&mut self, hidden: bool) {
        self.page_hidden = hidden;
    }

    pub fn set_sidebar_mode(&mut self, enabled: bool) {
        self.sidebar_mode = enabled;
    }

    pub fn add_widget_to_page(&mut self, page_idx: usize, widget: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        if page_idx < self.plates.len() {
            self.plates[page_idx].add_child(widget, ctx);
            unsafe {
                (*widget).set_parent(Some(&mut self.plates[page_idx] as *mut _), ctx);
            }
        }
    }

    pub fn clear_page_widgets(&mut self, page_idx: usize, ctx: &mut UiContext) {
        if page_idx < self.plates.len() {
            self.plates[page_idx].clear_children(ctx);
        }
    }

    pub fn with_tabs_at_top(mut self, top: bool) -> Self {
        self.tabs_at_top = top;
        self.update_target_pos();
        self
    }

    pub fn with_tab_y_offset(mut self, offset: f32) -> Self {
        self.tab_y_offset = offset;
        if self.current_y == Some(10.0) {
            self.current_y = Some(offset);
        }
        self.update_target_pos();
        self
    }

    pub fn with_tabs_rotated(mut self, rotated: bool) -> Self {
        self.tabs_rotated = rotated;
        self.update_target_pos();
        self
    }

    pub fn on_page_changed<F: Fn(usize) + Send + Sync + 'static>(mut self, cb: F) -> Self {
        self.on_page_changed_cb = Some(Box::new(cb));
        self
    }

    pub fn selected_page(&self) -> usize {
        self.selected_page
    }

    pub fn set_selected_page(&mut self, page: usize) {
        if page < self.plates.len() {
            if self.selected_page != page {
                self.plates[self.selected_page].visible = false;
                self.selected_page = page;
                self.plates[self.selected_page].visible = true;
                self.update_target_pos();
                
                // Update MenuBar focus/selection
                self.sidebar_menu.menus.set_selected(Some(page));

                if let Some(ref cb) = self.on_page_changed_cb {
                    cb(page);
                }
            }
        }
    }

    pub fn set_pages(&mut self, pages: Vec<String>) {
        if self.pages == pages {
            return;
        }
        self.pages = pages.clone();
        let num_pages = pages.len();
        
        self.update_sidebar_w();
        let mut sidebar_menu = MenuBar::new(0.0, 0.0, self.sidebar_w, 0.0)
            .with_vertical(true);
        if let Some(ref l) = self.sidebar_label {
            if !self.context_options.is_empty() {
                sidebar_menu = sidebar_menu.with_title(l);
            } else {
                sidebar_menu = sidebar_menu.with_label(l);
            }
        }
        if !self.context_options.is_empty() {
            sidebar_menu = sidebar_menu.with_context_options(self.context_options.clone(), self.context_selected);
        }
        for page in &pages {
            sidebar_menu = sidebar_menu.with_item(page, &[]);
        }
        self.sidebar_menu = sidebar_menu;

        let mut plates = Vec::new();
        for _ in 0..num_pages {
            let mut plate = Plate::new(0.0, 0.0, 0.0, 0.0).with_draggable(false);
            plate.visible = false;
            plates.push(plate);
        }
        self.plates = plates;
        if self.selected_page >= num_pages {
            self.selected_page = 0;
        }
        if !self.plates.is_empty() {
            self.plates[self.selected_page].visible = true;
            self.sidebar_menu.menus.set_selected(Some(self.selected_page));
        }
        self.update_sidebar_w();
        self.update_target_pos();
    }

    pub fn set_pages_with_items(&mut self, pages: Vec<String>, _items: Vec<Vec<String>>) {
        self.set_pages(pages);
    }

    pub fn set_scale_factor(&mut self, scale: f32) {
        self.scale_factor = scale;
    }

    pub fn vertical_tab_size(&self) -> (f32, f32) {
        let margin_x = crate::layout::paginator_tab_margin_x();
        let padding_y = crate::layout::paginator_tab_padding_y();
        let current_sidebar_w = self.sidebar_w();
        let (_, font_size) = crate::layout::menubar_font_parsed();

        if self.tabs_rotated {
            let tab_w = (current_sidebar_w - 2.0 * margin_x).max(24.0);
            let text_h = 64.0 * (font_size / 12.0);
            let tab_h = 4.0 * padding_y + text_h;
            (tab_w, tab_h)
        } else {
            let tab_w = current_sidebar_w - 2.0 * margin_x;
            let tab_h = 2.0 * padding_y + font_size;
            (tab_w, tab_h)
        }
    }

    fn total_sidebar_height(&self) -> f32 {
        let (_, tab_h) = self.vertical_tab_size();
        let spacing = crate::layout::paginator_tab_margin_y();
        let step = tab_h + spacing;
        self.tab_y_offset + self.sidebar_label_height() + self.pages.len() as f32 * step - spacing
    }

    fn update_target_pos(&mut self) {
        self.update_sidebar_w();
        if self.tabs_at_top {
            let tab_w = if self.pages.is_empty() { 0.0 } else { self.w / self.pages.len() as f32 };
            self.target_x = self.selected_page as f32 * tab_w;
            if self.current_x.is_none() {
                self.current_x = Some(self.target_x);
            }
        } else {
            let (_, tab_h) = self.vertical_tab_size();
            let spacing = crate::layout::paginator_tab_margin_y();
            let target = self.tab_y_offset + self.sidebar_label_height() + self.selected_page as f32 * (tab_h + spacing);
            self.target_y = target;
            if self.current_y.is_none() {
                self.current_y = Some(target);
            }
        }
        self.generate_tab_quads();
    }

    fn generate_tab_quads(&mut self) {
        self.tab_text_quads.clear();
        let (tab_w, _) = self.vertical_tab_size();
        let active_color = colors::paginator_tab_label_color();
        let active_srgb = colors::to_srgb(active_color);
        let active_r = (active_srgb[0] * 255.0) as u8;
        let active_g = (active_srgb[1] * 255.0) as u8;
        let active_b = (active_srgb[2] * 255.0) as u8;
        let inactive_r = (active_r as f32 * 0.78) as u8;
        let inactive_g = (active_g as f32 * 0.78) as u8;
        let inactive_b = (active_b as f32 * 0.78) as u8;

        let (font_fam, font_size) = crate::layout::menubar_font_parsed();
        let scale = crate::scale::scale_factor().max(1.0);

        for (i, page_name) in self.pages.iter().enumerate() {
            let color = if self.selected_page == i {
                [active_r, active_g, active_b]
            } else {
                [inactive_r, inactive_g, inactive_b]
            };
            let hex_color = format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2]);

            let trimmed = page_name.trim();
            let has_icon = trimmed.find(' ').is_some();
            let label_text = if let Some(space_idx) = trimmed.find(' ') {
                trimmed.split_at(space_idx).1.trim()
            } else {
                trimmed
            };

            let padding_y = crate::layout::paginator_tab_padding_y();
            let text_h = if has_icon { 52.0 } else { 92.0 };
            let logical_h = (2.0 * padding_y + text_h * (font_size / 12.0)).max(1.0);

            let w_px = (tab_w * scale) as u32;
            let h_px = (logical_h * scale) as u32;

            if w_px == 0 || h_px == 0 {
                self.tab_text_quads.push(Vec::new());
                continue;
            }

            let cache_key = format!("{}:{}:{}:{:?}:{}:{}", trimmed, w_px, h_px, color, font_fam, scale);
            if let Some(cached_quads) = self.tab_quads_cache.get(&cache_key) {
                self.tab_text_quads.push(cached_quads.clone());
                continue;
            }

            let svg_data = format!(
                r##"<svg width="{}" height="{}" viewBox="0 0 {} {}" xmlns="http://www.w3.org/2000/svg">
  <text x="{}" y="{}" font-family="{}" font-size="{}" fill="{}" text-anchor="middle" dominant-baseline="middle" transform="rotate(-90 {} {})">{}</text>
</svg>"##,
                w_px, h_px,
                tab_w, logical_h,
                tab_w / 2.0, logical_h / 2.0,
                font_fam,
                font_size,
                hex_color,
                tab_w / 2.0, logical_h / 2.0,
                label_text
            );

            let opt = resvg::usvg::Options::default();
            let fontdb = get_font_db();
            
            let mut page_quads = Vec::new();
            if let Ok(tree) = resvg::usvg::Tree::from_data(svg_data.as_bytes(), &opt, fontdb) {
                if let Some(mut pixmap) = resvg::tiny_skia::Pixmap::new(w_px, h_px) {
                    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
                    let pixels = pixmap.data();
                    for row in 0..h_px {
                        for col in 0..w_px {
                            let idx = ((row * w_px + col) * 4) as usize;
                            if idx + 3 < pixels.len() {
                                let a = pixels[idx + 3] as f32 / 255.0;
                                if a > 0.0 {
                                    let r = ((pixels[idx] as f32 / 255.0) / a).min(1.0);
                                    let g = ((pixels[idx + 1] as f32 / 255.0) / a).min(1.0);
                                    let b = ((pixels[idx + 2] as f32 / 255.0) / a).min(1.0);
                                    page_quads.push((
                                        col as f32 / scale,
                                        row as f32 / scale,
                                        1.2 / scale,
                                        1.2 / scale,
                                        [r, g, b, a],
                                    ));
                                }
                            }
                        }
                    }
                }
            }
            eprintln!("DEBUG_PAGINATOR: page='{}', w_px={}, h_px={}, quads_len={}", trimmed, w_px, h_px, page_quads.len());
            if !page_quads.is_empty() {
                eprintln!("DEBUG_PAGINATOR_EXAMPLES: {:?}", &page_quads[..5.min(page_quads.len())]);
            }
            self.tab_quads_cache.insert(cache_key, page_quads.clone());
            self.tab_text_quads.push(page_quads);
        }
    }
}


impl Element for Paginator {
    fn rect(&self) -> (f32, f32, f32, f32) {
        (self.x, self.y, self.w, self.h)
    }

    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.x = x;
        self.y = y;
        self.w = w;
        self.h = h;
        self.update_target_pos();

        let self_ptr = self as *mut Paginator;
        let mut dummy = crate::context::UiContext::new();
        self.sidebar_menu.set_parent(Some(self_ptr), &mut dummy);
        for plate in &mut self.plates {
            plate.set_parent(Some(self_ptr), &mut dummy);
        }

        let tabs_at_top = self.tabs_at_top;
        if tabs_at_top {
            let padding_y = crate::layout::paginator_tab_padding_y();
            let (_, font_size) = crate::layout::menubar_font_parsed();
            let tab_h = 2.0 * padding_y + font_size;
            self.sidebar_menu.set_rect(self.x, self.y, self.w, tab_h);
            for plate in &mut self.plates {
                plate.set_rect(self.x, self.y + tab_h, self.w, (self.h - tab_h).max(0.0));
            }
        } else {
            let sidebar_w = self.sidebar_w;
            let label_h = self.sidebar_label_height();
            self.sidebar_menu.set_rect(self.x, self.y + label_h, sidebar_w, (self.h - label_h).max(0.0));
            for plate in &mut self.plates {
                plate.set_rect(self.x + sidebar_w, self.y, (self.w - sidebar_w).max(0.0), self.h);
            }
        }
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.sidebar_menu.set_modifiers(ctrl, shift, alt);
        if self.selected_page < self.plates.len() {
            self.plates[self.selected_page].set_modifiers(ctrl, shift, alt);
        }
    }

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn set_hovered(&mut self, v: bool) {
        self.hovered = v;
    }

    fn hovered(&self) -> bool {
        self.hovered
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self as *const Self as *const () as usize, px, py) {
            return false;
        }
        if self.sidebar_menu.hit_test(px, py, ctx) {
            return true;
        }
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].hit_test(px, py, ctx) {
                return true;
            }
        }
        let (x, y, w, h) = self.rect();
        px >= x && px <= x + w && py >= y && py <= y + h
    }

    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{
        if let Some(i) = self.hovered_tab {
            if self.tabs_at_top {
                let (bx, by, bw, bh) = self.tab_rect(i);
                let bx = bx + 2.0;
                let by = by + 2.0;
                let bw = bw - 4.0;
                let bh = bh - 4.0;
                let scroll_offset = ctx.get_scroll_offset();
                Some((bx, by + scroll_offset, bw, bh, colors::HIGHLIGHT_SECONDARY))
            } else {
                let (bx, by, bw, bh) = self.tab_rect(i);
                let scroll_offset = ctx.get_scroll_offset();
                let qy = by + scroll_offset;
                let qh = bh;

                let min_y = self.y;
                let max_y = self.y + self.h;
                let ry1 = qy.max(min_y);
                let ry2 = (qy + qh).min(max_y);
                let rh = ry2 - ry1;
                if rh > 0.0 {
                    Some((bx, ry1, bw, rh, colors::HIGHLIGHT_SECONDARY))
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = Vec::new();
        if self.tabs_rotated {
            let current_sidebar_w = self.sidebar_w();
            quads.push((self.x, self.y, current_sidebar_w, self.h, colors::sidebar_bg_color()));
            
            let (tab_w, tab_h) = self.vertical_tab_size();
            let margin_x = crate::layout::paginator_tab_margin_x();
            let min_y = self.y;
            let max_y = self.y + self.h;

            // Draw primary highlight for the selected active tab
            if let Some(cy) = self.current_y {
                let bx = self.x + margin_x;
                let by = self.y + cy - self.sidebar_scroll_y;
                let ry1 = by.max(min_y);
                let ry2 = (by + tab_h).min(max_y);
                let rh = ry2 - ry1;
                if rh > 0.0 {
                    quads.push((bx, ry1, tab_w, rh, colors::highlight_primary_color()));
                }
            }

            for (i, page_name) in self.pages.iter().enumerate() {
                let (bx, by, _, _) = self.tab_rect(i);

                let trimmed = page_name.trim();
                let has_icon = trimmed.find(' ').is_some();
                let padding_y = crate::layout::paginator_tab_padding_y();
                let y_offset = if has_icon { 2.0 * padding_y + 12.0 } else { 0.0 };

                if i < self.tab_text_quads.len() {
                    for &(qx, qy, qw, qh, qc) in &self.tab_text_quads[i] {
                        let absolute_x = bx + qx;
                        let absolute_y = by + y_offset + qy;
                        
                        let ry1 = absolute_y.max(min_y);
                        let ry2 = (absolute_y + qh).min(max_y);
                        let rh = ry2 - ry1;
                        if rh > 0.0 {
                            quads.push((absolute_x, ry1, qw, rh, qc));
                        }
                    }
                }
            }
        } else {
            quads.extend(self.sidebar_menu.extra_quads());
        }
        quads
    }

    fn all_quads(&self, ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let mut quads = self.extra_quads();
        if !self.tabs_rotated {
            quads.extend(self.sidebar_menu.all_quads(ctx));
        }

        if self.selected_page < self.plates.len() {
            quads.extend(self.plates[self.selected_page].all_quads(ctx));
        }
        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if self.tabs_rotated {
            if let Some(ref label) = self.sidebar_label {
                let font_size = 11.0;
                let line_height = font_size * 1.2;
                let sidebar_w = self.sidebar_w();
                let start_y = self.y + 10.0;
                let char_w = TextLabel::estimate_width("o", font_size);
                let x_pos = self.x + (sidebar_w - char_w) / 2.0;
                for (i, c) in label.chars().enumerate() {
                    let char_str = c.to_string();
                    let y_pos = start_y + i as f32 * line_height;
                    labels.push(TextLabel {
                        text: char_str,
                        x: x_pos,
                        y: y_pos,
                        font_size,
                        color: [0x83, 0x83, 0x8a],
                    });
                }
            }

            let (tab_w, _) = self.vertical_tab_size();
            let padding_y = crate::layout::paginator_tab_padding_y();
            let active_color = colors::paginator_tab_label_color();
            let active_srgb = colors::to_srgb(active_color);
            let active_r = (active_srgb[0] * 255.0) as u8;
            let active_g = (active_srgb[1] * 255.0) as u8;
            let active_b = (active_srgb[2] * 255.0) as u8;
            let inactive_r = (active_r as f32 * 0.78) as u8;
            let inactive_g = (active_g as f32 * 0.78) as u8;
            let inactive_b = (active_b as f32 * 0.78) as u8;

            for (i, page_name) in self.pages.iter().enumerate() {
                let color = if self.selected_page == i {
                    [active_r, active_g, active_b]
                } else {
                    [inactive_r, inactive_g, inactive_b]
                };
                let (bx, by, _, _) = self.tab_rect(i);
                let bw = tab_w;

                let trimmed = page_name.trim();
                let has_icon = trimmed.find(' ').is_some();
                if has_icon {
                    if let Some(space_idx) = trimmed.find(' ') {
                        let (icon, _) = trimmed.split_at(space_idx);
                        let icon = icon.trim();
                        if !icon.is_empty() {
                            let icon_font_size = 14.0;
                            let est_icon_w = TextLabel::estimate_width(icon, icon_font_size);
                            let icon_y = by + (padding_y - 2.0).max(0.0);
                            if icon_y >= self.y && icon_y + icon_font_size <= self.y + self.h {
                                labels.push(TextLabel {
                                    text: icon.to_string(),
                                    x: bx + (bw - est_icon_w) / 2.0,
                                    y: icon_y,
                                    font_size: icon_font_size,
                                    color,
                                });
                            }
                        }
                    }
                }
            }
        } else {
            labels.extend(self.sidebar_menu.text_labels());
        }

        if self.selected_page < self.plates.len() {
            labels.extend(self.plates[self.selected_page].text_labels());
        }
        labels
    }

    fn text_labels_with_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<[f32; 4]>)> {
        let mut labels = Vec::new();
        for l in self.text_labels() {
            labels.push((l, None));
        }
        labels
    }

    fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        let was_hovered_tab = self.hovered_tab;
        self.hovered_tab = None;
        if self.tabs_rotated {
            for i in 0..self.pages.len() {
                let (bx, by, bw, bh) = self.tab_rect(i);
                if px >= bx && px <= bx + bw && py >= by && py <= by + bh && py >= self.y && py <= self.y + self.h {
                    self.hovered_tab = Some(i);
                    break;
                }
            }
        }
        if was_hovered_tab != self.hovered_tab {
            changed = true;
        }

        if self.sidebar_menu.cursor_moved(px, py, ctx) {
            changed = true;
        }
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].cursor_moved(px, py, ctx) {
                changed = true;
            }
        }

        changed
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].popover_rect().is_some() {
                if self.plates[self.selected_page].mouse_input(button, state, px, py, ctx) {
                    return true;
                }
            }
        }

        let mut clicked_tab = false;
        if button == MouseButton::Left {
            if self.tabs_rotated {
                match state {
                    ElementState::Pressed => {
                        self.pressed_tab = None;
                        for i in 0..self.pages.len() {
                            let (bx, by, bw, bh) = self.tab_rect(i);
                            if px >= bx && px <= bx + bw && py >= by && py <= by + bh && py >= self.y && py <= self.y + self.h {
                                self.pressed_tab = Some(i);
                                clicked_tab = true;
                                break;
                            }
                        }
                        if !clicked_tab {
                            if self.sidebar_menu.mouse_input(button, state, px, py, ctx) {
                                if let Some(new_sel) = self.sidebar_menu.take_context_change() {
                                    self.context_selected = new_sel;
                                    self.context_just_changed = true;
                                }
                                clicked_tab = true;
                            }
                        }
                    }
                    ElementState::Released => {
                        if let Some(i) = self.pressed_tab.take() {
                            let (bx, by, bw, bh) = self.tab_rect(i);
                            if px >= bx && px <= bx + bw && py >= by && py <= by + bh && py >= self.y && py <= self.y + self.h {
                                if self.selected_page != i {
                                    self.set_selected_page(i);
                                    self.page_changed = true;
                                }
                                clicked_tab = true;
                            }
                        } else {
                            if self.sidebar_menu.mouse_input(button, state, px, py, ctx) {
                                if let Some(new_sel) = self.sidebar_menu.take_context_change() {
                                    self.context_selected = new_sel;
                                    self.context_just_changed = true;
                                }
                                clicked_tab = true;
                            }
                        }
                    }
                }
            } else {
                if self.sidebar_menu.mouse_input(button, state, px, py, ctx) {
                    if let Some(new_sel) = self.sidebar_menu.take_context_change() {
                        self.context_selected = new_sel;
                        self.context_just_changed = true;
                    }
                    if let Some((idx, _)) = self.sidebar_menu.menu_click() {
                        if self.selected_page != idx {
                            self.set_selected_page(idx);
                            self.page_changed = true;
                        }
                    }
                    clicked_tab = true;
                }
            }
        }

        if clicked_tab {
            return true;
        }

        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].mouse_input(button, state, px, py, ctx) {
                return true;
            }
        }

        false
    }

    fn keyboard_input(&mut self, event: &KeyEvent, ctx: &mut UiContext) -> bool {
        if self.sidebar_menu.keyboard_input(event, ctx) {
            if let Some(new_sel) = self.sidebar_menu.take_context_change() {
                self.context_selected = new_sel;
                self.context_just_changed = true;
            }
            return true;
        }
        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].keyboard_input(event, ctx) {
                return true;
            }
        }
        false
    }

    fn mouse_wheel(&mut self, delta: &MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.update_sidebar_w();
        if !self.tabs_at_top {
            let bx = self.x;
            let by = self.y;
            let bw = self.sidebar_w;
            let bh = self.h;
            if px >= bx && px <= bx + bw && py >= by && py <= by + bh {
                let scroll_speed = 24.0;
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y * scroll_speed,
                    MouseScrollDelta::PixelDelta(pos) => -pos.y as f32,
                };
                let old_scroll = self.sidebar_scroll_y;
                let max_scroll = (self.total_sidebar_height() - self.h).max(0.0);
                self.sidebar_scroll_y = (self.sidebar_scroll_y + dy).clamp(0.0, max_scroll);
                if (self.sidebar_scroll_y - old_scroll).abs() > 0.01 {
                    self.update_target_pos();
                    return true;
                }
            }
        }

        if self.sidebar_menu.mouse_wheel(delta, px, py, ctx) {
            return true;
        }

        if self.selected_page < self.plates.len() {
            if self.plates[self.selected_page].mouse_wheel(delta, px, py, ctx) {
                return true;
            }
        }
        false
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if let Some(r) = self.sidebar_menu.popover_rect() {
            return Some(r);
        }
        if self.selected_page < self.plates.len() {
            if let Some(r) = self.plates[self.selected_page].popover_rect() {
                return Some(r);
            }
        }
        None
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        self.sidebar_menu.render_popover(pc);
        if self.selected_page < self.plates.len() {
            self.plates[self.selected_page].render_popover(pc);
        }
    }

    fn take_click(&mut self) -> bool {
        if self.page_changed {
            self.page_changed = false;
            true
        } else {
            false
        }
    }

    fn value(&self) -> i32 {
        self.selected_page as i32
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        let mut changed = false;
        if let Some(current) = self.current_y {
            let diff = self.target_y - current;
            if diff.abs() > 0.1 {
                let decay = 15.0;
                let next = current + diff * (1.0 - (-decay * dt).exp());
                self.current_y = Some(next);
                changed = true;
            } else {
                self.current_y = Some(self.target_y);
            }
        }
        if let Some(current) = self.current_x {
            let diff = self.target_x - current;
            if diff.abs() > 0.1 {
                let decay = 15.0;
                let next = current + diff * (1.0 - (-decay * dt).exp());
                self.current_x = Some(next);
                changed = true;
            } else {
                self.current_x = Some(self.target_x);
            }
        }

        let self_ptr = self as *mut Paginator;
        self.sidebar_menu.set_parent(Some(self_ptr), ctx);
        for plate in &mut self.plates {
            plate.set_parent(Some(self_ptr), ctx);
        }

        if self.sidebar_menu.tick(dt, ctx) {
            changed = true;
        }

        for plate in &mut self.plates {
            if plate.tick(dt, ctx) {
                changed = true;
            }
        }

        changed
    }

    fn parent(&self, ctx: &UiContext) -> Option<*mut (dyn Element + 'static)> {
        self.parent
    }

    fn set_parent(&mut self, parent: Option<*mut (dyn Element + 'static)>, ctx: &mut UiContext) {
        self.parent = parent;
    }

    fn children(&self, ctx: &UiContext) -> Vec<*mut (dyn Element + 'static)> {
        let mut list = Vec::new();
        list.push(&self.sidebar_menu as *const dyn Element as *mut dyn Element);
        for plate in &self.plates {
            list.push(plate as *const dyn Element as *mut dyn Element);
        }
        list
    }

    fn add_child(&mut self, _child: *mut (dyn Element + 'static), ctx: &mut UiContext) {}
    fn clear_children(&mut self, ctx: &mut UiContext) {}

    fn as_page_selector(&self) -> Option<&dyn PageSelector> { Some(self) }
    fn as_page_selector_mut(&mut self) -> Option<&mut dyn PageSelector> { Some(self) }
    fn as_menu_controller(&self) -> Option<&dyn MenuController> { Some(self) }
    fn as_menu_controller_mut(&mut self) -> Option<&mut dyn MenuController> { Some(self) }

    fn prepare_text(&mut self, fs: &mut glyphon::FontSystem) {
        self.sidebar_menu.prepare_text(fs);
        for plate in &mut self.plates {
            plate.prepare_text(fs);
        }
    }

    fn text_labels_with_font_and_bounds(&self, ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        let mut result = Vec::new();
        if self.tabs_rotated {
            let font = self.widget_font();
            for l in self.text_labels() {
                result.push((l, font.clone(), None));
            }
        } else {
            result.extend(self.sidebar_menu.text_labels_with_font_and_bounds(ctx));
        }

        if self.selected_page < self.plates.len() {
            result.extend(self.plates[self.selected_page].text_labels_with_font_and_bounds(ctx));
        }
        result
    }

    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::menubar_font())
    }

    fn focus(&mut self) {
        self.sidebar_menu.focus();
    }

    fn unfocus(&mut self) {
        self.sidebar_menu.unfocus();
    }

    fn focused(&self, ctx: &UiContext) -> bool {
        self.sidebar_menu.focused(ctx)
    }
}

unsafe impl Send for Paginator {}
unsafe impl Sync for Paginator {}

impl PageSelector for Paginator {
    fn selected_page(&self) -> usize {
        self.selected_page()
    }
    fn set_selected_page(&mut self, page: usize) {
        self.set_selected_page(page);
    }
    fn is_page_hidden(&self) -> bool {
        self.is_page_hidden()
    }
    fn set_page_hidden(&mut self, hidden: bool) {
        self.set_page_hidden(hidden);
    }
    fn set_pages(&mut self, pages: Vec<String>) {
        self.set_pages(pages);
    }
    fn set_pages_with_items(&mut self, pages: Vec<String>, items: Vec<Vec<String>>) {
        self.set_pages_with_items(pages, items);
    }
    fn sidebar_w(&self) -> f32 {
        self.sidebar_w()
    }
    fn set_sidebar_mode(&mut self, enabled: bool) {
        self.set_sidebar_mode(enabled);
    }
    fn set_sidebar_label(&mut self, label: Option<String>) {
        self.sidebar_label = label.clone();
        if !self.context_options.is_empty() {
            self.sidebar_menu.title = label.unwrap_or_default();
            self.sidebar_menu.label = None;
        } else {
            self.sidebar_menu.label = label;
            self.sidebar_menu.title = String::new();
        }
        self.update_target_pos();
    }
    fn add_widget_to_page(&mut self, page_idx: usize, widget: *mut (dyn Element + 'static), ctx: &mut UiContext) {
        self.add_widget_to_page(page_idx, widget, ctx);
    }
    fn clear_page_widgets(&mut self, page_idx: usize, ctx: &mut UiContext) {
        self.clear_page_widgets(page_idx, ctx);
    }
}

impl MenuController for Paginator {
    fn menu_click(&mut self) -> Option<(usize, usize)> {
        if self.sidebar_mode {
            self.sidebar_menu.menu_click()
        } else {
            None
        }
    }

    fn trigger_menu_click(&mut self, menu_idx: usize, item_idx: usize) {
        self.sidebar_menu.trigger_menu_click(menu_idx, item_idx);
    }

    fn set_item_checked(&mut self, menu_idx: usize, item_idx: usize, checked: bool) {
        self.sidebar_menu.set_item_checked(menu_idx, item_idx, checked);
    }

    fn set_menu_items(&mut self, menu_idx: usize, items: &[String]) {
        self.sidebar_menu.set_menu_items(menu_idx, items);
    }

    fn is_menu_bar(&self) -> bool {
        true
    }

    fn is_menu_open(&self) -> bool {
        self.sidebar_menu.is_menu_open()
    }

    fn menu_items(&self) -> Vec<String> {
        self.sidebar_menu.menu_items()
    }

    fn menu_item_checked(&self) -> Vec<Option<bool>> {
        self.sidebar_menu.menu_item_checked()
    }

    fn is_vertical(&self) -> bool {
        self.sidebar_menu.is_vertical()
    }

    fn menu_names(&self) -> Vec<String> {
        self.pages.clone()
    }

    fn menu_items_list(&self) -> Vec<Vec<String>> {
        if let Some(c) = self.sidebar_menu.as_menu_controller() {
            c.menu_items_list()
        } else {
            vec![]
        }
    }

    fn menu_checked_list(&self) -> Vec<Vec<Option<bool>>> {
        if let Some(c) = self.sidebar_menu.as_menu_controller() {
            c.menu_checked_list()
        } else {
            vec![]
        }
    }

    fn take_context_change(&mut self) -> Option<usize> {
        if self.context_just_changed {
            self.context_just_changed = false;
            Some(self.context_selected)
        } else {
            None
        }
    }

    fn set_context_selected(&mut self, selected: usize) {
        self.set_context_selected(selected);
    }

    fn set_center_items(&mut self, center: bool) {
        if let Some(c) = self.sidebar_menu.as_menu_controller_mut() {
            c.set_center_items(center);
        }
    }

    fn get_menu_items_at(&self, px: f32, py: f32) -> Option<(usize, String, Vec<String>, f32, f32, f32, f32)> {
        if let Some(c) = self.sidebar_menu.as_menu_controller() {
            c.get_menu_items_at(px, py)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paginator_rotated_tabs() {
        // Force initialization from ccec/config.toml so Once won't overwrite our test values
        let orig_margin_x = crate::layout::paginator_tab_margin_x();
        let orig_margin_y = crate::layout::paginator_tab_margin_y();
        let orig_padding_x = crate::layout::paginator_tab_padding_x();
        let orig_padding_y = crate::layout::paginator_tab_padding_y();
        let orig_font = crate::layout::menubar_font();

        crate::layout::set_paginator_tab_margin_x(8.0);
        crate::layout::set_paginator_tab_margin_y(10.0);
        crate::layout::set_paginator_tab_padding_x(10.0);
        crate::layout::set_paginator_tab_padding_y(14.0);
        crate::layout::set_menubar_font("Berkeley Mono 12");

        let pages = vec!["📁 Browse".to_string(), "🌐 Network".to_string()];
        let mut paginator = Paginator::new(56.0, pages)
            .with_tab_y_offset(100.0)
            .with_tabs_rotated(true);
        
        paginator.set_rect(0.0, 0.0, 1000.0, 600.0);

        // Verify target_y is updated correctly
        // selected_page = 0: tab_y_offset = 100.0
        assert_eq!(paginator.target_y, 100.0);

        paginator.set_selected_page(1);
        // selected_page = 1: tab_y_offset + 1 * (120.0 + 10.0) = 230.0
        assert_eq!(paginator.target_y, 230.0);

        // Verify hover coordinates
        // Tab 0 bx/by bounds:
        // tab_w = 32.0, tab_h = 120.0, spacing = 10.0, sidebar_w = 48.0
        // bx = self.x + (sidebar_w - tab_w) / 2 = 8.0
        // by = self.y + tab_y_offset + i * 130.0 = 100.0
        // bx range: [8.0, 40.0], by range: [100.0, 220.0]
        
        let mut dummy = crate::context::UiContext::new();

        // Hover at (24.0, 150.0) should hit Tab 0
        let hover_tab0 = paginator.on_cursor_moved(24.0, 150.0, &mut dummy);
        assert!(hover_tab0);
        assert_eq!(paginator.hovered_tab, Some(0));

        // Hover at (24.0, 280.0) should hit Tab 1 (by range: [230.0, 350.0])
        let hover_tab1 = paginator.on_cursor_moved(24.0, 280.0, &mut dummy);
        assert!(hover_tab1);
        assert_eq!(paginator.hovered_tab, Some(1));

        // Clicking Tab 0 selects it
        let click_tab0 = paginator.mouse_input(MouseButton::Left, ElementState::Pressed, 24.0, 150.0, &mut dummy);
        assert!(click_tab0);
        assert_eq!(paginator.pressed_tab, Some(0));

        let release_tab0 = paginator.mouse_input(MouseButton::Left, ElementState::Released, 24.0, 150.0, &mut dummy);
        assert!(release_tab0);
        assert_eq!(paginator.selected_page, 0);

        // Check vertical text formatting
        let labels = paginator.text_labels();
        assert_eq!(labels.len(), 2);
        assert_eq!(labels[0].text, "📁");
        assert_eq!(labels[1].text, "🌐");
        
        // Check that rotated text quads were generated
        assert!(!paginator.tab_text_quads[0].is_empty());
        assert!(!paginator.tab_text_quads[1].is_empty());

        // Restore original values
        crate::layout::set_paginator_tab_margin_x(orig_margin_x);
        crate::layout::set_paginator_tab_margin_y(orig_margin_y);
        crate::layout::set_paginator_tab_padding_x(orig_padding_x);
        crate::layout::set_paginator_tab_padding_y(orig_padding_y);
        crate::layout::set_menubar_font(&orig_font);
    }

    #[test]
    fn test_paginator_vertical_tabs() {
        let pages = vec!["File".to_string(), "Edit".to_string()];
        let mut paginator = Paginator::new(56.0, pages)
            .with_tab_y_offset(10.0)
            .with_tabs_rotated(false);
        
        paginator.set_rect(0.0, 0.0, 1000.0, 600.0);

        let mut dummy = crate::context::UiContext::new();

        // Check rects of menus
        let (bx, by, bw, bh) = paginator.sidebar_menu.menus.item_rect(1);
        assert!(by > 0.0);

        // Click Edit menu (index 1)
        let px = bx + bw / 2.0;
        let py = by + bh / 2.0;

        // Press
        let click_press = paginator.mouse_input(MouseButton::Left, ElementState::Pressed, px, py, &mut dummy);
        assert!(click_press);
        assert_eq!(paginator.selected_page, 0);

        // Release
        let click_release = paginator.mouse_input(MouseButton::Left, ElementState::Released, px, py, &mut dummy);
        assert!(click_release);
        assert_eq!(paginator.selected_page, 1);
        assert!(paginator.page_changed);
    }
}
