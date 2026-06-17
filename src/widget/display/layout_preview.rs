use crate::widget::*;
use crate::widget::display::TextLabel;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewLayoutMode {
    Fullscreen,
    Cascade,
    Stack,
    Grid,
    LeftTiled,
    RightTiled,
    Equal,
    Spiral,
    Floating,
}

struct SimNode {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    label: String,
}

#[derive(Debug, Clone)]
pub struct LayoutPreview {
    base: Widget,
    pub mode: PreviewLayoutMode,
    pub is_active: bool,
}

impl LayoutPreview {
    pub fn new(mode: PreviewLayoutMode) -> Self {
        Self {
            base: Widget::new(),
            mode,
            is_active: false,
        }
    }

    pub fn with_active(mut self, active: bool) -> Self {
        self.is_active = active;
        self
    }
    
    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }
}

impl Element for LayoutPreview {
    crate::impl_widget_base!(LayoutPreview);
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        let (x, y, w, h) = self.rect();
        let mut quads = Vec::new();

        let bg_col = if self.is_active { [0.12, 0.24, 0.14, 0.55] } else { [0.08, 0.08, 0.12, 0.35] };
        let border_col = if self.is_active { [0.36, 0.56, 0.38, 0.95] } else { [0.24, 0.24, 0.28, 0.45] };

        quads.push((x, y, w, h, bg_col));
        quads.push((x, y, w, 1.0, border_col));
        quads.push((x, y + h - 1.0, w, 1.0, border_col));
        quads.push((x, y, 1.0, h, border_col));
        quads.push((x + w - 1.0, y, 1.0, h, border_col));

        let preview_x = x + 8.0;
        let preview_y = y + 38.0;
        let preview_w = w - 16.0;
        let preview_h = h - 46.0;

        if preview_w > 0.0 && preview_h > 0.0 {
            quads.push((preview_x, preview_y, preview_w, preview_h, [0.16, 0.16, 0.20, 0.6]));
            let preview_border = [0.22, 0.22, 0.26, 0.8];
            quads.push((preview_x, preview_y, preview_w, 1.0, preview_border));
            quads.push((preview_x, preview_y + preview_h - 1.0, preview_w, 1.0, preview_border));
            quads.push((preview_x, preview_y, 1.0, preview_h, preview_border));
            quads.push((preview_x + preview_w - 1.0, preview_y, 1.0, preview_h, preview_border));

            let mut nodes = Vec::new();
            match self.mode {
                PreviewLayoutMode::Fullscreen => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "F".to_string() });
                }
                PreviewLayoutMode::Cascade => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 6.0, y: 6.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 10.0, y: 10.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Stack => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "Stack".to_string() });
                }
                PreviewLayoutMode::Grid => {
                    let hw = (preview_w - 6.0) / 2.0;
                    let hh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: hw, h: hh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 2.0, w: hw, h: hh, label: "2".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + hh, w: hw, h: hh, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 4.0 + hh, w: hw, h: hh, label: "4".to_string() });
                }
                PreviewLayoutMode::LeftTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                }
                PreviewLayoutMode::RightTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + sw, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                }
                PreviewLayoutMode::Equal => {
                    let ew = (preview_w - 8.0) / 3.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: ew, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 6.0 + 2.0 * ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Spiral => {
                    let w1 = (preview_w - 6.0) * 0.5;
                    let w2 = (preview_w - 6.0) - w1;
                    let h2 = (preview_h - 6.0) * 0.5;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: w1, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 2.0, w: w2, h: h2, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1 + w2 * 0.5, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "4".to_string() });
                }
                PreviewLayoutMode::Floating => {
                    nodes.push(SimNode { x: 4.0, y: 6.0, w: preview_w * 0.45, h: preview_h * 0.5, label: "1".to_string() });
                    nodes.push(SimNode { x: preview_w * 0.4, y: 12.0, w: preview_w * 0.5, h: preview_h * 0.45, label: "2".to_string() });
                    nodes.push(SimNode { x: 8.0, y: preview_h * 0.4, w: preview_w * 0.55, h: preview_h * 0.5, label: "3".to_string() });
                }
            }

            for node in nodes {
                let rect_x = preview_x + node.x;
                let rect_y = preview_y + node.y;
                let node_bg = if self.is_active { [0.30, 0.45, 0.65, 0.45] } else { [0.20, 0.24, 0.30, 0.25] };
                let node_border = if self.is_active { [0.45, 0.65, 0.90, 0.85] } else { [0.35, 0.40, 0.45, 0.55] };

                quads.push((rect_x, rect_y, node.w, node.h, node_bg));
                quads.push((rect_x, rect_y, node.w, 1.0, node_border));
                quads.push((rect_x, rect_y + node.h - 1.0, node.w, 1.0, node_border));
                quads.push((rect_x, rect_y, 1.0, node.h, node_border));
                quads.push((rect_x + node.w - 1.0, rect_y, 1.0, node.h, node_border));
            }
        }

        quads
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let (x, y, w, h) = self.rect();
        let mut labels = Vec::new();

        labels.push(TextLabel {
            text: self.base.label.clone().unwrap_or_else(|| "TAG".to_string()),
            x: x + 8.0,
            y: y + 8.0,
            font_size: 10.0,
            color: [140, 140, 153],
        });

        let layout_name = match self.mode {
            PreviewLayoutMode::Fullscreen => "Fullscreen",
            PreviewLayoutMode::Cascade => "Cascade",
            PreviewLayoutMode::Stack => "Stack",
            PreviewLayoutMode::Grid => "Grid",
            PreviewLayoutMode::LeftTiled => "L-Tiled",
            PreviewLayoutMode::RightTiled => "R-Tiled",
            PreviewLayoutMode::Equal => "Equal",
            PreviewLayoutMode::Spiral => "Spiral",
            PreviewLayoutMode::Floating => "Floating",
        };

        labels.push(TextLabel {
            text: layout_name.to_string(),
            x: x + 8.0,
            y: y + 20.0,
            font_size: 13.0,
            color: [230, 230, 242],
        });

        let preview_x = x + 8.0;
        let preview_y = y + 38.0;
        let preview_w = w - 16.0;
        let preview_h = h - 46.0;

        if preview_w > 0.0 && preview_h > 0.0 {
            let mut nodes = Vec::new();
            match self.mode {
                PreviewLayoutMode::Fullscreen => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "F".to_string() });
                }
                PreviewLayoutMode::Cascade => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 6.0, y: 6.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 10.0, y: 10.0, w: preview_w - 12.0, h: preview_h - 12.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Stack => {
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: preview_w - 4.0, h: preview_h - 4.0, label: "Stack".to_string() });
                }
                PreviewLayoutMode::Grid => {
                    let hw = (preview_w - 6.0) / 2.0;
                    let hh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: hw, h: hh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 2.0, w: hw, h: hh, label: "2".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + hh, w: hw, h: hh, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + hw, y: 4.0 + hh, w: hw, h: hh, label: "4".to_string() });
                }
                PreviewLayoutMode::LeftTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + mw, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                }
                PreviewLayoutMode::RightTiled => {
                    let mw = (preview_w - 6.0) * 0.55;
                    let sw = (preview_w - 6.0) - mw;
                    let sh = (preview_h - 6.0) / 2.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: sw, h: sh, label: "1".to_string() });
                    nodes.push(SimNode { x: 2.0, y: 4.0 + sh, w: sw, h: sh, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + sw, y: 2.0, w: mw, h: preview_h - 4.0, label: "M".to_string() });
                }
                PreviewLayoutMode::Equal => {
                    let ew = (preview_w - 8.0) / 3.0;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: ew, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "2".to_string() });
                    nodes.push(SimNode { x: 6.0 + 2.0 * ew, y: 2.0, w: ew, h: preview_h - 4.0, label: "3".to_string() });
                }
                PreviewLayoutMode::Spiral => {
                    let w1 = (preview_w - 6.0) * 0.5;
                    let w2 = (preview_w - 6.0) - w1;
                    let h2 = (preview_h - 6.0) * 0.5;
                    nodes.push(SimNode { x: 2.0, y: 2.0, w: w1, h: preview_h - 4.0, label: "1".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 2.0, w: w2, h: h2, label: "2".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "3".to_string() });
                    nodes.push(SimNode { x: 4.0 + w1 + w2 * 0.5, y: 4.0 + h2, w: w2 * 0.5, h: h2, label: "4".to_string() });
                }
                PreviewLayoutMode::Floating => {
                    nodes.push(SimNode { x: 4.0, y: 6.0, w: preview_w * 0.45, h: preview_h * 0.5, label: "1".to_string() });
                    nodes.push(SimNode { x: preview_w * 0.4, y: 12.0, w: preview_w * 0.5, h: preview_h * 0.45, label: "2".to_string() });
                    nodes.push(SimNode { x: 8.0, y: preview_h * 0.4, w: preview_w * 0.55, h: preview_h * 0.5, label: "3".to_string() });
                }
            }

            for node in nodes {
                let rect_x = preview_x + node.x;
                let rect_y = preview_y + node.y;
                let text_sz = 9.0;
                let text_w = node.label.len() as f32 * 6.0;
                let text_color = if self.is_active { [242, 242, 255] } else { [178, 178, 191] };
                let tx_offset = ((node.w - text_w) / 2.0).max(1.0);

                labels.push(TextLabel {
                    text: node.label,
                    x: rect_x + tx_offset,
                    y: crate::layout::align_text_y(rect_y, node.h, text_sz, 0.0),
                    font_size: text_sz,
                    color: text_color,
                });
            }
        }

        labels
    }
}
