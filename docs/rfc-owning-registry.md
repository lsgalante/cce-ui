# RFC: a registry that owns its widgets

Status: in progress (opened 2026-10-08). Phases below carry DONE notes as they land.

## Why

`UiContext`'s tree does not own its widgets. The app owns them (struct fields, `Vec`s of
`Owned<Adapted<X>>`) and registers raw pointers; the context dispatches through those pointers,
and every accessor checks a liveness token before it resolves one. Since 2026-10-08 that is
sound and checked by Miri (`widget::owned`), but one rule is left to discipline rather than the
compiler: **a `&mut` reached through the registry must not overlap one taken through the
`Owned`.** An app that holds `&mut self.x` across a `UiContext` call that reaches `x`, or a
widget that reaches itself through the context while the context is dispatching into it, aliases
two `&mut`s to one widget.

The end state that makes the compiler enforce it: the context OWNS every registered widget and
lends it out. The app keeps a typed handle; to touch a widget it borrows the context
(`ui.get_mut(h)`), so the borrow checker refuses any overlap with another context call; and the
context, while it is inside a widget, has that widget out on loan, so nothing — not the widget
itself, reaching back through the context — can reach it a second time.

## Design

- **`Handle<W>`** — `Copy`, typed, a `WidgetId` and a marker. Stable for the widget's life;
  a removed widget's handle resolves to `None`.
- **Owned slots.** `UiContext::insert(w) -> Handle<W>` moves the widget into a heap allocation
  the tree keeps (the same raw-root shape `Owned` uses, never a `Box` held across accesses), with
  the allocation's `TypeId` for typed access and its liveness token. `get(h)` / `get_mut(h)` hand
  out `&W` / `&mut W` borrowed from the context; `remove(h)` gives the widget back by value;
  dropping the context drops every widget it owns.
- **Lending.** Every call the context makes INTO a widget — events, ticks, focus changes,
  `mark_dirty`, popover and context-menu actions — goes through `UiContext::lend(id, |w, ctx|
  ..)`, which marks the slot lent for the call. While lent, nothing in the context resolves it
  (`get_ptr`, `children_ptrs`, `iter_registered`, `get_widget[_mut]`, `get(h)`, `lend` itself
  all answer `None`/skip it), so a re-entrant reach is a visible miss, never an alias. Lending
  applies to the legacy pointer entries too, which closes the self-reach hole for apps that have
  not moved yet.
- **Hosts that place and paint a widget** take a handle: `render_widget_h(pc, h, rect, ctx)`,
  `Form::widget_h`, `set_focused_h`, `link_handles`… — each lends the widget for its call.
- **Embedded children** (a tree list's search box, a paginator's menu): the parent holds handles
  and reaches its children through the context it is handed (`EventCtx`, `paint_ui`'s `&UiContext`).
  The parent is out on loan while it runs, its children are not.

## Phases

1. **Owned slots, handles and lending in the toolkit.** `Handle`, `insert` / `get` / `get_mut` /
   `remove` / `lend`; the lent flag in the tree and every resolver honouring it; the context's
   own calls into widgets routed through `lend`. Both kinds of entry coexist: an app migrates
   field by field. Tests, Miri included.
   **DONE (2026-10-08).** `widget::Handle`; `UiContext::{insert, get, get_mut, remove, lend,
   lend_h, register_popover_id}` and `ctx[h]`; `WidgetTree::{insert_owned, owned_root,
   take_owned, set_lent}` (owned slots keep the raw-root shape `Owned` uses; `clear_all`
   keeps them). Every call the context makes into a widget that hands it the context —
   routed events, drag start / update / end, grabs, keys to the focused widget, ticks,
   FocusIn / FocusOut, the focused widget's context action, the popover-close sweep — goes
   through `lend`; dispatch works by id (`propagate_event_impl`, `find_hovered_scrollable`).
   Lending covers the pointer entries too, so a widget that reached itself through the
   context mid-event now misses instead of aliasing. Host helpers: `render_widget_h`,
   `Form::widget_h`. Tests in `widget::handle` (CI's Miri job runs them beside
   `widget::owned`); the 604 existing tests pass unchanged.
2. **The demo app** (`src/main.rs`) on handles — the reference every client copies.
   **DONE (2026-10-08).** Twelve widgets inserted in `create`; `register_roots` and the
   first-frame registration went. A shadow session driven through a click, the toggle,
   typing, the theme dropdown and the Options dialog (open, pick, OK; and Escape) is
   identical to the pixel to the pointer-registry build at every step.
3. **Every app**, one crate at a time, by widget count: the smallest first.
4. **The toolkit's embedded children** on handles.
5. **Delete the pointer path**: `Owned`, `register_host` / `register_widget` / `set_focused_ptr`
   and the other `unsafe fn`s, `Liveness`, `stable_target`. The tree holds owned slots only.
