//! `Group` — a lasso around registered widgets. See "Plates, wells and seams"
//! in `CLAUDE.md`: a group is a segment of the plate it sits on, parted from
//! the rest by a section carve rather than a seam.
//!
//! Unlike a container it OWNS nothing and lays nothing out: it is defined by
//! membership alone (widget ids), and its frame is derived every paint from
//! where the host's own layout put those members — the padded hull of their
//! rects, with a title tab flush on the top edge when it has a label. So a
//! host groups controls without restructuring its tree or its layout code.
//!
//! **Fit to plate.** Given the plate it sits on ([`Adapted<Group>::with_plate`]
//! and [`Adapted<Group>::with_fit`]), any side of the frame within `snap` of
//! that plate's edge (or past it) extends to the edge, one padding in, and a
//! corner whose two sides both snapped takes the plate's corner concentrically
//! — the Dropdown's corner-frame rule. A group on a narrow pane becomes that
//! pane's inset lining; on a wide one it stays a lasso around its members.
//!
//! **The frame is the section's frame.** Under `control_relief` it is the
//! settings app's union well ([`PaintCtx::section_well`]): the body carved as
//! a recess, the title tab carved with it as one shape, the throat between
//! them filleted. Without relief it is the section outline: a hairline ring
//! with a gap for the title. The section style keys
//! (`style.container.section.{depth,font,padding}`) apply.
//!
//! The group is never hittable and draws only through `paint_ui` (it needs
//! the context to find its members), so hosts on the legacy quad bridges see
//! it only through the prim replay — which carries every prim it emits
//! (recesses, fillets, vectors, text). `CCE_GROUP_DEBUG=1` prints each
//! paint's hull, plate and frame.

use crate::scene::layout::Rect;
use crate::scene::paint::{Cap, PaintCtx};
use crate::widget::{Adapted, Input, Layout, Paint, UiContext, WidgetId};

/// A group's frame for one paint: the body box, the title tab flush on its top
/// edge (when labelled), and the body's corner radii (TL, TR, BR, BL — a
/// fitted corner is the plate's, concentric).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupFrame {
    pub body: Rect,
    pub tab: Option<Rect>,
    pub radii: (f32, f32, f32, f32),
}

#[derive(Debug, Clone)]
pub struct Group {
    members: Vec<WidgetId>,
    label: Option<String>,
    /// Frame inset around the members' hull.
    padding: f32,
    /// The plate the group sits on, and its corner radius — the fit target.
    plate: Option<(Rect, f32)>,
    fit: bool,
    /// How close (px) a side must be to the plate's edge to snap to it.
    snap: f32,
}

impl Group {
    /// A group of `members` (registered widget ids); no plate, no fit, the
    /// section padding plus the relief width around the hull.
    pub fn new(members: Vec<WidgetId>) -> Adapted<Group> {
        Adapted::new(Group {
            members,
            label: None,
            // The section padding plus the wall's outer half: the carve straddles
            // the frame line, so this keeps its outer slope clear of the members.
            padding: crate::layout::section_padding() + crate::layout::bevel_width() * 0.5,
            plate: None,
            fit: false,
            snap: 12.0,
        })
    }

    pub fn members(&self) -> &[WidgetId] {
        &self.members
    }

    pub fn set_members(&mut self, members: Vec<WidgetId>) {
        self.members = members;
    }

    /// The plate this group sits on (rect, corner radius) — what `fit` snaps to.
    pub fn set_plate(&mut self, plate: Rect, corner_radius: f32) {
        self.plate = Some((plate, corner_radius));
    }

    pub fn set_fit(&mut self, fit: bool) {
        self.fit = fit;
    }

    /// The section carve's wall width: the DE relief scaled by the section depth
    /// multiplier, capped against the frame height (the ParametersBg rule).
    fn depth(&self, h: f32) -> f32 {
        (crate::layout::bevel_width() * crate::layout::section_depth()).min(h * 0.2).max(0.5)
    }

    fn title_font(&self) -> (String, f32) {
        let (fam, size) = crate::layout::parse_font_string(&crate::layout::section_label_font());
        (fam, size.unwrap_or(14.0))
    }

    /// The members' hull: the union of the registered, visible, on-screen members' rects.
    fn hull(&self, ui: &UiContext) -> Option<Rect> {
        let mut hull: Option<(f32, f32, f32, f32)> = None;
        for id in &self.members {
            let Some(ptr) = ui.tree.get_ptr(*id) else { continue };
            if ptr.is_null() {
                continue;
            }
            let w = unsafe { &*ptr };
            if !w.visible() {
                continue;
            }
            let (x, y, ww, hh) = w.rect();
            if ww <= 0.0 || hh <= 0.0 || x + ww <= 0.0 || y + hh <= 0.0 {
                continue;
            }
            // The rect IS the block — a labelled control's detached label strip
            // is already in it (`WidgetHost::label_strip`: "a widget's rect is
            // always its content plus this strip"), so the lasso wraps the label
            // by taking the rect as is. Subtracting the strip here again pushed
            // every labelled member's hull one strip too high, and the tab with it.
            hull = Some(match hull {
                None => (x, y, x + ww, y + hh),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + ww), y1.max(y + hh)),
            });
        }
        hull.map(|(x0, y0, x1, y1)| Rect { x: x0, y: y0, width: x1 - x0, height: y1 - y0 })
    }

    /// This paint's frame — `None` when no member is on screen.
    pub fn frame(&self, ui: &UiContext) -> Option<GroupFrame> {
        let hull = self.hull(ui)?;
        let p = self.padding;
        let mut body = Rect { x: hull.x - p, y: hull.y - p, width: hull.width + 2.0 * p, height: hull.height + 2.0 * p };
        let r = crate::layout::plate_corner_radius();
        let mut radii = (r, r, r, r);
        let title = self.label.as_deref().filter(|l| !l.is_empty());
        let (fam, size) = self.title_font();
        let tab_h = size + 8.0;
        if let (true, Some((plate, pr))) = (self.fit, self.plate) {
            // The frame's natural seat is one padding inside the plate's edge. A
            // side within `snap` of that seat — or past it — takes it, moving
            // OUTWARD only: a member that already sits in the padding zone keeps
            // the frame outside itself, up to the plate's own edge. A snapped
            // top leaves the tab room inside the plate.
            let (l, t) = (plate.x, plate.y);
            let (rr, b) = (plate.x + plate.width, plate.y + plate.height);
            let top_room = if title.is_some() { tab_h } else { 0.0 };
            let (x0, y0, x1, y1) = (body.x, body.y, body.x + body.width, body.y + body.height);
            let snap_l = x0 - (l + p) <= self.snap;
            let snap_t = y0 - (t + p + top_room) <= self.snap;
            let snap_r = (rr - p) - x1 <= self.snap;
            let snap_b = (b - p) - y1 <= self.snap;
            let nx0 = if snap_l { x0.min(l + p).max(l) } else { x0 };
            let ny0 = if snap_t { y0.min(t + p + top_room).max(t + top_room) } else { y0 };
            let nx1 = if snap_r { x1.max(rr - p).min(rr) } else { x1 };
            let ny1 = if snap_b { y1.max(b - p).min(b) } else { y1 };
            body = Rect { x: nx0, y: ny0, width: nx1 - nx0, height: ny1 - ny0 };
            // A corner on the plate's corner follows its curve, concentrically —
            // at the inset the two sides actually landed at.
            let cr = |a: f32, bb: f32| (pr - a.max(bb)).max(0.0);
            radii = (
                if snap_l && snap_t { cr(nx0 - l, ny0 - top_room - t) } else { r },
                if snap_r && snap_t { cr(rr - nx1, ny0 - top_room - t) } else { r },
                if snap_r && snap_b { cr(rr - nx1, b - ny1) } else { r },
                if snap_l && snap_b { cr(nx0 - l, b - ny1) } else { r },
            );
        }
        let tab = title.map(|l| {
            let tw = (crate::widget::display::measure_text_width(l, &fam, size) + 16.0).clamp(8.0, body.width.max(8.0));
            // Flush on the body's top edge, at its left (the settings' tab).
            Rect { x: body.x, y: body.y - tab_h, width: tw, height: tab_h }
        });
        if std::env::var_os("CCE_GROUP_DEBUG").is_some() {
            eprintln!("[group] {:?} hull={:?} plate={:?} fit={} body={:?} tab={:?}", self.label, hull, self.plate, self.fit, body, tab);
        }
        Some(GroupFrame { body, tab, radii })
    }
}

impl Adapted<Group> {
    pub fn with_padding(mut self, padding: f32) -> Self {
        self.padding = padding;
        self
    }

    /// The plate this group sits on (rect, corner radius) — see [`Group::set_plate`].
    pub fn with_plate(mut self, plate: Rect, corner_radius: f32) -> Self {
        self.plate = Some((plate, corner_radius));
        self
    }

    /// Fit to the plate: sides near its edges snap to them (see the module doc).
    pub fn with_fit(mut self, fit: bool) -> Self {
        self.fit = fit;
        self
    }

    pub fn with_snap(mut self, snap: f32) -> Self {
        self.snap = snap;
        self
    }
}

impl Layout for Group {
    /// The title is part of the frame (the tab), not a detached control label.
    fn inline_label(&self) -> bool {
        true
    }
}

impl Paint for Group {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }

    fn sync_label(&mut self, label: &str) {
        self.label = Some(label.to_string());
    }

    /// The title is authored here, with its font, in `paint_ui` — it must pass
    /// through the paint walk verbatim rather than be re-derived from `paint`
    /// (which, without the context, has nothing to say).
    fn paints_own_subtree(&self) -> bool {
        true
    }

    /// Nothing without the context: the frame is where the members are.
    fn paint(&self, _rect: Rect, _ctx: &mut PaintCtx) {}

    fn paint_ui(&self, ui: &UiContext, _rect: Rect, ctx: &mut PaintCtx) {
        let Some(f) = self.frame(ui) else { return };
        let (fam, size) = self.title_font();
        if crate::layout::control_relief() {
            ctx.section_well(f.body, f.tab, f.radii, self.depth(f.body.height));
        } else {
            // The section outline: a hairline ring, the top edge parted for the title.
            let c = [0.25, 0.25, 0.35, 1.0];
            let (x0, y0) = (f.body.x, f.body.y);
            let (x1, y1) = (f.body.x + f.body.width, f.body.y + f.body.height);
            match f.tab {
                Some(t) => {
                    let (gap0, gap1) = (t.x + 2.0, t.x + t.width - 2.0);
                    if gap0 > x0 {
                        ctx.vector(x0, y0, gap0, y0, 1.0, c, Cap::Flat);
                    }
                    if x1 > gap1 {
                        ctx.vector(gap1, y0, x1, y0, 1.0, c, Cap::Flat);
                    }
                }
                None => ctx.vector(x0, y0, x1, y0, 1.0, c, Cap::Flat),
            }
            ctx.vector(x0, y1, x1, y1, 1.0, c, Cap::Flat);
            ctx.vector(x0, y0, x0, y1, 1.0, c, Cap::Flat);
            ctx.vector(x1, y0, x1, y1, 1.0, c, Cap::Flat);
        }
        if let (Some(label), Some(t)) = (self.label.as_deref(), f.tab) {
            let color = crate::color::control_label_color_for_state(false, false);
            let ty = if crate::layout::control_relief() { t.y + 4.0 } else { t.y + t.height - size - 2.0 };
            ctx.text_with(label.to_string(), t.x + 8.0, ty, size, color, Some(format!("{fam} {size}")), None);
        }
    }
}

impl Input for Group {
    /// Never hittable: the members underneath take the pointer.
    fn hit(&self, _rect: Rect, _x: f32, _y: f32) -> bool {
        false
    }

    fn blocks_root_plate_drag(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::widget::{Button, Slider, WidgetHost};

    fn register(ctx: &mut UiContext, w: &mut dyn WidgetHost) -> WidgetId {
        let id = w.base().id();
        let ptr = unsafe { std::mem::transmute::<*mut dyn WidgetHost, *mut (dyn WidgetHost + 'static)>(w as *mut dyn WidgetHost) };
        ctx.register_widget(id, ptr);
        id
    }

    /// The frame is the members' hull plus the padding; a parked member is not in it.
    #[test]
    fn frame_is_the_padded_hull_of_the_members() {
        let mut ctx = UiContext::new();
        let mut a = Button::new(0.0, 0.0, 80.0, 24.0);
        let mut b = Button::new(0.0, 0.0, 80.0, 24.0);
        let mut parked = Button::new(0.0, 0.0, 1.0, 1.0);
        WidgetHost::set_rect(&mut a, 100.0, 50.0, 80.0, 24.0);
        WidgetHost::set_rect(&mut b, 200.0, 90.0, 60.0, 30.0);
        WidgetHost::set_rect(&mut parked, -1000.0, -1000.0, 1.0, 1.0);
        let ids = vec![register(&mut ctx, &mut a), register(&mut ctx, &mut b), register(&mut ctx, &mut parked)];
        let g = Group::new(ids).with_padding(10.0);
        let f = g.inner().frame(&ctx).expect("members on screen");
        assert_eq!((f.body.x, f.body.y, f.body.width, f.body.height), (90.0, 40.0, 180.0, 90.0));
        assert!(f.tab.is_none(), "no label, no tab");
    }

    /// A labelled member's rect already holds its label strip; the hull takes the
    /// rect as it is, not the rect less another strip.
    #[test]
    fn a_labelled_members_strip_is_counted_once() {
        let mut ctx = UiContext::new();
        let mut s = Slider::new().with_label("Amount");
        let strip = s.label_strip();
        assert!(strip > 0.0, "a detached label has a strip");
        // The block: strip + control, as `layout` lands it.
        WidgetHost::set_rect(&mut s, 100.0, 50.0, 160.0, 16.0 + strip);
        let ids = vec![register(&mut ctx, &mut s)];
        let g = Group::new(ids).with_padding(10.0);
        let f = g.inner().frame(&ctx).unwrap();
        assert_eq!(f.body.y, 40.0, "one padding above the block, not a strip more");
        assert_eq!(f.body.height, 16.0 + strip + 20.0);
    }

    /// Fit to plate: a side near the plate's edge takes it (one padding in), a
    /// far side keeps the hull, and a corner on the plate's corner is concentric.
    #[test]
    fn fit_snaps_near_sides_to_the_plate() {
        let mut ctx = UiContext::new();
        let mut a = Button::new(0.0, 0.0, 80.0, 24.0);
        WidgetHost::set_rect(&mut a, 20.0, 20.0, 80.0, 24.0);
        let ids = vec![register(&mut ctx, &mut a)];
        let plate = Rect { x: 0.0, y: 0.0, width: 400.0, height: 300.0 };
        let g = Group::new(ids).with_padding(10.0).with_plate(plate, 16.0).with_fit(true).with_snap(24.0);
        let f = g.inner().frame(&ctx).unwrap();
        // Left/top hull edges (10, 10) sit on the plate's inset seat (10, 10): snapped.
        assert_eq!((f.body.x, f.body.y), (10.0, 10.0));
        // A member inside the padding zone keeps the frame outside itself, up to the edge.
        let mut edge = Button::new(0.0, 0.0, 80.0, 24.0);
        WidgetHost::set_rect(&mut edge, 4.0, 60.0, 80.0, 24.0);
        let eid = register(&mut ctx, &mut edge);
        let ge = Group::new(vec![eid]).with_padding(10.0).with_plate(plate, 16.0).with_fit(true).with_snap(24.0);
        let fe = ge.inner().frame(&ctx).unwrap();
        assert_eq!(fe.body.x, 0.0, "clamped to the plate's own edge, never inside the member");
        // Right/bottom are far from the plate: the hull's own.
        assert_eq!((f.body.x + f.body.width, f.body.y + f.body.height), (110.0, 54.0));
        assert_eq!(f.radii.0, 6.0, "the top-left corner is the plate's, concentric (16 - 10)");
        assert_eq!(f.radii.2, crate::layout::plate_corner_radius(), "the far corner keeps the section radius");
        // Unfitted, the same group hugs its member.
        let loose = Group::new(vec![a.base().id()]).with_padding(10.0).with_plate(plate, 16.0);
        assert_eq!(loose.inner().frame(&ctx).unwrap().body.x, 10.0);
        assert_eq!(loose.inner().frame(&ctx).unwrap().radii.0, crate::layout::plate_corner_radius());
    }

    /// A labelled group carries a title tab flush on the body's top edge.
    #[test]
    fn a_label_adds_a_tab_on_the_top_edge() {
        let mut ctx = UiContext::new();
        let mut a = Button::new(0.0, 0.0, 80.0, 24.0);
        WidgetHost::set_rect(&mut a, 100.0, 100.0, 80.0, 24.0);
        let ids = vec![register(&mut ctx, &mut a)];
        let g = Group::new(ids).with_label("Group");
        let f = g.inner().frame(&ctx).unwrap();
        let t = f.tab.expect("a tab");
        assert_eq!(t.x, f.body.x);
        assert_eq!(t.y + t.height, f.body.y, "flush on the top edge");
    }
}
