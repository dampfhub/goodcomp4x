# src/game/

Everything game-specific. The rules this code implements are written down in
`docs/game-rules.md` and the keys in `docs/controls.md`; change those in the same commit as the
behavior.

## Shape

`GameState` (in `mod.rs`) is the one struct holding the whole game. Each module below adds an
`impl GameState` block for its concern, so look for a method by concern, not by type.

| Module | Concern |
|---|---|
| `mod.rs` | `GameState` fields, scenario setup (`new`, `city_scenario`, `frontier_scenario`, `world_scenario`), shared queries (`units_at`, `rival_of`, `swap_partner`, `reachable_hexes`), `CONTROLS_HELP` (a startup pointer to `docs/controls.md`), the main tests |
| `orders.rs` | player input and order planning: `ClickMode`, click (move), right-click (attack), swap, ability toggle, hold, guard |
| `order_queue.rs` | multi-turn order queues: Shift-click adds turns (`queue_move`, `queue_attack`, equal lengths for a group), `advance_queues` at each turn's end |
| `group.rs` | multi-unit selection (drag a box, Shift-click adds, Ctrl-click removes) and group orders |
| `turn.rs` | `RESOLUTION_ORDER` and `Step` (unit steps, then the workers'), `update(dt)`, simultaneous step resolution (moves, attacks) |
| `combat.rs` | damage formula, retaliation, combat log helpers |
| `ability.rs` | the abilities and their tuning constants |
| `unit.rs` | `Team`, `UnitType`, base stats, `Unit` and its state-aware `stats()` |
| `ai.rs` | the Red AI |
| `city/mod.rs` | `City`, `Site`, `LaborFocus`, city tuning constants (barracks HP and defense, population cap), setup of the city scenarios (`setup_cities`, `setup_frontier`, `setup_world`) |
| `city/logistics.rs` | roads and Canoe House river corridors, logistics routes (`routes_from_by`), `delivered_share`, tile yields, Mill food share, Cannery/Smelter collection, city and Barracks income |
| `city/rail.rs` | Railhead road connectivity and long-range transfer eligibility; city center is the origin terminal |
| `city/citizens.rs` | citizens: labor focus, the manager and its workers, auto-assignment and reconciling blocked tiles, growth, `resolve_economy`, Field Hospital healing |
| `city/builds.rs` | `Building`, `Build`, `BuildUnit`; city and Barracks queues, building sites, resource support from Forge/Stable, Workshop discount, confirmation, `complete_builds` |
| `city/founding.rs` | settlers founding cities |
| `workers.rs` | workers: each city's pool and job list, Work Camp bases, the workers' last step of the turn (walking, working, going home), capture and death; structures (walls and gates on hex edges, outposts and forts on tiles) and the passability they add (`can_step`, `can_cross`) |
| `city/view.rs` | opening and leaving the city and Barracks views, map clicks while one is open (`city_click`), the yields toggle, `end_planning` |
| `city/interior.rs` | city tactical grid, projecting adjacent troops, independent interior orders, command-post capture |
| `city/tests.rs` | the city tests |
| `hex.rs`, `terrain.rs` | axial hex math, `HexGrid` (shape, tiles, rivers, resources); `Tile` = ground + hills + feature, with yields, route cost, defense |
| `mapgen.rs` | seeded world generation for the F4 scenario (own RNG: a seed always rebuilds the same map) |
| `fog.rs` | fog of war: sight, line of sight, the player's memory of seen hexes |
| `scenario.rs` | scenarios (F1-F4, F12), savestate (F6/F7), instant playback (F8) |
| `settings.rs` | the player's options (`Settings`, one field each, and `Setting`, how the menu lists and steps them), Escape (`press_escape`) and the settings menu's open state; its module comment says how to add a setting |
| `simulation.rs` | tests only: seeded AI-vs-AI games (and games where the player's units follow order queues) in every scenario, board invariants checked each turn, same seed replays the same game |
| `camera.rs` | orthographic camera: pan, zoom, glide, screen/world conversion |
| `draw.rs` | world geometry (`build_vertices`): hexes, terrain, ghosts, attack arcs, units, badges |
| `unit_icons.rs` | unit pictograms (sword, bow, horse head, ...) built from rects, triangles, circles and lines, in the mockup coordinates they were designed in |
| `map_icons.rs` | resource and improvement icons (horse head, ingot, wheat, ore cart, fence, logs) in a hex's top corners, and the food and production icons in yield pips; dark-edged shapes in their mockup coordinates |
| `effects.rs` | attack animations during playback |
| `ui/mod.rs` | screen-space UI entry points (`build_ui`, `click_ui`, `update_hover`, `layout`), its shared constants and types (`Target`, `UnitAction`, `Button`, `Shape`, `Layout`) |
| `ui/builder.rs`, `ui/paint.rs`, `ui/dock.rs` | `PanelBuilder` (rows, measuring, placement); drawing shapes and buttons to vertices; `dock.rs` places panels by screen zone |
| `ui/trays.rs`, `ui/panels.rs`, `ui/queue.rs`, `ui/roster.rs`, `ui/settings_menu.rs` | the command tray (unit, group, city, Barracks); top bar, debug panel, structure hover panel; queue panels with scrolling and drag to reorder; the unit strip of units needing orders; the settings menu (a row per `Setting`) |
| `ui/tooltips.rs`, `ui/text.rs` | button and tile tooltips (`tooltip_lines`, `unit_action_text`); number and text formatting (`quantity`, `ability_text`, `wrap`) |
| `ui/imgui.rs` | dockable ImGui presentation using the shared panel content |
| `ui/tests.rs` | the UI's layout, hit-test and tooltip tests |
| `mesh.rs`, `font.rs` | shape helpers (`polygon` ear-clips concave outlines); TrueType text and the glyph atlas |

## Invariants

- `units` holds living units only. Dead units are removed with `retain` at the end of an attack
  step, which shifts indices, so code that spans a removal uses unit `id`s, not indices.
  `selected` and `group` are cleared before a turn resolves for the same reason.
  A unit has exterior `hp` and persistent `interior_hp`; death in either layer removes its
  exterior body and every interior copy with that source id.
- Everything that asks what a unit can do goes through `Unit::stats()`, which applies abilities
  and siege deployment on top of the base table in `UnitType::stats()`. Don't read base stats
  directly.
- Attack steps read the board as it stood at the step's start and apply summed damage at its
  end; moves in a step are simultaneous. Keep new resolution logic in that shape (`turn.rs`).
- Workers aren't units: those at home are a count on their city, those out are
  `field_workers`, with their own ids. They never block a unit's move. Walls and gates sit on hex
  edges (`barriers`, keyed by `hex::edge`), so passability is per step: anything that walks
  (units, workers, the AI's distances) or routes goods checks `can_step` / `can_cross`, and the
  player's planning checks what they know (`known_can_cross`, `fog.rs`).
- **Screen-space UI: read `ui/AGENTS.md` and `docs/ui-system.md` first.** Its rules (shared
  `PanelBuilder` content for both presentations, every new ImGui panel draggable and dockable,
  classic docking by zone) live there.
- Text: `font.rs` rasterizes printable ASCII from IBM Plex Mono SemiBold (`assets/fonts/`, SIL
  Open Font License) once into one R8 atlas with 4 mip levels, padding glyphs by 8 px so the
  smallest mip doesn't bleed neighbors together. UI text uses coverage glyphs from
  `font::ui(px)` with `px` one of `UI_SIZES` and snaps to whole pixels; world text (`push_text`,
  `push_glyph`) is sized by capital-letter height and scales one 64 px set of signed distance
  fields (measured on a 4x raster), which the shader thresholds so outlines stay sharp at any
  zoom. Distance-field glyphs carry `u` shifted up by 1 so the shader can tell them apart.
- Colors are linear and the swapchain is sRGB: dark panels need values around 0.01-0.05.
- `Camera::view_proj` builds an OpenGL orthographic projection and flips Y itself, because
  glam 0.33's `vulkan::orthographic` flips the Y scale but not the translation.
- The AI must stay deterministic: ties break by hex coordinates. All randomness goes through
  `GameState.rng` (never `rand::random` or the thread RNG) and never depends on hash-map
  iteration order, so a seed replays the same game (`simulation.rs` checks this).

## Recipes

- **New key:** a `KeyCode` arm in `App::window_event` (`src/app.rs`) calling a `GameState`
  method (a key that acts on release or while held needs its own arm, like Escape), and a row in
  `docs/controls.md`, the one description of the controls. `CONTROLS_HELP` (printed at startup)
  only points to that file; don't list keys in it.
- **New unit button or panel:** see the recipes in `ui/AGENTS.md`.
- **Stat or tuning change:** `unit.rs` or `ability.rs`, then every place that states the number
  to players: `ability_text` in `ui/text.rs` (tooltips) and the tables in
  `docs/game-rules.md`. Grep for the old value.
- **New player setting:** only `settings.rs`: a field in `Settings` (and its default), a
  `Setting` variant in `Setting::ALL`, and its arms in the `Setting` and `Settings` matches (the
  module comment lists them). The settings menu shows it in both presentations with no UI
  change; game code reads the field (`self.settings.<field>`), and a row in the settings table
  of `docs/controls.md` describes it.
- **New scenario:** a `Scenario` variant (`scenario.rs`: `ALL`, `name`, `key`, `start`), its
  constructor in `mod.rs`, a key in `app.rs`; the debug panel lists `Scenario::ALL` itself.

## Tests

Each module's tests live in its own `#[cfg(test)] mod tests` (`mod.rs`, `city/tests.rs` and
`ui/tests.rs` hold most of them). Build a `GameState` from a scenario constructor, drive it through the same
methods input uses, and assert on state. `cargo test` needs no GPU or window. A new rule that
constrains the board (occupancy, HP, population...) belongs in `simulation.rs`'s
`check_invariants` too, so every scenario exercises it.
