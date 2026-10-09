//! `impl WidgetHost for Adapted`: the host surface, each method forwarded to the narrow trait
//! that answers it, with the adapter's visibility gate, label strip and hit gating.

use super::*;

impl<W: Layout + Paint + Input + 'static> WidgetHost for Adapted<W> {
    fn layout_model(&self) -> &dyn Layout {
        &self.inner
    }
    fn paint_model(&self) -> &dyn Paint {
        &self.inner
    }
    fn input_model(&self) -> &dyn Input {
        &self.inner
    }
    fn input_model_mut(&mut self) -> &mut dyn Input {
        &mut self.inner
    }
    fn base(&self) -> &Widget {
        &self.base
    }
    fn base_mut(&mut self) -> &mut Widget {
        &mut self.base
    }
    // `as_any` exposes the *inner* widget: legacy code downcasts by concrete widget type
    // (`json_layout`'s `downcast_mut::<Checkbox>()`), and the adapter must be transparent to it.
    fn as_any(&self) -> &dyn std::any::Any {
        &self.inner
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        &mut self.inner
    }
    fn set_visible(&mut self, visible: bool) {
        if self.visible != visible {
            self.visible = visible;
            Input::visibility_changed(&mut self.inner, visible);
        }
    }
    fn visible(&self) -> bool {
        self.visible
    }

    fn focused(&self, _ctx: &UiContext) -> bool {
        Input::is_focused(&self.inner, self.base.focused)
    }


    fn layout(&mut self, origin: crate::widget::Point, constraints: crate::widget::LayoutConstraints, ctx: &mut UiContext) {
        // The WidgetHost default (measure + set_rect), plus recursive child layout for visible
        // containers — the ctx-carrying half of the arrangement the model can't do in
        // `arrange_children`.
        // `origin` is the CONTENT box's top-left and `measure` its height; the detached
        // label hangs in the strip above, so the block `set_rect` takes starts `strip`
        // higher and is `strip` taller.
        let size = self.measure(constraints, ctx);
        let strip = self.label_strip();
        self.set_rect(origin.x, origin.y - strip, size.width, size.height + strip);
        let host_id = self.base.id();
        Layout::register_embedded_children(&mut self.inner, host_id, ctx);
    }

    fn prepare_text(&mut self, fs: &mut cosmic_text::FontSystem) {
        if self.visible() {
            let rect = self.content_rect();
            Paint::prepare_text(&mut self.inner, fs, rect);
        }
    }

    fn popover_rect(&self) -> Option<(f32, f32, f32, f32)> {
        if !self.visible() {
            return None;
        }
        Paint::popover(&self.inner, self.content_rect())
    }

    fn render_popover(&self, pc: &mut dyn crate::layout::RenderTarget) {
        if !self.visible() {
            return;
        }
        Paint::draw_popover(&self.inner, self.content_rect(), pc);
    }

    // --- Legacy structural conventions the adapter owns on the widget's behalf ---

    /// The assigned rect is the widget's whole block: the detached label strip (if any)
    /// at its top, the content below (`content_rect`). One convention for every
    /// control — a caller sizing a labeled widget by hand adds `label_strip` to the
    /// content height; `layout` does that for it.
    fn set_rect(&mut self, x: f32, y: f32, w: f32, h: f32) {
        let r = Layout::adjust_rect(&self.inner, Rect { x, y, width: w, height: h });
        self.base.x = r.x;
        self.base.y = r.y;
        self.base.w = r.width;
        self.base.h = r.height;
        // Ungated rect notification (TextBox re-clamps scroll on every assignment, hidden or
        // not — the legacy `set_rect` side effect).
        let landed = Rect { x: self.base.x, y: self.base.y, width: self.base.w, height: self.base.h };
        Layout::rect_assigned(&mut self.inner, landed);
        // Containers position their children from the assigned rect (legacy `set_rect`
        // overrides); hidden containers skip it, like the legacy impls.
        if self.visible {
            let content = self.content_rect();
            Layout::arrange_children(&mut self.inner, content);
        }
    }


    fn label_strip(&self) -> f32 {
        if Layout::inline_label(&self.inner) { 0.0 } else { self.base.label_offset() }
    }

    /// The detached label's box, as `base_label_fallback` places the text: at the
    /// label inset on the strip above the content, measured in the detached-label font.
    fn detached_label_rect(&self) -> Option<Rect> {
        if Layout::inline_label(&self.inner) {
            return None;
        }
        let label = self.base.label.as_deref()?;
        let (fam, size) = crate::layout::control_label_font_detached_parsed();
        let width = crate::widget::display::measure_text_width(label, &fam, size);
        Some(Rect { x: self.base.x + Layout::detached_label_inset(&self.inner), y: self.base.y, width, height: self.label_strip() })
    }

    /// The `WidgetHost::measure` default, except the width consults the intrinsic size when the
    /// widget opts in ([`Layout::intrinsic_measure_width`] — Dropdown's `auto_width`).
    fn measure(&self, constraints: crate::widget::LayoutConstraints, _ctx: &UiContext) -> crate::widget::Size {
        let (_, _, w, h) = self.rect();
        let pref_w = if Layout::intrinsic_measure_width(&self.inner) {
            Layout::intrinsic_size(&self.inner).map_or(w, |s| s.width)
        } else {
            w
        };
        // Content height: the intrinsic one, else the landed rect less its label strip.
        let pref_h = self.preferred_height().unwrap_or(h - self.label_strip());
        crate::widget::Size {
            width: pref_w.clamp(constraints.min_width, constraints.max_width),
            height: pref_h.clamp(constraints.min_height, constraints.max_height),
        }
    }


    /// Report the *inner* type's name, not `Adapted<W>`: runtime type-name matching (e.g.
    /// `layout.rs`' span-full widget list) must keep seeing the widget it knows.
    fn type_name(&self) -> &'static str {
        std::any::type_name::<W>().split("::").last().unwrap_or("Widget")
    }

    // --- Paint concern -> `Paint` ---






    fn a11y_items(&self) -> Vec<crate::a11y::A11yItem> {
        Input::a11y_items(&self.inner, self.content_rect())
    }


    fn paint_self(&self, ui: &UiContext, ctx: &mut PaintCtx) {
        let mut tmp = PaintCtx::new();
        Paint::paint_ui(&self.inner, ui, self.content_rect(), &mut tmp);
        // Subtree painters (paints_own_subtree) author their COMPLETE text in paint() —
        // per-child fonts and clip bounds included — so their Text prims pass through
        // verbatim and the single-font own-labels re-derivation below is skipped
        // (re-deriving would flatten a composite's mixed child fonts to widget_font).
        let subtree = Paint::paints_own_subtree(&self.inner);
        // The Text prims the own-labels bridge below re-derives labels from. Only
        // ParametersBg and Group override `paint_ui`, and neither reaches that
        // bridge, so this pass's text is exactly what `Paint::paint` would emit.
        let mut painted_text: Vec<TextLabel> = Vec::new();
        for item in tmp.finish().items {
            // Re-emitting through ctx re-records clip state, so restore the
            // circular clip the widget authored the prim under (Ramp's
            // foam-cell fills) — it would otherwise be dropped here.
            let clip_circle = item.clip_circle;
            if let Some(c) = clip_circle {
                ctx.push_clip_circle(c);
            }
            // `replay` emits every prim but Text and hands Text back — the two
            // callers disagree about it. A subtree painter authored its own text
            // (per-child fonts and clips) so that passes through verbatim;
            // otherwise it is dropped in favour of the own-labels bridge below.
            if let Some(Prim::Text { text, x, y, font_size, color, font, bounds, .. }) =
                ctx.replay(item.prim)
            {
                if subtree {
                    ctx.text_with(text, x, y, font_size, color, font, bounds);
                } else {
                    painted_text.push(TextLabel { text, x, y, font_size, color });
                }
            }
            if clip_circle.is_some() {
                ctx.pop_clip_circle();
            }
        }
        // The focus highlight a widget opts into (the TextBox): the primary tint over the
        // row span while focused, over the background. (The hover tint never drew here.)
        if Paint::legacy_focus_highlight(&self.inner) && ui.is_focused_id(self.base.id()) {
            let b = &self.base;
            let (hx, hw) = if b.row_w > 0.0 { (b.row_x, b.row_w) } else { (b.x, b.w) };
            ctx.quad(Rect { x: hx, y: b.y, width: hw, height: b.h }, crate::colors::highlight_primary_color());
        }
        // Own text with per-label font+bounds: the hatch view verbatim for hatched widgets
        // (caveat: its contract includes raw container children — those few widgets keep the
        // hatch until their hosts adopt the walk), else the standard own-labels bridge (prim
        // text + the detached base label, one font, text_bounds or the scroll-ancestor clip).
        if subtree {
            // The detached label is the adapter's, not the widget's: a subtree painter
            // authors its own text but knows nothing of the label strip above its
            // content (TreeList, Spreadsheet, Ramp), so the bridge's base-label half
            // still runs for it — in the detached-label font, like every control's.
            // Skipping it left a labelled tree's strip reserved but blank.
            if self.visible() && !Layout::inline_label(&self.inner) {
                let font = Some(crate::layout::control_label_font_detached());
                for tl in self.base_label_fallback() {
                    ctx.text_with(tl.text, tl.x, tl.y, tl.font_size, tl.color, font.clone(), None);
                }
            }
            return;
        }
        // `own_labels_from_painted` lists the content labels first (all of them
        // while visible), then the base label: the first `content` take the
        // widget's `text_attrs`.
        let (labels, content) = if Paint::serves_legacy_labels(&self.inner) {
            (Paint::legacy_labels_with_font_and_bounds(&self.inner, self.content_rect(), ui), 0)
        } else {
            let content = if self.visible() { painted_text.len() } else { 0 };
            (self.own_labels_from_painted(ui, painted_text, Paint::text_font(&self.inner)), content)
        };
        let attrs = Paint::text_attrs(&self.inner);
        for (i, (tl, font, bounds)) in labels.into_iter().enumerate() {
            let attrs = if i < content { attrs } else { crate::scene::paint::TextAttrs::default() };
            ctx.text_attrs(tl.text, tl.x, tl.y, tl.font_size, tl.color, font, bounds, attrs);
        }
    }

    // The legacy per-widget text getters are deleted from `WidgetHost`: this adapter's text
    // reaches the frame through `paint_self` above (prim-derived own labels + the
    // detached base label), and composites that need a concrete Adapted child's labels
    // call `own_labels_with_font_and_bounds` directly (pub(crate)).







    // --- Input concern -> `Input` ---
    /// Row-rect assignment (row-layout hosts): apply the widget's clamp
    /// ([`Layout::adjust_row_rect`] — TextBox's `width`/`max_width`), then the base write the
    /// `WidgetHost` default does.
    fn set_row_rect(&mut self, x: f32, w: f32) {
        let (rx, rw) = Layout::adjust_row_rect(&self.inner, x, w);
        self.base.row_x = rx;
        self.base.row_w = rw;
    }
    fn attach_embedded(&mut self, ctx: &mut UiContext) {
        let host_id = self.base.id();
        Layout::register_embedded_children(&mut self.inner, host_id, ctx);
    }

    fn release_embedded(&mut self, ctx: &mut UiContext) {
        Layout::release_embedded_children(&mut self.inner, ctx);
    }

    fn tick(&mut self, dt: f32, ctx: &mut UiContext) -> bool {
        // A composite places the children the context holds from the rect it kept: every
        // tick, as on every layout.
        let host_id = self.base.id();
        Layout::register_embedded_children(&mut self.inner, host_id, ctx);
        let rect = self.content_rect();
        let mut changed = Input::tick(&mut self.inner, dt, rect);
        let mut ectx = EventCtx::new(rect, host_id, Some(ctx));
        changed |= Input::tick_ctx(&mut self.inner, dt, &mut ectx);
        ectx.open_requested_menu(&*self);
        changed
    }
    /// Focus set/cleared directly (hosts call `w.focus()`/`w.unfocus()`): keep the base flag
    /// (unless the widget opts out — [`Input::tracks_base_focus`], TextBox's legacy `focus`
    /// never set it) and tell the widget via the same `FocusIn`/`FocusOut` events the router
    /// would send.
    fn focus(&mut self) {
        if Input::tracks_base_focus(&self.inner) {
            self.base.focused = true;
        }
        let mut ectx = EventCtx::new(self.content_rect(), self.base.id(), None);
        Input::on_event(&mut self.inner, &Event::FocusIn, &mut ectx);
    }
    fn unfocus(&mut self) {
        if Input::tracks_base_focus(&self.inner) {
            self.base.focused = false;
        }
        let mut ectx = EventCtx::new(self.content_rect(), self.base.id(), None);
        Input::on_event(&mut self.inner, &Event::FocusOut, &mut ectx);
    }

    fn hit_test(&self, px: f32, py: f32, ctx: &UiContext) -> bool {
        // Hidden widgets are not hittable. Legacy widgets with a visibility toggle (Spreadsheet)
        // carry this gate themselves — and need it: hosts broadcast wheel/press dispatch to
        // every widget (the designer) and rely on hidden ones rejecting the hit.
        if !self.visible() {
            return false;
        }
        // Preserve the legacy occlusion check (a covering layer swallows the hit), then delegate
        // the geometric test to the narrow trait instead of the row/label-offset machinery.
        if ctx.is_coordinate_covered(self.base.id(), px, py) {
            return false;
        }
        let (x, y, w, h) = self.rect();
        // Row-hit opt-in ([`Layout::hit_row_rect`]): replicate the legacy `hit_test` default's
        // geometry — substitute the host-pushed row span — before
        // the narrow test. The width<=0 reject also comes from that default.
        if Layout::hit_row_rect(&self.inner) {
            if w <= 0.0 || h <= 0.0 {
                return false;
            }
            let (hx, hw) = if self.base.row_w > 0.0 { (self.base.row_x, self.base.row_w) } else { (x, w) };
            return Input::hit(&self.inner, Rect { x: hx, y, width: hw, height: h }, px, py);
        }
        Input::hit(&self.inner, Rect { x, y, width: w, height: h }, px, py)
    }

    fn handle_event(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        let rect = self.content_rect();
        match event {
            // A hit right-press on a context-menu widget routes to the shared config menu,
            // which reads the widget itself, so the adapter owns it.
            Event::MouseButton {
                button: crate::widget::MouseButton::Right,
                state: crate::widget::ElementState::Pressed,
                x: px,
                y: py,
                ..
            } if Input::opens_context_menu(&self.inner) => {
                if self.hit_test(*px, *py, ctx) {
                    ctx.handle_right_click(&*self, *px, *py);
                    return true;
                }
                false
            }
            // Hit-gate PRESSES and wheel once, here, so narrow widgets never carry the
            // per-widget "check hit_test first" boilerplate legacy `mouse_input` overrides do.
            // RELEASES are deliberately NOT gated: a press-tracking widget (Button) must see the
            // release wherever the cursor ended up, to commit or cancel — exactly what legacy
            // `mouse_input` overrides did by receiving every release. Event-proxying containers
            // opt out of the press gate (`Input::gates_presses`): legacy container overrides
            // saw every press (Switcher unfocuses its child on an outside press).
            Event::MouseButton { state: crate::widget::ElementState::Pressed, x: px, y: py, .. }
                if !Input::gates_presses(&self.inner) =>
            {
                let _ = (px, py);
                self.offer(event, ctx)
            }
            Event::MouseButton { state: crate::widget::ElementState::Pressed, x: px, y: py, .. }
            | Event::MouseWheel { x: px, y: py, .. } => {
                self.hit_test(*px, *py, ctx) && self.offer(event, ctx)
            }
            Event::MouseButton { state: crate::widget::ElementState::Released, .. } => {
                self.offer(event, ctx)
            }
            // Offer the raw move to the widget; if unconsumed, run the legacy hover bookkeeping
            // (base.hovered + MouseEnter/MouseLeave synthesis, which re-enters this method and
            // reaches `on_event` through the arm below).
            Event::PointerMove { x: px, y: py, .. } => {
                if self.offer(event, ctx) {
                    return true;
                }
                let (px, py) = (*px, *py);
                self.cursor_moved(px, py, ctx)
            }
            // The router's drag lifecycle (recorded drag target → DragStart/DragUpdate/
            // DragEnd) maps to the Input drag hooks, exactly like the direct
            // `WidgetHost::drag_*` entry points below — `on_event` is offered first, but no
            // widget consumes Drag* there today; without these arms the events fell into the
            // on_event default and every ROUTED drag was silently dead (the reason each app
            // historically kept its own held-drag index and called drag_update directly).
            Event::DragStart { start_x, start_y } => {
                if self.offer(event, ctx) {
                    return true;
                }
                Input::drag_begin(&mut self.inner, *start_x, *start_y, rect);
                true
            }
            Event::DragUpdate { x, y, .. } => {
                if self.offer(event, ctx) {
                    return true;
                }
                if let Some((nx, ny)) = Input::drag_reposition(&mut self.inner, *x, *y, rect) {
                    self.base.x = nx;
                    self.base.y = ny;
                    return true;
                }
                Input::drag_update(&mut self.inner, *x, *y, rect)
            }
            Event::DragEnd => {
                if self.offer(event, ctx) {
                    return true;
                }
                Input::drag_end(&mut self.inner);
                true
            }
            // A widget hidden while still holding focus (the designer keys into
            // `focused_widget`; hiding a pane doesn't unfocus it) must not consume keys —
            // formerly the `keyboard_input` entry point's gate, now on the one funnel
            // (which also closes the routed path's missing-gate hole).
            Event::KeyInput(_) if !self.visible() => false,
            // Everything else (KeyInput, Tick, Enter/Leave, Focus*) forwards directly —
            // the legacy default dispatch would route these to leaf handlers Adapted never
            // overrides, so there is no behavior to fall back to.
            _ => self.offer(event, ctx),
        }
    }
}
