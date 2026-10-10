# Changelog

What changed in cce-ui, when, and why — with how it was verified where that mattered. Newest
first. The current behaviour is described in `CLAUDE.md` and `docs/*.md`; this file is the
history behind it. Work before 2026-09-11 is recorded in `docs/rfc-core-rebuild.md` (phases 0–6)
and `docs/rfc-material.md`, and in `git log`.

When you change behaviour: update the reference to the new state, and add an entry here under
today's date — what changed, why, and how it was checked.

## 2026-10-10

- **The doc links resolve** (27 files): `cargo doc -p cce-ui --all-features` reported 41 warnings
  and reports none. Sixteen links named items that had moved or been renamed — the `a11y_*`
  reads on `WidgetHostExt`, `Application::create`, `IDLE_DISPATCH`, `build_frame`,
  `TextAttrs`, `Prim::Text`, `Frost::Frosted`, `PaintCtx::inset_plate`, several broken when a
  split changed what `super::` names — and now point at the item. Twenty-five public docs
  linked private items (constants and helpers such as `PLATE_SHARE`, `Self::plain_quads`,
  `DOUBLE_CLICK`), which rustdoc cannot render; they name them in plain code. The text box's
  well doc named a `RenderTarget::recess` that does not exist and the removed `all_quads`; it
  now names `relief_carve`, what the bridge calls. Clippy and the suite pass.
- **Module docs describe the present** (24 files): the module docs that narrated the core
  rebuild ("Phase 5c leaf sweep", "until 2026-10-08", "the legacy `extra_quads`") now say what
  the module is. Several were wrong, not just dated: `scene/mod.rs` said nothing in it was wired
  into the render path; `scene/painter.rs` called routing the backend through the walk a
  follow-up; `StatusBar`'s described theming coupled to a "root plate container" and a
  `text_items` path, both gone; `UsageBar`'s and `StatusDot`'s named the removed
  `all_rounded_quads`; the reference app's said there is no popup surface (the context menu has
  one); `scene/anim.rs` now says no widget holds an `Animated` yet. "Legacy" stays where it names
  a path that exists (the banded edge shading, the colour-file key fallback, the flat-host
  `RenderTarget`). One broken intra-doc link fixed on the way (`clips_children` lives on
  `WidgetHostExt`). Also deleted `probe_slider_bridge`, a test that only printed. Clippy and
  the suite pass.
- **`DatePicker`, a month grid for choosing a day** (`widget/input/date_picker.rs`): an
  immediate-mode helper a host opens over its content and routes input to, built first inside
  cce-list for its items' due dates and moved here when cce-calendar wanted the same one.
  Popover surface, cce-icons chevrons, names through `l10n` (`date-picker-*`), sizes from the
  getters; `with_title` / `with_week_start` / `without_clear`. Adds `chrono` without default
  features (date arithmetic only; the host passes today). Four unit tests (grid start for both
  week starts, hit mapping, the footer and Clear, placement flipping in a small window, keys
  across a month boundary); clippy and the suite pass; checked on screen in both apps in a
  shadow session. `scripts/check-wasm` not run (no wasm32 target on this machine).
- **The Vulkan renderer's backdrop pass has a file of its own** (`vk/renderer/backdrop.rs`):
  `record_backdrop` — the 3D scene and the tracer into the backdrop, and its copy into the
  swapchain image — moves out of `record.rs`, which keeps the UI pass. A pure move, checked line
  for line. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass (also
  excluding cce-data-editor, mid-edit in another session); not run on screen.
- **The height field is a directory module** (`scene/heightfield/`): `mod.rs` (`HeightField`,
  with `from_frame` — one 200-line function, left whole — and its readers), `profiles` (the
  wall and roll height curves, the rounded-rect SDF), `export` (`request`, the `CCE_HEIGHTMAP`
  request, `export_png`) and `tests`; re-exported, so `scene::heightfield::…` paths are
  unchanged. A pure move, checked line for line. Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **The Vulkan tracer's construction and tests have files of their own** (`vk/rt/`): `init.rs`
  (`RtStage::new`, one 250-line function — the tier's pipelines, descriptor layouts and sets,
  the frames in flight) and `tests.rs` (the GPU tests); `mod.rs` keeps the tier, `RtStage`'s
  state, staging and teardown. A pure move, checked line for line. Clippy, the suite, the
  tracer's GPU tests (`CCE_VK_RT=compute`, `--ignored`) and `cargo check --workspace --exclude
  cce-fx` pass.
- **A text measurement reads the scale once** (`backend/text/shape.rs`, the text box's
  `prepare_text` and shaping, `shaped_cluster_offsets`, `ShapingMeasure`): the new
  `shared_text_buffer_at` shapes at a scale the caller passes, and every path that divides a
  buffer's physical positions by a scale now shapes that buffer at the same value.
  `shared_text_buffer` read `scale_factor()` for the buffer by itself, so when the scale changed
  between the caller's read and its own, the offsets came out multiplied by the ratio — on CI's
  macos job (run 38064718562) a key read at 1.0 and a buffer shaped at 1.5 gave offsets 1.5× too
  large. The text box's key, its column-advance probe, its single-line and per-line offsets and
  its advance cache all take the key's scale; `shared_laid_out_buffer` passes its scale to the
  single run it starts from. The `.max(1.0)` the text box and `shaped_cluster_offsets` put on
  the division (but not on the buffer) is gone from both: the glyph pass draws the buffer shaped
  at the raw scale, so the measurement uses the raw scale too — below 1 the clamp put the carets
  at `scale×` their place (at 0.75, three quarters of the way along the text). Checked by two
  tests that pin the window's scale on their own thread through `window_state::enter`:
  `a_shape_keyed_at_one_scale_is_measured_at_that_scale` (a key taken at 1.0, the window at 1.5
  while it shapes) and `offsets_are_logical_px_at_every_window_scale` (0.75, 1.5, 2.0 against
  1.0, one line and wrapped). Against the tree before, the first fails with the CI failure's
  1.5× offsets and the second with 0.75× offsets; both pass after. This is the reader's half of
  "Tests no longer share the process-wide scale" below, which fixed the writer: that one stops
  tests rescaling each other, this one makes a scale change mid-shape harmless anywhere.

- **The material setters change their cells under one guard, merged by name and by rung.**
  `color::set_named_material` and `set_material_binding` read the whole cell (`style_read`),
  changed one entry and wrote the whole cell back (`style_write`): two guards, so a material
  another thread defined, or a rung it bound, between the read and the write was put back
  out. (Under the per-slot `RwLock`s it was the same race; the 2026-10-09 guard fix could not
  reach it, since each guard was right on its own.) They now go through `style_update`, which
  runs the change under ONE write guard — and, under `cfg(test)`, on the thread's overlay as
  `style_write` does — and the two cells are `StyleCell::merging`: `NAMED_MATERIALS` merges
  by name (`merge_materials`: a name the write added, redefined or dropped is applied to the
  newest list; every other is left as it is), `MATERIAL_BINDINGS` by rung. One guard alone
  would not do: a guard whose field another write changed meanwhile otherwise lands as a
  store. A config load still replaces both cells whole, inside its batch. Tests:
  `color::materials::tests::concurrent_material_writes_keep_each_others_change` (sequenced
  with channels on the real cells, under `test_color_state_lock`; fails every run with the
  cells unmerged), `the_material_merges_land_only_what_the_write_changed`, and
  `tests/material_setters_race.rs`, an integration test (cce-ui without `cfg(test)`, so the
  shipped setters run, not the overlay) racing two threads of 500 definitions and bindings
  each: the old setters failed it 20 of 20 runs (the last lost 470 of 1000 materials), the
  new ones pass 20 of 20. Verified on the rebased tree: 10 of 10 full
  `cargo test --all-features` runs and 50 of 50 `--lib` runs; clippy clean. (Earlier loops on
  the pre-rebase tree also had every style and material test passing; the runs that failed
  there failed only in five tests that write temp files, when the shared `/tmp` quota ran out.)

- **Tests no longer share the process-wide scale** (`scale.rs`): under `cfg(test)` the value a
  thread on no window reads, and `set_scale_factor` writes besides the window's, is per thread.
  `offsets_count_chars_and_reshape_on_change` failed once on CI's macos job (run 38064718562,
  commit 8007f9e) with the first `prepare_text`'s offsets exactly 1.5× the second's: no test
  enters a window but `window_state`'s, which sets the process scale to 2.0 then leaves it at
  1.5, and it did so while the text box's first shape was under way — `prepare_text` had read
  1.0 for its key and its division, `shared_text_buffer` read 1.5 for the buffer, so the offsets
  came out in physical px; the second call saw a new key, reshaped consistently, and differed.
  (The slow first shape over macOS's system fonts is what made the window wide.) Every shaping
  test read that one value, so the fix is at the writer, not a pin in one test.
  `each_window_has_its_own_scale_and_a_worker_reads_the_last` now checks the fallback on its own
  thread and that another thread keeps 1.0; its old worker check compared two reads made in the
  worker, which held whatever the value. Checked by setting the scale from another thread
  between the two calls: the tree before fails with the CI failure's shape, the tree after
  passes; the suite ran clean several times.
- **The context menu's API and marks have files of their own** (`widget/core/context_menu/`):
  `api.rs` (the free-function API apps and the runner call, each acting on the current window's
  menu) and `marks.rs` (the row marks and chevrons, `split_mark`, the label-to-action
  fallback); `mod.rs` keeps the constants, slider rows and `ContextMenuState`. Re-exported, so
  `context_menu::…` paths are unchanged. A pure move, checked line for line. Clippy, the suite
  and `cargo check --workspace` pass, excluding cce-fx and two crates another session had
  mid-edit (cce-sheets, cce-documents), whose errors are in their own uncommitted changes.
- **`Prim` has a file of its own** (`scene/paint/prim.rs`): the enum (one 280-line item), `faded`,
  `Cap`, `Radii`, the relief-family naming note, the `DropletSpec` re-export and `DropletFinish`;
  `paint/mod.rs` keeps the display list and `PaintCtx`'s stacks, `push`, `append_items`, `replay`
  and `finish`. Re-exported, so `scene::paint::Prim` and the rest are unchanged. A pure move,
  checked line for line; CLAUDE.md now points at `prim.rs` for the prim list. Clippy, the suite
  and `cargo check --workspace --exclude cce-fx` pass.
- **The Vulkan core is a directory module** (`vk/core/`): `mod.rs` (`VkCore`, its constructors,
  the device setup in `new_inner` — one 295-line function, left whole — and `Drop`), `instance`
  (the process-wide shared instance, its validation layer and debug callback), `surface`
  (`SurfaceTarget`, `SurfaceLost`) and `device` (naming a device type, reporting an unmet
  `CCE_VK_DEVICE` preference, with its test); `SurfaceTarget` and `SurfaceLost` re-exported. A
  pure move, checked line for line. Clippy, the suite and `cargo check --workspace --exclude
  cce-fx` pass.
- **`ScrollBox` is a directory module** (`widget/container/scroll_box/`): `mod.rs` (the struct,
  its rect and bounds, the bar's geometry and raise state, the virtualization), `input`, `paint`
  (the relief scrollbar, re-exported for the text box, the pills, the flat quads) and `tests`. A
  pure move, checked line for line. The module doc, an account of its Phase 6av demotion from
  `WidgetHost` naming a consumer that no longer exists, now says what it is and who embeds it.
  Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The text box's tests are split by topic** (`widget/input/text_box/tests/`): `shaping` (glyph
  offsets, right-to-left text, wrapping), `pointer` (focus, clicks, drags, selection, the
  context menu), `editing` (composition, undo runs, the clipboard and passwords, the search
  box's clear) and `geometry` (horizontal scrolling, the border, where tall text starts). A pure
  move, checked line for line, except one doc: `multiline_shaped_offsets_round_trip`'s, which had
  ended up at the top of the file glued to the first right-to-left test's, is back on it. The
  suite still runs 657 tests.
- **Smooth scrolling is a directory module** (`widget/scroll_motion/`): `mod.rs` (the scroll phase,
  the shared constants, `Bounds`), `settings` (`ScrollSettings`, the animations switch, the
  per-thread pin), `axis` (`ScrollAxis`), `motion` (`ScrollMotion`) and `tests`; re-exported, so
  `widget::scroll_motion::…` paths are unchanged. A pure move, checked line for line;
  `ScrollAxis`'s fields are `pub(super)`, since `ScrollMotion` stops both axes directly. Clippy,
  the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The `DocEditor`'s line layout is a directory module** (`widget/doc_editor/layout/`): `mod.rs`
  (`Run`, `Deco`, `LineLayout` and `layout_line`, one 320-line function left whole), `theme` (the
  `EditorTheme`, indents, the pill metrics), `geometry` (image layouts, the caret's x, selection
  rects, the column at an x, the link at a point) and `tests`; re-exported, so paths are
  unchanged. A pure move, checked line for line. Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **The browser shell is a directory module** (`web/shell/`): `mod.rs` (`Sizing`, `Fonts`, `run`,
  `capture`, the small DOM helpers), `canvas` (`WebShell` and its `Shell` impl) and `events` (the
  page's event loop: `Loop`, its scheduling, the events installed on the canvas and the keyboard
  sink). `run` builds `WebShell` and `Loop`, so their fields and methods are `pub(super)`. A pure
  move, checked line for line. Browser-only code this machine cannot compile: committed on a
  branch and built by CI's `wasm` job first.
- **The Wayland runner's reconnect policy is its own file** (`backend/window_runner/reconnect.rs`):
  `SessionEnd`, `AfterSession`, the compositor-socket wait, the backoff constants and
  `after_session`, with the `reconnect_tests` that test them; `session.rs` keeps
  `raise_fd_limit`, `run` and `run_session` (one 350-line function, left whole). A pure move,
  checked line for line. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The path tracer's CPU half is a directory module** (`draw/rt/`): `mod.rs` (the scene schema),
  `bvh` (the binned-SAH build), `pack` (the GPU layouts, constants, packing, `PreparedRtScene`,
  the parameter blocks) and `tests`; re-exported, so `draw::rt::…` paths are unchanged. A pure
  move, checked line for line; three section banners are dropped, one of which named a shader
  (`rt.wgsl`) that no longer exists. Clippy, the suite, the tracer's GPU tests
  (`CCE_VK_RT=compute`, `--ignored`) and `cargo check --workspace --exclude cce-fx` pass.
- **The Vulkan renderer's frame is split** (`vk/renderer/`): `frame.rs` keeps `draw_frame_2d`
  (the order of its phases), the immediate uploads, text and the frame's upload; recording the
  backdrop, the UI pass, the display list, images and the overlay moves to `record` (with
  `clip_to`), acquiring, the damage and partial region, submitting, image ages and presenting
  to `present`. A pure move, checked line for line; the methods called across the files are
  `pub(super)`. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass; not run
  on screen.
- **`Checkbox` and `Toggle` are split** (`widget/input/checkbox/`): `mod.rs` (the box both share,
  `paint_box`, which the radio group also draws with, the value parser, `Checkbox` whole),
  `toggle` (`Toggle` whole, re-exported) and `tests`. A pure move, checked line for line; two of
  `Toggle`'s fields the tests set (`slide_t`, `focused`) are `pub(super)`. The module doc
  ("Phase 5e", byte parity with the legacy `extra_quads` / `extra_arcs` views, which are gone)
  describes the two controls as they are. Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **Materials are a directory module** (`scene/material/`): `mod.rs` (`Material`, `PlateRole`),
  `finish`, `frost`, `def` (`MaterialDef`, `FrostDef`, `PlateRung`) and `tests`; all
  re-exported, so `scene::material::…` paths are unchanged. A pure move, checked line for line;
  the tests' `include_str!` of `shader2d.wgsl` gains a `../` for the deeper file. The module doc
  narrated RFC steps 1 and 2 as in progress; it now describes the finished design. Clippy, the
  suite and `cargo check --workspace --exclude cce-fx` pass.
- **`Spinbox` is a directory module** (`widget/input/spinbox/`): `mod.rs` (the struct, its
  relief and geometry, construction, the caret, the value's text, parsing and stepping, the
  builders, `impl Layout`), `paint`, `input` and `tests`. A pure move, checked line for line;
  the module doc ("Phase 5i") describes the spinbox as it is. Clippy, the suite and `cargo check
  --workspace --exclude cce-fx` pass.
- **`Button` is a directory module** (`widget/input/button/`): `mod.rs` (the kinds, the struct,
  construction, icons, the label's font and width, the plate, the builders, `impl Layout`,
  `PageButton`), `paint`, `input`, and the two test modules as files. A pure move, checked line
  for line. The module doc ("Phase 5f", parity with the legacy `mouse_input`, a colour matrix
  that "becomes `Animated<f32>` lerping in RFC §3.6") describes the button as it is. Clippy,
  the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The box model is a directory module** (`scene/layout/`): `mod.rs` (`Size`, `Rect`, `Edges`,
  `fit_rect`), `style` (axes, modes, lengths, alignment, `Style` and its presets, `LayoutBox`),
  `solve` (`compute_layout`, `measure`, `arrange` and their per-mode halves) and `tests`; all
  re-exported, so `scene::layout::…` paths are unchanged. A pure move, checked line for line.
  The module doc's account of the legacy layout it replaced and its "Phase 2" framing are cut;
  what it does not handle stays, stated as a limit. Clippy, the suite and `cargo check
  --workspace --exclude cce-fx` pass.
- **`Breadcrumb` is a directory module** (`widget/container/breadcrumb/`): `mod.rs` (the
  struct, its constants and associated constants, construction and setters, the path,
  `impl Layout`, `PathController`), `geometry` (the visible segments and their elision, hit
  zones, the plate band, the run box and seams), `paint`, `input`, and the two test modules as
  files. A pure move, checked line for line. The module doc, which called it "the first
  controller widget across" and described the designer downcasting a roster entry to reach it
  (it takes a `&mut dyn PathController` through the context now), describes the breadcrumb as
  it is. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The WebGPU renderer is a directory module** (`web/renderer/`): `mod.rs` (its state, images,
  captures, `new`, resizing, text, `impl Stage3D`), `gpu` (the WebGPU helpers; the four the 3D
  and tracer passes import are `pub(in crate::web)` and re-exported from `renderer`, so
  `super::renderer::…` still resolves for them) and `frame` (draining images, the blur
  snapshot, the passes, `draw_frame_2d`). A pure move, checked line for line. Browser-only
  code this machine cannot compile: committed on a branch and built by CI's `wasm` job first.
- **The Markdown reading view is a directory module** (`widget/markdown/`): `mod.rs` (the theme
  and colours, `Draw` / `Hit` / `Layout` and painting it, the `layout` entry points, the
  layouter's state), `blocks` (each block kind's layout) and `inline` (spans laid out word by
  word, with their tokens), and `tests`. A pure move, checked line for line; the doc's
  "moved here from cce-notes" note is dropped. Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **The macOS shell is split** (`mac/`): `mod.rs` keeps `run`, the `Ev`s the AppKit side hands
  the shell (`send`, the sink) and the menu; `MacShell` moves to `shell`, `CceView` (with its
  input-method state) to `view`, the application and window delegate to `delegate`. What
  crosses between them is `pub(super)`: `MacShell`'s fields and methods (`run` builds and drives
  it), `ViewIme`'s fields (the shell reads the marked text), the two classes
  (`define_class!` takes a visibility) and their constructors. A pure move, checked line for
  line. Nothing here compiles `src/mac` (no `aarch64-apple-darwin` target on this machine), so
  CI's `macos` job is the first build of it.
- **The params pane's tests are split by topic** (`widget/container/parameters_bg/tests/`):
  `mod.rs` (the imports and `panel_with`), `rows`, `code` (with the key and dispatch helpers
  only it uses), `sections` (with the outline-end helpers) and `pointer`. A pure move, checked
  line for line; the suite still runs 657 tests.
- **The widget module root is split** (`widget/`): `mod.rs` keeps widget ids, the layout value
  types, `ContextAction`, `EmbedImage`, `CornerRadii` and the module tree with its re-exports;
  the input vocabulary moves to `events` (with `match_key_shortcut` and its tests),
  `WidgetHost` / `WidgetHostExt` / `NoModel` to `host`, the controller traits to
  `controllers`. All re-exported, so `widget::…` paths are unchanged. A pure move, checked line
  for line, except a tombstone comment about the deleted `Control` subtrait, dropped; the module
  gained the doc it lacked. Clippy, the suite and `cargo check --workspace --exclude cce-fx`
  pass.
- **The `DocEditor`'s live preview is a directory module** (`widget/doc_editor/preview/`):
  `mod.rs` (what a line is, the styled `Line`, block contexts, `style_line`), `embeds`,
  `properties` (the frontmatter as a Properties table), `inline` (the inline-markup scanner) and
  `tests`; everything public re-exported, so `doc_editor::preview::…` paths are unchanged. A
  pure move, checked line for line. Clippy, the suite and `cargo check --workspace --exclude
  cce-fx` pass.
- **The params pane's paint is split** (`widget/container/parameters_bg/`): `paint.rs` keeps row
  floors, section outlines and arcs, the scrollbar, the ramps and trackballs a host paints
  through the scene path, and `impl Paint`; the labels and glyphs move to `text` (with
  `fit_tail`), the controls' quads, reliefs, fields, grooves, spheres and fillets to `chrome`
  (with `Relief` and its helpers). A pure move, checked line for line. Clippy, the suite and
  `cargo check --workspace --exclude cce-fx` pass.
- **The params pane's input is split by event** (`widget/container/parameters_bg/`): `input.rs`
  keeps hover, the row being typed into, committing a row, the pointer moving and `impl Input`;
  presses move to `press` (with `hold_focus` and `fill_from_pick`), keys to `keys`, the wheel to
  `wheel`. A pure move, checked line for line. Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **`Float3` is a directory module** (`widget/display/float3/`): `mod.rs` (the struct,
  construction, components and values, the rows' layout, `impl Layout` and `impl Paint`), `ball`
  (the trackball: the camera view, rolling, rings, its paint, drag and scroll), `input` (the
  wheel routed to a row, `impl Input`) and `tests`. A pure move, checked line for line. The
  module doc spoke of hosts reading the group "through the legacy flat views", which are gone;
  it now describes the group as it is, two to four rows included. Clippy, the suite and
  `cargo check --workspace --exclude cce-fx` pass.
- **`ColorSelector` is a directory module** (`widget/input/color_selector/`): `mod.rs` (the
  struct, construction and builders, the field's relief, the hex value, `impl Layout`, placing
  the picker), `paint`, `input` (editing the hex, launching the picker and reading its stream)
  and `tests`. A pure move, checked line for line; the module gained the doc it lacked. Clippy,
  the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The Vulkan image stage is a directory module** (`vk/image/`): `mod.rs` (the GPU image, the
  stage and its limits, the descriptor bindings, `new` and `destroy`), `pending` (draining the
  upload queue, freeing images), `upload` (the shared staging buffer, one-shot and batched
  uploads, recording the copy and the mip chain), `write` (replacing pixels in place, whole or by
  regions), `draw` (the frame's quad vertices, descriptors, recording a quad) and `tests`. A
  pure move, checked line for line. Clippy, the suite and `cargo check --workspace --exclude
  cce-fx` pass; not run on screen.
- **`ScrollRegion` is a directory module** (`widget/scroll_region/`): `mod.rs` (the region:
  construction, rect and bounds, scroll position, the virtualization maths), `activity`
  (`ScrollbarActivity` and its timings), `scrollbar` (the bars' geometry, hit tests and
  raising), `input`, `emit` (the frame and bars as prims or flat quads) and `tests`;
  `ScrollbarActivity` and the timings re-exported, so paths are unchanged. A pure move,
  checked line for line; the module doc's account of where the region was lifted from is cut
  to the apps that share it. Clippy, the suite and `cargo check --workspace --exclude cce-fx`
  pass.

## 2026-10-09

- **The input `Driver` is a directory module** (`backend/driver/`): `mod.rs` (the vocabulary a
  shell speaks — `Turn`, `Modifiers`, `PressSite`, `ScrollFrame`, … — the `Driver`, its tick,
  and the small helpers), `pointer`, `scroll` (wheel frames, touch, the pinch fallback), `keys`
  (modifiers, focus, keys, committed and composed text, repeat, the Tab and undo/redo chords)
  and `tests`. A pure move, checked line for line; the children reach `touch` as
  `crate::backend::touch`. Clippy, the suite and `cargo check --workspace --exclude cce-fx`
  pass.
- **`MenuBar` is a directory module** (`widget/container/menu/`): `mod.rs` (the struct, its
  geometry, construction and builders, `impl Layout`, its `Send` / `Sync`), `paint`, `input`,
  `controller` (`MenuController` and `PageSelector`) and `tests`. A pure move, checked line for
  line. The module doc was a migration log (the standalone `Menu` deleted, the text caches and
  vertical-mode height dropped — recorded in `git log`) and said `arrange_children` parented
  the strip back to the adapter, which it no longer does; it now describes the bar as it is.
  Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The sliders are a directory module** (`widget/input/slider/`): `mod.rs` (`Slider`: its
  geometry, construction and setters, value stepping, the readout's commit, `impl Layout`, the
  label strip), `band` (the band's profile and shape, public for app-owned scrubbers), `paint`,
  `input`, `range` (`RangeSlider` whole), and the two test modules as files. A pure move,
  checked line for line; the module doc is rewritten without its migration-era notes. Clippy,
  the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The `DocEditor` is split by concern** (`widget/doc_editor/`): beside `buffer`, `layout` and
  `preview`, its `mod.rs` keeps the struct, construction and the text in and out, and the rest
  of `impl DocEditor` moves into `incremental` (which lines show raw, line heights, keeping
  layouts in step), `composition`, `caret` (geometry, the position at a point, vertical moves),
  `keys` (the keymap, edits, undo and redo), `pointer` (presses, drags, links, the wheel and
  tick) and `paint`, and the tests into a file. A pure move, checked line for line; the
  `// ---- section ----` banners are dropped. Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **The Vulkan 3D scene stage is a directory module** (`vk/scene/`): `mod.rs` (the stage's
  state, staging a frame, teardown), `pipelines` (`SceneStage::new` — render pass, layouts,
  pipelines, per-frame buffers — and the descriptor write), `targets` (the backdrop and depth
  images), `mesh` (`Mesh`, creating and replacing meshes, reclaiming spares) and `record` (a
  frame's uniforms and the recorded pass). A pure move, checked line for line; the children
  reach `renderer` and `image` as `crate::vk::…`. `new` is still one 575-line builder. Clippy,
  the suite and `cargo check --workspace --exclude cce-fx` pass; not run on screen.
- **`LineEdit` is a directory module** (`widget/line_edit/`): `mod.rs` (the model, its
  outcomes, char boundaries, construction, the display, selection helpers), `composition` (the
  input method's composition and the index maps across it and a mask), `pointer` (presses,
  drags, multi-clicks, word boundaries), `keys` (the keymap, undo and redo, the reader's text)
  and `tests`. A pure move, checked line for line; the `// ---- section ----` banners are
  dropped, and the pointer section's note on how the app hands in hit-tested offsets is now
  `pointer`'s module doc. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The runner's text shaping is a directory module** (`backend/text/`): `mod.rs` (vertical
  text), `cache` (the shaped-buffer cache), `fonts` (face aliases, rescans and the font-op log,
  family resolution, face snapping), `shape` (shaping a buffer, single run or laid out),
  `runs` (shaped runs and clusters, bidi levels and visual order), `display` (the display
  list's text for the glyph pass, the popover-occlusion clamp), and the two test modules as
  files. Items the siblings share are `pub(super)` and glob-imported; every public item is
  re-exported, so `backend::text::…` paths are unchanged. A pure move, checked line for line.
  Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass; `check-wasm` was not
  run (no wasm32 target here), so CI's wasm job is the first browser build of it.
- **`UiContext` is a directory module** (`context/`): `mod.rs` (the struct, the registry —
  insert, get, remove, lend — delivery, dirty state, ticks, the hierarchy, `ctx[h]`), `spatial`
  (`SpatialGrid`), `events` (dispatch and the scroll-gesture bookkeeping), `focus` (focus by id,
  modals, the Tab walk), `popovers` (popovers, coverage, the hit tests that ask them), `menu`
  (the context menu), and the two test modules as files. A pure move, checked line for line,
  with three exceptions: the `// --- Section ---` banners and two tombstone comments about code
  removed long ago (`navigate_focus`, the context's own hover highlight) are dropped, and
  `propagate_event`'s doc paragraph, which sat glued to the front of `note_scroll_event`'s and
  left `propagate_event` undocumented, is back on `propagate_event`. Clippy, the suite and
  `cargo check --workspace --exclude cce-fx` pass.
- **The colour getters are split by topic** (`color/`): `mod.rs` keeps the palette constants,
  the `style_slots!` block, the test overlay and `style_read` / `style_write`; the getters and
  setters move with the `StyleCell` statics they read into `surfaces` (pages, pane and root
  plates, the well frame, menus, frost and finish, the theme), `controls`, `lists` and `graph`,
  and the two test modules into files. The statics are `pub(super)` and glob-imported, so `load`
  and `chords` still write them through `use super::*`; every getter is re-exported, so
  `color::…` paths are unchanged. A pure move, checked line for line; the module gained the doc
  it lacked. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **`Spreadsheet` is a directory module** (`widget/container/spreadsheet/`): `mod.rs` (the
  struct, its constants and scroll geometries, construction, column widths, taking a table,
  `SpreadsheetController`), `column` (`SheetColumn`), `scroll` (both scroll geometries and the
  scrollbars), `rows` (the header and body hit tests, sorting, selection), `paint`, `input` and
  `tests`. `SheetColumn` re-exported, so paths are unchanged. A pure move, checked line for
  line; the module doc's dated notes are rewritten in the present tense. Clippy, the suite and
  `cargo check --workspace --exclude cce-fx` pass.
- **The widget model is a directory module** (`widget/model/`): `mod.rs` (the `Adapted` struct,
  `Drop`, `Default`, `Deref`), `layout`, `paint` and `input` (one trait each; `input` also holds
  `FocusRole` and `EventCtx`), `adapted` (`Adapted`'s inherent API), `host` (`impl WidgetHost
  for Adapted`) and `tests`; every public item re-exported, so `widget::model::…` paths are
  unchanged. A pure move, checked line for line. The module doc, which narrated the Phase 5
  migration in the future tense ("when the last widget is migrated, `WidgetHost` and this
  adapter are deleted"), now describes the traits and the adapter as they stand. Clippy, the
  suite and `cargo check --workspace --exclude cce-fx` pass.
- **The ramp editors are a directory module** (`widget/input/ramp/`): `mod.rs` (the float
  `Ramp`: its key, construction, presets, the spec in and out, `impl Layout`), `geometry` (the
  plot, key rings, rolled rim, where a dragged key lands, the controls' places), `paint`,
  `input`, `color` (the `ColorRamp` widget whole) and `tests`; `ColorRamp`, `ColorRampKey` and
  the spec parsers re-exported, so `widget::input::…` paths are unchanged. A pure move, checked
  line for line; the module gained the doc it lacked, in place of two section-banner comments.
  Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **`TreeList` is a directory module** (`widget/container/treelist/`): `mod.rs` (the element
  types, the struct, construction, its embedded fields and their placement, rebuilding,
  selection, `impl Layout`), `tree` (key paths, the search match, building rows from the value;
  copy, delete, expand, collapse), `paint`, `input` and `tests`; its five `impl TreeList` blocks
  are now one per concern. A pure move, checked line for line; the module gained the doc it
  lacked. Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **`Dropdown` is a directory module** (`widget/input/dropdown/`): `mod.rs` (the struct, its
  constants and associated constants, construction and setters, `impl Layout`), `text`
  (measuring the trigger's text), `popover` (the list's animation, geometry and rows), `paint`,
  `input` and `tests`. A pure move, checked line for line; the module doc's migration-era
  "parity notes" are rewritten in the present tense, keeping what still holds (the parent
  snapshot, the hit, the auto-width measure). Clippy, the suite and `cargo check --workspace
  --exclude cce-fx` pass.
- **The paint vocabulary is a directory module** (`scene/paint/`): `mod.rs` (`Prim`, the display
  list, `PaintCtx`'s clip and translate stacks, `push`, `append_items`, `replay`, `finish`),
  `surfaces` (`PlateSpec`, `PlateStance`, `Field`, `ControlPlate`), `relief` and `flat`
  (`PaintCtx`'s emitters), `text`, `render_target` (`PaintCtx` as a flat host's target) and
  `tests`; every public type re-exported, so `scene::paint::…` paths are unchanged. A pure move,
  checked line for line; the module doc, which described the three paint routes the rebuild
  replaced as "today", is rewritten for what the module is. Clippy, the suite and
  `cargo check --workspace --exclude cce-fx` pass, and `tests/plate_golden.rs` tessellates to
  the same bytes as this morning's golden from `fdd46b3`.
- **`Graph` is a directory module** (`widget/display/graph/`): `mod.rs` (the node and port types,
  `node_wires`, the struct, its construction and setters, `impl Layout`), `geometry` (the
  lattice, node and port rects, a free cell, zoom), `wires` (styles, paths drawn and hit alike,
  stroke and turn, painting, splicing; `WireStyle` re-exported), `paint`, `input` (presses,
  drags and their commit, the swap target, the zoom keys), `controller` and `tests`. A pure move,
  checked line for line; the module doc lost its two dated history sentences. Clippy, the suite
  (the graph's wire, turn and swap tests included) and `cargo check --workspace --exclude cce-fx`
  pass.
- **The layout getters are split by topic.** `layout/mod.rs` (2.3k lines) keeps the style slots,
  the shared constants, text-line metrics and `reload_config`; the getters and setters are
  `relief` (light, bevel and roll geometry, corner shape and window radii, carve depth, the
  profile tables and their tests), `spacing` (the ladder and label margins), `fonts`, `controls`
  (heights, radii, opacities, scrollbar sizes) and `graph`, each re-exported whole so every
  `crate::layout::…` path is unchanged; the registry's typed reads (`registry_float`,
  `registry_bool`, `registry_string`, `parsed_font`) moved into `registry.rs` beside the
  registry; the tests are `tests.rs`. A pure move, checked line for line across both files.
  Clippy, the suite and `cargo check --workspace --exclude cce-fx` pass.
- **The path tracer's Vulkan side is a directory module** (`vk/rt/`): `mod.rs` (the tier,
  `RtStage`'s construction, background, environment, staging and destroy, the tests), `scene`
  (a scene's upload and image descriptors, the acceleration structures), `frame` (targets,
  uniforms, `record`), `denoise`, `texture` and `offscreen` (`RtOffscreen`, re-exported). A pure
  move, checked line for line; the moved impls' private methods and the moved structs' fields
  are `pub(super)`. Clippy, the suite, and the tracer's `#[ignore]`d GPU tests pass on the
  compute tier (`CCE_VK_RT=compute`), the default device and the NVIDIA device
  (`VK_DRIVER_FILES` pinned, `CCE_VK_DEVICE=discrete`); `cargo check --workspace --exclude
  cce-fx` builds.
- **The Wayland runner is a directory module** (`backend/window_runner/`): `mod.rs` (the
  re-exports clients reach the runner through, `EngineState`, GPU init, geometry, resize),
  `present` (render, the text input's sync, the `Shell` impl), `layer` (a layer surface dropped
  and re-attached), `handlers` (the SCTK handlers and delegates), `pointer`, `keyboard` (with
  `text-input-v3`), `protocols` (the cce inspector and window management) and `session` (`run`,
  reconnects, waiting out a compositor). A pure move, checked line for line. Clippy, the suite
  and `cargo check --workspace --exclude cce-fx` pass; the demo built from this tree matches
  `fdd46b3`'s to the pixel through the 13-step shadow script, and `CCE_UI_FAULT_RECONNECT=2`
  drops and re-opens its session (a second toplevel from the same pid two seconds in).
- **Found: a binary built together with examples reads no config.** Building `--example …` in
  the same `cargo build` unifies cce-ui's dev-dependency on cce-core's `test-isolation` into the
  binary, which then draws the default style — the draw_frame_2d A/B's builds were both made so
  (a fair comparison, in the default style). `ccebuild` is unaffected (no examples). CLAUDE.md
  ("Verify on screen") now says so, with the shadow-driving lessons from the same A/B.
- **`widget/core.rs` is a directory module, and the context menu is split.** `core/mod.rs` keeps
  the `Widget` base, `clear_widget_references` and `impl_widget_base!`; the inline modules it
  carried are files — `hover_animation`, `clipboard`, `context_menu`, and the menu's four test
  modules — at the same nesting, so every `widget::core::…` path and `super::` inside them is
  unchanged. `context_menu/` is then `mod.rs` (the constants, marks, slider rows,
  `ContextMenuState` construction / show / hide, and the whole free-function API apps call),
  `turn` (page rows and the animated turn), `geometry` (placement, scrolling, rows as drawn),
  `input` (hover and keyboard stepping, presses, the wheel, slider rows) and `paint`
  (`paint_menu_plate` re-exported). Both steps pure moves, checked line for line; clippy, the
  suite and `cargo check --workspace --exclude cce-fx` pass.
- **`TextBox` is a directory module** (`widget/input/text_box/`): `mod.rs` (the struct,
  construction and setters, `impl Layout`, the shared font database), `shaping` (advances, wrap,
  x ↔ column ↔ index), `geometry` (border, padding, clip, content width, scrolling to the
  caret), `editing` (editor state, undo, clipboard, selection, keys), `composition` (the
  input-method run), `paint` and `input`, and `tests`. A pure move, checked line for line; the
  module doc, which still described the deleted `extra_quads` / `all_rounded_quads` render
  paths, is rewritten. Clippy, the suite (the text box's tests included) and
  `cargo check --workspace --exclude cce-fx` pass (the compositor's C build fails against this
  machine's wlroots headers, independently of this change).
- **`draw_frame_2d` is its phases.** The 653-line method keeps the guards, the fence wait and
  the command-buffer bracketing, and calls `want_snapshot`, `acquire`, `upload_frame`,
  `record_backdrop` (the 3D and tracer passes and the backdrop copy), `frame_damage`,
  `partial_region` (the damage grown around frosted plates), `record_ui_pass` (which calls
  `record_display_list` and `record_overlay`), `submit`, `age_images` and `present`, in the
  order the original recorded them. The image-quad scissor (written out three times) is
  `image_scissor` / `record_image`, the batch scissor `batch_scissor`, the partial-region clip
  `clip_to`. Verified by pixel A/B in two private shadow sessions, at scale 1 and scale 2, of
  release builds before and after: the demo through 13 scripted steps (hover, toggle, a wheel
  and a finger scroll on the slider, text focus, the dropdown opened and a theme picked — a full
  repaint — the Options dialog opened and cancelled), the renderer probe (nearly every prim,
  images, text, frosted plates), `frost_pair`, and `probe3d` raster and traced — identical to
  the pixel, every shot. (One scale-2 "before" run of the demo landed its slider wheel
  differently, an input-timing outlier; two further "before" runs matched "after" exactly.)
- **A style write outside a batch lands its change on the newest snapshot, not the copy it
  took.** `StyleCell::write` outside a `style::batch` returned a guard holding a clone of the
  field from the snapshot current at `write()`, and its drop installed that clone whole — so a
  write to the same field another thread published while the guard was held was reverted.
  Under the per-slot `RwLock`s the one-snapshot refactor (2026-10-08) replaced, a write guard
  held its slot's lock and same-slot read-modify-writes were serialised. The visible case was
  the registry: two setters on different keys lost one, and under `cfg(test)` a registry
  setter (which writes this thread's overlay and changes nothing) re-installed its stale copy
  of the whole registry over a load made meanwhile. Guards still never wait on each other —
  serialising them again would have meant a lock held across the caller's code, with the
  deadlocks that brings and an end to nesting writes on one field. Instead the guard keeps the
  snapshot it copied from, and its drop (`style::land`) publishes nothing when the copy is
  unchanged, installs it when the newest snapshot's field still equals the one it copied, and
  otherwise hands the change to the cell's merge: a plain value is a store (the last guard to
  drop wins, a serial order of stores); `StyleCell::merging` declares another, and
  `STYLE_REGISTRY` merges key by key (`StyleRegistry::merge`: a key the guard added, changed or
  removed relative to its copy is applied to the newest registry; every other key is left as
  the newest has it). `write` now needs `T: PartialEq` (every slot type has it; the registry
  derives it). Untouched: a batch still carries its fields whole onto the newest snapshot (a
  field both wrote ends with the batch's value), and a read-then-`style_write` pair, as the
  material setters in `color/materials.rs` do, is two guards, not one, and can still interleave.
  Three tests, each failing on the old guard every run:
  `a_write_keeps_what_another_thread_published_to_its_field_meanwhile` (a merging probe field
  and an unchanged scalar guard, sequenced with channels),
  `a_registry_setter_does_not_revert_a_load_meanwhile` (the real registry, the `cfg(test)`
  case above) and `writes_to_one_field_nest` (an outer guard merging over an inner one's
  publish, removals included). Verified: the three fail on every run against the old guard
  (3 of 3); fixed, 0 failures in 200 `cargo test --all-features --lib` runs and 30 full
  `cargo test --all-features` runs (`tests/style_registry_reentrancy.rs`, which hammers the
  registry's getters and setters across threads outside `cfg(test)`, among them); clippy
  clean.

- **`style::batch` publishes the fields it wrote, not the snapshot it began from.** A batch
  cloned the whole style when it began and published that clone whole when it ended, so every
  change another thread published while it ran was reverted — even in fields the batch never
  touched, and even by a batch that wrote nothing (`reload_config` with no config file, as
  `lazy_init_style_registry` runs it in every test). Under the per-slot locks this replaced, a
  reload never undid another slot's write; the one-snapshot refactor of 2026-10-08 brought the
  bug in. Each `StyleCell::write` inside a batch now records its field, and the batch's end
  publishes its snapshot whole only when nothing was published since it began; otherwise it
  carries the fields it wrote onto the newest snapshot (a field both wrote ends with the
  batch's value). It is what made two tests flaky: `style::tests::writes_publish_and_a_batch_publishes_once`
  (lost its writes at the post-batch read, line 250, or at the read after the first write,
  238, when a parallel test's reload spanned them) and
  `scene::material::tests::the_frost_block_is_the_only_spelling_of_the_default_recipe` (its
  colour reloads reverted by an unlocked test's `reload_config` batch). That style test now
  probes `Style::probe`, a test-only field nothing else writes, so a registry setter's
  re-published copy in a parallel test cannot touch it either; the new
  `a_batch_keeps_what_another_thread_published_meanwhile` sequences a write into another
  thread's open batch with channels and fails on the old `batch` every time. Verified: the
  unfixed crate failed 9 of 200 `cargo test --all-features --lib` runs (5 the style test,
  4 the frost test); fixed, 0 of 200, and 0 of 30 full `cargo test --all-features` runs;
  clippy clean.
- **The Vulkan renderer is a directory module.** `vk/renderer/`: `mod.rs` (`VkRenderer`, its
  `Frame`, the push-constant and window-info sizes, `Drop`), `init` (construction), `surface`
  (the swapchain: create, recreate, detach and attach, resize, surface loss, window info),
  `frame` (`draw_frame_2d`, uploads, text), `snapshot` (the blur snapshot and its mip chain, the
  backdrop and snapshot targets), `stage3d` (meshes, lit meshes, the tracer's scene, the
  `Stage3D` / `LitStage3D` impls), `pipeline` (`create_ui_pipeline`), `shaders`, `damage`,
  `images` and `buffers`. A pure move, done by script at item and method boundaries and checked
  line for line against the old file; `renderer/mod.rs` re-exports the helpers, so every
  `super::renderer::…` path in `vk` resolves as before. Clippy, the suite, the GPU tests
  (`vk::plate_probe`, `vk::compute`) and the path tracer's ignored tests on both tiers pass;
  `cargo check --workspace` builds. `draw_frame_2d` (653 lines) is unchanged: splitting it is a
  logic change to every app's frame path, for a live pixel A/B.
- **The tessellator's carve arm is a `Carve` and five methods.** The recess / boss / ridge /
  trough SDF arm (about 250 lines) is now 18: `Carve::of` reads the prim; `carve_host` decides
  grouping; `group_carve` appends the CSG feature; `overlay_carve` builds the overlay's cover
  quad and push; `warn_near_roll_fallback` (debug builds) and `note_carve_verdict`
  (`CCE_PLATE_DEBUG`, kept beside `carve_host` since it re-derives the same guards in order)
  report. `Carve` holds what three copies each of the containment test and the shaded-region box,
  two of the extended wall box and two of the kind name computed (`within`, `shaded`, `wall_box`,
  `kind`, `groupable`). The overlay's cover quad keeps its own expression, since computing it
  from the shaded box would round differently. Verified byte-identical against the golden, the
  probe dump and the `CCE_PLATE_DEBUG` output, and the near-roll warning, fired headless from the
  `trigger_near_roll_warning` example's scene, prints the same line before and after (that
  example's doc named `backend/window_runner.rs`; it is `backend/tessellate/debug.rs`).
- **The tessellator is a directory module, and `tessellate_display_list` is no longer one
  1,134-line loop.** `backend/tessellate/` is `mod.rs` (the types, and the frame's `Tess`: each
  item's prelude, a dispatch by prim family, and the batch tail), `flat`, `plates`, `carves`,
  `droplet` (the prim families, each keeping its SDF arm and its banded arm side by side),
  `shapes` and `bevel` (the vertex builders), and `debug` (`CCE_PLATE_DEBUG`, now a `PlateDbg`
  struct, and the near-roll warning). The arm bodies moved verbatim, their identifiers rewritten
  by script (skipping comments and string literals), with the four `continue`s that left the
  item becoming `return false` and the banded groove's inner-loop `continue` kept. The function's
  doc comment, which had drifted onto `prim_kind`, is back on it.
  Removed as unreachable — no caller in the crate or any workspace app, nine of them only
  re-exported from `engine`: `quad_vertices_clipped`, `rounded_rect_vertices`,
  `push_rounded_rect_vertices`, `plate_bevel_vertices`, `circle_border_vertices`,
  `arc_background_vertices`, `push_plate_solid_border_vertices_legacy`, and the extra-quad chain
  left from the legacy tuple views (`extra_quad_vertices`, `extra_quad_vertices_clipped`,
  `push_extra_quad_vertices`, `push_extra_quad_vertices_clipped`, `get_child_widget_for_quad`),
  319 lines; with them the tessellator no longer imports the widget traits. Verified against
  three baselines from the commit before: `tests/plate_golden.rs` (145k lines, both shading paths
  at two scales), the renderer probe's scene tessellated and dumped (62k lines, likewise), and
  the `CCE_PLATE_DEBUG` output of both — each byte-identical; `cargo check --workspace` builds.
- **The params pane's `fields` is a loop over `row_field(i)`**, one `Option`-returning arm per
  row type (textpick, button, choice, spinbox, toggle/checkbox). A control's band — its rect
  below the label strip — and the wall depth carved into it (the roll width capped at a fifth of
  the band) are `control_band` and `wall_depth`, used by `fields` and `push_row_relief`. Verified
  by dumping `fields`, `reliefs` and `plain_quads` from a pane of every row type (relief on and
  off, full and scrolled) on the previous commit and on this one: byte-identical, every field
  kind present.
- **The params pane's row press dispatch is flat.** `press_row_controls` is a loop over
  `press_row_control(i, …)`, one early-returning arm per row type, with a text row's picker-then-
  box handling in `press_text_row`. Two rules written out several times are functions:
  `hold_focus` (the row holds the param focus while its control is open or editing, four sites)
  and `fill_from_pick` (a textpick pick fills the row's text box, three sites: the open popover,
  the press and the key path). Verified by replaying 2,074 events (press/release pairs on a grid
  over a pane of every pressable row type, with ArrowDown + Enter between, at two heights) on the
  previous commit and on this one, recording the return, the focused row, every value, the open
  dropdowns and the editing fields after each: byte-identical.
- **The params pane's wheel handler is staged**: `wheel_owner` (the latched control, else the
  nearest value control under the pointer, unless the pane holds the gesture), `wheel_control`
  (that row's control takes the event and its value is written back) and `wheel_pane` (the
  pane's own scroll, and swallowing every wheel over the opaque pane). Removed as dead: the
  `finger` test and `pane_takes` — the row loop acted only on the owner row, so with no owner it
  did nothing whatever `finger` said, and with an owner `pane_takes` reduced to `pane_owns`, which
  cannot hold beside one (a latch names one id). Also gone: the comment block describing the
  2026-09-21 "a finger gesture is the pane's from anywhere" rule, superseded on 09-28. Verified
  by replaying 4,932 wheel events (notches and finger scrolls on a grid over a pane of every
  value-row type, gestures beginning, continuing and drifting, with and without overflow) on
  the previous commit and on this one, recording the return, every row's value, the scroll and
  the gesture's owner after each: byte-identical.
- **The params pane's flat views are split.** `plain_quads` is `push_section_outline_quads`,
  `push_row_quads` per row, and a viewport clip; a code row's chrome (box, gutter, error band,
  selection, caret, border) is `push_code_row_quads` in `code.rs`. `reliefs` is
  `push_section_reliefs` (the section well's edge-suppressed pieces, unchanged) and
  `push_row_relief`, which drops a `(&dyn WidgetHost, radius, raised)` plumbing only text rows
  ever filled (always with `raised` false) for a plain dispatch: text well, spinbox well without
  its run, colour well. The tuple is named `Relief`. Verified by dumping both views from a pane
  with every row type — sections open and collapsed, a focused code row with a selection and an
  error line, relief on and off, full and scrolled viewport — on the previous commit and on
  this one: byte-identical.
- **The params pane's press and key handling are staged.** `on_mouse_button` offers an event to
  `press_scrollbar`, `press_section_title`, `press_open_popovers`, `press_row_controls` and
  `focus_on_press` in turn; `on_key` sends a code row's keys to `code_key` and the rest to
  `row_key`. The code editor's key behaviour is `code_editor_key` in `code.rs`, a function of the
  editor and its history alone, and a code-row click is `place_code_caret`. The scrollbar thumb's
  arithmetic, written out five times (press, pointer drag, `drag_update`, `scrollbar_quads`,
  `hit_test_scrollbar`), is one `Thumb` (`ParametersBg::thumb`, `drag_thumb_to`). Stage bodies
  were lifted unchanged; the focus-on-press loop became index-based to call `place_code_caret`.
- **`ParametersBg` is a directory module split by concern** (`widget/container/parameters_bg/`:
  `mod.rs`, `rows`, `geometry`, `paint`, `input`, `code`, `controller`, `tests`), where it was one
  5.1k-line file. A pure move, done by script at method boundaries and checked line for line
  (every line of the old file is in exactly one new file); moved private items are `pub(super)`,
  and the pane-wide associated constants sit in `mod.rs`. `on_event`'s 930-line match is now one
  handler per event kind (`on_pointer_move`, `on_mouse_button`, `on_key`, `on_wheel`), each arm's
  body unchanged. The module doc, which still described a raw-pointer child list and a
  `window_runner` downcast, was rewritten. Tests, clippy, and cce-designer / cce-files builds pass.
- **Docs: `CLAUDE.md` is a reference; history moved here.** The 200 KB file (≈50k tokens loaded by
  every session in this crate, mostly dated narrative) became a short reference plus four topic
  docs (`docs/runtime.md`, `surfaces.md`, `widgets.md`, `platforms.md`). `tests/doc_claims.rs`
  checks the topic docs' test citations and source paths as well as `CLAUDE.md`'s.
- **Accessibility paused.** Work stops after the items below; the cheap rules (focus roles,
  action IDs, strings in the `.ftl`) stay in force.
- **A text field is text to a screen reader**: `Input::a11y_text` (`A11yText`: text runs and the
  caret; a password as bullets) and `Input::a11y_set_text` (AccessKit `SetTextContents`) on
  `TextBox` and `ColorSelector`; a field an app draws (`LineEdit`) is `AppNodes::text_field`, and a
  reader's edit arrives as `Application::accessibility_action` (`AppAction`, including
  `ShowContextMenu`). Found: a node with a role description becomes AT-SPI `Extended` and never
  registers on the bus — never set one.
- **The registry owns its widgets; the pointer path is gone** (owning-registry RFC phase 5).
  Deleted `Owned`, `register_host`, `register_widget`, `set_focused[_ptr]`, `focus_widget` /
  `unfocus_widget`, `register_popover`, `link_parent_child`, `Liveness`, `stable_target`, and the
  container-children raw-pointer channel. `show_context_menu` takes an id; a widget asking for the
  menu mid-event has it opened by the adapter afterwards. Shadow A/B of the data editor, cce-files,
  the gallery and the designer: identical to the pixel.

## 2026-10-08

- **Owning registry, phases 1–4.** `UiContext::insert` → `Handle<W>`, `get` / `get_mut` /
  `remove` / `lend` / `lend_h`; every call into a widget lent. The demo, then every app (18 crates,
  each pixel-A/B'd in a shadow), then the toolkit's embedded children (`widget::Embedded`:
  Paginator, TreeList; Ramp fields never inserted; the designer dialog's dropdown). Added
  `focus_id` / `unfocus_id`, `Handle::none`, `Group::widget_w_h`; `insert` registers tick
  receivers (an inserted tree list never filtered without it). Retired a real bug: cce-files'
  prompt focused a stack-local `TextBox` by pointer then moved it, and a second prompt corrupted
  the heap.
- **The registry's pointer aliasing was made sound and checked by Miri** (superseded the same
  day by the owning registry): `Owned` held its widget in a `Box`, so every app reborrow
  invalidated the registry's pointer (UB every frame in every app); registrations from inside a
  widget replaced the root; a widget focused itself through the registry mid-event
  (`claim_focus` added).
- **A section's contents are a `Form` on `scene::layout`**: settings pages declare widgets, text,
  rows, rules and fills; the cursor (`VStack`, `add_row`, a hidden 1–2 column grid keyed on type
  names) and pages' own insets went. `list_gap` (the rung inside a list row) and `lay_row` added;
  the settings app's lists and process table use them.
- **`PaintCtx::append_items`** keeps a walked widget's clips under the host's.
- **`WidgetHost` 57 → 31 methods.** What the widget model answers moved to `WidgetHostExt`
  (blanket-implemented); the host hands out `layout_model` / `paint_model` / `input_model`.
- **The legacy tuple views are gone**: `extra_quads`, `extra_arcs`, `extra_circles`, `all_quads`,
  `all_rounded_quads`, `highlight_quad`, host `corner_style`, and the model hooks that served only
  them; the painter's `paint_legacy_leaf` and the tessellator's `widget_vertices`. The designer,
  gallery, greeter, settings and cce-secrets moved, each pixel-A/B'd. Composites read children's
  `painted_prims`. `append_widget_plate` is the plate alone (it drew arcs a host then drew twice).
  `plate_bevel` removed (never overridden).
- **Another program's config keys never become the toolkit's.** The flatten cut unmapped paths to
  their bare keys, so the compositor's `layout { grid_gap 18 }` (window-tiling gap) arrived as the
  toolkit's `grid_gap`. Now only mapped paths become toolkit keys.
- **A layout style key has one home, the registry.** About fifty keys also had a slot filled by a
  second prefix scan (`button_height` matched longer keys), and thirty-five getters re-read and
  re-parsed `config.kdl` on first use, after an app's setter — so cce-system-interface's
  `set_grid_gap(root_plate_gap())` lost to config. A `(mm)` length never reached a slot. Font
  parses are cached against their string. Also: an overflowing right-to-left line shows its start.
- **A layer app keeps its renderer across an empty spell** (`wants_surface`): detach/attach instead
  of a new device and pipelines (~45 ms before the first card after an empty spell); the
  `surface_hidden` hook went. Checked pixel-identical in a scale-2 shadow on a private bus.
  Unused widgets removed; accessible names for widgets.
- **The toolkit names no app.** The runner no longer reads the app id (apps beginning
  `cce-status` lost CSD and had their input region pinned); the status bar says so with
  `Application::standard_csd`, and the input region went. `SplitterLayout` /
  `CircularPaneLayout` moved into cce-designer; the relief editors `cce-relief` and `cce-ramp`
  became the `cce-relief` crate (pixel-identical).
- **`layout/legacy.rs` retired; a page's sections are drawn once.** Every section was drawn twice
  (once to measure) through `LayoutStrategy`; a section's position never depended on its own
  height. `LayoutStrategy`, `ColumnLayout`, `AdaptiveGrid`, `FlexLayout`, `RadialLayout`,
  `Column`, `Row`, `Section`, `UiFrame`, `Radial` removed. All 14 settings pages pixel-identical.
- **Global-state RFC phases 1–4**: one keyboard-focus store (`UiContext::focused_widget`); a
  window owns its interaction state (`WindowState`, `window_state::enter`); the style is one
  snapshot (`StyleCell`, `style::batch`; ≈200 `RwLock`s gone); a window's properties are its own
  (scale, metric, app id, fullscreen, maximized, vertical text, scroll phase — the scroll phase
  was one process-wide atomic that tests raced on).
- **CI**: clippy with `-D warnings` (crate made clippy-clean; five lints allowed as house style);
  a `wasm` job (its first run found the browser build broken since 10-06 by two native-only calls
  in portable code); a `macos` job; Miri for the registry.
- **Accessibility RFC phases 0–5**: font systems built with the user's locale (were `en-US`); the
  accessibility tree in AccessKit's schema (menus included, rows keyed by action, a hook for apps
  without widgets); AT-SPI publishing behind the `a11y` feature (proven on cce-data-editor); the
  Tab walk on by default (`plate_navigation`), a modal `Dialog` and a `RadioGroup`;
  right-to-left and proportional text edited where drawn (carets, clicks, selections, RTL
  paragraphs set right, bidi runs in the `DocEditor`, wrap by shaped width); translatable toolkit
  strings (`crate::l10n`, `locale/en-US/cce-ui.ftl`).
- **`Application::create` is required**; the legacy `new(qh, sender)` and the default that
  panicked at startup when an app implemented neither are gone.
- **`tests/doc_claims.rs` checks the names `CLAUDE.md` cites** (tests and source paths), after an
  audit found a renamed test, a constant that never existed, `layout.rs` called the largest file
  after it became a directory, and claims about methods that did not exist.

## 2026-10-07

- **`widget::Owned<W>`**: app widgets in a heap allocation of their own, so a moved owner never
  moves the registered widget (replaced by handles on 10-08/09).
- **The GUI-free modules moved to `cce-core`** (`config`, `input`, `motion`, `units`,
  `relief_spec`, `ipc`, spec parsers), re-exported at their old paths. The compositor, cce-browser-open
  and cce-window-manager now depend on `cce-core` alone. `DropletSpec::finish()` became the
  extension trait `scene::paint::DropletFinish`.
- **`layout.rs` (7.4k lines) and `color.rs` split into directories**; vertical text left the crate
  root (`backend::text::set_vertical_text`).
- **A mesh update no longer waits for the GPU** (`device_wait_idle` before each update made every
  frame of a playing simulation wait out the previous frame's GPU work); spare buffers tagged by
  submitted frame. Designer replay at 57k points: stage pass 5.6–6.0 ms → 3.6–3.8.
- **Tests never read the machine's config.** Six heightfield and material tests failed on the
  desktop (its `relief edge height` and frost keys overrode what they set) and passed elsewhere.
- **Screen-space draws are a flag** (`SceneDraw::screen_space`). The shader had treated any vertex
  within 0.01 of z = 9.99 as a background corner, tearing real geometry at that plane (cce-model's
  25 mm STL sphere) into spikes. Regression in the 3D probe.
- **Lit and textured meshes** (`draw::lit`, Vulkan): PBR-ish shading for model files; cce-model
  is the consumer. `probe3d` pixel-identical before and after (the flat path untouched).
- **Large traced scenes are prepared off the UI thread** (`PreparedRtScene`): 5M triangles froze
  cce-model for 2.3 s building the BVH inside `stage_3d`.
- **Spreadsheet**: columns of values written as painted (the designer formatted every cell of
  10k rows into `String`s each playback frame); columns as wide as their content (were even
  shares floored at 76 px).
- **CI installs `fonts-liberation`** as `CCE_FONTS_DIR`: with no faces, tests needing a second
  face or a fallback glyph kept the workflow red from 10-07 to 10-08.

## 2026-10-06

- **The runner derives each frame's damage** (`derive_damage`) instead of repainting the whole
  window; a frosted batch grows the region rather than forcing a full frame. Checked by pixel A/B
  (full vs derived) on the demo, cce-gallery and the settings app.
- **`Prim::Frame`**, an inside-out plate whose hole has coves; the designer's playbar.
- **The blur reads a mip chain**: stride-apart taps at level 0 drew faint horizontal bands (an
  8–16 level sawtooth) under a dropdown over the params rows; now a smooth ramp.
- **Instanced scene draws** (`SceneDraw::instances`): the designer's point markers were a
  240-vertex sphere copied per point — 46 MB at 10k points, 240 KB instanced. Vulkan verified;
  the WebGPU half written but not run.
- **Every scrollbar rides a centre line and sinks behind the plate**; the spreadsheet's cross
  first (were 6 px always-on strips at the right and bottom).
- **Graph**: wires run across on the first lattice line below the source (halfway, they ran
  through nodes in the source's column); swap-on-drop; a snapped drag stands only where it
  could land; wires may be thinner than a pixel (were clamped to one logical px).
- **`ParametersBg` paints its controls' glyphs**: after symbols became glyphs, every dropdown,
  picker and spinbox in the pane lost its arrow or −/+.

## 2026-10-05

- **An `Application` runs in a page** (`web::run`, the browser shell), on the same `Driver` and
  `Pacer`. The 24-step demo replay matches native except 1 px at the slider band's tips.
- **Browser clipboard** through the page's clipboard events (a copy in a page had panicked on
  `std::thread::spawn`); **the keyboard sink** (a hidden `<textarea>`) for input methods.
- **Compute jobs run in the browser** (`web::ComputeDevice`); the job rules moved to the portable
  `crate::compute`. Native and browser outputs identical to the bit.
- **3D scenes and the path tracer in the browser** (`Stage3D`, `web/scene.rs`, `web/rt.rs`).
- **The macOS shell** (AppKit over MoltenVK), type-checked only.
- **Input-method composition is one model for every shell** (`crate::ime`), shown by `TextBox`,
  `LineEdit` and `DocEditor` without ever being held; **`text-input-v3` on Wayland**, verified
  end to end in a headless sway with a scriptable input-method client; cce-notes driven through
  it, and its note on disk never held a composition.
- **Touch**: `wl_touch` bound; tap, scroll and hold from the first finger. Binding it also moves a
  cce-ui window off the compositor's emulated-pointer route, where a finger drag selected rather
  than scrolled.
- **A field being edited claims it every frame** (`text_input::claim`) for the on-screen
  keyboard; apps that replay cached geometry replay the claim; a press re-announces an open field.
- **Layer apps can drop their surface while empty** (`wants_surface`) — at first by dropping the
  renderer (kept since 10-08).
- **Every symbol is a cce-icons glyph** (`PaintCtx::icon`): dropdown arrows, menu marks and
  chevrons, spreadsheet sort marks, spinbox −/+, font selector, breadcrumb overflow, markdown
  embeds. The menu popup got images of its own and stopped draining the shared upload queue
  (uploads queued between the window's frame and the popup's had landed in the popup's table).
  A flat host now receives glyphs, strokes, arcs and discs (cce-files' Graph page had shown no
  wires).
- **A checked check box is a square plate in its well** (bae3712); the toggle's half-width run in
  a square box read as the box narrowing.

## 2026-10-04

- **The library builds for the browser**, and **draws there** (`WebRenderer`), from the same
  `Frame2D`, shaders and atlas. What a renderer draws moved from `vk/` into `draw/`, re-exported
  at the old paths. The renderer probe: lavapipe vs SwiftShader, 96.8% of channels equal, every
  other within 2 levels but one at 3.

## 2026-10-03

- **`Application::create(sender: AppSender)`**, naming no window system.
- **The runner split** into `app.rs`, `driver.rs`, `frame.rs`, `shell.rs` (`Shell`, `Pacer`) and
  the Wayland `window_runner.rs`, so other shells can share everything else.

## 2026-10-02

- **A field is one object** (`scene::paint::Field`, `PaintCtx::field`), asked for by its form;
  the toggle became a field whose run glides (it was a recess with a raised boss — the one
  control whose plate stood above the surface); the check box and inline checks became fields
  (inline checks had been a ring with a blue dot).
- **Every flush control wears the run's edge**: `inset_plate` draws a field that is all run
  instead of a `Prim::Trough` (whose outer half was a compressed step).
- **The run's face is inset on every side**, and the run is laid out a wall wider than its face
  (for a few hours the seam was a mirrored ridge, which read as a strip of new surface); picker
  arrows line up with every dropdown's.
- **A grouped carve shades as its overlay does.** Grouped recesses drew a doubled outline: the
  shade line took the carves' slope and fired twice per wall, and diffuse and curvature were
  multiplied onto a dark face while the glint was added whole. 5,462 px apart before, 0 after.
  It was never the GPU: grouping is decided per frame, and inside a shadow the NVIDIA ICD failed to
  load without an X display, so `CCE_VK_DEVICE=discrete` fell back to Intel silently — an unmet
  device preference is now printed to stderr. A plain text box briefly became an all-well field
  to dodge it (`well_field`), reverted once fixed.
- **`vk::plate_probe`**: 2D looks tested by rendering through the live pipeline.
- **Menu rows can lead to pages; a side swipe turns them**, with an animated turn. Submenus
  (flyouts, 09-29 to 10-02) went: the designer had flyouts on some rows and plate swaps on
  others, two gestures for one idea.

## 2026-10-01

- **`Prim::Field`**: a sunken well ending in a flush run, one outline (two prims side by side
  turned their own corners at the seam).
- **Parameter pane**: `float2` / `float4` rows; separator rows; soft slider ranges.
- **The keyboard can walk a menu** (`set_hovered_item`, `step_hovered`).
- **`Dropdown::is_expanded`**; **a dropdown's text names its font** (a stamped dropdown took its
  painter's font and its menu opened in another).
- **Markdown features**: `MarkdownView` and `DocEditor` with live preview, for the note clients.

## 2026-09-30

- **Graph wires are strokes in a style** (orthogonal, rounded, bezier, straight); `wire_color` /
  `wire_size` are read (parsed long before and read by nothing — wires were hard-coded cyan 3 px).
- **A node has as many wires as the host says** (it drew one, the parameter named `input`).
- **Value controls read the wheel as "up is more"** (`value_notches_y`); each control had read
  `notches_y` with a sign of its own.
- **Scroll phase per thread in tests**; `ScrollMotion::apply_phase`;
  `scroll_motion::force_scroll_settings`.

## 2026-09-29

- **A flick coasts with animations off** (the switch had stopped both glide and coast, so on
  battery a two-finger scroll stopped dead at the lift in every pane).
- **Spreadsheet row selection.**

## 2026-09-28

- **Frost is one block** (`plate { frost radius compression refraction }`); the four keys it
  replaced are retired and reported. `Frost::Opaque` renamed `Unfrosted` (it never meant solid).
  The pane tint got a clear spelling (`plate.pane.color`).
- **The relief is two shapes**, `wall` and `edge`, as nodes under `relief`; `light` replaced
  `depth` (it was never a length); the flat and prefixed spellings retired.
- **The editor's knobs left the style** for cce-relief's own state file.
- **One roll width**: the widget-plate path had its own (`plate.bevel_width`, 6 vs the relief's
  9.3), so two pane plates in a window rolled differently by painter.
- **cce-relief writes heights in the config's own unit** (a headless session had turned
  `(mm)0.3` into `(px)1.1339`).
- **The params pane stacks its labels by track width** (the first cut measured the rect and let
  a slider's track shrink to 52 px); **a scroll gesture belongs to the control it begins on**
  (from 09-21 every finger gesture was the pane's, leaving sliders a trackpad could not turn).
- **The suite's animations switch is its own** (the spreadsheet glide and scroll-region fade
  tests passed on mains and failed on battery).
- **A colour test that reloads a knob reloads its default back** (`frost_from_style_and_flag`
  failed one run in eight).

## 2026-09-25

- **The context menu draws in its own popup** (cut off at the window edge before). A popup path
  had been deleted in July for drawing in one place and hit-testing in another; the configure
  write-back and the popup's own pointer input are what make this one correct.
- **A lost surface ends the session; it does not panic** (`VkRenderer::try_new`, the only
  constructor). Found as cce-cloud's daemon panicking at logout.
- **`Application::outlives_compositor`**: the status bar's modules exited at every logout and the
  tray's watcher came back seconds after the next login.

## 2026-09-20

- **Materials** (rfc-material steps 1–4): `scene::Material`, threaded through every rung;
  per-plate frost; named materials in config, edited by cce-relief.
- **`backdrop_compression`**, measured in a 24-cell sweep (in `docs/surfaces.md`); **refraction**
  at the rim — exempting the clear rim from compression made it 5.9× stronger at the same setting.

## 2026-09-19

- **The reconnect sweep**: image ids cached across a reconnect drew nothing in many clients; the
  `renderer_init` pattern and its three traps (`docs/runtime.md`).
- **The hex gamma trap** cost a measurement round: a compression sweep meant to hold the tint
  constant swept it too (`#1a1a24` is ten times darker than `PARAM_BG`).
- **Size claims are tested**: every `(~N lines)` figure in every CLAUDE.md was stale, the worst by
  49%.

## 2026-09-11

- **The runner sleeps when idle** instead of ticking a flat 16 ms forever (every client was awake
  60×/s doing nothing).
