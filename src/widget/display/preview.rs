use std::path::PathBuf;
use crate::widget::{Widget, Element};
use crate::context::UiContext;
use crate::layout::{RenderTarget, SectionContext};
use crate::color;

pub struct PreviewState {
    pub base: Widget,
    pub visible: bool,
    pub path: Option<PathBuf>,
    pub path_display: String,
    pub name: String,
    pub is_dir: bool,
    pub size: String,
    pub permissions: String,
    pub modified: String,
    pub file_type: String,
    pub target: String, // for symlinks
    pub content_preview: Option<String>,
    pub scroll_line: usize,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            base: Widget::new(),
            visible: true,
            path: None,
            path_display: String::new(),
            name: String::new(),
            is_dir: false,
            size: String::new(),
            permissions: String::new(),
            modified: String::new(),
            file_type: String::new(),
            target: String::new(),
            content_preview: None,
            scroll_line: 0,
        }
    }
}

impl std::fmt::Debug for PreviewState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PreviewState")
            .field("base", &self.base)
            .field("visible", &self.visible)
            .field("path", &self.path)
            .field("path_display", &self.path_display)
            .field("name", &self.name)
            .field("is_dir", &self.is_dir)
            .field("size", &self.size)
            .field("permissions", &self.permissions)
            .field("modified", &self.modified)
            .field("file_type", &self.file_type)
            .field("target", &self.target)
            .field("content_preview", &self.content_preview)
            .field("scroll_line", &self.scroll_line)
            .finish()
    }
}

impl Clone for PreviewState {
    fn clone(&self) -> Self {
        Self {
            base: self.base.clone(),
            visible: self.visible,
            path: self.path.clone(),
            path_display: self.path_display.clone(),
            name: self.name.clone(),
            is_dir: self.is_dir,
            size: self.size.clone(),
            permissions: self.permissions.clone(),
            modified: self.modified.clone(),
            file_type: self.file_type.clone(),
            target: self.target.clone(),
            content_preview: self.content_preview.clone(),
            scroll_line: self.scroll_line,
        }
    }
}

impl PreviewState {
    pub fn handle_mouse_wheel(&mut self, delta: &crate::widget::MouseScrollDelta, ch: f32) -> bool {
        let content = match &self.content_preview {
            Some(c) => c,
            None => return false,
        };
        let total_lines = content.lines().count();
        let half_h = ch * 0.5;
        let mut max_visible_lines = 0;
        let mut text_y = 44.0;
        while text_y + 14.0 <= half_h - 16.0 {
            max_visible_lines += 1;
            text_y += 15.0;
        }
        if total_lines <= max_visible_lines {
            if self.scroll_line != 0 {
                self.scroll_line = 0;
                return true;
            }
            return false;
        }
        let max_scroll = total_lines.saturating_sub(max_visible_lines);
        let scroll_speed = 3.0;
        let diff = match delta {
            crate::widget::MouseScrollDelta::LineDelta(_, y) => {
                -y * scroll_speed
            }
            crate::widget::MouseScrollDelta::PixelDelta(pos) => {
                -pos.y as f32 / 15.0
            }
        };
        let prev_scroll = self.scroll_line;
        let new_scroll = (self.scroll_line as f32 + diff).round() as isize;
        self.scroll_line = new_scroll.clamp(0, max_scroll as isize) as usize;
        self.scroll_line != prev_scroll
    }
}

// Simple canvas struct that accumulates rendering primitives
struct WidgetCanvas {
    quads: Vec<(f32, f32, f32, f32, [f32; 4])>,
    labels: Vec<(crate::widget::display::TextLabel, Option<String>, Option<[f32; 4]>)>,
}

impl WidgetCanvas {
    fn new() -> Self {
        Self {
            quads: Vec::new(),
            labels: Vec::new(),
        }
    }
}

impl RenderTarget for WidgetCanvas {
    fn rect(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32) {
        self.quads.push((x, y, w, h, color));
    }
    fn rect_with_radius(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, _radius: f32) {
        self.quads.push((x, y, w, h, color));
    }
    fn rect_with_radius_corners(&mut self, color: [f32; 4], x: f32, y: f32, w: f32, h: f32, _radius: f32, _corners: (bool, bool, bool, bool)) {
        self.quads.push((x, y, w, h, color));
    }
    fn text(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4]) {
        let label_color = [
            (color[0] * 255.0).round() as u8,
            (color[1] * 255.0).round() as u8,
            (color[2] * 255.0).round() as u8,
        ];
        self.labels.push((
            crate::widget::display::TextLabel {
                text: content.to_string(),
                x,
                y,
                font_size: size,
                color: label_color,
            },
            None,
            None,
        ));
    }
    fn text_with_font(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str) {
        let label_color = [
            (color[0] * 255.0).round() as u8,
            (color[1] * 255.0).round() as u8,
            (color[2] * 255.0).round() as u8,
        ];
        self.labels.push((
            crate::widget::display::TextLabel {
                text: content.to_string(),
                x,
                y,
                font_size: size,
                color: label_color,
            },
            Some(font.to_string()),
            None,
        ));
    }
    fn text_with_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], bounds: Option<[f32; 4]>) {
        let label_color = [
            (color[0] * 255.0).round() as u8,
            (color[1] * 255.0).round() as u8,
            (color[2] * 255.0).round() as u8,
        ];
        self.labels.push((
            crate::widget::display::TextLabel {
                text: content.to_string(),
                x,
                y,
                font_size: size,
                color: label_color,
            },
            None,
            bounds,
        ));
    }
    fn text_with_font_and_bounds(&mut self, content: &str, x: f32, y: f32, size: f32, color: [f32; 4], font: &str, bounds: Option<[f32; 4]>) {
        let label_color = [
            (color[0] * 255.0).round() as u8,
            (color[1] * 255.0).round() as u8,
            (color[2] * 255.0).round() as u8,
        ];
        self.labels.push((
            crate::widget::display::TextLabel {
                text: content.to_string(),
                x,
                y,
                font_size: size,
                color: label_color,
            },
            Some(font.to_string()),
            bounds,
        ));
    }
}

impl PreviewState {
    fn render_to_canvas(&self) -> WidgetCanvas {
        let mut canvas = WidgetCanvas::new();
        
        let cx = self.base.x;
        let cy = self.base.y;
        let cw = self.base.w;
        let ch = self.base.h;

        let text_fg = color::TEXT_FG;
        let text_dim = color::TEXT_DIM;
        let label_fg = color::TEXT_ACCENT;

        if self.path.is_none() {
            canvas.text("Select a file to view details", cx + 12.0, cy + 12.0, 13.0, text_dim);
            return canvas;
        }

        let half_h = ch * 0.5;

        // 1. Top pane: File Preview Section
        let mut preview_sec = SectionContext::new(&mut canvas, cx + 4.0, cy + 12.0, cw - 8.0, "Preview", false, false);
        preview_sec.content_y = cy + half_h - 20.0;
        preview_sec.finish();

        let bg_color = color::scrollinglist_bg_color();
        canvas.rect(bg_color, cx + 12.0, cy + 32.0, cw - 24.0, half_h - 40.0);

        if let Some(content) = &self.content_preview {
            let mut text_y = cy + 44.0;
            for line in content.lines().skip(self.scroll_line) {
                if text_y + 14.0 > cy + half_h - 16.0 {
                    break;
                }
                let limit = (((cw - 40.0) / 6.8).floor() as usize).max(20);
                let line_truncated = if line.chars().count() > limit {
                    let mut s: String = line.chars().take(limit - 3).collect();
                    s.push_str("...");
                    s
                } else {
                    line.to_string()
                };
                canvas.text_with_font(&line_truncated, cx + 20.0, text_y, 11.0, text_fg, "monospace");
                text_y += 15.0;
            }
        } else {
            canvas.text("No preview available", cx + 20.0, cy + 44.0, 11.0, text_dim);
        }

        // 2. Bottom pane: Details Section
        let bottom_y = cy + half_h + 12.0;
        let icon = if self.is_dir { "📁" } else { "📄" };

        let details = [
            ("Path", &self.path_display),
            ("Type", &self.file_type),
            ("Size", &self.size),
            ("Permissions", &self.permissions),
            ("Modified", &self.modified),
        ];

        let details_content_start_y = bottom_y + 19.0;
        let mut details_content_end_y = details_content_start_y + 36.0 + details.len() as f32 * 20.0;
        if !self.target.is_empty() {
            details_content_end_y += 24.0;
        }

        let mut details_sec = SectionContext::new(&mut canvas, cx + 4.0, bottom_y, cw - 8.0, "Details", false, false);
        details_sec.content_y = details_content_end_y;
        details_sec.finish();

        let header_y = details_content_start_y + 6.0;
        canvas.text(icon, cx + 12.0, header_y, 20.0, text_fg);
        
        let name_truncated = if self.name.len() > 30 {
            format!("{}...", &self.name[..27])
        } else {
            self.name.clone()
        };
        canvas.text(&name_truncated, cx + 42.0, header_y + 4.0, 16.0, text_fg);

        let mut y = details_content_start_y + 36.0;
        for (label, val) in &details {
            canvas.text(label, cx + 12.0, y, 12.0, label_fg);
            
            let val_str = if val.len() > 40 {
                format!("...{}", &val[val.len() - 37..])
            } else {
                val.to_string()
            };
            canvas.text(&val_str, cx + 112.0, y, 12.0, text_dim);
            y += 20.0;
        }

        if !self.target.is_empty() {
            y += 8.0;
            canvas.text("Target", cx + 12.0, y, 12.0, label_fg);
            
            let target_str = if self.target.len() > 40 {
                format!("...{}", &self.target[self.target.len() - 37..])
            } else {
                self.target.clone()
            };
            canvas.text(&target_str, cx + 112.0, y, 12.0, text_dim);
        }

        canvas
    }
}

impl Element for PreviewState {
    crate::impl_widget_base!(PreviewState);

    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn visible(&self) -> bool {
        self.visible
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    fn all_quads(&self, _ctx: &UiContext) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        if !self.visible {
            return Vec::new();
        }
        self.render_to_canvas().quads
    }

    fn text_labels_with_font_and_bounds(&self, _ctx: &UiContext) -> Vec<(crate::widget::display::TextLabel, Option<String>, Option<[f32; 4]>)> {
        if !self.visible {
            return Vec::new();
        }
        self.render_to_canvas().labels
    }
}
