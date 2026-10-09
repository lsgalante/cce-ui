//! The list colours and their slots: lists, breadcrumbs, the tree list, and scrollbars.

use super::*;

pub(super) static LIST_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.LIST_BG_COLOR, |s| &mut s.color.LIST_BG_COLOR);

pub(super) static LIST_ENTRY_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.LIST_ENTRY_BG_COLOR, |s| &mut s.color.LIST_ENTRY_BG_COLOR);

pub(super) static LIST_ENTRY_HIGHLIGHT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.LIST_ENTRY_HIGHLIGHT_COLOR, |s| &mut s.color.LIST_ENTRY_HIGHLIGHT_COLOR);

pub(super) static LIST_FONT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.LIST_FONT_COLOR, |s| &mut s.color.LIST_FONT_COLOR);

pub(super) static BREADCRUMB_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.BREADCRUMB_BG_COLOR, |s| &mut s.color.BREADCRUMB_BG_COLOR);

pub(super) static TREE_BACKGROUND_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_BACKGROUND_COLOR, |s| &mut s.color.TREE_BACKGROUND_COLOR);

pub(super) static TREE_BORDER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_BORDER_COLOR, |s| &mut s.color.TREE_BORDER_COLOR);

pub(super) static TREE_BORDER_HOVER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_BORDER_HOVER_COLOR, |s| &mut s.color.TREE_BORDER_HOVER_COLOR);

pub(super) static TREE_BORDER_FOCUS_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_BORDER_FOCUS_COLOR, |s| &mut s.color.TREE_BORDER_FOCUS_COLOR);

pub(super) static TREE_OPEN_SEARCH_KEY: crate::style::StyleCell<String> = crate::style::StyleCell::new(|s| &s.color.TREE_OPEN_SEARCH_KEY, |s| &mut s.color.TREE_OPEN_SEARCH_KEY);

pub(super) static LIST_OPEN_SEARCH_KEY: crate::style::StyleCell<String> = crate::style::StyleCell::new(|s| &s.color.LIST_OPEN_SEARCH_KEY, |s| &mut s.color.LIST_OPEN_SEARCH_KEY);

pub(super) static LIST_CLOSE_SEARCH_KEY: crate::style::StyleCell<String> = crate::style::StyleCell::new(|s| &s.color.LIST_CLOSE_SEARCH_KEY, |s| &mut s.color.LIST_CLOSE_SEARCH_KEY);

pub(super) static TREE_SECTION_BG_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_SECTION_BG_COLOR, |s| &mut s.color.TREE_SECTION_BG_COLOR);

pub(super) static TREE_SECTION_BG_HOVER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_SECTION_BG_HOVER_COLOR, |s| &mut s.color.TREE_SECTION_BG_HOVER_COLOR);

pub(super) static TREE_LEAF_BG_EVEN_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_LEAF_BG_EVEN_COLOR, |s| &mut s.color.TREE_LEAF_BG_EVEN_COLOR);

pub(super) static TREE_LEAF_BG_ODD_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_LEAF_BG_ODD_COLOR, |s| &mut s.color.TREE_LEAF_BG_ODD_COLOR);

pub(super) static TREE_LEAF_BG_HOVER_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_LEAF_BG_HOVER_COLOR, |s| &mut s.color.TREE_LEAF_BG_HOVER_COLOR);

pub(super) static TREE_LEAF_BG_SELECTED_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_LEAF_BG_SELECTED_COLOR, |s| &mut s.color.TREE_LEAF_BG_SELECTED_COLOR);

pub(super) static TREE_SECTION_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_SECTION_TEXT_COLOR, |s| &mut s.color.TREE_SECTION_TEXT_COLOR);

pub(super) static TREE_LEAF_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_LEAF_TEXT_COLOR, |s| &mut s.color.TREE_LEAF_TEXT_COLOR);

pub(super) static TREE_LEAF_TEXT_SELECTED_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_LEAF_TEXT_SELECTED_COLOR, |s| &mut s.color.TREE_LEAF_TEXT_SELECTED_COLOR);

pub(super) static TREE_TYPE_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_TYPE_TEXT_COLOR, |s| &mut s.color.TREE_TYPE_TEXT_COLOR);

pub(super) static TREE_VALUE_TEXT_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_VALUE_TEXT_COLOR, |s| &mut s.color.TREE_VALUE_TEXT_COLOR);

pub(super) static TREE_SEPARATOR_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.TREE_SEPARATOR_COLOR, |s| &mut s.color.TREE_SEPARATOR_COLOR);

pub(super) static SCROLLBAR_TRACK_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SCROLLBAR_TRACK_COLOR, |s| &mut s.color.SCROLLBAR_TRACK_COLOR);

pub(super) static SCROLLBAR_THUMB_COLOR: crate::style::StyleCell<[f32; 4]> = crate::style::StyleCell::new(|s| &s.color.SCROLLBAR_THUMB_COLOR, |s| &mut s.color.SCROLLBAR_THUMB_COLOR);

pub fn list_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_BG_COLOR)
}

pub fn set_list_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = LIST_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 0.3];
    }
}

pub fn list_entry_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_ENTRY_BG_COLOR)
}

pub fn set_list_entry_bg_color(color: [f32; 4]) {
    style_write(&LIST_ENTRY_BG_COLOR, color);
}

pub fn list_entry_highlight_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_ENTRY_HIGHLIGHT_COLOR)
}

pub fn set_list_entry_highlight_color(color: [f32; 4]) {
    style_write(&LIST_ENTRY_HIGHLIGHT_COLOR, color);
}

pub fn list_font_color() -> [f32; 4] {
    load_colors_once();
    style_read(&LIST_FONT_COLOR)
}

pub fn set_list_font_color(color: [f32; 4]) {
    style_write(&LIST_FONT_COLOR, color);
}

pub fn breadcrumb_bg_color() -> [f32; 4] {
    load_colors_once();
    style_read(&BREADCRUMB_BG_COLOR)
}

pub fn set_breadcrumb_bg_color(color: [f32; 4]) {
    if let Ok(mut lock) = BREADCRUMB_BG_COLOR.write() {
        *lock = [color[0], color[1], color[2], 1.0];
    }
}

pub fn tree_background_color() -> [f32; 4] {
    style_read(&TREE_BACKGROUND_COLOR)
}

pub fn tree_border_color() -> [f32; 4] {
    style_read(&TREE_BORDER_COLOR)
}

pub fn tree_border_hover_color() -> [f32; 4] {
    style_read(&TREE_BORDER_HOVER_COLOR)
}

pub fn tree_border_focus_color() -> [f32; 4] {
    style_read(&TREE_BORDER_FOCUS_COLOR)
}

pub fn tree_section_bg_color() -> [f32; 4] {
    style_read(&TREE_SECTION_BG_COLOR)
}

pub fn tree_section_bg_hover_color() -> [f32; 4] {
    style_read(&TREE_SECTION_BG_HOVER_COLOR)
}

pub fn tree_leaf_bg_even_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_EVEN_COLOR)
}

pub fn tree_leaf_bg_odd_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_ODD_COLOR)
}

pub fn tree_leaf_bg_hover_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_HOVER_COLOR)
}

pub fn tree_leaf_bg_selected_color() -> [f32; 4] {
    style_read(&TREE_LEAF_BG_SELECTED_COLOR)
}

pub fn tree_section_text_color() -> [f32; 4] {
    style_read(&TREE_SECTION_TEXT_COLOR)
}

pub fn tree_leaf_text_color() -> [f32; 4] {
    style_read(&TREE_LEAF_TEXT_COLOR)
}

pub fn tree_leaf_text_selected_color() -> [f32; 4] {
    style_read(&TREE_LEAF_TEXT_SELECTED_COLOR)
}

pub fn tree_type_text_color() -> [f32; 4] {
    style_read(&TREE_TYPE_TEXT_COLOR)
}

pub fn tree_value_text_color() -> [f32; 4] {
    style_read(&TREE_VALUE_TEXT_COLOR)
}

pub fn tree_separator_color() -> [f32; 4] {
    style_read(&TREE_SEPARATOR_COLOR)
}

pub fn set_tree_background_color(c: [f32; 4]) {
    style_write(&TREE_BACKGROUND_COLOR, c);
}

pub fn set_tree_border_color(c: [f32; 4]) {
    style_write(&TREE_BORDER_COLOR, c);
}

pub fn set_tree_border_hover_color(c: [f32; 4]) {
    style_write(&TREE_BORDER_HOVER_COLOR, c);
}

pub fn set_tree_border_focus_color(c: [f32; 4]) {
    style_write(&TREE_BORDER_FOCUS_COLOR, c);
}

pub fn set_tree_section_bg_color(c: [f32; 4]) {
    style_write(&TREE_SECTION_BG_COLOR, c);
}

pub fn set_tree_section_bg_hover_color(c: [f32; 4]) {
    style_write(&TREE_SECTION_BG_HOVER_COLOR, c);
}

pub fn set_tree_leaf_bg_even_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_EVEN_COLOR, c);
}

pub fn set_tree_leaf_bg_odd_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_ODD_COLOR, c);
}

pub fn set_tree_leaf_bg_hover_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_HOVER_COLOR, c);
}

pub fn set_tree_leaf_bg_selected_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_BG_SELECTED_COLOR, c);
}

pub fn set_tree_section_text_color(c: [f32; 4]) {
    style_write(&TREE_SECTION_TEXT_COLOR, c);
}

pub fn set_tree_leaf_text_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_TEXT_COLOR, c);
}

pub fn set_tree_leaf_text_selected_color(c: [f32; 4]) {
    style_write(&TREE_LEAF_TEXT_SELECTED_COLOR, c);
}

pub fn set_tree_type_text_color(c: [f32; 4]) {
    style_write(&TREE_TYPE_TEXT_COLOR, c);
}

pub fn set_tree_value_text_color(c: [f32; 4]) {
    style_write(&TREE_VALUE_TEXT_COLOR, c);
}

pub fn set_tree_separator_color(c: [f32; 4]) {
    style_write(&TREE_SEPARATOR_COLOR, c);
}

pub fn scrollbar_track_color() -> [f32; 4] {
    style_read(&SCROLLBAR_TRACK_COLOR)
}

pub fn set_scrollbar_track_color(c: [f32; 4]) {
    style_write(&SCROLLBAR_TRACK_COLOR, c);
}

pub fn scrollbar_thumb_color() -> [f32; 4] {
    style_read(&SCROLLBAR_THUMB_COLOR)
}

pub fn set_scrollbar_thumb_color(c: [f32; 4]) {
    style_write(&SCROLLBAR_THUMB_COLOR, c);
}
