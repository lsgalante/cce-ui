//! `WidgetHost`, the one surface the machinery (routing, the paint walk, the render loop) sees, and
//! `WidgetHostExt`, the reads derived from a host's narrow models, blanket-implemented for every
//! host; the shown-prim helpers composites read a child through; and `NoModel`.

use super::*;

/// The single host surface every widget presents to the machinery (context routing, the
/// paint walk, the render loop, app dyn broadcasts). **Formerly `Element`**, the ~125-method
/// god-trait — renamed at the 6bd flip once census-driven shrink batches brought it down to
/// the measured blueprint. `Adapted<W>` is the one production implementor; concrete behavior
/// lives on the narrow `Layout`/`Paint`/`Input` traits it wraps. The direct-dispatch and
/// value blocks shrink further as apps move to routed events / concrete slots.
pub trait WidgetHost {
    /// The widget's shared base state — GUARANTEED (the flip): the `Option` escape hatch
    /// and its `WidgetId(0)` sentinel class are gone. `Adapted` (the one production
    /// implementor) always owns a base; test shims carry one via `impl_widget_base!`.
    fn base(&self) -> &Widget;
    fn base_mut(&mut self) -> &mut Widget;

    /// The height of the detached-label strip above this widget's content: zero for
    /// unlabeled widgets and for those whose base label IS their content
    /// ([`Layout::inline_label`]). A widget's rect is always its content plus this
    /// strip — `set_rect` takes that block, `layout` lands the content at the origin
    /// and hangs the strip above it. A strategy reserves that
    /// row above every child's content (`container_layout::label_lead`) and puts
    /// `layout::CONTROL_GAP` between the blocks.
    fn label_strip(&self) -> f32 { self.base().label_offset() }

    /// Where the detached label is drawn: the strip above the content, as wide as the
    /// label's text. `None` for an unlabeled widget and for an inline label. The label
    /// may be wider than the widget's rect (a StatusDot's, a Checkbox's) — the rect is
    /// the content's width, and the text runs past it — so anything wrapping a widget
    /// as a block (a `Group`'s hull) unions this with the rect.
    fn detached_label_rect(&self) -> Option<crate::scene::layout::Rect> { None }


    // Required (the flip): the old defaults manufactured DummyAny stand-ins nothing
    // could legitimately use. `impl_widget_base!` provides both. `as_ptr`/`as_ptr_mut`
    // are GONE from the trait (the plumbing retype): a pointer to a widget you already
    // hold is a plain cast (`w as *mut (dyn WidgetHost + 'static)`); concrete
    // registration sites ride the inherent `Adapted<W>` methods (the registration
    // bridge — derived from a live borrow, never stored beyond the registry).
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// The widget's own model, as its narrow traits: what [`WidgetHostExt`] reads its
    /// one-line answers off (`Adapted` hands out its inner widget; a test shim with no
    /// model gets [`NoModel`]'s defaults).
    fn layout_model(&self) -> &dyn Layout {
        &NoModel
    }
    fn paint_model(&self) -> &dyn Paint {
        &NoModel
    }
    fn input_model(&self) -> &dyn Input {
        &NoModel
    }
    fn input_model_mut(&mut self) -> &mut dyn Input {
        // A zero-sized value: leaking it allocates nothing.
        Box::leak(Box::new(NoModel))
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        // The default serves test shims only (Adapted overrides this): base hover
        // bookkeeping on moves, tick forwarding, everything else inert — the old
        // per-method dispatch died with the direct-dispatch entry points (6bd collapse).
        match event {
            Event::PointerMove { x, y, .. } => {
                let (px, py) = (*x, *y);
                ctx.set_cursor_pos(px, py);
                let was = self.base().hovered;
                let is_hit = self.hit_test(px, py, ctx);
                self.base_mut().hovered = is_hit;
                was != is_hit
            }
            Event::Tick(dt) => {
                self.tick(*dt, ctx)
            }
            _ => false,
        }
    }

    fn measure(&self, constraints: LayoutConstraints, _ctx: &UiContext) -> Size {
        let (_, _, w, h) = self.rect();
        let pref_h = self.preferred_height().unwrap_or(h);
        
        let width = w.clamp(constraints.min_width, constraints.max_width);
        let height = pref_h.clamp(constraints.min_height, constraints.max_height);
        
        Size { width, height }
    }

    /// Land the CONTENT box at `origin`, the label strip hanging above it — the one
    /// placement contract (`Adapted` repeats it over its measured content size).
    fn layout(&mut self, origin: Point, constraints: LayoutConstraints, ctx: &mut UiContext) {
        let size = self.measure(constraints, ctx);
        let strip = self.label_strip();
        self.set_rect(origin.x, origin.y - strip, size.width, size.height + strip);
    }

    fn rect(&self) -> (f32, f32, f32, f32) {
        let b = self.base();
        (b.x, b.y, b.w, b.h)
    }


    // The value/polling block (`get_value_string`/`set_value_string`/`take_change`/
    // `take_click`/`value`/`set_text`/`set_selected`) is GONE from the trait (6bd value
    // shrink): apps drain widget state through the concrete inherent `Adapted<W>` methods
    // (which forward to the narrow `Input` hooks). The last dyn readers went concrete-slot
    // (TI's roster drain, cloud's JsonControl, designer's pane-focus sync).


    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let b = self.base_mut();
        b.x = x;
        b.y = y;
        b.w = w;
        b.h = h;
    }

    fn set_row_rect(&mut self, x: f32, w: f32) {
        let b = self.base_mut();
        b.row_x = x;
        b.row_w = w;
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        if ctx.is_coordinate_covered(self.base().id(), px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        if w <= 0.0 || h <= 0.0 {
            return false;
        }
        let b = self.base();
        let (hx, hw) = if b.row_w > 0.0 { (b.row_x, b.row_w) } else { (x, w) };
        px >= hx && px <= hx + hw && py >= y && py <= y + h
    }

    // The direct-dispatch entry points (`cursor_moved`, `on_cursor_moved`, `mouse_input`,
    // `mouse_wheel`, `keyboard_input`, `drag_begin`/`drag_update`/`drag_end`) are GONE from
    // the trait (6bd collapse): every event delivery goes through `handle_event` — the entry
    // points live on as inherent `Adapted<W>` methods for concrete in-crate forwards.

    // `hovered`/`set_hovered` are GONE from the trait (6bd batch 2): the state is the base
    // `Widget::hovered` flag, read/written directly by the defaults above; Button/Checkbox
    // keep inherent accessors for immediate-mode hosts.



    // `draggable`/`is_dragging` are GONE from the trait (the ControlPanel endgame
    // removed their last stored-child-pointer consumer): the drag queries are concrete
    // inherent `Adapted<W>` reads; index-driven rosters (TI, designer) route them
    // through per-slot matches like the other value drains.

    

    /// Emit this widget's OWN primitives (non-recursive) into the single paint pass (Phase 3).
    /// Every production host overrides this (`Adapted`); the
    /// default serves a host with no widget of its own (the test shims): its plate — a solid
    /// border, or a rounded fill in its colour where its corner style rounds — then what its
    /// model paints, text aside. Recursion into children and clipping are the paint walk's
    /// (`scene::painter`), not this.
    fn paint_self(&self, ui: &UiContext, ctx: &mut crate::scene::paint::PaintCtx) {
        use crate::scene::layout::Rect;
        use crate::scene::paint::{PaintCtx, Prim};
        let (x, y, w, h) = self.rect();
        let rect = Rect { x, y, width: w, height: h };
        let color = self.color();
        if let Some((border_color, thickness)) = self.solid_border() {
            let cr = self.corner_radii();
            ctx.border(rect, (cr.top_left, cr.top_right, cr.bottom_right, cr.bottom_left), color, border_color, thickness);
        } else if let Some((radius, corners)) = self.paint_model().corner_style(self.content_rect()) {
            if corners != (false, false, false, false) && color[3].abs() > 0.001 {
                ctx.rounded_rect(rect, radius, corners, color);
            }
        }
        if !self.visible() {
            return;
        }
        // Text: none. A host with text of its own overrides this; emitting the model's text
        // here would double what the walk's descent draws for a container (the Phase 6d trap).
        let mut tmp = PaintCtx::new();
        self.paint_model().paint_ui(ui, self.content_rect(), &mut tmp);
        for item in tmp.finish().items {
            // `replay` emits every prim but Text, which it hands back; dropping it is the point.
            let _text: Option<Prim> = ctx.replay(item.prim);
        }
    }





    // The per-widget text getters (text_labels / text_labels_with_bounds /
    // text_labels_with_font_and_bounds / get_text_items) are GONE: every widget emits
    // its own text as display-list prims via paint_self (Adapted::paint_self). The
    // deleted default's base-label synthesis lives on in Adapted's base-label fallback,
    // and its scroll-ancestor clamp in scene::painter::scroll_ancestor_text_bounds.

    fn type_name(&self) -> &'static str {
        let full_name = std::any::type_name::<Self>();
        full_name.split("::").last().unwrap_or("Widget")
    }
    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> { None }
    fn render_popover(&self, _pc: &mut dyn crate::layout::RenderTarget) {}

    fn focus(&mut self) {
        self.base_mut().focused = true;
    }
    fn unfocus(&mut self) {
        self.base_mut().focused = false;
    }
    fn focused(&self, ctx: &UiContext) -> bool {
        ctx.is_focused_id(self.base().id())
    }
    fn prepare_text(&mut self, _fs: &mut cosmic_text::FontSystem) {}

    fn set_visible(&mut self, _visible: bool) {}
    fn visible(&self) -> bool { true }
    fn tick(&mut self, _dt: f32, _ctx: &mut UiContext) -> bool { false }

    /// Put the widget's embedded children (`widget::Embedded`) into `ctx`: what
    /// `UiContext::insert` calls once the widget is in. The adapter forwards to
    /// `Layout::register_embedded_children`, which also runs on every layout and tick.
    fn attach_embedded(&mut self, _ctx: &mut UiContext) {}

    /// Take the widget's embedded children back out of `ctx`, so it leaves whole: what
    /// `UiContext::remove` calls before the widget goes. The adapter forwards to
    /// `Layout::release_embedded_children`.
    fn release_embedded(&mut self, _ctx: &mut UiContext) {}

    // `set_parent`/`add_child` are GONE from the trait (6bd batch 4): linking is a tree
    // operation, by id (`UiContext::link_ids`, `ctx.tree`). `parent`/`children` are GONE
    // too: tree structure is read off `ctx.tree` (`parent_id` / `child_ids`), and no trait
    // method returns a raw pointer.









    /// The parts a screen reader sees as nodes of their own (`Input::a11y_items`).
    fn a11y_items(&self) -> Vec<crate::a11y::A11yItem> {
        Vec::new()
    }


}

/// The host surface that does not need a slot of its own in `WidgetHost`: what a widget's
/// narrow traits answer ([`Input`], [`Paint`], [`Layout`], reached through the host's
/// [`WidgetHost::input_model`] / [`paint_model`](WidgetHost::paint_model) /
/// [`layout_model`](WidgetHost::layout_model)) and what is derived from the host's own state.
/// Implemented for every host, `dyn WidgetHost` included, so `w.focus_role()` reads as it
/// always did — with this trait in scope (`use cce_ui::widget::WidgetHostExt`). Until
/// 2026-10-08 each of these was a `WidgetHost` method that `Adapted` overrode with a one-line
/// forward.
pub trait WidgetHostExt: WidgetHost {
    /// This widget's part in keyboard navigation — `Input::focus_role` through
    /// the adapter; `FocusRole::None` for anything that is not a plate or a well.
    fn focus_role(&self) -> FocusRole {
        self.input_model().focus_role()
    }

    /// Whether, focused, it takes Tab itself instead of the Tab walk (`Input::keeps_tab`).
    fn keeps_tab(&self) -> bool {
        self.input_model().keeps_tab()
    }

    fn blocks_root_plate_drag(&self) -> bool {
        self.input_model().blocks_root_plate_drag()
    }

    fn wants_tick(&self) -> bool {
        self.input_model().wants_tick()
    }

    fn is_scrollable(&self) -> bool {
        self.input_model().scrollable()
    }

    /// An explicit accessibility role, overriding the guess `crate::a11y::role_for` makes
    /// from the widget's type and focus role. Default `None`.
    fn a11y_role(&self) -> Option<accesskit::Role> {
        self.input_model().a11y_role()
    }

    /// The widget's value for assistive technology: a field's text, a slider's number, a
    /// check box's "true" / "false". Default `None`.
    fn a11y_value(&self) -> Option<String> {
        self.input_model().value_string()
    }

    /// The `(min, max, step)` an assistive tool may set the value in (`Input::a11y_range`).
    fn a11y_range(&self) -> Option<(f64, f64, f64)> {
        self.input_model().a11y_range()
    }

    /// Set the value an assistive tool asked for (`Input::a11y_set_value`).
    fn a11y_set_value(&mut self, value: f64) -> bool {
        self.input_model_mut().a11y_set_value(value)
    }

    /// A text field's text, caret and selection for a reader (`Input::a11y_text`).
    fn a11y_text(&self) -> Option<crate::a11y::A11yText> {
        self.input_model().a11y_text()
    }

    /// Replace a text field's text for an assistive tool (`Input::a11y_set_text`).
    fn a11y_set_text(&mut self, text: &str) -> bool {
        self.input_model_mut().a11y_set_text(text)
    }

    /// An assistive tool clicked one of them (`Input::a11y_select_item`).
    fn a11y_select_item(&mut self, idx: usize) -> bool {
        self.input_model_mut().a11y_select_item(idx)
    }

    fn set_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool) {
        self.input_model_mut().set_modifiers(ctrl, shift, alt)
    }

    /// Dispatch a context-menu action on this widget. Returns whether it was applied.
    /// Default inert; the adapter forwards to `Input::context_action` (whose default gives
    /// every widget whole-value Cut/Copy/Paste through the value-string pair).
    fn context_action(&mut self, action: ContextAction) -> bool {
        self.input_model_mut().context_action(action)
    }

    fn color(&self) -> [f32; 4] {
        self.paint_model().color()
    }

    fn solid_border(&self) -> Option<([f32; 4], f32)> {
        self.paint_model().solid_border()
    }

    fn widget_font(&self) -> Option<String> {
        self.paint_model().widget_font()
    }

    /// Whether the paint walk should clip this widget's children to its rect (scroll/root plate
    /// containers). Default: no clipping.
    fn clips_children(&self) -> bool {
        self.paint_model().clips_children()
    }

    /// Whether this widget paints its ENTIRE subtree itself through its (recursive)
    /// `all_rounded_quads` / `all_quads` — a legacy "subtree painter" such as `TreeList`, whose
    /// row backgrounds and separators live in an `all_rounded_quads` override that also recurses
    /// into its children. When true, the paint walk emits those directly and does NOT recurse
    /// (the widget already did). Transitional: such widgets will eventually get a proper
    /// non-recursive `paint_self`. Default: false.
    fn renders_own_subtree(&self) -> bool {
        self.paint_model().paints_own_subtree()
    }

    fn z_index(&self) -> i32 {
        self.layout_model().z_order()
    }

    /// The widget's natural CONTENT height — the control below its detached label, if
    /// any. What a layout strategy allots; [`WidgetHost::layout`] places that content
    /// box at the origin it is given and hangs the label ([`WidgetHost::label_strip`])
    /// above it. `None` when the widget has no natural height.
    fn preferred_height(&self) -> Option<f32> {
        self.layout_model().intrinsic_size().map(|s| s.height)
    }

    fn label(&self) -> Option<String> {
        self.base().label.clone()
    }

    fn corner_radii(&self) -> CornerRadii {
        let (r, (tl, tr, br, bl)) =
            self.paint_model().corner_style(self.content_rect()).unwrap_or((0.0, (false, false, false, false)));
        CornerRadii::new(
            if tl { r } else { 0.0 },
            if tr { r } else { 0.0 },
            if br { r } else { 0.0 },
            if bl { r } else { 0.0 },
        )
    }

    fn mark_dirty(&mut self, ctx: &mut UiContext){
        let b = self.base_mut();
        if b.dirty {
            return;
        }
        b.dirty = true;
        if let Some(parent) = b.id.get().and_then(|id| ctx.tree.parent_id(id)) {
            ctx.lend(parent, |p, ctx| p.mark_dirty(ctx));
        }
    }

    // ── What the widget paints, read from its model ──────────────────────────────
    // The legacy tuple views (`extra_quads`, `all_quads`, `all_rounded_quads`,
    // `extra_arcs`, `extra_circles`, `highlight_quad`, `corner_style`) are gone since
    // 2026-10-08: every host paints a widget through `paint_self` / the paint walk, and a
    // composite that draws a child's chrome in its own order reads `painted_prims`.

    /// The rect the widget's model paints into: the host's rect below its detached label.
    fn content_rect(&self) -> crate::scene::layout::Rect {
        let b = self.base();
        let top = if self.layout_model().inline_label() { 0.0 } else { b.label_offset() };
        // Deliberately NOT clamped at zero: hosts under-size labeled sliders (label taller
        // than the assigned rect), and the negative-height quads still rasterize.
        crate::scene::layout::Rect { x: b.x, y: b.y + top, width: b.w, height: b.h - top }
    }

    /// Everything the widget's model paints into [`content_rect`](Self::content_rect), as prims.
    fn painted_prims(&self) -> Vec<crate::scene::paint::Prim> {
        let mut pc = crate::scene::paint::PaintCtx::new();
        self.paint_model().paint(self.content_rect(), &mut pc);
        pc.finish().items.into_iter().map(|item| item.prim).collect()
    }
}

impl<T: WidgetHost + ?Sized> WidgetHostExt for T {}

/// A shown widget's prims as its model paints them, nothing while it is hidden — for a
/// composite that draws a child's chrome in its own order rather than walking it (the params
/// pane's rows, the ramp's key editor, the menubar's strip).
pub(crate) fn shown_prims<W: WidgetHost + ?Sized>(w: &W) -> Vec<crate::scene::paint::Prim> {
    if w.visible() { w.painted_prims() } else { Vec::new() }
}

/// The plain quads among [`shown_prims`], as `(x, y, w, h, colour)`.
pub(crate) fn shown_quads<W: WidgetHost + ?Sized>(w: &W) -> Vec<(f32, f32, f32, f32, [f32; 4])> {
    shown_prims(w)
        .into_iter()
        .filter_map(|prim| match prim {
            crate::scene::paint::Prim::Quad { rect, color } => Some((rect.x, rect.y, rect.width, rect.height, color)),
            _ => None,
        })
        .collect()
}

/// The rounded quads among [`shown_prims`], as `(x, y, w, h, radius, colour, corners)`.
pub(crate) fn shown_rounded_quads<W: WidgetHost + ?Sized>(
    w: &W,
) -> Vec<(f32, f32, f32, f32, f32, [f32; 4], (bool, bool, bool, bool))> {
    shown_prims(w)
        .into_iter()
        .filter_map(|prim| match prim {
            crate::scene::paint::Prim::RoundedRect { rect, radius, corners, color } => {
                Some((rect.x, rect.y, rect.width, rect.height, radius, color, corners))
            }
            _ => None,
        })
        .collect()
}

/// The narrow traits' defaults, for a host that has no widget model of its own (the test
/// shims that implement `WidgetHost` directly): transparent, no focus role, no value.
pub struct NoModel;

impl Layout for NoModel {}

impl Paint for NoModel {
    fn color(&self) -> [f32; 4] {
        [0.0; 4]
    }
}

impl Input for NoModel {}
