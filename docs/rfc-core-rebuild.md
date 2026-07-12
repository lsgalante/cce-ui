# RFC: cce-ui core rebuild

**Status:** Draft / proposal
**Scope:** The core of `cce-ui` — tree ownership, layout, paint, clipping, and animation.
**Appetite:** Breaking changes are acceptable. Migration is incremental, behind a stable
`Application` trait, one widget/app at a time. Every crate must continue to build standalone.

---

## 1. Why

`cce-ui` works, but it is not one system — it is three half-systems overlaid, with nothing
arbitrating between them. Concretely, from an audit of the current code:

- **Tree ownership is tripled.** A child lives in a container's own
  `Vec<*mut dyn Element>`, in `ctx.layout_tree.children`, *and* in `ctx.widget_registry`,
  kept in sync by hand in every `add_child`/`clear_children`. All three are raw
  `*mut dyn Element`. `Drop` (`core.rs` `clear_widget_references`) clears focus/context-menu
  refs but **not** the registry or parent/child maps, so stale pointers can linger.
- **Rendering has three paths with no single owner:** the app's top-level `widgets: Vec<Box<dyn Element>>`
  iteration, parent→child `all_rounded_quads` recursion, and immediate-mode
  `render_widget`/`SectionContext`. Nothing enforces that a widget is drawn by exactly one.
  *(This is the direct cause of the breadcrumb "black rectangle": the breadcrumb was sized
  full-width by its container and drawn a second time under the dropdown.)*
- **Layout is smeared across five mechanisms:** `LayoutStrategy::allocate` (a child-driven
  bump cursor), `Container::layout`, immediate-mode builders that literally *render twice to
  measure*, `Backplate` (which clips children but does not lay them out), and hand-written
  `set_rect` with absolute screen coordinates in page code. There is no measure→arrange pass
  and no owner of any given rect.
- **No clip/transform abstraction.** Rect clipping is hand-copied rect-intersection in every
  container's `all_quads`/`all_rounded_quads`/`text_*`. The GPU has no `set_scissor_rect`;
  the only GPU clip is a per-fragment circular test in `shader.wgsl`.
- **Animation barely exists.** One real helper (`hover_animation`) — copy-pasted into a
  second, *dead* implementation in `UiContext` (`tick_hover`/`get_hover_quad`, zero callers).
  Everything else (button hover/press, `network_opacity`) is an **instant boolean flip or a
  static multiplier, not interpolated**. No `Animated<T>`, tween, spring, or easing library.
  "Keep animating" is a bool hand-propagated up the `tick` chain — miss one link and the
  animation silently freezes.

Every fragility we have hit is a symptom of this. The `Element` trait has grown to ~90
methods spanning layout, paint, hit-testing, clipboard, tree expand/collapse, and ~12
`as_*_controller` downcast escape hatches — a god-trait that makes each of the above worse.

### What is already good (keep it)

- **The frame loop is sound.** Demand-driven redraw with a single `redraw` dirty bool, gated
  by a Wayland frame-callback vsync (`frame_callback_pending`). It idles correctly when
  nothing changes. `tick(dt)` plumbing (clamped `dt`, ~60 Hz dispatch) already exists.
- **Immediate-per-frame tessellation from a retained tree** is a reasonable bones: widgets are
  long-lived, geometry is re-emitted each frame into one shared vertex buffer and one shader
  pipeline. We are not throwing this out.
- **The primitive tessellators** (rounded rects with per-corner radii, vectors with caps,
  arcs, circles, bevels) are solid and reusable as-is.

---

## 2. Goals / non-goals

**Goals**
1. **Solid** — one source of truth for the widget tree; no raw pointers; no manual multi-store
   sync; no dangling-pointer class.
2. **Flexible** — one real two-phase layout pass (measure → arrange) *separate from paint*,
   with genuine flex/grow, so layout can be recomputed without re-running paint.
3. **Dynamic** — a first-class animation primitive; hover/press/opacity/slide/scale/collapse
   as interpolated values; the loop automatically keeps frames coming while anything is live.
4. **Fast** — GPU scissor clipping, no per-frame debug I/O, and a path to per-subtree geometry
   caching later.

**Non-goals (for this RFC)**
- Changing the Wayland/wgpu/glyphon backend, the `calloop` loop, or the frame-callback vsync.
- Changing the KDL config system or IPC.
- A big-bang rewrite. This lands incrementally behind the existing `Application` trait.
- Per-subtree tessellation caching — designed-for, but deferred (see §9).

---

## 3. Target architecture

A retained scene graph with a clean separation of concerns, borrowing the proven
Flutter/GPUI/Taffy split of *tree · layout · paint*:

```
            ┌─────────────────────────────────────────────┐
   update → │  Arena (owns all nodes, keyed by NodeId)     │
            │    Node { parent, children, widget, style,   │
            │           layout_out, anim_state, dirty }    │
            └───────────────┬─────────────────────────────┘
                            │
      ┌─────────────────────┼──────────────────────┬───────────────┐
      ▼                     ▼                      ▼               ▼
  Layout pass          Paint pass             Input pass       Anim tick
 (measure→arrange)   (emit DisplayList)     (hit-test by      (advance
  → LayoutOut rects   under clip/xform       layout rect+z)    Animated<T>)
      │                     │
      │                     ▼
      │            DisplayList → existing tessellators → one vertex buffer
      └── taffy (recommended) or hand-rolled solver
```

### 3.1 Node arena — one source of truth

Replace `Vec<*mut dyn Element>` + `layout_tree` + `widget_registry` with a single arena
(a `slotmap`/generational-index store). No raw pointers cross frames; code passes
`&Arena` / `&mut Arena` + `NodeId`.

```rust
pub struct NodeId(/* slotmap key: generational index */);

pub struct Node {
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub widget: Box<dyn Widget>,   // the payload (see §3.5)
    pub style: Style,              // layout inputs (flex/size/padding/…)
    pub layout_out: LayoutOut,     // computed rect+transform (written by layout pass)
    pub anim: AnimSet,             // this node's live Animated<T> values
    pub dirty: Dirty,              // LAYOUT | PAINT bitflags
}

pub struct Arena { nodes: SlotMap<NodeId, Node>, root: NodeId, /* free lists, dirty set */ }
```

Why an arena and not `Rc<RefCell<>>` or keeping raw pointers:
- Generational keys make use-after-free a `None` lookup, not UB. The entire dangling-pointer
  class disappears.
- One store means no hand-sync of three collections; `add_child`/`remove` touch one place.
- It sidesteps the borrow-checker tree problem: recursion passes `NodeId` and re-borrows the
  arena, and layout operates on `style`/`layout_out` (not the `widget` payload) so it never
  needs `&mut` to two nodes at once. `SlotMap::get_disjoint_mut` covers the rare cases that do.

Widget **identity** becomes `NodeId` uniformly. Today identity is split: parent/child links
key on `WidgetId` while `hit_test`/focus/`highlight_color` key on the raw `self` address cast
to `usize`. Unify on `NodeId`.

### 3.2 Layout pass — measure then arrange, separate from paint

Introduce a real two-phase pass that runs *before* paint and writes `LayoutOut` per node:

- **measure(constraints) → Size** bubbles intrinsic sizes child→parent. Leaves (text, icons)
  measure their content — text via a glyphon measurement hook so wrapping is correct.
- **arrange(final_rect)** flows final positions parent→child, writing absolute (or
  transform-local, see §3.4) rects into `layout_out`.

**Decision: hand-roll the solver** (taffy was considered and declined). We own a compact
measure/arrange engine inside `cce-ui`: `measure(constraints) -> Size` bubbles intrinsic sizes
child→parent (leaves measure content — text via a glyphon hook), `arrange(final_rect)` flows
final positions parent→child. Start with the layout primitives cce-ui actually needs —
row/column with gap + padding, flex grow/shrink, main/cross alignment, and fixed/intrinsic
sizing — rather than a full CSS flexbox/grid clone. This deletes `LayoutStrategy`, the
`allocate` bump-cursor, and the render-twice-to-measure `SectionContext` pattern outright.

Rationale for hand-rolling over a dependency: full control over the exact box model (no
impedance-matching a general CSS engine to our primitives), no external version churn, and a
solver scoped to what the DE uses. The cost is that we implement and test grow/shrink/wrap
ourselves — acceptable given the constrained widget set.

The key property either way: **layout is computed independently of paint.** That is what makes
the breadcrumb bug structurally impossible (one owner writes each rect) and what makes animated
layout cheap (re-arrange without re-emitting paint).

### 3.3 Paint pass — one path, a display list

Collapse the three render paths into one. Paint walks the arena in z-order and each node emits
primitives into a single `DisplayList`, given its computed `layout_out` and a `PaintCtx`:

```rust
pub enum Prim {
    RoundedRect { rect: Rect, radii: CornerRadii, color: Rgba, corners: Corners },
    Vector { a: Vec2, b: Vec2, thickness: f32, color: Rgba, cap: LineCap },
    Circle { center: Vec2, r: f32, color: Rgba },
    Arc { .. }, Text { .. },
}
pub struct DisplayList { prims: Vec<(ZIndex, Prim)>, /* under active clip/xform */ }
```

The `DisplayList` is then fed to the **existing tessellators** (`push_rounded_rect_vertices_corners`,
`vector_vertices`, `circle_vertices`, glyphon text) → the existing single vertex buffer + shader.
The backend `render()` stays; only its *input* changes from "call `view`/`view_rounded_quads`/
`overlay_quads` on the app" to "walk the arena into one display list." The top-level `widgets`
Vec iteration and the `render_widget`/`SectionContext` immediate path both go away.

### 3.4 Clip + transform stack

`PaintCtx` carries a clip stack and a transform (translate+scale is enough for v1; a full 2×3
affine is a small extension):

```rust
impl PaintCtx {
    fn with_clip(&mut self, rect: Rect, f: impl FnOnce(&mut PaintCtx));
    fn with_transform(&mut self, xform: Affine2, f: impl FnOnce(&mut PaintCtx));
}
```

- Rect clips lower to GPU **`set_scissor_rect`** (a real render-pass feature we currently do
  not use), deleting the hand-copied rect-intersection in every container.
- Keep the shader's circular clip for the cases that need it.
- The transform gives **slide / scale / collapse transitions for free** and lets children use
  local coordinates instead of everyone storing absolute screen coords.

### 3.5 Trait split — kill the god-trait

Replace the ~90-method `Element` with narrow traits, each a single concern:

```rust
/// The payload. Most widgets implement only this + Paint.
pub trait Widget: 'static {
    fn style(&self) -> Style { Style::default() }        // layout inputs
    fn measure(&self, c: Constraints, ctx: &MeasureCtx) -> Option<Size> { None } // leaves only
}
pub trait Paint  { fn paint(&self, layout: &LayoutOut, ctx: &mut PaintCtx); }
pub trait Input  {
    fn hit(&self, layout: &LayoutOut, p: Vec2) -> bool { layout.rect.contains(p) } // default!
    fn on_event(&mut self, ev: &Event, ctx: &mut EventCtx) -> EventStatus { EventStatus::Ignored }
}
```

- Hit-testing gets a correct default from `layout_out.rect` + z-order, so the manual
  `hit_test`-by-`self`-address code across widgets disappears.
- The 12 `as_*_controller` downcasts are replaced by typed messages / commands through
  `EventCtx` (an app-defined message channel), not runtime `Any` casts.
- `Container` stops being special: a container is just a `Widget` with children and a flex
  `Style`; it no longer hand-clips or hand-lays-out.

### 3.6 Animation — first-class

```rust
pub struct Animated<T> { current: T, target: T, motion: Motion } // Motion: Tween(easing,dur) | Spring(k,damp)
impl<T: Lerp> Animated<T> {
    fn set_target(&mut self, t: T);
    fn tick(&mut self, dt: f32) -> bool;  // returns true while still moving
    fn value(&self) -> T;
}
```

- Hover/press/focus become `Animated<f32>` 0→1 factors, not bools. Buttons interpolate color
  by `lerp(idle, hover, factor)` instead of `if hovered { a } else { b }`.
- `network_opacity` becomes an `Animated<f32>` that actually tweens.
- **The loop keeps frames coming automatically:** the arena tracks "any node has a live
  animation." `tick` OR-reduces over the arena's animated nodes and sets `redraw`, so the
  hand-propagated `tick`-bool chain (and its silent-freeze failure mode) is gone.
- Fold `hover_animation` (and delete its dead `UiContext` twin) into this; the global
  thread-local singleton — which today allows only one animated highlight at a time — becomes
  per-node state, so multiple highlights animate independently.

---

## 4. How this kills the classes of bug we have

| Bug class | Fixed by |
|---|---|
| Dangling / stale `*mut dyn Element` | §3.1 arena, generational keys, single store |
| Tree desync (Vec vs layout_tree vs registry) | §3.1 one store |
| Same widget drawn twice at different rects (breadcrumb) | §3.2 single layout owner + §3.3 single paint path |
| Overflow bleeding past clip regions | §3.4 GPU scissor + clip stack |
| Animation silently freezes (missed tick-bool link) | §3.6 arena-driven auto frame requests |
| One-highlight-at-a-time hover | §3.6 per-node animation state |
| Per-frame debug I/O in hot path | §7 quick win |

---

## 5. Impact on the `Application` trait & clients

The client-facing `Application` trait shape stays as stable as possible so apps migrate one at
a time. Two viable migration shapes:

- **Adapter (recommended):** the old `Element` widgets keep working via a compatibility shim
  that wraps each in a `Node` and forwards `all_rounded_quads`/`text_items` into a `DisplayList`.
  New/migrated widgets implement the narrow traits directly. Both coexist until the last old
  widget is gone.
- The `view`/`view_rounded_quads`/`overlay_quads` methods become thin shims over the arena walk
  during the transition, then are removed.

Constraint respected: **each crate still builds standalone** — the new core is entirely inside
`cce-ui`; clients depend on it by path exactly as today. No `[workspace.dependencies]`.

---

## 6. Migration plan (staged; every stage leaves the tree building)

- **Phase 0 — Quick wins (independent of the rebuild).** Fix breadcrumb double-render; remove
  the per-frame `eprintln!` text reconstruction in `render()`; add `set_scissor_rect` for the
  existing clip call sites; delete the dead `tick_hover`/`get_hover_quad` twin. *Ships value now.*
- **Phase 1 — Arena + identity.** Introduce `Arena`, `NodeId`, `Node`; port `add_child`/tree
  ops onto it behind the adapter; unify identity on `NodeId`. No visual change.
  - **1a — Arena data structure: DONE.** `cce-ui/src/scene/arena.rs` — a generational forest
    arena, generic over payload, added additively (`pub mod scene;`) with nothing wired into the
    live path yet. `NodeId` carries a `NonZeroU32` generation so a handle to a removed node reads
    back as `None` (use-after-free → missed lookup, not UB). Tree ops (`insert`, `append_child`,
    `detach`, `remove_subtree`, `get_pair_mut`, `subtree`/`ancestors` iterators, cycle rejection)
    with 14 headless unit tests, including one that owns and trees real `dyn Element` payloads.
    Full `cce-ui` suite: 83 passing.
  - **1b — Adapter: DONE.** `cce-ui/src/scene/tree.rs` — `WidgetTree`, the arena-backed
    replacement for `UiContext`'s two stores (`widget_registry` + `layout_tree`), keyed by a
    `WidgetId → NodeId` index so the public `WidgetId` API is preserved. Consolidates both maps
    into one generational store; link-only-before-register is modeled as `Entry.ptr: None`. 10
    headless tests against real `dyn Element` payloads (register/overwrite, symmetric+deduped
    link, reparent, link-before-register, symmetric detach, clear_children, clear_all, removal
    staleness, registered-iteration). Not yet wired into `UiContext`.
  - **1b — Live swap: DONE (compile + tests; runtime verification pending).**
    `UiContext.{widget_registry, layout_tree}` are replaced by a single `tree: WidgetTree`. All
    access routed through it: the 5 public methods, the internal direct field reads in
    `context.rs` (event routing, `clear_dirty`, `rebuild_spatial_grid`, `is_widget_visible`,
    `tick`, `is_coordinate_covered`, `is_movable_backplate_at`, `is_widget_at`), the widget-layer
    defaults in `widget/mod.rs`, and the direct pokes in `keybinds_control.rs`/`multi_control.rs`
    (now `link_ids`) and `plate.rs`/`parameters_bg.rs` (now `tree.set_parent(id, None)`). `cce-ui`
    builds clean; **93 tests pass**; all 7 app crates that use the API build unchanged. Still to
    do: **`make run`** cce-files + cce-designer to confirm the symmetric-tree change (below) is
    behavior-safe in paint/event handling.
    - Original swap notes below retained for reference.

    Replace `UiContext.{widget_registry, layout_tree}`
    with a `WidgetTree`, and route through it: the 5 public methods apps depend on
    (`register_widget`, `link_ids`, `clear_hierarchy`, plus internally `clear_children_ids`,
    `unlink_child`), the internal direct field reads in `context.rs` (event routing, `mark_dirty`
    walk, `is_widget_visible`, `clear_dirty`, `rebuild_spatial_grid`, coverage/hit tests), and the
    widget-layer defaults in `widget/mod.rs` (`parent`/`set_parent`/`children`/`add_child`/
    `mark_dirty`) plus the direct pokes in `keybinds_control.rs`, `multi_control.rs`, `plate.rs`,
    `parameters_bg.rs`.
    - **One deliberate behavior change to verify at runtime:** the legacy maps are left
      *asymmetric* in a few spots (`set_parent(Some)` writes only `parents`; `plate`/`parameters_bg`
      detach via `parents.remove` only). `WidgetTree` keeps parent/child links symmetric, so
      `children()` — read by paint recursion and event propagation — becomes self-consistent. This
      is almost certainly a latent-bug fix, but it must be confirmed against the running apps
      (cce-files, cce-designer, cce-graph, cce-system-settings) before landing.
    - App compatibility: only `register_widget`/`link_ids`/`clear_hierarchy` have app callers;
      `get_widget`/`get_widget_mut`/`unlink_child` have **zero callers** workspace-wide and can be
      dropped or kept as thin shims.
- **Phase 2 — Layout pass.** Add `Style` + measure/arrange (taffy). Migrate containers to
  emit `Style` instead of `LayoutStrategy`; delete `allocate` and render-twice measurement as
  containers move over.
- **Phase 3 — Paint unification.** Introduce `DisplayList` + `PaintCtx` (clip/transform);
  route the backend `render()` through the arena walk; retire the top-level `widgets` Vec and
  `render_widget`/`SectionContext`.
- **Phase 4 — Animation.** Land `Animated<T>` + arena-driven frame requests; convert
  hover/press/`network_opacity`; consolidate `hover_animation`.
- **Phase 5 — Trait split & cleanup.** Split `Element` into `Widget`/`Paint`/`Input`; remove
  `as_*_controller` downcasts; migrate remaining widgets; delete the compatibility shim.
  - **Approach correction (resolved by experiment).** A *non-breaking supertrait carve-out* of
    `Element` (`trait Element: Paint + …`) turns out to be impossible in Rust here. The structural
    methods the passes need (`rect`, `children`, `set_rect`) are overridden in dozens of widgets
    across cce-ui **and** the app crates (`rect` 31+8, `children` 23+2, `set_rect` 42+4): moving
    them off `Element` breaks every override, and merely *declaring* them on a supertrait makes
    every `elem.children()`/`elem.rect()` call site ambiguous (a supertrait method is always in
    scope on the subtrait). Nor does a blanket "view" `impl<T: Element> Paint for T` let
    `&dyn Element` coerce to `&dyn Paint` — that coercion exists only for real supertraits. So the
    split follows the **adapter** path from §5, not a supertrait split: narrow traits independent
    of `Element`, with `Adapted<W>` bridging a narrow-trait widget into the `*mut dyn Element`
    tree. The compatibility-shim bullet is thus *this* adapter (there was never a discrete legacy
    shim to delete — the earlier migration hung hooks directly on `Element`).
  - **5a — Layout + Paint concerns + adapter: DONE (compile + tests; no runtime surface yet).**
    `cce-ui/src/widget/model.rs` — the independent `Layout` (`layout_style` / `intrinsic_size` /
    `layout_children`) and `Paint` (`color` / `paint` / `clips_children`, where `paint` takes the
    laid-out rect rather than reading a stored one) traits, plus `Adapted<W>`: a wrapper that
    carries the `Widget` base and forwards the `Element` layout/paint methods to `W`'s narrow
    traits. A headless test builds a pure narrow-trait tree (a `Col` container + two `Dot` leaves,
    none of which implement `Element`), wraps each in `Adapted`, and drives it through the
    *existing* `scene::bridge` layout pass and `scene::painter` paint pass — asserting both the
    computed rects and the painted quads. Purely additive: no existing widget or app changes, all
    137 cce-ui tests pass. Runtime verification is N/A until a real widget is migrated onto the
    adapter (nothing in a running app uses it yet).
  - **5b — Input concern: DONE (compile + tests; no runtime surface yet).** `widget/model.rs` —
    the `Input` trait (`hit` / `on_event`, both against the laid-out rect) plus adapter
    forwarding with the RFC's centralizations: `Adapted::handle_event` hit-gates pointer-
    positioned events (`MouseButton`/`MouseWheel`) once, so narrow widgets never carry the
    per-widget "check hit_test first" boilerplate every legacy `mouse_input` override does;
    unconsumed `PointerMove` falls back to the legacy hover bookkeeping, so `base.hovered` and
    the synthesized `MouseEnter`/`MouseLeave` (which re-enter `handle_event` and reach
    `on_event`) keep working; `hit_test` keeps the occlusion (`is_coordinate_covered`) check
    while delegating the geometric test to `Input::hit`. A headless test drives a narrow
    `Clicker` through the *real* `UiContext::propagate_event` router: in-rect click consumed +
    counted, out-of-rect click gated out, hover enter/leave transitions observed on both the
    narrow widget and the base flag. 138 cce-ui tests pass.
  - **5c — First real widget migrated: `ProgressBar`. DONE (runtime-verified, pixel-identical).**
    `widget/display/progress_bar.rs` now implements only `Layout` + `Paint` + `Input`;
    `ProgressBar::new` returns `Adapted<ProgressBar>`, so both construction sites
    (`cce-ui` demo, `cce-test-interface`, incl. `.with_label`) compile unchanged. The migration
    forced the adapter to absorb the legacy surface external render loops actually read, all
    added to `Adapted` in this step: an `all_rounded_quads` **reverse bridge** (the widget's
    `Paint::paint` output converted back to legacy tuples — cce-test-interface renders via this),
    `Paint::corner_style` → `corner_radius`/`rounded_corners` (for style-property painters like
    the demo's `widget_vertices`; transitional, dies with those paths), the detached-label
    convention (`set_rect` inflation + `with_label` builder + content-rect inset),
    `preferred_height` ← `intrinsic_size`, `highlight_quad → None` (narrow widgets own their
    pixels), and `type_name` reporting the *inner* type (layout.rs string-matches
    `"ProgressBar"` for span-full sizing). **Runtime verification:** ran cce-test-interface and
    the demo on the live compositor (via `ccectl center-window` + `grim`); an A/B pixel diff of
    the demo against the pre-migration build showed the two frames identical except a 19×20
    compositor corner artifact — zero differing pixels at any widget. 141 tests pass.
  - **5d — Leaf sweep: `Separator`, `StatusDot`, `UsageBar`. DONE (runtime-verified via the
    settings app's render stream).** New adapter machinery this round: `Deref`/`DerefMut` to the
    wrapped widget (call sites keep `dot.set_status(..)` / `bar.value`); per-prim reverse
    bridges (`Prim::Quad` → `extra_quads`, `RoundedRect` → `all_rounded_quads`, `Circle`/`Arc` →
    `extra_circles`/`extra_arcs`) so apps reading BOTH `all_quads` and `all_rounded_quads` draw
    each prim exactly once; `Input::blocks_backplate_drag`; and the mirrored-by-value-builder
    pattern (`Adapted<UsageBar>::with_colors`) since builders can't flow through `Deref`.
    `Separator` had no `Widget` base (public x/y/w/h fields) — its rect now lives on the adapter
    base, and cce-status-interface's rotation loop was updated to transpose via `rect`/`set_rect`.
    **One deliberate behavior fix:** legacy `StatusDot` emitted **zero** geometry on every render
    path (probe-confirmed — `render_widget` reads only `all_quads`/`all_rounded_quads`, both
    empty for it), so the Processes-page dots were invisible; the narrow `Paint` default emits
    the color quad, and the dots now render (verified in the live app's render dump: 10×10 rects
    in exact status colors). UsageBar verified byte-identical in the same dump (bg+fill rects at
    the exact `with_colors` colors). 147 tests pass; status-interface, system-settings, and
    test-interface all build.
  - **5e — First interactive widgets: `Checkbox` + `Toggle`. DONE (verified end-to-end with an
    injected live click).** New machinery: the legacy polling/value surface on `Input`
    (`take_click`/`take_change`/`value_string`/`set_value_string`/`value` — kept there to avoid
    a fourth bound; dies with RFC §3.5 typed messages); `Input::opens_context_menu` (the adapter
    routes a hit right-press to `UiContext::handle_right_click`, which ctx-less `on_event`
    can't); `Layout::inline_label` (Checkbox/Toggle draw the label inside their rect — no
    `set_rect` inflation/content inset, matching the legacy `label_offset` type-name special
    cases); `Paint::{solid_border, widget_font, sync_label}` (transitional forwards);
    prim-derived `text_labels` for inline-label widgets (one paint source feeds every text
    path) with a base-label fallback replica for detached ones; `as_any` now exposes the *inner*
    widget so legacy `downcast_mut::<Checkbox>()` sites keep working; `Drop` on `Adapted`
    clears the global focus/context-menu refs (bounds moved onto the struct for this);
    `Debug`/`Clone` derives; and an inherent `Adapted::set_label` that shadows
    `Control::set_label` (which writes only the base and left self-painted labels stale —
    caught by a test; `Control` impls override to route here). Both widgets track
    `hovered`/`focused` from the forwarded `MouseEnter`/`MouseLeave`/`FocusIn`/`FocusOut`
    events — the state that becomes `Animated<f32>` in §3.6. In-crate consumers updated
    (`json_layout` direct Element calls, `multi_control` enum variant, `parameters_bg` field);
    app repos updated (system-settings network+notifications, data-editor, layout-interface
    field types — construction sites unchanged). **Verification:** cce-test-interface pixel-
    diffed 0 against the pre-migration baseline, and a `wlrctl`-injected click on the live
    compositor flipped the Toggle's bordered half on-screen — the full input path through the
    adapter exercised for real. 152 tests pass.
  - **5f — `Button` (widest-radius widget: 19 app files + 8 in-crate). DONE.** The press/release
    contract forced an adapter refinement: **presses stay hit-gated, releases now flow ungated**
    — a press-tracking widget must see the release wherever the cursor ended up to commit
    (in-rect → `take_click` + `on_click_cb`) or cancel, exactly the legacy `mouse_input`
    contract (pinned by a router-level test incl. out-of-rect cancel). Also added:
    `Layout::layout_ignore` and `Input::set_selected` forwards. The model ports the per-kind
    color matrix (Primary/Reset/ListRow/CopyIcon × pressed/hovered/selected + bg overrides),
    SVG icon quads, per-kind label justification/fonts, and the Phase 2b `intrinsic_size`; the
    9 by-value builders are mirrored on `Adapted<Button>` (`with_label` comes from the generic
    + `sync_label`). In-crate consumers fixed (multi_control, keybinds_control, ramp, treelist,
    parameters_bg fields; json_layout + demo now call `take_click` on the box instead of
    concrete downcasts); ~11 app repos updated (field types + raw-cast→`as_ptr_mut` cleanups).
    **Verification:** full workspace (minus compositor, which doesn't use widgets) builds;
    154 cce-ui tests + cce-cloud's json_layout hover-simulation test pass; test-interface
    pixel-diffs cursor-only vs the 5e baseline; a live hover A/B against the stashed legacy
    build showed the identical fill pixel (the inert hover on that page is pre-existing app
    behavior, not a regression).
  - **5g — `Label`. DONE.** Text lives on the model, emitted as a `Text` prim; the adapter's
    prim bridge serves every legacy text path. Added the generic synced `Element::set_text`
    override on `Adapted` (same trap as `set_label`: the trait method wrote only the base and
    left the painted text stale — live-updating labels like system-info's CPU readouts hit it
    constantly). Builders mirrored; three app repos' field types updated. Workspace builds;
    155 tests pass; test-interface diff vs the post-Button baseline has a 0x0 bbox at 2%
    threshold (sub-perceptual blend noise only).
  - **5h — `Slider` + `RangeSlider`, and the event-capability layer. DONE.** Introduced the
    RFC §3.5 **`EventCtx`** (`on_event(&mut self, event, &mut EventCtx)`): content rect, widget
    id, `request_focus()` (readout edit mode), and a transitional `ui: Option<&mut UiContext>`
    for the legacy scroll-gesture gating. Added `Input` drag hooks
    (`draggable`/`is_dragging`/`drag_begin`/`drag_update`/`drag_end`, rect-carrying — hosts
    drive drags by direct call) and — critically — **direct-dispatch overrides**: hosts call
    `mouse_input`/`mouse_wheel`/`keyboard_input`/`focus`/`unfocus` directly on widgets, and
    without adapter overrides those hit the inert Element defaults (a latent 5e/5f regression:
    treelist's add-key button and parameters_bg checkboxes were deaf on that path — now routed
    into `handle_event`). `Layout::inflates_label_rect` distinguishes ProgressBar-style rect
    inflation from Slider-style label-eats-into-rect. `text_labels` is now prim-derived PLUS
    base fallback (sliders paint readout text AND have a detached label). **Found the hard
    way:** hosts under-size labeled sliders, so legacy content height went NEGATIVE and the
    flipped quads still rasterized — `content_rect` must not clamp at zero or tracks vanish
    (pixel-diffed to 0 vs baseline after the fix). Slider geometry consolidated into one
    `geom()` helper (legacy re-derived it in five places). 154 tests pass; workspace builds.
  - **5i — Display leaves + `Spinbox`: InfoBox, FontPreview, Sidebar, Splitter, Panel, Spinbox.
    DONE.** New adapter machinery: `Input::drag_reposition` (self-moving widgets — Panel,
    Splitter — return a new origin; the adapter applies it to the base rect the model can't
    reach), `Input::set_drag_bounds`, and `Layout::detached_label_inset` (the legacy
    `Control::control_label` +4px x-offset that the default `text_labels` path lacked — caught
    as a 4px label shift in the pixel diff, fixed to a 1×1-pixel residual). Spinbox ports the
    sub-zone hover (-/+ buttons) into `PointerMove` handling, display-click edit mode with
    cursor placement + `request_focus`, decimals/unit value formatting, and drops its vestigial
    raw-pointer parent/children fields. Skipped for later: `PreviewState` (needs per-label fonts
    on the `Text` prim), `StatusBar` (bigger custom surface), Float3/LayoutPreview (time).
    156 tests pass; workspace builds; test-interface pixel-diff vs the 5h baseline: 1 pixel.
  - **5j — `InteractiveListItem`. DONE (verified in the live settings render dump: service-row
    titles/subtitles + themed overlays through the prim bridge).** Button-pattern press/release
    with themed selected/hover/press overlays. `StatusBar` was surveyed and DEFERRED: it is
    parent-coupled (reads its parent's backplate state/rect/radius at paint time and owns
    `parent`/`set_parent`) — that belongs with the container/children design, not the leaf
    recipe. Also still pending from the leaf tier: Float3, LayoutPreview (time), PreviewState
    (needs per-label fonts on `Prim::Text`).
  - **5k — Controller capabilities + first controller widgets: `Breadcrumb`, `Node`. DONE.**
    The controller tier was blocked because `Element` is implemented exactly once (for
    `Adapted<W>`), so a migrated widget couldn't re-expose its `as_*_controller` downcasts.
    Resolution: transitional **capability hooks on `Input`** (`menu/graph/spreadsheet/path/
    param/geom _controller[_mut]`, default `None`) that the adapter forwards the `Element`
    downcast pairs to — a controller widget returns `Some(self)`. This is the pragmatic half of
    the §3.5 "typed messages" bullet: the controller *traits* are already the typed surface;
    what dies with `Element` is reaching them through the god-trait (end state: hold the
    concrete `Adapted<W>` or a `&dyn XController` directly). Also added:
    `EventCtx::open_context_menu` (Breadcrumb records the right-clicked segment *before* the
    shared menu opens — `opens_context_menu` can't express work-before-menu), an
    `Input::copy_path` forward (context menu "Copy Path"), and an `Adapted::on_cursor_moved`
    override — another direct-dispatch entry (cce-files drives breadcrumb hover through it)
    routing the raw move to `on_event` with the base hover bookkeeping as fallback.
    `ScrollController` was deleted outright: zero implementors and zero live callers
    workspace-wide (only a dead cce-designer helper, removed there). **Breadcrumb** (first
    controller widget, live in cce-files + cce-designer) verified on the live compositor:
    idle/hover captures pixel-identical to the stashed legacy build at the widget, and a
    breadcrumb-segment click navigates correctly through the direct-dispatch
    `mouse_input → on_event → path_click` chain. **Node** (ParamController + GeomController,
    self-moving drag with grid snap via `drag_reposition`) has no live constructors in any app
    repo — compile + router-level tests only. 160 tests pass; all 19 client crates build.
  - **5l — `Spreadsheet` + the tick/scroll adapter surface. DONE (live-verified in the
    designer: startup and pane-open captures diff 4px/32px vs the stashed legacy build, all in
    a one-pixel bottom-edge blend strip).** New `Input` surface: `tick(dt, rect)` +
    `wants_tick` (inertial scroll — hosts broadcast `Element::tick` per frame; §3.6
    `Animated<T>` eventually replaces this), `scrollable` (→ `is_scrollable`), and
    `draggable` now takes the laid-out rect (scroll widgets are draggable only while content
    overflows). **`Adapted` now owns real visibility**: the `Widget` base carries none and the
    legacy `Element` defaults are no-ops, so each hideable widget stored its own flag; the
    adapter stores it once, gating `hit_test` (hosts broadcast wheel/press dispatch and rely
    on hidden widgets rejecting the hit), direct-dispatch `keyboard_input` (hiding a pane
    doesn't unfocus it), and the `text_labels` bridge. Deliberate fix: `set_visible` on
    migrated widgets now works instead of being silently ignored. Spreadsheet's 7×-duplicated
    scroll/thumb math collapsed into one `geom()` helper; its `paint` deliberately does NOT
    emit the translucent `PARAM_BG` background (the designer draws widget backgrounds itself
    from `color()` + `corner_style` — emitting it again would double-blend).
  - **5m — `Graph` + the legacy dual-geometry escape hatch. DONE (designer startup pixel-diffs
    ZERO; cce-graph diffs to an empty 2%-threshold bbox; cce-files' Graph view verified
    visually + getter-contract unit test — its pixel A/B was blocked by the live session's
    terminal covering the capture region).** Graph's three hosts consume DIFFERENT getters
    (designer: plain `extra_quads` + `extra_circles`; cce-files `render_widget`:
    `all_rounded_quads` with highlight-only `all_quads`; cce-graph: the scene path's
    `paint_self`). The model keeps one geometry generator; `paint` emits the rounded view, and
    transitional `Paint` hooks serve the rest: `serves_legacy_plain_quads`/`legacy_plain_quads`
    (verbatim through `extra_quads`, with the adapter emptying `all_quads` to preserve the
    no-double-draw contract) and `text_bounds` (node names clip to the widget rect — the
    adapter now overrides `text_labels_with_[font_and_]bounds`, replicating the
    scroll-ancestor walk when the hook is `None`). Also: `Input::hit` override for the legacy
    edge-exclusive hit test; ctrl-wheel zoom reads `ctrl_pressed` through `EventCtx.ui`;
    the `Element::paint` register_hovered pre-pass is dropped (shared hover highlight is
    suppressed for all adapted widgets). Discovered en route: the designer's CONTENT pane IS a
    real `Graph` (index 1), and `ContentBg` is a standalone grid background whose
    GraphController impl is mostly stubs — it is NOT a Graph wrapper.
  - **5n — The container concern + first containers: `Switcher`, `ContentBg`. DONE
    (render-dump-verified: cce-system-settings — whose every page lives under the Switcher —
    A/B'd byte-identical across four pages against the stashed legacy build, modulo live
    system data).** The children/tree design, resolved transitional-first: tree links already
    live in `ctx.tree` (Phase 1b), so a container needs only (a) its own child-pointer Vec
    (ctx-less `set_rect` arrangement — the same reason legacy containers kept one) and (b) the
    subtree plumbing every legacy container hand-copied. (a) stays in the model behind new
    `Layout` hooks (`has_container_children`/`container_children`/`child_added`/
    `children_cleared`/`parent_changed`/`adjust_rect`/`arrange_children`/
    `layout_children_ctx`/`child_visible`); (b) moved into the ADAPTER once, filtered by the
    `child_visible` policy: plain-quad aggregation with the shared rounded-bg-skip rule,
    rounded recursion (the Element default this override had been shadowing — a latent
    container blocker), per-kind text aggregation, `get_text_items`/`prepare_text`/`tick`/
    popover recursion, container-style `add_child` (parents the child back), the ctx-carrying
    child layout pass, and `is_child_visible`. `Input` gains `hits_through_children` and
    `gates_presses` (event-proxying containers must see every press — Switcher unfocuses its
    child on an outside click). Event proxying itself stays in the model via `EventCtx::ui`,
    bug-for-bug (including Switcher's double `mouse_input` dispatch while a popover is open).
    `ContentBg` turned out to be a leaf and rode the ordinary recipe. NOT for this path:
    deep-composition containers (Page/Plate embed `Layer`; SectionContainer embeds
    `Container`; Paginator embeds ButtonStrip + `Vec<Page>`) — embedding means migrating the
    base struct inverts the dependency; those dissolve when their hosts move to the scene
    walk (Phase 6), not through `Adapted`.
  - **5o — `MenuBar` migrated; standalone `Menu` DELETED (zero constructors workspace-wide —
    dead code).** MenuBar keeps its legacy `ButtonStrip` EMBEDDED in the model (owned by
    value, driven through `Element` calls; events reach it via `EventCtx::ui`;
    `arrange_children` parents it back to the adapter). New adapter surface: `Paint::popover`
    / `draw_popover` (own dropdowns — the container recursion only covered child popovers),
    `Layout::z_order`, `Layout::tracked_parent` (serves `Element::parent` from the model's
    field — legacy parent-chain styling walks use a DUMMY ctx that tree lookups can't
    answer), `Input::set_modifiers` / `visibility_changed` / `is_focused` (conditional focus:
    the bar holds the global slot only while something is open) + `EventCtx::release_focus`,
    the `as_page_selector` capability pair, and `Paint::corner_style` now takes the laid-out
    rect (corners computed against the parent backplate's edges). Dropped, flagged: the
    never-read glyphon buffer caches and the vertical-mode dynamic `rect()` (`with_vertical`
    has no callers). Verified: 168 tests (full open→click→close roundtrip through real
    adapter dispatch), all four hosts run, live A/B on cce-test-interface pixel-equivalent
    (the strip has zero diffs above the 8% threshold; the File-click-opens-nothing behavior
    there is byte-identical pre-existing app behavior). Designer A/B (was pending on screen
    contention): DONE — startup, params-pane dropdown clicks, circular-pane mode, and a
    View→Circular-Pane `trigger_menu_click` roundtrip all pixel-equivalent vs the f1523ab^
    baseline (all residual diffs are composited-cursor + bottom-status-strip artifacts).
    Driven deterministically via the designer's embedded HTTP API on :3000
    (`{"action":"toggle_circular_pane"}`, `{"action":"menu_click","widget_idx":8,
    "menu_idx":2,"item_idx":2}`); the params-pane Circular-Pane dropdown not opening on
    click is pre-existing app behavior, identical in both builds.
  - **5p — `Dropdown` (first popover widget through the 5o `Paint::popover`/`draw_popover`
    surface). DONE (render-dump + live-A/B verified).** Slider label convention
    (`inflates_label_rect=false`), `Control::control_label`'s +4px via
    `detached_label_inset`, `gates_presses=false` (an open dropdown must see the outside
    press that closes it), `opens_context_menu`, dynamic `z_order` (100 while open), and
    focus parity bug-for-bug: `FocusIn` re-claims the global slot (direct `focus()` callers —
    test-interface), `FocusOut` closes without releasing it. New adapter surface:
    `Layout::intrinsic_measure_width` + an `Element::measure` override on `Adapted`
    (identical to the `Element` default unless a widget opts in — preserves `auto_width`
    measuring, which cce-system-settings sizes its page dropdown through). Parity decisions,
    flagged: the public `parent` field stays direct-write-only (legacy `set_parent` never
    wrote it — Ramp's dummy-ctx `set_parent` calls were silently discarded, so the Ramp
    popover clamp and fade-blend parent color were dormant in production and stay dormant);
    the backplate-concentric corner walk starts from a `parent_changed`-tracked pointer and
    hops field-based legacy `parent(&dummy)` impls (exact for cce-graph's
    Dropdown→Plate→Backplate chain; deep tree-only chains lose the adjustment); the row-rect
    hit expansion is dropped, consistent with every migrated control. Verified: 169 tests,
    full workspace builds, cce-system-settings fonts/notifications render dumps
    content-identical (only the detached label's emission order shifts — adapter appends it
    after the widget's prims), cce-graph startup/open/close live A/B **byte-identical**
    (AE=0 open state on a clean run; run-to-run compositor translucency noise ~15k AE dwarfs
    any residual), cce-fonts + cce-test-interface smoke-run. App sweep: 8 repos (graph,
    text-editor, fonts, layout-interface, system-settings, data-editor, files,
    test-interface) — field types to `Adapted<Dropdown>`, raw casts to `.as_ptr_mut()`,
    two `&mut Dropdown` fn params in cce-files pages.
  - **5q — `TextBox` (widest-surface leaf; the clipboard/selection tier). DONE (render-dump
    + live-A/B verified, data-editor byte-identical).** New adapter surface: the `Input`
    clipboard quintet (`cut_selection`/`copy_selection`/`paste_from_clipboard`/`select_all`/
    `clear_text` — defaults replicate the whole-value `Element` defaults so earlier
    migrations keep their shipped behavior), `Paint::prepare_text` (TextBox's glyph shaping
    is load-bearing: `map_x_to_idx` reads the measured advances), `Layout::hit_row_rect`
    (restores the legacy row-substituted, side-label-inset hit geometry — cce-files'
    save-name box relies on row hits; earlier migrations' drop of it stands, opt-in),
    `Layout::adjust_row_rect` + `Layout::rect_assigned` (the width/max-width clamp on both
    rect paths; ungated scroll re-clamp on every `set_rect`), `Input::tracks_base_focus`
    (legacy TextBox's `focus()` never set the base flag — its detached label must not color
    as focused), and **`Paint::legacy_focus_highlight`**: the shared focus-highlight overlay
    the adapter suppresses for every migrated widget is re-enabled per-widget — legacy
    TextBox kept the `Element` default, and the focused editor's primary-tint wash
    (data-editor's teal editing surface) is real legacy behavior. Found the honest way: the
    first A/B came back 1.4M pixels apart; after restoring the overlay (replicated
    byte-for-byte in `Adapted::highlight_quad` + the `all_quads`/`paint_self` inclusion
    points), data-editor startup AND focused-editor states are **byte-identical (AE=0)**.
    The asymmetric legacy render split is preserved (non-rounded `extra_quads`: full-width
    background + disabled special-case; rounded `all_rounded_quads`: side-label inset, no
    disabled branch). Flagged approximations: releases re-check plain-rect containment
    (legacy hit-gated them through the row-substituted test); wheel is now hit-gated by the
    adapter (legacy hosts dispatched it to the hovered widget themselves). Verified: 169
    tests, workspace builds, settings fonts/processes dumps content-identical (processes
    modulo live PID/CPU data), cce-data-editor live A/B byte-identical. Sweep: 9 app repos
    (authenticator, data-editor, display-manager, email, files, fonts, layout-interface,
    system-settings, text-editor) + 5 in-crate embedders (treelist, scrolling_list,
    keybinds_control, multi_control's `InstancedWidget` variant, parameters_bg).
  - **5r — `Paginator` (value-embedded container: ButtonStrip + Vec<Page>). DONE (live-A/B
    verified, cce-layout-interface byte-identical across four states).** The model owns both
    embedded legacy widgets by value and proxies events to them (strip first, draining its
    click into the selection; then the selected page while not hidden); the container
    concern serves them via `container_children`/`child_visible` (strip always, selected
    page only), and `arrange_children` is the legacy `set_rect` body. New adapter surface:
    **`Layout::register_embedded_children(host_id, ctx)`** — legacy `tick`/`layout`
    re-registered the strip + pages into the ctx registry every frame, and that registration
    is load-bearing (the spatial grid is rebuilt from registered widgets; the registered
    strip is what blocks backplate drags over the sidebar —
    `backplate::tests::test_paginator_blocks_backplate_drag`); the adapter calls it from
    `Element::tick` and `Element::layout`, the legacy cadence. **`Paint::
    aggregates_child_extra_quads`** — legacy container `extra_quads` served the CHILDREN's
    chrome only, while the widget's own background quad lived in `all_quads` alone;
    cce-email and cce-layout-interface render the tab column through `extra_quads` over
    their own backgrounds (emitting the bg there would double-blend), and cce-test-interface
    renders through `all_quads` (dropping the bg there would blank it). The adapter's
    `all_quads` now draws own prims from a shared `own_plain_quads()` helper instead of
    `extra_quads()` (identical for every prior migration) so the two views never
    double-serve. **`Paint::forwarded_highlight(ctx)`** — legacy `highlight_quad` forwarded
    to the strip's (the hovered-tab tint cce-layout-interface draws by calling
    `highlight_quad` directly); served only through that getter, kept out of
    `all_quads`/`paint_self` (gated on `legacy_focus_highlight` now — where the strip's own
    aggregation already carries it, as legacy container `all_quads` overrides did).
    `PageSelector` + `MenuController` ride the existing `Input` capability hooks
    (cce-test-interface reaches `sidebar_w` through an `as_page_selector()` downcast on
    `dyn Element`). Ported bug-for-bug though unused workspace-wide: the `pages` container
    surface (`add_widget_to_page`/`set_pages`/…) — every app manages page content itself
    keyed on `selected_page()`. Verified: 171 tests, all four consumer apps build,
    cce-layout-interface live A/B **byte-identical (AE=0)** on idle, File-tab hover,
    Page-tab click, and Page-selected hover (exercises extra_quads aggregation, highlight
    forwarding, labels, and the click→selection→page-switch path);
    cce-test-interface A/B equals its launch-to-launch noise exactly (same 7.5k-px bbox —
    animated waveform phase; the `all_quads` gallery path contributes zero residual).
    Sweep: 3 app repos re-typed to `Adapted<Paginator>` (email, files, layout-interface);
    test-interface's `Box<dyn Element>` gallery needed no change.
  - **5s — `ParametersBg` (the designer's parameter panel; the last real container). DONE
    (live-A/B verified, cce-designer pixel-equivalent across four states).** The model caches
    its laid-out rect via `Layout::rect_assigned` (all row geometry derives from it — the
    TextBox pattern; `arrange_children` is the legacy child-stacking tail, visible-gated by
    the adapter exactly as legacy gated it), keeps the value-owned per-row widget vecs
    (mostly `Adapted<W>` already) plus the raw-pointer `children` list on the 5n container
    hooks, and ports the full bespoke event surface into `on_event` arms: the every-press
    dispatch chain (`gates_presses=false` — the panel consumes every left press, scrollbar
    thumb drag math, popover-first ordering, per-row-type dispatch with value commit-back,
    the inline emacs-flavored code editor) and the host-driven drag surface on the `Input`
    drag hooks. New adapter surface: **`Paint::serves_legacy_labels` /
    `legacy_labels_with_font_and_bounds(rect, ctx)`** — the text sibling of the 5m
    dual-geometry hatch: the standard bridge gives every own label ONE font and ONE clip
    rect, but this panel assigns them PER LABEL (viewport clip everywhere, code-box clip +
    monospace inside a code row); served as a full replacement, children included. Also
    **`EventCtx::widget_addr()`** — the wheel arm's occlusion check
    (`is_coordinate_covered`) keys on the adapter's address, which `on_event` couldn't
    reach. Reuses 5m's plain-quad hatch for the designer's raw `extra_quads` render path
    (row chrome, section borders, code cursor, scrollbar — with the panel's translucent
    PARAM_BG plate deliberately NOT emitted: the designer draws it from
    `color()`/`corner_style`, the 5l double-blend trap). `window_runner`'s
    `get_child_widget_for_quad` downcast keeps working unchanged (`as_any` exposes the
    inner type; the 9 pub sub-widget fields stay pub). Flagged approximation: the legacy
    scrollbar-press called `self.focus()` (base flag only — nothing reads it; the highlight
    keys on the ctx focus slot and the designer tracks panes by index); dropped.
    Verified: 175 tests (4 new: controller roundtrip + row layout, checkbox/code-editor
    commit flows, hatch split, overflow scrolling), cce-designer builds with ZERO app
    changes (`Box::new(ParametersBg::new())` coerces), live A/B across startup /
    wheel-scrolled pane / dropdown-row click / second click identical except the bottom
    status strip — calibrated as launch-to-launch live-data noise (same 1447×21 bbox,
    77 px between two launches of the SAME baseline vs 81 px old-vs-new). The wheel state
    changed 107k px within a build and matched across builds, so the event path is
    genuinely exercised. (The params-pane Circular-Pane dropdown not opening on click is
    the pre-existing app behavior recorded in 5o, identical in both builds.)
  - **5t — the deferred leaves: `Float3`, `LayoutPreview`, `PreviewState`, `StatusBar`.
    DONE (live-A/B verified: cce-files byte-identical ×3 states, cce-designer params pane
    byte-identical ×3 states, cce-system-settings byte-identical whole-window).**
    Consumer survey first (it reshaped the work): every external `Float3` grep hit is the
    MATH type (designer `GAttribute::Float3` / wgpu `Float32x3`) — the widget's only
    consumer is ParametersBg, whose pub-field reach (`values`/`mins`/`maxs`/`edit_buffer`/
    `editing_idx`) flows through `Deref` unchanged; LayoutPreview has ZERO consumers
    (definition + re-exports only); PreviewState is cce-files' preview pane; StatusBar has
    six construction sites across five apps plus the demo.
    **Float3**: rect cached via `rect_assigned` (`get_row_rects` is pub API with no rect
    param), readout-edit + track-drag into `on_event`/the drag hooks, the readout click's
    legacy `focus::set_focused(self)` rides `EventCtx::request_focus`, commit-on-unfocus via
    the FocusOut arm. **LayoutPreview**: mechanical (paint = quads + text; the duplicated
    SimNode match collapsed into one helper). **PreviewState**: the widget the 5s labels
    hatch was built for — canvas quads flow from `paint`, canvas labels (per-label
    monospace for content lines) through `serves_legacy_labels`; plain `text_labels` stays
    EMPTY like legacy (emitting the text as prims too would double-render under container
    aggregation — the legacy scene path showed no text either, preserved). The adapter
    gains a blanket `impl Default for Adapted<W: Default>` (cce-files constructs it via
    `Default`); the app's struct-literal update became field mutation (the private cached
    rect can't ride functional-update syntax — and now survives updates instead of zeroing
    until the next layout pass). **StatusBar**: the MenuBar parent-coupling pattern
    (tracked parent, backplate-aware color/text-color/blur, corners-against-parent at the
    parent's radius) plus ONE new hook — **`Paint::text_items()`**: pre-shaped glyphon
    buffers for the legacy `get_text_items` path, which prim-derived text cannot serve (it
    returns borrows of widget-owned buffers); cce-status-interface drives the bar by hand
    (`prepare_text` → `get_text_items` into its own paint) and data-editor/system-settings
    host it as a Backplate child. `inline_label` keeps `Element::set_text`'s base-label
    write from leaking a detached label. Deliberately preserved asymmetry: NO
    `widget_font`, so the container text path keeps rendering the bar's text in the
    default font while the buffer path uses the statusbar font, exactly as legacy.
    Verified: 177 tests (Float3 readout/drag flow; StatusBar manual-host pipeline —
    covering the path of the one app, cce-status-interface, not A/B'd live: it is the
    user's session status bar). Sweep: 4 app repos (files: field + literal→mutation;
    data-editor: field + one raw `*mut StatusBar` cast → `as_ptr_mut()`; system-settings +
    status-interface: field types).
  - **Phase 5 widget migration COMPLETE.** Everything remaining on `impl Element` is
    embedded-base machinery by design: the containers
    (Layer/Container/Page/Plate/Backplate/ScrollBox/List/…) dissolve via Phase 6 scene
    adoption instead. Then delete `Element` + `Adapted` once the last widget is across, at
    which point the `as_*_controller` pairs and the `Input` capability hooks die together
    (callers hold concrete types or `&dyn XController`).
- **Phase 6 — Per-app migration.** Move each `cce-*` app onto the new core; delete legacy paths
  once the last app is across. **Definition of done per app:** the whole frame — geometry AND
  text — is one `Application::display_list()` (+ `display_list_text()`), layout runs through
  the scene solver where the app has a real tree, events reach widgets through routed dispatch
  rather than hand-rolled per-widget loops, and no embedded-base container
  (Layer/Page/Plate/Backplate/ScrollBox/List) is load-bearing. Seven apps already feed
  geometry through `display_list()` (colors, data-editor, files, fonts, graph, text-editor,
  system-settings) — their remaining gaps are text, layout, and events.
  - **6a — display-list text. DONE (live-A/B verified via cce-notifier).** `Prim::Text` now
    carries `font: Option<String>` + `bounds: Option<[f32;4]>` (`PaintCtx::text_with`; plain
    `text` emits None/None), and the backend renders a list's Text prims through the glyphon
    pass — shaped via the shared `get_text_buffer` cache, clipped to the item clip ∩ the prim
    bounds, held in `EngineState::dl_text_items` so the `TextArea`s can borrow the buffers.
    **Opt-in via `Application::display_list_text()` (default false)**: the seven Phase 3
    adopters' lists already carry Text prims that those apps ALSO push as `TextItem`s —
    rendering both would double-draw; each app flips the flag when it stops pushing its own.
    `Application::view`/`text_items` gained no-op defaults so a fully migrated app implements
    neither. Known limitation (scoped out, not a bug): the legacy `text_areas`
    popover-occlusion clip is not applied to display-list text yet — a popover plate does not
    hide list text beneath it (text draws after all geometry); apps with popovers keep their
    own text path until that lands. The 5s/5t labels hatches become expressible as prims once
    hosts consume lists directly.
  - **6b — `cce-notifier` (first app fully on one path). DONE (live-A/B on a private D-Bus
    session: text/accent pixel-identical; 60-px residual is compositor translucency noise in
    the alpha-0.9 background region).** The whole frame is one display list (accent quad +
    three `text_with` prims in the configured bundled family); deleted: the app-side
    `FontSystem`, the `TextItem` cache, `rebuild_layout`, and the scale/rebuild bookkeeping.
    Non-interactive, so no event surface. This is the reference shape for a minimal Phase 6
    app.
  - **6c — `cce-wallpaper` + `cce-screenaver` across; `display_list` gains `(size, scale)`.
    DONE (wallpaper live-A/B AE=0; screensaver background-fill verified live, sim quads are
    the same mechanical loop).** The Phase 6 frame entry point now receives the frame's
    logical size and HiDPI scale like `view` did (fullscreen apps size geometry from it);
    mechanical sweep across the eight implementors. Both apps' dead `TextItem` caches
    deleted.
  - **6d — the paint walk carries per-widget fonts + clip rects. DONE (179 tests; cce-graph
    live A/B shows zero structural diff — all residual below the 8% translucency-noise
    amplitude).** `Adapted::paint_self` no longer forwards `Paint::paint`'s plain Text prims:
    it re-emits the geometry verbatim (through the ctx so the walk's offset/clip apply
    once) and serves text as `text_with` prims from the SAME views the standard text bridges
    use — `own_labels_with_font_and_bounds` (prim text + detached base label, `widget_font`,
    `text_bounds` or the scroll-ancestor clip) or the 5s per-label hatch verbatim (caveat
    noted in-code: the hatch contract includes raw container children). This makes a
    `paint_tree` display list's text renderable-correct for migrated widgets, which is the
    precondition for the seven adopters flipping `display_list_text`. Found and recorded on
    the way: the LEGACY `Element::paint_self` default drains the child-aggregating
    `text_labels` for legacy containers, so scene-path text double-emits under the walk for
    trees that still contain Layer/Page/etc. — invisible today (text prims unrendered
    without the opt-in), but it means an app can only flip `display_list_text` once its
    tree is embedded-base-free. Consistent with the dissolution plan; revisit per app.
  - **6e — `cce-colors` flips `display_list_text` (first of the seven adopters). DONE
    (live-A/B'd; slider drag re-verified via wlrctl).** The whole frame is one
    `display_list()` — the rebuild check moved off the deleted `view`/`view_rounded_quads`
    overrides, `rebuild_layout` keeps the flat PageContent text tuples and the list emits
    them as `text_with` prims; deleted: the app-side `FontSystem`, the `TextItem`
    assembly, and the `CCE_LEGACY_PAINT` fallback. The A/B exposed a PRE-EXISTING runtime
    bug this fixes: the app shaped its `TextItem`s with its own
    `create_font_system_with_system_fonts()`, whose fontdb face IDs don't resolve in the
    engine's render `FontSystem` — every `font: None` label (slider names, channel
    values, hex readout) was INVISIBLE at runtime in the baseline (only the bundled-font
    button labels survived). Shaping through the engine's FontSystem (the dl-text path)
    is what makes the text render at all. Note for the remaining adopters: an app-side
    `FontSystem` is not just dead weight, it is a live font-resolution hazard — check
    each app's text for the same silent invisibility before trusting its baseline
    capture. cce-colors' safety: no popovers, no paint_tree (flat-list bridge), so
    neither the popover-occlusion gap nor the 6d embedded-base double-emit applies; its
    root `Backplate` remains for legacy layout/events (full definition-of-done still
    pending events + layout).
  - **6f — `cce-files` flips `display_list_text`. DONE (live-A/B pixel-identical, AE=0;
    dropdown popover + breadcrumb context menu re-verified live).** Same mechanical shape
    as 6e: rebuild check into `display_list()`, `rebuild_layout`'s text tuples emitted as
    `text_with` prims, `view*`/`CCE_LEGACY_PAINT`/TextItem assembly deleted. Two deltas
    from colors: the app-side `FontSystem` STAYS (TextBox/List `prepare_text` measurement
    still needs it — it's `create_font_system()`, bundled-only, so no 6e invisibility
    hazard), and popover occlusion needed nothing from the engine — cce-files folds
    popover/context-menu/dialog occlusion into each text's clip bounds app-side
    (`occlude_against`, both axes), and those bounds ride along as prim bounds. That's
    the general pattern for flat-list-bridge apps with popovers: the engine's missing
    dl-text occlusion pass only blocks apps that rely on the DEFAULT `text_areas`
    popover clamp (`ui_context().active_popovers`), e.g. widget-tree apps whose popovers
    register through `register_popover`.
  - **6g — popover occlusion for display-list text lands; `cce-system-settings` flips
    `display_list_text`. DONE (live-A/B pixel-identical, AE=0; page-dropdown popup, page
    switch, service-list scroll, live process refresh exercised).** Engine:
    `popover_occlusion_clamp` extracted from the default `text_areas` mapping and applied
    to `dl_text_items` in `render()`, driven by `ui_context().active_popovers` — the
    display_list_text known limitation is gone; apps whose popovers register through
    `register_popover` (which `render_widget` does for any open `popover_rect`) can flip.
    App: same mechanical shape as 6e/6f, with the app-side `FontSystem` kept for
    button-label width measurement (centering) and the search-match highlight rect, and
    the wheel fast-path mutating tuple y/bounds in place exactly as it did TextItems.
    Popovers/context menu were never main-surface here: they draw on the engine's
    xdg-popup surface via the `render_popovers` collector, which is orthogonal to the
    flip.
  - **6h — `cce-text-editor` flips `display_list_text`; `Paint::text_font` lands. DONE
    (live-verified: frame matches baseline modulo a uniform ~2px baseline shift from
    engine line-height shaping; File-menu popup + occlusion of editor text beneath it;
    click cursor placement identical to baseline).** The FIRST app rendering scene-walk
    text prims — its tree (Adapted Dropdown + Adapted TextBox, no embedded-base
    containers) was exactly the 6d-safe shape, unblocked by the 6g occlusion clamp.
    Engine: `own_labels_with_font_and_bounds` split into a parameterized helper — the
    tuple getters keep `widget_font` for every label (unmigrated apps byte-identical),
    while the walk view (`own_labels_for_walk`) attaches the new `Paint::text_font`
    (default = `widget_font`) to prim-derived labels; the detached base label keeps
    `widget_font`. TextBox overrides `text_font`: a customized `font_family`/`font_size`
    serves the bare family name so the value text draws in the widget's own font at the
    label's size (the control-font string's size suffix would otherwise override it) —
    this is what keeps the editor monospace. App: view()'s side effects (registration,
    initial focus, relayout, popover registration) moved into `display_list`; chrome
    text emitted as prims in the system mono family; `rebuild_text_items` + hand-rolled
    Buffer shaping deleted; the app `FontSystem` stays for `editor.prepare_text` (glyph
    advances — cursor↔pixel mapping).
  - **6i — the 6d trap is FIXED in the walk; `cce-data-editor` flips `display_list_text`.
    DONE (live-verified: full-frame parity modulo the uniform ~2px engine line-height
    shift; row selection → inline value editor + statusbar update; choice-dropdown
    popover renders and occludes rows beneath).** Engine, two changes that make a
    `paint_tree` list's text emit exactly once — the embedded-base-dissolution
    precondition is GONE for the flip step: (1) the legacy `Element::paint_self` default
    emits text only for LEAVES — the legacy container `text_labels` overrides
    (Backplate/Layer/Page/SplitBox/Plate) aggregate their children's labels, which the
    walk reaches itself; a legacy container with OWN text overrides `paint_self` (Plate
    now serves its label this way; SectionContainer still aggregates from internal
    non-child widgets and needs the same treatment if it is ever walked). (2) the walk's
    `renders_own_subtree` branch (TreeList) emits the subtree's text from the recursive
    `text_labels_with_font_and_bounds` — the walk never descends there, so the aggregate
    is that subtree's text once, with the tuple pipeline's fonts/bounds. App: the old
    `view()` body (registration, focus, relayout, inline-editor placement, popover
    registration) moved into `display_list`; `rebuild_text_items` shrank to a
    widget-state refresh (rebuild_tree, prepare_text, statusbar text); the toolbar file
    label is a prim; `add_element_labels` and the TextItem cache deleted (−240 lines).
  - **6j — `cce-graph` flips `display_list_text`. DONE (live-verified: static A/B
    residual is only the uniform ~2px engine line-height text shift; node selection,
    View-menu popover occluding the node beneath, control-panel toggle with its
    multi-line info text via the 6i container fix).** Same recipe as 6i; the app
    `FontSystem` deleted outright (no prepare_text dependency). Also fixed a latent
    Phase 3 loss found on the way: the loaded-image pixel quads and selection borders
    were pushed into `view()`'s plain quads, which the backend DISCARDS when
    `display_list` returns Some — images had only rendered under `CCE_LEGACY_PAINT`
    since the Phase 3 adoption; they now emit into the display list itself.
  - **6k — `Application::load_system_fonts` lands; cce-fonts opts in. DONE (live-verified:
    baseline previews Berkeley Mono (bundled) but drew Adwaita Mono (system-only) BLANK —
    the app's core purpose was broken for installed fonts; with the opt-in Adwaita renders
    as its own face).** The render-FontSystem design settled as a bool `Application` hook,
    consulted once at GPU init: the engine's `WgpuAdapter` FontSystem loads system fonts
    additively (bundled first, so fontdb face IDs stay aligned with every
    `create_font_system*` database — the alignment that makes app-side-shaped buffers
    rasterizable engine-side). This was the same face-ID-mismatch class as the 6e
    cce-colors bug, and it predates Phase 6 entirely.
  - **6l — `TextAttrs` lands; `cce-fonts` flips `display_list_text`. ALL TEN display_list
    adopters are now fully on the single paint path. DONE (live-verified: style popover
    renders with labels on top and occludes the text beneath via the ui_context-only
    registration; selecting Italic re-renders the alphabet in the italic face).** Engine:
    `Prim::Text` gains `attrs: TextAttrs { italic, weight }` (toolkit-plain — no glyphon
    types in the scene layer), emitted by `PaintCtx::text_attrs`, shaped by
    `get_text_buffer_attrs` (cache key includes them). App: same recipe, plus the popover
    drawn INTO the list (replacing `overlay_quads`) with labels bounded to the popover
    rect, and the open popover registered in `ui_context` ONLY — a global registration
    would spawn an empty xdg popup (no `render_popovers` here). Restored two more Phase 3
    view()-quad losses (panel borders, alphabet box) and fixed the alphabet's premature
    wrapping (legacy passed a LOGICAL width to `set_size` on a physical-unit buffer).
  - **6m — first container dissolution: `cce-data-editor`'s root Backplate. DONE
    (live-verified: A/B residual 43px over the 8% threshold — translucency noise;
    selection/editor/statusbar interactions exercised; held window drag not headlessly
    drivable, covered by the new unit test).** The root-Backplate dissolution recipe,
    now established: (1) the plate becomes prims replicating `Backplate::color()`/
    `corner_radius()` (page-low bg at active backplate opacity, config radius); (2)
    top-level widgets register directly in `ui_context`, parentless, and the paint walk
    runs per top-level widget in the old child order (composite widgets keep their own
    children — the splitter still owns its panes); (3) window dragging answers via the
    new `UiContext::drag_allowed_at` — the `is_movable_backplate_at` candidate walk
    minus the registered-movable-Backplate requirement, because the surface itself is
    the movable plate once the Backplate is gone.
  - **6n — `cce-graph`'s root Backplate dissolved; two engine input holes fixed. DONE
    (live-verified: A/B residual 14px over the 8% threshold; View menu → Control Panel
    toggle → panel renders through the walk).** The 6m recipe applied (plate prims,
    parentless top-level registration, walk in old child order, `drag_allowed_at`);
    popovers moved to the 6l ui_context-only registration (the global registration
    spawned a render-only xdg popup double-drawing the menu; `render_popovers` override
    deleted). Found en route, both pre-existing: (1) a press inside an OPEN popover's
    plate could start a window move and swallow the click when the widget beneath does
    not block dragging (Graph's edge-exclusive canvas hit) — both drag questions now
    veto via `point_in_active_popover`; (2) the engine's render-only popups took input
    with their default full input region — they now carry an EMPTY input region.
    Verification lesson recorded: the compositor drops pointer focus after each click
    (Leave with no re-Enter on in-window motion), so headless click sequences MUST
    re-park the pointer (`wlrctl pointer move -10000 -10000`) before every click — a
    skipped re-park looks exactly like an input regression.
  - **6o — graph's two Plates dissolved: `cce-graph` is EMBEDDED-BASE-FREE (the first app
    to get there via dissolution; text-editor was born free). DONE (dropdown row A/B'd to
    the solver's exact rects after switching to the bridge's own sizing entry,
    `Element::intrinsic_size` — a `measure` call returns display-label widths instead;
    control-panel area A/B: AE=0).** The transparent dropdown-row Plate (pure layout
    shim) became direct placement; the draggable control panel became app state + prims
    replicating Plate's exact visual (config plate color / drag tint, plate opacity,
    negative-alpha blur flag, border, radius) with the label walked standalone via
    Plate's centered-first-child rule. Panel drag reimplemented properly app-side —
    NB the legacy `Plate::on_cursor_moved` forwarded drags only to CHILDREN, so the old
    panel's own drag likely never moved it (manual drag check pending).
  - **6p — cce-fonts' root Backplate + all three Plates dissolved. DONE (live-verified:
    family select, style popover + occlusion, Oblique re-render, select-mode bar).** The
    6m/6o recipes at full width, plus the first APP-OWNED EVENT DISPATCH: a
    `dispatch_widgets` list replicating the Plates' forwarding — popover-first press
    pass, unfocus-on-missed-press, `drag_update` forwarding for dragging children
    (scrollbar thumbs), panel-grouped order. The A/B surfaced another legacy
    double-draw: `Plate::paint_self` aggregated child plain-quads while the walk painted
    the child again, double-compositing the ScrollBox background (~22 units darker) —
    the dissolved single-draw is the correct rendering. Remaining in fonts: `ScrollBox`
    (mid-panel scroll state/scrollbar) and `List` (browse-list scroll/frame) — the last
    two embedded-base types in the app.
  - **6q — fonts' ScrollBox + List dissolved: cce-fonts is FULLY EMBEDDED-BASE-FREE.
    DONE (live-verified: wheel scroll, scrollbar track-jump lands proportionally, family
    click after deep scroll + scroll_to_index, alphabet re-render in the new family).**
    Both were pure scroll frames in this app (List with columns=None; the rows are
    standalone Buttons), so they reduce to one app-owned `ScrollRegion` (~150 lines):
    scroll state, wheel, thumb-grab/track-jump/drag, hover-scoped keyboard scrolling,
    item-y math with List's silently adjusted item height (max(24, list font + 14)), and
    bg/track/thumb prims. The bg is a single list_bg layer — the legacy leaf-walk
    stacked rounded + plain copies (the translucent double-compositing class again);
    residual A/B delta is a 2px bottom-edge strip.
  - **6r — colors' and files' root Backplates dissolved, BOTH A/B'd to AE=0.** These are
    the flat-pipeline (render_widget) apps, and their dissolution surfaced the legacy
    aggregate's GLOBAL tuple-order contract: `render_widget(root)` emitted every
    descendant's PLAIN quads first (via `all_quads` aggregation), then the root's
    rounded bg, then every descendant's ROUNDED quads — so the root's translucent plate
    WASHES over the plain content (colors' muted slider gradients depend on it; a
    naive plate-first order renders saturated). Replication: per-child `render_widget`,
    partition the tuples by radius, and interleave [plain…, plate, rounded…]. Wheel in
    colors propagates per-slider; both apps answer dragging via `drag_allowed_at`.
    Files' view-dropdown popover + breadcrumb context menu re-verified live.
  - **6s — settings' root Backplate + StatusBar dissolved; `Dropdown::set_corner_frame`
    lands. DONE (WINDOW_PC tuple stream byte-identical; pixels AE=0; live: dropdown
    popover → page switch to Processes with statusbar text following, wheel scroll,
    service-list render).** Settings needed what colors/files didn't: its plate radius
    is a hardcoded 12, so the legacy aggregate's corner RESOLUTION mattered — a child
    plain quad flush with a window corner picks up the plate radius there (the
    `render_widget` extra-corners logic against the ROOT rect). The hand assembly
    replicates the full aggregate: child plain quads (window-clipped, root-corner-
    resolved), plate, child rounded quads, root-clamped text, in the old child order.
    Two parent couplings surfaced (the widgets read their Backplate ancestor):
    (1) StatusBar — bg falls back from the backplate-statusbar theme color to
    STATUS_BG, bottom corners round at the PARENT's radius, text color/font are
    backplate-specific; it dissolves outright (pure chrome in this app) into tuples +
    a `status_text` String. (2) Dropdown — the backplate-concentric corner cut walks
    for a Backplate ancestor and silently degrades to a plain rounded box when the
    walk finds nothing; new `Dropdown::set_corner_frame((rect, radius, corners))`
    hands it the frame explicitly and takes precedence. Also found (pre-existing,
    reproduced on the pre-6s baseline): the engine xdg-popup positioner anchors at
    the widget's BOTTOM edge regardless of the app's open-upward popover rect, so
    settings' page popover displays below the window while clicks land on the
    app-side (invisible, in-window) popover rect — the engine popup path's last
    consumer; fix when settings' popovers move to the 6l ui_context-only pattern.
    The root's `with_border` was never rendered (a rounded Backplate emits no plain
    bg quad; the border branch fires only on plain bg quads) — dropped, not ported.
  - **6t — settings' popovers + context menu draw INTO the frame;
    `Application::draws_own_popovers` lands. DONE (live-verified: dropdown popover
    in-window with page geometry occluded beneath, item click switches pages both
    ways, spinbox right-click context menu at cursor with dl-text occluded beneath,
    dismissal).** This fixes the 6s finding at the source: the engine's render-only
    xdg popup anchored at the widget's bottom edge regardless of the app's open-upward
    popover rect, so settings' page popover displayed BELOW the window while clicks
    landed on the app-side in-window rect. The app now runs the same
    `render_popovers` collector into its own tuple stream (appended above window/
    page/search content; kept out of the scrollable vecs so the wheel fast-path can't
    shift popover content) and deletes the override. Page-widget popovers
    (notifications/fonts menus) shift by −scroll_y — the subtraction the popup
    positioner used to apply — keeping display aligned with hit-testing under scroll.
    Engine: `draws_own_popovers` (default false) gates BOTH popup spawn triggers (the
    global popover registry and global context-menu visibility), and under the flag
    `render()` adds the visible context menu's rect to the dl-text occlusion overlays
    (the menu is engine-global state, not a `ui_context` popover; its own labels are
    exempt via bounds == rect). Remaining popup-path consumers: cce-data-editor and
    cce-text-editor (`render_popovers` overrides) — the popup surface, `ActivePopup`,
    `PopoverCollector`, and this flag all go away once they draw their own.
  - **6u — settings' Switcher + Page dissolved; the System page comes back from the
    dead. DONE (audio A/B: window tuple stream byte-identical, pixels AE=0;
    live-verified across six pages — spinboxes, context menu, page switching, wheel
    + fallback, System governor dropdown + scroll, notifications dropdown, fonts
    textbox focus).** The top two tree layers reduce to app state: the active page
    was always `app.current_page`, page scroll was already `scroll_y`, so what was
    load-bearing was Page's scrollbar child, its out-of-bounds event gate, keyboard
    scrolling, and being the propagate root. `dispatch_page_event` replicates the
    routing (OOB gate with scrollbar-drag bypass; scrollbar first with the y-unshift,
    then sections in reverse child order; PointerMove visits all, others stop at the
    first handler) against the app-held SectionContainer clones; the scrollbar is an
    app field whose quads collect into the window assembly's plain slot with the
    legacy one-frame-stale content height. Found on the way: the System page's
    widget-tree render path — the only page not on immediate-mode — SEGFAULTED at
    launch on the pre-6u baseline (raw-pointer one-time section/label tree; the
    use-after-free class this rebuild exists to kill). A complete immediate-mode
    view for it existed in the file, never wired to the `AppPage` impl; 6u flips it
    (labels → `sec.text`, InfoBoxes advance the section cursor, menus linked into
    the clone sections like every other page). Pre-existing, deferred to the
    List/ScrollBox dissolution: the processes lists' inner wheel is dead; the page
    scrollbar's right half sits in the compositor's 8px edge-resize zone.
  - **6v — settings' List/ScrollBox dissolved (all five lists). DONE (A/B render
    dumps on processes/packages/radios: rect streams byte-identical minus one
    duplicated pair per list, see below; live-verified — per-list wheel, page-scroll
    fallback, scrollbar track-jump that sticks, focus tint, package row click →
    selection + info fetch, scroll state surviving watcher rebuilds).** Every
    settings list was a pure scroll frame (`List` with `columns: None`; the pages
    draw the rows), so the recipe is fonts' 6q `ScrollRegion` ported app-side
    (`cce-settings/src/scroll_region.rs`) with the List-flavored visuals (1px
    focus/hover-tinted rounded border + inset bg) and the List-mirroring API the
    pages already used. Routing: `AppPage` grows `extra_dispatch_roots` — the
    `InteractiveListItem` rows dispatch directly as propagate roots (`Adapted`'s
    press/wheel hit-gate makes misses fall through, so root order is immaterial) —
    plus `handle_mouse_wheel` (after widget dispatch, before the manual page-scroll
    fallback: the legacy "inner ScrollBoxes take the wheel first" slot) and
    `handle_key_input` (hover/focus-scoped, before the page's scroll-key fallback);
    regions ride the existing pointer down/move/up hooks (audio's slider-drag slots).
    This FIXES the 6u-deferred dead inner wheel, and two latent visuals of the
    columns=None List path: `List::extra_quads`' early return never removed the
    ScrollBox bg quad, so `render_widget` emitted the border+bg pair TWICE (plain
    bg through the solid-border branch + `all_rounded_quads`) — the 6p double-
    composite class — with the scrollbar track/thumb sandwiched UNDER the second
    translucent bg wash. Single-drawn now; the inner scrollbars are visible for the
    first time. Only other A/B delta: item-label clip bounds relax by ScrollBox's
    4px text inset (rows are fully-visible-culled, nothing renders in that band).
    Replication trap for other apps: the region's `focused` is a local bool
    (press-inside sets, press-miss clears) standing in for the global
    `focus::set_focused(scroll_box)` — ctrl-nav can no longer land on a list, and
    the focused border tint shows through the translucent bg as a green wash
    (legacy did this too, darker under its doubled bg). Not headlessly drivable,
    user spot-check pending: held thumb drag, arrow/PageUp/Down over a hovered list.
  - **6w — settings' SectionContainer dissolved; cce-system-settings is
    embedded-base-FREE. DONE (A/B render dumps: all nine pages byte-identical
    modulo live data — the sections never painted; live-verified — notifications
    spinbox + menu open/select with in-frame occlusion, audio spinbox round trip
    through pactl and the watcher, processes filter-box click-to-focus, services
    list wheel).** The per-rebuild section clones were pure event/focus plumbing:
    propagate roots whose `container` children were the pages' widgets, plus the
    ctrl-nav focus targets. `AppPage::section_widgets()` (one widget-pointer group
    per section, old count/order) replaces `get_section_containers` +
    `link_children` + `clear_children`; the widgets dispatch directly as propagate
    roots flattened in the legacy order, and section-level keyboard focus is an
    app-side index, single-slot with the global widget focus exactly as when both
    lived in `FOCUSED_WIDGET` (entry → section 0; ctrl+j/k cycle; ctrl+i descends
    to the section's first widget — the legacy walk went through the
    header/container intermediates; ctrl+u ascends from a widget to its section;
    a focus-taking click and page switches drop the highlight). Also killed a
    latent use-after-free of exactly the class this rebuild targets: the focused
    section clone was dropped and reallocated EVERY rebuild while the global
    focus pointer kept aiming at it — it survived only because same-size Vec
    reallocation tends to reuse the freed block. Ctrl-nav is not headlessly
    drivable (no virtual-keyboard protocol) — user spot-check pending.
  - **6x — data-editor + text-editor off the engine popup path; the render-only xdg
    popup machinery is DELETED. DONE (live-verified: text-editor File menu open +
    item click; data-editor recent-files menu → config.kdl load, tree context menu
    with occlusion + Copy Key through the clipboard; settings page dropdown +
    page switch unaffected after losing its gate).** Both apps now collect their
    ui_context-registered popovers via `PopoverCollector` and emit them last in
    the display list (data-editor appends the global context menu too), labels
    bounded to the overlay rect — the 6l/6t recipe; registration is
    ui_context-only. With the last consumers across, the engine sheds the whole
    popup path: `ActivePopup` (wgpu surface + viewport + vertex buffer per
    popover), the xdg positioner/spawn/despawn block in the event loop, the popup
    render pass, the popup-surface pointer-coordinate translation, the
    `PopupHandler` + `delegate_xdg_popup` plumbing, and the
    `Application::render_popovers` + `draws_own_popovers` hooks (settings'
    override removed; the context-menu dl-text occlusion rect is now
    unconditional). Every popover in the workspace is app-drawn, in-frame, where
    it hit-tests — the 6s below-window-popover class of positioner bugs is
    unrepresentable. NOTE: the compositor-side dismissal in `PopupHandler::done`
    (unfocus popovers + hide context menu when the popup was dismissed) went with
    it — in-frame apps already own dismissal (press-outside), same as
    fonts/settings. The global `widget::popovers` registry is now write-only
    (apps still clear/register into it) — delete it with the legacy paths.
    Drive-by: cce-designer had not compiled since 6k (direct `WgpuAdapter::new`
    call missing the new `load_system_fonts` bool) — fixed.
  - **6y — files' SplitBoxes + BrowseContainer/NetworkContainer dissolved. DONE
    (browse page A/B: zero >8%-amplitude pixel diffs; network page's only diff is
    a removed paint bug, see below; live-verified — row select, double-click
    navigation, view-dropdown page switch, divider hover tint via hover-on/off
    crop diff, preview populate).** The split reduces to an app-owned `SplitPane`
    (frac + divider drag/hover + divider quad — the SplitBox two-child horizontal
    math verbatim); the pane containers were pure layout shims whose child copies
    the pages have always re-rendered on top (the Phase 0 double-paint), so the
    window assembly now emits only the divider quad and the preview pane
    (`render_widget` at the right pane rect, text clamped to the pane like the
    legacy SplitBox bounds clamp). Killed on the way: the left pane's under-copy
    double-compositing every translucent quad, including the NetworkContainer's
    full-width breadcrumb-copy strip that visibly leaked behind the graph page's
    top bar — the exact class the Phase 0 stopgap patched for Browse only.
    Verification trap for the log: `wlrctl` pointer warps land as Enter WITHOUT
    Motion — nudge (`move 2 2`) after warping or app hover state never updates
    (cost an hour chasing a "broken" divider tint that was fine). Held divider
    drag is not headlessly drivable — user spot-check pending.
  - **6z — files' List dissolved; cce-files is embedded-base-FREE. DONE
    (live-verified: row click select with preview/details update, double-click
    navigation, breadcrumb navigation, wheel scroll with selection retained,
    hover tint, item count; 3 new RowList unit tests).** The column-mode List
    flavor ports verbatim to the app-owned `RowList`
    (`cce-files/src/row_list.rs`): column_bounds (Flex/Absolute/RightOffset),
    row virtualization + hit math, 400ms double-click, the scrollbar, and the
    cell layout (icon column, primary/secondary tints, char-estimate truncation,
    viewport-inset clip bounds). The in-List search box became a standalone
    BrowseState TextBox; the open/close shortcuts and SearchChanged plumbing
    move app-side (close returns the empty SearchChanged the legacy
    just_changed flag produced). Two fixes: the 6v sandwich again
    (render_widget emitted scrollbar + row overlays UNDER the rounded bg), and
    a NEW DISSOLUTION TRAP for the checklist — a dissolved widget no longer
    blocks window drags via its registered `blocks_backplate_drag`, so
    `is_movable_backplate_at` must veto its rect app-side; without it every row
    press became a compositor window-move grab and the app saw only the release
    (looked exactly like a dead click). Kept legacy: the view's
    scroll-into-view snaps the wheel back while the selected row would leave
    the viewport. Search typing not headlessly drivable — user spot-check
    pending.
  - **6aa — data-editor's SplitBox dissolved. DONE (live-verified: empty-state
    pixels identical modulo the cursor sprite; config load via the File menu,
    tree wheel, tree row select with the inline value editor, divider hover tint
    via crop diff).** The 6y `SplitPane` recipe on the scene-walk app: panes
    positioned directly from the pane rects and walked as separate roots, the
    divider quad emitted in the splitter's old walk slot. Removes the app's last
    raw-pointer child container and retires the Phase 2b
    `scene::bridge::layout_subtree` showcase that drove the split (the layout
    engine's app-facing debut now waits for the routed-events/scene-layout
    phase). TreeList intentionally NOT dissolved: at ~1.8k lines of tree
    expansion/inline-edit/annotation logic it is a self-contained walked widget
    (renders_own_subtree) whose internal ScrollBox never leaks — porting it
    app-side buys no hazard reduction; it converts to narrow traits with the
    `Element` deletion instead. Held divider drag — user spot-check pending.
  - **6ab — cce-text-editor on routed events + scene-solver layout: the FIRST app
    fully on the target architecture, end to end. DONE (live-verified: menu-open
    pixels match the pre-change capture at 0.13% = cursor sprite; menu item
    click through the routed release; editor click focus; the
    focused-border-after-outside-click oddity reproduced byte-identically on
    the stashed pre-change binary — pre-existing).** Layout: the frame is a
    plain `Arena<LayoutBox>` tree solved by `scene::layout::compute_layout` —
    no Element in the loop, the solver used directly by the app (stretched
    column [top bar fixed 42 / content grow padded 10 / status fixed 30], menu
    a fixed leaf, editor growing) — and it reproduces the legacy hand-math
    rects exactly, clamps included. Events: each handler builds one `Event` and
    routes it through `UiContext::propagate_event` per root; the router owns
    press hit-gating, Enter/Leave synthesis, drag-target recording, and
    KeyInput-to-focused delivery, leaving the app take_change plumbing and
    app-level shortcuts only. This is the shape the remaining widget-tree apps
    (data-editor foremost) migrate toward, and the pattern the demo
    (`cce-ui/src/main.rs`) should teach.
  - **6ac — data-editor on routed events + scene-solver layout; the
    routed-events/scene-layout item is COMPLETE. DONE (live-verified:
    empty-state pixels match 6aa at 0.06% (cursor + caret); config load through
    the routed menu; tree row select → inline choice editor + raw-span sync;
    choice popover open/select; tree wheel).** Layout: chrome + panes are one
    solver tree (stretched column [menubar fixed 42 + File-menu leaf / content
    row grow with pad 10, gap = divider width, panes growing by the SplitPane
    fractions / statusbar fixed 30]) reproducing the 6aa hand rects exactly;
    the SplitPane keeps divider input state, its frame derived from the solved
    panes; the inline value editors stay hand-positioned (they float over tree
    rows). Events: all 30 direct dispatch call sites route one `Event` through
    `propagate_event` per root with the plumbing intact. THE ROUTING TRAP worth
    remembering: the router delivers KeyInput to the ctx-focused widget FIRST
    on every propagate call, so a legacy non-short-circuited keyboard chain
    would deliver a typed key to the focused widget once per call site
    (N-time character insertion) — short-circuit the chain on first handled,
    and re-gate any plumbing that keyed off WHICH call returned true onto
    widget state instead (Enter→ApplyValue now checks the value editor was
    editing when the key arrived). Keyboard flows not headlessly drivable —
    user spot-check (typing, Enter-apply, tree search, keybind recording).
  - **6ad — the demo rewritten as `DemoApp`, the reference `Application`. DONE
    (live-verified: button click, toggle with app-state re-assert, slider wheel
    nudge, dropdown popover open/select with the occlusion clamp visibly
    working, all through routed dispatch).** `src/main.rs` had never been the
    "reference Application" the docs claimed — it was a 1925-line fossil
    predating the engine entirely: a raw Wayland client with its own
    CompositorHandler/SeatHandler impls, its own wgpu state, and hand-copied
    tessellators. Replaced by ~450 teaching-commented lines on the full target
    architecture: display-list frame + display_list_text, solver-driven layout
    (with `shrink` demonstrated for min-width rows), routed events with the
    KeyInput short-circuit rule and state-gated `drain_widget_changes`
    plumbing, ui_context-only popover registration with the in-frame draw, the
    dissolved-root window plate, and `drag_allowed_at` window dragging. API
    footgun surfaced for the log: `Slider::set_value` takes the NORMALIZED
    0..1 value (`with_range` only scales `get_scaled_value`) — passing a
    ranged value silently clamps to 1.0.
  - **6ae — legacy deletion, part 1: the global `widget::popovers` registry is
    DELETED. DONE (write-only since 6x; the mod, its `render_widget` write, and
    the four apps' `clear()` calls are gone; settings' popover renders
    byte-identically after).** Part 1 also produced a CORRECTED precondition
    map for the rest of the deletion — the endgame list had been assuming "the
    last app is across," and it is not:
    - The `view*`/`text_items` paths CANNOT be deleted yet: SIX apps still
      implement them — cce-test-interface (2.1k), cce-authenticator (0.9k),
      cce-display-manager (1.5k), cce-email (2.5k), cce-layout-interface
      (3.8k), cce-status-interface (4.2k). Each needs its own Phase-6-style
      migration (display-list flip at minimum; dissolutions as found).
      Suggested order: smallest/least-critical first (test-interface,
      authenticator — NOTE it may be the lock screen, verify carefully),
      status-interface last (layer-shell, always-running).
    - The per-widget text getters are additionally load-bearing for the walk's
      legacy branches (`renders_own_subtree`, container `text_labels`
      aggregation) and the migrated apps' hand-rolled window aggregates
      (settings' `collect_window_child`, files' assembly) — they go when those
      consumers move to `paint_self`-only trees.
    - `Element` + `Adapted` go last, after both of the above; TreeList
      converts to narrow traits then.
  - **6af — cce-test-interface across (1 of 6). DONE (A/B: zero >8%-amplitude
    pixel diffs; live-verified — full gallery render, page-dropdown popover
    in-frame, page switch updating the MenuBar title and the status prim).**
    The recipe for the remaining five: move the `view()` +
    `view_rounded_quads()` bodies into `display_list()` in the engine wrapper's
    order (ROUNDED first, then plain, then popover rects — the wrapper reversed
    the intuitive order and apps' visuals bake it in), and re-emit the
    `rebuild_text_items` assembly as `Prim::Text` built fresh per frame,
    deleting the cache + its invalidation call sites + the app FontSystem +
    any `text_areas` override (its extra areas become prims).
    `custom_vertices` stays. Remaining queue: authenticator (verify carefully —
    lock screen), display-manager, email, layout-interface, status-interface.
  - **6ag — cce-authenticator across (2 of 6); the flip FIXED runtime-invisible
    text. DONE (live-verified --standalone + CCE_AUTH_SIMULATE: full dialog text
    renders, zero font-ID warnings — was hundreds per frame — fingerprint-scan
    click drives the animated glow + hint).** It is an xdg-toplevel polkit auth
    dialog (NOT a session lock — safe to run; needs `--standalone` +
    `CCE_AUTH_SIMULATE=1` to show a window without a live polkit request, and it
    auto-exits ~3s in simulate mode so capture fast). The single `view()` (both
    geometry and text) → `display_list()`; the `text_items` assembly → prims;
    app FontSystem / make_text_buffer / text_items field+getter deleted. The
    6e face-ID class again, and worse here — the app used
    `create_font_system_with_system_fonts()`, so EVERY label was invisible at
    runtime; the migration is the fix. General lesson reinforced: any
    legacy-path app with its own FontSystem is a latent-invisible-text
    candidate — don't trust its baseline capture.
  - **6ah — cce-display-manager across (3 of 6). DONE (A/B: 131px AE = 0.004%
    caret blink, zero >8%-amplitude diffs; live-verified --greeter renders
    identically).** The greetd login greeter — run the GUI standalone with
    `--greeter` (daemon mode needs root/greetd). Bundled fonts, byte-match flip.
    view() + view_rounded_quads() → display_list() (rounded then plain, both
    skipping the card); rebuild_text_items → prims; FontSystem / make_text_buffer
    / info_buffer / text_items machinery deleted. The card — a soft radial-glow
    blob drawn with the circular clip disabled — STAYS in custom_vertices
    (escape-hatch layer, on top, untouched). REPO HAZARD hit here: these crates
    live under ~/Dropbox, and a Dropbox sync reverted the edited main.rs to disk
    AFTER build+test but BEFORE the commit landed (a harness-interrupted commit,
    exit 144, left the tree clean at the old file) — had to re-apply and commit
    immediately. Verify `git log`/`grep display_list` actually stuck before
    moving on.
  - **6ai — cce-email across (4 of 6); the flip FIXED invisible list/detail text.
    DONE (live-verified: inbox list of 3 emails, detail placeholder, and on click
    the full detail view — subject/From/To/Date/body + Reply/Delete/Mark-Unread
    toolbar).** view() body (all plain quads) → display_list() via a small
    `__EmailQuadSink` shim mapping the ported quads.push/extend to PaintCtx::quad;
    rebuild_text_items → emit_text_prims(&mut pc). The 6e face-ID class AGAIN
    (create_font_system_with_system_fonts): the list rows, detail metadata/body,
    and placeholder were all invisible — only the paginator tabs showed. Fix:
    switch the app FontSystem to bundled create_font_system() (KEPT for the
    TextBoxes' prepare_text measurement — now matching the engine render FS) and
    render via prims. Note the AE-vs-baseline metric is misleading for these
    invisible-text fixes (tiny % of dark-on-dark pixels change) — judge by
    whether text APPEARS, not by AE.
  - **6aj — cce-layout-interface across (5 of 6); needed + consumes the new
    boxed-text prim; the flip FIXED invisible text. DONE (live-verified: the
    whole properties/geometry/alignment/add-elements UI renders where the
    baseline showed nothing; 104→0 font-ID warnings).** This app forced the
    boxed-text feature (previous commit): its canvas Element::Text boxes need
    word-wrap + h/v alignment, unrepresentable as a plain Text prim. view() +
    view_vectors() → display_list() (a __LayoutQuadSink shim for the
    quads.push/extend body; vectors → PaintCtx::vector); rebuild_text_items →
    text-prim tuples carrying an optional TextLayout, emitted via text_with /
    text_boxed. Same 6e face-ID class (create_font_system_with_system_fonts);
    fixed by bundled FS + prims. VERIFY GAP: the canvas boxed prim itself
    (wrap/align on the page) could not be pinned headlessly — a placed text box
    defaults to page x=40, behind the ~540px properties panel, and there is no
    virtual keyboard to type a clear coordinate; it renders via the identical
    dl_text_items path as the confirmed-visible text. User spot-check: place +
    drag a text box onto open page, confirm wrap + alignment.
  - **6ak — cce-status-interface across (6 of 6, LAST legacy-path app). DONE
    (live-verified: an isolated `--module clock` instance renders "Friday, July
    10, 2026 … PM" with its rounded background pill through the display list).**
    The persistent layer-shell bar. view() + view_rounded_quads() bodies move
    into display_list() (rounded boxes, then status-bar bg / module rects /
    separators as prims, in the wrapper's ROUNDED-then-plain order); module text
    becomes fresh Prim::Text each frame. overlay_quads() stays a separate on-top
    pass (tray hover highlights over text). The status bar's OWN text is never
    set in this app (get_text_items was a no-op), so its text_items()/custom
    text_areas() overrides are deleted; self.font_system is kept only for the
    modules' measure-then-position shaping. Mechanism: modules build a
    StyledLabel to measure width, then emit via the new StyledLabel::into_prim
    (cce-ui 005a53f) through a draw_label helper — the vertical bar's centered
    per-char text rides the boxed-text TextLayout. Bundled create_font_system(),
    so NO 6e invisible-text hazard — a byte-match flip. Full multi-module A/B was
    avoided (the no-arg binary is a launcher daemon that would spawn a bar
    conflicting with the user's live one); a single isolated module segment was
    the test surface.
  - **All six legacy-path apps are now across.** The `view*`/`text_items`
    Application-trait deletion is unblocked.
  - **6al — legacy `view*`/`text_items`/`text_areas` DELETED. DONE (16 client
    apps compile; 180 tests pass; settings live-verified).** With every app on
    `display_list()`, the legacy geometry/text trait surface was dead code:
    removed `view` / `view_rounded_quads` / `view_vectors` / `text_items` and the
    default `text_areas` mapping from the `Application` trait; in `render()`,
    dropped the `quads`/`rounded_quads`/`vectors` collection + the tuple-wrapping
    `None =>` branch (so `dl = display_list().unwrap_or_else(empty)`) and the
    `text_areas()` call (an empty `areas` vec the dl-text loop fills). KEPT:
    `overlay_quads` (status-bar tray hover), `custom_vertices` (display-manager
    card, test-interface gallery), `display_list` / `display_list_text`. Pure
    dead-path removal — every implementor already took the `Some(dl)`/empty-text
    arms. (cce-designer + cce-cloud drive `WgpuAdapter` directly, never implement
    `Application`, so they're untouched.)
  - **6am — dead widgets SplitBox / MultiControl / KeybindsControl DELETED.
    DONE (cce-ui + all 18 apps compile; 175 tests pass; settings A/B AE=0).**
    Phase-6 dissolutions orphaned all three — no client app (nor live cce-ui
    path) constructs them. Removed the files, re-exports, the two
    `get_*_sub_widget_info` helpers + their `render_widget`/`window_runner`
    downcast blocks (fire only when the widget IS that type — none is, so
    behavior-preserving), the dead `name == "MultiControl"/"KeybindsControl"`
    span-full terms, and `render_widget`'s now-redundant `let mut corners = …`
    shadows. Each carried the full legacy text-getter aggregation, so this trims
    a big slice of the getter consumer graph.
  - **Per-widget text getters — GATED, not yet deletable.** Investigation
    (6am) established that every getter still has a LIVE consumer, so none can
    be removed until those move to `paint_self`/prims first:
    - `get_text_items` → **cce-designer** (custom `WgpuAdapter` render loop in
      `render.rs`: text-buffer cache + curved-menu-text special cases). Designer
      + cce-cloud never implement `Application` — they were skipped by all of
      Phase 6 and still drive `WgpuAdapter` directly.
    - ~~`text_labels_with_font_and_bounds` + `widget_font` → `layout::render_widget`~~
      **DONE (6an).** render_widget now sources its text from the scene walk
      (`paint_root_into` → keep only `Prim::Text` → emit onto the RenderTarget),
      keeping its own geometry path; the prim carries the per-widget font+clip so
      the getters are gone from here. Settings A/B AE=0 on Accounts + (stash-based)
      the spinbox-heavy Audio page. The last difference vs the getter is
      widget_font→text_font, which coincides except for a custom-font TextBox.
    - ~~`.text_labels()` / `.text_labels_with_bounds()` → **four hand-aggregate
      apps** (email, authenticator, display-manager, layout-interface) whose
      `display_list()` emits widget text by calling the getter per widget.~~
      **DONE (6ao)** — see below.
    - Note: the "orphaned" containers Layer / Page are NOT deletable — Layer is
      the embedded base of the live Plate/Page; Page is embedded by the live
      Paginator (transitive liveness through inheritance, not direct app use).
  - **6an — render_widget off the getters (see above). DONE (all 18 apps
    compile; 176 tests; settings A/B AE=0 on Accounts + Audio).** One of the
    three getter-consumer classes cleared.
  - **6ao — the four hand-aggregate apps off the getters. DONE (176 tests;
    all four A/B-verified live).** New `scene::painter::append_widget_text(ui,
    &dyn Element, &mut PaintCtx)`: walks the subtree and appends only its
    `Prim::Text` items — per-widget content font, walk clip composed into prim
    bounds (the 6an recipe as a reusable helper). Every per-widget
    `.text_labels()`/`.text_labels_with_bounds()` call in email, authenticator,
    display-manager and layout-interface replaced with it. A/B results:
    email + layout-interface byte-identical; authenticator + display-manager
    identical except widget-owned labels now render in the widgets' configured
    control font (legacy aggregates dropped the font to `None` — the same
    widget_font→text_font delta as 6an, here visible because these apps
    configure a monospace control font).
    - **Display-manager UAF found + fixed:** its `new()` linked the ui_context
      tree and captured `focused_widget` while the State was a stack local, so
      every registered pointer dangled after the move — the walk's child
      descent was the first render-path consumer to dereference them (abort on
      a garbage-length alloc); `propagate_event` and the `all_*` child
      aggregation read the same stale pointers all along. Fix: per-frame
      idempotent `relink_tree()` (register/link are id-keyed) + initial focus
      re-derived from the boxes' own focus flags. Also: its flat `widgets_iter`
      lists the card AND the card's children, so text moved to walking the TRUE
      roots (bg + root_container) — flat would double-emit — and `LoginCard`
      (a container with own, non-aggregating labels) got the Plate-style
      `paint_self` override for its two header labels.
    - Lesson for the getter deletion: an app whose tree is linked from `new()`
      by value is a dangling-registry candidate — audit any remaining
      `link_parent_child` calls made before the owning struct reaches its
      final address.
  - **6ap — cce-designer's render loop off the getters. DONE (default +
    circular-pane A/B pixel-identical; add_node label renders via the new
    path; full workspace builds).** The custom `WgpuAdapter` loop's two text
    sources (`get_text_items` widget-buffer fast path +
    `text_labels_with_font_and_bounds` fallback) became one walk: per
    non-menubar widget, `append_widget_text` → text prims, shaped app-side in
    `text_buffer_cache` with the same size*1.4 metrics the fallback always
    used. Per-widget special cases (plate-ancestor bounds, circular cull,
    network opacity, curved-ring feed) operate on prim fields unchanged. The
    curved-ring branch is unreachable today (menubars are skipped before its
    condition) — preserved verbatim, flagged for a future dead-code decision.
    **ALL app-side getter consumers are now gone.**
  - **6aq — walk getter-use consolidated to ONE fonted default; the three
    missed app consumers cleared. DONE (176 tests; settings audio AE=0;
    test-interface AE=5 cursor-level; data-editor loaded-tree AE=0; cloud
    fuzzel+json standalone AE=0; designer unchanged).** CORRECTION to 6ap's
    "all app-side consumers gone": three call sites had escaped the audit —
    settings' `renderer.rs` `collect_window_child` (outside render_widget),
    test-interface's gallery loop, and cce-cloud's `jl`/`fuzzel` labels (cloud
    drives WgpuAdapter directly, like pre-6ap designer, and json_layout was
    live only through it). All three now use walk-derived text. Engine side:
    the `renders_own_subtree` walk branch is just `paint_self` (TreeList +
    newly-flagged JsonLayout carry subtree-emitting overrides; descending
    JsonLayout would draw inactive pages and miss its checkbox side-labels);
    the default `paint_self` leaf drain moved from `text_labels()` to the
    FONTED getter — same labels every legacy tuple consumer served. Ramp got
    the own-labels `paint_self` (the Plate/LoginCard class: container own
    text vs the walk's aggregate rule). Traps recorded: a widget with a
    ui-tree parent must NOT also be walked as a top-level root (test-
    interface's page selector under the status bar double-drew ~10%
    brighter); cce-cloud launches reach the user's DAEMON via
    /run/user/UID/cce-cloud.socket — hold the socket aside to A/B a local
    standalone build.
  - **6ar — the per-widget text getters are DELETED from `Element`. DONE
    (176 tests; full workspace builds; nine apps A/B-verified — AE=0 or
    cursor/translucency/status-race noise only).** `text_labels` /
    `text_labels_with_bounds` / `text_labels_with_font_and_bounds` /
    `get_text_items` are gone from the trait, with Adapted's impls, the
    `Paint::text_items` hook, and every container aggregate (Backplate,
    Layer, Page, Plate, SectionContainer, ColorRamp, ControlPanel,
    JsonLayout, ButtonStrip). Every widget reaches the frame through
    `paint_self`. The deleted defaults survive as painter helpers with the
    labels passed in (`paint_legacy_leaf`, `fonted_leaf_labels`,
    `scroll_ancestor_text_bounds`, `base_control_label`); legacy leaves keep
    their label logic as inherent `own_labels()`; TreeList reads its concrete
    Adapted children via the now-pub(crate) `own_labels_with_font_and_bounds`;
    ControlPanel/ParametersBg/JsonLayout source dyn-children labels off the
    paint walk (ControlPanel re-applies its scroll shift + viewport clamp;
    List and ControlPanel are `renders_own_subtree` — walking into a legacy
    scroll frame desyncs text from geometry at scroll ≠ 0). The concrete
    `context_menu::text_labels()` global stays (inherent method, not the
    trait). ~125-method god-trait is now 4 methods lighter and text has ONE
    path: prims.
  - **6as (teardown, in progress).** Landed: (1) census of all 117 `Element`
    methods vs workspace-wide call sites — five were call-less and are
    DELETED (`Element::paint` — the hover-registration default nothing
    invoked — `as_geom_controller`/`as_spreadsheet_controller` & variants,
    `color_u8`, `is_layer`); (2) the seven tree context-menu actions
    (`copy_key`/`copy_value`/`delete_key`/`expand_node`/`collapse_node`/
    `expand_all_nodes`/`collapse_all_nodes`) are now transitional `Input`
    capability hooks with `Adapted` forwards (the 5k pattern), so the global
    context menu's `dyn Element` dispatch survives the TreeList conversion.
  - **TreeList → `Adapted<TreeList>` — DONE (the staged plan below executed
    verbatim; 176 tests; data-editor loaded-tree A/B AE=0 byte-identical incl.
    the focus wash; row select / context-menu Copy-Key-to-clipboard / search
    click-to-focus verified live).** Two new transitional hooks landed with
    it: `Input::tick_ctx` (EventCtx-carrying tick — the rename-commit focus
    re-target needs the routing ctx) and `Paint::paints_own_subtree` →
    `Element::renders_own_subtree` (the field widgets stay ctx-linked for
    event propagation, but the walk must not also descend — descending
    double-painted them and drew the CLOSED add-key popover box). The 5q
    `legacy_focus_highlight` trap struck again (the focused tree's teal wash).
    KNOWN-LATENT (pre-existing since 6ac, verified identical in the
    pre-conversion baseline): wheel-over-tree doesn't scroll and the Add-Key
    button doesn't open its popover — children-first propagation
    short-circuits on the hit child before the tree's own toggle/scroll
    logic runs. Fix belongs to the event-routing follow-up, not the widget.
    Original staged plan (executed):
    - `Layout`: `rect_assigned` caches the rect; the `set_rect` body
      (search box / add-key button / popover box / scroll box arrangement +
      `update_bounds`) moves to the assignment hook. No container children —
      the walk treats the adapter as a leaf, so `renders_own_subtree`
      becomes unnecessary.
    - `Paint`: `color`/`rounded_corners`/`corner_radius`/`solid_border`/
      `widget_font` port straight; geometry aggregates become ctx-less
      (TreeList's children are FIELDS — its `children(_ctx)` ignores the
      ctx already); subtree text rides the `serves_legacy_labels` hatch
      (`legacy_labels_with_font_and_bounds(rect, ctx)` = today's
      `subtree_fonted_labels`); `prepare_text`; popover via
      `Paint::popover`/`draw_popover`.
    - `Input`: the mouse/cursor/wheel/keyboard/tick bodies move into
      `on_event` arms with `ectx.ui` (the 5s ParametersBg pattern);
      drag via the Input drag hooks; `blocks_backplate_drag`; the seven
      tree hooks return their inherent bodies; focus semantics —
      `ctx.set_focused(self)` sites become `ectx.request_focus()` (the
      ADAPTER's pointer, not the inner). CAUTION: `mouse_input` registers
      the inline `edit_box` into the ctx TREE (`register_widget` +
      `link_ids(self_id, …)` + `set_parent`) — under the adapter, `self_id`
      must be the adapter's base id (`EventCtx::widget_addr` precedent).
    - Sweep: data-editor field → `Adapted<TreeList>` (Deref covers the
      concrete calls: `scroll_box.scroll_y`, `set_flat_keys`, `take_*`
      drains, `get_row_rect`, `select_and_show_key`, `focus_search`), one
      raw `*mut TreeList` cast → `as_ptr_mut()`. A/B: loaded tree, row
      click + inline rename (double-click), context menu Copy Key via
      wl-paste, search focus, add-key popover, wheel.
  - **Leaf sweep COMPLETE — every widget is on the narrow traits (6as).**
    Trackpad, KeybindRecorder, FontSelector, ColorSelector, Ramp, ColorRamp
    all converted (each A/B'd: gallery/DE/LI diffs = cursors, launch-phase
    animation, or AE=0). Notables: ParametersBg's typed color rows re-typed
    to `Adapted<ColorSelector>`; ColorSelector's in-file keyboard tests pass
    THROUGH the adapter; Ramp/ColorRamp take the TreeList shape
    (paints_own_subtree + tick_ctx + per-tick re-parenting of field widgets
    so their label fade blends against the adapter's color); and
    `Adapted::paint_self` gained a subtree TEXT PASS-THROUGH — a
    paints_own_subtree widget's Text prims forward verbatim with per-child
    fonts/bounds instead of being flattened to widget_font by the
    own-labels re-derivation (composites with mixed child fonts rendered in
    the default serif without it). `impl Element` now remains ONLY on: the
    containers (Layer/Container/Page/Plate/Backplate/ScrollBox/ScrollBar/
    List/ControlPanel/SectionContainer/JsonLayout/Menu/MenuBar-internals),
    app-local widgets, ContextMenu, and `Adapted` itself.
  - **Then: retire the `as_*_controller` pairs — SCOPE CORRECTED.** The
    earlier four-site estimate came from an over-filtered grep; the real
    surface is ~45 sites: cce-designer's HTTP-action/menu plumbing holds
    ~16 (including generic roster queries like "does ANY widget have an
    open menu" via `as_menu_controller()` over `Box<dyn Element>`), and
    `Switcher` implements MenuController by forwarding to its ACTIVE PANE
    through `as_menu_controller_mut()` on `dyn Element` — a live
    capability-dispatch system, not vestigial casts. Retirement needs a
    design decision first: either a standalone capability registry
    (`&dyn XController` handles registered beside the tree) or deferral to
    the Element deletion itself, where the designer's roster becomes
    concretely typed. Do NOT sweep it mechanically.
  - **Controller-capability decision RESOLVED (6aw): option 2 — defer to
    concrete typing; no registry.** A registry would be permanent
    infrastructure preserving the anonymous-widget pattern the rebuild
    exists to kill. Instead the queries die when their callers get
    retyped. First payoff immediately: `Switcher` — the largest holder
    (17 sites) — turned out to have ZERO constructors workspace-wide
    (settings dissolved its switcher in 6u) and is DELETED. Remaining
    map: cce-designer's roster retype (~16 sites, the bulk), the
    test-interface roster (3), one production site in
    `display/serialize.rs` (`serialize_widgets`' menu-state dump), and
    in-file tests that assert capabilities ride the adapter (die with
    `Element`). The designer retype is therefore the finale's next
    structural step, folded into the tree-machinery retype.
  - **Capability system DELETED (6aw, same session).** The designer's
    accessor block reaches each controller trait by `as_any` downcast to
    the roster index's known concrete type (note: `Adapted::as_any`
    exposes the INNER widget, so the downcast targets `MenuBar`/`Graph`/
    `ParametersBg`/`Spreadsheet`/`Breadcrumb` directly — the first build
    against `Adapted<W>` panicked at launch, caught by the live A/B);
    test-interface downcasts its Paginator; `serialize_widgets` tries
    the two MenuController implementors a roster can hold. With no
    callers left, Element's 12 `as_*` hooks, Adapted's forwards, and the
    14 `Input` capability hooks + their per-widget impls are all
    deleted. Element: 92 methods. A/B: designer/TI static diffs are the
    known noise shapes; designer HTTP `menu_click` (the dynamic-index
    path) verified live.
  - **Gallery containers went app-local (6as).** cce-test-interface's
    widget gallery was the last constructor of ControlPanel / Plate /
    SectionContainer / Backplate: it now owns `ti_widgets.rs` — a verbatim
    ControlPanel copy plus passive Plate/SectionContainer/Backplate
    lookalikes (each reproducing EVERY Element getter the render/event
    paths read: plate color/opacity/blur alpha-negation/corner radius/
    border/label_offset bg shift, the childless section-header row, the
    backplate bevel). A/B: launch-phase progress-bar animation only.
  - **CONTAINER TYPES DELETED (6as).** `Backplate`, `Plate`, `List`,
    `ControlPanel`, `SectionContainer` (+`SectionHeader`) removed from
    cce-ui outright — five files, the re-exports, the `List` branches in
    the scroll-ancestor text-bounds walks, and the then-dead
    `own_labels_with_font_and_bounds`. `ColumnWidth`/`ListColumn` moved to
    their only consumer (cce-files `row_list.rs`). TreeList's drag tests
    re-anchored on `drag_allowed_at`. 171 tests (5 died with their files);
    settings/DE A/Bs cursor-only / AE=0.
  - **Finale opened: dead-flag constant-fold sweep (6at).** With the
    plates gone, census round 2 found no zero-CALL methods but seven
    zero-OVERRIDE ones (only the trait default exists ⇒ they are
    constants). Folded and deleted: `capture_event` (the capture-phase
    branch in `propagate_event_impl` was unreachable), `is_active` (folded
    into the `highlight_color` default), `is_plate` (designer render.rs's
    whole `parent_plate_rect` text-clamp machinery was dead),
    `is_backplate` + `is_movable_backplate` (folded the drag walks —
    `UiContext::is_movable_backplate_at` had become constant-false and is
    DELETED; the `Application` default now just returns false — plus the
    backplate-parent theming/corner branches in MenuBar, StatusBar, and
    Dropdown's `backplate_ancestor` walk; MenuBar/StatusBar `corner_style`
    still reports the parent radius for children that read it through the
    parent pointer, but corners never round). `corner_radii`/`mark_dirty`
    also have zero overrides but carry real derived logic — they die with
    the retype, not by folding. Element: 112 → 107 methods. A/B: files
    AE=0, settings audio AE=0, designer diff = terminal behind the
    translucent window.
  - **Layer + Page DELETED (6au).** Per the 5r survey, no app ever put
    content in Paginator's pages — every consumer keys its own content
    on `selected_page()` — so the `Vec<Page>` was empty containers being
    arranged/registered/toggled/event-proxied for nothing. Their one
    visual (the page-area bg quad, page_color × page_opacity) moved into
    `Paint::paint`. With the stack gone Page had no constructor and
    Layer's only constructor was Page's base: both files deleted, plus
    `Element::is_page`, ScrollBar's Page-downcast write-back, and the
    dead 8/11 of `PageSelector` (now just selected_page /
    set_selected_page / sidebar_w; MenuBar's impls + `page_hidden` field
    went with it). A/B: email/LI/files AE=0; live LI tab click switches
    pages correctly. Census round 3 then found the deletion stranded two
    more zero-override methods — `check_out_of_bounds` and
    `transform_event_for_child` (Page was the only override of each) —
    folded and deleted in the follow-up. Element: 104 methods.
  - **ScrollBar DELETED, ScrollBox demoted off `Element` (6av).**
    ScrollBar's only consumer was cce-system-settings' page scrollbar
    (the dissolved Page subtree's survivor, evented through
    `propagate_event` and painted through `collect_window_child`) — the
    file moved there verbatim and the cce-ui type is gone. ScrollBox is
    never ctx-registered by either consumer (TreeList + the
    test-interface panel copy call it concretely), so its `Element`
    impl was dyn-dispatch ballast: now a plain struct whose former
    Element entry points survive as inherent methods with
    default-derived parity (the scrollbar-click focus claim became
    `focus::clear_focus()` — unfocusing the previous holder was its
    only observable effect). The painter/model scroll-ancestor text
    clamps folded to `None` (no tree parent can be a ScrollBox; none
    ever was at runtime). A/B: data-editor AE=0 plus live wheel +
    track-jump-scroll on a 100-key tree; settings diff = process-row
    churn; TI sub-threshold. Census round 4: only `corner_radii` +
    `mark_dirty` remain zero-override (real derived logic — they die
    with the retype, not by folding).
  - **Dead raw-widget sweep (6ax).** The raw-`Element` census after the
    capability deletion found four more zero-constructor widgets:
    Header, VBox, HBox (export-only) and Svg — which also rode Button
    as an `Option<Svg>` payload no caller ever set, so Button's icon
    branches were statically dead and went too. All deleted.
  - **Container DELETED (6ax part 2).** Settings' system-info
    `actions_row` was a dead field; dm's `root_container` was a
    transparent origin-anchored fan-out — dissolved into direct
    dispatch/walk roots (session list first for events, matching the
    reversed child order; card first for text, matching `children()`
    order). dm A/B: background-animation phase only (B-vs-B control
    differs full-frame), click behavior identical to baseline, 3/3
    interaction rounds alive on both builds. One UNREPRODUCED
    bogus-alloc seen once on the new build — dm's known latent
    stale-pointer signature, 0/3 repro on either binary; watch it.
  - **JsonLayoutWidget moved into cce-cloud (6ay).** The KDL/JSON
    launcher-layout host had one consumer; it cannot be demoted off
    `Element` (cloud feeds it to the paint walk as `&dyn Element`), so
    the file moved app-side verbatim — the impl dies with the machinery
    retype. `Justification` stayed in cce-ui (Button/files/settings
    share it), same path. A/B: content-identical renders (raw diff =
    the overlay's run-to-run spawn position + wallpaper bleed through
    the translucent plate); live checkbox click toggles.
  - **Canvas + Viewport3D moved into cce-designer (6ay part 2).** Both
    designer-only (preview.rs's "canvas" is its own internal type).
    A/B: AE=0. cce-ui's raw `impl Element` surface is now exactly ONE
    production type: ButtonStrip, the ctx-registered embed of
    MenuBar/Paginator — load-bearing in the tree, pinned to the
    machinery retype.
  - **App-local impls onto the narrow traits (6az) — COMPLETE:
    `Element` has exactly ONE production implementor (`Adapted<W>`).**
    Each remaining raw `impl Element` converted to
    `Layout`/`Paint`/`Input` + `Adapted<W>` ahead of the machinery
    retype; only test mocks still implement the trait directly.
    Done: cce-colors' ColorSlider (constructor returns the wrapper, so
    construction and direct-dispatch sites are untouched; A/B AE=0,
    click/wheel live-verified); settings' ScrollBar (now plain data —
    its raw-pointer parent/children fields, Drop, and unsafe Send/Sync
    had zero consumers; strip A/B AE=0 and cross-build byte-identical
    after an identical wheel + track-click sequence); cloud's Fuzzel
    (set_rect side effects → rect_assigned; overlay A/B identical, row
    click moves the selection live) and JsonLayoutWidget (the TreeList
    shape — paints_own_subtree + whole-subtree routing in on_event with
    gates_presses off + tick_ctx; the old overrides survive verbatim as
    inherent methods paint composes with a dummy ctx; render sites'
    all_quads/all_rounded_quads calls now resolve to the adapter's
    reverse bridges, same tuples; checkbox toggle live-verified) —
    cce-cloud is raw-impl-free. Designer's four (PassivePlate, Canvas,
    NodePalette, Viewport3D) followed — constructors return the wrapper
    so the roster's Box pushes and as_any downcasts stand; PassivePlate's
    full getter surface rides Paint, Viewport3D's wheel inertia moves to
    on_event/tick; static A/B = the status sliver only, cross-build
    captures after an identical HTTP circular-toggle + viewport wheel
    structurally identical — cce-designer is raw-impl-free. TI's four
    followed (part 6): ControlPanel keeps its aggregate overrides as
    inherent methods composed by a paints_own_subtree paint, its child
    arrangement in Layout::arrange_children (adapter as parent), and its
    routing in on_event; the raw `as *mut ControlPanel` casts became
    as_any downcasts; inline_label keeps the adapter's label machinery
    out of all four. Gallery + child-window A/Bs pixel-parity, cross-
    build sequences identical — cce-test-interface is raw-impl-free.
    The greeter's three followed (part 7: labels fold into paint off the
    laid-out rect; the display list's card-skip moves from base-pointer
    equality to id comparison; cursor-only diffs, 3/3 click rounds alive
    on both builds, after-state byte-identical). ButtonStrip closed the
    sweep (part 8): embedders hold `Adapted<ButtonStrip>`; two wrapper-
    shadowing collisions (`set_selected`, `take_click` — Element's bool
    signatures vs the model's `Option<usize>` ones) route through
    `inner_mut()`; presses stay ungated so a tab press lands under an
    open dropdown popover; LI's Paginator strip and email's MenuBar
    strip both cross-build byte-identical after identical clicks.
  - **Compiler census of the dyn surface (6ba, experiment reverted —
    the finding is the deliverable).** Hypothesis: with one implementor,
    many `Element` methods are only ever called on concrete
    `Adapted<W>` receivers and could move off the trait before the
    retype. Method: delete a method from the trait, keep it inherent on
    the wrapper (same signature — concrete sites resolve unchanged),
    `cargo check --workspace`; every error is a true dynamic-dispatch
    site. Verdict: **~91 of 92 methods fail — the trait IS the
    machinery surface; there is nothing to slim first.** Three consumer
    classes pin it: (1) cce-ui machinery (context.rs routing/focus/
    drag, the paint walk, layout.rs render paths — `T: Element`
    generics count: measure/preferred_height/set_row_rect live there —
    and core.rs context-menu actions on `dyn` targets); (2) container
    child aggregation over raw child pointers (menu/paginator/treelist
    in-tree, ControlPanel/JsonLayout app-side) touching the full
    paint+input getter surface; (3) the designer/test-interface roster
    broadcast loops (`Vec<Box<dyn Element>>`) calling nearly everything.
    NOTE: a `--workspace` check that fails in cce-ui never reaches the
    app crates — the first pass under-reported; the app rosters were
    where 4 of 5 "clean" candidates actually failed.
  - **The designer roster is concretely typed (6bb).** First retype
    slice, in census order: `Vec<Box<dyn Element>>` → one
    `Box<WidgetSlots>` of 17 named concrete fields (boxed whole so
    registered pointers stay stable while `State` moves). Const-indexed
    sites reach fields directly; the genuinely index-driven paths
    (draw order, focus cycling, broadcast dispatch, the `*_IDX`-keyed
    HTTP API) go through `get_dyn`/`get_dyn_mut`; serialize takes a
    per-slot `dyn_refs` view (`serialize_widgets` now takes
    `&[&dyn Element]`). Static A/B byte-identical; cross-build final
    after an identical menu/toggle/wheel/click sequence within 6 px.
    TI's two-mode gallery followed (part 2): 53 named concrete slots
    for the gallery, typed enums for the child window's runtime-variant
    slots, a `Roster` enum carrying whichever mode runs; the numeric
    indexes keep addressing slots through `get_dyn`. **No app stores
    widgets behind `Box<dyn Element>` anywhere — the roster phase is
    complete.** Cross-build A/Bs: gallery empty masks static and after
    an identical interaction sequence; Ramp child at noise level;
    non-Ramp child arms hit the pre-existing unconditional
    Ramp-downcast panic identically on both builds.
    Cloud's `JsonControl` closed the owned-storage class (part 3): the
    JSON-config controls become a typed enum over `Adapted<T>` replacing
    `JsonWidget`'s `Box<dyn Element>`; the label walk dropped its
    `as_ptr` round-trip unsafe for a plain reborrow. **No owned
    type-erased widget storage remains anywhere in the workspace.**
  - **The borrowed-pointer retype design (6bc, decided 2026-07-12).**
    The replacement handle is **`WidgetId`, resolved through the
    generational `WidgetTree` at every use**. Rationale: apps own
    widgets concretely (the 6bb rosters) and re-register pointers
    idempotently per frame from boxed storage, so the registry is the
    one place a raw pointer is refreshed before use; every *other*
    stored `*mut dyn Element` bypasses that guard and is exactly where
    the real UAFs happened (display-manager 6ao — dangling registry
    from a by-value `new()`; settings 6w — `FOCUSED_WIDGET` surviving a
    rebuild via same-size alloc reuse). Under the retype, raw pointers
    remain only (a) as the `WidgetTree` registry payload and (b) as
    transient same-frame values inside resolution helpers; every stored
    reference becomes a `WidgetId`, and a stale id resolves to `None`
    and is skipped — the UAF class becomes unrepresentable outside the
    registry itself. Public signatures taking `&dyn Element` stay
    (deriving the id from `base()` internally) so most call sites
    survive verbatim; direct field readers convert compiler-driven.
    Slices, each independently shippable and A/B-verifiable:
    1. **Focus (DONE 2026-07-12)** — both stores
       (`UiContext.focused_widget` AND the `core.rs` thread-local
       `FOCUSED_WIDGET`) → `Option<WidgetId>`, kept as two stores with
       their existing reader sets (merging them changes observable
       focus behavior — not this phase's job). Thread-local fns that
       must dispatch `unfocus`/`FocusOut` gained a ctx param (every
       dispatching call site had one in reach). `set_focused` /
       `set_focused_ptr` refresh the registry with the pointer they
       are handed, so focus on a not-yet-registered widget keeps
       working; `is_focused_addr` → `is_focused_id` (base-id
       comparison). Verified: 168 tests; identical click sequences on
       text-editor (AE≤6, empty 8% masks) and data-editor (focus
       click AE=0); settings spinbox click-to-focus live.
    2. **Popovers + context-menu target (DONE 2026-07-12)** —
       `active_popovers: Vec<WidgetId>`, `ContextMenuState.target:
       Option<WidgetId>`; every occlusion/render walk resolves through
       the tree; `register_popover` takes `&mut` and self-registers;
       `show_context_menu` derives + registers the target id;
       `context_menu::mouse_input` takes the resolving ctx;
       `is_coordinate_covered` id-keyed with `WidgetId(0)` as the
       no-base sentinel; `EventCtx::widget_addr` deleted (use
       `ectx.id`). Verified: A/B vs slice-1 captures byte-equivalent
       (incl. the TE File-menu popover); DE leaf context menu Copy
       Key → wl-paste; settings page-dropdown popover switches pages.
    3. **Container child storage (DONE 2026-07-12 — by deletion, not
       retype).** The stored-field census found the slated vecs were
       ballast: `ParametersBg.children` was never populated anywhere
       (deleted with its parent-tracking twin, the container hooks,
       eleven dead iteration blocks, `collect_child_quads`, and
       window_runner's quad-attribution loop); `ScrollBox.children`
       was write-only (deleted with TI's one push). What still stores
       raw pointers after this slice: the `WidgetTree` registry (by
       design) and TI's app-side `ControlPanel.children` — live,
       pointing into boxed slots, consumed in ctx-less paint/arrange
       paths, so it retypes when those paths gain ctx (the endgame).
       Paginator's `container_children` is a transient field ref, not
       storage. The `Element::children`/`parent`/`add_child`/
       `set_parent` *signatures* still traffic in pointers, but every
       value is transient and tree-resolved at call time — they die
       with the `Element` endgame rather than warranting a standalone
       signature sweep.
    4. **`propagate_event(event, root: WidgetId)`** + the app dispatch
       loops off `as_ptr_mut` (the big app sweep). NOTE from the
       slice-3 census: propagate roots are live borrows at call time —
       this slice is API honesty, not a UAF fix; weigh folding it into
       the endgame instead of touching ~200 app sites twice.
    5. window_runner render plumbing + remaining `as_ptr` sites; then
       the `Element` + `Adapted` endgame (own design pass).
    Stored-pointer state remaining after slices 1–3, all deliberate:
    the `WidgetTree` registry; TI's `ControlPanel.children`; and the
    tick-refreshed parent copies in MenuBar/StatusBar/Dropdown models
    (`parent_changed`/`tracked_parent` — written each tick by the
    re-parenting pattern with the live host pointer, read in ctx-less
    popover-direction/corner-radius math). Endgame option for the
    parent copies: snapshot the *data* read through them (parent rect,
    radius, is-Ramp flag) at re-parent time instead of the pointer —
    same refresh cadence, no deref of potentially-dead memory; watch
    the one-frame rect lag on resize if reads move to snapshots.
  - **The `Element` endgame design (6bd, decided 2026-07-12).** The
    endgame is a **trait replacement, not an app rewrite**. Grounding
    facts (consumer survey): direct per-method dispatch
    (`mouse_input`/`cursor_moved`/`keyboard_input`/wheel on concrete
    `Adapted` fields) exists in nearly every app — making routed
    events a prerequisite would gate the endgame on ~12 app
    migrations, so the direct-dispatch surface *stays on the new
    trait* and shrinks later as apps move to routed events at their
    own pace. Designer's index-driven roster broadcast needs only a
    small dyn set (unfocus, drag hooks, set_modifiers, z_index,
    hit_test, visibility, tick, prepare_text, focused, as_any). The
    legacy tuple getters' consumers are the `T: Element` generics in
    layout.rs (render_widget/Column/Section — settings, LI, files,
    colors render paths) and window_runner's tessellators
    (widget_vertices — designer/TI): generic, not dyn, so they can
    re-bound onto a narrower bound without touching the dyn surface.
    Target shape: a **`WidgetHost` trait of ~40 methods** implemented
    once by `Adapted<W>` (blanket over the narrow traits). `Adapted`
    does NOT die — it survives as the single host wrapper owning base
    state; what dies is the 92-method god-trait and its Option-base
    escape hatches (`base()` becomes a guaranteed `id()`/state
    access, killing the `WidgetId(0)` sentinel class). Phases, each
    shippable:
    1. **Capability actions → one enum method (DONE 2026-07-12)**:
       the 13 context-menu action methods (`cut_selection`…
       `copy_path`) became `context_action(ContextAction) -> bool`
       across Element, the Input hooks, and the Adapted forwards; the
       Input default keeps whole-value Cut/Copy/Paste through the
       value-string pair; TreeList's seven action bodies moved to
       inherent methods via an impl split (zero code movement);
       core.rs maps the menu strings (incl. the load-bearing "Cear")
       to enum values. Element is down to 79 methods. Live-verified:
       tree Copy Key/Copy Value/Collapse and TextBox Paste through
       the enum dispatch.
    2. **Tuple getters — DISSOLVED into the flip (measured
       2026-07-12).** The premise was wrong on two counts. (a) The
       dyn consumers are real: designer's render loop reads
       `extra_quads`/`extra_circles`/`extra_arcs`/`color` directly
       off its `&dyn` handle AND feeds it to the tessellators, and
       the paint walk's legacy-leaf branch reads
       `all_quads`/`all_rounded_quads`/`extra_*`/`widget_font` via
       dyn — the visual surface must ride the host trait object.
       (b) A separate `LegacyVisual` trait can't be reached from the
       existing trait object (no cross-trait-object casts; a
       supertrait split trips over `rect`/`color` defaults needing
       `base()`). The genuinely generic consumers (layout.rs
       render_widget/Column/Section, settings'
       `collect_window_child`, dm/cloud's local tessellator copies
       with concrete receivers) simply re-bind at the flip.
    3. **Tree-link methods — landed pre-flip after all (2026-07-12).**
       `children`/`parent` are core machinery walk methods (propagate,
       painter, navigate) — host-trait material, they stay.
       `clear_children` left in batch 2; **`set_parent`/`add_child`
       left in batch 4**: both are inherent `Adapted<W>` methods now
       (files' concrete sites resolve unchanged); the dyn callers
       were only three — `focus::link_parent_child`'s body (rewritten
       as the register + `tree.link` + `tree.set_parent` ops the pair
       always was), TI's page-selector/StatusBar roster pair (now one
       `link_parent_child` call), and TI ControlPanel's per-arrange
       dummy-ctx child re-parent (deleted — every effect was
       discarded with the dummy ctx, the same inert-ritual class as
       Ramp's tick re-parents). A/B: TI gallery (flat-walk `parent()`
       skip intact — no page-selector double-draw) + dm greeter, both
       empty 8% masks. Note Paginator both serves
       `container_children` AND `link_ids`-registers its strip — the
       ctx-less walks (popover_rect/prepare_text/render_popover) are
       why the field-derived form must stay.
    4. **The flip**: define `WidgetHost`, blanket-impl for
       `Adapted<W>`, retype the registry/context/painter/
       window_runner from `dyn Element` to `dyn WidgetHost`, delete
       `Element`. ~~The MenuBar/StatusBar/Dropdown parent-pointer
       snapshot change rides this phase.~~ **Landed early
       (2026-07-12): the census showed all five stored widget-side
       parent pointers production-DEAD** (nothing ever set_parent's
       a MenuBar/StatusBar; ramp's per-tick re-parents fed a
       write-only field through a dummy ctx — legacy behaved the
       same). TextBox.parent deleted outright; Dropdown.parent
       became `parent_snapshot` read-DATA (rect/is_ramp/color, same
       direct-write activation, Ramp-clamp test adapted) and its
       write-only `tracked_parent` died; MenuBar/StatusBar lost the
       fields, their `parent_changed`/`tracked_parent` overrides,
       MenuBar's never-firing `adjust_rect` clamp, and now report
       the 0.0 corner radius production always read; the
       `Layout::parent_changed`/`tracked_parent` hooks are deleted
       (implementor-less), `Adapted::parent` is tree-only, and the
       Ramp/ColorRamp tick_ctx re-parent rituals are gone. Stored
       `*mut dyn Element` survives ONLY in the WidgetTree registry
       payload, EventCtx's transient host ptr, and TI's
       ControlPanel. A/B: text-editor + TI Ramp-child static AND
       preset-popover-open frames all empty 8% masks; 163 tests. **Measured blueprint (~55
       methods, from the machinery's actual call sites):**
       identity/tree — id (guaranteed, no more `Option<&Widget>`),
       type_name, label, as_any/as_any_mut, as_ptr/as_ptr_mut
       (transitional), visible/set_visible, z_index,
       is_child_visible, children(ctx), parent(ctx), add_child
       (transitional, TI);
       layout — rect, set_rect, measure, layout, label_x_offset,
       set_row_rect;
       events — handle_event, hit_test, mark_dirty, tick/wants_tick,
       set_modifiers, focus/unfocus/focused, context_action,
       blocks_backplate_drag, is_scrollable, plus the
       direct-dispatch block (mouse_input, cursor_moved, mouse_wheel,
       keyboard_input, drag_begin/drag_update/drag_end/is_dragging/
       draggable, take_click, take_change) — shrinks per app as they
       move to routed events;
       value — get_value_string/set_value_string, set_text,
       set_selected — shrinks as app loops go concrete-slot;
       paint — paint_self, clips_children, renders_own_subtree,
       prepare_text, popover_rect, render_popover, dirty-flag access;
       visual tuples (walk legacy branch + designer loop) — color,
       all_quads, all_rounded_quads, extra_quads, extra_circles,
       extra_arcs, corner_radii, plate_bevel, solid_border,
       widget_font, highlight_quad.
       Everything else on today's Element (79 methods plus the
       generic-only surface) either moves to inherent `Adapted<W>`
       methods for the generic render machinery or dies.
       **Execution mode: shrink Element IN PLACE toward the blueprint,
       then rename it to `WidgetHost` when it matches** — a parallel
       trait can't be reached from the existing trait object, but
       removing non-blueprint methods one census-driven commit at a
       time keeps every state shippable. **First shrink batch (DONE
       2026-07-12, 79→73):** `highlight_color` folded into the
       `highlight_quad` default (zero overrides); `set_drag_bounds` +
       `intrinsic_size` moved to inherent `Adapted<W>` methods (their
       concrete callers — designer's network panel, fonts'/graph's
       hand-laid sizing — resolve unchanged); `layout_style`/
       `layout_children` deleted with **scene/bridge.rs itself** (its
       last production user was retired in 6aa; the narrow
       `Layout::intrinsic_size` hook stays — `Adapted::measure` reads
       it); `layout_ignore` deleted with its only consumers, the
       uncalled `layout_widgets`/`layout_widget_ptors`. Census
       lesson: grep BOTH `.method(` and UFCS `::method(` forms — the
       fonts/graph `Element::intrinsic_size(&x)` callers only
       surfaced at compile. **Second shrink batch (DONE 2026-07-12,
       73→69):** `hovered`/`set_hovered` deleted — the state is the
       base `Widget::hovered` flag, read/written directly by the
       `cursor_moved`/`on_cursor_moved`/`highlight_quad` defaults and
       `serialize.rs`; Button/Checkbox keep inherent accessors for
       immediate-mode hosts (cloud's json_layout downcasts to
       concrete `Checkbox`, so it already resolved to those).
       `corner_radius` + `rounded_corners` replaced by ONE
       `corner_style() -> (f32, (bool,bool,bool,bool))` mirroring the
       narrow `Paint::corner_style` — NOT folded into `corner_radii`,
       which is lossy: the radius is meaningful with every corner off
       (Menu/StatusBar report their parent's radius to children
       through the flags-off channel; breadcrumb can be
       flags-true/radius-0, whose legacy radius-0 rounded bg quad
       would vanish). `clear_children` moved to an inherent
       `Adapted<W>` method (every caller is a concrete Adapted field
       in cce-files). Consumer commits: settings renderer,
       TI ControlPanel aggregates, cloud json_layout. Verified:
       settings audio render stream byte-identical, files A/B AE=0,
       TI gallery empty 8% amplitude mask, 163 tests.
       Census facts for the leftovers: `preferred_height` has DYN
       consumers (container_layout.rs child-ptr walks + layout.rs
       machinery on `&dyn` children) — blueprint-adjacent, rides the
       flip, not inherent-movable; `value` has a live dyn consumer
       (`serialize.rs` over designer's `dyn_refs()`) — rides the
       flip. Remaining non-blueprint candidates:
       `on_cursor_moved` (belongs in the direct-dispatch block —
       blueprint addition, not a deletion), `set_parent` (flip
       material, with the parent-ptr snapshot change), `value`,
       `preferred_height`.
    Former slices 4/5 fold in: the app `as_ptr_mut` dispatch sites
    are rewritten by whichever of routed-events (per app) or the
    phase-4 flip reaches them first; no standalone pointer-to-id
    signature sweep.

Order rationale: each phase is independently valuable and reversible, and no phase requires the
next to compile. Phase 0 can land immediately regardless of the rest.

---

## 7. Quick wins to land first (Phase 0 detail)

1. **Breadcrumb black rectangle — DONE.** `cce-files` `BrowseContainer::set_rect` now positions
   the breadcrumb to exactly match `browse::view`'s layout (inset by the page margin, reserving
   the dropdown width) so the container's duplicate paint sits fully behind the page copy
   instead of leaking a dark strip. This is a stop-gap; the real fix is the single paint path in
   Phase 3 (the breadcrumb is still painted twice — the copies now just coincide).
2. **Hot-path debug I/O — DONE.** Removed the per-frame `eprintln!` in `render()` that
   reconstructed every text area's string via `layout_runs()`.
3. **Dead code — DONE.** Removed `UiContext::tick_hover`/`get_hover_quad` (zero callers
   workspace-wide), the dead duplicate of `hover_animation`.
4. **GPU scissor — DEFERRED to Phase 3.** Threading clip rects to `render_pass.set_scissor_rect`
   is not actually a "quick win": clip rects are computed CPU-side and folded into geometry
   today, with nothing carried to the render pass. Doing it properly needs the clip stack from
   §3.4, so it lands with the paint-pass rework rather than as a risky standalone change.

---

## 8. Risks & mitigations

- **Migration surface across ~19 apps.** Mitigation: adapter shim + per-app Phase 6; the core
  lands and is validated before any app is forced across.
- **Borrow-checker friction with an arena tree.** Mitigation: layout operates on
  `style`/`layout_out`, not the `widget` payload; `get_disjoint_mut` for the rare dual-borrow.
- **Hand-rolled layout correctness.** Grow/shrink/wrap/alignment are subtle. Mitigation: keep
  the box model small (row/column + flex + align + gap/padding only), and unit-test the solver
  in isolation — it operates on `Style`/`Size`, independent of paint, so it is directly testable.
- **Effort.** This is multi-week. Phasing keeps every intermediate state shippable so it can be
  paused/resumed without a broken tree.

---

## 9. Deferred (designed-for, not built now)

- **Per-subtree geometry caching:** cache tessellated vertices per node, re-tessellate only
  dirty subtrees instead of the whole scene each frame. The arena `Dirty` flags are the hook;
  worth it only once scenes are large.
- **Full affine transforms / rotation** beyond translate+scale.
- **Damage-rect partial redraw** at the GPU level (currently full-surface clear each dirty frame).

---

## 10. Decisions (resolved 2026-07-07)

1. **Layout solver: hand-rolled** (not taffy). Compact measure/arrange engine owned in
   `cce-ui`, scoped to the DE's box model. See §3.2.
2. **A new shared dependency in `cce-ui` is acceptable** when needed (one shared path dep does
   not break standalone builds). Note the layout decision means no layout dep is required.
3. **Phase 0 quick wins land now**, as separate commits ahead of the rebuild. See §7.
