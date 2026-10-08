//! The search keys the tree and list widgets open and close their search with: input.kdl's
//! `open_search` / `close_search` chords, falling back to the legacy colour-file keys.

use super::*;

pub fn tree_open_search_key() -> String {
    load_colors_once();
    let legacy = TREE_OPEN_SEARCH_KEY.read().unwrap().clone();
    crate::input::widget_chord("open_search", &legacy, "ctrl+f")
}
pub fn set_tree_open_search_key(k: String) {
    style_write(&TREE_OPEN_SEARCH_KEY, k);
}

pub fn list_open_search_key() -> String {
    load_colors_once();
    let legacy = LIST_OPEN_SEARCH_KEY.read().unwrap().clone();
    crate::input::widget_chord("open_search", &legacy, "ctrl+f")
}
pub fn set_list_open_search_key(k: String) {
    style_write(&LIST_OPEN_SEARCH_KEY, k);
}

pub fn list_close_search_key() -> String {
    load_colors_once();
    let legacy = LIST_CLOSE_SEARCH_KEY.read().unwrap().clone();
    crate::input::widget_chord("close_search", &legacy, "escape")
}
pub fn set_list_close_search_key(k: String) {
    style_write(&LIST_CLOSE_SEARCH_KEY, k);
}
