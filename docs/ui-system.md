# Screen-space UI panels

The shared UI content lives in `src/game/ui/`. The module is split into
`mod.rs` (shared types and entry points), `builder.rs` (panel content),
`trays.rs`, `panels.rs`, `queue.rs`, `roster.rs`, `settings_menu.rs`, `network_menu.rs`, `tooltips.rs`, and `tests.rs`. The game starts with the
experimental ImGui presentation (`src/game/ui/imgui.rs`), and F11 switches
between it and the classic layout. `src/game/AGENTS.md` points contributors
here. Add new controls to the shared `PanelBuilder` content and route their
`Target` actions through `GameState::activate_target`, so both views work.
The city interior draws a separate 19-hex world map with the normal renderer.
Map clicks use the interior camera and issue orders to copies. Its small status
panel uses the shared `PanelBuilder` and `Target` dispatch and participates in
the City / Building view scope.

ImGui uses native windows, buttons, scrolling, drag/drop, hover tooltips, and
input capture. It is drawn at the end of the existing Vulkan render pass.
Action buttons in both presentations show costs and work times but not their
keyboard shortcuts, which are in their tooltips: hovering never changes a
button's text. Debug buttons keep their keys visible.
Unit orders and a city's priority chips use shared vector-icon toolbars; hovering an
icon gives its action name, shortcut and explanation.
An ImGui panel fits its content to whatever width it has, however narrow the
player makes it, so no text or icon runs into another: a line of text wraps
between words (`wrap_spans`; a label ending in one space stays with its value,
"DEFENSE 20"); a row of buttons takes as many columns as its widest button's
text leaves room for, so a pair such as Grow and Gather stacks when it can't sit
side by side, and a compact button's price and turns go under its label, then
wrap, before its text would leave the button (`button_grid`, which
`measure_panel` uses too); a catalogue card keeps a fixed room for its unit's
pictogram before the name and drops its price, which its tooltip gives, when the
two don't fit (`split_button`); a queue row's label wraps and the row grows
(`queue_row_lines`); a queue title wraps, its Clear button moving under the
title; a setting's control goes under its name, a choice becoming a
drop-down list when its buttons don't fit (`setting_fit`); and an order icon
moves aside for its cooldown badge. Text drawn on a
button is clipped to it, as ImGui clips its own labels.
`ImGuiLayoutState` measures rows in ImGui's logical pixels and docks visible
windows without overlap. A window follows the dock as its content changes
until the player drags its title bar or resize grip; moved windows reserve
their space so other panels avoid them. The production queue and the city tray's production catalogue have bounded heights and scroll internally when their content grows.
Hold Ctrl to show the title bars, collapse buttons, and resize grips for
arranging panels. During normal play, expanded panels hide their title bars;
collapsed panels keep a short title bar so they can be expanded again. While
Ctrl is held and a panel is being dragged, ImGui owns its position and size
until the drag ends, so dock previews remain stable. Showing title bars keeps
panel outer rectangles fixed, including automatically placed and player-sized
panels. While Ctrl is held, the title bar takes space inside the panel; content
can scroll until Ctrl is released.
The `imgui` dependency enables its `docking` feature. Selection, Production
Queue, and Debug can dock to one another. The transparent dockspace starts
below the ImGui status bar but does not accept drops onto the empty map;
this keeps a drop on a panel's bottom target from becoming a screen-wide split.
Docked rectangles are reserved in the automatic layout, and floating windows
are clamped below the status bar. Docking uses a transparent drag payload so
the chosen split target stays visible, and floating sizes are preserved across
dock and undock. Inspect stays transient unless docked into an outer box, where
it keeps the most recent hover summary visible.
**Every new panel is a full ImGui panel:** it gets a slot (`SLOT_TITLES` and
the slot constants in `imgui.rs`, a zone in `plan`, a measured size and a
`render_imgui_window` call), so it can be dragged, resized, docked and put in
a box like Selection or Debug. Only static chrome, such as the status bar, is
a fixed window. The turn strip (`roster.rs`, `Row::Roster` rows of chips,
slot `UNITS`, starting at the bottom center) is an example; its chips are
drawn with the map's own token and city tower geometry through the window draw
list (`paint::push_chip_icon`). `Zone::BottomCenter` (`dock.rs`) places it:
centered on the bottom edge if that's free, else beside whatever is in the
way, else above it. In the classic layout it docks there after the debug
panel. Its chips are keyed by `RosterKey` (a city's production or idle
workers, a city's or Barracks' idle queue, a group of units of one kind, or
one unit), which the `Roster*` targets carry. An idle queue's chip
(`RosterKey::Waiting`, from `idle_queues`) is rimmed in `WAITING_TEXT` red
(`RosterChip::warning`).
A queue whose first item waits has one account, `GameState::head_wait`
(`city/economy.rs`: the item, what it waits for, how much the stockpile is
short, and whether the queue works nothing else, `idle`), which the panels'
line (`queue::waiting_line`), the turn strip, End Turn's label and tooltip
and the map read; `queue::idle_word` is the one place the word for an idle
queue is written.
The settings menu (`settings_menu.rs`, slot `SETTINGS`) is another: Escape
opens it (`GameState::press_escape`, `game/settings.rs`) and it shows only
while open. It is a full panel rather than a fixed modal, so the player can
move or dock it and keep playing. Both presentations open it centered on the
screen, over the map, without taking room from the docked panels: ImGui's
`plan` centers the `SETTINGS` slot until the player moves it, and classic
places it last (`place_settings`), so it draws on top and `Layout::button_at`,
which prefers the last-placed button, gives it the clicks; `build_ui` draws
it as a layer of its own (`Layout::overlay`), after the other panels'
buttons, so none shows through. ImGui draws it opaque for the same reason.
Its content, `settings_panel_content`, has a `Row::Heading` for each
`Setting::group` and a `Row::Setting` for every entry of `Setting::ALL`,
then the City Yields overlay control, Multiplayer (`Target::OpenMultiplayer`), Close
(`Target::CloseSettings`), and Quit. Multiplayer swaps the menu's content for its
Multiplayer page (`network_menu.rs`, the same `SETTINGS` slot), so the menu is never taller
than one page; its typed fields are `Row::Field`s, which ImGui draws as text boxes (edits come
back as `Action::Text`) and classic as a button that starts typing into the field
(`Target::EditNetField`; `App` then hands it keys until Enter, Tab or Escape, and does
Ctrl+V/C/X itself with `src/clipboard.rs`). ImGui's text boxes get the clipboard from the
context (`App` sets its backend) and keep to what the field takes while edited (`CleanField`,
through `NetField::clean`). While hosting, the join code and the LAN address are
`Row::TitleWithButton`s with a COPY button (`Target::CopyJoinCode`, `CopyHostAddress`). Its
buttons leave a `NetRequest` that `App` carries out with `src/net` or the clipboard. A setting's `control` picks
its widget: ImGui (`render_setting`) puts the name in a label column and
beside it a checkbox (`Control::Toggle`), a slider showing `value_text`
(`Control::Slider`), or a button per value with the current one gold
(`Control::Choice`; a combo box past `Control::MAX_BUTTONS` values), with
the setting's description as the tooltip. Classic expands the two rows
(`builder::classic_rows`, `settings_menu::classic_setting_rows`) into a
gold heading line and, for each setting, one row
(`Row::LabeledButtons`, classic only) of its name and compact buttons at
the row's end: one per value (OFF / ON for a switch), or < and > beside
the value for a slider or long list.
Every control acts through `Target::SetSetting(setting, value)`, which
clamps to the range. A new setting therefore needs no UI code: add it in
`game/settings.rs` as its module comment describes (its name, tooltip and
values are entries in `text/menus.ini`), and both presentations show it
with its control.
**Text files.** The settings menu's words come from `text/menus.ini`
through `text!` and `tooltip!` (`game/strings.rs`), by tag; the rest of
the UI's text is still in the code and moves to `text/` area by area
(#341). In an area that has moved, new text is an entry in its file, not a
literal: `docs/text.md` has the format and the tests that check it. Both
presentations take the same strings from the shared content, so an entry
changes both.
At the start of each frame, synchronize native ImGui dock state before planning
floating positions: ImGui commits a highlighted drop in `NewFrame`, and using
the previous frame's floating state can immediately undo that split.
Selection and Production Queue may disappear together when city management
closes. Remember their adjacent dock relation while visible, then restore that
relation if ImGui removes an inactive split before they reappear. A lost dock
position is never reused as a floating position over another panel.
Debug can also be docked beside a contextual Selection group. City, barracks,
unit, and group menus and an open city interior each remember their own Debug
docking relation, by `SelectionKind` (`imgui.rs`). When a
menu closes or changes kind, Debug returns to its previous standalone position
or persistent panel group; reopening that kind of menu restores its split.
The last standalone position is frozen while dragging and is not overwritten
during an undock transition.
Dock relation detection walks the native dock tree, so it works beside a
nested Selection+Queue group as well as beside Selection alone.
Beside the turn number, the status bar (and the classic top bar) shows the
player's stockpile and supply from `GameState::status_line` (`stockpile_line`
then `supply_line`), one shared line of colored spans, so both presentations
read the same. The latest notice
follows in the room left (classic: between Menu and End Turn; ImGui: the
rest of the first line, up to End Turn, since Menu and the view controls
share the second line); a notice too long for it is cut after its last whole word
that fits, ending in "…" (`fit_text`, `text.rs`), and ImGui shows all of it
when the notice is hovered. Past ASCII, game text uses only the punctuation in
`font::UI_PUNCTUATION` (`font.rs`: the middle dot, em dash, times sign and
ellipsis), which the classic font has and ImGui's fonts are given
(`IMGUI_GLYPHS`, `imgui.rs`, built from it), and the map's icon characters
(below); `ui_text_uses_only_the_shared_glyphs` reads every string literal in
`src/` to hold to that, and checks ImGui's fonts have each glyph.
`style_imgui` (`imgui.rs`) gives a context the game's fonts and style, for
`App` and for the tests (`ImGuiScreen::styled`): Segoe UI where the system
has it, or else the classic font's IBM Plex Mono, which has the punctuation
(ImGui's own font doesn't).
Resources and turns appear in text as icons: a UI string may hold the icon
characters of `map_icons.rs` (`FOOD_ICON`, `WOOD_ICON`, `METAL_ICON`,
`TIME_ICON`), which the classic font (`font::Face::width` and `push`) and
ImGui (`rich_text`, `rich_width`, `rich_button` in `imgui.rs`, drawing the
icon's triangles into the window draw list) both draw as the map's own
icons. ImGui buttons with icons are drawn blank with the lines laid over
them; a dimmed button dims its icons too.
On its second line, after Menu, the ImGui status bar names the active
**Default**, **City / Building**, or **Troop** view (VIEW: ...), followed by
its layout controls, on a line of their own below the first line's text.
Nothing on either line overlaps at any window width the game allows (at least
`MIN_WINDOW_SIZE`, `ui/mod.rs`, in logical pixels): as room runs out the stockpile drops
its change a turn (`brief_status_line`), then whatever of it still won't fit
before End Turn, along with the notice; the view's name shortens to the bare
name and then goes, left to the layer button's tooltip.
`imgui_status_bar_keeps_its_parts_apart_at_every_width` sweeps the widths.
**EDIT VIEW** means panel placement and **+ BOX** belong to that view;
switching to **EDIT OUTER** creates boxes shared by all views and makes a moved
floating Debug panel use the same placement everywhere. A view box is hidden
outside its view, while its docked panels and layout remain available when
returning. The floating Debug panel inherits Default's position in City /
Building and Troop until dragged or resized in that view. Moving it in Default
updates views that still inherit; moving it while editing Outer applies the
position to all views and clears local overrides. Outer boxes keep one
position across all views.
In City / Building or Troop, **Ctrl+Shift+R** (or the status-bar Reset button)
clears that view's Debug position and the contextual dock relations of every
kind of selection shown in it (`SelectionKind::view`: an open interior's too,
in City / Building) so it inherits
Default again. It leaves the view's boxes and other panels in place.
One floating panel resets on its own by a double-click: on its title bar, it
goes back where its view's layout places it and keeps its size
(`ImGuiLayoutState::reset_position`); on its resize grip, back to the size the
layout gives it, keeping its place (`reset_size`). A panel's geometry says
whether its place (`manual`) and its size (`sized`) are the player's; dragging
either makes both the player's, and a reset gives one back to the layout (as it
was before the double-click's first click), which the saved layout keeps. The
layout's place and size are the automatic ones (the settings menu centered);
for Debug in City / Building or Troop, they are what RESET restores, Default's
placement, and once both are back that view follows Default's Debug again.
ImGui collapses a window double-clicked on its title bar: `title_double_clicked`
tells it an item is hovered that frame, so only the arrow collapses a panel.
Docked panels, captured ones in their boxes among them, take their place and
size from the dock and have no reset.

Hold Ctrl and drag Selection, Production Queue, Debug, or Inspect into a box.
Each box has an X to remove the whole container. A dropped city, barracks,
unit, group, or queue panel becomes a persistent window in that box; its
controls stay live after selection changes. Its actions carry the structure
or unit's stable ID and focus that entity before invoking the shared `Target`
action (`focus_pin`): through the map's own entry points (`set_selection`,
`open_city`, `open_barracks`, leaving any other view first), and not at all
when it is already the selection or the open view, so an armed button stays
armed and a second click disarms it; nothing while a turn plays out. Its
button tooltips describe its own unit or city (a tooltip `Subject`), not the
selection's. Debug is already persistent and docks directly. Moving a captured
panel out of every box returns it to contextual behavior. Deleting a box
releases its contents, and any new game clears captured panels while keeping
the boxes: a scenario switch or load, by key or Debug button, or a network
game (`GameState::generation`, checked at each frame's start), since a new
game reuses city and unit ids. Every new always-visible ImGui window can dock
in a box without special layout offsets. A new contextual panel needs a stable
identity and a renderer for captured content, as the city, barracks, unit,
group, and queue examples show.
Give draggable content a payload type scoped to its panel identity, so rows
from two city queues cannot reorder one another.
Floating Selection panels remember dimensions by context kind (interior, city,
barracks, unit, or group), so resizing the city controls does not stretch unit controls.
Edge resizing is disabled; hold Ctrl and use the lower corner grip to resize.
Docked windows use ImGui's geometry, while undocked panels use the measured
layout. Floating panel height is corrected from the previous rendered frame.
The world, tile overlays, and selection rectangle remain in
the game's renderer. Native window positions can be moved and resized while
the game is running. Queue panels show the full queue inside a scrollable
window; their rows share the same reorder and remove game actions as classic.

The classic layout uses reusable placement code in `src/game/ui/dock.rs`.

`PanelBuilder` is the content primitive. Add rows with `text`, `bar`, `gap`,
`buttons`, `compact_buttons`, `queue_item`, `title_with_button`, `heading`, or `setting`; `size()` measures the finished panel.
A button is a `ButtonSpec`, built as
`ButtonSpec::new(target, label, hint).queued(q).unavailable(why).armed(a)`:
gold when queued, and disabled exactly when it has a reason it's unavailable,
which its tooltip gives in red (`subject_tooltip_lines`) in both presentations.
Availability is decided only there, where the button is built (a unit's orders
in `unit_action_unavailable`, a group's when no member can take one), so a
button and its tooltip can't disagree; the tooltip adds only a caution on a
button that's on (what the stockpile is short of for a build, which then waits).
A build card the stockpile can't pay for this turn is `.short(true)`
(`GameState::build_shortfall`, the one test, which the tooltip's caution uses
too): still pressable, it's drawn with a red rim, a dark red ground (gold kept
when queued) and its price in red, in classic (`paint::draw_button`) and ImGui
(`short_style`, `short_price`); a disabled button is never drawn short.
A plan sent in a network game leaves every button as it was: one that changes
the plan takes the turn back (`docs/multiplayer.md`). A
`Layout` owns shapes, buttons, panel hit boxes, scroll regions, and a `Dock`.
`Layout::dock_panel(panel, Zone::BottomLeft)` places the measured panel and
registers its render and hit-test geometry together. Available zones are
`BottomLeft`, `BottomRight`, `TopLeft`, and `TopRight`.

Panels in a zone stack vertically from its corner. When that column fills,
the dock moves to another column. Zones share the same occupied rectangles,
so a new panel cannot overlap a panel from another zone. The top status bar
is reserved. Placement order determines priority: persistent command panels
are placed first, then the debug panel; transient hover panels are placed
afterward. Do not calculate offsets from panel counts or hardcode another
panel's dimensions.

```rust
let mut panel = PanelBuilder::default();
panel.text(SMALL, vec![("EXAMPLE".into(), LABEL_TEXT)]);
panel.bar(0.5);
layout.dock_panel(panel, Zone::BottomLeft);
```

For content that must scroll, ask the layout for
`remaining_height(Zone::BottomLeft, panel_width)` after the preceding panel
has been docked. Choose a visible row count from that measured space, then
build and dock the scrollable panel. The city and Barracks queues demonstrate
this. Their scrollbars and wheel hit regions are registered by the same panel
placement. `dock_panel` returns `None` when a whole panel cannot fit anywhere
inside the safe area; large content must choose a bounded or scrollable form.
Within a panel, a list that grows goes in a `scroll_list` (`Row::ScrollList`:
lines of text, rows of buttons and queue rows, scrolled a whole row at a time
by a `QueueKind`'s scroll position). `layout()` calls `fit_height` with the
room left before docking the tray, which shortens the tray's scroll lists (to
two rows' room), then its catalogue (to two cards), then the lists (to one
row) until it fits; the city tray's workers and jobs are one such list, so the
tray stays on screen however many jobs are placed. The list registers its
scrollbar and wheel region like a queue panel's. ImGui gets every entry in
place of the list (`flat_rows`) and scrolls the whole window instead.
`queue_item` registers its row body for drag reordering and a separate small X
button for removal. A queue panel's title is a `title_with_button` row
(`Row::TitleWithButton`): the text, and its Clear button (`Target::ClearCityQueue`,
`Target::ClearBarracksQueue`) at the right end of the same line, so it costs no queue row. Route row dragging through `GameState::start_queue_drag_at`,
`update_queue_drag_at`, and `finish_queue_drag_at`, so the city and Barracks
queues share the same hit-testing behavior. A row of buttons that reorder by
dragging (`reorder_buttons`, `Row::Reorder`: the city tray's priority chips)
registers each button as a drag region of its `QueueKind` too, so classic drags
it the same way, and a press let go on the chip it started on is its click;
ImGui gives each of its buttons a drag source and target (`render_buttons`).

Cursor-following tooltips, button tooltips, and the selection box are
overlays, so they use their own anchors (so is the centered settings menu). Ordinary status, control,
hover, and debug panels belong in a `Zone`.
Both presentations show the same tooltip text, from `subject_tooltip_lines`
(`tooltips.rs`): on every panel button, a queue row and its X, a turn strip
chip (`roster_tooltip`), End Turn and Menu. ImGui shows it on the hovered item
(`show_tooltip`, `imgui.rs`); classic picks what's under the cursor, a button
before a chip before a queue row (`classic_tooltip_at`), and shows no row's
tooltip while a row is dragged. A new kind of hoverable thing gets its text
there and a call in each presentation.

Add a layout test when adding a new panel or zone behavior. Useful assertions
are that panel rectangles do not overlap, buttons remain inside their panel,
and the same target is clickable after the screen size or content changes.

The city tray uses a shared `Row::BuildingCatalog` for unit and building production cards, separated by headings. ImGui renders it as a five-row child window with native scrolling; classic renders a five-row inset (fewer when `fit_height` needs the room) with wheel and draggable scrollbar. While the open city is placing a card's job on the map, a gold line over the catalogue, in place of the tile-assigning hint, says how to place it and to cancel, with a Cancel Placing button (`Target::CancelPlacing`) under it; the picked card is gold. City unit cards use the same pictograms in both presentations. The top-bar Menu button opens Settings without leaving a city view; the City Yields overlay toggle lives there.
