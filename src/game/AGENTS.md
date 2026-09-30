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
| `orders.rs` | player input and order planning: `ClickMode`, click (move), right-click (attack), swap, boarding and landing (`try_board`, `try_land`), ability toggle, hold, guard, alert (`toggle_alert`, `can_go_on_alert`), disband. Every new order calls `Unit::take_new_order` (it ends a hold, guard or alert and drops boarding and landing) |
| `order_queue.rs` | multi-turn order queues: Shift-click adds turns (`queue_move`, `queue_attack`, equal lengths for a group) or, on a planned stop, takes a move off (`unqueue_move`), `advance_queues` at each turn's end (real board only), and `replan_queues` as the player's planning begins (steered queues planned again toward their `waypoints` on what the player knows) |
| `group.rs` | multi-unit selection (drag a box, Shift-click adds, Ctrl-click removes) and group orders |
| `turn.rs` | `RESOLUTION_ORDER` and `Step` (unit steps, then the workers'), `update(dt)`, simultaneous step resolution (moves, attacks, and units on alert firing: `alert_target`), coastal battery fire and ship boarding |
| `combat.rs` | damage formula (no random spread), retaliation, combat log helpers; the attack preview (`attack_preview`: what the attacks on the hovered hex would do this turn, stepped in resolution order on what the player knows; the interior's is `interior_attack_preview`) |
| `ability.rs` | the abilities and their tuning constants |
| `unit.rs` | `Team` (the sides, and `Team::Wild`, the animals' owner, which is no side), `UnitType`, base stats, `Unit` and its state-aware `stats()` |
| `animals.rs` | animals (wolf packs, bears) and their dens: roaming and hunting within a territory and hunting range by kind (`territory`, `hunting_range`), decided as each animal step begins (`plan_animal_moves`, `plan_animal_attacks`) without the game's RNG (`roam_key`); a kill's bounty (`reward_hunts`), clearing dens and dens adding animals up to their cap and then strays up to the world's limit (`resolve_dens`, `den_cap`, `stray_limit`, at each turn's end; a stray is an animal with no `home`, roaming anywhere) |
| `ai.rs` | the AI, playing every side but the player's (`ai_teams`) |
| `multiplayer.rs` | network play in lockstep (`docs/multiplayer.md`): the `Message`s, a side's `TeamPlan` (`team_plan`, `apply_plan`), hosting a world and seating players (`host_game`, or `host_game_seeded` for a given world; `welcome`, `join_game`, `open_seats`, `seat_left`), `receive` and `check_plan` (every message checked before it touches the game), `checksum` |
| `city/mod.rs` | `City` (its citizens as `Cluster`s: a manager and its workers), `Site`, `Good` and `Priorities` (a city's priority order), city tuning constants (barracks HP and defense, the population cap as `MAX_MANAGERS` clusters of `CLUSTER_SIZE`), setup of the city scenarios (`setup_cities`, `setup_frontier`, `setup_world`) |
| `city/logistics.rs` | roads and Canoe House river corridors, logistics routes (`routes_from_by`), `delivered_share`, tile yields, Mill food share, Cannery/Smelter collection, city income (as food, wood and metal) and Barracks income |
| `city/rail.rs` | Railhead road connectivity and long-range transfer eligibility; city center is the origin terminal |
| `city/citizens.rs` | citizens: the priority order (scoring tiles, the food floor), managers and their workers in clusters, auto-assignment and reconciling blocked tiles, losing a citizen (`remove_citizen`, for starving and anything else that costs one), `resolve_economy` (the turn's economy), Field Hospital healing |
| `city/economy.rs` | the stockpile experiment (`docs/rts-economy.md`): `Stock` (food, wood, metal), each side's stockpile, queue items (`Queued`: the build, whether it's paid, its work), queueing unpaid and refunding what was paid (`queue_build`, `take_queue_item`), paying for the item each queue starts and working it (`work_queues`), a city whose queue works nothing gathering by itself (`auto_gather`; `gathers_this_turn` says so as things stand, for the panels and the map) and what it will do as things stand (`forecast`, `waiting_items`, `supply_waiting_items`, and why the first item of a queue waits: `head_wait`, `HeadWait`, the one account the panels, turn strip, End Turn and map show), growth prices, the wood/metal split of production (`metal_yield`), feeding citizens, a queue's work a turn (`work_rate`) and the production-speedup toggle; the icon forms of prices and turns (`stock_icons`, `turns_icon`) |
| `city/barracks.rs` | the Barracks as the military building: a city center's slower training (`CITY_TRAINING_SLOWDOWN`), the Horses and Iron deposits a Barracks draws on, the Cavalry and Armored cap (`UNITS_PER_DEPOSIT`, `special_cap`, `special_used`, alive or lifetime) and why a troop is locked (`barracks_lock`: a deposit, `deposit_lock`, or supply) |
| `city/supply.rs` | the military supply limit: what a side's cities give (`supply_from_cities`, with `SUPPLY_PER_CITY`, `SUPPLY_PER_CITIZEN` for the first `FULL_SUPPLY_CITIZENS` and 1 per `CITIZENS_PER_SUPPLY_PAST_CLUSTER` past them: the one formula), what each unit uses (`unit_type_supply`, the table; settlers and workers none), what counts against it (units alive and every queued item, `supply_used`) and what starting an item needs room in (units and paid items, `supply_started`, `supply_room`, which `work_queues` keeps new work within), and why a card is locked (`supply_lock`) |
| `city/builds.rs` | `Building`, `Build`, `BuildUnit`; city and Barracks queues (clearing one takes each item off through `take_queue_item` / `take_barracks_item`), Harbor naval spawning, the rules of a building's site (`ai_site_issue`), resource support from Forge/Stable, prices and turns of every build, the Grow build, the city-queue-only Settler and Scout (`city_build_issue`: population 3, one Scout at a time, and supply for any troop, ship or Scout; `waits_for_citizens`), `complete_builds` (buildings with a site are built by workers, `workers.rs`) |
| `city/founding.rs` | settlers founding cities: the founding rules (`founding_issue`, `MIN_CITY_DISTANCE`) and a new city (`found_city`, a worker only for a side's first) |
| `workers.rs` | workers: each city's pool and job list (everything the city places on the map, buildings with a site included: `JobKind`, placed from the open city's production list with `arm_worker_job`, paid when placed with `try_queue_job` and refunded if taken off or dropped), the Workshop speedup (`job_turns`), Work Camp bases, the workers' last step of the turn (walking, working, going home), capture and death; structures (walls and gates on hex edges, outposts and forts on tiles) and the passability they add (`can_step`, `can_cross`) |
| `city/view.rs` | opening and leaving the city and Barracks views, map clicks while one is open (`city_click_at`: a click on one of the player's unit tokens, hit as `draw.rs` draws it (`unit_token_contains`), selects the unit and closes the view; anywhere else is `city_click`, where one of the player's cities or Barracks opens its view, from any view), stopping placing (`stop_placing`), the yields toggle, `end_planning` |
| `city/interior.rs` | city tactical grid, projecting adjacent troops, independent interior orders, command-post capture (pushing units of other sides off the center: `push_off_city_center`) |
| `city/tests.rs` | the city tests |
| `hex.rs`, `terrain.rs` | axial hex math, `HexGrid` (shape, and tiles, rivers, resources and specials in flat arrays over the shape's bounding box); `Tile` = ground + hills + feature, with yields, route cost, defense |
| `fast_hash.rs` | the `HashMap` and `HashSet` the game uses: std's, with a fast fixed hasher (rustc's) for its small keys |
| `perf.rs` | tests only: `a_frame_stays_within_its_vertex_budget` caps one frame's vertices on the biggest world with everything shown at every zoom (fog off, Alt, a selection; #332), counting each layer with `draw.rs`'s `scene_stages`; `perf_report` and `world_overlay_report` (ignored; run with `--release -- --ignored --nocapture`) times a frame's and a turn's stages on a busy world |
| `mapgen.rs`, `mapgen/` | seeded world generation for the F4 scenario (own RNG: a seed always rebuilds the same map, on every machine), a function per stage (its module comment lists them): land and sea, mountain ranges, hills, lakes, passes, rivers, climate; then balanced starts for any number of sides, horses and iron by each start, special tiles and ruins on contested ground, and animal dens away from every start. `mapgen/tests.rs` holds its tests (a golden hash pins two seeds' maps); `mapgen/preview.rs` (tests only) draws whole maps as PNGs and measures many (`map_previews`, `map_stats`, run by hand) |
| `ruins.rs` | ruins: holding them for `RUIN_HOLD_TURNS` claims a reward (`resolve_ruins`, at each turn's end before the economy) |
| `fog.rs` | fog of war: sight, line of sight, the player's memory of seen hexes (this machine's view), each side's own memory (`side_fog`, game state, which the AI plans on), and the `known_*` queries that answer for either (`Fog` says which) |
| `scenario.rs` | scenarios (F1-F4, F12, Debug Naval), savestate (F6/F7), instant playback (F8) |
| `strings.rs` | the game's text from `text/*.ini` (`docs/text.md`): the parser, the `text!`, `tooltip!` and `hover_text!` macros that look an entry up by tag, and the tests that check the files against every call in the source |
| `keys.rs` | the key map: the key each `Command` is on (`PLAYING`, and `TYPING` for a text field), which `App` looks a key press up in (`command_for`) and the UI names a command's key from (`Command::key`), filling the text files' key placeholders |
| `settings.rs` | the player's options (`Settings`, one field each, and `Setting`, how the menu lists and changes them: heading, control, range), Escape (`press_escape`) and the settings menu's open state; its module comment says how to add a setting |
| `simulation.rs` | tests only: seeded AI-vs-AI games (and games where the player's units follow order queues and its cities queue ahead) in every scenario, board invariants checked each turn (and that every city spent the turn: worked its queue or gathered by itself, `play_out`), same seed replays the same game |
| `simulation/economy.rs` | tests only: `economy_report` (ignored; run with `--release -- --ignored --nocapture`) measures the economy's tempo over many seeds (units by type and turn, growth, stockpiles, fights, build times, spread; its `REPORT_*` knobs are in its module comment, its numbers in `docs/rts-economy.md`) |
| `simulation/wildlife.rs` | tests only: `animal_report` (ignored; run with `--release -- --ignored --nocapture`) measures the animals over many worlds: how many are alive turn by turn, what the sides lose to them early and late, and how many are killed and dens cleared (`REPORT_ANIMALS`, `REPORT_TURNS`; its module comment) |
| `camera.rs` | orthographic camera: pan, zoom, glide, screen/world conversion |
| `draw.rs` | world geometry (`build_vertices`): hexes, terrain, ghosts, attack arcs, units, badges, health bars and the attack preview's marks on them (`push_health_loss`), and the production tags over the player's cities and Barracks (`push_production_tags`: what waits, from `head_wait`; with Alt, what they work) |
| `unit_icons.rs` | unit pictograms (sword, bow, horse head, ...) built from rects, triangles, circles and lines, in the mockup coordinates they were designed in |
| `map_icons.rs` | resource and improvement icons (horse head, ingot, wheat, ore cart, fence, logs) in a hex's top corners, and the food, wood and metal icons in yield pips; the icon characters (`FOOD_ICON`, `WOOD_ICON`, `METAL_ICON`, `TIME_ICON`) that UI text draws as those icons (`push_inline_icon`); dark-edged shapes in their mockup coordinates |
| `effects.rs` | attack animations during playback |
| `transition.rs` | the turn transition, presentation only: what moved glides from where it was drawn, and the new turn's cue (the map dims a moment, the turn number flashes); the Turn Transition setting turns it off |
| `ui/mod.rs` | screen-space UI entry points (`build_ui`, `click_ui`, `update_hover`, `layout`), its shared constants and types (`Target`, `UnitAction`, `Button`, `Shape`, `Layout`) |
| `ui/builder.rs`, `ui/paint.rs`, `ui/dock.rs` | `PanelBuilder` (rows, measuring, placement); drawing shapes and buttons to vertices; `dock.rs` places panels by screen zone |
| `ui/trays.rs`, `ui/panels.rs`, `ui/queue.rs`, `ui/roster.rs`, `ui/settings_menu.rs`, `ui/network_menu.rs` | the command tray (unit, group, city, Barracks); top bar, debug panel, structure hover panel; queue panels with scrolling, drag to reorder and a Clear button, and each row's turns left or what it waits for (`queue_status`); the turn strip of everything needing orders (cities, idle queues, unit groups); the settings menu (a heading per group, a control per `Setting`) and its Multiplayer page (host, join, leave; typed fields) |
| `ui/tooltips.rs`, `ui/text.rs` | button and tile tooltips (`tooltip_lines`, and `subject_tooltip_lines` for a given unit or city, `unit_action_text`; `classic_tooltip_at` picks classic's); number and text formatting (`quantity`, `ability_text`, `wrap`) |
| `ui/imgui.rs` | dockable ImGui presentation using the shared panel content |
| `ui/tests.rs` | the UI's layout, hit-test and tooltip tests |
| `sprites.rs` | world icon and terrain sprite atlas, baked once from vector artwork; padded premultiplied RGBA mipmaps and one quad per placement |
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
  Once a network game's plan is sent it waits for the others' (`waiting_for_peers`), and
  `is_resolving` holds. Methods that change the plan and ones that only look (selecting,
  opening a view) alike refuse only while `is_playing_out`: a change to the plan while waiting
  takes the turn back (`take_back_on_new_orders`, which compares `team_plan` with the plan
  sent), and looking must leave `team_plan` as it was.

- `Team::Wild` owns the animals (`animals.rs`) and is no side: it isn't in `Team::ALL`, has no
  seat, stockpile, fog or plan, and `Team::index` must never be asked of it (arrays by side are
  `Team::ALL`'s length). Code that gives something to "the unit's side" (a reward, a capture,
  holding ruins, a city interior) must skip animals (`Unit::is_animal`), and anything a network
  message names as a side must be one (`Team::is_side`).
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
  (units, workers) or routes goods checks `can_step` / `can_cross`, and planning, the
  player's and the AI's, checks what the side knows (`known_step`, `known_can_cross`, `fog.rs`).
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
- **Supply** (`city/supply.rs`): anything that queues a troop, ship or Scout checks
  `supply_lock` (through `city_build_issue` or `barracks_lock`), and `work_queues` starts one
  only with room (`supply_room`). Tests' `fund` also lifts a side's supply (`extra_supply`);
  a test of the limit sets it back to 0. `simulation.rs` checks no item ever starts past its
  side's supply (`supply_overruns`, recorded in `work_item`), and `check_plan` refuses a plan
  that queues more than the side's supply allows.
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
- The AI plans only on what its side knows (`side_fog`, `fog.rs`), never the real board,
  and never on anything of this machine's (`local_team`, `memory`, `fog_of_war`): a side's
  memory is game state, the same on every machine and counted in `checksum`.
- The AI must stay deterministic: ties break by hex coordinates. All randomness goes through
  `GameState.rng` (never `rand::random` or the thread RNG) and never depends on hash-map
  iteration order, so a seed replays the same game (`simulation.rs` checks this).

## Recipes

- **Text:** an area whose text has moved to `text/` (so far the menus, the status and top
  bars, the Debug panel and the window titles) takes new words as entries there, asked for with `text!("tag")` or
  `tooltip!("tag", name = value)` (`strings.rs`, `docs/text.md`), the tag always a literal. The
  tests check each tag used exists with its placeholders and each entry is used. A key is never
  spelled out in a file: the entry has a placeholder the code fills with `Command::key` (`keys.rs`).

- **New key:** a `Command` bound to a key in the key map (`keys.rs`: `PLAYING`, or `TYPING` for a
  text field), its arm in `App::carry_out` (`src/app.rs`) calling a `GameState` method (a key
  that acts on release or while held needs an arm of its own in `App::window_event`), and a row in
  `docs/controls.md`, the one description of the controls. `CONTROLS_HELP` (printed at startup)
  only points to that file; don't list keys in it.
- **New unit button or panel:** see the recipes in `ui/AGENTS.md`.
- **Stat or tuning change:** `unit.rs` or `ability.rs`, then every place that states the number
  to players: `ability_text` in `ui/text.rs` (tooltips) and the tables in
  `docs/game-rules.md`. Grep for the old value.
- **New player setting:** `settings.rs`: a field in `Settings` (and its default), a
  `Setting` variant in `Setting::ALL`, and its arms in the `Setting` and `Settings` matches (the
  module comment lists them); its name, tooltip and values are entries in `text/menus.ini`. The settings menu shows it in both presentations with no UI
  change; game code reads the field (`self.settings.<field>`), and a row in the settings table
  of `docs/controls.md` describes it.
- **New scenario:** a `Scenario` variant (`scenario.rs`: `ALL`, `name`, `title`, `description`,
  `title_on_hover`, `start`; its title and description are an entry in `text/ui.ini`), its
  constructor in `mod.rs`, a key in the key map (`keys.rs`); the debug panel lists `Scenario::ALL` itself.

## Tests

Each module's tests live in its own `#[cfg(test)] mod tests` (`mod.rs`, `city/tests.rs` and
`ui/tests.rs` hold most of them). Build a `GameState` from a scenario constructor, drive it through the same
methods input uses, and assert on state. `cargo test` needs no GPU or window. A new rule that
constrains the board (occupancy, HP, population...) belongs in `simulation.rs`'s
`check_invariants` too, so every scenario exercises it.

Network-game tests host a fixed world, never a random one (`host_test_game`, and the tests'
own helpers, on `test_seed()`), so a test plays the same game every run. `MP_SEED=<n> cargo
test multiplayer` (or `net::`, or any test that hosts) runs them on another world instead, to
catch a test that leans on its map: a fixture should find what it needs (a unit with room to
move, a free hex) on any world.
