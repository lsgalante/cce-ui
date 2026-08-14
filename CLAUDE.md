# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> This is the `cce-ui` crate. It lives inside the larger **`cce` Cargo workspace** — read the
> workspace guide `../cce-compositor/WORKSPACE.md` first for the multi-repo layout, the
> standalone-build rule (no `[workspace.dependencies]`), the KDL config system, and the
> Unix-socket IPC convention. This file covers only what is specific to `cce-ui`.

## What this crate is

`cce-ui` is the **shared, custom retained-mode GUI toolkit** every `cce-*` client depends on
(`cce-ui = { path = "../cce-ui" }`), and the compositor's only intra-workspace dependency. It is
not a wrapper around an existing framework — it owns its transport, rendering, layout, and widget
set outright.

- **Transport**: raw `wayland-client` 0.31 + `smithay-client-toolkit` 0.19, driven by a `calloop`
  event loop. Clients are real Wayland surfaces (xdg toplevels, xdg popups, and `wlr-layer-shell`
  layer surfaces), not toolkit-owned windows.
- **Rendering**: **wgpu 24** for all geometry through a single `src/shader.wgsl` pipeline, and
  **glyphon** for text. There is no HTML/DOM — the UI is GPU primitives (quads, rounded rects with
  per-corner radii, vectors with caps, arcs, circles, bevels). Tessellators live in
  `backend/window_runner.rs` and are re-exported through `src/engine.rs`.
- It is **both a library and a binary.** `src/lib.rs` is the toolkit; `src/main.rs` is
  `DemoApp`, the reference `Application` — a small widget gallery on the Phase 6 target
  architecture (display-list frame, scene-solver layout, routed events, in-frame
  popovers). Copy it when starting a new client.

## Build, test, run

Use cargo directly (the `Makefile` just wraps `cargo build --release` + install of the demo
binary). Prefer `-p cce-ui` from anywhere in the workspace so you don't rebuild the compositor.

```sh
cargo build -p cce-ui                          # build the toolkit (+ demo binary)
cargo test  -p cce-ui                           # run the test suite (headless unit tests)
cargo test  -p cce-ui scene::arena              # tests in one module
cargo test  -p cce-ui --lib color::             # tests in one lib module path
cargo run   -p cce-ui                            # run the demo/reference app (needs a Wayland session)
```

Tests are headless unit tests colocated in `#[cfg(test)]` modules — concentrated in `src/scene/*`
(the arena/layout/paint/anim engine) and `src/color.rs`, `src/config.rs`, `src/layout.rs`, plus a
scattering of widgets (`text_box`, `slider`, `dropdown`, `treelist`, …). When touching the scene
engine, that module's tests are the fast feedback loop; run `cargo test -p cce-ui scene::` before
anything else.

Wayland protocol bindings are generated **inline at compile time** by `wayland-scanner` macros in
`src/protocol.rs` from `protocol/*.xml` (`cce-inspector-v1`, `cce-window-management-v1`) — there is
no `build.rs` and no codegen step to run.

## The `Application` trait — the client contract

Every client implements `Application` (`src/backend/window_runner.rs`, re-exported from
`engine.rs`). A client's `main.rs` is typically a struct implementing it plus a one-line
`cce_ui::engine::run::<MyApp>();`. When adding a widget or client, **mirror an existing client**
(e.g. `cce-status-interface`) — do not invent a new structure.

Key methods (see the trait def around `window_runner.rs:1450`):
- `new`, `settings()` (→ `WindowSettings`), `layer()` (→ optional `LayerSettings` for
  layer-shell surfaces like the status bar), `update(msg, needs_rebuild, exit)`, `tick(dt, …)`.
- **Draw**: `view` / `view_rounded_quads` / `view_vectors` / `overlay_quads` push legacy
  primitive tuples; `text_items()` returns text; `custom_vertices()` appends raw vertices (e.g.
  graph geometry). `display_list()` is the new opt-in path (see below).
- **Input**: `handle_pointer_move`, `handle_mouse_input`, `handle_mouse_wheel`,
  `handle_key_input` — most return an optional `Message`. `needs_rebuild: &mut bool` is how a
  handler requests a redraw; the loop is demand-driven and idles when nothing sets it.
- `ui_context()` / `ui_context_mut()` expose the widget tree (`UiContext`) for apps built on the
  retained widget system rather than immediate drawing.

The frame loop is demand-driven (single `redraw` dirty bool, gated by a Wayland frame-callback
vsync) — it idles correctly when nothing changes. Don't add per-frame I/O to the render hot path.

## Rendering: one paint path (the Phase 3 state)

The backend `render()` **always builds a `scene::paint::DisplayList` and tessellates that single
list** (`window_runner.rs` ~1799). Two ways an app feeds it:

1. **Migrated**: return `Some(DisplayList)` from `Application::display_list()`.
2. **Legacy (default)**: return `None`, and the backend wraps the app's `view*`/`view_vectors`
   tuples into a `DisplayList` via a `PaintCtx` — byte-for-byte the old geometry, just routed
   through the one path.

So every app, migrated or not, renders through the same tessellate step. `custom_vertices` is
appended as a final unclipped batch drawn on top.

## The `scene/` core rebuild (read `docs/rfc-core-rebuild.md` before touching it)

`src/scene/` is a **retained scene graph being grown additively** to replace three overlaid legacy
subsystems (tripled tree ownership via raw widget pointers; three uncoordinated render paths;
layout smeared across five mechanisms). The RFC (`docs/rfc-core-rebuild.md`) is the authoritative
design and phase tracker — its inline "DONE" notes are the source of truth for what has landed.
Modules:

- `arena.rs` — `Arena` / `NodeId` / `Node`: a generational forest, the single source of truth for
  tree ownership. Generational keys turn use-after-free into a `None` lookup, not UB.
- `tree.rs` — `WidgetTree`: arena-backed replacement for `UiContext`'s old `widget_registry` +
  `layout_tree` twin stores, keyed `WidgetId → NodeId` so the public `WidgetId` API is preserved.
- `layout.rs` — the hand-rolled measure→arrange solver (`Style`/`Size`/`Rect`/`LayoutBox`).
  Deliberately **not** taffy: a compact row/column + flex + align + gap/padding box model.
- `paint.rs` / `painter.rs` — `DisplayList` + `PaintCtx` (clip/transform stack) and the single
  paint walk. Each widget emits its own geometry via `WidgetHost::paint_self`; the walk owns
  recursion and clipping (`WidgetHost::clips_children`), instead of every container re-deriving
  intersections. `renders_own_subtree` is an escape hatch for legacy subtree painters.
- `anim.rs` — `Animated<T>` (tween + spring + easing), the Phase 4 animation primitive replacing
  ad-hoc bool flips.

### `WidgetHost` (formerly the `Element` god-trait)

`WidgetHost` (`src/widget/mod.rs`) is the single ~52-method host surface the machinery
(context routing, paint walk, render loop, app dyn broadcasts) sees, produced by the RFC's 6bd
shrink-then-rename of the old ~125-method `Element` god-trait. Its ONE production implementor
is `Adapted<W>`; concrete widget behavior lives on the narrow `Layout`/`Paint`/`Input` traits
(`src/widget/model.rs`). `base()` is guaranteed (`&Widget`, no Option). The direct-dispatch
block (mouse/key/drag) and the value/polling block (`take_click`/`take_change`/value strings)
are GONE from the trait — events route through `handle_event`, and apps drain widget state
through the concrete inherent `Adapted<W>` methods. See the RFC's blueprint notes before
adding anything to this trait.

**Runtime verification matters here.** Several scene changes are "compiles + tests pass; runtime
verification pending" per the RFC — the headless tests can't catch paint/event regressions. When
changing scene wiring, `cargo run` a real client (cce-files, cce-designer, cce-graph,
cce-system-interface) to confirm behavior, not just the test suite.

## Module map (where things live)

- `layout.rs` (largest file, ~5.7k lines) — fonts + sizing; many `*_font_parsed()` getters and
  the `read_preferred_fonts` / font-family resolution used by the glyphon text path.
- `color.rs` — color model and named colors (`colors` re-export module in `lib.rs`).
- `config.rs` — KDL loading and `kdl_to_json` conversion (see workspace `CLAUDE.md` for paths).
- `context.rs` — `UiContext`: the retained widget tree, event routing, spatial grid, dirty
  tracking, hit-testing.
- `widget/` — `container/` (vbox/hbox/scroll/menu/treelist/…), `input/` (button/slider/text_box/
  dropdown/…), `display/` (label/graph/svg/…), plus `editor.rs`, `json_layout.rs` (KDL/JSON-driven
  layouts), `core.rs`.
- `protocol.rs` — inline-generated Wayland protocol bindings.
- `ipc.rs` — the `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock` helpers (`socket_path`, `send_command`).
- `process.rs` — detached/tracked child spawning, plus the `cce-cloud` popup pattern:
  `CloudPopup` (one blocking `run_json`/`run_dmenu` invocation) and `CloudPopupTracker`
  (an app's single-active-popup toggle state; see cce-status-interface for the
  canonical usage).
- `file_dialog.rs` (rfd), `scale.rs` (HiDPI), `wayland.rs` (surface/scale detection).

## Fonts & assets

`lib.rs` builds the glyphon `FontSystem`. Bundled fonts load from `$CCE_FONTS_DIR` (else
`~/Dropbox/Fonts`); bundled icons from `$CCE_ICONS_DIR` (else `~/Dropbox/cce/cce-icons/svg`).
System fonts are loaded only when `$CCE_LOAD_SYSTEM_FONTS` is set (or via
`create_font_system_with_system_fonts()`, used by the font picker). Configured custom font
families are validated at startup with a warning if missing.
