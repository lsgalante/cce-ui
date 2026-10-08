# RFC: Global state — what lives in statics, and where it should

**Status:** Draft / roadmap (2026-10-08)
**Scope:** cce-ui's process-wide and per-thread state: what there is, which of it is a cache
(fine) and which is a window's or an app's state hiding in a static (not), and the order to
move it.
**Reads before this:** `CLAUDE.md` § "The registry holds pointers, and knows when they die",
§ "The context menu draws in its own popup surface"; `docs/rfc-core-rebuild.md`.

---

## 1. Why

The toolkit audit of 2026-10-07 listed global state as the largest remaining structural
debt. Its costs are concrete:

- **Tests pin settings per thread** (`force_natural_scroll`, `force_scroll_settings`,
  `motion::force_for_test`, the style registry's per-thread overlay), because what they read
  is process state another test can change. Every new global grows that list.
- **One window per thread.** The context menu, the hover highlight, input-method state and
  the focus thread-local are one per thread: a process with two windows on one loop would
  share a menu, a highlight and a composition.
- **Two sources of truth.** Several pieces of interaction state are stored twice, once in a
  static and once in `UiContext`, kept in step by convention (§ 2.2).

## 2. Inventory (measured 2026-10-08)

About 300 `static` items and 18 thread-locals in `src/`.

### 2.1 Style: about 200 `RwLock`s

`color/mod.rs` and `layout/mod.rs` hold one `RwLock` per configured colour, radius, font and
size (`BUTTON_CORNER_RADIUS`, `TREE_LEAF_TEXT_COLOR`, …), filled by `reload_colors` /
`reload_config`, beside the style registry (`layout/registry.rs`: `FLOATS`, `LENS`, the
flattened config) that holds much of the same configuration again. A read takes one lock; a
reload writes two hundred, so a frame drawn during a reload can see half of each.

**Verdict:** style IS process-wide (one DE config), so global is defensible — but as ONE
snapshot (an `Arc<Style>` swapped whole on reload, read without locks), with one per-thread
override for tests, not two hundred locks and a second registry.

### 2.2 Interaction state: per thread, and partly twice

| State | Where | Twin |
|---|---|---|
| keyboard focus | `widget::focus` thread-local `FOCUSED_WIDGET` | `UiContext::focused_widget` — the Tab walk, the accessibility tree and `set_focused_id` use this one; widgets claim the thread-local in `FocusIn` |
| hover highlight | `widget::hover_animation` thread-locals `HOVER_STATE`, `CURSOR_POS` | `UiContext::hover_state` and five methods — written by nobody, read by nobody |
| context menu | `context_menu::CONTEXT_MENU` thread-local | `UiContext::context_menu` — never read |
| a menu's page turn | `PAGE` thread-local | — |
| side swipe | `side_swipe::SHARED` | — |
| input-method composition | `ime::STATE` | — |
| wake hook | `backend::app::WAKE` | — |

Beside these, apps keep a third notion of focus: 139 direct `WidgetHost::focus()` /
`unfocus()` calls that toggle a widget's own `focused` flag without telling any store.

**Verdict:** a window's interaction state belongs to its `UiContext`. The twins go first:
they can disagree today.

### 2.3 Properties of "the" window: per process

`scale::scale_factor`, `units::metric`, the scroll phase the runner publishes before a
dispatch, vertical text (`backend::text::set_vertical_text`). Each is a property of the
window being drawn, published process-wide.

**Verdict:** per window, handed to what needs it (a `Frame`/`PaintCtx` field, the
`EventCtx`) — after § 2.2, since the same plumbing carries both.

### 2.4 Caches and switches: fine

Shaped-text buffers, family and face caches, icon rasters, the image-id queue
(`draw::images`), `OnceLock<bool>` debug switches read once from the environment, the l10n
catalogue. Process-wide by nature; nothing to do beyond keeping them caches (no state a
caller depends on between calls).

## 3. Plan

### Phase 1 — one store per piece of interaction state (the twins) — DONE 2026-10-08

Done as written below. `widget::focus` keeps only `link_parent_child`; `EventCtx` gained
`is_focused`, and `request_focus` records on the context (telling the previous holder, as
before) where it set the thread-local. cce-designer's dialog and cce-system-interface's
section navigation call `UiContext::set_focused_id` / `clear_focus` / `has_focus` /
`is_focused_id`, which deliver FocusIn / FocusOut as the Tab walk does — so their manual
`w.focus()` after a set went. Checked in a shadow: the demo's dropdown, reached by Tab,
opens on Enter (its gate reads the context now); the settings app's Browser page takes
typing in a clicked field and Enter commits it without opening a dropdown (the bug that
gate exists for). The whole workspace builds.

- **Focus:** `UiContext::focused_widget` is the store. The `widget::focus` thread-local and
  its free functions go; `EventCtx::request_focus` / `release_focus` act on the context;
  the dropdown's "am I focused" reads it. Apps that used the free functions
  (cce-designer, cce-system-interface) call `UiContext`'s.
- **Hover highlight and the context menu:** the dead `UiContext` twins are deleted, so the
  thread-locals are the one store each until phase 2 moves them.

### Phase 2 — interaction state into a window's own state — DONE 2026-10-08

Done, by a different route than first written below, for a reason found in the counting:
the context menu alone is reached 266 times from 18 apps, most of them through free
functions with no `UiContext` in hand, and some apps that show a menu have no context at
all (the terminal). So the state did not move INTO the context but beside it:
`window_state::WindowState` (the context menu, the hover highlight and its cursor, the side
swipe recognizer, the input-method composition) is OWNED by a window — the Wayland shell
makes one in `run` and keeps it across reconnects; the AppKit and browser shells make one
too — and the shell makes it current (`window_state::enter`, a guard) while it runs that
window's code. The modules' free functions act on the current one, so not one of their
callers changed; with none entered, each thread has a default (tests, tools that draw no
window), which is exactly what the thread-locals were. Two windows keep two menus
(`each_window_has_its_own_menu`); a shell that runs two windows on one thread enters each
around its dispatch. `context_menu::with_state` replaces the designer's reach into the old
`CONTEXT_MENU`. The browser clipboard bridge's `PAGE` stays: it is the page's clipboard,
not a window's interaction.

**Direct focus calls.** Of the 103 `w.focus()` / `w.unfocus()` calls in apps, the 73 in the
eleven apps with a `UiContext` moved to `UiContext::focus_widget` / `unfocus_widget`, which
do what the direct call did AND keep the window's record of focus (the Tab walk, the
accessibility tree) in step: before, unfocusing the focused widget left the record on it,
and focusing another left the old one lit. The rest were already paired with a context call,
or are in the three apps with no context (cce-authenticator, cce-mail, cce-secrets), where a
widget's own flag is the only focus there is. Checked in a shadow: cce-fonts' preview box
takes a click and typing as before (its search box ignores clicks before and after — a
separate, older bug).

The plan as first written:

The context menu, its page turn, the hover highlight, the side swipe and the composition
move into the context (or a per-window struct beside it), reached through `EventCtx` by
widgets and through `UiContext` by apps; the free functions stay as deprecated forwarders
for one release while the apps move. The 139 direct `focus()` / `unfocus()` calls move to
the context's focus at the same time.

### Phase 3 — style as one snapshot — DONE 2026-10-08

`crate::style::Style` is the whole style — the colour slots, the layout slots, the named
materials and the registry — published as an `Arc` and replaced whole on change. Each
former `static RwLock` (178 of them) is a typed handle on one field, `style::StyleCell`,
keeping the `RwLock` API (`.read()` / `.write()` returning a `Result` of a guard), so none of
the ~550 places that read or write them changed; `get_style_registry` returns the
registry's handle, and cce-grid, which writes it, compiles unchanged. A read takes no lock
(each thread keeps the last snapshot and checks a generation); a write publishes a new
snapshot when its guard drops, and guards never wait on each other. `reload_config` and
the colour load run as one `style::batch`: their writes go to one pending snapshot, which
their own reads see, published once — a reload is atomic, and its hundreds of per-key
writes cost one copy. 14 slots that were written and never read went with it (13
per-widget corner radii whose getters read the registry long ago, and the button hover
colour, a config key that did nothing). The per-thread test overlays are as they were.
Checked: the suite passes repeatedly; the demo and the settings app, run against the real
config, draw identically to the pixel before and after.

The keys parsed twice were folded in on 2026-10-08: every layout key's getter reads the
registry alone and its setter writes it, so the ~50 slots that duplicated registry keys, the
reload's prefix scan that filled them, and the 35 getters that each re-parsed `config.kdl`
on first use are gone (`a_style_key_lives_in_the_registry_alone`). The colour slots are a
system of their own, filled from the raw KDL rather than the registry, and stay.

`Style` (every colour, radius, font and size, and the registry's flattened config) built
whole by a reload and published as an `Arc`; getters read the current snapshot; tests
install their own per thread. The two hundred `RwLock`s and the second registry go.

### Phase 4 — per-window properties — DONE 2026-10-08

Not by threading them through every frame and event (the scale alone has 27 readers and 37
setters across the toolkit and 17 apps) but as phase 2 did the interaction state: the
scale, the display metric, the app id, fullscreen, maximized and vertical text are a
window's `window_state::Props`, and the scroll phase a field beside them. The setters
(`scale::set_scale_factor`, `set_app_id`, …, `backend::text::set_vertical_text`,
`window_state::set_metric`) write the current window's AND the process-wide value; the
getters read the current window's, else the process-wide one. That last rule is what keeps
a worker thread right — a page rasterized at the scale, a tile decoded for it, has no
window entered, and reads the last value any window set — and it makes a single-window
process read exactly what it did. The metric lives in cce-core, which knows nothing of
windows: `units::set_metric_resolver` lets cce-ui answer first. The scroll phase, read only
while a window dispatches, is the window's or the thread's own, never the process's — so
tests no longer race on it. `each_window_has_its_own_scale_and_a_worker_reads_the_last`.
Checked at a forced scale of 2 against the real config: the demo and the settings app draw
identically to the pixel before and after (the settings app's run-to-run hover noise
aside).

Scale, metric, scroll phase and vertical text travel with the frame and the event context
instead of process statics.

Each phase leaves every app building and drawing as before; the measured check for phases
2–4 is a shadow session of the apps that use the moved state (cce-files,
cce-system-interface, cce-designer, cce-gallery, cce-data-editor).
