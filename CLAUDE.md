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
  `src/vk/compute.rs` is the one non-drawing seam: `ComputeDevice::run` uploads a list of
  `Binding`s, dispatches a WGSL `Kernel` on a headless device, waits, and reads the read-write
  ones back — buffers are host-visible and mapped, so upload and readback are memcpys, and every
  failure (a user's bad WGSL included) is an `Err`, never a panic. Built for cce-designer's
  solver operators (its `shapeshifter.md`, Phase 7 step 4); its tests run on whatever Vulkan the
  machine has and skip with a note where there is none.
  The wgpu path is retired; cosmic-text used to be reached through **glyphon**, which is gone
  too — every `glyphon::` item used here was a cosmic-text re-export, and dropping it takes
  wgpu out of the build. There is no HTML/DOM — the UI is GPU primitives (quads, rounded rects with
  per-corner radii, vectors with caps, arcs, circles, and the **relief primitives** — the
  lit-surface family: bevels, plates, recesses, bosses, ridges, fillets, grooves, lattices, box unions; see the
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
  **`tick` is not a clock.** Since 2026-09-11 the runner sleeps between ticks while the
  window is idle (no redraw pending, no animation, no key held, no warm-down) — up to
  `IDLE_DISPATCH` (1 s, `CCE_UI_IDLE_MS` overrides) — and is woken by Wayland events and
  by messages on the calloop `Sender` handed to `new`. It used to tick a flat 16 ms
  forever: every client awake 60×/s doing nothing. So: deliver background results
  through that `Sender`, never by draining a `std::sync::mpsc` in `tick`; if a widget
  or app must poll something the loop cannot see, say so — a widget returns `true`
  from `tick` while the session is live (ColorSelector's picker), an app overrides
  `Application::idle_poll_interval` (cce-authenticator, cce-system-interface,
  cce-designer while a pane is detached). Any animation keeps the frame cadence by
  itself because it reports a change.
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

### A lost surface ends the session; it does not panic (since 2026-09-25)

`VkRenderer::try_new` returns `vk::SurfaceLost` when the display connection under the
surface is already dead — Mesa's Wayland WSI answers the surface queries with a roundtrip,
so `vkGetPhysicalDeviceSurfaceFormatsKHR` is the first call to find out, with
`ERROR_SURFACE_LOST_KHR`. The runner ends that session as `ConnectionLost`: a reconnect if
the compositor is still there, a clean exit if it is not. Mid-session, a swapchain
rebuild, acquire or present that reports the surface lost latches `surface_lost()` and
skips draws (one WARN) until the event loop sees the dead connection itself; the menu
popup just closes. `try_new` is the ONLY constructor: the panicking `new` was removed
once its last callers (cce-cloud, cce-lock, designer's `vk-smoke`) had moved over, so a
new client cannot pick the one that takes the process down at logout. Found as
cce-cloud's daemon panicking at logout on `No surface formats`: it had outlived a
compositor and asked for a window over its connection. Reproduced by opening a
`wl_surface`, killing the shadow compositor, then constructing: `try_new` returns the
error where the old `new` panicked.

### A daemon can outlive its compositor (`Application::outlives_compositor`, 2026-09-25)

When the compositor is gone (`SessionEnd::NoCompositor` — nothing at the socket) the
runner EXITS by default: the compositor saves windows for restore and its successor
respawns them, so a client that rejoined came up beside its own copy (2724002). That is
wrong for a process the compositor does not restore — a systemd user service like the
status bar or the notifier, which must outlive it and whose D-Bus names other programs
depend on. Such an app returns true from `outlives_compositor`; the runner then waits for
the successor's socket (`await_compositor_socket`, a 250 ms poll) and rejoins it with the
same `Application`. Found when the status bar was rebuilt against 2724002: at every logout
its modules exited, the launcher's backoff grew while nobody was logged in, and the tray's
StatusNotifierWatcher came back seconds after the next login — Dropbox, starting into the
gap, reported no tray.

### `renderer_init` — GPU handles do not survive a reconnect

A connection is one **session**. A Wayland transport cannot be repaired once it breaks,
so `run` opens a *new* session around the same live `Application` — same app state, same
calloop loop, same message channel, but a new surface, a new swapchain and **a new
`VkRenderer`**. `renderer_init(&mut self, renderer)` is called once per session: the
first call is the process's own renderer, every later call is a replacement.

What that costs you: an id from `vk::upload_rgba` names an entry in **one renderer's**
image table, and `Frame2D` **skips a draw for an unknown id without logging it**. So any
image id cached across frames — in a struct field, an LRU, a `static` — silently stops
drawing after a reconnect, while every other part of the window keeps working. That
asymmetry is the tell: numbers and text intact, pictures gone.

The fix shape, in every client that needed it, is one method:

```rust
fn renderer_init(&mut self, _r: &mut cce_ui::vk::VkRenderer) {
    if std::mem::replace(&mut self.seen_renderer, true) {
        // …drop the dead ids and arrange for the pixels to be produced again
    }
}
```

Act only on the second and later renderer: uploads queued before the first one existed
are drained into it, so dropping them there just uploads, destroys and re-uploads
everything before the first frame. Freeing a stale id is always safe and worth doing —
`ImageStage::destroy_image` returns early on an id it does not hold, and `NEXT_ID` never
resets, so a stale id can never collide with a live one. The failure is always "draws
nothing", never "draws the wrong picture".

Three traps, each of which cost a session real time in the 2026-09-19 sweep:

- **A widget can hold the id too.** `Button::with_icon(id, …)` captures what you hand it
  and outlives the renderer, so invalidating a cache underneath it changes nothing on
  screen. For bundled cce-icons artwork use `Button::with_icon_name` / `Button::new_icon`,
  which hold the NAME and re-resolve through `upload_icon` per read; `upload_icon`'s own
  cache is keyed on `vk::renderer_epoch()`. `with_icon` still means "the app owns this
  upload", which is right for app-rendered content — and carries the app's duty to
  re-set it from here. `ImageView` borrows its id on the same terms.
- **A WPE client does not self-heal.** Nothing provokes a repaint of a page that has
  finished loading, so `pump` finds no buffer held and the stale id just stays stale.
  cce-mail replays `MailWebView::last_frame`; cce-browser has to remap the active view,
  the same nudge `activate` uses. (A page that happens to animate *would* recover on its
  own, because `update_pixels` recreates an image under an id the new table lacks — which
  is exactly how this hides from whatever page you test with.)
- **An in-flight worker result can carry a dead id.** A thread that uploaded just before
  the drop delivers an id naming nothing, and a store caches it as an entry that draws
  blank for as long as it stays resident. `cce-preview`'s `PageStore` and `cce-map`'s
  `TileManager` carry a generation for this and free a mismatched result on arrival.

Verify with `CCE_UI_FAULT_RECONNECT` (below) — **and run the pre-change binary through
the same fault first.** A fix that passes a test which never reproduced the bug is worth
nothing, and both of the above traps first showed up as a "fixed" build that still drew
nothing.

## Rendering: one paint path (the Phase 3 state)

The backend `render()` **always builds a `scene::paint::DisplayList` and tessellates that single
list** (`window_runner.rs` ~1799). Two ways an app feeds it:

1. **Migrated**: return `Some(DisplayList)` from `Application::display_list()`.
2. **Legacy (default)**: return `None`, and the backend wraps the app's `view*`/`view_vectors`
   tuples into a `DisplayList` via a `PaintCtx` — byte-for-byte the old geometry, just routed
   through the one path.

So every app, migrated or not, renders through the same tessellate step. `custom_vertices` is
appended as a final unclipped batch drawn on top.

## The context menu draws in its own popup surface (since 2026-09-25)

`widget::context_menu` is one global menu that every app shows, paints into its own
display list and dispatches by window coordinates. Drawn in the window it was cut off
at the window's edge, and a menu taller than the room left could not be seen at all.
So on an xdg toplevel the runner mirrors the open menu into an `xdg_popup`
(`backend/menu_popup.rs`, with the reasoning in its module docs): the compositor may
put it anywhere on the output, and the positioner's flip-y / slide / resize-y keeps it
there. A menu cut short scrolls: `ContextMenuState` keeps `content_h` (all the rows)
apart from `h` (what is shown) and a `scroll`, and `row_at` / `row_y` / `hit_test`
answer for the rows as DRAWN — every host that dispatches through them scrolls for free.

A popup path existed before and was deleted in July (Phase 6x) for drawing in one
place and hit-testing in another. Two rules make this one different, and both are
load-bearing:

- **The configure is written back.** Where the compositor put the popup is where the
  menu IS (`context_menu::place`), so the rect apps hit-test is the rect on screen.
- **The popup takes its own pointer input**, translated by its offset into window
  coordinates (`pointer_frame`), so apps need no change — their menu coordinates may
  now simply lie outside the window. The CSD move/resize checks are skipped for it.

While the popup is up the menu is `hosted`: the apps' in-window `paint*`,
`text_labels` and `extra_quads` draw nothing, and the runner paints a copy at the
origin (`paint_hosted`). There the plate is the surface's ROOT, so its frost is the
compositor's blur-behind, not the in-app pass — which has nothing to sample inside a
popup and resolves to flat opaque grey. The compositor's blur cannot compress luminance
the way the in-app frost does, so the root plate's alpha is raised to
`1 - (1 - a)(1 - k)` to let the backdrop through by the same amount. The compositor
blurs popups since the same date (`xdg_popup.rs`'s `update_blur`).

The popup's renderer is kept across opens: `VkRenderer::detach_surface` /
`attach_surface` move it from one popup's `wl_surface` to the next, so a re-open costs a
swapchain rather than a device and every pipeline. **Detach before the popup drops** —
the drop destroys the `wl_surface`, and a swapchain must not outlive it.

Layer surfaces keep the in-window menu, placed by `context_menu::constrain_to` with the
same flip / slide / shorten rules inside the window; so does any app run with
`CCE_UI_MENU_POPUP=0`.

### A float row can be two, three or four wide (since 2026-10-01)

`float2:lo:hi` and `float4:lo:hi` parameter rows are the `float3` row's group with two
or four sliders (X Y, X Y Z W): `Float3::set_components(n)` / `with_components(n)`,
four sliders stored with the first `n` laid out, drawn, hit and written
(`value_string` joins `n` components, `scaled_values` / `set_values_n` read and write
them). `ParametersBg` treats every `floatN` row alike (`is_vec_row`, `vec_row_n`) and
sizes it with `Float3::preferred_height_for(labeled, n)`; a row whose width changes on
a re-read takes the new width in place. Only a three-wide group has a trackball — a
direction is three numbers — so `float4:…:trackball` has none. The designer's
Attribute node presents its Value through these.
`float2_and_float4_rows_are_the_group_with_two_or_four_sliders` is the test.

### A parameter pane can draw separators (since 2026-10-01)

A row of type `separator` (`parameters_bg::SEPARATOR`) is a hairline between two runs
of rows: one pixel tall with the ordinary row gap either side, drawn in
`plain_quads` in the theme's `surface_border`, and never hovered, focused, edited or
given a backing. Its key and value mean nothing — a host writing rows back finds no
parameter by them and skips it. Unlike a `section` it has no title and collapses
nothing; it is what a host puts between groups of parameters that are about different
things (the designer derives them from its templates' `group` metadata).
`a_separator_row_is_a_rule_between_rows` is the test.

### A slider's range can be soft (since 2026-10-01)

`Slider::set_soft` / `with_soft`, `Float3::set_soft`, and in `ParametersBg` a `soft`
segment after the range (`slider:lo:hi:dec:soft`, `float3:lo:hi:trackball:soft`,
`is_soft_row`): a value TYPED into the readout past either end widens the range to hold
it, where a hard range clamps it to the end. A drag and the wheel still stop at the
ends. It is for a value with no natural bounds whose range is only a scale to drag
over — the host is expected to choose the range around the value and re-choose it (the
designer's Attribute Value row). The pane now writes a slider row back from the
slider's OWN value (`get_scaled_value`) rather than its fraction over the row's
declared range, which was the same thing until a range could widen.
`a_soft_range_widens_to_a_typed_value` and `a_soft_row_writes_back_what_its_slider_holds`
are the tests.

### The keyboard can walk a menu (since 2026-10-01)

`context_menu::set_hovered_item(Some(idx))` highlights a row as the pointer would, and
`context_menu::step_hovered(dir)` moves the highlight to the next row that can run
(down for `dir > 0`), skipping header rows and `-` separators, stopping at either end
rather than wrapping. Both scroll a shortened menu to the row, WITHOUT re-hovering the
row under the pointer the way `scroll_by` does: the keyboard put the highlight there.
The menu itself still reads no keys — a host that wants a walkable menu routes Up/Down
to `step_hovered` and runs `hovered_item()` on Enter.
`the_keyboard_steps_the_highlight_over_what_cannot_run` is the test.

### A dropdown says whether it is taking input (since 2026-10-01)

`Dropdown::is_expanded()` is `open && !closing`: `open` alone stays true through the
closing animation, while the plate is still drawn but presses and keys are no longer
the dropdown's. A host that routes input to a dropdown ahead of what is under it (the
designer's dialog hosts one) asks this. Such a host should also know that the runner
hands every left press to each registered popover whose hit test MISSES it, before the
app sees the press (`close_popovers_missed_by_press`) — and a popover covered by an
outer popover's claim always misses — so the dropdown may already have taken the press
by the time the app is asked.

### A dropdown's text names its font (since 2026-10-01)

The trigger's text and ▼ are emitted with `control_label_font_detached()` named on the
prim, where they used to leave the font to the host. Through the widget walk the
adapter attached `widget_font` and nothing changed; but a dropdown painted as a STAMP
by another widget (the designer's palette paints its choice rows so) took that
widget's font, and the menu `draw_popover` grows out of it — which has always named
the font — opened in a different one. The whole configured string is passed, family
and size together (`Berkeley Mono 14`): the runner parses both and the size wins over
the prim's, which is how the menu's hard-coded 12 px rows and 10 px ▼ come out at the
trigger's size. Passing the family alone breaks exactly that.

A host that draws an open dropdown should hand `render_popover` a real `PaintCtx`, as
cce-files does: a `PopoverCollector` keeps fills as plain rects, so the grown plate
loses its relief and its corners (`inset_plate` degrades to a flat fill there). The
designer's params pane went through one until the same day.

### A well with a flush run at its end is ONE field (since 2026-10-01)

`Prim::Field { rect, radii, depth, split, tint }` (`PaintCtx::field`) is a sunken well
that ends in a flush run: left of `split` the interior one step down, right of it back at
the surface's level, as a flush control plate's face. Two controls are drawn so: a
`ParametersBg` textpick row (the TextBox and its completion picker, a menu-button Dropdown
`PICK_W` wide) and a `Spinbox` (its value and its -/+ run). In both the run is the
control's right end, reaching its outer edge on the top, right and bottom as a dropdown
trigger's plate does.

It is one prim because two did not work: a recess for the well and a trough for the run,
side by side, each shade their OWN box, so at the seam each turns its own square corner
and the strong line of the edge jumps — a recess is lit at its outer rim, a trough at its
inner lip — and the outline reads broken exactly where the two meet. The field's outline
is evaluated once (the whole rect, its own radii, `MODE_FIELD` in `shader2d.wgsl`). Its
outer half is ONE profile all the way round — the recess's fall and shoulder — and only
the inner half differs: on to the floor in the well, mirrored back up to the face in the
run, blended across one wall width about the seam. Note the run's valley is NOT
`Prim::Trough`'s, which fits its whole fall and rise into the wall's width, so its outer
half is a compressed copy of a step that read differently from the well's beside it. The
run's FACE is a rounded rect of its own, inset half a wall on every side — from the
outline on the top, right and bottom, from the seam on the left — whose lip is the
well's fall mirrored back up, and whose left corners are its right corners, so the
button has the same padding and rounding at both ends (since 2026-10-02; until then
the lip was the outline's own inner half and the face met the seam square). Where the
face's rounded corner leaves room by the straight outline is the valley's flat floor.
The seam is the well's own right wall's inner half, from half the step at `split`
down to the floor, meeting the face's lip where both stand at half the step; it fades
out at the outline, which runs straight across. **The run is laid out a wall wider than
its face**, so the BUTTON is what reaches the well and what it carries stands in its
middle: a textpick picker is `PICK_W` plus a wall, its arrow centred
(`Dropdown::center_arrow`) — which is a trigger's ARROW SLOT (`dropdown::arrow_slot`),
the right end every dropdown centres its ▼ in, so the picker's arrow and every other
trigger's stand in one column (since the same day; they stood 18 px in from the right
end, two pixels off the picker's; `a_pickers_arrow_lines_up_with_a_dropdowns`) — and a spinbox's run begins a wall before its flat layout's
buttons (`SpinGeom::run_x`), its face halved at its middle by the -/+ seam with each
glyph in the middle of its half — hit zones, washes, glyphs and relief all read
`SpinGeom`. For a few hours on 2026-10-02 the seam was the right side mirrored instead
— the button's valley rising to a ridge at the surface's level, the well's wall falling
from it a whole wall further left — which read as a strip of new surface between the
well and the button rather than a wider button; the well's floor ends where it did
then, the button having taken the strip. (Before that, the face began half a wall from
the seam with the arrow at the trigger's usual right-hand place, and its left side read
narrower than its right.) A textpick row's TextBox ends where its picker begins, so its
text stops at the well. Never grouped into a host
plate (its profile is not a monotonic step); the overlay's host-box slot carries `split`.
The legacy banded path and flat hosts (`layout.rs`'s bridge) draw the two-box form.

**Every flush control wears the run's edge** (since 2026-10-02): `PaintCtx::inset_plate`
— the one flush control plate, which `ControlPlate`'s Flush stance, every widget and the
apps' own plates all come through — draws a face and a field that is all run (its seam
`FIELD_RUN_ONLY` px to its left), where it drew a `Prim::Trough`, whose outer half is a
compressed copy of a step and read differently from a well beside it. So a dropdown
trigger and its grown menu, a button, a breadcrumb's run, the font selector, a menubar's
triggers, and the calendar's, cce-cloud's, cce-files' and the system interface's own
plates have the edge of the run at the end of a text row's field. It went in a widget at
a time the day before (`ControlPlate::with_run_edge`, `PaintCtx::flush_run`), both gone
now that the plate itself has it. In a parameter pane the dropdown and button rows are
fields too (`fields`; the pane's `troughs()` list went, and its button rows had been a
raised boss the button never drew). The flat-host bridge pairs a plate's face with its
field as it did with its trough. `Prim::Trough` / `PaintCtx::trough` stay for an
explicit valley and `cce-relief`'s preview. `a_dropdown_trigger_wears_the_runs_edge`,
`focus_lights_the_plate_rim`, `a_breadcrumb_and_a_font_selector_wear_the_runs_edge` and
`an_inset_plate_is_a_field_that_is_all_run` are the tests.

**A text box is a field that is all well** (since 2026-10-02): `PaintCtx::well_field`, a
`Prim::Field` whose seam is `FIELD_RUN_ONLY` px past its right end (`field_well_only`),
in `TextBox`'s own paint and in `ParametersBg::fields` for every text row without a
picker. It was a `Prim::Recess`, which the runner GROUPS into a live host plate as a CSG
feature, and a grouped recess drew a doubled outline where the field beside it — never
grouped, its own overlay — drew the single edge. That was first put down to the NVIDIA
device; it was not (see "A grouped carve shades as its overlay does" below, which fixed
the cause the same day, so the two now agree whichever is used). Both fallbacks draw an
all-well field as one plain recess. `a_text_box_is_a_field_that_is_all_well` is the test.

The pieces that feed it: `ParametersBg::fields` (the pane's list, drawn after its troughs,
hover-tinted like them; textpick rows and spinboxes with a run are in neither `reliefs`
nor `troughs`), `TextBox::joined_right` (the box stops at the seam), `Dropdown::set_radii`
(the picker's own raised paint, square at the seam), and `Spinbox::relief_parts`, which
returns a `SpinRelief` — the outline as handed over, its radius and depth, and the run's
`split` with the engraved -/+ seam; the widget's own paint carves it inside as every well
is. `textpick_rows_carry_a_picker` and `a_spinbox_is_one_field_with_its_run_at_the_right_end`
are the tests. Until the same day the picker and the run were nested INSIDE a full-width
well, their faces stopping at the base of its wall, so they never reached the edge a
dropdown's ▼ does.

### A grouped carve shades as its overlay does (since 2026-10-02)

A full-ring, untinted `Prim::Recess` / `Boss` emitted while a plate's grouping window is
open becomes a CSG feature of that plate's one draw (`MODE_PLATE` in `shader2d.wgsl`);
otherwise it is its own overlay (`MODE_RECESS` / `MODE_BOSS`). The two must look the same
on the plate's face. They did not: the grouped one drew a **doubled outline**, two thin
black lines down its shadowed wall and two bright ones down its lit wall, where the
overlay drew one soft edge. Two causes, both in how the plate path applied the carves:

- **The shade line took the carves' slope.** The plate's colour subtracted
  `roll_shade_line(sv)` with `sv` the roll's slope PLUS every carve's. The shade line is
  a narrow lobe at the half-vector's tilt (22.5° at the DE's 45° light); a carve wall's
  tilt rises through that angle and falls back through it, so the lobe fired twice per
  wall — subtracted in colour units from a face near 0.016 linear, both hits went to
  black. The overlay path, and `relief_shade::carve_shade`, never had a shade line: it
  exists for the raised roll. It is now `roll_shade_line(sv_rim)`, the roll's alone.
- **Diffuse and curvature were a multiply on the face; the glint was added whole.** On a
  dark face a multiply barely moves the pixel, so the overlay's lit shoulder vanished,
  and the glint's two crossings (the same twice-through-the-angle as above) no longer
  cancelled against the fillet's darkening as they do inside the overlay's one signed
  value. The plate now lights its roll alone as before (multiply plus glint and shade
  line), and composites what the carves ADD — the summed normal's diffuse and glint less
  the roll's, plus their curvature — as an overlay is blended (`carve_over`: screen
  toward white, multiply toward black). On the face that is the overlay's `v` term for
  term; across the roll the normal is still the summed one, the junction grouping is
  for. A plate with no carves composites `v = 0` and is unchanged to the bit.

Measured with `examples/grouped_recess_probe.rs` (a plate whose recesses group beside the
same recesses forced to overlay by a transparent quad): grouped and overlay columns
5,462 px apart before, 0 after; in the designer with grouped text wells, 0 px from the
`well_field` rendering.

**And it is tested by rendering, not by reading** (`vk::plate_probe`, `cfg(test)`): an
offscreen 2D render — a `DisplayList` through the runner's own `tessellate_display_list`
and `dl_batches_2d`, drawn with the LIVE pipeline into an image and read back. The
renderer's pieces it needs are shared functions, not copies, so the two cannot draw
differently: `create_ui_pipeline` (descriptor layout, push range, vertex layout, blend),
`batch_push_constants` (a batch's 32-float block, feature rebase included),
`window_info_data` and `relief_px_at`. It draws plates, carves and flat geometry; it
refuses blur-behind (the snapshot is the swapchain path's) and draws no text or images.
`render` returns `None` with no Vulkan device and the test skips with a note; it is not
`#[ignore]`d, since a device is the normal case here and a regression nobody runs is
not caught. `a_grouped_carve_is_drawn_as_its_overlay_is` renders two plates, one grouped
and one forced to overlay, asserts the grouping really happened (five features), and
holds them equal to the pixel: against the pre-fix shader it fails at 32,504 px, worst
channel 54. Clean under `VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation`. Opening the
device adds about 2 s to the suite. A test of any other 2D look can use the same harness.
(Across devices the live path agrees to within 5/255 on about 125 edge pixels of the
probe — float rounding at antialiased edges, not a shading difference.)

**It was never the GPU.** The report was "doubled on the NVIDIA card, single on the
Iris Xe"; the 2D path is identical to the pixel on both, before the fix and after. Two
things made it look GPU-specific. Grouping is decided per frame by what is painted
between a plate and its carves, so two captures of "the same" pane can differ in what
grouped. And inside a `cce-shadow` session the NVIDIA ICD does not load at all unless
the process can reach an X display (`DISPLAY=:0` and `XAUTHORITY=$HOME/.Xauthority`):
`vk_icdGetInstanceProcAddr` fails, the loader skips the ICD, and `CCE_VK_DEVICE=discrete`
fell back to the Intel device SILENTLY. It says so on stderr now (below, "Debug
environment variables"); `grep -c nvidia /proc/<pid>/maps` is the check from outside.

### A row can open a submenu (since 2026-09-29)

`context_menu::set_row_submenu(idx, SubmenuSpec { options, header_count, sliders })`,
called after `show` like `set_row_slider`, gives a row a SUBMENU: a second menu that
flies out beside the row while the pointer is on it. The row wears a `›` at its right
end. **The menu opens and closes it; a host says what is in it and dispatches its
rows.** One level deep: a submenu has no submenus.

- **It is a second `ContextMenuState`**, the `SUBMENU` thread-local, so everything a
  menu does — sliders, scrolling, the plate — a submenu does by the same code. Its
  `parent_row` is the row it flew out from and what tells the two apart.
- **The pointer calls answer for both; the row calls are each menu's own.** `hit_test`,
  `cursor_moved`, `mouse_wheel`, `slider_press`, `slider_dragging` and `slider_release`
  route to whichever menu the pointer is in, so a host with no submenus is unchanged and
  one with them routes nothing by hand. A row index means nothing without its menu, so
  `row_at` is the menu's alone (and `None` over the submenu) and `take_slider_change`
  the menu's sliders'; the submenu's are `context_menu::submenu::row_at`,
  `::take_slider_change`, `::parent_row`, `::slider`, `::options`.
- **Hover intent is a triangle, not a timer.** Moving from a row into its submenu
  crosses the rows between; while the pointer is inside the triangle from where it last
  was on the open submenu's row to the submenu's near edge (a row taller at each end),
  those rows neither hover nor swap the submenu. Straight down the menu is outside it,
  and once the pointer has arrived in the submenu the apex is dropped, so coming back
  out is an ordinary move. `open_submenu(idx)` is for a press on the row, under a
  pointer that has not moved since the menu came up.
- **Setting a submenu that is OPEN changes it where it stands** — labels and slider
  values, keeping its hover, scroll, held slider and popup — which is how a host
  re-marks a row after it was picked. A different number of rows shows it afresh.
- **Its popup is the CHILD of the menu's popup** (`menu_popup.rs`), positioned against
  the menu's whole width at the row's height: anchor top-right, gravity bottom-right,
  flip-x / slide / resize-y, so it lands on the menu's left where the output has no
  room on the right. Its configure is relative to its parent, so where it is, is that
  plus where the menu landed. It has its own renderer (`submenu_renderer`), kept across
  opens. **The child closes first**: a popup that is not the topmost may not be
  destroyed, so `close_menu_popup` closes the submenu's ahead of the menu's. In a window
  with no popup surface `submenu::constrain_beside` does the same flip and slide.
- The cursor over either menu is the default arrow (`cursor_icon_at`): what lies under
  a popup in window coordinates is the app's splitter or resize border, or nothing.

The designer's viewport menu is the first consumer (Style, Markers).
`context_menu_submenu_tests` covers the state; the popups were checked in a shadow
session, at the output's right edge included.

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
  tints the rim (a tinted `Trough`, `Boss` or `Bevel`), the same treatment a
  well's `recess_tinted` gives its rim while editing — never extra geometry.
  The tint recolours the relief rather than replacing it: the rim's light
  composites in the accent instead of white and its shadow in a dark accent
  instead of black (`FOCUS_SHADOW`, both at `FOCUS_GAIN`, in
  `shader2d.wgsl`), so a focused plate still reads which edges face the lamp.
  A Checkbox lights the ring its mark already draws; a Toggle lights the
  rim of the plate that slides in its well.
  Roles today: Button, Checkbox, Toggle, Dropdown, FontSelector, ButtonStrip
  (arrows move the selection between its segment plates) and Breadcrumb
  (arrows walk its visible segments, Enter navigates) are plates; TextBox,
  Spinbox, ColorSelector, KeybindRecorder, TreeList, Slider (a band, but
  entered and adjusted in place — arrows step it, Enter opens the readout)
  and RangeSlider (one stop, two ends: arrows step the focused end, Up / Down
  switch ends) are wells. A new focusable widget declares its role and handles `FocusIn`
  / `FocusOut`.
- **What a plate is made of is a `scene::Material`** — tint, `Frost` (opaque, or
  frosted with compression / refraction / radius) and `Finish` (how it answers the
  light: the old `relief_shade::Material`). `docs/rfc-material.md` is the design and
  its phase tracker. `Material::fill_tint` is the ONE place the blur-behind sentinel
  (a negative alpha) is written; `PlateSpec::fill` and `param_plate_fill` call it.
  Rung defaults: `Material::root()` / `pane()` / `control()`, `popover(base)` for
  menus; a well floor is `host.floor(lifted)`. `PlateSpec`, `ControlPlate.face`
  (`Option<Material>`: `None` = the surface below IS the face) and the prims
  `Plate` / `Bevel` / `Sphere` / `Droplet` carry one, and `PaintCtx::plate` /
  `bevel` / `sphere` / `droplet` / `inset_plate` take one; the tessellator reads
  each prim's fill and push-constant finish from it, and only the carves still take
  the DE finish. `Material::from_fill` / `face` decode a colour a legacy site still
  holds (the flat-path `RenderTarget` is colour-typed) — a new site says
  `Material::opaque` / `with_frost` instead. **`tests/plate_golden.rs` is the exit
  test for any change that must not move a pixel**: dump before, compare after.
- **One plate spec per rung, not five copies.** The root and pane rungs are
  `scene::paint::PlateSpec` (RFC 7b, painted by `PaintCtx::plate`). The
  control rung is `scene::paint::ControlPlate` (re-exported from `widget`):
  footprint, per-corner silhouette, `PlateStance` (raised, flush or flat), face
  and depth, painted by `PaintCtx::control_plate` — the ONE place a control
  face's relief is composed (raised with a face = bevel; raised faceless =
  carve inside + boss; flush = carve inside + inset plate). Button, Dropdown,
  FontSelector, Breadcrumb and the ButtonStrip's selected plateau draw through
  it; the migration was prim-identical against a dump of every face. A new
  control face goes through `ControlPlate`, never a hand-rolled carve.
- **`Flat` is the stance for a control made of its pane's material.** The two
  relief stances both carve INSIDE the footprint, which costs a control two
  things a pane has: its visible edge sits half the carve depth in, so a
  control laid out on the same numbers as a pane does not line up with one;
  and its face is laid through a stroke, which the blur-behind sentinel (a
  negative alpha) does not reach, so it cannot be frosted. `Flat` fills the
  footprint with a quad and nothing else — silhouette equal to the rect,
  frost carried, focus `tint` drawn as a ring since there is no rim to light.
  It carries ONE radius, not four, so the concentric corner adjustment a
  nested relief control computes has no equivalent. Reach for it when a bar
  or a toolbar should read as plates at a smaller scale rather than as
  controls of a different kind (`Button::with_flat`, `Dropdown::with_flat`);
  leave the relief stances alone for things that should feel pressable.
- **Radii are configured per rung, overridden per widget.** Root:
  `style.surface.plate.root.corner_radius` (`color::root_plate_corner_radius`).
  Pane: `plate_corner_radius`, falling back to the root's. Control:
  `style.control.corner_radius` (`layout::control_corner_radius`, default 8) —
  every control-scale getter (button, dropdown, font selector, slider,
  spinbox, textbox, toggle, list and tree wells; the ColorSelector's two via
  the textbox) falls back to it when the widget's own `corner_radius` key is
  unset, so a per-widget key is an override, not a requirement. Do not give a
  new control-scale radius getter a literal default; fall back to the rung.
- **A config hex is gamma-decoded; a built-in default colour is not.** The
  style loader's `parse_hex` runs every channel through `srgb_to_linear`
  (alpha excepted), so `"#595969"` arrives as `[0.10, 0.10, 0.14]` — which is
  exactly `PARAM_BG`'s default. The constants in `color.rs` are already
  linear, so **the hex that pins a default is not that default's floats times
  255.** `PARAM_BG = [0.10, 0.10, 0.14]` reads as `#1a1a24` if you scale it
  naively, and `#1a1a24` decodes to `[0.010, 0.010, 0.018]` — a plate ten
  times darker than the one you were trying to preserve, silently, because
  both spellings are valid config.

  Round-trip a default with `l2s(c) = 1.055·c^(1/2.4) − 0.055` (the inverse of
  `srgb_to_linear`) before writing it into a config, or read the value back
  out of the running app. This cost a measurement round on 2026-09-19: a
  `backdrop_compression` sweep meant to hold the tint constant was silently
  sweeping the tint too, and the two halves of the experiment disagreed by 3x
  on the plate's luminance.

  Two smaller edges of the same knife: an **8-digit** hex keeps its alpha raw
  (`a/255`, no decode), so `#05050840` really is a quarter opacity; a
  **6-digit** hex sets alpha to **1.0**, so dropping the last byte off a
  translucent plate colour makes it fully opaque rather than leaving it
  alone.
- **Frost is one block, and "unfrosted" is not "solid"** (2026-09-28). The
  default plate material's recipe is spelled `style.surface.plate { frost
  radius=5.5 compression=0 refraction=0 }` — the same `frost` child a named
  material has — with a bare `frost` frosted at the defaults and `frost
  (bool)false` off. The four keys it replaced (`blur` as the switch,
  `radius`, `backdrop_compression`, `refraction`) were aliases for the rest
  of that day and are RETIRED: the loader reports one it finds
  (`color::retired_surface_keys`, a warning naming the path) and does not read
  it — so a config with `blur=true` and no block is sharp, and the warning
  is what says why. A material's `frost` child spells its compression
  `compression` too; its `backdrop_compression` alias went with them.
  cce-relief writes the block (only for a frosted plate, since the block is
  what frosts one), seeds from a retired key it still finds, and removes
  the retired keys on Save, so a file migrates the first time it is saved.
  The enum variant is `Frost::Unfrosted`
  (was `Opaque`): it means the plate never samples its backdrop, and a
  translucent tint stays translucent with a SHARP view through it — which
  is what the old name kept reading as "covers everything". The pane tint
  has a clear spelling too, `style.surface.plate.pane.color`, whose alpha
  is the whole tint strength; the legacy `style.surface.param.color` is
  still multiplied by the top-level `plate_opacity` line, as it always was
  (`color::pane_color_is_whole`).
- **A frosted plate's legibility is `backdrop_compression`, not opacity.**
  Blur destroys a backdrop's spatial DETAIL and preserves its mean LUMINANCE,
  and text contrast is a mean-luminance property — so `resolve_blur`'s closing
  `mix(backdrop, plate, opacity)` hands the backdrop's brightness through at
  `1 - opacity` whatever the kernel does. At the designer dialog's 0.25 that is
  75% of whatever is behind it. Measured on a row label (`#ccccd4`) over the
  designer's Alt+D plate at the stock tint: **1.16:1 over a white viewport,
  6.65:1 over the dark one** — the bright end not a contrast ratio so much as
  its absence. More blur moves neither number, which is the whole of the
  "liquid glass" legibility problem, and why refraction and specular cannot
  help: they are shape cues, and legibility is a luminance budget.

  `style.surface.plate { frost compression=… }` (0..1, `color::plate_backdrop_
  compression`, **default 0** — every existing config keeps today's look)
  remaps the blurred backdrop's luminance toward the plate's own key before
  the tint, holding its chromaticity. It is not opacity and not "darken": it
  is SYMMETRIC, pulling a bright backdrop down and a dark one UP, so both ends
  converge on the plate's key. The contrast stops depending on what is behind
  the window, which is the actual goal; hue, chroma and movement still read
  through it.

  **It only works with a tint dark enough to converge ON.** A 24-cell sweep
  (k x tint x backdrop, 2026-09-20) — contrast on the bright/dark viewports,
  with `show` the luminance sigma across bare plate (x100), a proxy for how
  much backdrop still reads through:

  | tint | k=0 | k=0.4 | k=0.6 | k=0.85 |
  |---|---|---|---|---|
  | `#595969` (stock) | 1.16 / 6.65 | 2.25 / 4.87 | 3.10 / 4.54 | 4.10 / 4.34 |
  | `#1a1a24` | 1.23 / 9.97 | 2.99 / 10.34 | **5.08 / 10.53** | 9.36 / 10.70 |
  | `#050508` | 1.24 / 10.47 | 3.08 / 11.67 | **5.41 / 12.14** | 10.76 / 12.60 |
  | *show* (bright/dark) | 7.3 / 0.9 | 3.7 / 0.5 | 2.3 / 0.2 | 0.4 / 0.1 |

  Three readings. **The stock tint cannot be rescued at any k** — it never
  clears 4.5:1 on the bright backdrop, and on the DARK one it gets WORSE as k
  rises (6.65 -> 4.34), because `#595969` is lighter than the scene and
  compression lifts the plate toward it. **Tint does nothing without k**: at
  k=0 the three tints read 1.16/1.23/1.24, indistinguishable, because at 0.25
  opacity the tint barely participates — which is why "just darken it" was a
  dead end before this existed. And **k ~ 0.6 is the knee**: both dark tints
  clear the floor on both backdrops with a third of the backdrop variation
  intact, where 0.85 doubles contrast for 95% of the remaining glass.

  So the pair is orthogonal, and that is the point: **k buys independence from
  the backdrop, the tint picks the key it becomes independent at.** cce-designer
  ships `#05050840` at k=0.6 (5.41:1 / 12.14:1). Note `show` is 0.2-0.9 on a
  dark backdrop at EVERY k: there is little luminance variation behind the
  plate there to begin with, so "glass" on a dark desktop is carried by the rim
  and bevel, not by the backdrop.
- **The recipe is per plate.** Since 2026-09-20 (RFC material step 3) compression,
  refraction and the blur radius are a plate's own `Material.frost`
  (`Frost::Frosted { compression, refraction, radius }`), packed into `p_host.zw` of
  its push block by `Frost::pack` — the two style keys above are the DEFAULT
  material's values (`Frost::from_style`), not a window setting, and two plates in
  one window can differ. `radius` is the kernel sigma in logical px;
  `Frost::DEFAULT_RADIUS` (5.5) reproduces the old fixed 5.5-physical-px stride on
  the scale-2 panel; 0 is a clear plate. A frosted FLAT fill of any kind (Quad,
  RoundedRect, Border fill — a `Flat` face, a menu, a popover) is promoted by the
  tessellator to a zero-depth plate batch so it carries its recipe too; only a raw
  vertex from outside the display list falls to the no-recipe branch.
  `examples/frost_pair.rs` is the visual test: three recipes in one window, run in a
  shadow, measured in the RFC's step-3 note.
- **Named materials in config** (RFC step 4): `style.surface.material { <name> { color;
  frost …; finish … } }` and a binding per rung — `plate material="…"`, `plate { root
  material="…" }`, `style.control.material` — resolved by `MaterialDef::resolve` over
  the rung's legacy material (unset fields fall back; no `frost` child = opaque; a
  binding wins over the legacy keys; an undefined name warns and degrades to legacy).
  The DE finish's three fixed terms are `style.surface.relief.spec / shininess /
  curvature`; the default frost's blur sigma is `plate { frost radius=… }`. cce-relief
  edits them (Finish and Frost columns) and writes into the bound material's node or the
  DE keys — never restructuring an unbound config. KDL trap when writing fixtures: two
  nodes on one line need a `;`, and `a { b }` on one line is a parse error the loader
  swallows into an empty document.
- **`plate { frost refraction=… }` (0..1, default 0) is the rim, and it buys
  no legibility.** It is the answer to the other half of the question — not
  "can I read this" but "is this an object". The roll is a real surface with a
  real tilt, and `sv_rim` IS that tilt (the unnormalized normal's horizontal
  part, already computed for the specular), so displacing the backdrop sample
  along it is what a curved edge does to what you see through it. Scaled by the
  roll width, so a 12px bevel bends more than a 2px one.

  **It samples the CLEAN backdrop, not the blurred one**, cross-fading to the
  frosted body on `f*f`. Refraction has to bend something with STRUCTURE or it
  is invisible: displacing a field already blurred to sigma ~11px just moves
  smooth values around. A thin edge scattering over a shorter path than a thick
  middle is also what a real slab does — the droplet branch trades on the same
  thing ("thin edges are clearer water"). One extra tap, not three: per-channel
  dispersion inside a band this narrow is invisible once the body is 49 taps,
  and paying for it would triple the most expensive path in this shader to be
  erased.

  **The clear rim is exempt from `backdrop_compression`**, in proportion to how
  clear it is. Compression is a legibility control and the rim carries no text;
  tone-mapping it pulls the refracted view back toward the plate's own key,
  which is the exact contrast the rim exists to show. Measured, the two
  fighting made the effect nearly invisible — exempting the rim made it **5.9x
  stronger** at the same setting (rim pixel change 1.50 -> 8.86 of 255 at 0.3),
  with the body still under 0.4. Useful range is ~0.3-0.6; the effect is in the
  roll and stays there.

### The surface config shape (`style.surface`, as of 2026-09-28)

The block every plate, wall and roll in the toolkit reads, in the spelling
the loader treats as current — written down once because the rules below
were settled one at a time across a day and each paragraph names only its
own key. What a key is, in a line each:

```kdl
style {
    surface {
        plate material="glass" {                 // optional: bind the pane rung to a material node
            pane color=(rgba)"#6c6c7bf2"         // the pane tint, alpha = strength (legacy: param.color × top-level plate_opacity)
            frost radius=(f64)5.5 compression=(f64)0.0 refraction=(f64)0.0   // the ONE frost spelling; absent = unfrosted
            border_color (rgba)"#9595a9ff"        // the flat border — control_relief OFF only
            border_thickness (f64)1.0
            padding (i64)20                       // the pane rung's inset
            root {                                // the root rung
                color (rgba)"#5e657acf"
                blur (f64)0.1                     // the COMPOSITOR's blur-behind, not the client frost
                corner_radius (i64)24             // the pane radius falls back to this
            }
        }
        material {                                // named materials (RFC material, step 4)
            glass {
                color (rgba)"#05050840"
                frost compression=(f64)0.6 refraction=(f64)0.3 radius=(f64)5.5
                finish light=(f64)0.15 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2
            }
        }
        relief light=(f64)0.15 width=(f64)9.3 spec=(f64)0.4 shininess=(f64)24.0 curvature=(f64)0.2 shader=(bool)true {
            // light: the strength, NOT a length; width: the ONE run of every roll and wall;
            // spec / shininess / curvature: the DE finish beyond its strength;
            // shader: false = the legacy banded edge shading, for A/B comparison
            wall height=(mm)0.3 profile="smooth;…"   // a carve's side (buttons, wells, rows): height = drop, profile = ramp spec
            edge height=4.0    profile="smooth;…"   // a plate's perimeter roll: height = rise (unset = quarter-round of width)
        }
        menu color=(rgba)"#101018ff" opacity=(f64)0.06 compression=(f64)0.8 corner_radius=(i64)24   // every popover and the designer's dialog
    }
}
```

Spellings that are NOT current, and what the loader does with each:

| Spelling | Status |
|---|---|
| `param.color` (+ top-level `plate_opacity`) | alias of `plate.pane.color`, multiplied by the opacity line |
| `plate.blur` / `.radius` / `.backdrop_compression` / `.refraction`, `frost.backdrop_compression`, `plate.bevel_width`, `relief.depth`, a material's `finish depth=`, `relief.height` / `.profile` / `.edge_height` / `.edge_profile`, `window_manager.bevel_depth` / `.bevel_width` / `.bevel_shader` | RETIRED: reported by path (`color::retired_surface_keys`), not read; cce-relief seeds from each once and its Save writes the current spelling and removes the old (the shader toggle it carries across as `relief.shader`) |
| `relief.wall.knobs` / `edge.knobs`, `profile_knobs` / `edge_knobs` | not style: cce-relief's own state (`~/.config/cce/cce-relief/state.kdl`); read once as a seed, removed on its next Save |

Where each rule is argued, by its lead-in: **Frost is one block** and
**Named materials in config** above; **The relief is two shapes**, **The
editor's knobs are not a style key** and **There is one roll width** under
"Units" (the geometry is unit-aware, which is why they sit there); the
`menu` block under "The context menu draws in its own popup surface".
There is no alias precedence to decide any more — every legacy spelling of
the relief is retired, so a file is what it says — and
`color::retired_surface_keys` is the one place a retired key is named;
cce-relief's Save is the migration for all of it: it seeds from a retired
key once, writes the current spelling and removes every superseded one it
finds.

## The standard app — root plate, rungs, and the spacing ladder

Every cce app is built the same way, and this section is the standard.
`scripts/style-audit` checks the sibling app crates against it (one row per
app; `--strict` fails on any off-standard row); `src/main.rs`, the demo, is
the reference implementation.

**Anatomy.** A window is a **root plate** with things standing on it. The root
plate is the first prim of every frame — `pc.root_plate(w, h)`, which emits
`PlateSpec::window(w, h)`: the root rung's material (`Material::root()`, the
DE's `style.surface.plate.root.color` at its opacity unless a `material=` is
bound), all four corners on the shared silhouette, the perimeter rolled over
`bevel_width`. Nothing else paints a window base — not a quad, not a rounded
rect at the silhouette radius, not a stroked border. On the root plate stand
**pane plates** (`PlateSpec` with the corners that touch the window edge
flagged, or `plate` / `rounded_rect` at `plate_corner_radius`) and **carves**
(`inset_plate`, `recess_edges`: a menubar or status band stepping down into
the surface, a well you type into). Which to use is the vocabulary above:
things you press and content you read sit on plates; things you enter and
bands that are part of the window's own surface are carved. On the pane
plates sit **control plates**. A window that is deliberately not a plate — a
transparent bar whose modules are the plates, a notification stack, a black
lock or screensaver surface, the desktop grid overlay — says so with a
`// style-audit: opt-out <reason>` comment and is listed as an opt-out.

**Spacing is a ladder, and an app never names a number.** Three rungs, each a
config key read through the style registry (so a nested KDL key works and
live-reloads), each with a getter in `layout.rs`:

| rung | inset from the rim | gap between siblings |
|---|---|---|
| root plate | `root_plate_inset()` = `bevel_width` + `style.surface.plate.root.padding` | `root_plate_gap()` (`…root.gap`) |
| pane plate | `plate_padding()` (`style.surface.plate.padding`) | `plate_gap()` (`…plate.gap`, unset = the root gap) |
| controls | — (inside a pane or root inset) | `control_gap()` (`style.control.gap`, unset = `CONTROL_GAP`) |

The root inset carries the roll because the padding is a run of FLAT face —
the same run the gap leaves between two panes — and the face only begins
where the roll ends; a bare padding at a window edge measured 4px of visible
flat against 12 between panes. The legacy keys (`page_margin`, `column_gap`,
`control_panel_{padding,gap}`) are honoured when set and land on their rung
when unset, the radius rule applied to spacing; do not add a new one.

In the box model the ladder is presets — `Style::root_column()` /
`root_row()`, `pane_column()` / `pane_row()`, `controls_column()` /
`controls_row()` — and the legacy strategies' `Default`s and
`ColumnLayout::pane` / `controls` read the same getters. An app picks the
rung; a literal padding or gap in an app (`const PAD`, `+ 12.0`) is a number
the ladder should be supplying, and the audit counts them.

**Rules, restated as the audit checks them:**
- The first prim of the frame is `root_plate(w, h)`, or the crate declares an
  opt-out.
- Every inset and gap comes from a rung getter or a preset; the app declares
  no spacing constants of its own.
- A deliberate deviation — the system settings' own tint, an overlay's
  shallower roll — goes through `PlateSpec::window(..).with_material(..)` /
  `.with_depth(..)` and a comment saying it is one, never a hand-built spec.
- Migrating an app is pixel-neutral for the plate (`tests/plate_golden.rs`)
  and a measured change for spacing: screenshot in a shadow session, count
  the columns of flat face at the edges and across a split, and they match.

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
- `heightfield.rs` — the relief as a height field: the geometry the plate shader shades
  (plate rolls, CSG features, free carves) integrated back from the slopes it lights,
  sampled per physical px and exported as a 16-bit PNG + JSON sidecar in millimetres
  through the display metric. `CCE_HEIGHTMAP=<file>` in any client's environment, or
  `heightfield::request` from an app. See the Units section.

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

- `src/layout.rs` (largest file, ~6.9k lines) — fonts + sizing; many `*_font_parsed()` getters and
  the `read_preferred_fonts` / font-family resolution used by the cosmic-text path.
- `color.rs` — color model and named colors (`colors` re-export module in `lib.rs`).
- `config.rs` — KDL loading and `kdl_to_json` conversion (see workspace `CLAUDE.md` for paths).
- `context.rs` — `UiContext`: the retained widget tree, event routing, spatial grid, dirty
  tracking, hit-testing.
- `history.rs` — `History<T>`: the undo/redo snapshot stack (cap, gestures, grouped runs).
  The toolkit defines the stack and the routing, never the step — see the trait section.
- `widget/` — `container/` (vbox/hbox/scroll/menu/treelist/…), `input/` (button/slider/text_box/
  dropdown/…), `display/` (label/graph/svg/…), plus `editor.rs` and `core.rs`. (The
  KDL/JSON-driven `json_layout.rs` is dissolved; `scene/layout.rs` is the box model.)
- `protocol.rs` — inline-generated Wayland protocol bindings.
- `ipc.rs` — the `/tmp/<prefix>-<WAYLAND_DISPLAY>.sock` helpers (`socket_path`, `send_command`).
- `icon.rs` — XDG icon-theme lookup: a `.desktop` `Icon=` key (or an SNI tray icon
  name) → a file on disk, plus `upload_themed` to rasterize/decode and upload it.
  **Not** `lib.rs`'s `upload_icon`, which loads a *bundled* cce-icons glyph by its
  own name for in-widget use; this one resolves names any installed app may ship.
- `file_dialog.rs` (rfd), `scale.rs` (HiDPI), `wayland.rs` (surface/scale detection, and
  `detect_metric` — the display's logical px per mm from its `wl_output` geometry).
- `units.rs` — lengths with units and the display metric; see the Units section below.

## Markdown: `MarkdownView` and `DocEditor` (features, 2026-10-01)

Two opt-in features for the clients that show notes (Obsidian-on-cce):

- **`markdown`** — `widget::markdown`, the reading view: `cce_vault`'s
  blocks laid out at a width into draw items and click targets
  (`layout`, `Layout::paint` / `paint_scaled`). cce-notes' reading mode
  and cce-grid's note cards draw through it. Brings in `cce-vault`.
- **`doc_editor`** — `widget::doc_editor::DocEditor`, the editor with
  Markdown **live preview**: markup is hidden except on the caret's lines
  (the selection's, or the whole fenced block the caret is in), where it
  shows dimmed; `preview = false` is source mode. No extra dependencies.
  - `buffer` — lines, caret/selection as (line, byte), edits with merged
    typing/deleting undo runs, and a log of `Change`s for the layout.
  - `preview` — styles ONE line: block kind (heading, list, task, quote,
    rule, code, fence, frontmatter, table) plus inline segments that map
    1:1 onto source bytes. Markup is never replaced, only hidden, so
    caret maths never translates between screen and source.
  - `layout` — one styled line wrapped into runs, with the x of every
    byte (`ShapingMeasure::offsets`), so drawing, caret and clicks agree.
  - **Incremental:** a line is shaped only when it is drawn and has
    changed; undrawn lines keep an estimated height. A 5000-line note
    shapes one screen (`a_long_document_shapes_only_what_shows`).
  - **Host-driven, not a registered widget:** the app forwards keys,
    presses, motion and the wheel and paints it (`prepare` then
    `paint_prepared_with`, which takes a link resolver so unresolved
    links fade without a relayout). Answers come back as `Response`
    (`Follow(Target)` for a rendered-link click or a Ctrl+click). Undo
    and redo are the host's `Application::undo` / `redo` hooks calling
    `DocEditor::undo` / `redo` — the runner routes the chord there
    because the editor is not a focused widget.
  - Measure with the app's own font set: `DocEditor::new(.., system_fonts)`
    must match `Application::load_system_fonts`, or widths are not drawn
    widths. `widget::shaping` holds `Measure` / `ShapingMeasure`, shared
    with the reading view; a width includes trailing spaces (max of glyph
    x + w), which is what a run placed after it needs.
  - **Frontmatter is the Properties table** while the caret is outside
    it (`preview::properties` gives each line a role, `style_property`
    its row): a "Properties" header, keys in a column as wide as the
    reading view's, values inline-styled (links follow), list values as
    pills — a one-per-line YAML list is one pill per line, its key drawn
    by the first item and the key line itself zero height — and
    `true`/`false` as a checkbox that flips the bytes in place
    (`LineLayout::toggle`, undoable). The caret anywhere in the block
    shows all of it raw, like a fenced block; an unclosed block, nested
    maps and block scalars stay raw. `set_text` starts the caret past the
    block so a note opens on the table.
  - The caret does not blink (a blink is a frame every half second for
    as long as the window is open).

## A graph's wires are strokes in a style (since 2026-09-30)

`Graph` draws its wires in one of four `WireStyle`s: **orthogonal** (down,
across at half the height, down — what every wire was), **rounded** (the
same with the two bends rounded, the radius at most half a node's height),
**bezier** (a cubic that leaves the output and reaches the input heading
down, so a wire back up the graph loops) and **straight**. The style is
`style.surface.graph.node.wire_style` unless the host sets one
(`Graph::set_wire_style`, `None` to follow the config again). Colour and
width are `wire_color` and `wire_size` (px at 100%, scaled with the node
body) in the same block — both parsed since long before and READ BY
NOTHING until this change, when the wires were a hard-coded cyan 3 px.

- **The wires are not in `geometry_quads_tagged` any more.** They are
  `Graph::paint_wires` (also on `GraphController`), `Prim::Vector`s and
  `Prim::Arc`s, since only one style is axis-aligned. `Paint::paint` calls
  it after the grid; a host drawing the quads itself (the designer) calls
  it between `paint_grid` and the bodies. The legacy `extra_quads` view has
  no wires.
- **One path, drawn and hit**: `wire_path` derives each style's pieces, and
  the splice hit test (`splice_wire_at`) walks the same pieces against the
  dragged ghost, so a drop lands on the wire as drawn in any style.
- **The orthogonal joins do not overlap** — the across run is widened by
  half a thickness to fill the corners and the down runs stop at its edge —
  so a translucent wire is one alpha throughout. A bezier is flat-capped
  pieces about 6 px of control net apiece, fine enough that no notch shows
  at the joins.
- **A wire may be thinner than a pixel** (the same day). `wire_size` has no
  floor; what is drawn does (`wire_stroke`): one DEVICE pixel, half a
  logical one at 2x, since the 2D pass has no antialiasing and a narrower
  axis-aligned quad covers a row of pixel centres or none. A wire under
  that is the pixel at the share of it the wire covers, so it reads
  thinner by reading fainter. Until then the stroke was clamped to one
  LOGICAL pixel, two device pixels at 2x, and `wire_size` below 1 did
  nothing.

`every_wire_style_runs_from_port_to_port` and the splice test, run in all
four styles, are the tests.

## A node has as many wires as the host says (since 2026-09-30)

`Graph::wire_pairs` draws a wire for EVERY parameter a host types `node`,
the k-th into input port k (`node_wires`, public so a host can read the
rule back) — a Switch's four inputs, a Boolean's With, a Transfer's From.
Until then it drew one, the parameter NAMED `input`, so every second
operand was a real connection with no line. A host that types no parameter
`node` keeps exactly that (cce-files and cce-graph pass `("input", name,
"string")`), so nothing changed for them. An empty value is a port with
nothing wired; a wire past the node's ports lands on port 0.

A connection the pointer makes reports its port too:
`GraphController::take_pending_connection_to_port` gives (input node id,
output node name, port), and the old `take_pending_connection` — which
takes the same connection — is the default for hosts that do not care.
Splicing a dragged node onto a wire takes only a wire into port 0, since
the splice rewires Inputs. `every_node_parameter_is_a_wire_into_its_own_port`
is the test.

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

**The relief is two shapes, and the config says which (2026-09-28).** A
**wall** is a carve's side — a recess, boss, ridge or trough cut into a
surface: buttons, wells, text boxes, the rows of a params pane — shaded as a
translucent light-and-shadow overlay on whatever is under it. An **edge** is
a plate's perimeter roll, the face curving down to its silhouette, shaded as a
multiply on the plate's own fill plus a specular crest. Each is a node under
`style.surface.relief` with the same three keys:

```kdl
relief light=0.15 width=9.3 {
    wall height=(mm)0.3 profile="smooth;…"
    edge height=4.0    profile="smooth;…"
}
```

`width` (the run of both, one number — see the roll-width note below) and
`light` (how hard the light falls across either shape — NOT a length, it is
`bevel_depth` → `Finish.strength`; `depth`, what every config said until
2026-09-28, was its alias for the rest of that day and is retired — reported
by path, not read, seeded from once by cce-relief whose Save writes `light`
and takes `depth` off) stay on the node itself, since both shapes share
them. `height` is a length — the wall's drop, the edge's rise —
and `height=(mm)0.3` is honest geometry resolved through the metric; unset, a
carve drops `relief_shade::RECESS_DEPTH` (0.6) of its wall (saturating at the
DE roll width) and the roll is a quarter-round of radius width, the look every
config had. `profile` is the curve as a ramp spec (absent or the identity
sentinel = the analytic curve: smoothstep for a wall, the superellipse
quadrant for an edge). **`shader`** on the relief node is the A/B switch
for how every one of those edges is LIT: `shader=(bool)false` renders the
relief prims through the legacy banded vertex shading instead of
shader2d's per-pixel SDF-lit branch (`layout::bevel_shader`; the registry
key keeps the bevel name because it selects how the shared lit edge is
computed, not which shapes exist). It was `window_manager.bevel_shader`
until 2026-09-28 — the block the relief keys were born in — and only a
number ever switched it, since a `(bool)` flattens to the string "false"
that the float read never saw; `shader_on` reads both now, and the old
spelling is retired with the block's other bevel keys (cce-relief's Save
carries a value it finds across). The registry keys never moved —
`bevel_depth`, `bevel_width`, `bevel_shader`, `bevel_height` /
`roll_height`, `bevel_profile_spec` / `roll_profile_spec` — so nothing
downstream of the registry knows. Until 2026-09-28 the keys were flat on the
node with the wall UNNAMED (`height`, `profile`) and the edge prefixed
(`edge_height`, `edge_profile`), which read as one shape with an "edge"
variant rather than two shapes; those spellings were aliases for the rest
of that day and are RETIRED — not read, reported by path with the other
retired surface keys, reaching no registry key (the flatten test asserts
it). While they were aliases a `prefer_relief_spellings` pass dropped a flat
one whenever its node spelling was present, because two spellings of one
registry key were otherwise decided by line order; with nothing left to
prefer, the pass is gone. `cce-relief` seeds from a flat key once, and its
Save writes the node spellings and REMOVES the flat ones
(`config::remove_config_value`), so a file migrates the first time it is
saved; `wall` and `edge` are `PROP_NODES` members so their keys land as
properties.

**The editor's knobs are not a style key.** cce-relief's Shoulder / Base /
Bias triples — the slider positions behind each profile spec — rode in the
style block as `(bevel)`-typed keys (`relief.wall.knobs` / `edge.knobs`,
before that `profile_knobs` / `edge_knobs`) so the editor could reopen where
it was left: editor state beside the values that draw, and the one relief
key nothing but the editor read. They live in that app's own
`~/.config/cce/cce-relief/state.kdl` now, one `knobs` node per Save target
(`shared`, a retargeted file's path, or `<path>#<key>` for a `(relief)`
value — two materials must not seed each other's sliders; `knob_state` in
`cce-relief.rs`). The registry keys `bevel_profile_knobs` /
`roll_profile_knobs` and their flatten arms are gone; a config that still
carries a knob key seeds the editor once, off the raw file, and the next
Save takes the key off under either spelling. The `(bevel)` type and
`parse_bevel_knobs` stay, because a `(relief)` value still carries its own
`k=` ride-along and the data editor's preview of such a value draws it. `layout::carve_depth_px` states the drop rule
once for the tessellator's CSG features and, through `WindowInfo.relief_meta`,
the shader's free carves; `carve_depth_ratio` / `roll_height_ratio` feed the
shading twin (`Finish.carve_depth` / `roll_height`). A `(relief)` value
carries the drop as `h=` (a length: `h=0.5mm`, or bare px) beside `w=` and
`d=` (light; `l=` reads as an alias). `cce-relief`'s Height knob is the editor:
its section's depth numbers read in mm when the metric is real, and Save
writes `wall.height` in the unit the config already spells (an untouched
slider verbatim, a moved one converted through the same metric that seeded
it — so a headless session never turns `(mm)0.3` into `(px)1.1339`, which
it did until 2026-09-28), choosing a unit only for a height the config
never had: `(mm)` when the metric is real, px otherwise
(`height_len_for` in `cce-relief.rs`).

**There is one roll width.** `style.surface.relief.width` is the run of every
roll and wall: the root plate's perimeter (`PlateSpec::window`), a `PlateSpec`
pane plate, a bordered widget plate under relief (`append_widget_plate`, via
`colors::plate_bevel_width`), every control wall, and the length
`edge.height` is a rise against. Until 2026-09-28 the widget-plate path had a
width of its own — `style.surface.plate.bevel_width`, default 6 against the
relief's 9.3 — so two pane plates in one window rolled over different widths
depending on which painter drew them, and no single key made a pane match the
window lip. `plate_bevel_width()` now returns the relief width and nothing
else: the old key was an explicit override for the rest of that day (so a
config carrying it kept its look through the change) and is RETIRED — a
config still carrying it is reported by path with the other retired surface
keys and the key is not read, since an override that keeps working is a
second width by another name. `the_pane_roll_is_the_relief_width` pins it,
and cce-relief's Save removes the key.

**And the relief can leave the screen.** `scene/heightfield.rs` integrates the
height curves the shader only differentiates and samples a frame's plates into a
height field — plates stack, carves etch, exactly the composite model the shader
lights — then writes it as a 16-bit PNG whose sidecar carries the pitch and range
in millimetres via the metric. A pinned `height=(mm)0.3` is 0.3 mm in that file.
The sidecar states the metric's source; on an assumed metric the millimetres are
a guess, and a fabrication tool should say so.

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
`~/Dropbox/Fonts`); bundled icons from `$CCE_ICONS_DIR` (else `~/projects/cce/cce-icons/svg`).
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
- `CCE_UI_MENU_POPUP=0` — keep the context menu in the window instead of its popup
  surface (see "The context menu draws in its own popup surface").
- `CCE_VK_DEVICE=<substring>` — force a physical device; `CCE_VK_RT=0` disables ray tracing.
  `integrated`, `discrete` or a name substring; any of them also lifts a session-wide ICD
  pin (`VK_DRIVER_FILES`) for the process. **A preference the chosen device does not meet
  is printed to stderr** (since 2026-10-02, once per process, `unmet_device_preference` in
  `vk/core.rs`): what was asked, what was taken, and every device the loader offered — a
  driver that failed to LOAD is in no list, which is the case the line points at
  (`VK_LOADER_DEBUG=error` says why). Until then the fallback was silent, and a fallback
  renders exactly as the asked-for device would, so nothing on screen gave it away.
- The cce-ui suite reads `~/.config/cce` through the style registry, and LAZILY: a value
  read before the first load and one read after come from two configurations. A test
  asserting on shading numbers pins its inputs instead (`relief_shade`'s tests:
  `pinned_light`, `pinned_finish`); `deeper_carve_shades_harder` failed run alone and
  passed in the full suite until it did.
- `CCE_FORCE_SCALE=<f>` — override HiDPI scale detection.
- `CCE_FORCE_PPI=<f>` — pin the display metric (logical px per inch) regardless of what
  the outputs report; a headless shadow has no EDID and would run `assumed`. The live
  panel is 141.8.
- `CCE_HEIGHTMAP=<file.png>` — export the client's third rendered frame as a relief
  height field (16-bit greyscale PNG + `<file>.json` sidecar: pitch, range in mm, datum,
  metric source); `CCE_HEIGHTMAP_MM=<mm>` resamples to that pitch. `scene/heightfield.rs`.
- `CCE_UI_FAULT_RECONNECT=<seconds>` — drop the session that many seconds after it
  starts, exactly as a transport error would, so the reconnect path (and the second
  `renderer_init`) can be exercised on demand instead of waited for. One-shot per
  process: the app reconnects and then stays up. A float, so `0.5` works; logs
  `CCE_UI_FAULT_RECONNECT: dropping the session` at WARN. In a shadow, note that the
  window MOVES across the reconnect (off-view recall), so capture with
  `shot-window <id>` and re-read `ctl windows` before any pointer work.

### The params pane stacks its labels when it is narrow

`ParametersBg` has two label layouts: **inline**, the pane's own column of
labels beside unlabelled controls, and **stacked**, each control carrying
its label above itself. `param_label_layout` in the style config is the
PREFERENCE (`stacked`, or inline by default). Since 2026-09-28 the rows
decide on top of it: an inline pane whose label column would leave any
visible row's TRACK shorter than `MIN_INLINE_TRACK_W` (120 px) stacks, and
takes the column back when there is room. The track is what is measured —
the control's rect less its chrome (`control_chrome`: a slider's 60 px
readout and its gap, plus a float3's axis column) — because the first cut
measured the rect and let a slider's track shrink to 52 px before the
labels moved; a control with no track is measured whole. One decision for
the pane, taken by its shortest track. The rows' widgets are built with or
without their label and the two layouts have different row heights, so the
flip is a relabel and a re-layout on every metrics refresh — a rect
assignment, a rebuild, a section collapsing (`apply_label_layout` /
`relabel_rows`) — not a flag; `Adapted::clear_label` is the way a label
comes OFF a widget, and `slider::detached_strip` reads an empty label as
none for the widgets that store whatever they are handed.

### A value control reads the wheel as "up is more", a natural finger too

`MouseScrollDelta::value_notches_y` (2026-09-30) is what a VALUE control —
`Slider`, `Slider2D`, `Spinbox`, a context menu's slider row — turns by:
a wheel notch up is positive, and a finger's pixel delta is taken as it
comes with natural scrolling off and NEGATED with it on
(`input::natural_scroll`, input.kdl's `trackpad { natural_scroll }`, read
once). The delta the runner hands out is what a LIST scrolls by, and a
natural list moves its content the way the fingers went; a value has no
content to move, so under natural scrolling the fingers going up is
more. Until then each control read `notches_y` with a sign of its own:
the slider was right for a natural trackpad and backwards for a wheel,
the spinbox and the menu slider the other way round. Under `cfg(test)`
the toolkit's suite reads natural as off; `force_natural_scroll` sets it
per THREAD for a test that drives a control with a finger — a dependent's
test binary links cce-ui without `cfg(test)` and would otherwise read the
machine's. `a_value_control_reads_up_as_more_on_a_wheel_and_a_natural_finger`
is the test.

### A flick coasts with animations off

The animations switch (`motion::enabled`) stops a wheel notch's GLIDE and
not a trackpad flick's COAST (`scroll_motion::with_animations`, since
2026-09-29). Until then it turned both off, so on a power mode with
animations off a two-finger scroll stopped dead at the lift in every
pane that scrolls through `ScrollMotion` — lists, the params pane, the
spreadsheet, a graph's pan. The glide is an animation the toolkit adds;
the coast is the rest of a gesture the hand made, and its own setting is
input.kdl's `kinetic_scroll`. Two value controls still follow the switch,
deliberately left alone: a slider's and a ramp key's drift after a scroll
over them, which changes a VALUE after the hand has stopped.
`the_animations_switch_stops_the_glide_and_not_the_coast` is the test.

### A host may name the phase; a test may pin the settings (2026-09-30)

The phase a wheel event belongs to (`Finger`, `FingerEnd`, `Wheel`) is a
process GLOBAL the runner publishes before each dispatch, and
`ScrollMotion::apply_px` reads it. So does anything a test would set it
through — which, in a suite running tests in parallel, changes what every
other test's pixel delta means. `ScrollMotion::apply_phase` takes the phase
as an argument (`apply_px` is it with the published one), for a host that
reads the phase itself and hands it on; cce-designer's viewport does, and
its test drives a flick without touching the global.

`scroll_motion::force_scroll_settings` pins what `scroll_settings()`
answers on the calling THREAD, over input.kdl and the animations switch
alike, as `force_natural_scroll` pins natural scrolling: a dependent's test
binary links cce-ui without `cfg(test)`, so a test whose result hangs on a
coast would otherwise pass or fail by the machine's `kinetic_scroll`.

### The suite's animations switch is its own

`motion::enabled()` reads `/run/cce/animations` in a shipped binary (see
"Animations switch" in `../cce-compositor/WORKSPACE.md`). Under `cfg(test)`
it does NOT: it answers on, or whatever `motion::force_for_test` set on the
calling thread. Until 2026-09-28 the test binary read the machine's file,
so the spreadsheet's wheel-glide tests and the scroll region's fade test
passed on mains and failed on battery — the same lesson as cce-designer's
pinned settings path and lattice, one layer down. A test wanting the
snap-instead-of-ease path forces it for its thread; nothing sets
`CCE_ANIMATIONS`, which is process-wide and would race the parallel suite.
This pins only cce-ui's own suite: a dependent's test binary links cce-ui
without `cfg(test)`, so a dependent test that eases still follows the
machine — none does today.

### A colour test that reloads a knob puts it back

`color::style_write` (every `set_*`) is a per-thread overlay under
`cfg(test)`, but `reload_colors` writes the process-wide globals — and an
absent frost knob KEEPS its last value by design, so `reload_colors("")`
resets the named materials and bindings (replaced wholesale) and nothing
else. A test that reloads `frost radius=3.0` and closes with the empty
reload leaves radius 3 behind for every test after it; that is what had
`frost_from_style_and_flag` fail one run in eight (2026-09-28, the radius
read 3.0 for the default whenever its neighbour ran first). Two rules: a
test that ASSERTS a knob pins it on its own thread through the setter, the
radius included, not just the ones it is about; and a test that RELOADS a
knob reloads its default back before the empty reload, and asserts the
globals are back. `test_color_state_lock` orders the reloaders against
each other; it cannot undo what one of them left behind.

### A scroll gesture in the params pane belongs to what it begins on

`ParametersBg`'s wheel arm gives a gesture to the VALUE CONTROL it begins
on and to the pane otherwise, for a wheel notch and a trackpad finger
gesture alike: a slider and each row of a float3 by the band's halo
(`Slider::scroll_hit`), a spinbox by its row. The control that acquired it
keeps it until the gesture ends (`scroll_initiate_widget_id`, 250 ms of
quiet), so the latched control is asked first and a gesture the pane or
another control owns never lands on a band that slides under the pointer.
From 2026-09-21 to 2026-09-28 every FINGER gesture was the pane's from
anywhere, on the reasoning that a pane of mostly controls was scrollable
only from a label; that reasoning was about the designer's Alt+D Settings
list, which stopped being a `ParametersBg` on 2026-09-24, and what the
rule left behind was sliders a trackpad could not turn. The pane still
scrolls from the label column, the gaps between bands, and every row that
is not a value control.

### A slider's notch follows the value on a wide range

One wheel notch or arrow press moves a `Slider` by `notch_step`: 2% of the
range for a range up to `FINE_SPAN` (20) wide, which leaves every slider
the toolkit had exactly as it was. A WIDER range steps 2% of a span that
grows with the value's magnitude — 20 times it, floored at 20 and capped
at the range — so -1000..1000 moves 0.4 a notch near zero, 4 at ten, and
the old 40 only from a hundred up. By magnitude rather than a finer flat
step: one fine enough to set 0.06 takes thousands of notches to reach
1000, and this takes under sixty. A drag still maps the pointer to the
whole range; the readout is still where an exact value is typed.

### A float3 group can carry a trackball

`Float3::set_trackball` (a `ParametersBg` row typed
`float3:lo:hi:trackball`) stands a ball left of the three rows, as tall as
they are. The vector is drawn on it from the centre — X right, Y up, Z
toward the viewer; bright on the near side, dim pointing away — and
dragging the ball rolls it under the pointer, turning the vector and
keeping its length. The rows stay: a direction is turned on the ball, a
component typed or a length changed on a row. Three things to know. The
drag turns its OWN full-precision copy (`BallDrag`), because the rows hold
the vector rounded and a host writes the rounded string back between
moves; turning that loses every step smaller than a readout tick. With
the ball on the rows read to three decimals, since at two a vector of
length 0.06 has seven directions. And the ball is painted by
`paint_ball` through the host's scene path (`paint_scene_rows`), not
`Paint::paint`: a sphere is not a prim the legacy flat views carry. A
vector of no length is given a length of one by the first drag. The ball
counts as chrome in `ParametersBg::control_chrome`, so the label layout
is decided by the track it leaves.

**The ball can be seen from a host's camera** (`Float3::set_view`,
`ParametersBg::set_trackball_view`): three rows, the camera's right, its
up and the direction toward it, in the vector's space. The vector and its
rings are then drawn as the host's 3D view shows them, and a drag or a
scroll rolls about the CAMERA's axes — pushing the ball right swings the
vector to the right of the screen, whatever that is in the scene. Only
what the ball shows and how it turns: the rows and the value stay in the
vector's own space. Without one the view is X right, Y up, Z toward the
viewer. The pane keeps the view for rows built later.

**The ball carries rings** (`Float3::ring`, `RING_ANGLES`): five circles
of latitude about the vector as their pole, thirty degrees apart, the
near half of each drawn in short strokes. A lit ball is the same from
every side, so without them a drag showed the vector move and the ball
stand still; the rings are the vector's own, concentric circles when it
points at the viewer and foreshortening into ellipses as it turns, which
is how a rotation is read. They follow the full-precision direction a
drag or a scroll is turning, so they move on every pixel.

**A scroll rolls the ball** (`ball_scroll`), as content is scrolled: its
surface moves the way a page under the pointer would, a two-finger
gesture in both axes at once and a wheel notch in one, by `SCROLL_TURN`
(15 degrees) a notch — the fine handle beside the drag's 1:1. The ball
has an id of its own (`ball_id`) in the scroll-gesture bookkeeping, so a
gesture that begins on it is the ball's until it ends and one a band
holds stays the band's over the ball; `wheel_zone` / `wheel_latched` are
what the pane asks. A scroll keeps its own full-precision copy too
(`fine`), reused while the rows still hold what it rounds to: a trackpad
sends a pixel at a time, a quarter of a degree, which on a short vector
is less than the rows can hold.

### A spreadsheet's rows can be selected

`Spreadsheet` keeps a selection of rows (since 2026-09-29): a press on a
row selects it alone, or clears it where it was the whole selection; with
ctrl the press toggles that row and leaves the rest; with shift it selects
the run from the last row pressed without shift to this one. A press on
the empty body under the last row clears. The host pushes the modifiers in
ahead of the press (`set_modifiers`), as it does for every widget.

**The selection is of rows of the DATA, not of places in the pane.** A
sort moves where a selected row is drawn and not what is selected, and a
shift run is the rows DISPLAYED between the two, which under a sort is
what the eye sees. It stands across `set_spreadsheet_data`, less the rows
a shorter table no longer has: a host that re-sets the table on every
frame of a playback keeps its selection, and one whose table has become
something else clears it itself.

`SpreadsheetController` carries it: `selected_rows` (ascending),
`set_selected_rows`, and `take_selection_change`, which is true once after
anything changed the selection. A press on a scrollbar is the drag
surface's and selects nothing (`body_row_at`). Selected rows wear
`highlight_primary_color` at 28% over the zebra.
`rows_select_alone_toggled_and_in_runs` is the test.
