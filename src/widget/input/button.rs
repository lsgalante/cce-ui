//! Narrow-trait `Button` (Phase 5f). Press/release semantics match the legacy `mouse_input`
//! exactly: press (hit-gated by the adapter) arms it; release *anywhere* commits (in-rect,
//! firing `on_click_cb` + `take_click`) or cancels — which is why the adapter forwards releases
//! ungated. Hover is tracked from `MouseEnter`/`MouseLeave`; press+hover drive the per-kind
//! color matrix that becomes `Animated<f32>` lerping in RFC §3.6.

use crate::colors;
use crate::scene::layout::{Rect, Size};
use crate::scene::paint::PaintCtx;
use crate::widget::{
    Adapted, WidgetHost, ElementState, Event, EventCtx, Input, Justification, Key, Layout,
    MouseButton, NamedKey, Paint,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonKind {
    Primary,
    Reset,
    ListRow,
    CopyIcon,
    /// A row in a menu: no plate and no border of its own, transparent until
    /// hovered, because a menu draws ONE recess around the whole run and the
    /// items butt together inside it. Keeps button typography and honours
    /// `with_justify`, which is what separates it from `ListRow` (list font,
    /// list justification config).
    MenuItem,
}

#[derive(Clone)]
pub struct Button {
    pressed: bool,
    just_clicked: bool,
    kind: ButtonKind,
    pub selected: bool,
    pub on_click_cb: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    pub bg: Option<[f32; 4]>,
    pub hover_bg: Option<[f32; 4]>,
    pub label_color: Option<[f32; 4]>,
    pub justify: Justification,
    label: Option<String>,
    /// Icon face: an uploaded texture `(image id, pixel w, pixel h)` drawn
    /// centered in place of the label (see [`crate::upload_icon`]).
    icon: Option<(u32, f32, f32)>,
    /// Opacity of the icon face — the ONLY state lever an icon has, since
    /// `PaintCtx::image` carries no color and images ignore vertex color. A
    /// disabled icon button dims instead of graying its glyph.
    icon_alpha: f32,
    hovered: bool,
    /// Keyboard focus, tracked from `FocusIn`/`FocusOut` the way Checkbox does —
    /// `Paint` never sees the `Widget` base, so the flag has to live here to be
    /// paintable. Drives the focus ring and gates Enter/Space activation.
    focused: bool,
    /// Raised style: the background is an SDF-lit `Bevel` plate — fill plus a
    /// rolled, lit edge — instead of a flat fill + border stroke.
    raised: Option<bool>,
}

impl std::fmt::Debug for Button {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Button")
            .field("pressed", &self.pressed)
            .field("just_clicked", &self.just_clicked)
            .field("kind", &self.kind)
            .field("selected", &self.selected)
            .field("label", &self.label)
            .field("hovered", &self.hovered)
            .field("on_click_cb", &self.on_click_cb.as_ref().map(|_| "<callback>"))
            .finish()
    }
}

impl Button {
    /// The style in force: the per-widget override (`with_raised`) when set, else
    /// the DE's `control_relief`, read live so a runtime switch
    /// (`layout::set_control_relief`) restyles every control at once.
    fn raised(&self) -> bool {
        self.raised.unwrap_or_else(crate::layout::control_relief)
    }

    fn model(kind: ButtonKind) -> Button {
        Button {
            pressed: false,
            just_clicked: false,
            kind,
            selected: false,
            on_click_cb: None,
            bg: None,
            hover_bg: None,
            label_color: None,
            justify: Justification::Center,
            label: None,
            icon: None,
            icon_alpha: 1.0,
            hovered: false,
            focused: false,
            raised: None,
        }
    }

    fn adapted(kind: ButtonKind, x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        let mut b = Adapted::new(Button::model(kind));
        WidgetHost::set_rect(&mut b, x, y, w, h);
        b
    }

    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        Button::adapted(ButtonKind::Primary, x, y, w, h)
    }

    pub fn new_reset(x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        Button::adapted(ButtonKind::Reset, x, y, w, h)
    }

    pub fn new_list_row(x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        Button::adapted(ButtonKind::ListRow, x, y, w, h)
    }

    /// A menu row — see [`ButtonKind::MenuItem`]. The host draws the shared
    /// recess; this draws only its label and its hover.
    pub fn new_menu_item(x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        Button::adapted(ButtonKind::MenuItem, x, y, w, h)
    }

    pub fn new_copy_icon(x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        let b = Button::adapted(ButtonKind::CopyIcon, x, y, w, h);
        // Copy icon face (cce-icons); label fallback if the icon set is
        // missing on this machine.
        match crate::upload_icon("copy", 32) {
            Some((id, iw, ih)) => b.with_icon(id, iw as f32, ih as f32),
            None => b.with_label("📋"),
        }
    }

    /// Whether an icon face is set (hosts size icon buttons square).
    pub fn has_icon(&self) -> bool {
        self.icon.is_some()
    }

    /// Where the icon face draws inside `rect`: centered, inset one 4px margin
    /// per side from the shorter extent, native aspect kept. `None` when this
    /// button has no icon.
    ///
    /// Public because a flat-path host draws the icon itself — it consumes
    /// `all_quads` and a text list, so `paint` never runs for it and an image
    /// is neither a quad nor a label. Keeping the geometry here means the icon
    /// lands in the same place on both paths.
    pub fn icon_rect(&self, rect: Rect) -> Option<(u32, Rect, f32)> {
        let (image, iw, ih) = self.icon?;
        let s = (rect.width.min(rect.height) - 8.0).max(4.0);
        let (dw, dh) = if iw >= ih {
            (s, s * ih / iw.max(1.0))
        } else {
            (s * iw / ih.max(1.0), s)
        };
        Some((
            image,
            Rect {
                x: rect.x + (rect.width - dw) / 2.0,
                y: rect.y + (rect.height - dh) / 2.0,
                width: dw,
                height: dh,
            },
            self.icon_alpha,
        ))
    }

    /// Hover state, also settable by immediate-mode hosts that hit-test themselves.
    pub fn hovered(&self) -> bool {
        self.hovered
    }

    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    fn font(&self) -> (String, f32) {
        let font_str = if self.kind == ButtonKind::ListRow {
            crate::layout::list_font()
        } else {
            crate::layout::button_font()
        };
        let (family, size) = crate::layout::parse_font_string(&font_str);
        (family, size.unwrap_or(12.0))
    }

    fn label_width(&self, label: &str) -> f32 {
        if label == "📋" {
            12.0
        } else {
            let (family, size) = self.font();
            crate::widget::display::measure_text_width(label, &family, size)
        }
    }

    /// The control plate this Button's `paint` draws — flush, at the button
    /// radius, its state colour as the face — or `None` when it draws none
    /// (flat styling, or a ListRow / MenuItem, transparent-until-hover
    /// surfaces that would wear a permanent carved ring on every idle row).
    pub fn plate(&self, rect: Rect) -> Option<crate::widget::ControlPlate> {
        if !self.raised()
            || self.kind == ButtonKind::ListRow
            || self.kind == ButtonKind::MenuItem
        {
            return None;
        }
        let radius = crate::layout::button_corner_radius();
        // Keyboard focus lights the plate's own rim — the ring IS the silhouette.
        let tint = self.focused.then(crate::widget::ControlPlate::focus_tint);
        Some(
            crate::widget::ControlPlate::control(rect, radius, crate::widget::PlateStance::Flush, self.color())
                .with_tint(tint),
        )
    }

    /// [`Button::plate`] as the legacy `(rect, corner radius, depth, face
    /// colour)` tuple — the flat-path bridge's view of the same plate.
    pub fn inset_face(&self, rect: Rect) -> Option<(Rect, f32, f32, [f32; 4])> {
        self.plate(rect).map(|p| (p.rect, p.radii.0, p.depth, p.face))
    }
}

/// The by-value builder chain, mirrored on the wrapped type (`with_label` comes from the generic
/// `Adapted::with_label`, which syncs the model's copy via `Paint::sync_label`).
impl Adapted<Button> {

    /// Icon face: draw this uploaded texture centered in place of a label —
    /// pass [`crate::upload_icon`]'s `(id, w, h)`. Pairs with a plain `new()`
    /// (no `with_label`), so the legacy label views stay empty.
    pub fn with_icon(mut self, image: u32, w: f32, h: f32) -> Self {
        self.icon = Some((image, w, h));
        self
    }

    /// Dim the icon face — see the `icon_alpha` field. 1.0 is fully opaque.
    pub fn with_icon_alpha(mut self, alpha: f32) -> Self {
        self.icon_alpha = alpha;
        self
    }

    /// Raised style: see the `raised` field.
    pub fn with_raised(mut self, raised: bool) -> Self {
        self.raised = Some(raised);
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
        self.justify = if left_align { Justification::Left } else { Justification::Center };
        self
    }

    pub fn with_justify(mut self, justify: Justification) -> Self {
        self.justify = justify;
        self
    }
}

impl Layout for Button {
    fn inline_label(&self) -> bool {
        true
    }

    /// Content size for the scene layout engine (ported from Phase 2b): the label's measured
    /// width plus an 8px inset each side, at the configured button height; an icon button is a
    /// square at that height.
    fn intrinsic_size(&self) -> Option<Size> {
        let height = crate::layout::button_height();
        let label = self.label.as_deref().unwrap_or("");
        Some(Size::new(self.label_width(label) + 16.0, height))
    }
}

impl Paint for Button {
    fn color(&self) -> [f32; 4] {
        if self.pressed || self.hovered {
            if let Some(hbg) = self.hover_bg {
                return hbg;
            }
        } else if let Some(bg) = self.bg {
            return bg;
        }
        match self.kind {
            ButtonKind::Primary => {
                if self.pressed {
                    colors::button_press_color()
                } else if self.hovered {
                    colors::button_hover_color()
                } else {
                    colors::button_background_color()
                }
            }
            ButtonKind::MenuItem => {
                // Idle is fully transparent so the shared recess reads as one
                // continuous well; only the hovered row lifts out of it.
                if self.pressed {
                    colors::button_press_color()
                } else if self.hovered {
                    colors::button_hover_color()
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                }
            }
            ButtonKind::Reset => {
                if self.pressed {
                    colors::RESET_BTN_PRESS
                } else if self.hovered {
                    colors::RESET_BTN_HOVER
                } else {
                    colors::RESET_BTN_IDLE
                }
            }
            ButtonKind::ListRow => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.6] }
                    else if self.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.4] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.hovered { [0.20, 0.20, 0.25, 0.15] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
            ButtonKind::CopyIcon => {
                if self.selected {
                    if self.pressed { [0.30, 0.52, 0.78, 0.5] }
                    else if self.hovered { [0.30, 0.52, 0.78, 0.5] }
                    else { [0.20, 0.40, 0.65, 0.2] }
                } else {
                    if self.pressed { [0.20, 0.20, 0.25, 0.25] }
                    else if self.hovered { [0.20, 0.20, 0.25, 0.25] }
                    else { [0.0, 0.0, 0.0, 0.0] }
                }
            }
        }
    }

    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        let r = crate::layout::button_corner_radius();
        if r > 0.0 {
            Some((r, (true, true, true, true)))
        } else {
            None
        }
    }

    fn widget_font(&self) -> Option<String> {
        if self.kind == ButtonKind::ListRow {
            Some(crate::layout::list_font())
        } else {
            Some(crate::layout::button_font())
        }
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
        let radius = crate::layout::button_corner_radius();
        let color = self.color();

        // Relief style: a flush inset plate — the button sits sunken in a
        // carved groove ring with its beveled lip rising back to the surface
        // plane, face level with the surface. Transparent fills degrade to
        // edges-only inside the groove (an opaque hover_color fills the face).
        // List rows are exempt: they are transparent-until-hover/selected
        // surfaces, and the edges-only groove would stack a permanent carved
        // ring on every idle row of a list.
        if let Some(plate) = self.plate(rect) {
            ctx.control_plate(&plate);
        } else {
            // ListRow also skips the border idiom below: it draws the border
            // color as a FULL rect with the fill inset over it, which only
            // reads as a 1px ring when the fill is opaque — a row's
            // transparent idle fill left the whole row painted in the config
            // button border_color (an accidental coupling).
            // Keyboard focus reuses the border the button already draws, tinted with
            // the DE's existing focus-border colour — no new geometry, and nothing
            // changes for a button that is not focused. It overrides the ListRow
            // opt-out too: a focused row must show the ring, which is the whole point.
            let border_color = if self.focused {
                Some(colors::tree_border_focus_color())
            } else if self.kind == ButtonKind::ListRow || self.kind == ButtonKind::MenuItem {
                None
            } else {
                colors::button_border_color()
            };
            // Background (+ optional configured border), split by radius exactly as the legacy
            // `all_rounded_quads` (rounded) / `extra_quads` (square) overrides emitted it.
            if radius > 0.0 {
                if let Some(bc) = border_color {
                    ctx.rounded_rect(rect, radius, (true, true, true, true), bc);
                    ctx.rounded_rect(
                        Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: h - 2.0 },
                        (radius - 1.0).max(0.0),
                        (true, true, true, true),
                        color,
                    );
                } else if color[3].abs() > 0.001 {
                    ctx.rounded_rect(rect, radius, (true, true, true, true), color);
                }
            } else if let Some(bc) = border_color {
                ctx.quad(rect, bc);
                ctx.quad(Rect { x: x + 1.0, y: y + 1.0, width: w - 2.0, height: h - 2.0 }, color);
            } else if color[3].abs() > 0.001 {
                ctx.quad(rect, color);
            }
        }

        // Icon face: replaces the label. Geometry from `icon_rect` — see there
        // for why it is not inlined here.
        if let Some((image, rect, alpha)) = self.icon_rect(rect) {
            ctx.image(image, rect, alpha);
            return;
        }

        // Label, with per-kind justification/color (legacy `text_labels`).
        if let Some(ref label) = self.label {
            let (_, font_size) = self.font();
            let est_w = self.label_width(label);
            let color = if let Some(lc) = self.label_color {
                [(lc[0] * 255.0) as u8, (lc[1] * 255.0) as u8, (lc[2] * 255.0) as u8]
            } else {
                match self.kind {
                    ButtonKind::ListRow | ButtonKind::CopyIcon => {
                        if self.selected { [230, 230, 242] } else { [178, 178, 191] }
                    }
                    _ => colors::control_label_color_u8(),
                }
            };
            let justify = if self.kind == ButtonKind::ListRow {
                match crate::layout::list_justification() {
                    0 => Justification::Left,
                    2 => Justification::Right,
                    _ => Justification::Center,
                }
            } else {
                self.justify
            };
            let tx = match justify {
                Justification::Left => x + 8.0,
                Justification::Right => x + w - est_w - 8.0,
                Justification::Center => x + (w - est_w) / 2.0,
            };
            ctx.text(
                label.clone(),
                tx,
                crate::layout::align_text_y(y, h, font_size, 0.0),
                font_size,
                color,
            );
        }
    }
}

impl Input for Button {
    /// A plate — except a ListRow or MenuItem, which wears no plate (see
    /// [`Button::plate`]): a list's rows are walked by the list, not by Tab.
    fn focus_role(&self) -> crate::widget::FocusRole {
        match self.kind {
            ButtonKind::ListRow | ButtonKind::MenuItem => crate::widget::FocusRole::None,
            _ => crate::widget::FocusRole::Plate,
        }
    }
    fn on_event(&mut self, event: &Event, ectx: &mut EventCtx) -> bool {
        match event {
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, .. } => {
                // Presses are hit-gated by the adapter.
                self.pressed = true;
                true
            }
            Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x, y, .. } => {
                // Releases arrive ungated: commit in-rect, cancel anywhere else — the legacy
                // `mouse_input` released-while-pressed contract.
                if self.pressed && self.hit(ectx.rect, *x, *y) {
                    self.just_clicked = true;
                    if let Some(ref cb) = self.on_click_cb {
                        cb();
                    }
                }
                std::mem::take(&mut self.pressed)
            }
            Event::MouseEnter => {
                self.hovered = true;
                false
            }
            Event::MouseLeave => {
                self.hovered = false;
                false
            }
            Event::FocusIn => {
                self.focused = true;
                false
            }
            Event::FocusOut => {
                self.focused = false;
                false
            }
            Event::KeyInput(key_event) => {
                // Enter/Space activate a focused button, the same chord Dropdown
                // and Menu use. Routed through `just_clicked` + `on_click_cb` so a
                // keyboard press is indistinguishable downstream from a mouse one.
                if !self.focused || key_event.state != ElementState::Pressed {
                    return false;
                }
                match key_event.logical_key {
                    Key::Named(NamedKey::Enter) | Key::Named(NamedKey::Space) => {
                        self.just_clicked = true;
                        if let Some(ref cb) = self.on_click_cb {
                            cb();
                        }
                        true
                    }
                    _ => false,
                }
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


pub enum PageButton {
    Active,
    Inactive,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::UiContext;

    fn press(x: f32, y: f32) -> Event {
        Event::MouseButton { button: MouseButton::Left, state: ElementState::Pressed, x, y, local_x: x, local_y: y }
    }
    fn release(x: f32, y: f32) -> Event {
        Event::MouseButton { button: MouseButton::Left, state: ElementState::Released, x, y, local_x: x, local_y: y }
    }

    #[test]
    fn intrinsic_size_scales_with_label_and_has_button_height() {
        let short = Button::new(0.0, 0.0, 0.0, 0.0).with_label("Hi");
        let long = Button::new(0.0, 0.0, 0.0, 0.0).with_label("A much longer button label");

        let s = short.intrinsic_size().unwrap();
        let l = long.intrinsic_size().unwrap();
        assert!(s.width > 16.0, "includes the horizontal insets");
        assert!(l.width > s.width, "longer label measures wider");
        assert_eq!(s.height, crate::layout::button_height());
    }

    /// The legacy press/release contract through the real router: press arms, in-rect release
    /// clicks (firing the callback), out-of-rect release cancels without clicking.
    #[test]
    fn press_release_semantics_match_legacy() {
        let mut ctx = UiContext::new();
        let fired = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let fired2 = fired.clone();
        let mut b = Button::new(10.0, 10.0, 80.0, 24.0)
            .with_label("Go")
            .on_click(move || { fired2.fetch_add(1, std::sync::atomic::Ordering::SeqCst); });
        let (id, ptr) = (b.id(), b.as_ptr_mut());
        ctx.register_widget(id, ptr);

        // Press in, release in -> click.
        assert!(ctx.propagate_event(&press(20.0, 20.0), id));
        assert!(ctx.propagate_event(&release(25.0, 20.0), id), "release consumed (was pressed)");
        assert!(b.take_click());
        assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1, "callback fired");

        // Press in, release OUT -> cancelled, no click, but release still consumed.
        assert!(ctx.propagate_event(&press(20.0, 20.0), id));
        assert!(ctx.propagate_event(&release(500.0, 500.0), id), "cancelling release consumed");
        assert!(!b.take_click(), "no click on out-of-rect release");
        assert_eq!(fired.load(std::sync::atomic::Ordering::SeqCst), 1, "callback not re-fired");

        // Release without a press is not consumed.
        assert!(!ctx.propagate_event(&release(20.0, 20.0), id));
    }

    /// Bridge parity for the default config: bg on the rounded or plain path per the configured
    /// radius, and the label through the prim-derived text bridge with center justification.
    #[test]
    fn geometry_and_label_parity() {
        let ctx = UiContext::new();
        // Pin the flat style: this test is about the legacy quad-bridge paths,
        // which the config-default raised plate bypasses entirely.
        let b = Button::new(0.0, 0.0, 100.0, 24.0).with_label("Go").with_raised(false);

        let radius = crate::layout::button_corner_radius();
        let rounded = WidgetHost::all_rounded_quads(&b, &ctx);
        let plain = WidgetHost::extra_quads(&b);
        if radius > 0.0 {
            assert!(!rounded.is_empty() && plain.is_empty(), "rounded config -> rounded path only");
            assert_eq!(rounded[0].4, radius);
        } else {
            assert!(rounded.is_empty() && !plain.is_empty(), "square config -> plain path only");
        }

        let labels = b.own_text_labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].text, "Go");
        let est = b.label_width("Go");
        assert_eq!(labels[0].x, (100.0 - est) / 2.0, "center-justified");

        // Selection state flows through the WidgetHost forward (list hosts push it).
        let mut b = b;
        b.set_selected(true);
        assert!(b.selected);
    }
}

#[cfg(test)]
mod focus_ring_tests {
    use super::*;
    use crate::scene::paint::{PaintCtx, Prim};
    use crate::widget::{Event, WidgetHost};

    /// The focus ring is the plate's own rim lit: focused, the trough carries
    /// the highlight tint; unfocused, the same trough untinted — no extra geometry.
    #[test]
    fn focus_lights_the_plate_rim() {
        let mut ctx = crate::widget::UiContext::new();
        let mut b = Button::new(0.0, 0.0, 120.0, 26.0).with_label("Plate").with_raised(true);
        WidgetHost::set_rect(&mut b, 10.0, 20.0, 120.0, 26.0);
        let rect = Rect { x: 10.0, y: 20.0, width: 120.0, height: 26.0 };
        let troughs = |b: &Adapted<Button>| -> Vec<Option<[f32; 3]>> {
            let mut pc = PaintCtx::new();
            Paint::paint(b.inner(), rect, &mut pc);
            pc.finish().items.into_iter().filter_map(|i| match i.prim { Prim::Trough { tint, .. } => Some(tint), _ => None }).collect()
        };
        assert_eq!(troughs(&b), vec![None], "unfocused: one untinted trough");
        b.handle_event(&Event::FocusIn, &mut ctx);
        assert_eq!(troughs(&b), vec![Some(crate::widget::ControlPlate::focus_tint())], "focused: the rim lit");
        b.handle_event(&Event::FocusOut, &mut ctx);
        assert_eq!(troughs(&b), vec![None]);
    }
}
