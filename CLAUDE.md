# CLAUDE.md — cce-ui

`cce-ui` is the custom retained-mode GUI toolkit every `cce-*` client depends on. Read the
workspace guide `../cce-compositor/WORKSPACE.md` first: the multi-repo layout, the standalone-build
rule (no `[workspace.dependencies]`), `ccebuild`, the KDL config system, IPC, and the
concurrent-sessions rules for shared crates like this one.

This file is the **reference**: what the toolkit is, its contracts, and the rules that keep it
coherent. The depth lives in topic docs, read when you work in that area:

| Doc | Covers |
|---|---|
| `docs/runtime.md` | the run loop and shells: pacing, damage, reconnects and `renderer_init`, lost surfaces, layer surfaces, input methods and text input, touch, scrolling semantics |
| `docs/surfaces.md` | the surface model in full: plates, wells, fields, carves, materials and frost, the `style.surface` config and its retired keys, relief geometry, units and the height field |
| `docs/widgets.md` | per-widget behaviour: the context menu and page turns, params pane, slider, float3 trackball, dropdown, checkbox/toggle, spreadsheet, scrollbars, graph, Markdown and `DocEditor`, glyphs |
| `docs/platforms.md` | the browser shell and WebGPU renderer, the macOS shell, compute jobs, 3D scenes (`Stage3D`, lit meshes, instancing), the path tracer, and the probes that hold renderers to each other |
| `CHANGELOG.md` | what changed and when, with the reasoning and measurements. **History goes there, not here.** |
| `docs/rfc-*.md` | design records; see "RFCs" below for which are live |

**Keeping this file a reference.** When you change behaviour, update the reference text to
describe the NEW state in the present tense, and add a dated entry to `CHANGELOG.md` saying what
changed, why, and how it was verified. Do not write "since 2026-…" or "until X it was Y" here or
in `docs/*.md` — that is changelog material. A rule learned the hard way belongs here as a rule
(one line, with the reason); the story belongs in the changelog. `tests/doc_claims.rs` checks
that the tests, source paths and `(~N lines)` figures cited here and in the topic docs exist.

## What this crate is

- **Transport**: raw `wayland-client` 0.31 + `smithay-client-toolkit` 0.19 on a `calloop` loop.
  Clients are real Wayland surfaces: xdg toplevels, xdg popups, `wlr-layer-shell` surfaces.
- **Rendering**: raw Vulkan via ash (`src/vk/`, `VkRenderer`) for all geometry; cosmic-text +
  swash for text, shaped into the toolkit's own glyph atlas (`draw::glyphs`). No wgpu, no
  glyphon, no DOM. The UI is GPU primitives — quads, rounded rects, vectors, arcs, circles, and
  the lit **relief primitives** (bevels, plates, recesses, bosses, ridges, fillets, grooves,
  fields, frames…; see the `Prim` enum in `src/scene/paint/mod.rs`).
- **Three shells over one `Driver`/`Pacer`**: Wayland (the real one), the browser (WebGPU on a
  `<canvas>`), and macOS (AppKit + MoltenVK, type-checked only — it has never run). See
  `docs/platforms.md`.
- **Library and binary.** `src/lib.rs` is the toolkit; `src/main.rs` is `DemoApp`, the reference
  `Application` (display-list frame, scene-solver layout, routed events, in-frame popovers,
  widgets on handles). **Copy it when starting a new client.**
- **The GUI-free half is the sibling crate `cce-core`**: `config`, `input`, `motion`, `units`,
  `relief_spec`, `ipc`, `locale`, `l10n` and the spec parsers (hex colours, ramps, droplets).
  cce-ui re-exports each at its old path (`cce_ui::config::…`). The compositor and
  cce-window-manager depend on `cce-core` alone. A change to those modules is a change to
  `cce-core`: push it, then run the workspace's `bump-revs.sh`.
- Apps depend on cce-ui as a git pin on GitHub; the workspace root's `[patch]` redirects it to
  this tree.

## Build, test, run

```sh
cargo build -p cce-ui                       # the toolkit + demo binary
cargo test  -p cce-ui                        # headless unit tests (~650; under a second)
cargo test  -p cce-ui scene::                # one module's tests
cargo clippy -p cce-ui --all-features --all-targets -- -D warnings   # CI's clippy gate
cargo run   -p cce-ui                        # the demo (needs a Wayland session)
scripts/check-wasm                           # type-check the browser build (needs wasm32 target)
scripts/check-mac                            # type-check for aarch64-apple-darwin
scripts/style-audit [--strict]               # sibling apps against "The standard app"
```

Always scope to `-p cce-ui`; never bare `ccebuild install`/`restart` (other sessions' work).
The `Makefile` wraps `cargo build --release` and `ccebuild install --no-build cce-ui`.

**Tests.** Headless `#[cfg(test)]` modules beside the code, densest in `src/scene/*`,
`src/color/`, `src/layout/` and the larger widgets; integration tests in `tests/`
(`plate_golden.rs` is the pixel-neutrality gate for plate changes; `doc_claims.rs` checks the docs).
- **Tests never read the machine's config.** Under `cfg(test)` the config home is a per-process
  directory nobody creates (cce-ui's dev-dependency turns on cce-core's `test-isolation`), so
  every getter answers its default. A test needing a setting loads it from a string
  (`color::reload_colors`) or pins it on its thread. Fonts are the exception: `fonts_dir()` still
  reads `$CCE_FONTS_DIR` (else `~/Dropbox/Fonts`) and the shaping tests need real faces.
- **Pin what a result depends on, per thread.** `motion::force_for_test`,
  `input::force_natural_scroll`, `scroll_motion::force_scroll_settings` pin the animations
  switch, natural scrolling and kinetic settings for the calling thread. Never set a
  process-wide env var in a test (the suite is parallel). A dependent crate's test binary links
  cce-ui WITHOUT `cfg(test)`, so it reads the machine unless it pins.
- **A colour test that reloads a knob reloads its default back.** `reload_colors` writes
  process-wide globals and an absent frost knob keeps its last value; `reload_colors("")` does
  not reset it. Assert-tests pin every knob they read through the setter; `test_color_state_lock`
  orders reloaders but cannot undo what one left behind.
- **GPU tests run on whatever Vulkan exists** and skip with a printed note where none does
  (`vk::compute`, `vk::plate_probe`); the path tracer's are `#[ignore]`d
  (`CCE_VK_RT=compute cargo test --lib vk::rt -- --ignored`).

**CI** (`.github/workflows/ci.yml`, every push; warnings are errors):
`test` (Ubuntu 24.04 with lavapipe so GPU tests run — the job fails if any printed a skip note —
and `fonts-liberation` as `CCE_FONTS_DIR`), `clippy`, `miri` (`widget::handle`,
`widget::embedded`, `scene::tree` under Stacked and Tree Borrows), `wasm` (`scripts/check-wasm`),
`macos` (builds and links every target on a macOS runner, runs the tests). To match `test`
locally: `apt install libwayland-dev libxkbcommon-dev mesa-vulkan-drivers fonts-liberation`, then
`CCE_FONTS_DIR=/usr/share/fonts/truetype/liberation RUSTFLAGS="-D warnings" cargo test --all-features`.
Five clippy lints are allowed as house style in `Cargo.toml` `[lints.clippy]`, each with its
reason; anything else clippy reports is fixed, not allowed.

**Build steps.** `build.rs` compiles the renderer's fixed WGSL shaders to SPIR-V (a WGSL error is
a build failure; `precompiled_spirv_matches_runtime_compile` is the test).
Wayland protocol bindings (`protocol/*.xml`) are generated inline by `wayland-scanner` macros in
`src/protocol.rs`.

**Features**: `markdown` (`widget::markdown`, brings in cce-vault), `doc_editor`
(`widget::doc_editor`), `a11y` (AT-SPI publishing; see "Accessibility" below).

## Verify on screen, not just in tests

The headless tests cannot see paint or event regressions. A change to rendering, layout, input
routing or a widget's look is verified by running a real client, and the house method is a
**pixel A/B in a shadow session**: the binary before the change and after, driven through the
same steps, screenshots diffed (`cce-shadow`; see WORKSPACE.md). Notes that cost sessions time:

- **Run the pre-change binary through the same steps first.** A test that never reproduced the
  bug proves nothing; several "fixed" builds still drew nothing.
- A shadow session needs `CCE_ICONS_DIR` (its HOME is isolated, so every glyph is blank
  otherwise), and the NVIDIA ICD loads only with `DISPLAY=:0 XAUTHORITY=$HOME/.Xauthority`.
- Never use sway's `hide_cursor` to clear the cursor for a diff (it clears pointer focus and the
  app drops its hover); mask the cursor's box instead.
- **Build both sides of an A/B the same way.** A binary built in the same `cargo build` as an
  example or test (`--example`, `--examples`, `--all-targets`) gets cce-core's `test-isolation`
  through cce-ui's dev-dependency, so it reads NO config and draws the default style. Fine for
  an A/B whose both sides were built so; misleading against one that was not, and wrong for a
  binary you mean to run as the user's. (`ccebuild` builds `--release --workspace`, no examples.)
- When driving a shadow with `ctl pointer-*`, wait for a closed window to be gone and for the
  new one's geometry to hold still before clicking: a fresh window can be placed and then moved
  to its restored spot, and clicks against the first position land on nothing. Check a contact
  sheet of the shots before trusting a "no difference".
- In-crate harnesses: `vk::plate_probe` renders a `DisplayList` offscreen through the live 2D
  pipeline for pixel assertions; `tests/plate_golden.rs` dumps and compares plates;
  `examples/probe*` and `scripts/web-probe/*` hold the Vulkan and WebGPU renderers to each
  other (`docs/platforms.md`).

## The `Application` trait — the client contract

Every client implements `Application` (`src/backend/app.rs`, re-exported from `engine`). Its
`main` is `cce_ui::engine::run::<MyApp>();`. Mirror an existing client; do not invent a structure.

- **Construction**: `create(sender: AppSender<Self::Message>)` is required. `AppSender` is
  cce-ui's handle (`send`, `Clone`, `Send`, `From` both ways with calloop's `Sender`), so the
  constructor names no window system. `register_sources` is a Wayland-only calloop hook.
- **Window**: `settings()` → `WindowSettings`; `layer()` → `LayerSettings` for layer-shell
  surfaces; `desired_size`, `adjust_size`, `clear_color`.
- **State**: `update(msg, needs_rebuild, exit)`, `tick(dt, needs_rebuild)`.
- **Draw**: `display_list(size, scale)` returns the frame — the ONE paint path. `display_list_text`
  opts its `Prim::Text` into the glyph pass. Side channels: `overlay_quads`, `custom_vertices`
  (a final unclipped batch), `take_damage` (only cce-grid reports its own).
- **Input**: `handle_pointer_move`, `handle_mouse_input`, `handle_mouse_wheel`, `handle_pinch`,
  `handle_key_input`. `needs_rebuild: &mut bool` requests a redraw.
- **Widgets**: `ui_context()` / `ui_context_mut()` expose the app's `UiContext`; exposing it is
  what gives the app routed events, Tab navigation and the accessibility tree.
- **Undo/redo**: the runner routes the `undo`/`redo` chords (input.kdl; `ctrl+z` /
  `ctrl+shift+z`) to the focused widget first, then the app's `undo` / `redo` hooks, and only
  then to `handle_key_input`. Apps keep document history on `history::History<T>`.
- **3D**: `init_3d(stage)` / `stage_3d(stage, size, scale)` through `&mut dyn Stage3D` — portable.
  The native `renderer_init` / `stage_renderer` take the `VkRenderer` and forward to them by
  default.
- **Behaviour hooks** (all defaulted): `standard_csd`, `csd_resize_borders`, `csd_titlebar_move`,
  `plate_navigation` (Tab walk, default on), `outlives_compositor`, `wants_surface`,
  `idle_poll_interval`, `load_system_fonts`, `is_movable_root_plate_at`, `take_window_action`,
  `focus_stepped`, `handle_focus_change`, `on_exit`.

Rules of the contract:

- **`tick` is not a clock.** The loop is demand-driven: between ticks the runner sleeps while
  nothing is pending (up to `IDLE_DISPATCH`, 1 s; `CCE_UI_IDLE_MS` overrides) and wakes on
  Wayland events and `AppSender` messages. Deliver background results through the sender, never
  by draining an `mpsc` in `tick`. Something the loop cannot see is polled by a widget returning
  `true` from `tick` while live, or by `Application::idle_poll_interval`. Animations keep the
  cadence by reporting change.
- **Don't put per-frame I/O on the render path.**
- **The toolkit names no app.** Nothing in cce-ui branches on the app id. When an app needs the
  runner to behave differently, add a defaulted `Application` hook. (Two widgets LAUNCH DE apps
  by name — the colour selector `cce-color-editor`, the font selector `cce-fonts` — which is the
  DE using itself, not a branch.)
- **GPU handles do not survive a reconnect.** A broken connection opens a new session around the
  same app with a NEW `VkRenderer`; `renderer_init` runs once per renderer. An image id cached
  across frames silently stops drawing after a reconnect (`Frame2D` skips unknown ids without
  logging). Act on the second and later `renderer_init`; prefer `Button::with_icon_name` (holds
  the name) over `with_icon(id)`. The three traps and the verification recipe
  (`CCE_UI_FAULT_RECONNECT`) are in `docs/runtime.md`.
- **`VkRenderer::try_new` is the only constructor**; a lost surface ends the session cleanly,
  never a panic.

## The widget model

**Traits.** `WidgetHost` (`src/widget/mod.rs`, 31 methods) is what the machinery — routing, paint
walk, render loop — sees. Its one production implementor is `Adapted<W>`. A widget's behaviour
lives on the narrow traits in `src/widget/model/`: `Layout`, `Paint`, `Input`. The host hands
those out (`layout_model()`, `paint_model()`, `input_model[_mut]()`), and `WidgetHostExt`
(blanket-implemented, `dyn` included) carries the derived reads — `focus_role`, `label`,
`corner_radii`, `painted_prims`, `preferred_height`, `z_index`, … — so import
`cce_ui::widget::WidgetHostExt`. A method stays on `WidgetHost` only when the host adds something
the model cannot (visibility gating, the content rect, child recursion, registry state). Events
route through `handle_event`; apps drain widget state through `Adapted<W>`'s inherent methods
(`take_click`, `take_change`, …).

**The registry owns its widgets** (`UiContext`, `src/context.rs`; tree in `scene::tree`):

- `ctx.insert(w)` moves a widget in and returns `Handle<W>` (`Copy`, typed). Reach it through the
  context: `ctx[h]`, `ctx.get(h)` / `get_mut(h)`, or `ctx.lend_h(h, |w, ctx| ..)` when you need
  the widget and the context together. The borrow checker therefore refuses an app access that
  overlaps a context call. `ctx.remove(h)` returns it by value (its children stay as roots).
- Every call the context makes into a widget goes through `lend`, which takes the widget out of
  reach for the call: a widget reaching itself through the context mid-event gets `None`, never a
  second `&mut`.
- Host-side API: `render_widget_h`, `Form::widget_h` / `widget_w_h`, `register_popover_id`,
  `focus_id` / `unfocus_id` / `set_focused_id` / `claim_focus`, `link_ids`,
  `paint_root_into(ctx, &ctx[h], pc)`, `get_widget(id)` / `get_widget_mut(id)` / `widgets()`,
  `tree.parent_id` / `child_ids`.
- **A composite's children are `widget::Embedded`**: held by value until the composite is
  inserted, then inserted under their own ids by `Layout::register_embedded_children` (which
  also places them: it runs on insert, every layout and every tick); `remove` takes them back.
- A widget made per frame (a status dot in a row) is inserted, placed and removed. A value built
  off-thread holds `Handle::none()`. Two widgets with one id cannot both be inserted (a clone
  copies its id; debug builds assert). An inserted widget that `wants_tick` is ticked.
- A widget used only as a paint STAMP stays a bare `Adapted` and is never inserted.
- Drive focus through the context (`focus_id` / `unfocus_id`), never `w.focus()` directly, so
  the window's focus record follows.
- `widget::handle`, `widget::embedded` and `scene::tree` hold the tests; CI runs them under Miri.

**A new widget declares what it is**: its `Input::focus_role` — `Plate` (you press it: Enter /
Space act), `Well` (you type or adjust in it), or `None` — and a label that names it to a person.
It handles `FocusIn` / `FocusOut`, paints through `Paint::paint` (never a hand-rolled carve: a
control face goes through `ControlPlate`), and draws symbols as glyphs (below).

**Keyboard navigation** is the runner's: Tab / Shift+Tab walk the focus stops in reading order
(`UiContext::focus_step`), a `Group`'s members as one run; `ctrl+tab` jumps between runs
(`focus_step_group`). An app opts out with `plate_navigation() == false` (the designer, display
manager and cce-notes do). A focused widget that types Tab keeps it (`Input::keeps_tab`). Register
a widget only while it is shown — a hidden registered widget is a stop nobody can see.
`CCE_FOCUS_DEBUG=1` prints the stops. A `Dialog` (`UiContext::open_modal`) traps the walk and
covers everything behind it.

**Context-menu actions are keyed by ID, never by label text.** Use
`context_menu::set_row_actions` or `UiContext::show_context_menu_rows`. Matching the English
label (`context_menu::legacy_action_for_label`) is a fallback for menus that set none; do not add
code that matches displayed text.

## Rendering

- **One paint path.** `backend::frame::build_frame` builds the frame from the app's
  `DisplayList` (via `PaintCtx`: clip/transform stack, the paint walk in `scene/painter.rs`;
  each widget emits its own prims, the walk owns recursion and clipping), tessellates it
  (`backend/tessellate/`), and the shell presents it. `None` from `display_list` is an empty
  frame. `build_frame` has no window system in it and is tested with no GPU.
- **Damage is derived.** The Wayland shell diffs each frame's batches, text and images against
  the last and repaints only what changed; frosted plates grow the region by their blur reach.
  `CCE_UI_FULL_DAMAGE=1` turns it off. Details in `docs/runtime.md`.
- **What a renderer draws is renderer-free**: `src/draw/` (`Frame2D`, `Batch2D`,
  `batch_push_constants`, the image-id queue `draw::images`, the glyph atlas, the shaders
  `shader2d.wgsl` / `glyph.wgsl` / the 3D ones) is shared by Vulkan and WebGPU; `vk` re-exports
  it at its old paths. Portable code reaches it through `crate::draw`, never `crate::vk`.
- **Never key behaviour off vertex values.** Say it in the draw (e.g. `SceneDraw::screen_space`).
- **The context menu draws in its own `xdg_popup`** on toplevels (in-window on layer surfaces,
  or with `CCE_UI_MENU_POPUP=0`); the popup's configure is written back so hit tests match the
  screen. `docs/widgets.md` has the menu in full.
- **Every symbol is a cce-icons glyph.** Draw a chevron, check, mark, +/− only as a glyph from
  `cce-icons/svg` through `PaintCtx::icon(name, rect, color)` — never a Unicode character in a
  fallback font, never built from primitives. A missing glyph draws nothing silently;
  `every_glyph_the_toolkit_names_is_in_the_icon_set` is the test. A container that collects
  its children's text must collect their glyphs too (`Adapted::own_glyphs`). Indicators
  (`StatusDot`, the plate dock's dot) stay shapes. Context-menu row marks are label prefixes
  (`MARK_CHECK`, `MARK_ON`, `MARK_OFF`) drawn as glyphs.

## Surfaces — the vocabulary

Everything is a lit surface. Use these words in code, comments and commits; when something
fits none of them, say so rather than stretching a word. Full model: `docs/surfaces.md`.

- **Plate**: a lit, bounded surface with a silhouette (corner radius, the DE's superellipse
  family) and a **stance**: *raised* (`Bevel` / `Boss`: menus, popovers, raised buttons),
  *flush* (level with the surface inside a groove: buttons, dropdown triggers — `inset_plate`),
  or *flat* (`PlateStance::Flat`: a quad of the pane's material, can be frosted; for bars that
  should read as panes, not controls). Plates nest in three rungs of one object: the **root
  plate** (the window), **pane plates**, **control plates**.
- **Well**: an opening cut into a plate that you look or type into (`Recess`; a `Trough` when it
  holds a moving part): text boxes, slider tracks, canvas wells. Things you press are plates;
  things you enter are wells.
- **Field** (`scene::paint::Field`, painted by `PaintCtx::field`): a well with a flush plate, the
  **run**, standing in it, one outline round both. Text box (`Field::well`), flush control
  (`Field::run`), text-with-picker and spinbox (`ending_in_run`), toggle (`sliding_run`),
  checkbox (a well, with a square run when checked). Ask for a field by its form, never by
  numbers that encode one.
- **Segments and seams**: plates or floors sharing one silhouette parted by `Groove`s
  (Breadcrumb, ButtonStrip). A `Separator` is that cut alone.
- **Group**: a lasso of member ids with a framed title (`widget::Group`); owns nothing, never hits.
- **Dialog**: a raised plate around a lasso, modal through the context (`widget::Dialog`; the
  demo's Options… is the pattern).
- **Bands** (the slider's swelling band) are deliberately outside the vocabulary.

**Composition rules.** A control face goes through `scene::paint::ControlPlate` /
`PaintCtx::control_plate` — the one place a control's relief is composed. The root and pane
rungs are `PlateSpec` (`PaintCtx::plate`). What a plate is made of is a `scene::Material` (tint,
`Frost`, `Finish`); `Material::fill_tint` is the one place the blur-behind sentinel is written.
The focus ring is the plate's own rim, tinted (`ControlPlate::with_tint`) — never extra geometry.

## The standard app — root plate and the spacing ladder

Every app is built the same way; `scripts/style-audit` checks the siblings (`--strict` fails on
any off-standard row), and `src/main.rs` is the reference.

- **The first prim of every frame is `pc.root_plate(w, h)`** (`PlateSpec::window`). Nothing else
  paints a window base. A window that is deliberately not a plate (a transparent bar, a lock
  screen, a full-bleed 3D view) declares `// style-audit: opt-out <reason>`.
- On the root plate stand **pane plates** and **carves**; on panes stand **control plates**.
- **A deliberate deviation** goes through `PlateSpec::window(..).with_material(..)` /
  `.with_depth(..)` with a comment saying so, never a hand-built spec.
- **Spacing is a ladder; an app never names a number.** Each rung is a config key read through
  the style registry, with a getter in `layout`:

| rung | inset | gap between siblings |
|---|---|---|
| root plate | `root_plate_inset()` = relief width + `style.surface.plate.root.padding` | `root_plate_gap()` |
| pane plate | `plate_padding()` | `plate_gap()` (unset = the root gap) |
| controls | — | `control_gap()` (`style.control.gap`, unset = `CONTROL_GAP`) |
| inside a list row | `list_gap()` from the list's wall | `list_gap()` (`style.control.list_gap`) |

  In the box model the rungs are presets: `Style::root_column()` / `root_row()`,
  `pane_column()` / `pane_row()`, `controls_column()` / `controls_row()`. A literal padding or
  gap in an app (`const PAD`, `+ 12.0`) is a number the ladder should supply; the audit counts
  them. Do not add new legacy spacing keys.
- **Radii are per rung, overridden per widget**: root `style.surface.plate.root.corner_radius`,
  pane `plate_corner_radius` (falls back to root), control `style.control.corner_radius`
  (`layout::control_corner_radius`, default 8). A new control-scale radius getter falls back to
  the control rung, never to a literal.
- **New layout is `scene::layout`** (measure → arrange; hand-rolled, not taffy). A settings page
  section's contents are a `Form` (`layout/form.rs`): declare widgets, text, rows, rules and
  fills; `SectionContext::place` solves and paints. One list row's cells are `lay_row`.
- Migrating an app onto the standard is pixel-neutral for the plate (`tests/plate_golden.rs`)
  and a measured change for spacing (screenshot, count the columns of flat face).

## Style and config

- **The style is one snapshot** (`crate::style::Style`, published as an `Arc`). Each style value
  is a `StyleCell` handle (`.read()` / `.write()`, lock-free reads); a reload runs as one
  `style::batch`. A new style value is a field in its module's `style_slots!` block and a
  `StyleCell`, never a new lock.
- **A write lands on the newest snapshot, never the one its guard copied** (guards do not wait
  on each other, so another write to the same field may publish while one is held). An
  unchanged guard publishes nothing; a changed one is a store, unless its cell is
  `StyleCell::merging` — a slot whose writers each change a piece of a collection (the
  registry, key by key) declares its merge, or concurrent writers lose each other's pieces.
- **A layout style key has one home, the registry** (`layout/registry.rs`). Its getter reads
  `registry_float` / `registry_string` / `registry_bool` with the default; its setter writes the
  registry. Do not add a separate slot or a config re-scan for a key. The flatten maps the paths
  the toolkit reads to its keys and leaves every other path whole — **another program's key never
  becomes the toolkit's**; a new key read from a nested block needs its mapping
  (`another_programs_keys_do_not_become_the_toolkits` is the test).
- The registry loads lazily: a value read before the first load and one after come from two
  configurations. A test asserting on shading numbers pins its inputs.
- **A config hex is gamma-decoded; a built-in default colour is not.** `parse_hex` runs `#rrggbb`
  through `srgb_to_linear`, so `"#595969"` is `[0.10, 0.10, 0.14]`. Converting a linear default
  to hex needs `l2s(c) = 1.055·c^(1/2.4) − 0.055`, not `c·255`. An 8-digit hex keeps its alpha
  raw; a 6-digit hex sets alpha to 1.0.
- **The surface block** (`style.surface`: plate, frost, material, relief wall/edge, menu) and the
  table of retired spellings are in `docs/surfaces.md`. A retired key is reported by path
  (`color::retired_surface_keys`) and not read; cce-relief's Save migrates a file.
- **Units**: the working unit is the logical pixel. Config lengths may carry units as KDL type
  annotations (`width=(mm)2.0`), resolved through the display metric at every read
  (`units::Len`, `units::metric()`); a bare number is logical px. See `docs/surfaces.md`.

## Global state

Style is the snapshot above. **A window's interaction and properties live in its
`window_state::WindowState`** — the context menu, hover highlight, side swipe, input-method
composition, scale, display metric, app id, scroll phase — which the shell makes current
(`window_state::enter`) while it runs that window's code. The modules' free functions act on the
current one; a thread with no window (a worker, a test) gets a default or the process-wide value.
**Do not add a static for state that belongs to a window: give it a field in `WindowState`.**
Caches and process-wide switches (vertical text, the icon cache) are fine as statics.
`docs/rfc-global-state.md` has the inventory.

## Accessibility and locale — paused

Accessibility work is **paused** (2026-10-09) to make room for other work; do not extend it unless
asked. What exists: the widget tree in AccessKit's schema (`a11y.rs`, `app_tree`), published over
AT-SPI on Linux behind the `a11y` feature for apps that opt in (`publishes_accessibility` or
`CCE_A11Y=1`), keyboard-first navigation (above), right-to-left text editing, and translatable
toolkit strings (`crate::l10n`: `tr("id")` over `locale/en-US/cce-ui.ftl`;
`every_message_the_toolkit_names_is_in_its_english` is the test). Font systems are built with the
user's locale (`locale::locale()`). The cheap rules stay in force because the rest of the
toolkit leans on them: widgets declare a `focus_role` and a label, menu actions are keyed by ID,
and a string the toolkit shows goes in the `.ftl`, not the source. Never give an AccessKit node a
role description (it registers as AT-SPI `Extended` and never appears on the bus).
`docs/rfc-accessibility-locale.md` has the state and the parked plan.

## Module map

- `backend/` — the runner, split so other shells share everything but Wayland:
  `app.rs` (the `Application` trait, `AppSender`), `driver.rs` (input state and routing:
  modifiers, key repeat, undo/redo and Tab chords, CSD hit zones, popover close, scroll phase,
  touch — unit-tested with no compositor), `frame.rs` (`build_frame`, damage), `shell.rs` (the
  `Shell` trait and `Pacer`: one turn of the loop over any shell), `tessellate/` (its `mod.rs` has the table), `text.rs`,
  `dom.rs` / `appkit.rs` (the browser's and AppKit's input vocabularies, portable),
  `touch.rs`, and the Wayland-only `window_runner/` (`EngineState`; its `mod.rs` has the table), `menu_popup.rs`,
  `dnd.rs`, `text_input.rs`, `a11y_unix.rs`. A routing change belongs in `driver.rs`, a frame
  content change in `frame.rs`, a pacing change in `shell.rs` — never in the Wayland code.
- `context.rs` — `UiContext`: the widget registry, routing, hit-testing, focus, modals.
- `scene/` — the core: `arena.rs` (generational forest), `tree.rs` (`WidgetTree`), `layout.rs`
  (the box model), `paint/` (`DisplayList`, `Prim`, `PaintCtx`, `Field`, `ControlPlate`,
  `PlateSpec`), `painter.rs` (the paint walk), `anim.rs` (`Animated<T>`), `material.rs`,
  `heightfield.rs`.
- `widget/` — `mod.rs` (`WidgetHost`), `model/` (the narrow traits and `Adapted`), `core/` (the `Widget` base, the context menu, the clipboard, hover animation), `handle.rs`,
  `embedded.rs`, `container/` (params pane, tree list, spreadsheet, menus, scroll boxes…),
  `input/` (button, slider, text box, dropdown, ramp…), `display/` (label, graph, svg…),
  `editor.rs` (`TextEditorState`, behind `TextBox`), `line_edit.rs` (`LineEdit`, a field an app
  draws itself), `context_menu`, `doc_editor/`, `markdown`.
  `widget/container/parameters_bg/` is the designer's and cce-files' parameter pane, split by
  concern (its `mod.rs` has the table). The largest files are now `UiContext`,
  `src/context.rs` (largest file, ~1.5k lines), and the text shaping in `backend/text.rs`.
- `layout/` — the style getters and setters, by topic (`relief.rs`, `spacing.rs`, `fonts.rs`,
  `controls.rs`, `graph.rs`; `mod.rs` has the table, `reload_config` and the slots), `registry.rs` (the style
  registry), `bridge.rs` (the flat-host bridge: `RenderTarget`, `render_widget_h`), `section.rs`
  (settings-page sections: `PageFlow`, `SectionContext`), `form.rs` (`Form`, `lay_row`).
- `color/` — the palette constants and the style slots (`mod.rs`, with the table), the getters
  and setters by topic (`surfaces.rs`, `controls.rs`, `lists.rs`, `graph.rs`), `load.rs` (config →
  colours, `retired_surface_keys`), `math.rs` (sRGB/linear, OKLab), `materials.rs`, `chords.rs`.
- `style.rs` (the snapshot), `window_state.rs`, `ime.rs` (composition shared by widgets and
  shells), `text_input.rs` (the per-frame field claim), `history.rs` (`History<T>`), `a11y.rs`,
  `l10n.rs`, `compute.rs` (what a compute job is, device-free).
- `draw/` — renderer-free drawing (above); `draw::scene` and `draw::rt` for 3D and the tracer.
- `vk/` — the Vulkan renderer (`renderer/` and `rt/`, each `mod.rs` with its table; `scene.rs`, `compute.rs`,
  `plate_probe.rs`); `web/` — wasm32 only: `WebRenderer`, the browser shell, its 3D, tracer and
  compute; `mac/` — macOS only: the AppKit shell.
- `icon.rs` (XDG icon-theme lookup for names other apps ship — not `lib.rs`'s `upload_icon`, which
  loads a bundled cce-icons glyph), `file_dialog.rs`, `scale.rs`, `wayland.rs` (scale and
  metric detection), `protocol.rs`, `mcp.rs`, `engine.rs` (the client-facing re-export surface).
- Portability: native-only code (`vk`, `ipc`, `file_dialog`) is `cfg(not(target_arch =
  "wasm32"))`; the Wayland shell is additionally `not(target_os = "macos")`. Portable code keeps
  time with `web_time::Instant`. A portable path that calls into a native module fails
  `check-wasm` — put the native half behind the cfg.

**Fonts and icons.** `lib.rs` builds the cosmic-text `FontSystem` (re-exported as
`cce_ui::cosmic_text`). Bundled fonts load from `$CCE_FONTS_DIR` (else `~/Dropbox/Fonts`); system
fonts only with `$CCE_LOAD_SYSTEM_FONTS` or `Application::load_system_fonts` (and always on
macOS). Icons from `$CCE_ICONS_DIR` (else `~/projects/cce/cce-icons/svg`).

## RFCs

| RFC | State |
|---|---|
| `rfc-core-rebuild.md` | Phases 0–6 done; now a design record and work log. Read §3 for the architecture's intent. |
| `rfc-material.md` | Done (steps 1–4: per-plate materials, named materials in config). |
| `rfc-global-state.md` | Phases 1–4 done. |
| `rfc-owning-registry.md` | Done. |
| `rfc-accessibility-locale.md` | Paused 2026-10-09 (see "Accessibility and locale — paused"). |

## Debug environment variables

All opt-in and read once. Set one and run any client.

| Variable | Effect |
|---|---|
| `CCE_PLATE_DEBUG=1` | per frame, which carves grouped into their host plate vs drew as overlays, and why |
| `CCE_PRESENT_DEBUG=1` | swapchain present/acquire tracing; each derived damage rect |
| `CCE_UI_FULL_DAMAGE=1` | repaint the whole window every frame |
| `CCE_UI_MENU_POPUP=0` | keep the context menu in the window |
| `CCE_UI_FAULT_RECONNECT=<s>` | drop the session after `s` seconds, once, to exercise reconnects |
| `CCE_UI_IDLE_MS=<ms>` | the idle dispatch ceiling |
| `CCE_UI_TURN_MS=<ms>` | slow a menu page turn down, to capture it frame by frame |
| `CCE_FOCUS_DEBUG=1` | print the Tab stops in walk order |
| `CCE_SCROLL_DEBUG=1` | log touch scrolls |
| `CCE_VK_DEVICE=<integrated\|discrete\|name>` | pick the GPU, lifting a session-wide `VK_DRIVER_FILES` pin; an unmet preference is printed to stderr with every device offered (`VK_LOADER_DEBUG=error` says why a driver failed to load) |
| `CCE_VK_RT=0` / `compute` | disable ray tracing / force the tracer's compute tier |
| `CCE_FORCE_SCALE=<f>` | override HiDPI scale detection |
| `CCE_FORCE_PPI=<f>` | pin the display metric (the live panel is 141.8) |
| `CCE_HEIGHTMAP=<file.png>` | export the third frame's relief as a height field (`CCE_HEIGHTMAP_MM` resamples) |
| `CCE_A11Y=1` / `CCE_A11Y_DEBUG=1` | publish the accessibility tree / log reader traffic |
| `CCE_LOAD_SYSTEM_FONTS=1`, `CCE_FONTS_DIR`, `CCE_ICONS_DIR` | font and icon sources |
