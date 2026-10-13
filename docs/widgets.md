# Widgets: behaviour reference

Per-widget behaviour that is not obvious from the code's signatures. Present tense; history in
`CHANGELOG.md`. Surface details (fields, stances) are in `docs/surfaces.md`.

## Widget events (`WidgetEvent`, `UiContext::take_events`)

How a widget tells the app something happened to it. `widget::WidgetEvent` is the vocabulary:
`Clicked` (pressed and released on it, Enter / Space on a focused plate, an assistive tool's
click) and `Changed` (its value changed: typed, dragged, picked, toggled, set by an assistive
tool). It is `#[non_exhaustive]`; the payloads the widgets' own drains carry (a `ButtonStrip`'s
segment, a `TreeList`'s row, the `GraphController` drains) are not in it yet.

- **Raising one.** A widget calls `EventCtx::emit(ev)` from any handler the adapter gives an
  `EventCtx`: `on_event` (routed, or the `FocusIn` / `FocusOut` of a direct `focus()` /
  `unfocus()`, which has no routing context) and `tick_ctx`. The adapter keeps what it emitted
  on the widget (`Adapted`), in order, until a sweep. Built-in widgets emit nothing: the sweep
  turns the one-shot flags behind `Input::take_click` / `take_change` into `Clicked` /
  `Changed` (`WidgetHost::drain_events`), so every widget reports without one. A widget
  reports a thing by emitting it or by its flag, never both, or it arrives twice.
- **Collecting them.** `UiContext::take_events()` sweeps every widget the context holds,
  embedded children under their own ids, in id order. A widget's events come as it raised
  them: everything it emitted, then `Clicked` if its click flag was up, then `Changed` if its
  change flag was. A widget with nothing to say contributes nothing; a window nobody touched
  returns an empty list. The app matches ids against its handles:

  ```rust
  for (id, ev) in self.ui.take_events() {
      match ev {
          WidgetEvent::Clicked if id == self.save.id() => return Some(Message::Save),
          WidgetEvent::Changed if id == self.volume.id() => self.set_volume(self.ui[self.volume].value()),
          _ => {}
      }
  }
  ```
- **When.** After every dispatch path the app runs — `handle_mouse_input`, `handle_key_input`,
  `tick` — or a keyboard activation or an assistive tool's change waits for the next pointer
  event. A widget that is not in the context (a composite's by-value child) is not swept; its
  composite drains it.
- **One drain per widget.** The sweep drains the flags `take_click` / `take_change` read, so an
  app uses either the sweep or the per-handle polls (`ctx[h].take_click()`, which stay) for a
  given widget; whichever runs first has the event.
- **Echoes of programmatic sets.** `set_value_string` raises the change flag on every widget
  that takes a value (it is also how a context-menu Paste and a config binding set one), and so
  does `TextBox::set_value`, so the next sweep reports `Changed`. The typed setters
  (`Slider::set_value`, `Checkbox::set_checked`, `Toggle::set_toggled`, a Dropdown's
  `selected`) raise nothing. An app that does not want the echo drains it (`take_change`)
  right after setting.
- **One flag, one event.** Dropdown and Toggle keep one flag behind both polls; the sweep reads
  the change first and reports it as `Changed`. A Checkbox press raises both: `Clicked`, then
  `Changed`.
- **Composites and their embedded children.** The sweep reaches an embedded child, so a
  composite reads its child's flag in the same call that raised it (a `Paginator`'s strip, a
  `TreeList`'s add-key button), or compares the child's value with its own (the `TreeList`
  query follows its search box's text). A flag left for a later tick may already be swept.
- **Forward your drain.** A widget with an inherent `take_change` / `take_click` (TextBox,
  Dropdown, FontSelector, KeybindRecorder) forwards it from its `Input` impl; without the
  forward, `Adapted::take_change` and the sweep answer the trait's default, `false`.
  `ButtonStrip::take_click` returns the segment (`Option<usize>`) and is not forwarded.

`emitted_events_arrive_in_order_before_the_flags`, `a_commit_from_a_direct_unfocus_is_swept`
and `an_embedded_childs_change_is_swept_and_its_composite_still_sees_it` are the tests.

## Context menu (`widget::context_menu`)

One global menu per window that every app shows, paints into its display list and dispatches by
window coordinates. On a toplevel the runner hosts it in an `xdg_popup` (`docs/runtime.md`).

- **Rows**: give each row an action ID (`set_row_actions`, or
  `UiContext::show_context_menu_rows`); a press runs the row's action. A label beginning
  `MARK_CHECK` (`"✓ "`), `MARK_ON` (`"● "`) or `MARK_OFF` (`"○ "`) draws `check`, `circle` or
  `circle-outline` at its left and the label without it. Slider rows: `set_row_slider`.
  `menu_marks_and_chevrons_are_glyphs` is the test.
- **Rows the host owns** (no action of their own): the host keeps their meaning beside the
  labels (a `Vec` of its actions and the `generation()` of its `show`). Running one — a press
  routed through `mouse_input` (with the host's `UiContext` or `None`), Enter through
  `activate_hovered`, a screen reader's Click — closes the menu and records `(generation,
  row)`, which `take_picked()` hands over once; the host runs it when the generation is its
  own. A reader's Click arrives between events, so a host takes the pick in `tick` too. The
  English label fallback still runs on a target that takes it. `activate(row, ctx)` runs any row
  by index, wherever it is drawn (a row scrolled out of a short menu included); `show` drops an
  untaken pick. `a_row_the_host_owns_is_picked_by_a_press_enter_or_a_reader` is the test.
- **Disabled rows** (`set_row_disabled(idx)`, or `set_rows_disabled(vec)` parallel to the
  options, after `show` / `show_page`, which enable every row): drawn dim (`TEXT_DIM`, the mark
  too) among the rows that run, never highlighted, stepped over by the keyboard, never run or
  turned into; a press on one only closes the menu, as on a header. A reader reads it where it
  stands, marked disabled, with no Click. `a_disabled_row_is_dim_and_runs_nothing` is the test.
- **A scaled menu** (`show_scaled(.., scale)` in place of `show`): labels, rows, marks and
  chevrons `scale` times the DE's menu font and `ROW_H`, the padding unchanged. Sizing, hit tests
  and paint all read `row_h()` / `font()`; a host hit-testing rows itself asks `row_at` or
  `row_h()`, never `ROW_H`. Every `show` sets the scale again, so it never outlives its menu.
  `a_scaled_menu_is_measured_hit_and_drawn_at_its_scale` is the test.
- **A short menu scrolls**: `ContextMenuState` keeps `content_h` (all rows) apart from `h` (shown)
  and a `scroll`; `row_at` / `row_y` / `hit_test` answer for rows as DRAWN.
- **Keyboard walk**: `set_hovered_item(Some(i))` highlights a row; `step_hovered(dir)` moves to
  the next runnable row (skipping headers, disabled rows and `-` separators, stopping at the
  ends) and scrolls to it without re-hovering under the pointer. The menu reads no keys itself:
  a host routes Up/Down to `step_hovered` and Enter to `activate_hovered`.
  `the_keyboard_steps_the_highlight_over_what_cannot_run` is the test.
- **Page rows** (`set_row_page(idx)` after `show`): the row wears `chevron-right`; a press on it,
  or a two-finger side swipe with the pointer on it, asks to TURN the menu. The menu recognises
  the turn; the host shows the page:
  - a turn is a `PageTurn` (`Into(row)` / `Back`), read with `turn_at(x, y)` for a press or
    `take_turn()` after a swipe;
  - `show_page(x, y, back, options, header_count, target)` shows a page at the corner of the
    plate it replaces; with `back` set, a back band reads `‹ Title` (press or swipe back = `Back`).
    A turned page slides on screen and never flips up;
  - `refill(options, sliders)` changes labels and slider values in place (same row count);
  - the swipe is `widget::side_swipe`, one recogniser per window shared by the menu and any plate
    a host turns into: it fires after `SWIPE_PX` (40) sideways at 1.5× more sideways than
    vertical; a tilt-wheel notch is a whole swipe. Forward follows the content (under natural
    scrolling, fingers left = in). The rest of a gesture that turned is the turn's:
    `side_swipe::swallow(delta)` at the top of a host's wheel handling. Tests that swipe twice
    call `side_swipe::end_gesture()` between;
  - **a turn animates** (`TURN_MS`, 180 ms): the plate resizes from the old size at the shared
    corner (`drawn_rect`), the old rows slide `TURN_SLIDE` px and fade, the new slide in from the
    side the turn comes from. Both row sets fade through `Prim::faded` (alpha for colour, text
    and images; `strength` for a groove). `turn_from_size(w, h, forward)` turns from a plate the
    menu does not draw (the designer's dialog). `is_turning` keeps frames coming.
  `a_page_turn_grows_the_plate_from_the_one_it_replaced` and
  `a_turn_fades_the_separators_of_both_plates` are the tests.
- Submenus (flyouts) do not exist; a page row is the one way a row leads somewhere.

## Dropdown

- `is_expanded()` is `open && !closing`: during the closing animation the plate is still drawn
  but input is no longer the dropdown's. A host routing input to a dropdown ahead of what is under
  it asks this.
- The runner hands every left press to each registered popover whose hit test MISSES it, before
  the app sees it (`close_popovers_missed_by_press`), and a popover covered by an outer claim
  always misses — so a dropdown may have taken the press before the app is asked.
- The trigger's text and arrow name their font on the prim (`control_label_font_detached()`), the
  whole configured string (`Berkeley Mono 14`), so a dropdown painted as a stamp by another widget
  keeps its own font, and the menu's rows come out at the trigger's size.
- A host draws an open dropdown with `render_popover(&mut pc)` after everything the list covers;
  the list is the trigger's flush plate grown over the revealed box, in the popover material.
- A list honours the context menu's marks: a marked option draws its glyph in a column every row
  keeps, and the closed trigger shows its value without the mark.
  `a_marked_option_draws_its_glyph` is the test.

## Date picker (`DatePicker`)

- An immediate-mode helper, not a `WidgetHost` widget: the host opens it (`DatePicker::open`,
  below what is being dated and right-aligned to it, flipped above when there is no room, kept
  in the window), routes the pointer (`hit`, `press`), keys (`key`) and the wheel
  (`shift_month`) to it while it is open, and acts on the `Outcome` each answers — `Set(day)`,
  `Clear`, `Close`, `Redraw`, `Ignored`. A press outside is the host's to turn into `Close`.
- The surface is the dropdown's: `Material::popover` on a carved plate. Day cells are
  `dropdown_height()` square; the content stands `bevel_width()` plus half the text inset in.
  Month and weekday names and the footer go through `l10n` (`date-picker-*`).
- Options: `with_title` names the item on the first line (it may cover its own row),
  `with_week_start` sets the first column, `without_clear` keeps a date that must stay (Clear
  draws faint and does nothing).
- Display-list text draws above all geometry, and only a registered widget's popover gets the
  engine's text clamp, so the host leaves its own text out from under `rect` while it is open.
  cce-list (list items, vault tasks) and cce-calendar (day-pane rows) are the hosts.

## Parameter pane (`ParametersBg`)

The designer's and cce-files' row-based parameter editor. Row types are strings
(`slider:lo:hi:dec`, `float3:lo:hi:trackball`, `textpick`, `checkbox`, `separator`, …).

- **Label layout**: inline (a label column beside unlabelled controls) or stacked (each control
  carries its label above). `param_label_layout` is the preference; an inline pane whose label
  column would leave any visible row's TRACK shorter than `MIN_INLINE_TRACK_W` (120 px) stacks,
  and goes back when there is room. The track is the control's rect less its chrome
  (`control_chrome`: a slider's 60 px readout and gap, a float3's axis column and trackball). The
  flip is a relabel and re-layout (`apply_label_layout` / `relabel_rows`, `Adapted::clear_label`),
  not a flag.
- **Separator rows** (`parameters_bg::SEPARATOR`): a one-pixel hairline in `surface_border` with
  the row gap either side; never hovered, focused or written back.
  `a_separator_row_is_a_rule_between_rows` is the test.
- **Vector rows**: `float2` / `float3` / `float4` are one `Float3` group with 2–4 sliders
  (`set_components(n)`); only a three-wide group can carry a trackball.
  `float2_and_float4_rows_are_the_group_with_two_or_four_sliders` is the test.
- **Soft ranges** (`…:soft`, `Slider::set_soft`): a TYPED value past an end widens the range; a
  drag and the wheel still stop at the ends. The pane writes a slider back from the slider's own
  value (`get_scaled_value`). `a_soft_range_widens_to_a_typed_value` and
  `a_soft_row_writes_back_what_its_slider_holds` are the tests.
- **Fields**: textpick rows and spinboxes are `Field`s (`ParametersBg::fields`), dropdown and
  button rows too; `TextBox::joined_right` stops the box at the seam.
- **Scroll gestures belong to what they begin on**: a value control (a slider's band by its halo,
  `Slider::scroll_hit`; each float3 row; a spinbox row; the trackball) takes a gesture that starts
  on it and keeps it until the gesture ends (`scroll_initiate_widget_id`, 250 ms of quiet);
  otherwise the pane scrolls.
- The pane paints its controls' chrome and collects their text and glyphs itself
  (`own_text_labels`, `child_glyphs`), and claims text input for the row being typed into
  (`claim_typing`). `the_pane_draws_its_controls_glyphs` is the test.

## Slider

One wheel notch or arrow press moves `notch_step`: 2% of the range for a range up to `FINE_SPAN`
(20) wide; for wider ranges, 2% of a span that grows with the value's magnitude (20× it, floored
at 20, capped at the range) — so −1000..1000 moves 0.4 near zero and 40 from a hundred up. A drag
maps the pointer to the whole range; the readout is where an exact value is typed. Value controls
read the wheel as "up is more" (`docs/runtime.md`).

## Float3 trackball

`Float3::set_trackball` stands a ball left of the three rows. The vector is drawn from the centre
(X right, Y up, Z toward the viewer; bright near side, dim away) with five rings of latitude about
it (`Float3::ring`, `RING_ANGLES`, 30° apart) so rotation reads. Dragging rolls it under the
pointer, keeping length; a scroll rolls it like content (`ball_scroll`, `SCROLL_TURN` 15° a notch;
a trackpad in both axes). Drags and scrolls turn a full-precision copy (`BallDrag`, `fine`)
because the rows hold the vector rounded (to three decimals with a ball); a zero vector gets
length one on the first drag. `Float3::set_view` / `ParametersBg::set_trackball_view` show the
ball from a host's camera (its right, up and toward-viewer rows), so drags roll about the
camera's axes; the value stays in the vector's own space. The ball has its own scroll-gesture id
(`ball_id`; `wheel_zone` / `wheel_latched`). It paints through `paint_ball` on the host's scene
path (`paint_scene_rows`), not `Paint::paint`.

## Spreadsheet

- **Columns of values** (`SheetColumn`: `Text`, `Int`, `Float` to N decimals), set with
  `SpreadsheetController::set_spreadsheet_columns`; a cell's text is written only when painted.
  `set_spreadsheet_data` (rows of strings) still works as text columns. Sorting compares values.
  `a_column_table_is_written_as_painted_and_sorts_by_value` is the test.
- **Columns fit their content**: header plus room for the sort glyph (`SORT_MARK_CHARS`, kept
  whether sorted or not) or the widest cell, plus `CELL_PAD`, at least `MIN_COL_CHARS`; counted
  in characters from the values (`SheetColumn::max_chars`) and made pixels by the monospace label
  font (`col_edges`). The table does not stretch; a wider table scrolls.
  `columns_fit_their_content_and_overflow_scrolls` and
  `a_columns_widest_cell_is_worked_out_from_its_values` are the tests.
- **Row selection** of DATA rows: press = select alone (or clear if it was the whole selection),
  ctrl = toggle, shift = run from the last unmodified press over the rows as DISPLAYED; a press
  on empty body clears. Survives sorting and `set_spreadsheet_data` (less rows that no longer
  exist). `selected_rows`, `set_selected_rows`, `take_selection_change`; selected rows wear
  `highlight_primary_color` at 28%. The host pushes modifiers first (`set_modifiers`).
  `rows_select_alone_toggled_and_in_runs` is the test.
- Scrollbars: the centred, sink-behind design below (`paint_scrollbars`, `scrollbars_shown`).

## Scrollbars: one design

Every scrolling list and pane offers the DE's one scrollbar design:

- It rides the **centre line** of what it scrolls (vertical down the middle of the width,
  horizontal across the middle of the viewport), over the content, reserving no lane;
  `layout::centred_scrollbar_width()` thick, pills in the shared track and thumb colours.
- It idles **behind** the host's translucent plate and takes no press there.
- A scroll raises it in front with a fade (`ScrollbarActivity`: wheel, key, glide or coast, a
  host's `notify_scrolled`, a thumb drag); a pointer over a RAISED bar holds it; hover never
  raises a sunk one; with nothing holding it for `SCROLL_ACTIVE_HOLD` it sinks over
  `SCROLL_FADE_SECS`.
- Two copies: the idle one at full alpha BEFORE the plate, every frame; the fore one after the
  content at `fade()`.

| Widget | Opt in | Idle copy (before the plate) | Fore copy (after the content) |
|---|---|---|---|
| `Spreadsheet` | always | host: `paint_scrollbars(rect, ctx, 1.0)` | the widget's paint |
| `ParametersBg` | always | host: `scrollbar_quads()` as pills | host: same at `scrollbar_fade()` |
| `ScrollRegion` | `with_sink_behind(true)` | framed: `push_prims`; frameless: host, `push_scrollbar_prims` | host: `push_scrollbar_fore` |
| `ScrollBox` | `sink_behind = true` | host: `paint_scrollbar_pills(pc, 1.0)` | host: `paint_scrollbar_pills(pc, scrollbar_fade())` |
| `TreeList` | always | the widget, under its plate | the widget, over the rows |

A region or box that does not opt in keeps an always-on edge bar (`edge_inset` applies only
there; `paint_relief_scrollbar` is for edge bars only). A multi-line `TextBox`'s position
indicator stays at the right edge, always shown — an editor keeps its place marker.
`sink_behind_bars_cross_at_the_centre`, `the_fore_copy_is_drawn_after_the_rows` and
`a_sink_behind_bar_rides_the_centre_and_sinks_until_scrolled` are the tests.

## Graph

Behind the `graph` feature (`widget::Graph`, `GraphNode`, `TaggedQuad`, `node_wires`,
`GraphController`); cce-designer, cce-files and cce-graph turn it on.

- **Wire styles** (`WireStyle`): orthogonal, rounded, bezier, straight;
  `style.surface.graph.node.wire_style` unless the host sets one (`Graph::set_wire_style`, `None`
  to follow config). Colour and width are `wire_color` / `wire_size` (px at 100%, scaled with the
  body; no floor — under one device pixel it draws one device pixel at reduced alpha,
  `wire_stroke`). Wires are `Graph::paint_wires` (also on `GraphController`) as `Vector` / `Arc`
  prims, not part of `geometry_quads_tagged`; a host drawing the quads itself calls it between
  `paint_grid` and the bodies. One path, drawn and hit (`wire_path`; the splice hit test
  `splice_wire_at` walks it). Orthogonal joins do not overlap, so translucent wires are one alpha.
  `every_wire_style_runs_from_port_to_port` is the test.
- **The run across is on the first lattice line below the source** (`Graph::wire_turn_y`) for
  orthogonal and rounded wires running down; between adjacent rows, or running up, they turn
  halfway. `a_wire_turns_on_the_first_line_below_its_source` is the test.
- **Wires per node**: every parameter typed `node` is a wire into its own port, the k-th into
  port k (`node_wires`); an empty value is an unwired port; a wire past the node's ports lands on
  port 0. `take_pending_connection_to_port` reports the port. Splicing a dragged node onto a wire
  takes only a wire into port 0. `every_node_parameter_is_a_wire_into_its_own_port` is the test.
- **Swap on drop** (`set_swap_on_drop(true)`, off by default): a node dropped on another trades
  cells with it, and `take_pending_swap` hands the host (dragged, other) to trade what else they
  own. The swap target (`swap_target`) wins over a wire; turn it off for multi-node drags. A
  snapped drag only stands where it could land (nearest free crossing via `find_empty_cell`, or a
  swap target), so `drop_target_cell_rect` is where it is.
  `a_node_dropped_on_a_node_swaps_with_it` is the test.

## Markdown: `MarkdownView` and `DocEditor`

Opt-in features for the note clients.

- **One set of rules.** Both read Obsidian's syntax by `cce_core::markdown`: the reading view's
  blocks come from its parser (the `markdown` feature turns on cce-core's), and the DocEditor,
  which takes no dependencies, uses its leaf rules (`tag_name`, `split_target`, `is_url`,
  `embed_size`, `fmt::percent_decode`), as cce-vault's index does. A tag drops a trailing `/`,
  a `#subpath` is trimmed and an empty one is none, any `scheme:` leaves the vault, and a size
  of `0` is no size — in all three. `links_tags_and_sizes_read_as_the_reading_view_does` is
  the editor's test.
- **`markdown`** — `widget::markdown`, the reading view: cce-core's blocks laid out at a width
  into draw items and click targets (`layout`, `Layout::paint` / `paint_scaled`). cce-notes'
  reading mode and cce-grid's note cards.
- **Links, tags, the quote bar and the highlight take the theme's colours**, `style.text.link`
  (`color::text_link_color`), `style.text.tag` (`color::text_tag_color`),
  `style.text.quote_bar` (`color::text_quote_bar_color`) and `style.text.highlight`
  (`color::text_highlight_color`), in both views. A link with no target is the link colour
  darker and greyer (`color::text_link_unresolved_color`, derived in OKLab); a tag's and a
  highlight's washes are their colours made translucent (`color::text_tag_background_color`,
  `text_highlight_background_color`). No derived colour has a key of its own. The code wash,
  `style.text.code_background` (`color::text_code_background_color`), is set directly, alpha
  included; it also fills an image's box until the image loads. `EditorTheme::new` reads them once, so an editor built before a
  reload keeps the old colours.
- **`doc_editor`** — `widget::doc_editor::DocEditor`, the editor with live preview: markup is
  hidden except on the caret's lines (the selection's, or the whole fenced block the caret is in),
  where it shows dimmed; `preview = false` is source mode.
  - `buffer`: lines, caret/selection as (line, byte), merged typing/deleting undo runs, a
    `Change` log. `preview`: styles ONE line (block kind plus inline segments mapped 1:1 onto
    source bytes — markup is hidden, never replaced, so caret maths never translates).
    `layout`: one styled line wrapped into runs with the x of every byte
    (`ShapingMeasure::offsets`).
  - Incremental: a line is shaped only when drawn and changed; undrawn lines keep an estimated
    height. `a_long_document_shapes_only_what_shows` is the test.
  - Host-driven, not a registered widget: the app forwards keys, presses, motion and the wheel,
    calls `prepare` then `paint_prepared_with` (with a link resolver), and gets `Response`s
    (`Follow(Target)`). Undo/redo are the host's `Application::undo` / `redo` calling
    `DocEditor::undo` / `redo`.
  - Measure with the app's own font set: `DocEditor::new(.., system_fonts)` must match
    `Application::load_system_fonts`. A width includes trailing spaces.
  - **Frontmatter is a Properties table** while the caret is outside it (`preview::properties`,
    `style_property`): keys in a column, values inline-styled, list values as pills,
    `true`/`false` as a checkbox that flips the bytes in place (`LineLayout::toggle`, undoable).
    The caret anywhere in the block shows it raw. `set_text` starts the caret past it.
  - The caret does not blink (a blink is a frame every half second).

## Glyphs in widgets

Every symbol is a cce-icons glyph (the rule is in `CLAUDE.md`). `PaintCtx::icon(name, rect,
color)` tints like adjacent text and rasterises at twice the rect; `icon_untinted` is for the
self-coloured `weather-*` set. Underneath: `upload_icon_tinted` / `icon_tint` / `icon_pixels`
(`icon_source` names the glyph an image id holds). A vertical `ButtonStrip` / `Paginator` takes
glyphs by name (`with_icons`). A widget a host places (`compose::place`) and paints
(`paint_root_into`) reaches it whole: glyphs, strokes, arcs and discs included.
`a_placed_dropdown_paints_its_arrow_glyph` and `a_placed_graph_paints_its_wires_and_ports` are
the tests.

## Other widget notes

- **`LineEdit`** (`widget/line_edit/`): the text, caret, selection and keymap of one line of
  text an app draws itself (cce-browser's URL bar and dialogs, cce-sheets' cell editor,
  cce-documents' fields). An Alt or Ctrl chord types nothing: it is left to the host as
  `Ignored` (AltGr is not Alt). `an_alt_chord_types_nothing` is the test.
  - **Measuring and drawing it** (`line_edit/marks.rs`): `LineMarks::of(fs, &edit, size,
    family)` reads what it shows off the run shaped as the renderer draws it — the caret's x,
    the selection's and the composition's spans, `x_of` / `byte_at` (into the shown text),
    `index_at` (back into the edit's text, across a mask or a composition), `width` — right in
    either direction. `keep_caret(shift, avail)` is the scroll that keeps the caret in a text
    area too narrow for the line. `paint_line(pc, area, origin_x, &marks, &LineLook, placeholder,
    caret)` draws the selection, the text (or the placeholder), the composition's underline and
    the caret inside `area` and returns the caret's rect, which the edit with the keyboard
    claims (`text_input::claim`). The well around it is the app's.
    `marks_stand_on_char_boundaries_in_multibyte_text` is the test.
  - **A line inside the app's own popover** (a dialog that registers its plate as a popover)
    sets `LineLook::bounds` to that popover's rect: the engine's text clamp keeps page text out
    from under an open popover and knows the popover's own text by its bounds, read before the
    area's clip narrows them. Without them the line's text is clamped away.
    `a_line_inside_its_popover_keeps_its_text_when_it_says_so` is the test.
  - The app reports the caret and composition (`docs/runtime.md`).
- **Clipboard** (`widget::clipboard`): one synchronous text pair, `copy_to_clipboard` /
  `read_from_clipboard`, behind every widget's copy, cut and paste — `wl-copy` / `wl-paste`
  (`xclip`) on Wayland, `NSPasteboard` on macOS, the page's clipboard events in a browser
  (`docs/platforms.md`).
- **Vertical text** (`text::set_vertical_text(Some(bar_thickness))`): the status bar's
  mode on a screen edge — labels stack their characters at 1.05 line height. Process-wide.
- **Settings-page sections** (`compose/section.rs`, `compose/form.rs`): `PageFlow` places sections
  in a masonry of columns at least `grid_min_col_width` wide; `PageLayoutBuilder` draws each once;
  a section's contents are a `Form` on `scene::layout` (widgets, text, rows, `block`s, `rule`s,
  `space`, `draw`, `fill`), placed by `SectionContext::place` in declaration order with the
  ladder's spacing. `lay_row(rect, &[Cell])` lays one list row's cells `list_gap()` apart.
  They paint onto any `P: AsMut<PaintCtx>` (a `PaintCtx`, or a host's page buffer around one):
  a widget piece is `place`d in its block and painted there with `paint_root_into`, and a
  section's `finish` carves its own well under relief styling (`PaintCtx::section_well`, radius
  20, after the content; a focused section keeps the 1px outline, the section walk's
  feedback). A section's content clip is its content box snapped out to whole logical pixels,
  since the scissor truncates a fractional edge. cce-system-interface is the user.
- **Laying out a set of widgets** is the box model, `scene::layout` (measure → arrange), or a
  `Form` on it (above). A widget's block is its content plus its label strip
  (`WidgetHost::label_strip`).
