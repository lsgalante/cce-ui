//! `Dialog` — a modal plate around registered widgets (`docs/rfc-accessibility-locale.md`,
//! phase 3).
//!
//! Like a [`Group`](super::Group) it OWNS nothing and lays nothing out: the host lays its
//! members out where it wants them, and the dialog's plate is their padded hull with a title
//! band above. What makes it a dialog is what [`Adapted<Dialog>::open`] does to the
//! [`UiContext`]:
//!
//! - **Focus is trapped.** The Tab walk visits only the members (and their embedded
//!   children), wrapping inside; focus moves to the first of them on open and goes back to
//!   whatever had it on [`close`](Adapted::<Dialog>::close).
//! - **What is behind takes nothing.** Every widget outside reads as covered
//!   (`UiContext::is_coordinate_covered`, which every widget's hit test and hover ask), so
//!   neither a press nor a hover reaches it; [`set_backdrop`](Adapted::<Dialog>::set_backdrop)
//!   dims it.
//! - **A screen reader is told.** The members are linked as the dialog's children, so the
//!   accessibility tree nests them under a `Dialog` node marked modal, named by the title.
//!
//! **Paint the dialog, not its members**: it paints its plate and then each member, so they
//! stand on it. Paint it after everything it covers. Escape is the host's to read: close the
//! dialog on it as on Cancel.
//!
//! The plate is the menu's material (`Material::menu`, the `style.surface.menu` block): a
//! dialog is a popover that holds controls instead of rows.

use crate::scene::layout::Rect;
use crate::scene::paint::PaintCtx;
use crate::widget::{Adapted, Input, Layout, Paint, UiContext, WidgetHost, WidgetId};

/// What dims the window behind an open dialog with a backdrop.
const SCRIM: [f32; 4] = [0.0, 0.0, 0.0, 0.5];

#[derive(Debug, Clone)]
pub struct Dialog {
    members: Vec<WidgetId>,
    title: Option<String>,
    /// Plate inset around the members' hull.
    padding: f32,
    /// The window rect to dim behind the plate, while open.
    backdrop: Option<Rect>,
    open: bool,
}

impl Dialog {
    /// A closed dialog titled by its label (`with_label`), at the pane rung's padding.
    pub fn new() -> Adapted<Dialog> {
        Adapted::new(Dialog {
            members: Vec::new(),
            title: None,
            padding: crate::layout::plate_padding(),
            backdrop: None,
            open: false,
        })
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn members(&self) -> &[WidgetId] {
        &self.members
    }

    fn title_font(&self) -> (String, f32) {
        let (fam, size) = crate::layout::parse_font_string(&crate::layout::section_label_font());
        (fam, size.unwrap_or(14.0))
    }

    /// The title band's height — zero without a title.
    fn title_height(&self) -> f32 {
        match self.title.as_deref().filter(|t| !t.is_empty()) {
            Some(_) => self.title_font().1 + crate::layout::control_gap(),
            None => 0.0,
        }
    }

    /// The room the plate takes ABOVE its members: the padding and the title band. A host
    /// lays the first member out this far below where it wants the plate's top.
    pub fn headroom(&self) -> f32 {
        self.padding + self.title_height()
    }

    /// The padding between the members and the plate's edge.
    pub fn padding(&self) -> f32 {
        self.padding
    }

    /// The plate: the members' hull, padded, with the title band on top. `None` when no
    /// member is on screen.
    pub fn plate(&self, ui: &UiContext) -> Option<Rect> {
        let mut hull: Option<(f32, f32, f32, f32)> = None;
        for &id in &self.members {
            let Some(w) = ui.get_widget(id) else { continue };
            let (x, y, ww, hh) = w.rect();
            if !w.visible() || ww <= 0.0 || hh <= 0.0 || x < -9000.0 || y < -9000.0 {
                continue;
            }
            let mut grow = |x: f32, y: f32, w: f32, h: f32| {
                hull = Some(match hull {
                    None => (x, y, x + w, y + h),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h)),
                });
            };
            grow(x, y, ww, hh);
            if let Some(l) = w.detached_label_rect() {
                grow(l.x, l.y, l.width, l.height);
            }
        }
        let (x0, y0, x1, y1) = hull?;
        let (p, top) = (self.padding, self.headroom());
        Some(Rect { x: x0 - p, y: y0 - top, width: x1 - x0 + 2.0 * p, height: y1 - y0 + top + p })
    }
}

impl Adapted<Dialog> {
    /// Open around `members` (registered widget ids, laid out by the host): link them as
    /// this dialog's children, trap focus among them and cover everything else (see the
    /// module doc). Opening an open dialog changes its members and keeps where focus goes
    /// back to.
    pub fn open(&mut self, ctx: &mut UiContext, members: Vec<WidgetId>) {
        let id = self.base().id();
        for &old in &self.members {
            ctx.unlink_child(id, old);
        }
        for &m in &members {
            ctx.link_ids(id, m);
        }
        self.members = members.clone();
        self.open = true;
        self.fit(ctx);
        ctx.open_modal(id, members);
    }

    /// Close: unlink the members, release the trap and give focus back.
    pub fn close(&mut self, ctx: &mut UiContext) {
        if !self.open {
            return;
        }
        let id = self.base().id();
        for &m in &self.members {
            ctx.unlink_child(id, m);
        }
        self.open = false;
        self.backdrop = None;
        ctx.close_modal(id);
    }

    /// Take the plate's rect as the widget's own (its hit area and its accessible bounds),
    /// from where the members now are. Call after laying them out.
    pub fn fit(&mut self, ctx: &UiContext) {
        match self.inner().plate(ctx).filter(|_| self.open) {
            Some(r) => self.set_rect(r.x, r.y, r.width, r.height),
            None => self.set_rect(0.0, 0.0, 0.0, 0.0),
        }
    }

    /// Dim `window` (the window's rect) behind the plate while open; `None` for no dimming.
    pub fn set_backdrop(&mut self, window: Option<Rect>) {
        self.inner_mut().backdrop = window;
    }

    pub fn with_padding(mut self, padding: f32) -> Self {
        self.inner_mut().padding = padding;
        self
    }
}

impl Layout for Dialog {
    /// The title is the plate's own band, not a detached control label.
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for Dialog {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }

    fn sync_label(&mut self, label: &str) {
        self.title = Some(label.to_string());
    }

    /// It paints its members itself, standing on its plate; the walk must not paint them
    /// again.
    fn paints_own_subtree(&self) -> bool {
        true
    }

    /// Nothing without the context: the plate is where the members are.
    fn paint(&self, _rect: Rect, _ctx: &mut PaintCtx) {}

    fn paint_ui(&self, ui: &UiContext, _rect: Rect, ctx: &mut PaintCtx) {
        if !self.open {
            return;
        }
        if let Some(b) = self.backdrop {
            ctx.quad(b, SCRIM);
        }
        let Some(plate) = self.plate(ui) else { return };
        let r = crate::layout::menu_corner_radius().min(plate.height * 0.5);
        let depth = crate::layout::bevel_width().min(plate.height * 0.2);
        if crate::color::menu_color()[3] > 0.001 {
            ctx.plate(plate, (r, r, r, r), &crate::scene::material::Material::menu(), depth);
        } else {
            let (plateau, radii) = crate::layout::carve_inside(plate, (r, r, r, r), depth);
            ctx.boss(plateau, radii, depth);
        }
        if let Some(title) = self.title.as_deref().filter(|t| !t.is_empty()) {
            let (fam, size) = self.title_font();
            let color = crate::color::control_label_color_for_state(false, false);
            let (x, y) = (plate.x + self.padding, plate.y + self.padding);
            ctx.text_with(title.to_string(), x, y, size, color, Some(format!("{fam} {size}")), None);
        }
        for &id in &self.members {
            if let Some(w) = ui.get_widget(id) {
                crate::scene::painter::paint_root_into(ui, w, ctx);
            }
        }
    }
}

impl Input for Dialog {
    /// The plate takes a press nothing on it took, so it never falls to what is behind.
    fn hit(&self, rect: Rect, x: f32, y: f32) -> bool {
        self.open && x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
    }

    /// A press on the plate is not a drag of the window.
    fn blocks_root_plate_drag(&self) -> bool {
        self.open
    }

    fn a11y_role(&self) -> Option<accesskit::Role> {
        Some(accesskit::Role::Dialog)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Button, Owned, TextBox};

    /// A dialog traps the Tab walk among its members, covers what is behind it, and gives
    /// focus back on close.
    #[test]
    fn a_dialog_traps_focus_covers_the_window_and_gives_focus_back() {
        let mut ctx = UiContext::new();
        let mut behind = Owned::new(Button::new(10.0, 10.0, 80.0, 24.0).with_label("Behind"));
        let mut name = Owned::new(TextBox::new(String::new()).with_label("Name"));
        name.set_rect(120.0, 120.0, 200.0, 24.0);
        let mut ok = Owned::new(Button::new(120.0, 160.0, 80.0, 24.0).with_label("OK"));
        let mut dialog = Owned::new(Dialog::new().with_label("Rename"));
        ctx.register_host(&mut behind);
        ctx.register_host(&mut dialog);
        ctx.register_host(&mut name);
        ctx.register_host(&mut ok);
        ctx.set_focused_id(behind.base().id());

        dialog.open(&mut ctx, vec![name.base().id(), ok.base().id()]);
        assert_eq!(ctx.modal_owner(), Some(dialog.base().id()));
        assert_eq!(ctx.focused_widget, Some(name.base().id()), "focus moves in");
        ctx.focus_step(false);
        assert_eq!(ctx.focused_widget, Some(ok.base().id()));
        ctx.focus_step(false);
        assert_eq!(ctx.focused_widget, Some(name.base().id()), "the walk wraps inside, never to Behind");
        assert!(!behind.hit_test(20.0, 20.0, &ctx), "behind the dialog nothing hits");
        assert!(ok.hit_test(130.0, 170.0, &ctx), "inside it does");

        let plate = dialog.inner().plate(&ctx).expect("a plate round the members");
        let (p, top) = (dialog.inner().padding(), dialog.inner().headroom());
        assert_eq!((plate.x, plate.y), (120.0 - p, 120.0 - top), "the title band is above the members");
        assert_eq!(dialog.rect(), (plate.x, plate.y, plate.width, plate.height), "opening fits it");
        assert_eq!(ctx.tree.child_ids(dialog.base().id()), vec![name.base().id(), ok.base().id()]);

        dialog.close(&mut ctx);
        assert_eq!(ctx.modal_owner(), None);
        assert_eq!(ctx.focused_widget, Some(behind.base().id()), "focus goes back");
        assert!(behind.hit_test(20.0, 20.0, &ctx));
        assert!(ctx.tree.child_ids(dialog.base().id()).is_empty(), "the members are unlinked");
    }

    /// The dialog paints its plate, then its members on it.
    #[test]
    fn a_dialog_paints_its_plate_then_its_members() {
        use crate::scene::paint::Prim;
        let mut ctx = UiContext::new();
        let mut ok = Owned::new(Button::new(120.0, 160.0, 80.0, 24.0).with_label("OK"));
        let mut dialog = Owned::new(Dialog::new().with_label("Rename"));
        ctx.register_host(&mut dialog);
        ctx.register_host(&mut ok);
        let mut pc = PaintCtx::new();
        crate::scene::painter::paint_root_into(&ctx, &*dialog, &mut pc);
        assert!(pc.finish().items.is_empty(), "closed, it paints nothing");

        dialog.open(&mut ctx, vec![ok.base().id()]);
        let mut pc = PaintCtx::new();
        crate::scene::painter::paint_root_into(&ctx, &*dialog, &mut pc);
        let prims: Vec<Prim> = pc.finish().items.into_iter().map(|i| i.prim).collect();
        let texts: Vec<&str> = prims
            .iter()
            .filter_map(|p| if let Prim::Text { text, .. } = p { Some(text.as_str()) } else { None })
            .collect();
        assert_eq!(texts, ["Rename", "OK"], "the title, then the member on the plate");
    }
}
