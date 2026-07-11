use crate::widget::*;
use crate::widget::display::{TextLabel, TextItem};
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;

/// Narrow-trait text label (Phase 5f leaf sweep). The text lives on the model and is emitted by
/// `paint` as a `Text` prim, which the adapter's prim-derived `text_labels` bridge serves to
/// every legacy text path; `set_text` sync comes from the adapter's generic `Element::set_text`
/// override via [`Paint::sync_label`].
#[derive(Debug, Clone)]
pub struct Label {
    text: String,
    font_size: f32,
    color: [u8; 3],
}

impl Label {
    pub fn new(text: &str) -> Adapted<Label> {
        let (_, font_size) = crate::layout::control_label_font_parsed();
        let mut l = Adapted::new(Label {
            text: text.to_string(),
            font_size,
            color: colors::control_label_color_u8(),
        });
        // Keep the base copy in step too (context menus, fallback machinery).
        Element::set_text(&mut l, text);
        l
    }

    pub fn set_color(&mut self, color: [u8; 3]) {
        self.color = color;
    }
}

impl Adapted<Label> {
    pub fn with_font_size(mut self, size: f32) -> Self {
        self.font_size = size;
        self
    }

    pub fn with_color(mut self, color: [u8; 3]) -> Self {
        self.color = color;
        self
    }
}

impl Layout for Label {
    fn inline_label(&self) -> bool {
        true
    }

    /// Content size for the scene layout engine (Phase 2b). Width is the measured text extent
    /// (via the FontSystem-free `measure_text_width`); height is one line at this font size.
    fn intrinsic_size(&self) -> Option<Size> {
        let (family, _) = crate::layout::control_label_font_parsed();
        let width = crate::widget::display::measure_text_width(&self.text, &family, self.font_size);
        // Match the line-height factor used elsewhere in the toolkit (e.g. text_box).
        let height = self.font_size * 1.333;
        Some(Size::new(width, height))
    }
}

impl Paint for Label {
    fn color(&self) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    fn sync_label(&mut self, label: &str) {
        self.text = label.to_string();
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        ctx.text(
            self.text.clone(),
            rect.x,
            crate::layout::align_text_y(rect.y, rect.height, self.font_size, 0.0),
            self.font_size,
            self.color,
        );
    }
}

impl Input for Label {
    fn blocks_backplate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Legacy `text_labels` parity through the prim bridge, and `set_text` staying in sync
    /// through the adapter's `Element::set_text` override (the trait method apps actually hit).
    #[test]
    fn text_flows_and_set_text_syncs() {
        let mut l = Label::new("CPU: 3%").with_font_size(13.0).with_color([1, 2, 3]);
        Element::set_rect(&mut l, 10.0, 20.0, 100.0, 16.0);

        let labels = l.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "CPU: 3%");
        assert_eq!(labels[0].x, 10.0);
        assert_eq!(labels[0].font_size, 13.0);
        assert_eq!(labels[0].color, [1, 2, 3]);

        Element::set_text(&mut l, "CPU: 99%");
        assert_eq!(l.own_text_labels()[0].text, "CPU: 99%", "set_text reaches the paint source");

        let size = Element::intrinsic_size(&l).unwrap();
        assert!(size.width > 0.0);
        assert!(!Element::blocks_backplate_drag(&l));
    }
}

#[derive(Clone)]
pub struct SectionHeader {
    base: Widget,
}

impl SectionHeader {
    pub fn new(title: &str) -> Self {
        let mut base = Widget::new();
        base.label = Some(title.to_string());
        Self { base }
    }
}

impl Element for SectionHeader {
    crate::impl_widget_base!(SectionHeader);

    // Leaf legacy widget: own fonted labels via paint_self (the default no longer
    // drains the text getters).
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) {
        crate::scene::painter::paint_legacy_leaf(
            self, ui, ctx,
            crate::scene::painter::fonted_leaf_labels(self, ui, self.own_labels()),
        );
    }
    fn blocks_backplate_drag(&self) -> bool { false }
    fn color(&self) -> [f32; 4] { [0.0, 0.0, 0.0, 0.0] }
    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        vec![(self.base.x + 8.0, self.base.y + 22.0, self.base.w - 16.0, 1.0, [0.18, 0.18, 0.27, 1.0])]
    }

}

// Styled label builder with optional strikethrough
#[derive(Debug)]
pub struct StyledLabel {
    pub buffer: glyphon::Buffer,
    pub w: f32,
    pub color: [f32; 4],
    pub g_color: glyphon::Color,
    pub strikethrough: bool,
    pub strikethrough_color: Option<[f32; 4]>,
    // Source retained so the label can be re-emitted as a display-list Text prim (Phase 6ak):
    // the (possibly vertical-transformed) text, its size, family, and the box layout for the
    // vertical case (per-char lines + centered wrap). `buffer` above is kept for the legacy
    // draw()/width measurement path.
    src_text: String,
    src_size: f32,
    src_family: String,
    prim_layout: Option<crate::scene::paint::TextLayout>,
}

/// The data to emit a [`StyledLabel`] as a display-list `Prim::Text`: what
/// [`StyledLabel::into_prim`] returns, matching `PaintCtx::text_with` / `text_boxed` args.
pub struct LabelPrim {
    pub text: String,
    pub size: f32,
    pub x: f32,
    pub y: f32,
    pub color: [u8; 3],
    pub font: Option<String>,
    pub layout: Option<crate::scene::paint::TextLayout>,
}

impl StyledLabel {
    pub fn new(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4]) -> Self {
        Self::new_with_family(fs, text, size, color, "sans-serif")
    }

    pub fn new_with_family(fs: &mut glyphon::FontSystem, text: &str, size: f32, color: [f32; 4], family: &str) -> Self {
        let scale = crate::scale::scale_factor();
        let mut final_text = text.to_string();
        let is_vert = crate::IS_VERTICAL.load(std::sync::atomic::Ordering::Relaxed);
        if is_vert {
            final_text = text.chars().map(|c| c.to_string()).collect::<Vec<_>>().join("\n");
        }
        let mut buffer = crate::backend::window_runner::get_text_buffer(fs, &final_text, size, Some(family));
        if is_vert {
            let bar_thickness = crate::BAR_THICKNESS.load(std::sync::atomic::Ordering::Relaxed) as f32;
            buffer.set_size(fs, Some(bar_thickness * scale as f32), None);
            for line in &mut buffer.lines {
                line.set_align(Some(glyphon::cosmic_text::Align::Center));
            }
            buffer.shape_until_scroll(fs, true);
        }
        let mut w = buffer.layout_runs().next().map(|r| r.line_w).unwrap_or(0.0) / scale;
        if is_vert {
            let num_lines = buffer.layout_runs().count();
            w = num_lines as f32 * size * 1.05;
        }
        let g_color = glyphon::Color::rgb(
            (color[0] * 255.0) as u8,
            (color[1] * 255.0) as u8,
            (color[2] * 255.0) as u8,
        );
        // Vertical text is boxed: the per-char-newline `final_text` wrapped to the bar
        // thickness and centered — the same set_size + center-align the buffer path applied.
        let prim_layout = if is_vert {
            let bar_thickness = crate::BAR_THICKNESS.load(std::sync::atomic::Ordering::Relaxed) as f32;
            Some(crate::scene::paint::TextLayout {
                // Effectively unbounded height (the legacy vertical path used height None);
                // align_v Top means no vertical offset, so only set_size's height sees this.
                wrap_width: Some(bar_thickness),
                box_height: 100_000.0,
                align_h: crate::scene::paint::AlignH::Center,
                align_v: crate::scene::paint::AlignV::Top,
            })
        } else {
            None
        };
        Self {
            buffer,
            w,
            color,
            g_color,
            strikethrough: false,
            strikethrough_color: None,
            src_text: final_text,
            src_size: size,
            src_family: family.to_string(),
            prim_layout,
        }
    }

    /// Consume the label and return the data to emit it as a display-list `Prim::Text`
    /// (Phase 6ak): the source text, size, family, and — for vertical bars — the box layout.
    /// `x, y` are the draw position; vertical labels pin `y` to 0 (as `draw` did).
    pub fn into_prim(self, x: f32, y: f32) -> LabelPrim {
        let is_vert = crate::IS_VERTICAL.load(std::sync::atomic::Ordering::Relaxed);
        LabelPrim {
            text: self.src_text,
            size: self.src_size,
            x,
            y: if is_vert { 0.0 } else { y },
            color: [
                (self.color[0] * 255.0) as u8,
                (self.color[1] * 255.0) as u8,
                (self.color[2] * 255.0) as u8,
            ],
            font: Some(self.src_family),
            layout: self.prim_layout,
        }
    }

    pub fn with_strikethrough(mut self, enabled: bool) -> Self {
        self.strikethrough = enabled;
        self
    }

    pub fn with_strikethrough_color(mut self, color: [f32; 4]) -> Self {
        self.strikethrough_color = Some(color);
        self
    }

    pub fn draw(self, text_items: &mut Vec<TextItem>, x: f32, y: f32) -> f32 {
        let w = self.w;
        let is_vert = crate::IS_VERTICAL.load(std::sync::atomic::Ordering::Relaxed);
        text_items.push(TextItem {
            buffer: self.buffer,
            x,
            y: if is_vert { 0.0 } else { y },
            color: self.g_color,
            bounds: None,
        });
        w
    }

    pub fn strikethrough_rect(&self, x: f32, y: f32, scale: f32) -> Option<(f32, f32, f32, f32, [f32; 4])> {
        if self.strikethrough {
            let col = self.strikethrough_color.unwrap_or(self.color);
            let font_size = self.buffer.metrics().font_size / scale;
            let line_y = self.buffer.layout_runs().next().map(|r| r.line_y).unwrap_or(font_size * scale * 1.05) / scale;
            let offset_y = line_y - 0.28 * font_size;
            let padding = 4.0;
            Some((
                x - padding,
                y + offset_y,
                self.w + 2.0 * padding,
                1.0,
                col,
            ))
        } else {
            None
        }
    }
}

impl SectionHeader {
    pub(crate) fn own_labels(&self) -> Vec<TextLabel> {
        vec![TextLabel {
            text: self.base.label.clone().unwrap_or_default(),
            x: self.base.x + 12.0,
            y: self.base.y,
            font_size: 14.0,
            color: [212, 212, 212],
        }]
    }
}
