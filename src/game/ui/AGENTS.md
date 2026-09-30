# src/game/ui/

The screen-space UI: shared panel content and its two presentations, ImGui (the default,
`imgui.rs`) and classic (F11, drawn by `paint.rs`). Read `docs/ui-system.md` before editing;
this file is the short list of rules. The module table in `src/game/AGENTS.md` says what each
file holds.

## Rules

- **Both presentations.** Build panel content once with `PanelBuilder` (`builder.rs`) and route
  every action through a `Target` and `GameState::activate_target` (`mod.rs`), so the ImGui and
  classic views show the same thing and do the same thing. A new kind of row is a `Row` variant
  that `builder.rs` places (classic) and `imgui.rs` measures (`measure_panel`) and renders
  (`render_imgui_panel`).
- **A button that changes the plan** (an order, a build, a citizen, a worker) stays on while a
  network game waits for the others' plans: the method it calls refuses only while
  `is_playing_out`, and the change takes the turn back (`take_back_on_new_orders`,
  `multiplayer.rs`). Never refuse an order on `is_resolving`, which also holds while waiting.
- **A button that can be off says why.** Build buttons with `ButtonSpec::new(target, label, hint)`
  and its `.queued(..)`, `.unavailable(reason)`, `.armed(..)` (`builder.rs`): a button is
  disabled exactly when it has a reason, which its tooltip shows in red in both presentations, so
  never decide a button's availability anywhere else (not in `tooltips.rs`).
  `every_disabled_button_says_why_in_every_scenario` (`tests.rs`) walks every view.
- **Every new ImGui panel is draggable and dockable.** Give it a slot: a constant and an entry in
  `SLOT_TITLES` (`imgui.rs`), a zone in `ImGuiLayoutState::plan`, a measured size in
  `draw_imgui`, and a `render_imgui_window` call. That makes it movable, resizable (a
  double-click on its title bar or grip resets its place or size), dockable (to the other
  panels and to the game window's edges, where each view keeps its own docking:
  `update_game_dock`) and boxable (Ctrl) like Selection, Debug or the turn
  strip (`UNITS`). Only static chrome, such as the status bar, may be a fixed `ui.window` with
  its own flags. The game window's dockspace must keep passing the mouse through to the map
  (`draw_game_dockspace`; `imgui_a_panel_docks_to_the_window_edge_and_the_map_keeps_the_mouse`).
- **Classic placement.** Place persistent panels with `Layout::dock_panel(panel, Zone::..)`
  (zones in `dock.rs`); never position one by hand or compute offsets from another panel's
  size. `layout()` docks the persistent panels and `layout_with_hover()` adds the hover panels;
  drawing (`build_ui`) and button clicks (`click_ui`) both use `layout_with_hover()`, so a drawn
  button is clickable. Queue dragging, wheel scrolling, scrollbars and `update_hover` use
  `layout()`, so scrollable or draggable content belongs in a persistent panel, not a hover
  panel. Placement order is priority: command panels first, then debug, then the turn strip.
  A button takes a click before a turn strip chip under it (the centered settings menu).
  Rows that can grow without bound (a city's workers and jobs) go in a `scroll_list`, which
  `fit_height` shortens until the tray fits, so a tray never outgrows a short screen.
- Cursor-following tooltips and the selection box are overlays with their own anchors. The
  settings menu is the one panel placed outside the dock: centered, in both presentations.
- Colors are linear and the swapchain is sRGB: dark panels need values around 0.01-0.05. ImGui
  style colors (`style_imgui`, `imgui.rs`) and draw-list colors are linear too.
- Map geometry drawn in a panel (the turn strip's chips) comes from the world drawing code
  (`draw::push_unit_token`, `draw::push_city_tower`, via `paint::push_chip_icon`), built Y-up;
  ImGui's draw list is Y-down, so flip it.
- **Fit any width.** An ImGui panel can be resized narrow, so text and icons in it must never
  overlap or leave their button: lay a new kind of row out from the width it has (as
  `button_grid`, `wrap_spans`, `setting_fit` do), measure it the same way in `measure_panel`,
  and note what it draws for the test (`note_mark`, `note_text_item`, `note_button_label`;
  `fill_shapes` and `draw_rich` note theirs). `imgui_panels_keep_their_text_and_icons_apart_at_every_width`
  sweeps the panels' widths.
- **Icons in text.** Prices, stockpiles and turns use the icon characters of `map_icons.rs`
  (`stock_icons`, `turns_icon`, `cost_hint` in `text.rs`); both presentations draw them as the
  map's icons: classic in `font::Face`, ImGui through `rich_text` / `rich_button` (`imgui.rs`).
  Render game text in ImGui with those, never `ui.text`, or an icon shows as `?`. A count of
  turns is always the clock and the number (`turns_icon`, or `turns_text` in the UI), never
  "N TURNS" or "NT"; world text (`font::push_text`) draws the icons too.
- **Text files.** The settings menu's and its Multiplayer page's text is in `text/menus.ini`,
  asked for by tag (`text!`, `tooltip!`: `game/strings.rs`, `docs/text.md`); new text there is
  an entry, not a literal. Other areas move there in stages (#341); until theirs does, keep
  their text as it is. In a moved area, a key's name in text or a button's hint comes from the
  key map (`Command::key`, `game/keys.rs`), never a literal such as `"ESC"`.
- **Characters past ASCII** in game text: only those in `font::UI_PUNCTUATION`
  (`font.rs`), which both presentations' fonts carry. A new one goes in that list;
  `ui_text_uses_only_the_shared_glyphs` finds one used and not listed.
- **New kind of turn task** (research, say): a `RosterKey` variant, its place in
  `roster_tasks` (civilian tasks before the unit groups), its chip in `roster_chip`, what a
  click does in `roster_select`, and its tooltip in `roster_tooltip` (`roster.rs`).

## Recipes

- **New piece of ImGui layout state** worth keeping between sessions (where the player put
  something): a line in `ImGuiLayoutState::to_text` and its reading in `from_text`
  (`imgui.rs`), with the round-trip test. Save only what the player chose; what the layout
  computes is computed again.

- **New unit button:** a `UnitAction` variant (`mod.rs`), a `ButtonSpec` in the tray's button
  list (`unit_buttons` or `group_tray_for` in `trays.rs`), when it's off and why
  (`unit_action_unavailable`, `trays.rs`; a group's is off when no member can take it), its
  tooltip text (the `UnitAction` match in `unit_action_text`, `tooltips.rs`), and an arm in
  `activate_target` (`mod.rs`). Add a hit-test unit test in `tests.rs`.
- **New panel:** shared content in a `GameState` method returning a `PanelBuilder`; dock it in
  `layout()` for classic and give it an ImGui slot (above). Add a layout test (no overlap,
  buttons inside their panel) in `tests.rs`, and for ImGui extend the `plan` tests in
  `imgui.rs` (their size arrays have one entry per slot). A slot's place in `PLAN_ORDER` is
  its priority within its zone.
- **New player setting:** no UI change. `settings_menu.rs` builds a `Row::Setting` for every
  entry of `Setting::ALL`, under its `group`'s heading, and its `control` (checkbox, slider or
  choice) picks the widget in both presentations, so add the setting in `game/settings.rs`
  (its module comment lists the steps; its words are entries in `text/menus.ini`). The settings menu tests in `tests.rs` walk
  `Setting::ALL` too. A new kind of control (text entry, say) is a `Control` variant, drawn by
  `render_setting` (`imgui.rs`) and laid out as buttons by `classic_setting_rows`.

## Verifying

`tests.rs` covers layout and hit-testing without a window, in both presentations: classic through
`layout()` and `handle_click`, ImGui through `ImGuiScreen`, a headless ImGui context that draws
the real panels and clicks a button where ImGui drew it (`imgui::DRAWN_BUTTONS`, filled by
`note_drawn_button` for panel buttons and catalogue cards). `imgui` allows one context per process,
so any test that makes one holds `imgui::one_context_at_a_time()`. `imgui.rs` has the dock-plan
tests. `ImGuiScreen::styled` uses the game's fonts and style, for tests of where text goes;
`imgui::DRAWN_MARKS` holds each piece of text and icon drawn, with what clips it.
After UI changes run `cargo build --release` and look at a screenshot
(`cargo run -- --screenshot out.png --scenario cities`); screenshot mode shows ImGui, and the
classic view needs a temporary `use_imgui: false` in `App::new` (revert it by editing the line,
not with `git checkout`, which would drop your other changes to `app.rs`).
