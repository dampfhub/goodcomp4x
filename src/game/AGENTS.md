# src/game/

Everything game-specific. The rules this code implements are written down in
`docs/game-rules.md` and the keys in `docs/controls.md`; change those in the same commit as the
behavior.

## Shape

`GameState` (in `mod.rs`) is the one struct holding the whole game. Each module below adds an
`impl GameState` block for its concern, so look for a method by concern, not by type.

| Module | Concern |
|---|---|
| `mod.rs` | `GameState` fields, scenario setup (`new`, `city_scenario`, `frontier_scenario`, `world_scenario`, `naval_scenario`), shared queries (`units_at`, `rival_of`, `swap_partner`, `reachable_hexes`), `CONTROLS_HELP` (a startup pointer to `docs/controls.md`), the main tests |
| `orders.rs` | player input and order planning: `ClickMode`, click (move), right-click (attack), swap, ability toggle, hold, guard |
| `order_queue.rs` | multi-turn order queues: Shift-click adds turns (`queue_move`, `queue_attack`, equal lengths for a group) or, on a planned stop, takes a move off (`unqueue_move`), `advance_queues` at each turn's end (real board only), and `replan_queues` as the player's planning begins (steered queues planned again toward their `waypoints` on what the player knows) |
| `group.rs` | multi-unit selection (drag a box, Shift-click adds, Ctrl-click removes) and group orders |
| `turn.rs` | `RESOLUTION_ORDER` and `Step` (unit steps, then the workers'), `update(dt)`, simultaneous step resolution (moves, attacks), coastal battery fire and ship boarding |
| `combat.rs` | damage formula, retaliation, combat log helpers |
| `ability.rs` | the abilities and their tuning constants |
| `unit.rs` | `Team`, `UnitType`, base stats, `Unit` and its state-aware `stats()` |
| `ai.rs` | the AI, playing every side but the player's (`ai_teams`) |
| `multiplayer.rs` | network play in lockstep (`docs/multiplayer.md`): the `Message`s, a side's `TeamPlan` (`team_plan`, `apply_plan`), hosting a world and seating players (`host_game`, `welcome`, `join_game`, `open_seats`, `seat_left`), `receive` and `check_plan` (every message checked before it touches the game), `checksum` |
| `city/mod.rs` | `City`, `Site`, `LaborFocus`, city tuning constants (barracks HP and defense, population cap), setup of the city scenarios (`setup_cities`, `setup_frontier`, `setup_world`) |
| `city/logistics.rs` | roads and Canoe House river corridors, logistics routes (`routes_from_by`), `delivered_share`, tile yields, Mill food share, Cannery/Smelter collection, city income (as food, wood and metal) and Barracks income |
| `city/rail.rs` | Railhead road connectivity and long-range transfer eligibility; city center is the origin terminal |
| `city/citizens.rs` | citizens: labor focus, the manager and its workers, auto-assignment and reconciling blocked tiles, `resolve_economy` (the turn's economy), Field Hospital healing |
| `city/economy.rs` | the stockpile experiment (`docs/rts-economy.md`): `Stock` (food, wood, metal), each side's stockpile, queue items (`Queued`: the build, whether it's paid, its work), queueing unpaid and refunding what was paid (`queue_build`, `take_queue_item`), paying for the item each queue starts and working it (`work_queues`) and what it will do as things stand (`forecast`, `waiting_items`, `city_waits_for`), growth prices, the wood/metal split of production (`metal_yield`), feeding citizens, a queue's work a turn (`work_rate`) and the production-speedup toggle; the icon forms of prices and turns (`stock_icons`, `turns_icon`) |
| `city/barracks.rs` | the Barracks as the military building: a city center's slower training (`CITY_TRAINING_SLOWDOWN`), the Horses and Iron deposits a Barracks draws on, the Cavalry and Armored cap (`UNITS_PER_DEPOSIT`, `special_cap`, `special_used`, alive or lifetime) and why a troop is locked (`barracks_lock`) |
| `city/builds.rs` | `Building`, `Build`, `BuildUnit`; city and Barracks queues (clearing one takes each item off through `take_queue_item` / `take_barracks_item`), Harbor naval spawning, the rules of a building's site (`ai_site_issue`), resource support from Forge/Stable, prices and turns of every build, the Grow build, `complete_builds` (buildings with a site are built by workers, `workers.rs`) |
| `city/founding.rs` | settlers founding cities |
| `workers.rs` | workers: each city's pool and job list (everything the city places on the map, buildings with a site included: `JobKind`, placed from the open city's production list with `arm_worker_job`, paid when placed with `try_queue_job` and refunded if taken off or dropped), the Workshop speedup (`job_turns`), Work Camp bases, the workers' last step of the turn (walking, working, going home), capture and death; structures (walls and gates on hex edges, outposts and forts on tiles) and the passability they add (`can_step`, `can_cross`) |
| `city/view.rs` | opening and leaving the city and Barracks views, map clicks while one is open (`city_click_at`: a click on one of the player's unit tokens, hit as `draw.rs` draws it (`unit_token_contains`), selects the unit and closes the view; anywhere else is `city_click`), stopping placing (`stop_placing`), the yields toggle, `end_planning` |
| `city/interior.rs` | city tactical grid, projecting adjacent troops, independent interior orders, command-post capture |
| `city/tests.rs` | the city tests |
| `hex.rs`, `terrain.rs` | axial hex math, `HexGrid` (shape, and tiles, rivers, resources and specials in flat arrays over the shape's bounding box); `Tile` = ground + hills + feature, with yields, route cost, defense |
| `fast_hash.rs` | the `HashMap` and `HashSet` the game uses: std's, with a fast fixed hasher (rustc's) for its small keys |
| `perf.rs` | tests only: `perf_report` (ignored; run with `--release -- --ignored --nocapture`) times a frame's and a turn's stages on a busy world |
| `mapgen.rs`, `mapgen/` | seeded world generation for the F4 scenario (own RNG: a seed always rebuilds the same map, on every machine), a function per stage (its module comment lists them): land and sea, mountain ranges, hills, lakes, passes, rivers, climate; then balanced starts for any number of sides, horses and iron by each start, and special tiles and ruins on contested ground. `mapgen/tests.rs` holds its tests (a golden hash pins two seeds' maps); `mapgen/preview.rs` (tests only) draws whole maps as PNGs and measures many (`map_previews`, `map_stats`, run by hand) |
| `ruins.rs` | ruins: holding them for `RUIN_HOLD_TURNS` claims a reward (`resolve_ruins`, at each turn's end before the economy) |
| `fog.rs` | fog of war: sight, line of sight, the player's memory of seen hexes |
| `scenario.rs` | scenarios (F1-F4, F12, Debug Naval), savestate (F6/F7), instant playback (F8) |
| `settings.rs` | the player's options (`Settings`, one field each, and `Setting`, how the menu lists and changes them: heading, control, range), Escape (`press_escape`) and the settings menu's open state; its module comment says how to add a setting |
| `simulation.rs` | tests only: seeded AI-vs-AI games (and games where the player's units follow order queues) in every scenario, board invariants checked each turn, same seed replays the same game |
| `camera.rs` | orthographic camera: pan, zoom, glide, screen/world conversion |
| `draw.rs` | world geometry (`build_vertices`): hexes, terrain, ghosts, attack arcs, units, badges |
| `unit_icons.rs` | unit pictograms (sword, bow, horse head, ...) built from rects, triangles, circles and lines, in the mockup coordinates they were designed in |
| `map_icons.rs` | resource and improvement icons (horse head, ingot, wheat, ore cart, fence, logs) in a hex's top corners, and the food, wood and metal icons in yield pips; the icon characters (`FOOD_ICON`, `WOOD_ICON`, `METAL_ICON`, `TIME_ICON`) that UI text draws as those icons (`push_inline_icon`); dark-edged shapes in their mockup coordinates |
| `effects.rs` | attack animations during playback |
| `transition.rs` | the turn transition, presentation only: what moved glides from where it was drawn, and the new turn's cue (the map dims a moment, the turn number flashes); the Turn Transition setting turns it off |
| `ui/mod.rs` | screen-space UI entry points (`build_ui`, `click_ui`, `update_hover`, `layout`), its shared constants and types (`Target`, `UnitAction`, `Button`, `Shape`, `Layout`) |
| `ui/builder.rs`, `ui/paint.rs`, `ui/dock.rs` | `PanelBuilder` (rows, measuring, placement); drawing shapes and buttons to vertices; `dock.rs` places panels by screen zone |
| `ui/trays.rs`, `ui/panels.rs`, `ui/queue.rs`, `ui/roster.rs`, `ui/settings_menu.rs`, `ui/network_menu.rs` | the command tray (unit, group, city, Barracks); top bar, debug panel, structure hover panel; queue panels with scrolling, drag to reorder and a Clear button, and each row's turns left or what it waits for (`queue_status`); the turn strip of everything needing orders (cities, unit groups); the settings menu (a heading per group, a control per `Setting`) and its Multiplayer page (host, join, leave; typed fields) |
| `ui/tooltips.rs`, `ui/text.rs` | button and tile tooltips (`tooltip_lines`, `unit_action_text`); number and text formatting (`quantity`, `ability_text`, `wrap`) |
| `ui/imgui.rs` | dockable ImGui presentation using the shared panel content |
| `ui/tests.rs` | the UI's layout, hit-test and tooltip tests |
| `mesh.rs`, `font.rs` | shape helpers (`polygon` ear-clips concave outlines); TrueType text and the glyph atlas |

## Invariants

- Hash maps and sets come from `fast_hash` (`HashMap::default()`, not `new()`), not
  `std::collections`: the default SipHash was a large share of frame and turn time. The
  exception is a set of keys straight off the network before they're checked (`check_plan`),
  which keeps std's randomly keyed hasher.
- "The player" is `self.local_team` (the side played at this machine), never a fixed team, and
  the AI plays the sides not in `self.humans` (`is_human`). A networked game seats each player
  on their own side (`multiplayer.rs`); anything a player's planning changes must be in their
  `TeamPlan` (`team_plan`, `apply_plan`, `check_plan`), or the other machine never learns of it.
  Anything else a player's machine does (opening a view, looking inside a city) must not change
  the game in a network game (`is_networked`): plans are checked and applied against the game
  as the turn began, the same on every machine.
  Once a network game's plan is sent it waits for the others' (`waiting_for_peers`): a method
  that changes the plan refuses while `is_resolving` (true then, and while a turn plays out),
  and one that only looks (selecting, opening a view) refuses only while `is_playing_out`.

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
- Builds are paid when work on them starts: add to a city or Barracks queue through
  `queue_build` / `queue_barracks` (an unpaid item with no work), take items out through
  `take_queue_item` / `take_barracks_item`, which refund only a paid item, and pay only in
  `work_queues`, as the turn's economy starts an item (`city/economy.rs`). Payment is in city
  order, then each city's queue before its Barracks', so it's the same on every machine;
  `forecast` plays the same choices out without changing anything, for the UI and the AI. A
  paid item (`Queued::prepaid`) pushed directly is for tests and scenario setup; it skips the
  price. `simulation.rs` checks that no stockpile goes negative, that an unpaid item has no
  work, and that no item has more work than it needs. In a network game planning can't pay:
  `check_plan` refuses a plan with a paid item or work the turn didn't start with.
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
  UI text may hold the icon characters of `map_icons.rs`: `font::Face` (classic) and ImGui's
  `rich_text` / `rich_button` (`ui/imgui.rs`) draw each as its icon, so put resources and
  turns in text with `stock_icons` and `turns_icon`, and draw any new game text in ImGui
  through those helpers, not `ui.text`.
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
