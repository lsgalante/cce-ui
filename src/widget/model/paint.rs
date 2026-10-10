//! The paint concern: a widget's colour, the prims it emits for its laid-out rect, its silhouette,
//! whether it clips its children, its popover, and its fonts and text.

use super::*;

/// The paint concern — a widget's fill color, its own (non-recursive) geometry emission, and
/// whether it clips its children. Mirrors `WidgetHost::color` / `paint_self` / `clips_children`, but
/// [`paint`](Paint::paint) receives the laid-out `rect` as a parameter (the RFC shape) rather than
/// reading a stored rect, so a narrow widget carries no base of its own.
pub trait Paint {
    /// This widget's fill color (RGBA).
    fn color(&self) -> [f32; 4];

    /// Emit this node's OWN primitives (non-recursive) into `ctx`, given its final `rect`. The
    /// default paints a plain background from [`color`](Paint::color) — the common leaf case.
    /// Recursion into children and clipping are the paint walk's job ([`crate::widget::painter`]),
    /// not this method's.
    fn paint(&self, rect: Rect, ctx: &mut PaintCtx) {
        let color = self.color();
        if color[3].abs() > 0.001 {
            ctx.quad(rect, color);
        }
    }

    /// [`paint`](Paint::paint) with the live [`UiContext`] — what `paint_self` actually calls.
    /// The default forwards to `paint`, so ordinary widgets implement only that. Override this
    /// for composites whose own geometry aggregates hover/coverage-dependent child chrome that
    /// needs the context (ParametersBg): child-holding widgets that never entered the arena tree
    /// have no other way to reach it from the paint path.
    fn paint_ui(&self, _ui: &UiContext, rect: Rect, ctx: &mut PaintCtx) {
        self.paint(rect, ctx);
    }

    /// Whether the paint walk clips this widget's children to its `rect` (scroll/root plate
    /// containers). Default: no.
    fn clips_children(&self) -> bool {
        false
    }

    /// Corner rounding `(radius, per-corner flags)` of the widget's background, given its
    /// laid-out rect (MenuBar's corners depend on where it sits against its parent's edges).
    /// What a host drawing the widget's PLATE reads — `WidgetHostExt::corner_radii`, the
    /// designer's `append_widget_plate`, the flat-host bridge; the widget's own geometry is
    /// whatever [`paint`](Paint::paint) emits. Default: sharp corners.
    fn corner_style(&self, _rect: Rect) -> Option<(f32, (bool, bool, bool, bool))> {
        None
    }

    /// This widget's OWN popover (dropdown) rect, if one is open — hosts float it above
    /// z-ordered siblings (`register_popover` + `render_popovers`). Containers combine this
    /// with their children's popovers in the adapter. Default: none.
    fn popover(&self, _rect: Rect) -> Option<(f32, f32, f32, f32)> {
        None
    }

    /// Draw this widget's own popover (legacy `WidgetHost::render_popover`).
    fn draw_popover(&self, _rect: Rect, _pc: &mut dyn crate::scene::paint::RenderTarget) {}

    /// Solid border `(color, thickness)` of the widget's background quad. **Transitional**, like
    /// [`corner_style`](Paint::corner_style): `render_widget` gives a widget's background quad a
    /// border+inset treatment when this is `Some` — `Toggle`'s square mode depends on it.
    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        None
    }

    /// Font for this widget's text on legacy text paths (`render_widget` reads
    /// `WidgetHost::widget_font`). **Transitional.**
    fn widget_font(&self) -> Option<String> {
        None
    }

    /// Font for the widget's OWN prim-derived text on the scene paint walk (Phase 6). Defaults
    /// to [`widget_font`](Paint::widget_font) — one font for everything the widget draws, which
    /// is the legacy tuple-pipeline convention. A widget whose content text deliberately
    /// differs from its control font (TextBox with a customized `font_family`/`font_size`)
    /// overrides this; the detached base label always renders in `widget_font`. Only the paint
    /// walk consults it — the legacy `text_labels_with_font_and_bounds` getters keep serving
    /// `widget_font` so unmigrated apps stay byte-identical.
    fn text_font(&self) -> Option<String> {
        self.widget_font()
    }

    /// Shaping attributes (italic / weight) for the same content text
    /// [`text_font`](Paint::text_font) names — a TextBox showing a specific
    /// face of its family (the font picker's preview of a Bold or Thin cut).
    /// The detached base label never takes them.
    fn text_attrs(&self) -> crate::scene::paint::TextAttrs {
        crate::scene::paint::TextAttrs::default()
    }

    /// Receive the control label set on the wrapper via [`Adapted::with_label`] (and legacy
    /// `Control::set_label` paths). Widgets that paint their label themselves (inline-label
    /// widgets) store it here; the default discards it, leaving label drawing to the adapter's
    /// base-label machinery.
    fn sync_label(&mut self, _label: &str) {}

    /// Clip rect `[x1, y1, x2, y2]` for this widget's text on the legacy bounded-text paths
    /// (`text_labels_with_bounds` / `text_labels_with_font_and_bounds`). `None` (default) keeps
    /// the legacy behavior: unbounded, except inside a scroll ancestor. Graph clips its node
    /// names to its own rect.
    fn text_bounds(&self, _rect: Rect) -> Option<[f32; 4]> {
        None
    }

    /// Per-frame text shaping against the app's `FontSystem` (legacy `WidgetHost::prepare_text`
    /// overrides). TextBox measures its glyph advances here — load-bearing for cursor↔pixel
    /// mapping, not just a render cache. Receives the laid-out content rect. Default: nothing
    /// to shape.
    fn prepare_text(&mut self, _fs: &mut cosmic_text::FontSystem, _rect: Rect) {}

    /// Whether [`paint`](Paint::paint) emits the widget's ENTIRE subtree, so the paint walk
    /// must not also descend into its (ctx-linked) children — the legacy
    /// `WidgetHost::renders_own_subtree` contract. TreeList: its field widgets stay ctx-linked
    /// for event propagation, but their pixels come from `paint`'s own child pass (which
    /// gates the add-key popover box on the popover actually being open).
    fn paints_own_subtree(&self) -> bool {
        false
    }


    /// Whether the widget wears the shared focus highlight: the primary tint over its row
    /// span while it holds focus, drawn by the adapter over its background. Only the TextBox
    /// opts in (the focused editor's teal wash in the data editor). Default: none.
    fn legacy_focus_highlight(&self) -> bool {
        false
    }

    /// Whether this widget serves
    /// [`legacy_labels_with_font_and_bounds`](Paint::legacy_labels_with_font_and_bounds) —
    /// the text sibling of the dual-geometry escape hatch. The adapter's standard text bridge
    /// gives every own label ONE font ([`widget_font`](Paint::widget_font)) and ONE clip rect
    /// ([`text_bounds`](Paint::text_bounds)); a widget whose legacy
    /// `text_labels_with_font_and_bounds` override assigns them PER LABEL (ParametersBg clips
    /// each label to its viewport but its code editor's to the code box, in monospace) serves
    /// that view verbatim instead. Transitional — dies when `Prim::Text` carries font+bounds.
    fn serves_legacy_labels(&self) -> bool {
        false
    }

    /// The per-label font+bounds text view for legacy `text_labels_with_font_and_bounds`
    /// readers. Served as a FULL replacement: the adapter adds no child aggregation on top, so
    /// a container's implementation must include its children (as the legacy overrides did —
    /// hence the ctx, which the child recursion needs).
    fn legacy_labels_with_font_and_bounds(&self, _rect: Rect, _ctx: &UiContext) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        Vec::new()
    }

}
