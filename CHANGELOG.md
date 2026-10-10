# Changelog

What changed in cce-ui, when, and why — with how it was verified where that mattered. Newest
first. The current behaviour is described in `CLAUDE.md` and `docs/*.md`; this file is the
history behind it. Work before 2026-09-11 is recorded in `docs/rfc-core-rebuild.md` (phases 0–6)
and `docs/rfc-material.md`, and in `git log`.

When you change behaviour: update the reference to the new state, and add an entry here under
today's date — what changed, why, and how it was checked.

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
