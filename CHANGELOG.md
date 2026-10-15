# Changelog

What changed in cce-ui, when, and why — with how it was verified where that mattered. Newest
first. The current behaviour is described in `CLAUDE.md` and `docs/*.md`; this file is the
history behind it. Work before 2026-09-11 is recorded in `docs/rfc-core-rebuild.md` (phases 0–6)
and `docs/rfc-material.md`, and in `git log`.

When you change behaviour: update the reference to the new state, and add an entry here under
today's date — what changed, why, and how it was checked.

## 2026-10-15

- **A registry entry with no display is found only by a caller with none.** It was a stopgap
  while the live apps were built before `Entry::display` existed; with all of them rebuilt
  (designer last, 2026-10-15), a display-less entry can only be a stale app, typically one left
  running in a shadow, and it must not answer a live popup. Unit-tested (a `wayland-0` caller no
  longer finds a display-less entry; a caller with no display still does).
- **Ramp key pegs are disc plates of frosted accent glass.** Each peg was a flat translucent
  circle with a rim of ~35 `ArcShaded` sub-arcs lit by hand from the light azimuth, and
  overlapping pegs were cut into foam cells (strips, shared walls, merged rim spans); none of it
  read `control_relief`. Under relief a peg is now `plate_shaped(.., Some(2.0))` — a true circle
  at any `corner_shape` — of `highlight_primary` at 0.25 (selected 0.85) frosted with
  `Frost::from_style`, at a control plate's depth (`bevel_width`, at most 0.4 r), lit per pixel
  by the DE light. Frosted rather than a plain translucent plate: an unfrosted plate at 0.25
  shows its roll at 0.25 and the relief all but vanishes, where a frosted one composites opaque
  with the full roll. Overlapping pegs stack, the selected one drawn last, on top. Flat, a peg is
  a flat accent circle and the selected one wears a `ControlPlate::focus_tint` ring at its outer
  edge. Press and hover-scroll latch share one `key_at`, topmost first (selected, then reverse
  index), each peg within the radius it is drawn to (`peg_drawn_r`): the disc under relief
  (`key_peg_r`, the old ring + rim extent, so sizes are unchanged); flat, the selected peg's ring
  and an unselected peg's face, 3 px inside it. Nearest-centre matched the foam walls, not
  stacked plates; one radius for every peg let a press on one flat face take the neighbour
  beside it. `rolled_rim_arc` and the foam code are gone; `Prim::ArcShaded` stays, with no
  widget user. Checked:
  `ramp_pegs_under_relief_are_frosted_disc_plates_the_selected_on_top`,
  `ramp_pegs_flat_are_circles_the_selected_ringed_last`,
  `a_press_where_ramp_pegs_overlap_takes_the_topmost`,
  `a_press_on_a_flat_ramp_peg_beside_its_neighbour_takes_that_peg`, each failing with its part
  reverted (unfrosted, circles, selected not last, bottom-first hit, one hit radius); shadow
  shots of the gallery's Ramp in Relief and Flat. Under the live config's frost recipe (radius
  1.5, no compression or refraction) the glass is nearly clear and the curve shows through
  sharp; at the default radius it reads frosted. The gutter axis numbers draw over the pegs
  (text is drawn after all geometry), so the left `0.50` sits on the Raised preset's first key.
- **ColorRamp declares its height and looks like the Ramp.** It declared nothing, and placed its
  sliders at a fixed offset whatever its rect, so they spilled out of a short one. It now
  declares `natural_height` (`intrinsic_size`, width 0): the track, the peg row and the three
  slider rows, reserved whether or not a key is selected, so selecting never changes its rect;
  `arrange_fields` places the slider block inside it. No container box (the 1px border quads and
  the `ramp_background_color` fill are gone, `Paint::color` is transparent): the gradient track
  is a canvas well (`well_floor` / `well_rim` under `control_relief`) at the control radius —
  it is `ramp_height` tall, and the panel radius would make it a pill — with the gradient
  clipped to the well's rounded shape. Pegs are disc plates of the key's own colour, opaque
  (the face is the key's data, so selection does not tint it); the selected one is drawn last
  and ringed in `ControlPlate::focus_tint` outside its face. Flat, a peg is a circle in a
  `well_frame_color` hairline a pixel wider (a key the colour of the plate still shows), ring the
  same. Hit test topmost first, each peg within the radius it is drawn to: the selected one's
  ring, an unselected face (its hairline, flat) — at the ring's radius for every peg, a press on
  one face took a neighbour 14-18 px away whose face it was not on. Checked:
  `a_color_ramp_declares_its_slider_rows_and_holds_them`,
  `color_ramp_pegs_under_relief_are_opaque_disc_plates_the_selected_on_top`,
  `color_ramp_pegs_flat_are_circles_in_a_hairline`,
  `a_press_where_color_ramp_pegs_overlap_takes_the_topmost`,
  `a_press_on_a_color_ramp_peg_beside_its_neighbour_takes_that_peg`, each failing with its part
  reverted (the flat look back to the old black ring, one hit radius); the existing ColorRamp
  tests unchanged in intent (selecting adds one prim, now the ring arc); shadow shots of the
  gallery's Color Ramp Editor in Relief and Flat with a key selected. With no box left to fill,
  `color::ramp_background_color` / `ramp_border_color` and their style slots are removed, and
  `style.control.ramp.background` / `border_color` are no longer read (a note at the loader says
  so; `ramp height` still is). No crate in the workspace called either getter.

- **The MCP registry is scoped by Wayland display.** A shadow compositor shares the live
  session's `$XDG_RUNTIME_DIR`, so a cce-claude popup run in a shadow reached the user's LIVE
  notes and calendar as companions (seen 2026-10-15; nothing was sent). `registry::Entry` now
  records `display` (`$WAYLAND_DISPLAY`), and `registry::find` takes the caller's display and
  never returns an entry on another; an entry from an older build, with no display, still
  matches until its app is rebuilt. Unit-tested (a `wayland-1` caller does not see a
  `wayland-0` entry).
- **`McpServer::unclassed` / `class_of`**, so an app's test can assert every tool it serves has a
  class (an unclassed tool makes the cce-claude popup ask before every call). Unit-tested.
- **MCP servers say where they are, what each tool does, and how to use the app** — step M1 of
  the in-app Claude popup (a shortcut opens a Claude prompt scoped to the focused app; design doc
  "In-app Claude popup"). New `mcp::McpServer` builder: it binds a port the OS picks, writes
  its URL and a fresh 32-byte bearer token to `mcp::registry`
  (`$XDG_RUNTIME_DIR/cce/mcp/<app_id>-<pid>.json`, dir 0700, file 0600, removed at a normal
  exit), and refuses requests without that token (401). Before this, every server sat on a fixed
  port, so a second instance could not serve and a client could not tell which instance owned
  the focused window. A registry entry records the process start time too: a file left by a
  killed process is swept by the next reader even if its pid has been reused. `classify` marks
  tools `Read` / `Edit` / `Outward`, sent as the spec's `annotations` hints, so a client can run
  reads unasked and ask before anything irreversible; `instructions` is sent in `initialize`;
  `context_tool()` and `Context` are the standard "what is the user looking at" tool and answer
  (always classed `Read`). `start_mcp_server(name, port, ..)` is unchanged for its callers: it
  is `McpServer::new(..).fixed_port(port)`, which still serves tokenless requests for clients
  configured by hand in `~/.claude.json`, now files itself in the registry as well, and refuses
  a request carrying a WRONG token. Checked by unit tests (21 in `mcp::`, including a started
  server on port 0 filing itself in a scratch registry and admitting only its token), clippy,
  and `cargo check` of the five apps that serve MCP (designer, notes, calendar, spreadsheet,
  browser), whose code did not change.

- **ColorRamp: a drag keeps its key, arrows recolour it, sliders whole, fields painted once.**
  From an audit of the ramp widgets. Dragging a key onto a neighbour's position re-sorted the keys
  and found the dragged one again by position, so the selection jumped to the neighbour (and the
  sort panicked on a NaN); keys now resettle by adjacent swaps (`ColorRamp::resettle_key`, the
  float Ramp's). Arrow keys moved a focused R/G/B slider but not the key's colour; the colour now
  follows (`apply_sliders_to_selected`). The sliders sat in fixed 20 px rects, about 18 of them
  label, 25 px apart: each row is now the label strip over a `slider_height()` slider, 8 px apart,
  and the delete button `button_height()` tall (one `arrange_fields`, which `rect_assigned` had
  duplicated). The fields' quads were added to the ramp's quad list and painted again by
  `paint_self`, so each was drawn twice. Checked: `a_color_ramp_drag_onto_a_neighbour_keeps_its_key`,
  `a_color_ramp_slider_moved_from_the_keyboard_moves_the_key`, `a_color_ramp_lays_its_sliders_out_whole`,
  `a_color_ramp_paints_its_fields_once`, each failing with its bug put back; a shadow shot of the
  gallery's Color Ramp Editor with a key selected.
- **ColorRamp: a release is its own only when it used it; Enter on Delete deletes.** The fixes
  the float Ramp had (2026-10-14). With a key selected it claimed every left release, so a host's
  next control never got its click; it claims one only when a key drag, a slider or the delete
  button used it, and tells every field regardless of the selection. Enter/Space on the focused
  delete button set its click and nothing read it, so the next mouse press on the button deleted;
  the click is read at once, through one `delete_selected`. Checked:
  `a_color_ramp_release_is_its_own_only_when_it_used_it`, `a_color_ramp_deletes_from_the_keyboard`,
  each failing with its bug put back.
- **The Ramp's graph is a canvas well, in the theme's colours.** It drew its own: a near-black
  floor, three contact-shadow rings and a recess carved whatever the relief setting, so in Flat
  style it still wore a bevel and ignored frosted materials. It now takes the well every canvas
  shares (`well_floor` / `well_rim`, as Slider2D, Trackpad and the previews do). The curve, its
  under-fill and the key pegs are the DE accent (`highlight_primary_color`), the grid and axis
  numbers the detached label colour, the numbers in the detached label font; `Paint::color` is
  transparent (it was an unused grey). The under-fill dropped from 0.10 to 0.05 of the accent: on
  the shared floor, lighter than the old near black, 0.10 read as a solid block. Checked in shadow
  shots of the gallery's Ramp, Relief and Flat.

## 2026-10-14

- **Ramp fixes, and a ramp that declares the height it needs.** From an audit of the widget,
  each with a test that fails without it. Opening the Preset dropdown re-applied the preset it
  still named and wiped a hand-edited curve — `Dropdown::tick` reports its open/close animation,
  not a pick; a pick is now its change flag. With a key selected the ramp claimed every left
  release, so a parameter pane's next row never got its click; it claims one only when a key
  drag, the key pad or the delete button used it. Enter on the delete button did nothing, then
  deleted on the next mouse press; it deletes at once (`delete_selected`). A key grabbed inside a
  neighbour's resistance zone jumped away from it on the first nudge toward it, and could land
  just outside 0..1: the zone starts no earlier than the grab (`DragGrab::origin`), the result is
  clamped, and a key keeps where in its ring it was taken. The 26 px key rings hung past the
  graph's walls (the plot is inset 22): they are capped inside. The ramp declared 150, below its
  ~180 px control strip alone, so its graph sank to 30 px: `natural_height` is a 120 px graph over
  the strip, and the parameter pane sizes a ramp row from it, not from a fixed 260 that left the
  plot ~20 px. `parse_ramp_spec` (cce-core) refuses non-finite numbers, which panicked its sort.
  Checked: the new `ramp::tests` and `a_ramp_row_gives_its_ramp_its_natural_height`; shadow shots
  of the gallery's Ramp.
- **Panels share a corner radius**, `style.control.panel.corner_radius` (`panel_corner_radius`,
  24 unset): Group frames, InfoBoxes, TreeLists (under `style.data.tree.corner_radius`), a
  multiline TextBox, the Ramp's graph and the canvas wells (Slider2D, Trackpad, the bevel and
  ramp previews). Groups and InfoBoxes took the window's radius, so reshaping windows reshaped
  them, and the wells and trees took the control's 8: two families where the user saw one. A
  one-line TextBox keeps `textbox_corner_radius`. Checked in shadow shots of the gallery, corners
  zoomed before and after.
- **A TreeList's type column fits its labels**, and `value_x()` says where a row's value starts.
  The columns were fixed offsets (type 180–235), so "(string)" drew as "(string". The type column
  is now as wide as "(keybind)" in the tree font, never under 55; every separator, label, swatch
  and button reads it, and cce-data-editor places its inline editors at `value_x()`. Checked in
  shadow shots of the gallery's tree and cce-data-editor with a row selected.
- **Slider2D is a 120 px pad by default**, the height of the other canvases; at 64 it sat small in
  a control column. (The Ramp's key pad sizes itself.)
- **An InteractiveListItem's hover and press are the theme's overlays.** It painted
  `surface_bg` — the popover colour, opaque and near black — with the overlay's alpha added, so
  hovering a row blacked it out. Checked: a shadow shot of the gallery row under the pointer.

- **A grid app's menu opens on the output it was asked for.** The menu layer named no output,
  so the compositor put every one on its first output — with a second display, a right-click
  there opened its menu on the wrong one. `context_menu::set_output(name)` (after `show`; `show`
  clears it, a page turn keeps it) names the `wl_output`, and `sync_menu_layer` creates the
  surface on it, fits the menu to its logical size and draws at its scale; a menu for another
  output gets a fresh surface, since a layer surface cannot move. Checked in a scale-2 shadow
  with a second (scale-1) headless output: a right-click on the second output opened cce-desktop's
  menu there, at the click and at scale 1, and none on the first.

- **A button's icon sits at its optical centre** (`Button::optical_offset`, `OPTICAL_SHIFT`).
  The user saw cce-claude's sidebar toggle (the `menu` glyph) as off centre. Measured on the
  live window it was centred to half a pixel; what moves it to the eye is the flush plate's
  lighting: under the top-left light the bright lines are inside the groove top and left and
  outside it bottom and right, a frame about 1.5 logical px right of and below the footprint's
  centre (wall 4.7). The icon now moves away from the light (`relief_shade::light_vector`) by
  a fraction of the wall. 0.4 put it on the bright lines' centre but read low on a 3x zoom, the
  dark face pulling the eye back; 0.3 read centred. Only icons move: a symmetric glyph shows
  the shift where a label does not. Checked: `an_icon_on_a_relief_plate_sits_away_from_the_light`;
  shadow shots of cce-claude's header at 0, 0.3 and 0.4, the glyph's pixels measured against
  the rim's.
- **The parameter pane has a slider-with-spinbox row** (`sliderspinbox:min:max:step[:decimals]`),
  hosting a `SliderSpinbox` held by value as it hosts a `Float3`: the pane draws and routes the
  composite's two parts with its slider and spinbox code and writes the number back as the box
  shows it. `SliderSpinbox` gained the held surface for it (`parts`, `parts_mut`, `sync_parts`;
  its number moved into a `Number` so a sync holds the parts and the state at once). Slider rows
  are now `is_slider_row`, since `starts_with("slider")` also matched the new type. Asked for so
  cce-designer can declare such parameters (its `sliderspinbox` kind). Checked: the pane tests
  `a_slider_spinbox_row_is_a_band_with_a_box_at_its_end` and
  `a_slider_spinbox_row_writes_back_from_both_parts` (a press on +, a drag, the wheel on the box,
  a host write), `a_held_composite_syncs_its_parts`, the full suite; a shadow cce-designer with
  two such parameters on a sphere: + stepped 4 to 6 and a band drag set 1, both read back
  through its MCP `get_state`.
- **A grid app's context menu stands on a layer surface of its own** (`backend/menu_layer.rs`).
  cce-grid is to draw the desktop's background menu in process (it was a `cce-cloud --json`
  popup from a shell script), and a popup cannot ride its surface: the compositor scales that
  surface with the desk camera, so a menu opened in overview would shrink with the desk; it is
  pinned at buffer scale 1; and it never holds the keyboard. For a grid app `sync_menu_popup`
  now hands over to `sync_menu_layer`: the menu is in OUTPUT-logical px, placed by
  `constrain_to` against the output and hosted on an Overlay layer surface (`cce-menu`
  namespace, exclusive keyboard) at the output's scale. Pointer events on it are offset by its
  place, as the popup's are; the new `context_menu::pointer_on_host` says which surface an event
  came from, which a grid app needs (its own surface speaks patch px). Keyboard leave closes it,
  the compositor's click-away for cce-cloud's menus, which it now gives `cce-menu` too.
  `render_menu_popup`'s drawing moved into `draw_menu_to`, shared by both hosts. Checked in a
  scale-2 shadow with cce-grid: the menu opens at the click, full size and sharp at zoom 1 and in
  overview at zoom 0.29, slides in from the right edge, hover and the page turn (Window Mode)
  work, a row runs, Up/Down/Enter walk and run it, and Escape and a click away close it.

- **`SliderSpinbox`: a slider with a spinbox at its right end, for an int or a float.**
  `new_int(value, min, max, step)` and `new_float(value, min, max, step, decimals)` build a
  standard `Slider` and `Spinbox` as `Embedded` children holding one number, kept as the spinbox
  keeps it (units of `10^-decimals`), so neither widget changed. The box follows the band on the
  step grid, the band follows the box exactly, and the composite reports `Changed` when the
  number moves (`docs/widgets.md`, "SliderSpinbox"). A slider's readout types a value but has
  no step of the host's choosing and no integer form, and a spinbox alone has no coarse handle.
  Checked: seven unit tests (the start values, the side-by-side placement and linking, the box
  following the band on the grid, a routed press on + moving the band, a routed drag on the
  band, a value string, a set value reaching children held and attached); the cce-gallery
  exhibits (an int and a float) in a shadow session: + stepped, a drag moved the box, a typed
  value moved the band.
- **Images honour `clip_rounded`.** The paint walk recorded the rounded clip on every item, but
  tessellation queued an image with only its scissor, so an image inside `clip_rounded` kept
  square corners (cce-mail's HTML view drew its page under three scissors to dodge it).
  `DlImage` and `ImageQuad` now carry `clip_rrect` (`[cx, cy, bx, by, r]`, scaled to physical
  px as `dl_batches_2d` scales a batch's), and `image_quad_vertices` hands it to the glyph
  shader's existing rounded-rect clip. That clip was a hard circular-arc discard; it is now
  shader2d's: the corner shape (`layout::corner_shape`, carried as a third `clip_extents`
  component, so `GlyphVertex` is 56 bytes and location 4 a vec3 in all three pipelines) and a
  ~1px feather folded into alpha. A clipped image corner and a plate corner now share both curve
  and AA, and text under a rounded clip, which goes through the same shader, gets the same edge. The 3D
  pass's `scene3d_image.wgsl` is unchanged: display-list images never reach it, only
  app-staged world-space `SceneImage`s, where a screen-space rounded clip has no meaning.
  Checked: two unit tests (an image under a translated `clip_rounded` keeps the clip through
  tessellation; an `ImageQuad`'s clip lands on its six vertices, and an unclipped or radius-0
  one leaves it off). Also checked in a scale-2 shadow session, in cce-mail's HTML view with a
  full-bleed red page: all four corners were cut on the plate's squircle. The WebGPU path was
  not compiled (no wasm target here).

## 2026-10-13

- **A widget's own clips reach the frame.** `Adapted::paint_self` paints a widget into a
  scratch list and replays it into the frame, and it restored only an item's circle clip: the
  scissor and rounded clip a widget painted under were dropped, so TreeList's viewport clip and
  Graph's wire clip never took effect (found during the RFC's round 4; see below). The replay now
  pushes each item's scissor, circle and rounded clip around it, composing with the walk's clip
  and translation as any push does (`PaintCtx::push_clip_rrect` re-pushes a recorded rounded
  clip). The own-labels bridge still re-derives a non-subtree widget's text without these clips;
  no such widget paints text under one. cce-designer's palette goes back through `paint_self`.
  Checked: `a_widgets_own_clips_survive_the_paint_walk` (fails without the fix), lib tests with
  every feature (711), every integration test, clippy, the wasm build; scale-2 shadow A/Bs of
  cce-data-editor's tree (scrolled) and the designer's default view and palette (open, typed,
  caret moved, closed): identical (the designer's start shot differs only where the pointer
  sat).
- **`hover_animation` keeps only the page scroll offset.** Its hover highlight glided toward a
  box `register_hovered` set, and nothing called `register_hovered`: `get_quad` never returned a
  box, `post_render_check` (cce-system-interface, until its Stage A) aimed an invisible one at
  the cursor, and `tick` (cce-gallery, cce-designer) reported no change. `register_hovered`,
  `post_render_check`, `get_quad`, `reset_frame_registration`, `set_cursor_pos`, `tick` and
  `HoverState` are gone, with the window's cursor copy only `post_render_check` read;
  `WindowState` holds the offset as a `Cell<f32>`. `set_scroll_offset` / `get_scroll_offset`
  stay: the popover hit tests (the context's, the context menu's, TreeList's add-key box)
  subtract the offset a scrolling host sets. The three apps dropped their calls first. Checked:
  `the_scroll_offset_reads_back_as_set`, lib tests with every feature (710), every integration
  test, clippy, the wasm build with and without features, `cargo check --workspace --all-targets`.
- **The flat-host bridge is gone: `render_widget`, `RenderTarget` and the carve types.** Every
  app paints its widgets with `paint_root_into` into a `PaintCtx` now (cce-system-interface was
  the last to replay them, until its Stage B), so the walk that replayed a widget onto a
  colour-typed flat target goes: `compose::render_widget` / `render_widget_h`, the
  `scene::paint::RenderTarget` trait and `PaintCtx`'s impl of it (`render_target.rs`), the carve
  types it took (`ReliefCarve`, `CarveKind`) and `PaintCtx::carve`, `Button::inset_face` and
  `ControlPlate::face_fill` (the tuple and the fill those hosts read). `compose/render.rs` keeps
  `place` / `place_h`. The tests that probed the bridge with hand-made targets assert on display
  lists instead: `a_placed_button_takes_its_block_and_paints_there`,
  `a_placed_labelled_text_box_lays_its_field_out_below_its_label`, `a_placed_dot_paints_its_disc`,
  `a_placed_graph_paints_its_wires_and_ports` and `a_placed_dropdown_paints_its_arrow_glyph`
  (which replace the bridge's own, as the bridge had no other job). Checked: lib tests with every
  feature (709), every integration test, clippy, the wasm build with and without features,
  `cargo check --workspace --all-targets` (no app named any of it).
- **`ScrollRegion`'s frame and bars paint onto a `PaintCtx`.** `push_prims`,
  `push_scrollbar_fore`, `push_scrollbar_prims` and `push_scrollbar_prims_alpha` took `&mut dyn
  RenderTarget`; every caller (cce-fonts, cce-mail, cce-cloud, cce-system-interface) passes a
  `PaintCtx`, so they take `&mut PaintCtx` and emit the same rounded fills directly. Checked:
  `the_fore_copy_is_drawn_after_the_rows` (now reading the fills' alphas off a display list),
  lib tests with every feature (709), clippy, the wasm build with and without features,
  `cargo check --workspace --all-targets`.
- **Popovers paint onto a `PaintCtx`: `render_popover(&mut PaintCtx)`.**
  `WidgetHost::render_popover` and `Paint::draw_popover` took `&mut dyn RenderTarget`, the flat
  host's colour-typed surface; every host already handed them its frame's `PaintCtx`, which
  implemented that trait. They take `&mut PaintCtx` now, and the five popovers (Dropdown, Menu,
  TreeList's add-key box, Ramp and ParametersBg, which forward to their dropdowns) are written in
  `PaintCtx` calls that emit the prims the trait's `PaintCtx` impl did: a `rect` is a `quad`, a
  `rect_with_radius` a `rounded_rect` on all corners, an `inset_plate` an `inset_plate` with four
  equal radii and `Material::face`, text a `text_with` in bytes truncated as before
  (`scene::paint::text_rgb`, crate-private), the Dropdown's mark glyph a `clip` round an `icon`.
  `compose::render_popovers` and `PopoverCollector`, which kept only rects and text, are gone
  (no host called them), and `partition_concentric_corners` moved from `scene::paint` to the
  Dropdown, its one user, as a private function. Checked: `a_marked_option_draws_its_glyph` (now
  on a display list), lib tests with every feature (709), every integration test, clippy, the
  wasm build with and without features, and a scale-2 shadow A/B (`CCE_UI_FULL_DAMAGE=1`) at 0
  differing pixels in every state: cce-gallery idle, its Layout dropdown open, a row hovered,
  its File menu open and its row hovered; cce-data-editor idle and its TreeList's Add Key box
  open.
- **Sections and forms paint onto a `PaintCtx`, not a flat `RenderTarget`.**
  `compose::{Form, FormGroup, SectionContext, PageLayoutBuilder}` take any
  `P: AsMut<PaintCtx>` (a `PaintCtx` is `AsMut` of itself; a host may wrap one) instead of
  `P: RenderTarget`. A widget piece is placed (`compose::place`) and painted with
  `paint_root_into`, so the page gets the prims the widget paints — a spinbox's seams, a
  field's one outline, a trough's per-corner radii, the widget's own clips — where the flat-host
  replay (`render_widget`) handed it an approximation. A group's text, lines and rule and a
  section's title are `PaintCtx` calls, colours truncated to bytes as before. Under relief
  styling a section's `finish` carves its own well after its content (`PaintCtx::section_well`,
  radius 20, as deep as the bevel and at most a fifth of the well: what the one host drew from
  the hook), so `RenderTarget::section_relief_style` / `section_relief` and `SectionFrame` are
  gone; a focused section keeps the 1px outline, as that host chose. A tabless well follows an
  empty clear quad so it always shades as an overlay: as the first carve after the window plate
  it became a CSG feature of the plate instead, and switched look as a list row's hover wash
  above it came and went. The content clip `PageLayoutBuilder` pushes is a `push_clip` snapped
  out to whole logical pixels, because the scissor truncates a fractional left edge and width
  separately and shaved the last physical column off a control's right rim running the full
  content width. `PaintCtx::len` / `is_empty` are new, for a host that splices another walk's
  items into its list at a recorded point. cce-system-interface, the one user, moved with it
  (its Stage B; the visible differences are listed in its commit). Checked:
  `a_widget_piece_paints_the_widget_where_it_was_placed`, `finish_carves_the_well_after_the_content`,
  `the_content_clip_is_snapped_out_to_whole_pixels`, `text_is_cut_at_the_edge_of_its_piece` (now
  on a display list), lib tests with every feature (709), every integration test, clippy on the
  crate (`--no-deps`: cce-core's own `single_match` in `desktop_entry.rs` stops the dependency
  pass), and the wasm build with and without features.
- **A line of text inside an app's own popover keeps its text: `LineLook::bounds`.** The
  engine clamps display-list text away from every open popover rect it runs under, and knows a
  popover's own text by bounds equal to its rect. It read those bounds after merging in the
  item's clip, so a label clipped narrower than its popover — `paint_line`'s text, cut at its
  well inside a dialog that registers its plate as a popover — no longer matched and was
  clamped to nothing. cce-designer's palette worked round it by re-emitting the query's text
  under the dialog's bounds, cut by hand to the characters wholly inside the line
  (`visible_span`). Now `DlText` keeps the prim's own bounds beside the merged ones and
  `popover_occlusion_clamp` asks them first (then the merged ones, so unbounded text under a
  clip of exactly the rect still counts), and `LineLook` gains `bounds: Option<[f32; 4]>` (`None`
  from `LineLook::new`), which `paint_line` gives its text. The designer's query line is one
  `paint_line` again; an overflowing query now shows the glyph cut at the line's edge, as a
  TextBox does, instead of stopping at the last whole character. Checked:
  `a_line_inside_its_popover_keeps_its_text_when_it_says_so` (fails with the merged-bounds
  check), `a_popovers_label_is_known_by_its_bounds_or_an_exact_clip`, lib tests with every
  feature (706), every integration test, clippy, the wasm build with and without features, and
  a scale-2 shadow A/B of the designer's palette (typed query, Left ×4, Home, a query
  overflowing the line, End and Home): identical but for the cut glyph at the line's edge and a
  one-pixel sliver of the next row's glyph tops below the list, which the dialog's list clip
  now cuts. Found on the way: `Adapted::paint_self` replays a widget's prims under the host's
  clip and drops the rect clips the widget painted them under (it restores only a circle
  clip), so TreeList's viewport clip and Graph's wire clip never take effect; the designer
  paints its dialog through `Paint::paint_ui` to keep them. Not changed here.
- **A cancelled keybind recording keeps the committed binding.** `KeybindRecorder` tracked the
  modifiers held while recording by writing them into `value`, so Escape (or focus leaving)
  mid-recording left the field showing `ctrl` with no change raised: a value the document did
  not have. The held chord now lives in a private `pending` field; `value` changes only on a
  commit, which raises the change flag as before, and a modifier-only chord still commits when
  its last modifier comes up. A modifier coming up during a recording it was not pressed in (held
  when the field was armed) no longer re-commits the old value either. Checked:
  `escape_keeps_the_committed_binding`, `focus_leaving_keeps_the_committed_binding`,
  `a_modifier_only_chord_commits_on_release`, lib tests with every feature (704), clippy, the
  wasm build, and cce-data-editor and cce-gallery (the recorder's users) checked.
- **A lent widget's popover covers again once it is back.** `is_coordinate_covered` memoizes
  the set of popovers covering one point, built from the widgets the context can see, and a
  widget out on loan (`UiContext::lend` / `lend_h`) is one it cannot. A query made during a lend
  (every widget's hit gate asks one) left the lent widget's popover out, and the memo served
  that answer to the next query at the same point after the lend: an app handing one pointer
  event to its panes in turn (cce-designer's `broadcast_pointer`) would let a pane under an open
  menu take the event. The memo now counts its builds; `lend` notes the count when the widget
  goes out, and when it comes back with a popover rect, a memo built in between is dropped. A
  widget with no popover keeps the memo, since a pointer move lends every widget in turn and a
  rebuild after each is the N×N scan the memo exists to avoid. cce-designer's
  `deliver_to_slot`, which dropped the memo itself after lending a slot with a popover, no
  longer needs to. Checked: `a_lent_widgets_popover_covers_again_once_it_is_back` (fails without
  the fix, through `lend_h` and `lend`) and `lending_a_widget_with_no_popover_keeps_the_coverage_memo`
  (one build serves the lend and the query after it), lib tests with every feature (701), every
  integration test, clippy, the wasm build with and without features, and cce-designer's tests
  (520) without its workaround.
- **`Application::custom_vertices` is gone.** The hook appended raw `Vertex`es as a final
  unclipped batch after the display list's. Its one user, cce-display-manager, draws its login
  card's blob as the last geometry of its display list (cce-display-manager 78e2f3a), which
  tessellates to the same six vertices in the same unclipped last batch
  (`card_blob_tessellates_to_the_old_custom_vertices` there, which uses the tessellator and not
  the hook). `build_frame` loses the plumbing; the frame test that covered it becomes
  `overlay_quads_are_their_own_vertex_list`, keeping its overlay half. `overlay_quads` is the one
  side channel left. The trait goes from 44 methods to 43. Checked: lib tests with every feature
  (699), every integration test, clippy, the wasm build with and without features, the display
  manager's tests (22), and `cargo check --workspace --all-targets`.
- **`ContainerLayout` and its seven strategies are gone.** `widget::ContainerLayout`,
  `OverlayLayout`, `VerticalLayout`, `GridLayout`, `AdaptiveGridLayout`, `ColumnsLayout`,
  `MosaicLayout` and `ReverseMosaicLayout` (`widget/container/container_layout.rs`: 581 lines, 13
  `unsafe` blocks walking raw child pointers, and a `measure` half nothing called) had one user,
  cce-gallery, which now arranges its exhibits with pure functions of its own
  (`cce-gallery/src/arrange.rs`) and lands each one through `UiContext::lend`. The golden numbers
  of `test_mosaic_layout` and `test_reverse_mosaic_layout` moved there with them, so those two
  tests and their `MockWidget` go too. The doc comments that described layout as "a strategy"
  (`WidgetHost::label_strip`, `preferred_height`, `CONTROL_GAP`, `control_gap`) now name the
  host's layout and the box model's `controls_column` / `controls_row`. Checked: lib tests with
  every feature (699), every integration test, clippy with every feature and target, the wasm
  build with and without features (warnings denied), and `cargo check --workspace --all-targets`.

## 2026-10-12

- **The layering gate is closed: no module names a layer above its own.** `tests/layering.rs`'s
  `ALLOWED_UPWARD` is empty (architecture RFC phase 3, layering). Four upward edges went. `layout`
  moves to layer 1 beside `style` and `color`, which is what it is: its slots are part of `Style`,
  its getters read it, and it names nothing above the base. That retires `style -> layout` and
  `color -> layout`. For `layout -> scene`: `Rect` moves to a new layer-0 `geom` module
  (`scene::layout` re-exports it, so `cce_ui::scene::layout::Rect` is unchanged), `carve_inside`
  spells its radii as the `(f32, f32, f32, f32)` that `Radii` aliases, and `RECESS_DEPTH` moves
  into `layout` beside `carve_depth_px` (`relief_shade` re-exports it). For `color -> scene`:
  `PlateRung`, `MaterialDef` and `FrostDef` move into `color::materials` beside the slots that
  hold them (`scene::material` re-exports them and keeps `MaterialDef::resolve`), the 5.5 frost
  kernel default is `color`'s (`Frost::DEFAULT_RADIUS` is defined as it), and `param_plate_fill`
  moves to `scene::material`. One public path breaks: `cce_ui::color::param_plate_fill` is now
  `cce_ui::scene::param_plate_fill` (cce-designer, three sites). Checked: lib tests (601 default,
  673 with every feature), every integration test including `layering`, `doc_claims` and
  `plate_golden`, clippy with every feature and target, and the wasm build with and without
  features, warnings denied.
- **`Application` is a core plus four extensions.** Hooks only a few apps override move off
  `Application` onto extension traits, each reached through one accessor on `Application` that
  answers `None` by default (architecture RFC phase 3). `LayerApp` (`layer`, `wants_surface`;
  Wayland shell only) goes through `layer_app`. `GridSurface` (`grid_patch`, `take_damage`) goes
  through `grid_surface`; whether that returns `Some` replaces `grid()` and is read once per
  session into `EngineState::is_grid`. `App3D` (`init_3d`, `stage_3d`, and the native
  `stage_renderer`, which defaults to `stage_3d`; the `3d` feature) goes through `app_3d`. `Csd`
  (`standard_csd`, `csd_resize_borders`, `csd_titlebar_move`, `take_window_action`) goes through
  `csd`. The accessor is the opt-in: an extension implemented without it is never called, and each
  trait's docs say so. Every extension is dyn-compatible, so no runner bound changed and apps that
  override none of these hooks compile unchanged. The eight impls that did override one (gallery,
  keyboard, notifier, the status bar and its OSD, grid, model, designer) move those methods
  verbatim and add the accessor. `renderer_init` stays in the core as a lifecycle hook that does
  nothing by default; `backend::app::renderer_ready` runs it and then `App3D::init_3d` for every
  native renderer. `adjust_size`, which nothing overrode, is gone, and `Driver::cursor_icon_at`
  now takes the app by `&mut`. The trait goes from 53 methods to 44. Checked: lib tests with every
  feature (673), `doc_claims` and `layering`, clippy with every feature, `cargo check --all-
  targets` with and without `3d`, and the wasm builds (library with and without features, the web
  examples) with warnings denied.
- **Widgets can tell the app what happened, in one sweep.** `UiContext::take_events()` returns
  `(WidgetId, WidgetEvent)` pairs (`Clicked`, `Changed`) for every widget the context holds,
  embedded children included, in id order. A widget raises one with the new `EventCtx::emit`, from
  a routed event, a `tick_ctx`, or the `FocusIn`/`FocusOut` of a direct `focus()`/`unfocus()`; the
  adapter keeps them in order until the sweep. The adapter also reports every widget's
  `take_click`/`take_change` flags as `Clicked`/`Changed`, so no existing widget needed to change.
  Until now an app could only poll each handle (122 calls in 19 apps), and `EventCtx` had no way
  to send anything to the app; this is the prerequisite for moving the remaining apps to
  `EventCtx` (architecture RFC phase 3). The per-handle polls stay, but an app uses one or the
  other for a given widget, since both drain the same flags. Dropdown and Toggle keep one flag
  behind both polls, so the sweep reports theirs as `Changed`. Because the sweep reaches embedded
  children, TreeList's search now follows the search box's text instead of reading its change flag
  a tick later. Checked: 7 tests in `context::widget_event_tests`; the embedded-search and
  recorded-chord tests fail with their fixes reverted. Also lib tests (682), every integration
  test, clippy, and the wasm check with and without features.
- **FontSelector and KeybindRecorder report their changes.** Both keep their own `take_change`,
  but their `Input` impls did not forward it, so `ctx[h].take_change()` always answered `false`. A
  picked font family or a recorded chord never reached the app, and cce-data-editor's font and
  keybind value editors never fired. Both `Input` impls now forward it, as TextBox and Dropdown
  do. Checked: `a_picked_family_reaches_the_poll_and_the_sweep` and
  `a_recorded_chord_reaches_the_hosts_poll`.
- **`compose::place`: `render_widget`'s layout half.** `place` / `place_h` lay a widget out in a
  flat host's block exactly as `render_widget` does — the label strip, `layout` at exactly the
  block, `prepare_text`, the open popover's registration — and paint nothing. A display-list host
  leaving the bridge (cce-files, cce-system-interface; architecture RFC phase 3) drops the target
  argument and paints with `paint_root_into`. `render_widget` calls `place` first, so the two
  cannot drift; the popover registration now comes before the replay, which reads none. Checked:
  `a_placed_button_paints_what_render_widget_drew` and
  `a_placed_labelled_text_box_paints_what_render_widget_drew` (same rect, prims equal to what
  `render_widget` drew into a `PaintCtx`), `placing_an_open_dropdown_registers_its_popover`.
- **`PaintCtx::append_translated`: a cached list under an offset.** It appends an already-built
  `DisplayList` moved by `(dx, dy)`, as though the walk that recorded it ran under `translate(dx,
  dy)`: each prim's geometry (`Prim::translated`, every variant named), a text's origin and
  bounds, and each item's scissor, circle and rounded clip (`PaintItem::translated`). The
  context's own translation and clips apply on top. One item in, one out: nothing is re-derived
  through the emitters as `replay` does, which drops a tinted wall's partial edges and re-clamps a
  field's span. It is for a host that records a page once in page coordinates and emits it every
  frame under its scroll offset (cce-system-interface stage A). Checked:
  `every_prim_moves_as_its_emitter_moves_it` (all 27 kinds equal the same paint under
  `translate`), `an_appended_list_moves_as_though_painted_under_translate` (all three clip kinds,
  under a host translate and clip), and three targeted tests; lib tests 681, clippy, the wasm
  builds.
- **Context menu: disabled rows, and the app's own rows collected.** `set_row_disabled(idx)` /
  `set_rows_disabled(vec)` (after `show` / `show_page`, which enable every row) draw a row dim
  (`TEXT_DIM`, its mark too) among the rows that run. It is never hovered, `step_hovered` passes
  over it, it is never run or turned into, a press on it only closes the menu (as on a header),
  and the accessibility tree marks it disabled with no Click. A row with no action of its own is
  now the app's: running it — a press through `mouse_input`, Enter through the new
  `activate_hovered(ctx)`, a screen reader's Click — closes the menu and records `(generation,
  row)` for `take_picked()`. `activate(row, ctx)` runs any row by index, wherever it is drawn. The
  reader's Click goes through `activate` (`a11y_unix::menu_click`) instead of a press at the row's
  centre, so it now reaches app-owned rows and rows scrolled out of a short menu; before, it only
  closed the menu on the terminal's rows and the cce-files breadcrumb view rows. Both are what the
  cce-browser and cce-files menu migrations need (architecture RFC phase 3); the browser's Back /
  Forward / Favourite rows are the first disabled ones. Checked: lib tests with every feature
  (684), the new `a_disabled_row_is_dim_and_runs_nothing`,
  `a_row_the_host_owns_is_picked_by_a_press_enter_or_a_reader`,
  `a_row_scrolled_out_of_view_is_run_by_index`, `a_disabled_menu_row_is_read_but_offers_no_click`
  and `a_readers_click_on_an_apps_menu_row_is_the_apps_pick`; clippy; doc_claims.
- **One copy of the line-edit measuring: `LineMarks` and `paint_line`.** cce-sheets, cce-browser
  and cce-documents each carry their own helpers to measure a `LineEdit` as drawn and paint it;
  the cce-cloud and cce-designer query migrations would have made five.
  `widget/line_edit/marks.rs` holds them once: `LineMarks::of(fs, &edit, size, family)` (the
  caret's x, the selection and composition spans, `x_of` / `byte_at` / `index_at` / `width`),
  `keep_caret` (the scroll that keeps the caret in view), and `paint_line(pc, area, origin_x,
  &marks, &LineLook, placeholder, caret)`. That draws the selection, the text or placeholder, the
  composition underline and the caret, and returns the caret's rect to claim. It reads
  `text::ShapedRun` — the shaping `shaped_cluster_offsets` uses, through the same buffer cache,
  but with a stop at every char boundary and the right end for right-to-left text — never
  `measure_text_width`. It is named for the line, not a field: the well is the app's. The apps
  keep their copies until they adopt it. Checked:
  `marks_stand_on_char_boundaries_in_multibyte_text` (é, 日),
  `a_masked_edit_is_measured_as_drawn_and_hit_back_into_its_text`,
  `a_composition_is_measured_where_it_is_drawn`, `the_caret_stays_in_view`,
  `a_line_paints_its_text_and_marks_and_returns_the_caret`; layering; both wasm checks.
- **`LineEdit`: an Alt chord types nothing.** Typed text was inserted unless Ctrl was held, so
  Alt+letter typed the letter. It is now `Ignored`, left to the host as a Ctrl chord is
  (`DocEditor` already treats Alt this way; AltGr is not Alt). Each new adopter would otherwise
  have guarded Alt itself. Checked: `an_alt_chord_types_nothing`.
- **Markdown without the notes domain.** The `markdown` feature turns on cce-core's `markdown`
  (the reading view's parser and the frontmatter properties, moved there from cce-vault)
  instead of depending on cce-vault, so the toolkit no longer pulls the notes crate in
  (architecture RFC phase 3, extraction). The DocEditor's live preview reads tags, link
  targets, URLs, percent-escapes and embed sizes by cce-core's leaf rules, the reading view's
  and the vault index's: it used to keep a trailing `/` in a tag, keep an untrimmed or empty
  `#subpath`, decode only `%20`, treat only `://` and `mailto:` as leaving the vault and accept
  a size of 0. Its line-role frontmatter classifier stays its own — it styles lines being
  edited rather than reading values. Checked: lib tests with every feature (672), the new
  `links_tags_and_sizes_read_as_the_reading_view_does`, clippy, the wasm build.
- **3D is a feature.** The scene pass, lit meshes and the path tracer — `draw::{scene, lit,
  rt}`, `vk::{scene, rt}`, `web::{scene, rt}`, their seven shaders, `Stage3D`,
  `Application::init_3d` / `stage_3d` and glam — are behind `3d` (architecture RFC phase 3,
  extraction): about 6,000 lines a 2D app no longer compiles, and its device no longer enables
  the ray-query stack (acceleration structures, buffer device address) it never used. The 2D
  shader still samples a backdrop at binding 0, so without the feature the Vulkan renderer's
  scene stage is `vk/scene_off.rs`, the same 1×1 zeroed image the 3D stage makes before its
  first scene, and the WebGPU renderer keeps its empty one; `create_backdrop_image` is shared
  by both stages. `probe3d_native` / `probe3d_web` need the feature; CI's GPU tests and
  check-wasm / check-mac turn it on. cce-designer and cce-model enable it. Checked: lib tests
  without (601) and with (631), clippy both ways, check-wasm's three builds with warnings
  denied, and cce-gallery built from one tree with and without it, shot in a scale-2 shadow:
  pixel-identical.
- **The node graph is a feature.** `widget::Graph` (with `GraphNode`, `TaggedQuad`,
  `node_wires`, `WireStyle`) and the `GraphController` trait are behind `graph`, so the apps
  that draw no node graph stop compiling its 2,300 lines (architecture RFC phase 3,
  extraction). `ContentBg`'s grid setters are its `GraphController` impl, so without the
  feature its grid keeps the defaults — no client without a graph set them. The lattice
  geometry and colours (`layout::graph`, `color::graph`) stay ungated. cce-designer, cce-files
  and cce-graph turn it on. Checked: lib tests with and without (631 / 613), `lattice_probe`,
  and the examples with it on.
- **Each test thread has its own style.** The style was one process-wide snapshot every unit
  test wrote, kept apart by a per-thread colour overlay, a per-thread registry overlay and
  `test_color_state_lock` — with holes (setters that wrote the global style even under test,
  getters that skipped the overlay, config tests that never reset). `style::Store` holds the
  snapshot; under `cfg(test)` each thread has its own, and the overlays and the lock are gone.
  Tests that race writers across threads share theirs (`style::share` / `style::adopt`).
  Checked: 631 lib tests, ten runs in a row; the style integration tests pass.
- **Window properties are per thread off a window.** The scale, app id, fullscreen, maximized
  and vertical text were written to the entered window AND to process statics, and a read off
  any window got whichever window wrote last. No production worker reads them (every
  off-window reader is a custom shell such as cce-cloud's, a tool or a test, each on one
  thread), so `scale::Props` now holds them: the entered window's, else the thread's own. The
  `cfg(test)` scale thread-local and the layering test's `scale -> window_state` allowance
  are gone. Checked by the lib tests and `layering`.

- **An MCP tool can answer with images.** `mcp::content` wraps ready-built content blocks
  (`image_block`, `text_block`) so a tool's result is sent as they are rather than as one text
  block; every other `Ok` value is serialized exactly as before. For cce-browser's agent tools,
  whose screenshot is an image an agent has to see. Checked by
  `content_results_are_sent_as_built`, and over the wire against cce-browser's screenshot tool.

- **The browser shell starts under a strict Content-Security-Policy.** `request_device`
  installed the device's error and loss logging through `js_sys::Function::new_with_args`,
  i.e. `new Function(…)`, which a page without `unsafe-eval` refuses, so the shell failed
  before its first frame. It is two closures set through `Reflect` now. Checked by serving
  cce-vault-web's site with `script-src 'self' 'wasm-unsafe-eval'` in Chromium: it starts,
  with no console errors.
- **Touch in the browser shell.** A finger on a phone was a mouse: a drag selected instead of
  scrolling, and every press focused the keyboard sink, so iOS raised its keyboard for every
  scroll and tap. Touch pointers now go through `backend::touch::TouchTracker`, the tracker
  `wl_touch` already used, and the keyboard is taken at the lift, after an immediate turn, only
  when the frame reports a caret (`ime::caret`). Checked in a shadow session with Chromium
  (WebGPU) driven by `ccectl touch`: taps open rows and buttons, a field's tap focuses the sink,
  leaving it blurs it, and the mouse path is unchanged.

## 2026-10-10

- **`file_dialog` is a feature.** rfd, and through it ashpd and its D-Bus stack, was a
  dependency of every client for a 19-line module seven apps use. It is now optional behind
  `file_dialog` (cce-data-editor, cce-documents, cce-graph, cce-image, cce-mail, cce-model and
  cce-sheets turn it on): a client without it resolves 194 crates instead of 313. A workspace
  build still unifies it on; a standalone build of any other app sheds them.
- **The `colors` alias is gone; the module is `color`.** `colors` re-exported all of `color`,
  and the two spellings were used about equally (134 and 110), in the toolkit and 18 apps,
  so readers met one module under two names. Every path now says `color`; the ramp widget,
  which has its own `color` submodule, names the palette `crate::color::` in full. A rename:
  a whole-workspace build passes, and the cce-ui suite (624) and every app's suite pass.
- **`layout` is the style getters again; `compose` and `scene::paint::target` take the rest.**
  `layout/bridge.rs` mixed the flat-host target with the walk that replays widgets onto it,
  and `layout/section.rs` / `form.rs` placed widgets: all four upward edges out of `layout`
  came from them. The target (`RenderTarget`, `ReliefCarve`, `CarveKind`, `SectionFrame`,
  `PopoverCollector`, `partition_concentric_corners`) is paint vocabulary `PaintCtx` already
  implements, so it is `scene::paint`'s (`target.rs`); the widget walk (`render_widget[_h]`,
  `render_popovers`), `PageFlow`/`SectionContext` and `Form`/`lay_row` are the new top-level
  `compose` module, beside `widget`. `relief.rs` parses ramp specs through cce-core instead of
  `widget`'s re-export. `layering.rs` allows 5. Paths moved, nothing else: six apps'
  imports rewritten (`cce_ui::compose::…`, `cce_ui::scene::paint::RenderTarget`), the suite
  and a whole-workspace build pass.
- **Five more upward edges gone; `layering.rs` allows 8.** The paint walk (`painter`) and
  `WidgetTree` (`tree`) walk and own widgets, so they moved from `scene` to `widget`; `scene`
  keeps the generic `Arena` and names no widget type. `Vertex`, what the 2D shaders take, is
  `draw`'s (`backend::tessellate` re-exports it), so `draw` no longer reaches into `backend` and
  `vk` no longer reaches into `engine`. `a11y::app_tree`, generic over `Application`, is now
  `engine::accessibility_tree` beside the trait, so `a11y` takes only parts. `icon` sits with
  `draw`, which it uploads through. App paths moved with them: `cce_ui::widget::painter`,
  `cce_ui::widget::WidgetTree` (20 apps rewritten), and cce-data-editor's test calls
  `engine::accessibility_tree`. The suite (624), the CI Miri filter (`widget::tree`) and a
  whole-workspace build pass.
- **Text shaping is the top-level `text` module** (was `backend/text/`): shaping, the
  shaped-buffer cache, the font set and bidi. Widgets, `layout` and `window_state` need it and
  it is not part of a shell, so `widget -> backend` and `window_state -> backend` are gone
  from the import graph. `backend::text` stays as a re-export for apps
  (cce-status-interface and cce-system-interface still name it). A path rewrite, nothing
  else: the suite and a whole-workspace build pass.
- **`tests/layering.rs` holds the module layering.** An audit found 17 top-level modules in
  one import cycle. The test fixes a direction (base; `style`/`color`/`scale`;
  `draw`/`layout`; `scene`; `widget`, `text` and their peers; the shells and the renderer;
  `engine`) and fails on an upward `crate::` edge in non-test code unless it is in
  `ALLOWED_UPWARD`, the edges that already existed. An allowance whose edge is gone fails too,
  so the list only shrinks: 15 at first, 12 after the `text` move.
- **cce-core's `plan`, `process`, `fmt` and `desktop_entry` are re-exported** at
  `cce_ui::plan` (the `/run/cce` files), `cce_ui::process` (`spawn_detached`, `de_bin`),
  `cce_ui::fmt` (byte-safe `percent_decode`) and `cce_ui::desktop_entry`, so apps dropped
  their private copies (twelve `spawn_detached`, eight `percent_decode`, five desktop-entry
  parsers).
- **The colour selector places its picker through `ipc::ctl`**: `pointer-location` and
  `place-next` are `ctl::Request`s with a one-second timeout, and the reply is read by
  `ctl::parse_pointer_location`. They were hand-built lines sent with no deadline, on the
  click.
- **`wayland-protocols` is 0.32**, the version smithay-client-toolkit 0.19.2 and ashpd already
  use; pinning 0.31 put both in every client. No API change; checked by a whole-workspace
  build and cce-gallery mapping in a scale-2 shadow session.

- **The code wash is a theme key, `style.text.code_background`** (`color/text.rs`): the wash
  behind inline code and code blocks in the reading view and `DocEditor`, and the box an image
  draws until it loads, read `colors::text_code_background_color`. Unlike the tag and highlight
  washes it is set directly, alpha and all — there is no code colour to derive it from — so an
  8-digit hex keeps it translucent (the default is white at 0.06) and a 6-digit one makes a
  solid panel. The rule and the callout kinds' colours stay constants. Clippy, the suite and
  `cargo doc` pass (`the_code_background_key_is_the_wash_itself`).
- **The highlight is a theme key, `style.text.highlight`** (`color/text.rs`): the wash behind
  `==highlighted==` text in the reading view and `DocEditor` reads
  `colors::text_highlight_background_color`. The key names the colour; the wash is derived, at
  0.40 of its alpha (what it was), so an opaque hex in a theme still leaves the text on it
  readable. With it, every colour the two views drew from Obsidian's theme is the DE's; the
  code wash, the rule and the callout kinds' colours stay constants. Clippy, the suite and
  `cargo doc` pass (`the_highlight_key_sets_the_highlight_and_its_wash`).
- **The quote bar is a theme key, `style.text.quote_bar`** (`color/text.rs`): the reading
  view's block-quote and callout bar, and `DocEditor`'s, read `colors::text_quote_bar_color`.
  The default is the link's violet, as before. `EditorTheme::accent`, whose only use was the
  quote bar, is renamed `quote_bar`; cce-notes, the one client that builds a theme, calls
  `EditorTheme::new` and does not name the field. The highlight wash is the last hard-coded
  colour in the two views. Clippy, the suite and `cargo doc` pass; cce-notes and cce-grid
  check (`the_quote_bar_key_sets_the_quote_bar`).
- **The tag colour is a theme key, `style.text.tag`** (`color/text.rs`): the reading view's
  `#tag` text and wash, and `DocEditor`'s, read `colors::text_tag_color` /
  `text_tag_background_color`. The wash is derived (the tag colour at 0.15 of its alpha, what
  it was), not a second key. The default is the link's violet, as before; the editor drew tags
  in `link` and now has a `tag` field on `EditorTheme`, so a theme can part the two. The quote
  bar and highlight are still hard-coded. Clippy, the suite and `cargo doc` pass
  (`the_tag_key_sets_the_tag_and_its_wash`).
- **The link colour is a theme key, `style.text.link`** (`color/text.rs`): the markdown
  reading view and `DocEditor`'s `EditorTheme::new` read `colors::text_link_color` instead of
  hard-coding Obsidian's violet, so a theme recolours links in cce-notes and cce-grid's cards.
  The unresolved-link colour is derived, not a second key: the link in OKLab at 0.82 of the
  lightness and 0.64 of the chroma, the hue kept, so a theme that sets the link moves both. The
  ratios were fitted to the two hand-picked colours; the derived default is within 1/255 of the
  old one (`the_default_unresolved_link_is_the_old_one`). The reading view's link glyph now
  matches the link text: it was handed the sRGB constant as if it were linear and drew lighter.
  Tag, quote and highlight colours are still hard-coded. Clippy, the suite and `cargo doc` pass.
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
