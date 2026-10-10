//! `Adapted`'s inherent API: construction and labels, the values an app drains, the direct event
//! entry points a host calls, and the widget's own text labels and glyphs.

use super::*;

impl<W: Layout + Paint + Input + 'static> Adapted<W> {
    /// Wrap `inner` with a fresh [`Widget`] base.
    pub fn new(inner: W) -> Self {
        Adapted { base: Widget::new(), visible: true, inner }
    }

    /// The wrapped widget.
    pub fn inner(&self) -> &W {
        &self.inner
    }

    /// The wrapped widget, mutably.
    pub fn inner_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// This widget's tree id (assigned lazily), for registering it in a [`UiContext`].
    pub fn id(&self) -> WidgetId {
        self.base.id()
    }

    /// Attach a control label. Mirrors the `with_label` builders legacy control widgets carry,
    /// so construction sites keep their shape when a widget migrates. The label is stored on the
    /// base (legacy machinery: label offsets, context-menu titles) *and* pushed into the widget
    /// via [`Paint::sync_label`] for widgets that paint it themselves.
    pub fn with_label(mut self, label: &str) -> Self {
        self.base.label = Some(label.to_string());
        self.inner.sync_label(label);
        self
    }

    /// Bind this widget to a config file/key (right-click context-menu editing). Mirrors the
    /// legacy `with_config` builders.
    pub fn with_config(mut self, file: &str, key: &str) -> Self {
        self.base.config_file = Some(file.to_string());
        self.base.config_key = Some(key.to_string());
        self
    }

    /// Update the control label, keeping the base copy (legacy machinery) and the widget's own
    /// copy ([`Paint::sync_label`]) in step. Inherent so it shadows `Control::set_label` — which
    /// writes only the base and would leave a self-painting label stale — at every call site,
    /// regardless of which traits are in scope.
    pub fn set_label(&mut self, label: &str) {
        self.base.label = Some(label.to_string());
        self.inner.sync_label(label);
    }

    /// Take the control label off again: the base copy goes to `None`, so
    /// the adapter reserves no label strip, and the widget's own copy is
    /// synced empty — which the self-painting widgets' strip rule
    /// (`slider::detached_strip`) reads as no label. What a host needs when
    /// it moves a label from the control to a column of its own
    /// (`ParametersBg` re-flowing a narrow pane).
    pub fn clear_label(&mut self) {
        self.base.label = None;
        self.inner.sync_label("");
    }

    /// Name the widget to a screen reader without drawing a label
    /// ([`Widget::accessible_name`]); `None` falls back to the label.
    pub fn set_accessible_name(&mut self, name: Option<&str>) {
        self.base.accessible_name = name.map(str::to_string);
    }

    // --- The value/polling drains (off `WidgetHost` in the 6bd value shrink): apps read
    // widget state through these concrete methods; each forwards to the narrow `Input`
    // hook. The last dyn readers went concrete-slot instead (TI roster, cloud JsonControl,
    // designer pane-focus sync).

    /// Drain the one-shot click flag (Button-class widgets).
    pub fn take_click(&mut self) -> bool {
        Input::take_click(&mut self.inner)
    }

    /// Drain the one-shot value-changed flag.
    pub fn take_change(&mut self) -> bool {
        Input::take_change(&mut self.inner)
    }

    /// The widget's value serialized to a string (config writes, context-menu Copy).
    pub fn get_value_string(&self) -> Option<String> {
        Input::value_string(&self.inner)
    }

    /// Parse and apply a value string; returns whether the value changed.
    pub fn set_value_string(&mut self, val: &str) -> bool {
        Input::set_value_string(&mut self.inner, val)
    }

    /// The widget's value as an integer.
    pub fn value(&self) -> i32 {
        Input::value(&self.inner)
    }

    /// Selection state pushed in by list/row hosts.
    pub fn set_selected(&mut self, selected: bool) {
        Input::set_selected(&mut self.inner, selected)
    }

    /// Text-content mutation: keep the base copy and the widget's own copy
    /// ([`Paint::sync_label`]) in step, like `set_label`.
    pub fn set_text(&mut self, text: &str) {
        self.base.label = Some(text.to_string());
        Paint::sync_label(&mut self.inner, text);
    }

}

impl<W: Layout + Paint + Input + 'static> Adapted<W> {
    /// Whether a press here may start a drag (off `WidgetHost` — the ControlPanel
    /// endgame; forwards to the narrow `Input` hook with the laid-out content rect).
    pub fn draggable(&self) -> bool {
        Input::draggable(&self.inner, self.content_rect())
    }

    /// Whether the widget's own drag is live (off `WidgetHost` with `draggable`).
    pub fn is_dragging(&self) -> bool {
        Input::is_dragging(&self.inner)
    }

    /// Movement bounds pushed in by hosts (off the `WidgetHost` trait since 6bd — the one
    /// production caller is concrete: designer's network panel).
    pub fn set_drag_bounds(&mut self, bx: f32, by: f32, bw: f32, bh: f32) {
        Input::set_drag_bounds(&mut self.inner, bx, by, bw, bh)
    }

    /// Unlink all tree children (off `WidgetHost` in 6bd batch 2 — every caller is a concrete
    /// `Adapted` field).
    pub fn clear_children(&mut self, ctx: &mut UiContext) {
        ctx.clear_children_ids(self.base.id());
    }

    // --- The direct-dispatch entry points, inherent since the 6bd collapse. In-crate
    // composites forward to their CONCRETE embedded children through these; dyn callers
    // and the router go through `handle_event`, which these forward to (the two paths
    // are identical by construction — including Drag*, which handle_event maps onto the
    // Input drag hooks).

    pub fn mouse_input(&mut self, button: crate::widget::MouseButton, state: crate::widget::ElementState, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(
            &Event::MouseButton { button, state, x: px, y: py, local_x: px, local_y: py },
            ctx,
        )
    }
    pub fn mouse_wheel(&mut self, delta: &crate::widget::MouseScrollDelta, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        self.handle_event(
            &Event::MouseWheel { delta: *delta, x: px, y: py, local_x: px, local_y: py },
            ctx,
        )
    }

    /// [`Self::mouse_wheel`] WITHOUT the adapter's rect hit-gate: straight to the
    /// widget's `Input::on_event`. For hosts that already zone-gated the wheel
    /// themselves against a capture region LARGER than the widget rect — the
    /// band slider's shape-conforming halo extends past the row rect, and the
    /// rect gate would clip exactly the fringe the halo exists to catch
    /// (`ParametersBg`'s slider forwarding). The widget's own on_event still
    /// applies its fine-grained zone test.
    /// Offer `event` to the widget's `on_event`, routed (with the context), and open the
    /// context menu it asked for, if any, once it is done.
    pub(super) fn offer(&mut self, event: &Event, ctx: &mut UiContext) -> bool {
        let mut ectx = EventCtx::new(self.content_rect(), self.base.id(), Some(ctx));
        let consumed = Input::on_event(&mut self.inner, event, &mut ectx);
        ectx.open_requested_menu(&*self);
        consumed
    }

    pub fn mouse_wheel_ungated(
        &mut self,
        delta: &crate::widget::MouseScrollDelta,
        px: f32,
        py: f32,
        ctx: &mut UiContext,
    ) -> bool {
        self.offer(&Event::MouseWheel { delta: *delta, x: px, y: py, local_x: px, local_y: py }, ctx)
    }
    pub fn keyboard_input(&mut self, event: &crate::widget::KeyEvent, ctx: &mut UiContext) -> bool {
        self.handle_event(&Event::KeyInput(event.clone()), ctx)
    }
    pub fn drag_begin(&mut self, px: f32, py: f32) {
        let rect = self.content_rect();
        Input::drag_begin(&mut self.inner, px, py, rect)
    }
    pub fn drag_update(&mut self, px: f32, py: f32) -> bool {
        let rect = self.content_rect();
        if let Some((nx, ny)) = Input::drag_reposition(&mut self.inner, px, py, rect) {
            self.base.x = nx;
            self.base.y = ny;
            return true;
        }
        Input::drag_update(&mut self.inner, px, py, rect)
    }
    pub fn drag_end(&mut self) {
        Input::drag_end(&mut self.inner)
    }

    /// The deleted trait default: coverage-gated hover dispatch (an open popover covering
    /// the point clears the hover instead of recomputing it).
    pub fn cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        ctx.set_cursor_pos(px, py);
        if ctx.is_coordinate_covered(self.base.id(), px, py) {
            let was = self.base.hovered;
            if was {
                self.base.hovered = false;
                self.handle_event(&Event::MouseLeave, ctx);
            }
            return was;
        }
        self.on_cursor_moved(px, py, ctx)
    }

    /// Ungated pointer moves (no popover-coverage check): offer the raw move to the widget,
    /// then fall back to the base hover bookkeeping, mirroring `handle_event`'s `PointerMove`
    /// arm. Not routed *through* `handle_event`, because the routed path reaches this method
    /// too (via `cursor_moved`) and would recurse; on that path `on_event` sees the same
    /// unconsumed move twice, which is fine — a hover recompute is idempotent (anything
    /// that changed on the first call consumed it there).
    pub fn on_cursor_moved(&mut self, px: f32, py: f32, ctx: &mut UiContext) -> bool {
        let event = Event::PointerMove { x: px, y: py, local_x: px, local_y: py };
        if self.offer(&event, ctx) {
            return true;
        }
        let was = self.base.hovered;
        let is_hit = self.hit_test(px, py, ctx);
        self.base.hovered = is_hit;
        if was != is_hit {
            let transition = if is_hit { Event::MouseEnter } else { Event::MouseLeave };
            self.handle_event(&transition, ctx);
            true
        } else {
            false
        }
    }

    /// Link this widget under `parent`, or unlink it from its parent (`None`).
    pub fn set_parent(&mut self, parent: Option<WidgetId>, ctx: &mut UiContext) {
        ctx.tree.set_parent(self.base.id(), parent);
    }

    /// The model's intrinsic content size (off the `WidgetHost` trait since 6bd — the concrete
    /// callers are fonts'/graph's hand-laid button/dropdown sizing).
    pub fn intrinsic_size(&self) -> Option<Size> {
        Layout::intrinsic_size(&self.inner)
    }




    /// This widget's OWN text (prim-derived + detached base label), before any child
    /// aggregation — the shared source for the three text getters.
    pub(crate) fn own_text_labels(&self) -> Vec<TextLabel> {
        if !self.visible() {
            return Vec::new();
        }
        let mut out: Vec<TextLabel> = self
            .painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Text { text, x, y, font_size, color, .. } => {
                    Some(TextLabel { text, x, y, font_size, color })
                }
                _ => None,
            })
            .collect();
        if !Layout::inline_label(&self.inner) {
            out.extend(self.base_label_fallback());
        }
        out
    }

    /// This widget's GLYPHS — the image prims its [`Paint::paint`] emits, a
    /// dropdown's arrow or a spinbox's −/+ — as `(image, rect, alpha)`, for a
    /// container that paints its children's chrome itself and collects their
    /// text through [`own_text_labels`](Self::own_text_labels) (the
    /// parameters pane). Until 2026-10-05 those symbols were text and rode
    /// the labels; as glyphs nothing carried them, and the pane drew its
    /// controls without their arrows.
    pub(crate) fn own_glyphs(&self) -> Vec<(u32, Rect, f32)> {
        if !self.visible() {
            return Vec::new();
        }
        self.painted_prims()
            .into_iter()
            .filter_map(|prim| match prim {
                Prim::Image { image, rect, alpha } => Some((image, rect, alpha)),
                _ => None,
            })
            .collect()
    }

    /// The paint walk's own-labels bridge: prim-derived text in the widget's content font
    /// ([`Paint::text_font`]), plus the detached base label in the configured detached-label
    /// font. Fed the Text prims `paint_self` already holds from its own pass — running
    /// `Paint::paint` a second time just to get them back doubled every leaf's paint cost.
    pub(super) fn own_labels_from_painted(
        &self,
        _ctx: &UiContext,
        painted: Vec<TextLabel>,
        prim_font: Option<String>,
    ) -> Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> {
        // The detached label is the adapter's, not the widget's: one font for every
        // control's label — `style.control.label.font_detached`, whose size
        // `base_label_fallback` already takes — whatever font the widget's own content
        // uses (a TreeList's rows, a Breadcrumb's segments) or does not declare. A
        // widget with no `widget_font` used to fall back to the engine's sans default
        // here, so half the gallery's labels were in a different face.
        //
        // `text_bounds` clips the widget's content text only. The detached label sits
        // in the strip above the content, so a widget clipping its content to its own
        // rect (Graph) clipped its label away, and one clipping to the inside of its
        // relief (TextBox's well floor) could not do so at all while the label shared
        // the clip — TextBox widened it to the whole block, and its text ran over its
        // wall. (The legacy scroll-ancestor clamp ended here too: always a no-op since
        // Phase 6av — ScrollBox, the last scroll ancestor type, never appeared as a
        // tree parent.)
        let base_font = Some(crate::layout::control_label_font_detached());
        let bounds = Paint::text_bounds(&self.inner, self.content_rect());
        let mut out: Vec<(TextLabel, Option<String>, Option<[f32; 4]>)> = Vec::new();
        if self.visible() {
            out.extend(painted.into_iter().map(|l| (l, prim_font.clone(), bounds)));
            if !Layout::inline_label(&self.inner) {
                out.extend(self.base_label_fallback().into_iter().map(|l| (l, base_font.clone(), None)));
            }
        }
        out
    }

    /// The base-label text of a *detached*-label widget — a replica of the legacy default
    /// `WidgetHost::text_labels` body (which an overriding impl can no longer call).
    pub(super) fn base_label_fallback(&self) -> Vec<TextLabel> {
        let b = &self.base;
        if let Some(ref label) = b.label {
            let (_, font_size) = crate::layout::control_label_font_detached_parsed();
            let color = crate::color::control_label_color_detached_for_state(b.hovered, b.focused);
            let inset = Layout::detached_label_inset(&self.inner);
            return vec![TextLabel { text: label.clone(), x: b.x + inset, y: b.y, font_size, color }];
        }
        Vec::new()
    }
}
