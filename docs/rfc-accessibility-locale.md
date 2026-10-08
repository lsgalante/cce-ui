# RFC: Accessibility and locale — what the toolkit owes people who are not its author

**Status:** Draft / roadmap (2026-10-08)
**Scope:** Assistive-technology access (screen readers, switch and voice control),
keyboard-only use, the user's locale, right-to-left and non-monospace text editing, and
translatable UI strings — across `cce-ui` and the apps built on it.
**Appetite:** Phased. Phase 0 is a few lines; each later phase is its own piece of work
and leaves every app building and drawing as before until the step meant to change it.
**Reads before this:** `CLAUDE.md` § "Plates, wells and seams" (focus roles, the walk),
§ "Input-method composition is one model for every shell" (the IME model this extends);
`docs/rfc-core-rebuild.md` (the widget tree the accessibility tree would mirror).

---

## 1. Why now

The toolkit audit of 2026-10-07 found no accessibility support and no locale handling at
all. Both are cheap to design for and expensive to retrofit: every widget added without a
role, every label used as an identifier, and every editor that assumes left-to-right
monospace text is a later rewrite. The widget set is still growing (the field family, the
spreadsheet's selection and the trackball all landed in the last two weeks), so this is
the cheapest moment there will be.

## 2. Where things stand (measured 2026-10-08)

**Accessibility.**
- Nothing in the tree speaks to assistive technology: no AT-SPI, no AccessKit, no
  NSAccessibility, no ARIA. A screen reader sees an untitled surface.
- What exists is a good seed. Every retained widget reports a `FocusRole` (`Plate`, `Well`
  or `None`), a `label()`, a value (`Adapted::get_value_string`), its rect, its focus and
  its `type_name()`, and `UiContext` holds them all by `WidgetId`. Since phase 2 of the
  pointer work (`widget::Owned`) every registered app widget sits at a stable address.
- 11 of the 27 `Application` apps draw immediate-mode (no `UiContext` tree): calendar,
  browser, grid, keyboard, screensaver, map, status bar, model, preview, terminal,
  notifier. They have nothing for a tree to be built from.

**Keyboard.**
- Tab / Shift+Tab plate navigation exists (`UiContext::focus_step`, `focus_step_group`)
  but is OPT-IN (`Application::plate_navigation`, default false). Four apps turn it on:
  cce-data-editor, cce-files, cce-gallery, cce-system-interface (plus the demo).
- There is no tooltip (where an accessible description usually lives), no modal dialog
  widget (where focus must be trapped), and no radio group.

**Locale.**
- The locale is the literal `"en-US"` in all four `FontSystem` constructors
  (`lib.rs`, `backend/text.rs` twice, `backend/frame.rs`). cosmic-text uses it to order
  its font fallback, so a Japanese user gets the Chinese variants of shared Han glyphs.
- No message catalogue. The toolkit itself ships about 20 English strings (the context
  menu's Cut / Copy / Paste / Select All / Undo / Redo, Copy Path / Copy Key / Copy Value,
  Expand / Collapse, Properties, Search…, OK / Cancel / Save / Delete).
- **Labels are identifiers.** A context-menu row's text is its identity (CLAUDE.md: "the
  text stays the row's identity, so hosts that match their own labels still match"), and
  hosts dispatch by comparing label strings. Translating a label today breaks the action
  behind it.

**Text direction and script.**
- cosmic-text shapes bidirectional text and lays out right-to-left runs, so DRAWING Arabic
  or Hebrew mostly works.
- EDITING does not. `ShapingMeasure::offsets` sorts glyph positions by byte and assumes x
  grows with the text (false inside a right-to-left run); the `DocEditor` and `LineEdit`
  carets are built on it. `TextBox` is a column editor: it measures one monospace advance
  (`"MMMMMMMM"` / 8) and wraps by character count, which holds for neither proportional
  fonts nor CJK, whose characters are two columns wide.
- The IME model (`crate::ime`) is direction-agnostic and needs no change.

## 3. The plan

### Phase 0 — the locale comes from the user (small; do first) — DONE 2026-10-08

Done as written below: `cce_core::locale` (re-exported as `cce_ui::locale`), used by all four
constructors, and the browser shell sets it from `navigator.language` before the first font
system. Tests: `locale::tests` in cce-core (the parsing and the environment's order) and
`every_font_system_is_built_with_the_users_locale` in cce-ui.

One `cce_core::locale()`: `LC_ALL`, else `LC_CTYPE`, else `LANG`, turned from POSIX form
(`ja_JP.UTF-8`) into a BCP 47 tag (`ja-JP`), falling back to `en-US`; in the browser,
`navigator.language`. The four `FontSystem` constructors take it. Nothing changes for an
`en_US` session, which is every session today.

### Phase 1 — an accessibility tree the toolkit can produce

- A node per registered widget: **role** (from `type_name` and `FocusRole` first, then an
  explicit `Input::a11y_role()` a widget can override: button, checkbox, toggle-button,
  slider, spin-button, text-input, combo-box, list, tree, table, menu), **name** (the
  label), **value** (`get_value_string`), **bounds**, **focus**, and the **actions** it
  takes (activate, increment / decrement, set value, focus). Parent and child links come
  from `UiContext`'s tree.
- For the immediate-mode apps, an `Application::accessibility(&mut self, out)` hook where an
  app declares its own nodes, the way it already declares `input_regions`.
- Incremental: a node is keyed by `WidgetId`, and only changed nodes are re-sent. The
  per-frame damage diff (`backend::frame`) is the model for what "changed" means.
- **Rule from this phase on:** a new widget declares its role and name when it lands, and a
  context-menu row or any other actionable label gets an ID separate from its text.

### Phase 2 — speak to the platform

- **Wayland / Linux:** AT-SPI over D-Bus. AccessKit's Unix adapter is the likely carrier
  (evaluate it first; the alternative is a small AT-SPI server of our own). The compositor
  needs nothing new: AT-SPI is a session-bus protocol between the app and the reader.
- **macOS:** NSAccessibility through the same tree (AccessKit's macOS adapter, or direct).
- **Browser:** mirror the tree as hidden ARIA elements over the canvas — the same move the
  IME already makes with its hidden `<textarea>`.

### Phase 3 — keyboard first

- `plate_navigation` defaults to true; an app that routes Tab itself (a terminal, a web
  view) opts OUT.
- A modal dialog widget that traps focus, a tooltip that doubles as the accessible
  description, a radio group.

### Phase 4 — text that is not left-to-right monospace

- Carets and selection from shaped clusters in VISUAL order (cosmic-text's layout runs
  carry direction), not byte-sorted x positions: `ShapingMeasure::offsets` grows a
  direction-aware form, and `DocEditor` and `LineEdit` use it.
- `TextBox` moves from its column model to shaped wrapping (cosmic-text's own wrap), which
  also makes it correct for proportional fonts and CJK.
- Base direction per paragraph from the first strong character, as the Unicode
  bidirectional algorithm does.

### Phase 5 — translatable strings

- The toolkit's ~20 strings through a small catalogue keyed by message ID, in `cce-core`
  so services can use it too. Fluent is the candidate format.
- Apps adopt it at their own pace. Only phase 1's rule — actions keyed by ID, never by
  label — has to come first.

## 4. Order and cost

Phase 0 is an afternoon. Phase 1 is the real design work, and everything after it stands
on it; it is additive, and the tree costs nothing for an app nobody inspects. Phases 3 and
4 are independent of 1 and 2 and can go in parallel. Phase 5 waits on phase 1's ID rule.

## 5. Open questions

- AccessKit, or our own AT-SPI server? Decide by building phase 2 against AccessKit for one
  app (cce-data-editor: plate navigation already on, a tree list, text fields, a menubar)
  and measuring what it costs in dependencies and in frame time.
- Should the compositor expose its own UI (window titles, overview, the grid) to AT-SPI?
  It draws natively, so it would need its own tree.
- Do immediate-mode apps get the hook or get ported? The status bar and the notifier are
  the most-used surfaces in the DE, and both are immediate.
