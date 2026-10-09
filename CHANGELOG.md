# Changelog

What changed in cce-ui, when, and why — with how it was verified where that mattered. Newest
first. The current behaviour is described in `CLAUDE.md` and `docs/*.md`; this file is the
history behind it. Work before 2026-09-11 is recorded in `docs/rfc-core-rebuild.md` (phases 0–6)
and `docs/rfc-material.md`, and in `git log`.

When you change behaviour: update the reference to the new state, and add an entry here under
today's date — what changed, why, and how it was checked.

## 2026-10-09

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
