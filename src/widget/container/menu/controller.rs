//! What a host drives a `MenuBar` through: `MenuController` (the menus, their items and checks,
//! clicks, the context selector) and `PageSelector`.

use super::*;

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
                        // `context_menu`'s mark convention: a checked item
                        // is drawn with the `check` glyph.
                        let prefix = match checked {
                            Some(true) => crate::widget::context_menu::MARK_CHECK,
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
