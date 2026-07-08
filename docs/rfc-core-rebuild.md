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
  - **Still to do:** migrate remaining widgets
    off `impl Element` onto the narrow traits (per-widget, Phase 6 flavour); replace the 8 live
    `as_*_controller` downcast pairs (called by `cce-designer`, `cce-test-interface`, and ~10
    cce-ui widgets) with a typed message/command channel; delete `Element` + `Adapted` once the
    last widget is across.
- **Phase 6 — Per-app migration.** Move each `cce-*` app onto the new core; delete legacy paths
  once the last app is across.

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
