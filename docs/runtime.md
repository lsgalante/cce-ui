# Runtime: the run loop, sessions and input

How the runner behaves, what a client may rely on, and the input paths that are not plain
pointer-and-key. Present-tense reference; history is in `CHANGELOG.md`.

## The loop and pacing

- **One turn** of the loop is `backend::shell::Pacer::turn` over a `Shell` (the window system's
  side: exit, size requests, per-turn sync, title, the frame gate, present). The pacer owns the
  tick's `dt` and its idle clamp, `desired_size`, key repeat, the title, the present-or-warm-down
  decision, and the ACTIVE / idle cadence. Every shell (Wayland, browser, macOS) drives the same
  pacer from its own loop.
- **Demand-driven.** A frame is drawn only when something set `needs_rebuild`, an animation
  reported a change, a key is held, or a warm-down is running; the Wayland shell gates on the
  frame callback. Idle, the runner sleeps up to `IDLE_DISPATCH` (1 s; `CCE_UI_IDLE_MS`) and wakes
  on Wayland events and `AppSender::send`. Something the loop cannot see is polled by a widget
  returning `true` from `tick` while live (the colour selector's picker) or by
  `Application::idle_poll_interval` (cce-authenticator, cce-system-interface, cce-designer while a
  pane is detached).

## Frames and damage

`backend::frame::build_frame` turns the app's display list, custom vertices, overlays, widget
shaping, text and popover-occlusion rects into a `BuiltFrame`. The Wayland shell presents it: grid
patch, input region, glyph upload, extent gate and buffer scale, frame callback,
`stage_renderer`, draw.

**Damage** (`backend::frame::derive_damage`): for an app that does not report its own
(`take_damage`), the shell diffs the tessellated batches (vertex bytes, scissor, clip, plate push,
blur flag), the display-list text (text, position, colour, size, clips) and the image quads
against the previous frame, and damages what changed — where it was and where it is — over the
common prefix and suffix. Ids whose pixels `update_pixels` / `update_pixel_regions` replaced are
damaged where drawn. It falls back to a full frame on a new size, scale or clear colour, changed
plate carves, a frame owed after a skipped present, an app staging its own text
(`display_list_text` false), and a 3D backdrop.

**Frost and partial frames.** A frosted plate that the region touches is repainted whole plus
`BLUR_REACH_PX`, repeated until nothing more is touched (its blur reads a snapshot valid only
inside the region). The first-drawn frosted batch over a transparent clear with no scene — the root
plate of every themed app — samples the zeroed backdrop instead of a snapshot
(`first_frost_exempt`). `write_window_info` marks every image stale. `CCE_UI_FULL_DAMAGE=1`
disables derivation; `CCE_PRESENT_DEBUG=1` logs each rect.

## Sessions, reconnects and GPU handles

A connection is one **session**. A broken Wayland transport cannot be repaired, so `run` opens a
new session around the same live `Application`: same app state, calloop loop and channel; a new
surface, swapchain and **`VkRenderer`**. `renderer_init(&mut self, renderer)` runs once per
renderer — the first is the process's own, every later one a replacement.

An id from `vk::upload_rgba` names an entry in one renderer's image table, and `Frame2D` skips an
unknown id without logging. So an image id cached across frames — in a field, an LRU, a static —
silently stops drawing after a reconnect while text and numbers keep working. The fix shape:

```rust
fn renderer_init(&mut self, _r: &mut cce_ui::vk::VkRenderer) {
    if std::mem::replace(&mut self.seen_renderer, true) {
        // drop the dead ids and arrange for the pixels to be produced again
    }
}
```

Act only on the second and later renderer (uploads queued before the first are drained into it).
Freeing a stale id is always safe: `ImageStage::destroy_image` ignores an id it does not hold, and
`NEXT_ID` never resets, so a stale id can never collide with a live one. The failure is always
"draws nothing", never "draws the wrong picture". Three traps:

- **A widget can hold the id too.** `Button::with_icon(id, …)` captures it. For bundled cce-icons
  artwork use `Button::with_icon_name` / `Button::new_icon`, which re-resolve through
  `upload_icon` (cached per `vk::renderer_epoch()`). `with_icon` means the app owns the upload and
  must re-set it; `ImageView` borrows its id on the same terms.
- **A WPE client does not self-heal.** A finished page provokes no repaint. cce-mail replays
  `MailWebView::last_frame`; cce-browser remaps the active view. (An animating page recovers by
  itself — which is how this hides from the page you test with.)
- **An in-flight worker result can carry a dead id.** cce-preview's `PageStore` and cce-map's
  `TileManager` carry a generation and free a mismatched result on arrival.

Verify with `CCE_UI_FAULT_RECONNECT=<seconds>` (drops the session once, as a transport error
would; logs a WARN) — and run the pre-change binary through the same fault first. In a shadow
the window moves across the reconnect, so capture with `shot-window <id>` and re-read
`ctl windows` before pointer work.

**A lost surface ends the session; it never panics.** `VkRenderer::try_new` returns
`vk::SurfaceLost` when the display connection under the surface is already dead (Mesa's WSI finds
out at `vkGetPhysicalDeviceSurfaceFormatsKHR`); the runner ends that session as `ConnectionLost`
(reconnect if the compositor is there, clean exit if not). Mid-session, a swapchain rebuild,
acquire or present that reports the surface lost latches `surface_lost()` and skips draws until
the loop sees the dead connection. `try_new` is the only constructor.

**When the compositor is gone** (`SessionEnd::NoCompositor`) the runner exits by default — the
compositor saves windows and its successor respawns them. A process the compositor does not
restore (a systemd user service: the status bar, the notifier) returns true from
`outlives_compositor`; the runner then waits for the successor's socket
(`await_compositor_socket`, 250 ms poll) and rejoins with the same `Application`.

**A layer app can drop its surface while empty** (`Application::wants_surface`). On a false turn
the runner detaches the renderer (`VkRenderer::detach_surface`: swapchain and `VkSurfaceKHR` go;
device, pipelines, atlases and image table stay) and drops the layer surface; on a true turn it
builds a fresh `wl_surface`, re-attaches the layer role and moves the same renderer onto it
(`attach_surface`). Image ids stay valid and `renderer_init` does not run; uploads made while
hidden are drained at the next frame. Only if attaching fails is a new renderer made (and
`renderer_init` called). Why: an always-mapped transparent overlay makes the compositor blur
behind it on every change and keeps fullscreen clients off direct scanout. xdg windows ignore it.

## The context menu popup (runner side)

On an xdg toplevel the runner mirrors the open menu into an `xdg_popup`
(`backend/menu_popup.rs`); layer surfaces keep the in-window menu, placed by
`context_menu::constrain_to`. Two load-bearing rules: the configure is written back
(`context_menu::place`), so the rect apps hit-test is the rect on screen; and the popup takes its
own pointer input, translated into window coordinates (`pointer_frame`), so apps need no change.
While hosted, the apps' in-window paint draws nothing and the runner paints a copy at the origin
(`paint_hosted`) with the plate as the surface's root, frosted by the compositor's blur-behind
(alpha raised to `1 - (1 - a)(1 - k)` to match the in-app compression). The popup's renderer is
kept across opens (`detach_surface` / `attach_surface`) — **detach before the popup drops**. The
popup keeps its own copies of glyph images (`menu_icon_ids`) and does not drain the shared upload
queue (`set_shared_uploads(false)`). A page turn moves the popup with `xdg_popup.reposition` and
hands a new popup over from the window until its first configure (`MenuPopup::handoff`). The
menu's own behaviour is in `docs/widgets.md`.

## Input methods and text input

**One composition model for every shell** (`crate::ime`). Three things cross between the editing
widget and the shell's input method:

- the **commit** is delivered as typed text (`Driver::commit_text`: a press and release of a key
  whose text it is — never a shortcut, never repeated), so every widget that inserts a key's text
  takes it unchanged;
- the **composition** (`ime::Preedit`: text and the input method's cursor as a byte range) is per
  window, set through `Driver::preedit`;
- the **caret** goes back: a widget editing text reports it as it paints (`ime::report_caret`,
  window px with the `PaintCtx` offset); `build_frame` brackets the frame
  (`begin_frame` / `end_frame`), and `ime::caret()` is where candidates go and whether text is
  wanted at all.

How the editors show a composition, without ever holding it:

- **`TextBox`** shows it as a provisional run in `edit_buffer` (`composing`), drawn and wrapped as
  typed text and underlined by `selection_quads`; never in `committed_buffer`, never in history;
  the box takes no key while composing. A press, or editing ending, drops it and asks the input
  method to cancel (`ime::request_reset`). A composition begun over a selection replaces it.
  `a_composition_is_shown_in_place_and_the_commit_is_typed` is the test.
- **`LineEdit`** splices it into `display()` at the caret; `display_index` / `text_index` map
  across it; `composition_range` is the span to underline. The app draws the field, so it calls
  `sync_ime` each frame the field has the keyboard, reports the caret it draws, and calls
  `drop_composition` when focus leaves. `a_composition_is_shown_at_the_caret_and_never_held` is
  the test.
- **`DocEditor`** lays out the caret's line with the composition (`laid_col` / `source_col` map
  columns across it), underlines it, and reports its caret itself; the host calls
  `drop_composition` on focus loss. `a_composition_is_laid_out_in_place_and_never_held` is the test.

**A field being edited claims it every frame.** A widget open for typing calls
`cce_ui::text_input::claim(x, y, w, h)` from its paint (window px). `claim` is
`ime::report_caret`; the frame's last claim is `ime::caret()`. A claim per frame, not an
enable/disable pair, because editing ends on too many paths (Enter, Escape, a click elsewhere,
focus loss, the page dropped). `Spinbox`, the slider's readout, `ColorSelector` and the params
pane's code rows claim their field; `TextBox` and a focused `DocEditor` claim the viewport, then
report the caret once drawn. An app that draws its own text (a `LineEdit`, a terminal) claims from
`display_list` while it has a caret. Two consequences:

- **An app that replays cached geometry must replay the claim.** A host that paints its widgets
  only in a `rebuild_layout` claims on rebuild frames alone, and the first replayed frame disables
  the field. Run the rebuild under `text_input::capture` and claim what it returns every frame.
- **A widget drawn from its host's aggregates never claims.** `ParametersBg` paints its hosted
  fields from its own views, so it claims for the row being typed into (`claim_typing`).

**On Wayland it is `text-input-v3`** (`backend/text_input.rs`), relayed by the compositor to an
`input-method-v2` client (fcitx5, IBus). The text input is the first keyboard seat's. After each
render (`EngineState::sync_text_input`) it is enabled while the seat's text-input focus is on our
surface and a widget is editing (content type normal, the caret as the cursor rectangle in surface
px), re-sent when the caret moves; disabled when nothing edits; disabled-then-enabled for a
composition a widget dropped (`ime::take_reset`). Every change is one counted `commit`.
`preedit_string` / `commit_string` / `delete_surrounding_text` are applied on `done` in the
protocol's order (`Batch::apply_order`); `leave` drops the composition and disables an enabled
text input (wlroots keeps the enabled state across a leave). **A press re-announces an open
field**: the driver marks a pointer or touch press (`ime::note_press`), and the next plan that
still has a caret commits once more, so tapping an already-open field raises the on-screen
keyboard (the compositor reacts only to an enable or commit right after a touch). The decisions
are pure (`TextInput::plan`, `Batch`) and tested with no compositor. Sway routes text-input focus
only while an input method is bound.

Not supported yet: surrounding text (so `delete_surrounding_text` is not applied), and a password
box is announced with the normal content purpose. The browser and macOS input-method paths are in
`docs/platforms.md`.

## Touch

The runner binds `wl_touch` when the seat offers it; `backend/touch.rs` turns the FIRST finger
into pointer input by what it does: a **tap** clicks where it landed; a finger that **moves**
past `SLOP` (10 px) scrolls (`PixelDelta` equal to its travel, `ScrollPhase::Finger`, dispatched
at the down point, then `FingerEnd` at the lift so a flick coasts); a finger **held** `HOLD_MS`
(400 ms) before moving is a held left button. The hold needs no timer: the decision is made at the
first motion past the slop. Other fingers are ignored until the first lifts. `TouchTracker` is the
pure state machine; `Driver::touch` routes. A finger is always natural (dispatch runs inside
`input::with_natural_scroll(true, …)`) and 1:1. Not by finger: CSD moves/resizes and
`take_window_action` (their serials must match a pointer grab). Drive it in a shadow with
`ccectl touch down|motion|up`.

## Scrolling semantics

- **The delta the runner hands out is what a list scrolls by**; under natural scrolling a list
  moves its content the way the fingers went.
- **A value control reads "up is more"** (`MouseScrollDelta::value_notches_y`): a wheel notch up
  is positive; a finger's pixel delta is taken as it comes with natural scrolling off and negated
  with it on (`input::natural_scroll`, input.kdl `trackpad { natural_scroll }`). `Slider`,
  `Slider2D`, `Spinbox` and the menu's slider rows use it.
  `a_value_control_reads_up_as_more_on_a_wheel_and_a_natural_finger` is the test.
- **The animations switch stops a wheel's glide, not a flick's coast**
  (`scroll_motion::with_animations`). The coast is the rest of the hand's gesture; its own setting
  is input.kdl's `kinetic_scroll`. A slider's and a ramp key's post-scroll drift still follow the
  switch (they change a value after the hand stopped).
  `the_animations_switch_stops_the_glide_and_not_the_coast` is the test.
- **The phase** (`Finger`, `FingerEnd`, `Wheel`) is the window's (`window_state`), published by
  the runner before each dispatch; `ScrollMotion::apply_px` reads it, and
  `ScrollMotion::apply_phase` takes it as an argument for a host that reads it itself.
- **`motion::enabled()`** reads `/run/cce/animations` in a shipped binary; under `cfg(test)` it
  answers on, or what `motion::force_for_test` set on the thread.
