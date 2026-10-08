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

### Phase 2 — interaction state into `UiContext`

The context menu, its page turn, the hover highlight, the side swipe and the composition
move into the context (or a per-window struct beside it), reached through `EventCtx` by
widgets and through `UiContext` by apps; the free functions stay as deprecated forwarders
for one release while the apps move. The 139 direct `focus()` / `unfocus()` calls move to
the context's focus at the same time.

### Phase 3 — style as one snapshot

`Style` (every colour, radius, font and size, and the registry's flattened config) built
whole by a reload and published as an `Arc`; getters read the current snapshot; tests
install their own per thread. The two hundred `RwLock`s and the second registry go.

### Phase 4 — per-window properties

Scale, metric, scroll phase and vertical text travel with the frame and the event context
instead of process statics.

Each phase leaves every app building and drawing as before; the measured check for phases
2–4 is a shadow session of the apps that use the moved state (cce-files,
cce-system-interface, cce-designer, cce-gallery, cce-data-editor).
