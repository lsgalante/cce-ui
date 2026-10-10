//! `Button`: a press (hit-gated by the adapter) arms it; a release anywhere commits — inside its
//! rect it fires `on_click_cb` and `take_click` — or cancels, which is why the adapter forwards
//! releases ungated. Hover is tracked from `MouseEnter` / `MouseLeave`; pressed and hovered pick
//! the face colour from the button's kind (`ButtonKind`), unless the host set its own. A button
//! can carry an icon (a bundled glyph by name, or an image the app uploaded) in place of, or
//! beside, its label.
//!
//! | module | holds |
//! |---|---|
//! | `mod.rs` | the kinds, the struct, construction, icons, the label's font and width, the plate, the builders, `impl Layout`, `PageButton` |
//! | `paint` | `impl Paint`: the face colour, the plate, the icon and label |
//! | `input` | `impl Input`: press, release, hover, the keyboard |

mod input;
mod paint;
#[cfg(test)]
mod focus_ring_tests;
#[cfg(test)]
mod tests;

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
    ///
    /// Set directly by [`Adapted::with_icon`] for an app that owns its own
    /// upload — and then it is the APP's job to replace it when the renderer
    /// is rebuilt, because an image id names an entry in one renderer's image
    /// table and nothing here can produce those pixels again.
    /// [`icon_name`] is the way out of that for a bundled glyph.
    ///
    /// [`Adapted::with_icon`]: Adapted::<Button>::with_icon
    /// [`icon_name`]: Button::icon_name
    icon: Option<(u32, f32, f32)>,
    /// A bundled cce-icons glyph name, when the face came from one
    /// ([`Adapted::with_icon_name`], [`Button::new_icon`]). Takes precedence
    /// over [`icon`]: the id is then re-resolved through
    /// [`crate::upload_icon`] on every read rather than captured once.
    ///
    /// That indirection is the whole point. An id captured at construction
    /// dies with its renderer — `window_runner` builds a new one around the
    /// same `Application` when it repairs a lost Wayland transport, and a
    /// draw for an id the new image table does not hold is skipped rather
    /// than reported, so every icon button in the process went blank and
    /// stayed blank. `upload_icon`'s cache is keyed on the renderer epoch, so
    /// re-reading through it costs a hash lookup per frame and yields a live
    /// id on the first frame after a rebuild.
    ///
    /// [`Adapted::with_icon_name`]: Adapted::<Button>::with_icon_name
    /// [`icon`]: Button::icon
    icon_name: Option<String>,
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
    /// Flat stance ([`crate::widget::PlateStance::Flat`]): the face alone,
    /// no relief, silhouette equal to the rect. Overrides `raised`.
    flat: bool,
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
            icon_name: None,
            icon_alpha: 1.0,
            hovered: false,
            focused: false,
            raised: None,
            flat: false,
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
        Button::new_icon("copy", &crate::l10n::tr("button-copy"), x, y, w, h)
    }

    /// A plateless icon button faced with the bundled cce-icons glyph
    /// `<name>.svg` (see [`crate::upload_icon`]): transparent until hovered,
    /// the [`ButtonKind::CopyIcon`] treatment, for glyphs that sit in a bar
    /// rather than on a plate. `fallback` is the label drawn instead when the
    /// icon set is missing on this machine.
    pub fn new_icon(name: &str, fallback: &str, x: f32, y: f32, w: f32, h: f32) -> Adapted<Button> {
        Button::adapted(ButtonKind::CopyIcon, x, y, w, h).with_icon_name(name, fallback)
    }

    /// Whether an icon face is set (hosts size icon buttons square).
    pub fn has_icon(&self) -> bool {
        self.icon_face().is_some()
    }

    /// The face to draw: the live id for a named bundled glyph, else whatever
    /// the app handed to [`Adapted::with_icon`].
    ///
    /// Named glyphs re-resolve here instead of being captured, so the face
    /// survives a renderer rebuild — see the [`icon_name`] field.
    ///
    /// [`Adapted::with_icon`]: Adapted::<Button>::with_icon
    /// [`icon_name`]: Button::icon_name
    fn icon_face(&self) -> Option<(u32, f32, f32)> {
        match &self.icon_name {
            Some(name) => {
                crate::upload_icon(name, 32).map(|(id, w, h)| (id, w as f32, h as f32))
            }
            None => self.icon,
        }
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
        let (image, iw, ih) = self.icon_face()?;
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

    /// The label's width AS THE FRAME DRAWS IT: shaped by cosmic-text in
    /// [`Paint::widget_font`] — the configured button family (`list_font`
    /// for a ListRow), which is the font the adapter attaches to this
    /// widget's text when it re-emits `paint`'s label prim
    /// (`Adapted::paint_self` → `own_labels_with_prim_font(text_font())`).
    /// Same font string, same shaped-buffer cache the draw reads.
    ///
    /// Two wrong measures preceded this one. First `measure_text_width` in
    /// the button family: an SVG-inked extent, not the shaped run. Then
    /// shaping with no family (the UI sans), on the belief that the label
    /// was drawn in it — the `text_with(.., None, ..)` in `paint` below
    /// says so, but that `None` never reaches the frame: the paint walk
    /// swaps in `widget_font`. Under the default Berkeley Mono every label
    /// then drew ~20% wider than measured, so `intrinsic_size` cut the plate
    /// short and the glyphs ran over its right rim ("Load Images",
    /// "View: HTML" in cce-mail). Legacy hosts (`render_widget`) draw in
    /// `widget_font` too, so this measure holds on both paths. The inked
    /// measure is now only the fallback for a font system that shapes nothing.
    ///
    /// Blocking lock, as the context menu takes it: `try_lock` fell back to
    /// the wrong measure whenever another thread was shaping. The one site
    /// that holds this lock across a widget call (the flat host, around
    /// `prepare_text`) never reaches a Button's measure — keep it that way.
    fn label_width(&self, label: &str) -> f32 {
        let (family, size) = self.font();
        let font = Paint::widget_font(self);
        crate::geometry_font_system()
            .lock()
            .ok()
            .and_then(|mut fs| {
                crate::backend::text::shaped_cluster_offsets(&mut fs, label, size, font.as_deref())
                    .last()
                    .map(|&(_, total)| total)
            })
            .filter(|&w| w > 0.0)
            .unwrap_or_else(|| crate::widget::display::measure_text_width(label, &family, size))
    }

    /// The control plate this Button's `paint` draws — flush, at the button
    /// radius, its state colour as the face — or `None` when it draws none
    /// (flat styling, or a ListRow / MenuItem, transparent-until-hover
    /// surfaces that would wear a permanent carved ring on every idle row).
    pub fn plate(&self, rect: Rect) -> Option<crate::widget::ControlPlate> {
        if self.kind == ButtonKind::ListRow || self.kind == ButtonKind::MenuItem {
            return None;
        }
        let stance = if self.flat {
            crate::widget::PlateStance::Flat
        } else if self.raised() {
            crate::widget::PlateStance::Flush
        } else {
            return None;
        };
        let radius = crate::layout::button_corner_radius();
        // Keyboard focus lights the plate's own rim — the ring IS the silhouette.
        let tint = self.focused.then(crate::widget::ControlPlate::focus_tint);
        Some(
            crate::widget::ControlPlate::control(rect, radius, stance, crate::scene::Material::face(self.color()))
                .with_tint(tint),
        )
    }

    /// [`Button::plate`] as the legacy `(rect, corner radius, depth, face
    /// colour)` tuple — the flat-path bridge's view of the same plate.
    pub fn inset_face(&self, rect: Rect) -> Option<(Rect, f32, f32, [f32; 4])> {
        self.plate(rect).map(|p| (p.rect, p.radii.0, p.depth, p.face_fill()))
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

    /// Icon face from a bundled cce-icons glyph, by NAME — the form to prefer
    /// over [`with_icon`] whenever the artwork is one of cce-icons', because
    /// the id is re-resolved per read and so survives a renderer rebuild (see
    /// the [`icon_name`] field). `fallback` is the label drawn instead when
    /// the icon set is missing on this machine.
    ///
    /// [`with_icon`]: Adapted::<Button>::with_icon
    /// [`icon_name`]: Button::icon_name
    pub fn with_icon_name(mut self, name: &str, fallback: &str) -> Self {
        if crate::upload_icon(name, 32).is_some() {
            self.icon_name = Some(name.to_string());
            self
        } else {
            self.with_label(fallback)
        }
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

    /// Draw the face with no relief at all — see
    /// [`crate::widget::PlateStance::Flat`]. Overrides `with_raised`.
    pub fn with_flat(mut self, flat: bool) -> Self {
        self.flat = flat;
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

pub enum PageButton {
    Active,
    Inactive,
}
