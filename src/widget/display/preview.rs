//! Narrow-trait `PreviewState` (Phase 5t) — cce-files' file-preview pane: a procedural render
//! of a preview section (text lines in monospace, RLE-drawn image pixels) over a details
//! section, produced by an internal `WidgetCanvas: RenderTarget`. The canvas labels carry
//! per-label fonts (content lines are monospace, metadata is default-font), which is exactly
//! the [`Paint::serves_legacy_labels`] hatch from Phase 5s; the quads flow from
//! [`Paint::paint`]. Plain `text_labels` stays EMPTY like legacy (the pane's text is served
//! only through the font-and-bounds getter — emitting it as prims too would double-render
//! under container aggregation). The app owns all the data fields and mutates them through
//! `Deref`; scrolling is the inherent [`PreviewState::handle_mouse_wheel`], driven by hand.

use std::path::PathBuf;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Input, Layout, Paint, UiContext};
use crate::layout::{RenderTarget, SectionContext};
use crate::color;

#[derive(Debug, Clone, Default)]
pub struct ImagePreviewData {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<[u8; 4]>,
}

#[derive(Debug, Clone)]
pub struct PreviewState {
    rect: Rect,
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
    pub image_preview: Option<ImagePreviewData>,
    pub scroll_line: usize,
}

impl Default for PreviewState {
    fn default() -> Self {
        Self {
            rect: Rect { x: 0.0, y: 0.0, width: 0.0, height: 0.0 },
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
            image_preview: None,
            scroll_line: 0,
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

        let cx = self.rect.x;
        let cy = self.rect.y;
        let cw = self.rect.width;
        let ch = self.rect.height;

        let text_fg = color::TEXT_FG;
        let text_dim = color::TEXT_DIM;
        let label_fg = color::TEXT_ACCENT;

        if self.path.is_none() {
            canvas.text("Select a file to view details", cx + 12.0, cy + 12.0, 13.0, text_dim);
            return canvas;
        }

        let half_h = ch * 0.5;
        let pad = crate::layout::section_padding();

        // 1. Top pane: File Preview Section
        let mut preview_sec = SectionContext::new(&mut canvas, cx + 4.0, cy + 12.0, cw - 8.0, "Preview", false, false);
        let rect_y = cy + pad + 31.0;
        let rect_h = half_h - 2.0 * pad - 43.0;
        preview_sec.content_y = cy + half_h - pad - 12.0;
        preview_sec.finish();

        let bg_color = color::list_bg_color();
        canvas.rect(bg_color, cx + 12.0, rect_y, cw - 24.0, rect_h);

        if let Some(image_data) = &self.image_preview {
            let box_w = cw - 24.0;
            let box_h = rect_h;
            let img_w = image_data.width as f32;
            let img_h = image_data.height as f32;

            let scale_x = box_w / img_w;
            let scale_y = box_h / img_h;
            let scale = scale_x.min(scale_y).min(4.0).max(1.0);

            let draw_w = img_w * scale;
            let draw_h = img_h * scale;

            let start_x = cx + 12.0 + (box_w - draw_w) * 0.5;
            let start_y = rect_y + (box_h - draw_h) * 0.5;

            for row in 0..image_data.height {
                let mut col = 0;
                while col < image_data.width {
                    let idx = (row * image_data.width + col) as usize;
                    if idx >= image_data.pixels.len() {
                        break;
                    }
                    let pixel = image_data.pixels[idx];
                    let r = pixel[0];
                    let g = pixel[1];
                    let b = pixel[2];
                    let a = pixel[3];

                    let mut run_len = 1;
                    while col + run_len < image_data.width {
                        let next_idx = (row * image_data.width + col + run_len) as usize;
                        if next_idx >= image_data.pixels.len() {
                            break;
                        }
                        if image_data.pixels[next_idx] == pixel {
                            run_len += 1;
                        } else {
                            break;
                        }
                    }

                    let alpha = a as f32 / 255.0;
                    if alpha > 0.0 {
                        let rf = r as f32 / 255.0;
                        let gf = g as f32 / 255.0;
                        let bf = b as f32 / 255.0;

                        canvas.rect(
                            [rf, gf, bf, alpha],
                            start_x + col as f32 * scale,
                            start_y + row as f32 * scale,
                            run_len as f32 * scale,
                            scale,
                        );
                    }

                    col += run_len;
                }
            }
        } else if let Some(content) = &self.content_preview {
            let mut text_y = rect_y + 12.0;
            for line in content.lines().skip(self.scroll_line) {
                if text_y + 14.0 > rect_y + rect_h - 8.0 {
                    break;
                }
                let limit = (((cw - 40.0) / 6.8).floor() as usize).max(20);
                let line_truncated = crate::widget::display::truncate_tail(line, limit);
                canvas.text_with_font(&line_truncated, cx + 20.0, text_y, 11.0, text_fg, "monospace");
                text_y += 15.0;
            }
        } else {
            canvas.text("No preview available", cx + 20.0, rect_y + 12.0, 11.0, text_dim);
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

        let details_content_start_y = bottom_y + pad + 19.0;
        let mut details_content_end_y = details_content_start_y + 36.0 + details.len() as f32 * 20.0;
        if !self.target.is_empty() {
            details_content_end_y += 24.0;
        }

        let mut details_sec = SectionContext::new(&mut canvas, cx + 4.0, bottom_y, cw - 8.0, "Details", false, false);
        details_sec.content_y = details_content_end_y;
        details_sec.finish();

        let header_y = details_content_start_y + 6.0;
        canvas.text(icon, cx + 12.0, header_y, 20.0, text_fg);

        let name_truncated = crate::widget::display::truncate_tail(&self.name, 30);
        canvas.text(&name_truncated, cx + 42.0, header_y + 4.0, 16.0, text_fg);

        let mut y = details_content_start_y + 36.0;
        for (label, val) in &details {
            canvas.text(label, cx + 12.0, y, 12.0, label_fg);

            let val_str = crate::widget::display::truncate_head(val, 40);
            canvas.text(&val_str, cx + 112.0, y, 12.0, text_dim);
            y += 20.0;
        }

        if !self.target.is_empty() {
            y += 8.0;
            canvas.text("Target", cx + 12.0, y, 12.0, label_fg);

            let target_str = crate::widget::display::truncate_head(&self.target, 40);
            canvas.text(&target_str, cx + 112.0, y, 12.0, text_dim);
        }

        canvas
    }
}

impl Layout for PreviewState {
    fn rect_assigned(&mut self, rect: Rect) {
        self.rect = rect;
    }
}

impl Paint for PreviewState {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Quads only: the canvas text carries per-label fonts and is served exclusively through
    /// the labels hatch — legacy's plain `text_labels` was empty, and the scene path (which
    /// drained it) accordingly showed no text either.
    fn paint(&self, _rect: Rect, ctx: &mut PaintCtx) {
        for (qx, qy, qw, qh, qc) in self.render_to_canvas().quads {
            ctx.quad(Rect { x: qx, y: qy, width: qw, height: qh }, qc);
        }
    }

    fn serves_legacy_labels(&self) -> bool {
        true
    }

    fn legacy_labels_with_font_and_bounds(&self, _rect: Rect, _ctx: &UiContext) -> Vec<(crate::widget::display::TextLabel, Option<String>, Option<[f32; 4]>)> {
        self.render_to_canvas().labels
    }
}

impl Input for PreviewState {}
