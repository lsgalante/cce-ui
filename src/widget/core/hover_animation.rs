//! The page scroll offset a host lifts its page widgets' popovers by: a host that lays its page
//! out unscrolled and draws it `scroll_y` higher sets it each rebuild (`set_scroll_offset`), and
//! the popover hit tests that map a window point into the page subtract it
//! (`UiContext`'s popover queries, the context menu's, TreeList's add-key box). One value per
//! window (`crate::window_state`). The module keeps the name of the hover highlight it once
//! also held.

/// Set the current window's page scroll offset.
pub fn set_scroll_offset(offset: f32) {
    crate::window_state::with(|w| w.scroll_offset.set(offset));
}

/// The current window's page scroll offset (0 until a host sets one).
pub fn get_scroll_offset() -> f32 {
    crate::window_state::with(|w| w.scroll_offset.get())
}

#[cfg(test)]
mod tests {
    /// The offset a host sets is the one the hit tests read, until it sets another.
    #[test]
    fn the_scroll_offset_reads_back_as_set() {
        assert_eq!(super::get_scroll_offset(), 0.0, "a window starts unscrolled");
        super::set_scroll_offset(42.5);
        assert_eq!(super::get_scroll_offset(), 42.5);
        super::set_scroll_offset(0.0);
    }
}
