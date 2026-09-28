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
- **Every new ImGui panel is draggable and dockable.** Give it a slot: a constant and an entry in
  `SLOT_TITLES` (`imgui.rs`), a zone in `ImGuiLayoutState::plan`, a measured size in
  `draw_imgui`, and a `render_imgui_window` call. That makes it movable, resizable, dockable
  and boxable (Ctrl) like Selection, Debug or the turn strip (`UNITS`). Only static chrome,
  such as the status bar, may be a fixed `ui.window` with its own flags.
- **Classic placement.** Place persistent panels with `Layout::dock_panel(panel, Zone::..)`
  (zones in `dock.rs`); never position one by hand or compute offsets from another panel's
  size. `layout()` docks the persistent panels and `layout_with_hover()` adds the hover panels;
  drawing (`build_ui`) and button clicks (`click_ui`) both use `layout_with_hover()`, so a drawn
  button is clickable. Queue dragging, wheel scrolling, scrollbars and `update_hover` use
  `layout()`, so scrollable or draggable content belongs in a persistent panel, not a hover
  panel. Placement order is priority: command panels first, then debug, then the turn strip.
  A button takes a click before a turn strip chip under it (the centered settings menu).
- Cursor-following tooltips and the selection box are overlays with their own anchors. The
  settings menu is the one panel placed outside the dock: centered, in both presentations.
- Colors are linear and the swapchain is sRGB: dark panels need values around 0.01-0.05. ImGui
  style colors (`app.rs`) and draw-list colors are linear too.
- Map geometry drawn in a panel (the turn strip's chips) comes from the world drawing code
  (`draw::push_unit_token`, `draw::push_city_tower`, via `paint::push_chip_icon`), built Y-up;
  ImGui's draw list is Y-down, so flip it.
- **Icons in text.** Prices, stockpiles and turns use the icon characters of `map_icons.rs`
  (`stock_icons`, `turns_icon`, `cost_hint` in `text.rs`); both presentations draw them as the
  map's icons: classic in `font::Face`, ImGui through `rich_text` / `rich_button` (`imgui.rs`).
  Render game text in ImGui with those, never `ui.text`, or an icon shows as `?`.
- **New kind of turn task** (research, say): a `RosterKey` variant, its place in
  `roster_tasks` (civilian tasks before the unit groups), its chip in `roster_chip`, what a
  click does in `roster_select`, and its hint in `roster_hint` (`roster.rs`).

## Recipes

- **New piece of ImGui layout state** worth keeping between sessions (where the player put
  something): a line in `ImGuiLayoutState::to_text` and its reading in `from_text`
  (`imgui.rs`), with the round-trip test. Save only what the player chose; what the layout
  computes is computed again.

- **New unit button:** a `UnitAction` variant (`mod.rs`), a `ButtonSpec` in the tray's button
  list (`unit_buttons` or `group_tray_for` in `trays.rs`), its tooltip text (the `UnitAction`
  match in `unit_action_text`, `tooltips.rs`), and an arm in `activate_target` (`mod.rs`). Add
  a hit-test unit test in `tests.rs`.
- **New panel:** shared content in a `GameState` method returning a `PanelBuilder`; dock it in
  `layout()` for classic and give it an ImGui slot (above). Add a layout test (no overlap,
  buttons inside their panel) in `tests.rs`, and for ImGui extend the `plan` tests in
  `imgui.rs` (their size arrays have one entry per slot). A slot's place in `PLAN_ORDER` is
  its priority within its zone.
- **New player setting:** no UI change. `settings_menu.rs` builds a row (name, value, < and >)
  for every entry of `Setting::ALL`, so add the setting in `game/settings.rs` (its module
  comment lists the steps). The settings menu tests in `tests.rs` walk `Setting::ALL` too. A
  setting that needs a different kind of control (text entry, a slider) is a new `Row` variant
  (see Both presentations).

## Verifying

`tests.rs` covers layout and hit-testing without a window; `imgui.rs` has the dock-plan tests.
After UI changes run `cargo build --release` and look at a screenshot
(`cargo run -- --screenshot out.png --scenario cities`); screenshot mode shows ImGui, and the
classic view needs a temporary `use_imgui: false` in `App::new` (revert it by editing the line,
not with `git checkout`, which would drop your other changes to `app.rs`).
