use crate::colors;
use crate::widget::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Reset,
    ListRow,
    CopyIcon,
}

#[derive(Clone)]
pub struct Button {
    base: Widget,
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
    pub selected: bool,
    pub on_click_cb: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    pub bg: Option<[f32; 4]>,
    pub hover_bg: Option<[f32; 4]>,
    pub label_color: Option<[f32; 4]>,
    pub left_align: bool,
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("base", &self.base)
            .field("pressed", &self.pressed)
            .field("just_clicked", &self.just_clicked)
            .field("kind", &self.kind)
            .field("selected", &self.selected)
            .field("on_click_cb", &self.on_click_cb.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

impl Button {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::Primary,
            selected: false,
            on_click_cb: None,
            bg: None,
            hover_bg: None,
            label_color: None,
            left_align: false,
        }
    }

    pub fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::Reset,
            selected: false,
            on_click_cb: None,
            bg: None,
            hover_bg: None,
            label_color: None,
            left_align: false,
        }
    }

    pub fn new_list_row(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::ListRow,
            selected: false,
            on_click_cb: None,
            bg: None,
            hover_bg: None,
            label_color: None,
            left_align: false,
        }
    }

    pub fn new_copy_icon(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self {
            base: Widget::new_rect(x, y, w, h),
            pressed: false,
            just_clicked: false,
            kind: ButtonKind::CopyIcon,
            selected: false,
            on_click_cb: None,
            bg: None,
            hover_bg: None,
            label_color: None,
            left_align: false,
        }
    }

    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self
    }

    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    pub fn on_click<F: Fn() + Send + Sync + 'static>(mut self, cb: F) -> Self {
        self.on_click_cb = Some(std::sync::Arc::new(cb));
        self
    }

    pub fn with_bg(mut self, bg: [f32; 4]) -> Self {
        self.bg = Some(bg);
        self
    }

    pub fn with_hover_bg(mut self, hover_bg: [f32; 4]) -> Self {
        self.hover_bg = Some(hover_bg);
        self
    }

    pub fn with_label_color(mut self, label_color: [f32; 4]) -> Self {
        self.label_color = Some(label_color);
        self
    }

    pub fn with_left_align(mut self, left_align: bool) -> Self {
        self.left_align = left_align;
        self
    }
}

impl Element for Button {
    crate::impl_widget_base!(Button);
    fn highlight_quad(&self, ctx: &UiContext) -> Option<(f32, f32, f32, f32, [f32; 4])>{ None }

    fn color(&self) -> [f32; 4] {
        if self.pressed || self.base.hovered {
            if let Some(hbg) = self.hover_bg {
                return hbg;
            }
        } else {
            if let Some(bg) = self.bg {
                return bg;
            }
        }
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed { colors::BUTTON_PRESS }
                else if self.base.hovered { colors::BUTTON_HOVER }
                else { colors::BUTTON_IDLE }
            }
            ButtonKind::Reset => {
                if self.pressed { colors::RESET_BTN_PRESS }
                else if self.base.hovered { colors::RESET_BTN_HOVER }
                else { colors::RESET_BTN_IDLE }
            }
            ButtonKind::ListRow => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.6] }
                    else if self.base.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.4] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.base.hovered { [0.20, 0.20, 0.25, 0.15] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
            ButtonKind::CopyIcon => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.5] }
                    else if self.base.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.2] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.base.hovered { [0.20, 0.20, 0.25, 0.25] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
        }
    }

    fn mouse_input(&mut self, button: MouseButton, state: ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        if button != MouseButton::Left { return false; }
        match state {
            ElementState::Pressed => {
                if self.hit_test(px, py, ctx) {
                    self.pressed = true;
                    return true;
                }
            }
            ElementState::Released => {
                if self.pressed && self.hit_test(px, py, ctx) {
                    self.just_clicked = true;
                    if let Some(ref cb) = self.on_click_cb {
                        cb();
                    }
                }
                let was = self.pressed;
                self.pressed = false;
                return was;
            }
        }
        false
    }

    fn take_click(&mut self) -> bool {
        if self.just_clicked { self.just_clicked = false; true } else { false }
    }

    fn text_labels(&self) -> Vec<TextLabel> {
        let mut labels = Vec::new();
        if let Some(ref label) = self.base.label {
            let font_size = 12.0;
            let est_w = if label == "📋" {
                12.0
            } else {
                TextLabel::estimate_width(label, font_size)
            };
            let color = if let Some(lc) = self.label_color {
                [
                    (lc[0] * 255.0) as u8,
                    (lc[1] * 255.0) as u8,
                    (lc[2] * 255.0) as u8,
                ]
            } else {
                match self.kind {
                    ButtonKind::ListRow | ButtonKind::CopyIcon => {
                        if self.selected { [230, 230, 242] }
                        else { [178, 178, 191] }
                    }
                    _ => [0xcc, 0xcc, 0xd4]
                }
            };
            let x = if self.left_align {
                self.base.x + 8.0
            } else {
                self.base.x + (self.base.w - est_w) / 2.0
            };
            labels.push(TextLabel {
                text: label.clone(),
                x,
                y: crate::layout::align_text_y(self.base.y, self.base.h, font_size, 0.0),
                font_size,
                color,
            });
        }
        labels
    }
    fn set_selected(&mut self, selected: bool) {
        self.selected = selected;
    }

    fn extra_quads(&self) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
        vec![(self.base.x, self.base.y, self.base.w, self.base.h, self.color())]
    }

    fn rounded_corners(&self) -> (bool, bool, bool, bool) {
        let r = crate::layout::button_corner_radius();
        if r > 0.0 {
            (true, true, true, true)
        } else {
            (false, false, false, false)
        }
    }

    fn corner_radius(&self) -> f32 {
        crate::layout::button_corner_radius()
    }
}

pub enum PageButton {
    Active,
    Inactive,
}

impl Control for Button {}
