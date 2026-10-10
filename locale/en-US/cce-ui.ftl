# cce-ui's own words, in English: the messages every translation of the toolkit gives in
# its own language, as locale/<tag>/cce-ui.ftl in a translation directory (see
# cce_core::l10n). An id names the message, never its text; what a choice does is keyed
# elsewhere, so a translation changes only what it says.

## The standard context menu
menu-cut = Cut
menu-copy = Copy
menu-paste = Paste
menu-select-all = Select All
menu-clear = Clear
menu-copy-path = Copy Path
menu-collapse-controls = Collapse controls
# The menu's header rows for a widget bound to a config key.
menu-config-file = File: { $file }
menu-config-key = Key: { $key }

## The tree list
tree-expand = Expand
tree-collapse = Collapse
tree-expand-all = Expand All
tree-collapse-all = Collapse All
tree-copy-key = Copy Key
tree-copy-value = Copy Value
tree-delete = Delete

## A pane's corner dock
dock-expand = Expand
dock-collapse = Collapse
dock-detach = Detach
dock-reattach = Reattach

## Controls
# A search box's placeholder (`TextBox::with_search`): the tree list's, and any app's.
search-placeholder = Search...
button-copy = Copy
ramp-delete-key = Delete
keybind-recording = [ Press Keys... ]
keybind-none = None

## The date picker (`DatePicker`)
# Its header: the month's name and the year, as a calendar heads a month.
date-picker-month = { $month } { $year }
date-picker-january = January
date-picker-february = February
date-picker-march = March
date-picker-april = April
date-picker-may = May
date-picker-june = June
date-picker-july = July
date-picker-august = August
date-picker-september = September
date-picker-october = October
date-picker-november = November
date-picker-december = December
# The weekday row: two letters each, over a column a day cell wide.
date-picker-mon = Mo
date-picker-tue = Tu
date-picker-wed = We
date-picker-thu = Th
date-picker-fri = Fr
date-picker-sat = Sa
date-picker-sun = Su
# The footer's buttons.
date-picker-today = Today
date-picker-tomorrow = Tomorrow
date-picker-clear = Clear

## The document editor's Properties table
doc-properties = Properties
doc-empty = Empty

## Accessibility
# What a colour selector's hex field is, said by a screen reader after its name.
a11y-colour = colour
