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
- **Rendering**: raw Vulkan via **ash** (`src/vk/`, `VkRenderer`) for all geometry, and
  **cosmic-text** + swash for text (shaped into a self-managed glyph atlas by `src/vk/text.rs`).
  The wgpu path is retired; cosmic-text used to be reached through **glyphon**, which is gone
  too — every `glyphon::` item used here was a cosmic-text re-export, and dropping it takes
  wgpu out of the build. There is no HTML/DOM — the UI is GPU primitives (quads, rounded rects with
  per-corner radii, vectors with caps, arcs, circles, and the **relief primitives** — the
  lit-surface family: bevels, plates, recesses, bosses, ridges, fillets, grooves; see the
  `Prim` enum doc in `src/scene/paint.rs`). Tessellators live in
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
- **Undo/redo**: the runner owns the routing. A press matching the `undo` / `redo` chord
  (`input.kdl`, cce-ui domain defaults `ctrl+z` / `ctrl+shift+z`) goes to the focused widget
  as `ContextAction::Undo` / `Redo` (a TextBox that is editing steps its own typing), then to
  the app's `undo(needs_rebuild)` / `redo(needs_rebuild)` hooks (default false); only if both
  decline does the key reach `handle_key_input`. Apps keep their own document history on
  `cce_ui::history::History<T>` — snapshots of the app's state type, with gesture/group
  coalescing and the fork-on-new-edit rule built in (module doc in `src/history.rs`).

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

## Plates, wells and seams — the surface vocabulary

Everything cce draws is a lit surface, and the words below name those surfaces
so that a description of how a screen should look or behave can be given in
them. Use them in code comments, commit messages, and conversation; when a new
widget does not fit one of them, say so rather than stretching a word.

- **A plate is any lit, bounded surface with a silhouette and a stance.** The
  silhouette is its corner radius (the DE's superellipse corner family,
  `corner_shape`). The stance is how it sits on the surface beneath it:
  - **raised** — it floats above that surface, drawn as a `Bevel` (fill plus
    rolled edge) or a `Boss` (edges only, the surface below as its face):
    menus, popovers, raised buttons, a ButtonStrip's selected plateau, a
    Breadcrumb in its floating stance.
  - **flush** — it sits level with that surface inside a groove ring, drawn as
    an `inset_plate`: buttons, dropdown triggers, breadcrumb runs, font
    selectors. Its face is the surface below unless a fill is configured.
- **Plates nest, and the ladder has three rungs of the same object.** The
  **root plate** is a window's background (RFC 7a; `plate { root }` in
  config). **Pane plates** are the surfaces controls and content sit on inside
  a window; they carry the corner dock (`widget/plate_dock.rs`). **Control
  plates** are the things you press. A control plate is not a different kind
  of object from a root plate — it is a plate at a smaller scale.
- **Wells are not plates.** A well is an opening cut into a plate that you look
  into or type into, drawn as a `Recess` (a `Trough` when it holds a moving
  part): text boxes, keybind and spinbox fields, slider and progress tracks,
  the trackpad pane, the ColorSelector's recess. Things you press are plates;
  things you enter are wells. A well's floor can carry fills (a progress
  fill, a colour swatch) — those are segments of the floor, not plates.
  A **canvas well** is a well you look into or draw in — Trackpad, Slider2D,
  the bevel and ramp previews — and every one is cut from the same material:
  the plate darkened for its floor (`colors::WELL_FLOOR`) and the recess for
  its rim, through `PaintCtx::well_floor` / `well_rim` (`canvas_well`),
  rounded like the text wells. The Ramp editor's plot is the reference look.
  With relief off, a well is its frame: the one hairline
  `colors::well_frame_color` gives every well (lit in the highlight while it
  is active, the relief rim's focus cue), and still no floor of its own.
- **A group is a lasso.** `widget::Group` owns nothing: it is a set of member
  ids, and its frame is the padded hull of wherever the host's layout put
  them, with a title tab flush on the top edge — the section's frame
  (`PaintCtx::section_well` under relief, the section outline otherwise), so
  a group is a segment of the plate it sits on, parted by a section carve
  rather than a seam. Given its plate (`with_plate`) and `with_fit`, sides
  within `snap` of the plate's edge take the edge one padding in and corners
  on the plate's corner follow it concentrically: on a narrow pane a group is
  that pane's inset lining, on a wide one a lasso. A group never hits.
- **Segments are plates or floors sharing one silhouette, parted by seams.**
  A seam is a `Groove` cut across the shared surface, dying into its rolled
  edge: Breadcrumb segments, ButtonStrip segments, the ColorSelector's
  text/swatch split. One silhouette, one relief pass, seams between. A
  `Separator` is the same cut made in the plate it sits on, with no segment
  to part: a groove that dies out at its own ends (flat: a hairline).
- **Marks and bands sit outside this vocabulary on purpose.** The round
  Checkbox mark is a mark; the Slider's swelling band is a band. Do not call
  them plates or wells.

What this buys, and where the code is heading:

- **Navigation is stated in plate terms.** `Input::focus_role` says what a
  widget is to the keyboard: a `Plate` (a thing you press — Enter / Space act
  on it while focused), a `Well` (opens for typing when focused), or `None`
  (not a stop). `UiContext::focus_step` walks the stops in reading order (row,
  then x) with a `Group`'s members as one contiguous run where the group's
  first member falls (`focus_clusters`), wrapping; `focus_step_group` jumps
  between runs (input.kdl `focus_next_group` / `focus_prev_group`, defaults
  `ctrl+tab` / `ctrl+shift+tab`). The runner calls them for Tab / Shift+Tab
  and the chords when the app opts
  in with `Application::plate_navigation` (default off, so an app that routes
  Tab itself — a terminal, a web view, its own field order — is undisturbed)
  and tells the app through `Application::focus_stepped` — an app that caches
  its geometry until its own rebuild flag raises it there. The walk needs the
  app's context exposed (`ui_context_mut`); the ring reaches flat-path hosts
  through `RenderTarget::inset_plate_tinted` and `CarveKind::Boss { tint }`.
  `CCE_FOCUS_DEBUG=1` prints the stops in walk order.
  The focus ring is the plate's own silhouette: `ControlPlate::with_tint`
  lights the rim (a tinted `Trough`, `Boss` or `Bevel`), the same treatment a
  well's `recess_tinted` gives its rim while editing — never extra geometry.
  A Checkbox lights the ring its mark already draws; a Toggle lights its
  label (its rocker halves carve partial rings, which cannot be tinted).
  Roles today: Button, Checkbox, Toggle, Dropdown, FontSelector, ButtonStrip
  (arrows move the selection between its segment plates) and Breadcrumb
  (arrows walk its visible segments, Enter navigates) are plates; TextBox,
  Spinbox, ColorSelector, KeybindRecorder, TreeList, Slider (a band, but
  entered and adjusted in place — arrows step it, Enter opens the readout)
  and RangeSlider (one stop, two ends: arrows step the focused end, Up / Down
  switch ends) are wells. A new focusable widget declares its role and handles `FocusIn`
  / `FocusOut`.
- **One plate spec per rung, not five copies.** The root and pane rungs are
  `scene::paint::PlateSpec` (RFC 7b, painted by `PaintCtx::plate`). The
  control rung is `scene::paint::ControlPlate` (re-exported from `widget`):
  footprint, per-corner silhouette, `PlateStance` (raised or flush), face and
  depth, painted by `PaintCtx::control_plate` — the ONE place a control face's
  relief is composed (raised with a face = bevel; raised faceless = carve
  inside + boss; flush = carve inside + inset plate). Button, Dropdown,
  FontSelector, Breadcrumb and the ButtonStrip's selected plateau draw through
  it; the migration was prim-identical against a dump of every face. A new
  control face goes through `ControlPlate`, never a hand-rolled carve.
- **Radii are configured per rung, overridden per widget.** Root:
  `style.surface.plate.root.corner_radius` (`color::root_plate_corner_radius`).
  Pane: `plate_corner_radius`, falling back to the root's. Control:
  `style.control.corner_radius` (`layout::control_corner_radius`, default 8) —
  every control-scale getter (button, dropdown, font selector, slider,
  spinbox, textbox, toggle, list and tree wells; the ColorSelector's two via
  the textbox) falls back to it when the widget's own `corner_radius` key is
  unset, so a per-widget key is an override, not a requirement. Do not give a
  new control-scale radius getter a literal default; fall back to the rung.

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
  the `read_preferred_fonts` / font-family resolution used by the cosmic-text path.
- `color.rs` — color model and named colors (`colors` re-export module in `lib.rs`).
- `config.rs` — KDL loading and `kdl_to_json` conversion (see workspace `CLAUDE.md` for paths).
- `context.rs` — `UiContext`: the retained widget tree, event routing, spatial grid, dirty
  tracking, hit-testing.
- `history.rs` — `History<T>`: the undo/redo snapshot stack (cap, gestures, grouped runs).
  The toolkit defines the stack and the routing, never the step — see the trait section.
- `widget/` — `container/` (vbox/hbox/scroll/menu/treelist/…), `input/` (button/slider/text_box/
  dropdown/…), `display/` (label/graph/svg/…), plus `editor.rs`, `json_layout.rs` (KDL/JSON-driven
  layouts), `core.rs`.
- `protocol.rs` — inline-generated Wayland protocol bindings.
- `ipc.rs` — the `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock` helpers (`socket_path`, `send_command`).
- `process.rs` — detached/tracked child spawning, plus the `cce-cloud` popup pattern:
  `CloudPopup` (one blocking `run_json`/`run_dmenu` invocation) and `CloudPopupTracker`
  (an app's single-active-popup toggle state; see cce-status-interface for the
  canonical usage).
- `icon.rs` — XDG icon-theme lookup: a `.desktop` `Icon=` key (or an SNI tray icon
  name) → a file on disk, plus `upload_themed` to rasterize/decode and upload it.
  **Not** `lib.rs`'s `upload_icon`, which loads a *bundled* cce-icons glyph by its
  own name for in-widget use; this one resolves names any installed app may ship.
- `file_dialog.rs` (rfd), `scale.rs` (HiDPI), `wayland.rs` (surface/scale detection, and
  `detect_metric` — the display's logical px per mm from its `wl_output` geometry).
- `units.rs` — lengths with units and the display metric; see the Units section below.

## Units — logical px inside, real lengths at the edges

The toolkit's working unit is and stays the **logical pixel**: every layout
node, style slot and widget measure is an `f32` of logical px. `units.rs`
adds the bridge to real lengths, in two parts:

- **`Len`** — a value with a unit (`px`, `mm`, `cm`, `in`, `pt`), parsed from
  `"2mm"` and resolved to logical px through a `Metric`. In config a length
  carries its unit as a KDL type annotation, the same way `(rgba)` and
  `(relief)` do: `width=(mm)2.0`. A bare number is a logical px, forever —
  nothing migrates. `config::kdl_to_json` turns an annotated number into the
  string `"2mm"`; the writer turns it back into `(mm)2`; `reload_config`
  stores it in the style registry's `lens` map, and `get_float` resolves it
  against the live metric at every read. So `layout::bevel_width()` and every
  other getter are unit-aware without knowing it, and a metric that arrives
  after config load (outputs come in after the first style read) or changes
  with the display is honoured without a reload. `get_len` returns the
  configured unit for editors that should show what the user typed.
- **`Metric`** — logical px per mm for the display this process is on, plus
  its **source**: `measured` (EDID via `wl_output` geometry, or the
  compositor's configured `size_mm` in its place — the client cannot tell
  them apart; `ccectl outputs` can), `forced` (`CCE_FORCE_PPI`), or
  `assumed` — the CSS 96 px/in convention when nothing is known (a headless
  shadow, a projector with no EDID). The source is carried so fabrication
  can refuse a guess: `Metric::is_real()`. The window runner installs it
  beside `scale::set_scale_factor` (`units::set_metric`); apps read
  `units::metric()`, `units::mm(v)`, or `Len::to_px()`.

**Relief has a real depth axis now.** `style.surface.relief.width` (the wall's
run) and the new **`height`** (a carve's drop) and **`edge_height`** (the plate
roll's rise) are all lengths — `height=(mm)0.3` is honest geometry, resolved
through the metric. Unset, a carve drops `relief_shade::RECESS_DEPTH` (0.6) of
its wall (saturating at the DE roll width) and the roll is a quarter-round of
radius width — the look every config had. `style.surface.relief.depth` is NOT a
length: it is the light strength (`bevel_depth` → `Material.strength`), and
**`light`** is its honest alias. `layout::carve_depth_px` states the drop rule
once for the tessellator's CSG features and, through `WindowInfo.relief_meta`,
the shader's free carves; `carve_depth_ratio` / `roll_height_ratio` feed the
shading twin (`Material.carve_depth` / `roll_height`). A `(relief)` value
carries the drop as `h=` (a length: `h=0.5mm`, or bare px) beside `w=` and
`d=` (light; `l=` reads as an alias). `cce-relief`'s Height knob is the editor:
its section's depth numbers read in mm when the metric is real, and Save
writes `height` as a `(mm)` length then, px otherwise.

Why not millimetres inside: UI sizes are perceptual and angular, not physical
— a hit target should not become 8 mm on a projector three metres away.
Documents and fabrication content live in real units and convert at view
time. Two domains, one bridge.

On the live laptop panel (3840×2400 over 344×215 mm at scale 2) the metric is
5.58 logical px/mm (141.8 ppi); the default 9.3 px relief roll is 1.67 mm, and
the 96 ppi assumption would have called it 2.46 mm.

## Fonts & assets

`lib.rs` builds the cosmic-text `FontSystem` (re-exported as `cce_ui::cosmic_text` so clients
need no text dependency of their own). Bundled fonts load from `$CCE_FONTS_DIR` (else
`~/Dropbox/Fonts`); bundled icons from `$CCE_ICONS_DIR` (else `~/Dropbox/cce/cce-icons/svg`).
System fonts are loaded only when `$CCE_LOAD_SYSTEM_FONTS` is set (or via
`create_font_system_with_system_fonts()`, used by the font picker). Configured custom font
families are validated at startup with a warning if missing.

## Debug environment variables

All opt-in, all read once, all quiet when unset — set one and run any client.

- `CCE_PLATE_DEBUG=1` — per frame, how many relief carves grouped into their host plate
  as exact CSG features vs fell back to standalone overlay shading, and for each fallback
  **why** (one of six rules: ridge, edge-suppressed, tinted, feature budget, no enclosing
  plate, host's feature run closed). The two paths shade junctions differently, and three
  of those rules are dynamic, so this is the answer to "why does this widget's carve look
  different here?". Note what it reveals: grouping is *rare* — the reference demo groups
  2 of 10, cce-files 0 of 7, because any ordinary geometry painted after a plate closes
  its grouping window (correctly — the carve's shading is baked into the plate's earlier
  draw).
- `CCE_PRESENT_DEBUG=1` — swapchain present/acquire tracing.
- `CCE_VK_DEVICE=<substring>` — force a physical device; `CCE_VK_RT=0` disables ray tracing.
- `CCE_FORCE_SCALE=<f>` — override HiDPI scale detection.
- `CCE_FORCE_PPI=<f>` — pin the display metric (logical px per inch) regardless of what
  the outputs report; a headless shadow has no EDID and would run `assumed`. The live
  panel is 141.8.
- `CCE_UI_FAULT_RECONNECT=1` — exercise the Wayland reconnect path.
