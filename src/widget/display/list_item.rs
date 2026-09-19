//! `TextItem` (retained cosmic-text buffer holder, unchanged) and the narrow-trait
//! `InteractiveListItem` (Phase 5j): Button-style press/release with themed
//! selected/hover/press overlays and title/subtitle text.

use crate::colors;
use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, ElementState, Event, EventCtx, Input, Layout, MouseButton, Paint};

#[derive(Debug, Clone)]
pub struct TextItem {
    pub buffer: cosmic_text::Buffer,
    pub x: f32,
    pub y: f32,
    pub color: cosmic_text::Color,
    pub bounds: Option<[f32; 4]>,
    /// Optional circular clip `[cx, cy, r]` in logical pixels (a display-list item's
    /// `clip_circle` carried through to the glyph pass). `None` for ordinary labels.
    pub clip_circle: Option<[f32; 3]>,
    /// Optional rounded-rect clip `[cx, cy, bx, by, r]` in logical pixels (a display-list
    /// item's `clip_rrect` carried through to the glyph pass). `None` for ordinary labels.
    pub clip_rrect: Option<[f32; 5]>,
}

impl TextItem {
    pub fn new(
        fs: &mut cosmic_text::FontSystem,
        text: &str,
        size: f32,
        x: f32,
        y: f32,
        color: cosmic_text::Color,
        font: Option<&str>,
        bounds: Option<[f32; 4]>,
    ) -> Self {
        let buffer = crate::backend::window_runner::get_text_buffer(fs, text, size, font);
        Self { buffer, x, y, color, bounds, clip_circle: None, clip_rrect: None }
    }
}

#[derive(Debug, Clone)]
pub struct InteractiveListItem {
    pub title: String,
    pub subtitle: Option<String>,
    pub selected: bool,
    pub pressed: bool,
    pub just_clicked: bool,
    hovered: bool,
}

impl InteractiveListItem {
    pub fn new(title: &str) -> Adapted<InteractiveListItem> {
        Adapted::new(InteractiveListItem {
            title: title.to_string(),
            subtitle: None,
            selected: false,
            pressed: false,
            just_clicked: false,
            hovered: false,
        })
    }

    pub fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}

impl Adapted<InteractiveListItem> {
    pub fn with_subtitle(mut self, subtitle: &str) -> Self {
        self.subtitle = Some(subtitle.to_string());
        self
    }
}

impl Layout for InteractiveListItem {
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for InteractiveListItem {
    /// A list row's text is in the list font — the rows of a TreeList, a menu's items.
    fn widget_font(&self) -> Option<String> {
        Some(crate::layout::list_font())
    }

    fn color(&self) -> [f32; 4] {
        let theme = colors::active_theme();
        if self.selected {
            let mut base_color = colors::highlight_primary_color();
            if self.pressed {
                base_color[3] = (base_color[3] + theme.press_overlay[3]).min(1.0);
            } else if self.hovered {
                base_color[3] = (base_color[3] + theme.hover_overlay[3]).min(1.0);
            }
            base_color
        } else if self.pressed {
            let mut base_color = theme.surface_bg;
            base_color[3] = (base_color[3] + theme.press_overlay[3]).min(1.0);
            base_color
        } else if self.hovered {
            let mut base_color = theme.surface_bg;
            base_color[3] = (base_color[3] + theme.hover_overlay[3]).min(1.0);
            base_color
        } else {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let col = self.color();
        if col[3] > 0.0 {
            // The state wash, rounded like a list-row Button's.
            let r = crate::layout::button_corner_radius();
            ctx.rounded_rect(rect, r, (true, true, true, true), col);
        }

        let (x, y, h) = (rect.x, rect.y, rect.height);
        let title_y = if self.subtitle.is_some() {
            crate::layout::align_text_y(y, h, 22.0, 0.0)
        } else {
            crate::layout::align_text_y(y, h, 12.0, 0.0)
        };
        let fc = colors::list_font_color();
        let title_col = [(fc[0] * 255.0) as u8, (fc[1] * 255.0) as u8, (fc[2] * 255.0) as u8];
        // Clipped to the row. A row's width comes from the LIST, not from its
        // own text, so a long title or path is routine here — and unbounded it
        // simply kept drawing past the row's right edge, over the scrollbar and
        // out of the list.
        let clip = Some([rect.x, rect.y, rect.x + rect.width, rect.y + rect.height]);
        ctx.text_with(self.title.clone(), x + 8.0, title_y, 12.0, title_col, None, clip);
        if let Some(ref sub) = self.subtitle {
            ctx.text_with(sub.clone(), x + 8.0, title_y + 13.0, 10.0, [140, 140, 153], None, clip);
        }
    }
}

impl Input for InteractiveListItem {
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                self.pressed = true;
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x, y, .. } => {
                if self.pressed && self.hit(ectx.rect, *x, *y) {
                    self.just_clicked = true;
                }
                std::mem::take(&mut self.pressed)
            }
            Event::MouseEnter => {
                // Hover flips are visual changes: report them handled so the
                // demand-driven frame loop repaints now — returning false left
                // the highlight waiting for the next unrelated rebuild.
                self.hovered = true;
                true
            }
            Event::MouseLeave => {
                self.hovered = false;
                true
            }
            _ => false,
        }
    }

    fn take_click(&mut self) -> bool {
        std::mem::take(&mut self.just_clicked)
    }

    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }
}
