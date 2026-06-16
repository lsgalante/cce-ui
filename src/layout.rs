use crate::widget::Element;
use crate::context::UiContext;
use std::sync::RwLock;

fn read_config() -> Option<String> {
    let paths = [
        "/home/lsgalante/.config/cce/config.toml",
        "/home/lsgalante/.config/ccec/config.toml",
    ];
    for path in &paths {
        if let Ok(content) = std::fs::read_to_string(path) {
            return Some(content);
        }
    }
    None
}

pub fn parse_font_string(s: &str) -> (String, Option<f32>) {
    let s = s.trim();
    if let Some(last_space_idx) = s.rfind(' ') {
        let (family, size_str) = s.split_at(last_space_idx);
        let size_str = size_str.trim();
        if let Ok(size) = size_str.parse::<f32>() {
            return (family.trim().to_string(), Some(size));
        }
    }
    (s.to_string(), None)
}

static SECTION_PADDING: RwLock<f32> = RwLock::new(8.0);
static SPINBOX_HEIGHT: RwLock<f32> = RwLock::new(26.0);
static COLOR_SELECTOR_HEIGHT: RwLock<f32> = RwLock::new(22.0);
static TEXTBOX_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static FONT_SELECTOR_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static SLIDER_HEIGHT: RwLock<f32> = RwLock::new(28.0);
static TOGGLE_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static COLOR_SELECTOR_FONT: RwLock<String> = RwLock::new(String::new());
static COLOR_SELECTOR_PREVIEW_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static COLOR_SELECTOR_PREVIEW_MARGIN: RwLock<f32> = RwLock::new(0.0);
static MENUBAR_FONT: RwLock<String> = RwLock::new(String::new());
static MENUBAR_FONT_CACHED: RwLock<Option<(String, f32)>> = RwLock::new(None);
static SECTION_LABEL_FONT: RwLock<String> = RwLock::new(String::new());
static NESTED_SECTION_LABEL_FONT: RwLock<String> = RwLock::new(String::new());

static PAGINATOR_TAB_MARGIN_X: RwLock<f32> = RwLock::new(5.0);
static PAGINATOR_TAB_MARGIN_Y: RwLock<f32> = RwLock::new(10.0);
static PAGINATOR_TAB_PADDING_X: RwLock<f32> = RwLock::new(10.0);
static BUTTON_PADDING: RwLock<f32> = RwLock::new(14.0);

static PLATE_PADDING: RwLock<f32> = RwLock::new(20.0);
static DROPDOWN_HEIGHT: RwLock<f32> = RwLock::new(44.0);
static NESTED_SECTION_LABEL_ALIGNMENT: RwLock<u8> = RwLock::new(0);

static LABEL_MARGIN: RwLock<f32> = RwLock::new(6.0);
static BUTTON_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);
static SPINBOX_CORNER_RADIUS: RwLock<f32> = RwLock::new(4.0);

/// Standard line height multiplier for text layout in cce-ui.
pub const TEXT_LINE_HEIGHT_MULTIPLIER: f32 = 1.4;

/// Standard line height based on font size.
pub fn line_height(font_size: f32) -> f32 {
    font_size * TEXT_LINE_HEIGHT_MULTIPLIER
}

/// Aligns a text label's top coordinate (`y`) so it is centered vertically
/// inside a container of height `container_h` starting at `y`.
pub fn center_text_y(y: f32, container_h: f32, font_size: f32) -> f32 {
    y + (container_h - line_height(font_size)) / 2.0
}

pub fn reload_config() {
    if let Some(content) = read_config() {
        let mut menubar_font_changed = false;
        for line in content.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("label_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = LABEL_MARGIN.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("nested_section_label_alignment") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<u8>() {
                    if let Ok(mut lock) = NESTED_SECTION_LABEL_ALIGNMENT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("nested_section_label_offset") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = NESTED_SECTION_LABEL_OFFSET.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("plate_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PLATE_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("page_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGE_MARGIN.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("grid_min_col_width") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = GRID_MIN_COL_WIDTH.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("section_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SECTION_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("spinbox_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SPINBOX_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("spinbox_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SPINBOX_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("toggle_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TOGGLE_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("font_selector_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = FONT_SELECTOR_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = rest.trim();
                let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                    &rest[1..rest.len() - 1]
                } else {
                    rest
                };
                let font = val_str.trim().to_string();
                if let Ok(mut lock) = COLOR_SELECTOR_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("menubar_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = rest.trim();
                let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                    &rest[1..rest.len() - 1]
                } else {
                    rest
                };
                let font = val_str.trim().to_string();
                let mut changed = false;
                if let Ok(mut lock) = MENUBAR_FONT.write() {
                    if *lock != font {
                        *lock = font;
                        changed = true;
                    }
                }
                if changed {
                    menubar_font_changed = true;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("section_label_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = mod_rest(rest);
                let font = rest.trim().to_string();
                if let Ok(mut lock) = SECTION_LABEL_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("nested_section_label_font") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                let rest = mod_rest(rest);
                let font = rest.trim().to_string();
                if let Ok(mut lock) = NESTED_SECTION_LABEL_FONT.write() {
                    *lock = font;
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_preview_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("color_selector_preview_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_MARGIN.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin_x") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_X.write() {
                        *lock = val;
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_X.write() {
                        *lock = val;
                    }
                    if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_Y.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin_y") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_Y.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_x") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = PAGINATOR_TAB_PADDING_X.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_padding") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_PADDING.write() {
                        *lock = val;
                    }
                }
            } else if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_y") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_PADDING.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("textbox_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = TEXTBOX_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("dropdown_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = DROPDOWN_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("slider_height") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = SLIDER_HEIGHT.write() {
                        *lock = val;
                    }
                }
            }
            if let Some(rest) = trimmed.strip_prefix("button_corner_radius") {
                let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                let val_str = rest.trim_end_matches('"').trim();
                if let Ok(val) = val_str.parse::<f32>() {
                    if let Ok(mut lock) = BUTTON_CORNER_RADIUS.write() {
                        *lock = val;
                    }
                }
            }
        }
        if menubar_font_changed {
            if let Ok(mut lock) = MENUBAR_FONT_CACHED.write() {
                *lock = None;
            }
        }
        crate::color::reload_colors(&content);
    }
}

fn mod_rest(rest: &str) -> &str {
    let rest = rest.trim();
    if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
        &rest[1..rest.len() - 1]
    } else {
        rest
    }
}

pub fn label_margin() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("label_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = LABEL_MARGIN.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *LABEL_MARGIN.read().unwrap()
}

pub fn set_label_margin(margin: f32) {
    if let Ok(mut lock) = LABEL_MARGIN.write() {
        *lock = margin;
    }
}

pub fn nested_section_label_alignment() -> u8 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("nested_section_label_alignment") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<u8>() {
                        if let Ok(mut lock) = NESTED_SECTION_LABEL_ALIGNMENT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *NESTED_SECTION_LABEL_ALIGNMENT.read().unwrap()
}

pub fn set_nested_section_label_alignment(align: u8) {
    if let Ok(mut lock) = NESTED_SECTION_LABEL_ALIGNMENT.write() {
        *lock = align;
    }
}

static NESTED_SECTION_LABEL_OFFSET: RwLock<f32> = RwLock::new(0.0);

pub fn nested_section_label_offset() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("nested_section_label_offset") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = NESTED_SECTION_LABEL_OFFSET.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *NESTED_SECTION_LABEL_OFFSET.read().unwrap()
}

pub fn set_nested_section_label_offset(offset: f32) {
    if let Ok(mut lock) = NESTED_SECTION_LABEL_OFFSET.write() {
        *lock = offset;
    }
}


pub fn plate_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("plate_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PLATE_PADDING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PLATE_PADDING.read().unwrap()
}

pub fn set_plate_padding(padding: f32) {
    if let Ok(mut lock) = PLATE_PADDING.write() {
        *lock = padding;
    }
}

static PAGE_MARGIN: RwLock<f32> = RwLock::new(20.0);

pub fn page_margin() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("page_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PAGE_MARGIN.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PAGE_MARGIN.read().unwrap()
}

pub fn set_page_margin(margin: f32) {
    if let Ok(mut lock) = PAGE_MARGIN.write() {
        *lock = margin;
    }
}

static GRID_MIN_COL_WIDTH: RwLock<f32> = RwLock::new(260.0);

pub fn grid_min_col_width() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("grid_min_col_width") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = GRID_MIN_COL_WIDTH.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *GRID_MIN_COL_WIDTH.read().unwrap()
}

pub fn set_grid_min_col_width(width: f32) {
    if let Ok(mut lock) = GRID_MIN_COL_WIDTH.write() {
        *lock = width;
    }
}

pub fn section_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("section_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SECTION_PADDING.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SECTION_PADDING.read().unwrap()
}

pub fn set_section_padding(padding: f32) {
    if let Ok(mut lock) = SECTION_PADDING.write() {
        *lock = padding;
    }
}

pub fn spinbox_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("spinbox_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SPINBOX_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SPINBOX_HEIGHT.read().unwrap()
}

pub fn set_spinbox_height(height: f32) {
    if let Ok(mut lock) = SPINBOX_HEIGHT.write() {
        *lock = height;
    }
}

pub fn toggle_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("toggle_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TOGGLE_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TOGGLE_HEIGHT.read().unwrap()
}

pub fn set_toggle_height(height: f32) {
    if let Ok(mut lock) = TOGGLE_HEIGHT.write() {
        *lock = height;
    }
}

pub fn color_selector_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = COLOR_SELECTOR_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *COLOR_SELECTOR_HEIGHT.read().unwrap()
}

pub fn set_color_selector_height(height: f32) {
    if let Ok(mut lock) = COLOR_SELECTOR_HEIGHT.write() {
        *lock = height;
    }
}

pub fn font_selector_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("font_selector_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = FONT_SELECTOR_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *FONT_SELECTOR_HEIGHT.read().unwrap()
}

pub fn set_font_selector_height(height: f32) {
    if let Ok(mut lock) = FONT_SELECTOR_HEIGHT.write() {
        *lock = height;
    }
}

pub fn color_selector_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "monospace".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = COLOR_SELECTOR_FONT.write() {
            *lock = font;
        }
    });
    let lock = COLOR_SELECTOR_FONT.read().unwrap();
    if lock.is_empty() {
        "monospace".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_color_selector_font(font: &str) {
    if let Ok(mut lock) = COLOR_SELECTOR_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn menubar_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Outfit".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("menubar_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = MENUBAR_FONT.write() {
            *lock = font;
        }
    });
    let lock = MENUBAR_FONT.read().unwrap();
    if lock.is_empty() {
        "Outfit".to_string()
    } else {
        lock.clone()
    }
}

pub fn menubar_font_parsed() -> (String, f32) {
    if let Ok(lock) = MENUBAR_FONT_CACHED.read() {
        if let Some(ref val) = *lock {
            return val.clone();
        }
    }
    let font_str = menubar_font();
    let parsed = parse_font_string(&font_str);
    let size = parsed.1.unwrap_or(12.0);
    let val = (parsed.0, size);
    if let Ok(mut lock) = MENUBAR_FONT_CACHED.write() {
        *lock = Some(val.clone());
    }
    val
}

pub fn set_menubar_font(font: &str) {
    if let Ok(mut lock) = MENUBAR_FONT.write() {
        *lock = font.to_string();
    }
    if let Ok(mut lock) = MENUBAR_FONT_CACHED.write() {
        *lock = None;
    }
}

pub fn section_label_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Outfit".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("section_label_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = SECTION_LABEL_FONT.write() {
            *lock = font;
        }
    });
    let lock = SECTION_LABEL_FONT.read().unwrap();
    if lock.is_empty() {
        "Outfit".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_section_label_font(font: &str) {
    if let Ok(mut lock) = SECTION_LABEL_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn nested_section_label_font() -> String {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let mut font = "Outfit".to_string();
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("nested_section_label_font") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=');
                    let rest = rest.trim();
                    let val_str = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
                        &rest[1..rest.len() - 1]
                    } else {
                        rest
                    };
                    font = val_str.trim().to_string();
                }
            }
        }
        if let Ok(mut lock) = NESTED_SECTION_LABEL_FONT.write() {
            *lock = font;
        }
    });
    let lock = NESTED_SECTION_LABEL_FONT.read().unwrap();
    if lock.is_empty() {
        "Outfit".to_string()
    } else {
        lock.clone()
    }
}

pub fn set_nested_section_label_font(font: &str) {
    if let Ok(mut lock) = NESTED_SECTION_LABEL_FONT.write() {
        *lock = font.to_string();
    }
}

pub fn color_selector_preview_corner_radius() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_preview_corner_radius") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_CORNER_RADIUS.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *COLOR_SELECTOR_PREVIEW_CORNER_RADIUS.read().unwrap()
}

pub fn set_color_selector_preview_corner_radius(radius: f32) {
    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_CORNER_RADIUS.write() {
        *lock = radius;
    }
}

pub fn button_corner_radius() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("button_corner_radius") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = BUTTON_CORNER_RADIUS.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *BUTTON_CORNER_RADIUS.read().unwrap()
}

pub fn set_button_corner_radius(radius: f32) {
    if let Ok(mut lock) = BUTTON_CORNER_RADIUS.write() {
        *lock = radius;
    }
}

pub fn spinbox_corner_radius() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("spinbox_corner_radius") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SPINBOX_CORNER_RADIUS.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SPINBOX_CORNER_RADIUS.read().unwrap()
}

pub fn set_spinbox_corner_radius(radius: f32) {
    if let Ok(mut lock) = SPINBOX_CORNER_RADIUS.write() {
        *lock = radius;
    }
}

pub fn color_selector_preview_margin() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("color_selector_preview_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_MARGIN.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *COLOR_SELECTOR_PREVIEW_MARGIN.read().unwrap()
}

pub fn set_color_selector_preview_margin(margin: f32) {
    if let Ok(mut lock) = COLOR_SELECTOR_PREVIEW_MARGIN.write() {
        *lock = margin;
    }
}

pub fn paginator_tab_margin_x() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            let mut general_margin = None;
            let mut x_margin = None;
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin_x") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        x_margin = Some(val);
                    }
                } else if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        general_margin = Some(val);
                    }
                }
            }
            let val = x_margin.or(general_margin).unwrap_or(5.0);
            if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_X.write() {
                *lock = val;
            }
        }
    });
    *PAGINATOR_TAB_MARGIN_X.read().unwrap()
}

pub fn set_paginator_tab_margin_x(margin: f32) {
    if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_X.write() {
        *lock = margin;
    }
}

pub fn paginator_tab_margin_y() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            let mut general_margin = None;
            let mut y_margin = None;
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin_y") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        y_margin = Some(val);
                    }
                } else if let Some(rest) = trimmed.strip_prefix("paginator_tab_margin") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        general_margin = Some(val);
                    }
                }
            }
            let val = y_margin.or(general_margin).unwrap_or(10.0);
            if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_Y.write() {
                *lock = val;
            }
        }
    });
    *PAGINATOR_TAB_MARGIN_Y.read().unwrap()
}

pub fn set_paginator_tab_margin_y(margin: f32) {
    if let Ok(mut lock) = PAGINATOR_TAB_MARGIN_Y.write() {
        *lock = margin;
    }
}

pub fn paginator_tab_padding_x() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_x") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = PAGINATOR_TAB_PADDING_X.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *PAGINATOR_TAB_PADDING_X.read().unwrap()
}

pub fn set_paginator_tab_padding_x(padding: f32) {
    if let Ok(mut lock) = PAGINATOR_TAB_PADDING_X.write() {
        *lock = padding;
    }
}

pub fn button_padding() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            let mut found = false;
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("button_padding") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = BUTTON_PADDING.write() {
                            *lock = val;
                            found = true;
                        }
                    }
                }
            }
            if !found {
                for line in content.lines() {
                    let trimmed = line.trim();
                    if let Some(rest) = trimmed.strip_prefix("paginator_tab_padding_y") {
                        let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                        let val_str = rest.trim_end_matches('"').trim();
                        if let Ok(val) = val_str.parse::<f32>() {
                            if let Ok(mut lock) = BUTTON_PADDING.write() {
                                *lock = val;
                            }
                        }
                    }
                }
            }
        }
    });
    *BUTTON_PADDING.read().unwrap()
}

pub fn set_button_padding(padding: f32) {
    if let Ok(mut lock) = BUTTON_PADDING.write() {
        *lock = padding;
    }
}

pub fn paginator_tab_padding_y() -> f32 {
    button_padding()
}

pub fn set_paginator_tab_padding_y(padding: f32) {
    set_button_padding(padding);
}

pub fn textbox_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("textbox_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = TEXTBOX_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *TEXTBOX_HEIGHT.read().unwrap()
}

pub fn set_textbox_height(height: f32) {
    if let Ok(mut lock) = TEXTBOX_HEIGHT.write() {
        *lock = height;
    }
}

pub fn dropdown_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("dropdown_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = DROPDOWN_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *DROPDOWN_HEIGHT.read().unwrap()
}

pub fn set_dropdown_height(height: f32) {
    if let Ok(mut lock) = DROPDOWN_HEIGHT.write() {
        *lock = height;
    }
}

pub fn slider_height() -> f32 {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        if let Some(content) = read_config() {
            for line in content.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("slider_height") {
                    let rest = rest.trim_start_matches(|c: char| c == ' ' || c == '=' || c == '"');
                    let val_str = rest.trim_end_matches('"').trim();
                    if let Ok(val) = val_str.parse::<f32>() {
                        if let Ok(mut lock) = SLIDER_HEIGHT.write() {
                            *lock = val;
                        }
                    }
                }
            }
        }
    });
    *SLIDER_HEIGHT.read().unwrap()
}

pub fn set_slider_height(height: f32) {
    if let Ok(mut lock) = SLIDER_HEIGHT.write() {
        *lock = height;
    }
}


pub trait RenderTarget {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32);
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, _radius: f32) {
        self.rect(color, x, y, w, h);
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, radius: f32, _corners: (bool, bool, bool, bool)) {
        self.rect_with_radius(color, x, y, w, h, radius);
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]);
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _font: &str) {
        self.text(content, x, y, size, color);
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], _bounds: Option<[f32; 4]>) {
        self.text(content, x, y, size, color);
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, _bounds: Option<[f32; 4]>) {
        self.text_with_font(content, x, y, size, color, font);
    }
}

pub struct PopoverCollector {
    pub rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    pub texts: Vec<(String, f32, f32, f32, [f32; 4], Option<String>, Option<[f32; 4]>)>,
}

impl PopoverCollector {
    pub fn new() -> Self {
        Self { rects: Vec::new(), texts: Vec::new() }
    }
}

impl RenderTarget for PopoverCollector {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.rects.push((color, x, y, w, h));
    }

    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        self.texts.push((content.to_string(), size, x, y, color, None, None));
    }

    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string()), None));
    }

    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        self.texts.push((content.to_string(), size, x, y, color, None, bounds));
    }

    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        self.texts.push((content.to_string(), size, x, y, color, Some(font.to_string()), bounds));
    }
}

pub fn render_widget<T: Element + 'static>(pc: &mut dyn RenderTarget, w: &mut T, x: f32, y: f32, ww: f32, wh: f32, ctx: &mut UiContext) {
    w.layout(crate::widget::Point { x, y }, crate::widget::LayoutConstraints::new(ww, ww, wh, wh), ctx);
    let corners = w.rounded_corners();
    let r = if corners != (false, false, false, false) {
        w.corner_radius()
    } else {
        0.0
    };
    let (wx, mut wy, www, mut whh) = w.rect();
    let top_room = crate::widget::label_offset(w);
    wy += top_room;
    whh -= top_room;

    for (qx, qy, qw, qh, qc) in w.all_quads(ctx) {
        if r <= 0.1 || corners == (false, false, false, false) {
            pc.rect_with_radius_corners(qc, qx, qy, qw, qh, 0.0, (false, false, false, false));
            continue;
        }

        let extra_corners = (
            corners.0 && qx <= wx + 0.1 && qy <= wy + 0.1,
            corners.1 && qx + qw >= wx + www - 0.1 && qy <= wy + 0.1,
            corners.2 && qx + qw >= wx + www - 0.1 && qy + qh >= wy + whh - 0.1,
            corners.3 && qx <= wx + 0.1 && qy + qh >= wy + whh - 0.1,
        );

        if extra_corners == (false, false, false, false) {
            pc.rect_with_radius_corners(qc, qx, qy, qw, qh, 0.0, (false, false, false, false));
        } else {
            pc.rect_with_radius_corners(qc, qx, qy, qw, qh, r, extra_corners);
        }
    }
    let font_opt = w.widget_font();
    for (label, bounds) in w.text_labels_with_bounds(ctx) {
        let color_f32 = [
            label.color[0] as f32 / 255.0,
            label.color[1] as f32 / 255.0,
            label.color[2] as f32 / 255.0,
            1.0,
        ];
        if let Some(ref font) = font_opt {
            pc.text_with_font_and_bounds(&label.text, label.x, label.y, label.font_size, color_f32, font, bounds);
        } else {
            pc.text_with_bounds(&label.text, label.x, label.y, label.font_size, color_f32, bounds);
        }
    }
    if w.popover_rect().is_some() {
        ctx.register_popover(w);
    }
}

pub fn render_popovers(pc: &mut dyn RenderTarget, ctx: &UiContext) {
    for popover_ptr in &ctx.active_popovers {
        unsafe {
            (**popover_ptr).render_popover(pc);
        }
    }
}

pub struct UiFrame;

impl UiFrame {
    pub fn start(scroll_offset: f32) -> Self {
        crate::widget::hover_animation::reset_frame_registration();
        crate::widget::hover_animation::set_scroll_offset(scroll_offset);
        crate::widget::popovers::clear();
        Self
    }

    pub fn finish(self, pc: &mut dyn RenderTarget) {
        crate::widget::hover_animation::post_render_check();
        if let Some((qx, qy, qw, qh, qc)) = crate::widget::hover_animation::get_quad() {
            pc.rect(qc, qx, qy, qw, qh);
        }
        // render_popovers(pc);
    }
}

pub struct Column {
    ox: f32,
    oy: f32,
    cx: f32,
    pub y: f32,
    pub cw: f32,
}

impl Column {
    pub fn new(ox: f32, oy: f32, cx: f32, cy: f32, cw: f32) -> Self {
        Self { ox, oy, cx, y: cy, cw }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        self.ox + self.cx + x_off
    }

    pub fn ay(&self) -> f32 {
        self.oy + self.y
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.y += h;
    }

    pub fn advance(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn spacing(&mut self, dy: f32) {
        self.y += dy;
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let x = self.ax(8.0);
        let y = self.ay();
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 16.0, 1.0);
        self.y += 8.0;
    }

    pub fn header(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32) {
        let x = self.ax(x_off);
        let y = self.ay();
        pc.text(text, x, y, 14.0, [0.83, 0.83, 0.83, 1.0]);
        self.y += 22.0;
    }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        let x = self.ax(x_off);
        let y = self.ay() + y_off;
        pc.text(text, x, y, font_size, color);
    }

    pub fn widget<T: Element + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let top_room = crate::widget::label_offset(w);
        let total_h = wh + top_room;
        let x = self.ax(x_off);
        let y = self.ay();
        w.set_row_rect(self.ox + self.cx + 8.0, self.cw - 16.0);
        let clamped_w = ww.min((self.cw - x_off).max(0.0));
        render_widget(pc, w, x, y, clamped_w, total_h, ctx);
        self.y += total_h;
    }

    pub fn row<F: FnOnce(&mut Row)>(&mut self, pc: &mut dyn RenderTarget, h: f32, f: F) {
        let row_y = self.ay();
        let mut row = Row {
            pc: &mut *pc,
            base_x: self.ox + self.cx,
            y: row_y,
            cursor_x: 0.0,
            spacing: 8.0,
        };
        f(&mut row);
        self.y = self.y + h;
    }
}

pub struct Row<'a> {
    pc: &'a mut dyn RenderTarget,
    base_x: f32,
    y: f32,
    pub cursor_x: f32,
    pub spacing: f32,
}

impl<'a> Row<'a> {
    pub fn set_spacing(&mut self, spacing: f32) {
        self.spacing = spacing;
    }

    pub fn gap(&mut self, width: f32) {
        self.cursor_x += width;
    }

    pub fn text(&mut self, text: &str, y_off: f32, font_size: f32, color: [f32; 4], width: f32) {
        self.pc
            .text(text, self.base_x + self.cursor_x, self.y + y_off, font_size, color);
        self.cursor_x += width + self.spacing;
    }

    pub fn widget<T: Element + 'static>(&mut self, w: &mut T, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        render_widget(self.pc, w, self.base_x + self.cursor_x, self.y, ww, wh, ctx);
        self.cursor_x += ww + self.spacing;
    }
}

fn estimate_label_width_helper(label: &str, font_size: f32, font_fam: &str) -> f32 {
    let fam_lower = font_fam.to_lowercase();
    let is_mono = fam_lower.contains("mono") || fam_lower.contains("courier") || fam_lower == "monospace";
    if is_mono {
        label.chars().count() as f32 * font_size * 0.60
    } else {
        let mut width = 0.0;
        for c in label.chars() {
            let factor = match c {
                'i' | 'l' | 'I' | ' ' | '.' | ',' | '!' | ';' | ':' | '\'' | '"' | '(' | ')' | '[' | ']' | '-' => 0.30,
                'f' | 'j' | 't' => 0.35,
                'r' | 's' | 'c' | 'z' => 0.50,
                'a' | 'b' | 'd' | 'e' | 'g' | 'h' | 'k' | 'n' | 'o' | 'p' | 'q' | 'u' | 'v' | 'x' | 'y' => 0.60,
                'm' | 'w' | 'M' | 'W' | '&' | '@' | 'O' | 'Q' | 'G' => 0.85,
                'A' | 'B' | 'C' | 'D' | 'H' | 'N' | 'U' | 'V' | 'X' | 'Y' => 0.75,
                'E' | 'F' | 'K' | 'L' | 'P' | 'R' | 'S' | 'T' | 'Z' | 'J' => 0.68,
                '0'..='9' => 0.60,
                _ => 0.60,
            };
            width += factor * font_size;
        }
        width
    }
}

pub struct Section {
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub is_child: bool,
}

impl Section {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        if self.is_child {
            section_padding().max(8.0)
        } else {
            section_padding()
        }
    }

    pub fn new(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str) -> Self {
        Self::new_opt(pc, left, top, cw, label, false)
    }

    pub fn new_opt(pc: &mut dyn RenderTarget, left: f32, top: f32, cw: f32, label: &str, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else {
            left + (cw - label_width) / 2.0
        };
        pc.text_with_font(label, label_x, top, font_size, font_color, &font_fam);
        Self { left, top, content_y: top + font_size + 5.0, cw, label_width, is_child }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        let shift = if x_off >= 12.0 { self.padding() } else { 0.0 };
        self.left + x_off + shift
    }

    pub fn ay(&self) -> f32 { self.content_y }

    pub fn spacing(&mut self, dy: f32) { self.content_y += dy; }

    pub fn text(&mut self, pc: &mut dyn RenderTarget, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        pc.text(text, self.ax(x_off), self.ay() + y_off, font_size, color);
    }

    pub fn widget<T: Element + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, x_off: f32, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
        let x = self.ax(x_off);
        let right_edge = self.left + self.cw - pad;
        let clamped_w = ww.min((right_edge - x).max(0.0));
        let top_room = crate::widget::label_offset(w);
        let total_h = wh + top_room;
        render_widget(pc, w, x, self.ay(), clamped_w, total_h, ctx);
        self.content_y += total_h;
    }

    pub fn widget_full<T: Element + 'static>(&mut self, pc: &mut dyn RenderTarget, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off);
        self.widget(pc, w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self, pc: &mut dyn RenderTarget) {
        let pad = self.padding();
        let x = self.ax(pad);
        let y = self.ay();
        pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y += 8.0;
    }

    pub fn rect(&mut self, pc: &mut dyn RenderTarget, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.content_y += h;
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = self.padding() + 12.0;
        let usable_w = self.cw - 2.0 * margin_x;
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;
    }

    pub fn finish(&mut self, pc: &mut dyn RenderTarget) -> f32 {
        self.finish_focused(pc, false)
    }

    pub fn finish_focused(&mut self, pc: &mut dyn RenderTarget, focused: bool) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if focused {
                [0.30, 0.50, 0.32, 1.0]
            } else {
                [0.25, 0.25, 0.35, 1.0]
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;
        
        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = if self.is_child {
                let base_x = match nested_section_label_alignment() {
                    0 => self.left + 12.0,
                    1 => self.left + (self.cw - self.label_width) / 2.0,
                    2 => self.left + self.cw - 12.0 - self.label_width,
                    _ => self.left + 12.0,
                };
                base_x + nested_section_label_offset()
            } else {
                self.left + (self.cw - self.label_width) / 2.0
            };
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            pc.rect(border, left_edge, y, w, 1.0);
        }

        pc.rect(border, x, y + h + 12.0, w, 1.0);
        pc.rect(border, x, y, 1.0, h + 12.0);
        pc.rect(border, x + w - 1.0, y, 1.0, h + 12.0);
        self.content_y + 20.0
    }

    pub fn vstack<'a>(&'a mut self, pc: &'a mut dyn RenderTarget, spacing: f32) -> SectionVStack<'a> {
        SectionVStack {
            section: self,
            pc,
            spacing,
        }
    }
}

pub struct SectionVStack<'a> {
    section: &'a mut Section,
    pc: &'a mut dyn RenderTarget,
    spacing: f32,
}

impl<'a> SectionVStack<'a> {
    pub fn add_widget<T: Element + 'static>(&mut self, w: &mut T, ww: f32, wh: f32, ctx: &mut UiContext) {
        self.section.widget(self.pc, w, Section::DEFAULT_MARGIN_X, ww, wh, ctx);
        self.section.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        self.section.row(count, gap, h, f);
        self.section.spacing(self.spacing);
    }
}


pub struct SplitterLayout {
    pub splitter1_x: f32,
    pub splitter2_x: f32,
    pub splitter_width: f32,
    pub min_column_width: f32,
}

impl SplitterLayout {
    pub fn new(width: f32, splitter_width: f32, min_column_width: f32) -> Self {
        let s1 = (width - 2.0 * splitter_width) / 3.0;
        let s2 = s1 + splitter_width + (width - 2.0 * splitter_width) / 3.0;
        Self {
            splitter1_x: s1,
            splitter2_x: s2,
            splitter_width,
            min_column_width,
        }
    }

    pub fn clamp(&mut self, total_width: f32, detached_circular_network: bool) {
        if detached_circular_network {
            let min_s2 = self.min_column_width;
            let max_s2 = (total_width - self.min_column_width).max(min_s2);
            self.splitter2_x = self.splitter2_x.clamp(min_s2, max_s2);
        } else {
            let min_s1 = self.min_column_width;
            let max_s1 = (self.splitter2_x - self.splitter_width - self.min_column_width).max(min_s1);
            self.splitter1_x = self.splitter1_x.clamp(min_s1, max_s1);
            let min_s2 = self.splitter1_x + self.splitter_width + self.min_column_width;
            let max_s2 = (total_width - self.min_column_width).max(min_s2);
            self.splitter2_x = self.splitter2_x.clamp(min_s2, max_s2);
        }
    }

    pub fn scale(&mut self, factor: f32) {
        self.splitter1_x *= factor;
        self.splitter2_x *= factor;
    }

    pub fn left_col(&self) -> (f32, f32) { // (x, width)
        (0.0, self.splitter1_x)
    }

    pub fn center_col(&self) -> (f32, f32) { // (x, width)
        let x = self.splitter1_x + self.splitter_width;
        (x, self.splitter2_x - x)
    }

    pub fn right_col(&self, total_width: f32) -> (f32, f32) { // (x, width)
        let x = self.splitter2_x + self.splitter_width;
        (x, (total_width - x).max(0.0))
    }
}

pub struct Grid {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub col_width: f32,
    pub gap: f32,
    pub col_heights: Vec<f32>,
    pub col_lefts: Vec<f32>,
}

impl Grid {
    pub fn new(left: f32, top: f32, width: f32, min_col_width: f32, gap: f32, count: usize) -> Self {
        let total_gap = gap * (count - 1) as f32;
        let col_width = if count > 0 {
            (width - total_gap).max(0.0) / count as f32
        } else {
            min_col_width
        };
        let left_offset = 0.0;

        let mut col_lefts = Vec::with_capacity(count);
        let col_heights = vec![top; count];
        for i in 0..count {
            col_lefts.push(left + left_offset + i as f32 * (col_width + gap));
        }

        Self {
            left,
            top,
            width,
            col_width,
            gap,
            col_heights,
            col_lefts,
        }
    }

    pub fn next_column(&self) -> usize {
        let mut min_idx = 0;
        let mut min_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] < min_h {
                min_h = self.col_heights[i];
                min_idx = i;
            }
        }
        min_idx
    }

    pub fn max_height(&self) -> f32 {
        let mut max_h = self.col_heights[0];
        for i in 1..self.col_heights.len() {
            if self.col_heights[i] > max_h {
                max_h = self.col_heights[i];
            }
        }
        max_h
    }
}

pub struct CircularPaneLayout {
    pub x: f32,
    pub y: f32,
    pub r: f32,
}

impl CircularPaneLayout {
    pub fn new(x: f32, y: f32, r: f32) -> Self {
        Self { x, y, r }
    }

    pub fn hit_test_content(&self, cx: f32, cy: f32, menubar_h: f32, breadcrumb_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= self.r * self.r && cy >= self.y - self.r + 45.0 + menubar_h + breadcrumb_h
    }

    pub fn hit_test_menubar(&self, cx: f32, cy: f32, menubar_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist >= self.r - menubar_h && dist <= self.r && cy < self.y
    }

    pub fn hit_test_breadcrumb(&self, cx: f32, cy: f32, menubar_h: f32, breadcrumb_h: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist_sq = dx * dx + dy * dy;
        dist_sq <= self.r * self.r && cy >= self.y - self.r + menubar_h && cy < self.y - self.r + 45.0 + menubar_h + breadcrumb_h
    }

    pub fn hit_test_border(&self, cx: f32, cy: f32, border_thickness: f32) -> bool {
        let dx = cx - self.x;
        let dy = cy - self.y;
        let dist = (dx * dx + dy * dy).sqrt();
        dist >= self.r - border_thickness && dist <= self.r
    }
}

pub struct Radial {
    pub center_x: f32,
    pub center_y: f32,
    pub aspect_ratio: f32,
    pub base_spacing: f32,
}

impl Radial {
    pub fn new(center_x: f32, center_y: f32, aspect_ratio: f32, base_spacing: f32) -> Self {
        Self { center_x, center_y, aspect_ratio, base_spacing }
    }

    pub fn widget_rect(&self, idx: usize, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if idx == 0 {
            (self.center_x - ww / 2.0, self.center_y - wh / 2.0, ww, wh)
        } else {
            let mut ring = 1;
            let mut ring_start = 1;
            loop {
                let ring_capacity = ring * 6;
                if idx < ring_start + ring_capacity {
                    let pos_in_ring = idx - ring_start;
                    let angle = (pos_in_ring as f32) * (2.0 * std::f32::consts::PI / ring_capacity as f32);
                    let radius = (ring as f32) * self.base_spacing;

                    let x_offset = radius * angle.cos() * self.aspect_ratio;
                    let y_offset = radius * angle.sin();

                    return (
                        self.center_x + x_offset - ww / 2.0,
                        self.center_y + y_offset - wh / 2.0,
                        ww,
                        wh,
                    );
                }
                ring_start += ring_capacity;
                ring += 1;
            }
        }
    }

    pub fn layout_widgets<T: Element + 'static>(&self, widgets: &mut [&mut T], ctx: &mut UiContext) {
        let mut active_idx = 0;
        for w in widgets.iter_mut() {
            if !w.layout_ignore() {
                let (_, _, ww, wh) = w.rect();
                let use_w = if ww > 0.0 { ww } else { 100.0 };
                let use_h = if wh > 0.0 { wh } else { 50.0 };
                let (x, y, rw, rh) = self.widget_rect(active_idx, use_w, use_h);
                w.layout(crate::widget::Point { x, y }, crate::widget::LayoutConstraints::new(rw, rw, rh, rh), ctx);
                active_idx += 1;
            }
        }
    }

    pub fn layout_widget_ptors(&self, widgets: &[*mut (dyn Element + 'static)], ctx: &mut UiContext) {
        let mut active_idx = 0;
        for &w_ptr in widgets {
            let w = unsafe { &mut *w_ptr };
            if !w.layout_ignore() {
                let (_, _, ww, wh) = w.rect();
                let use_w = if ww > 0.0 { ww } else { 100.0 };
                let use_h = if wh > 0.0 { wh } else { 50.0 };
                let (x, y, rw, rh) = self.widget_rect(active_idx, use_w, use_h);
                w.layout(crate::widget::Point { x, y }, crate::widget::LayoutConstraints::new(rw, rw, rh, rh), ctx);
                active_idx += 1;
            }
        }
    }
}

pub trait LayoutStrategy {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32);
    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32);
    fn set_section_count(&mut self, _count: usize) {}
    fn get_column_width(&self) -> Option<f32> { None }
}

pub struct ColumnLayout {
    left: f32,
    top: f32,
    width: f32,
    current_y: f32,
    gap: f32,
}

impl ColumnLayout {
    pub fn new(gap: f32) -> Self {
        Self {
            left: 0.0,
            top: 0.0,
            width: 0.0,
            current_y: 0.0,
            gap,
        }
    }
}

impl LayoutStrategy for ColumnLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        self.left = left;
        self.top = top;
        self.width = width;
        self.current_y = top;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        let x = self.left;
        let y = self.current_y;
        self.current_y += wh + self.gap;
        (x, y, ww, wh)
    }

    fn get_column_width(&self) -> Option<f32> {
        Some(self.width)
    }
}

pub struct GridLayout {
    grid: Option<Grid>,
    min_col_width: f32,
    gap: f32,
    num_sections: Option<usize>,
}

impl GridLayout {
    pub fn new(min_col_width: f32, gap: f32) -> Self {
        Self {
            grid: None,
            min_col_width,
            gap,
            num_sections: None,
        }
    }
}

impl LayoutStrategy for GridLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, _height: f32) {
        let min_col_width = crate::layout::grid_min_col_width();
        let max_cols = ((width + self.gap) / (min_col_width + self.gap)).floor().max(1.0) as usize;
        let count = if let Some(n) = self.num_sections {
            n.min(max_cols).max(1)
        } else {
            max_cols
        };
        self.grid = Some(Grid::new(left, top, width, min_col_width, self.gap, count));
    }

    fn allocate(&mut self, _ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref mut grid) = self.grid {
            let col = grid.next_column();
            let x = grid.col_lefts[col];
            let y = grid.col_heights[col];
            grid.col_heights[col] += wh + grid.gap;
            (x, y, grid.col_width, wh)
        } else {
            (0.0, 0.0, 0.0, wh)
        }
    }

    fn set_section_count(&mut self, count: usize) {
        self.num_sections = Some(count);
    }

    fn get_column_width(&self) -> Option<f32> {
        self.grid.as_ref().map(|g| g.col_width)
    }
}

pub struct RadialLayout {
    radial: Option<Radial>,
    aspect_ratio: f32,
    base_spacing: f32,
    idx: usize,
}

impl RadialLayout {
    pub fn new(aspect_ratio: f32, base_spacing: f32) -> Self {
        Self {
            radial: None,
            aspect_ratio,
            base_spacing,
            idx: 0,
        }
    }
}

impl LayoutStrategy for RadialLayout {
    fn init(&mut self, left: f32, top: f32, width: f32, height: f32) {
        let cx = left + width / 2.0;
        let cy = top + height / 2.0;
        let aspect = if self.aspect_ratio > 0.0 {
            self.aspect_ratio
        } else {
            let screen_aspect = (width / height.max(1.0)).max(0.1);
            1.0 + (screen_aspect - 1.0) * 0.4
        };
        self.radial = Some(Radial::new(cx, cy, aspect, self.base_spacing));
        self.idx = 0;
    }

    fn allocate(&mut self, ww: f32, wh: f32) -> (f32, f32, f32, f32) {
        if let Some(ref radial) = self.radial {
            let rect = radial.widget_rect(self.idx, ww, wh);
            self.idx += 1;
            rect
        } else {
            (0.0, 0.0, ww, wh)
        }
    }
}

pub struct PageLayoutBuilder<'a, P> {
    pub strategy: &'a mut dyn LayoutStrategy,
    pub cx: f32,
    pub cy: f32,
    pub cw: f32,
    pub ch: f32,
    pub section_width: f32,
    pub idx: usize,
    _phantom: std::marker::PhantomData<P>,
}

impl<'a, P: RenderTarget + Default> PageLayoutBuilder<'a, P> {
    pub fn new(
        strategy: &'a mut dyn LayoutStrategy,
        cx: f32,
        cy: f32,
        cw: f32,
        ch: f32,
        section_width: f32,
    ) -> Self {
        strategy.init(cx, cy, cw, ch);
        Self {
            strategy,
            cx,
            cy,
            cw,
            ch,
            section_width,
            idx: 0,
            _phantom: std::marker::PhantomData,
        }
    }

    pub fn with_section_count(mut self, count: usize) -> Self {
        self.strategy.set_section_count(count);
        self.strategy.init(self.cx, self.cy, self.cw, self.ch);
        self
    }

    pub fn add_section<F>(&mut self, final_pc: &mut P, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let width = self.strategy.get_column_width().unwrap_or(self.section_width);
        let mut dummy = P::default();
        let mut dummy_ctx = SectionContext::new(&mut dummy, 0.0, 0.0, width, label, focused, false);
        render_fn(&mut dummy_ctx);
        let wh = dummy_ctx.finish();
        let (rx, ry, rw, _) = self.strategy.allocate(width, wh);
        let mut real_ctx = SectionContext::new(final_pc, rx, ry, rw, label, focused, false);
        render_fn(&mut real_ctx);
        real_ctx.finish();
        self.idx += 1;
    }

    pub fn add_section_with_width<F>(&mut self, final_pc: &mut P, width: f32, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let mut dummy = P::default();
        let mut dummy_ctx = SectionContext::new(&mut dummy, 0.0, 0.0, width, label, focused, false);
        render_fn(&mut dummy_ctx);
        let wh = dummy_ctx.finish();
        let (rx, ry, rw, _) = self.strategy.allocate(width, wh);
        let mut real_ctx = SectionContext::new(final_pc, rx, ry, rw, label, focused, false);
        render_fn(&mut real_ctx);
        real_ctx.finish();
        self.idx += 1;
    }
}

pub struct SectionContext<'a, P> {
    pub pc: &'a mut P,
    pub left: f32,
    pub top: f32,
    pub content_y: f32,
    pub cw: f32,
    pub label_width: f32,
    pub focused: bool,
    pub is_child: bool,
}

impl<'a, P: RenderTarget> SectionContext<'a, P> {
    pub const DEFAULT_MARGIN_X: f32 = 12.0;
    pub const DEFAULT_ROW_GAP: f32 = 8.0;

    fn estimate_label_width(label: &str, font_size: f32, font_fam: &str) -> f32 {
        estimate_label_width_helper(label, font_size, font_fam)
    }

    pub fn padding(&self) -> f32 {
        if self.is_child {
            section_padding().max(8.0)
        } else {
            section_padding()
        }
    }

    pub fn new(pc: &'a mut P, left: f32, top: f32, cw: f32, label: &str, focused: bool, is_child: bool) -> Self {
        let font_setting = if is_child {
            nested_section_label_font()
        } else {
            section_label_font()
        };
        let (font_fam, font_size_opt) = parse_font_string(&font_setting);
        let font_size = font_size_opt.unwrap_or(if is_child { 12.0 } else { 14.0 });
        let font_color = if is_child { [0.53, 0.53, 0.60, 1.0] } else { [0.83, 0.83, 0.83, 1.0] };
        let label_width = Self::estimate_label_width(label, font_size, &font_fam);
        let label_x = if is_child {
            let base_x = match nested_section_label_alignment() {
                0 => left + 12.0,
                1 => left + (cw - label_width) / 2.0,
                2 => left + cw - 12.0 - label_width,
                _ => left + 12.0,
            };
            base_x + nested_section_label_offset()
        } else {
            left + (cw - label_width) / 2.0
        };
        pc.text_with_font(label, label_x, top, font_size, font_color, &font_fam);
        Self {
            pc,
            left,
            top,
            content_y: top + font_size + 5.0,
            cw,
            label_width,
            focused,
            is_child,
        }
    }

    pub fn ax(&self, x_off: f32) -> f32 {
        let shift = if x_off >= 12.0 { self.padding() } else { 0.0 };
        self.left + x_off + shift
    }

    pub fn ay(&self) -> f32 {
        self.content_y
    }

    pub fn spacing(&mut self, dy: f32) {
        self.content_y += dy;
    }

    pub fn text(&mut self, text: &str, x_off: f32, y_off: f32, font_size: f32, color: [f32; 4]) {
        self.pc.text(text, self.ax(x_off), self.ay() + y_off, font_size, color);
    }

    pub fn widget<T: Element + 'static>(&mut self, w: &mut T, x_off: f32, ww: f32, mut wh: f32, ctx: &mut UiContext) {
        if let Some(pref) = w.preferred_height() {
            wh = pref;
        }
        let pad = self.padding();
        w.set_row_rect(self.left + pad, self.cw - 2.0 * pad);
        let x = self.ax(x_off);
        let y = self.ay();
        let right_edge = self.left + self.cw - pad;
        let clamped_w = ww.min((right_edge - x).max(0.0));
        let top_room = crate::widget::label_offset(w);
        let total_h = wh + top_room;
        render_widget(self.pc, w, x, y, clamped_w, total_h, ctx);
        self.content_y += total_h;
    }

    pub fn widget_full<T: Element + 'static>(&mut self, w: &mut T, wh: f32, ctx: &mut UiContext) {
        let x_off = 12.0;
        let ww = self.cw - 2.0 * (self.padding() + x_off); // cw - 40.0
        self.widget(w, x_off, ww, wh, ctx);
    }

    pub fn separator(&mut self) {
        let pad = self.padding();
        let x = self.ax(pad);
        let y = self.ay();
        self.pc.rect([0.18, 0.18, 0.27, 1.0], x, y, self.cw - 2.0 * pad, 1.0);
        self.content_y += 8.0;
    }

    pub fn rect(&mut self, color: [f32; 4], x_off: f32, w: f32, h: f32) {
        self.pc.rect(color, self.ax(x_off), self.ay(), w, h);
        self.content_y += h;
    }

    pub fn row_layout(&self, count: usize, gap: f32) -> Vec<(f32, f32)> {
        let margin_x = self.padding() + 12.0;
        let usable_w = self.cw - 2.0 * margin_x;
        if count == 0 {
            return Vec::new();
        }
        let total_gap = gap * (count - 1) as f32;
        let col_w = (usable_w - total_gap).max(0.0) / count as f32;

        let mut cols = Vec::with_capacity(count);
        for i in 0..count {
            let x = self.left + margin_x + i as f32 * (col_w + gap);
            cols.push((x, col_w));
        }
        cols
    }

    pub fn row<F>(&mut self, count: usize, gap: f32, h: f32, mut f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        let cols = self.row_layout(count, gap);
        for (i, &(x, w)) in cols.iter().enumerate() {
            f(i, x, w);
        }
        self.content_y += h;
    }

    pub fn vstack(&mut self, spacing: f32) -> VStack<'_, 'a, P> {
        VStack {
            context: self,
            spacing,
        }
    }

    pub fn add_section<F>(&mut self, label: &str, focused: bool, mut render_fn: F)
    where
        F: FnMut(&mut SectionContext<'_, P>),
    {
        let pad = self.padding();
        let left = self.ax(0.0) + pad;
        let top = self.content_y;
        let cw = self.cw - 2.0 * pad;

        let mut sub_ctx = SectionContext::new(self.pc, left, top, cw, label, focused, true);
        render_fn(&mut sub_ctx);
        self.content_y = sub_ctx.finish();
    }

    pub fn finish(self) -> f32 {
        let border: [f32; 4] = if self.is_child {
            if self.focused {
                [0.22, 0.38, 0.24, 1.0]
            } else {
                [0.18, 0.18, 0.25, 1.0]
            }
        } else {
            if self.focused {
                [0.30, 0.50, 0.32, 1.0] // Focused green
            } else {
                [0.25, 0.25, 0.35, 1.0] // Default gray
            }
        };
        let pad = self.padding();
        let x = self.left + pad;
        let y = self.top + 7.0;
        let w = self.cw - 2.0 * pad;
        let h = self.content_y - y;

        let left_edge = x;
        let right_edge = x + w;
        if self.label_width > 0.0 {
            let label_x = if self.is_child {
                let base_x = match nested_section_label_alignment() {
                    0 => self.left + 12.0,
                    1 => self.left + (self.cw - self.label_width) / 2.0,
                    2 => self.left + self.cw - 12.0 - self.label_width,
                    _ => self.left + 12.0,
                };
                base_x + nested_section_label_offset()
            } else {
                self.left + (self.cw - self.label_width) / 2.0
            };
            let gap_margin = 6.0;
            let gap_start = label_x - gap_margin;
            let gap_end = label_x + self.label_width + gap_margin;
            if gap_start > left_edge {
                self.pc.rect(border, left_edge, y, gap_start - left_edge, 1.0);
            }
            if right_edge > gap_end {
                self.pc.rect(border, gap_end, y, right_edge - gap_end, 1.0);
            }
        } else {
            self.pc.rect(border, left_edge, y, w, 1.0);
        }

        self.pc.rect(border, x, y + h + 12.0, w, 1.0);
        self.pc.rect(border, x, y, 1.0, h + 12.0);
        self.pc.rect(border, x + w - 1.0, y, 1.0, h + 12.0);
        self.content_y + 20.0
    }
}


pub struct VStack<'b, 'a, P> {
    context: &'b mut SectionContext<'a, P>,
    spacing: f32,
}

impl<'b, 'a, P: RenderTarget> VStack<'b, 'a, P> {
    pub fn add_widget<T: Element + 'static>(&mut self, w: &mut T, ww: f32, wh: f32, ctx: &mut UiContext) {
        self.context.widget(w, SectionContext::<P>::DEFAULT_MARGIN_X, ww, wh, ctx);
        self.context.spacing(self.spacing);
    }

    pub fn add_row<F>(&mut self, count: usize, gap: f32, h: f32, f: F)
    where
        F: FnMut(usize, f32, f32),
    {
        self.context.row(count, gap, h, f);
        self.context.spacing(self.spacing);
    }
}

pub fn get_system_monospace_font() -> &'static str {
    static MONOSPACE_FONT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    MONOSPACE_FONT.get_or_init(|| {
        if let Ok(output) = std::process::Command::new("fc-match")
            .args(["-f", "%{family}", "monospace"])
            .output()
        {
            let name = String::from_utf8_lossy(&output.stdout);
            let parsed = name.split(',').next().unwrap_or("monospace").trim();
            if !parsed.is_empty() {
                return parsed.to_string();
            }
        }
        "monospace".to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockRenderTarget {
        rects: Vec<([f32; 4], f32, f32, f32, f32)>,
    }

    impl RenderTarget for MockRenderTarget {
        fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
            self.rects.push((color, x, y, w, h));
        }
        fn text(&mut self, _content: &str, _x: f32, _y: f32, _size: f32, _color: [f32; 4]) {}
    }

    struct MockWidget {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    }

    impl Element for MockWidget {
        fn rect(&self) -> (f32, f32, f32, f32) {
            (self.x, self.y, self.w, self.h)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            self.x = x;
            self.y = y;
            self.w = w;
            self.h = h;
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    struct MockWidgetWithLabel {
        base: crate::widget::Widget,
    }

    impl Element for MockWidgetWithLabel {
        fn base(&self) -> Option<&crate::widget::Widget> {
            Some(&self.base)
        }
        fn base_mut(&mut self) -> Option<&mut crate::widget::Widget> {
            Some(&mut self.base)
        }
        fn rect(&self) -> (f32, f32, f32, f32) {
            let offset = crate::widget::label_offset(self);
            (self.base.x, self.base.y - offset, self.base.w, self.base.h + offset)
        }
        fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
            let offset = crate::widget::label_offset(self);
            self.base.x = x;
            self.base.y = y + offset;
            self.base.w = w;
            self.base.h = (h - offset).max(0.0);
        }
        fn color(&self) -> [f32; 4] {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    #[test]
    fn test_vstack_flow() {
        let orig_margin = label_margin();
        set_label_margin(6.0);
        let _ = section_padding();
        set_section_padding(8.0);
        let mut mock_pc = MockRenderTarget { rects: Vec::new() };
        let mut sec = Section::new(&mut mock_pc, 10.0, 20.0, 200.0, "Test Section");
        
        let start_y = sec.ay();
        let mut stack = sec.vstack(&mut mock_pc, 10.0);

        let mut dummy = crate::context::UiContext::new();
        let mut w1 = MockWidget { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w1, 50.0, 30.0, &mut dummy);

        // Standard margin should be applied
        assert_eq!(w1.x, 30.0);
        assert_eq!(w1.y, start_y);

        let mut w2 = MockWidget { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        stack.add_widget(&mut w2, 60.0, 40.0, &mut dummy);

        // Second widget should start after first widget height + vstack spacing
        assert_eq!(w2.y, start_y + 30.0 + 10.0);

        let mut base = crate::widget::Widget::new();
        base.label = Some("Test Label".to_string());
        let mut w3 = MockWidgetWithLabel { base };
        stack.add_widget(&mut w3, 70.0, 50.0, &mut dummy);

        // Third widget has label, so its y should be shifted by 18.0
        assert_eq!(w3.base.y, start_y + 30.0 + 10.0 + 40.0 + 10.0 + 18.0);
        set_label_margin(orig_margin);
    }

    #[test]
    fn test_grid_layout() {
        // Test single column layout (width = 200, min_col_width = 300)
        let grid1 = Grid::new(10.0, 20.0, 200.0, 300.0, 10.0, 1);
        assert_eq!(grid1.col_heights.len(), 1);
        assert_eq!(grid1.col_lefts[0], 10.0);
        assert_eq!(grid1.col_width, 200.0);

        // Test multi column layout (width = 700, min_col_width = 300, gap = 20)
        // count = floor((700 + 20) / (300 + 20)) = floor(720 / 320) = 2.
        // total_gap = 20 * 1 = 20.
        // col_width = (700 - 20) / 2 = 340.
        let mut grid2 = Grid::new(5.0, 15.0, 700.0, 300.0, 20.0, 2);
        assert_eq!(grid2.col_heights.len(), 2);
        assert_eq!(grid2.col_lefts[0], 5.0);
        assert_eq!(grid2.col_lefts[1], 365.0);
        assert_eq!(grid2.col_width, 340.0);

        assert_eq!(grid2.next_column(), 0);
        grid2.col_heights[0] += 50.0; // Column 0 height becomes 65.0
        assert_eq!(grid2.next_column(), 1);
        grid2.col_heights[1] += 30.0; // Column 1 height becomes 45.0
        assert_eq!(grid2.next_column(), 1);
        grid2.col_heights[1] += 30.0; // Column 1 height becomes 75.0
        assert_eq!(grid2.next_column(), 0);
        
        assert_eq!(grid2.max_height(), 75.0);
    }

    #[test]
    fn test_subsection() {
        let mut pc = PopoverCollector::new();
        let mut subsec = Section::new_opt(&mut pc, 10.0, 20.0, 300.0, "Test Subsec", true);
        assert_eq!(subsec.left, 10.0);
        assert_eq!(subsec.top, 20.0);
        assert_eq!(subsec.cw, 300.0);
        assert!(subsec.is_child);
        
        let mut dummy = crate::context::UiContext::new();
        let mut w = MockWidget { x: 0.0, y: 0.0, w: 0.0, h: 0.0 };
        subsec.widget(&mut pc, &mut w, 12.0, 100.0, 40.0, &mut dummy);
        
        let bottom = subsec.finish(&mut pc);
        assert!(bottom > 20.0);
    }

    #[test]
    fn test_radial_layout() {
        let radial = Radial::new(100.0, 100.0, 1.5, 50.0);
        
        // Check first widget is centered at (100.0, 100.0)
        let rect0 = radial.widget_rect(0, 40.0, 30.0);
        assert_eq!(rect0, (100.0 - 20.0, 100.0 - 15.0, 40.0, 30.0));
        
        // Check Ring 1 (idx = 1) vs Ring 2 (idx = 7)
        let rect1 = radial.widget_rect(1, 40.0, 30.0);
        let rect7 = radial.widget_rect(7, 40.0, 30.0);
        
        let c1_x = rect1.0 + rect1.2 / 2.0;
        let c1_y = rect1.1 + rect1.3 / 2.0;
        let c7_x = rect7.0 + rect7.2 / 2.0;
        let c7_y = rect7.1 + rect7.3 / 2.0;
        
        let d1 = ((c1_x - 100.0).powi(2) + (c1_y - 100.0).powi(2)).sqrt();
        let d7 = ((c7_x - 100.0).powi(2) + (c7_y - 100.0).powi(2)).sqrt();
        
        // Ring 2 should be further out than Ring 1
        assert!(d7 > d1);
        assert!(d1 > 0.0);
    }

    #[test]
    fn test_nested_section_label_alignment() {
        let _ = nested_section_label_alignment();
        set_nested_section_label_alignment(1);
        assert_eq!(nested_section_label_alignment(), 1);
        set_nested_section_label_alignment(2);
        assert_eq!(nested_section_label_alignment(), 2);
        set_nested_section_label_alignment(0);
        assert_eq!(nested_section_label_alignment(), 0);

        let _ = nested_section_label_offset();
        set_nested_section_label_offset(15.0);
        assert_eq!(nested_section_label_offset(), 15.0);
        set_nested_section_label_offset(0.0);
        assert_eq!(nested_section_label_offset(), 0.0);
    }

    #[test]
    fn test_dropdown_height() {
        let _ = dropdown_height();
        set_dropdown_height(48.0);
        assert_eq!(dropdown_height(), 48.0);
        set_dropdown_height(44.0);
        assert_eq!(dropdown_height(), 44.0);
    }

    #[test]
    fn test_print_fonts() {
        let db = crate::widget::get_font_db();
        for face in db.faces() {
            println!("FAMILY: {:?}", face.families);
        }
    }
}

