# Hex Combat Sandbox

A prototype for experimenting with combat rules for a Civ-like game, written in
Rust on raw Vulkan (`ash`) with `winit` for windowing. You play Blue against an
AI-controlled Red on a small hex map. Turns are simultaneous ("we-go"): both
sides queue orders, then the turn resolves step by step in a fixed order set by
unit type.

This file is the running record of the project: how it's built, how the rules
work, what's been done, and what's still open. Keep it current when behavior
changes.

## Build, run, test

- `cargo run --release`: normal play. Release builds skip Vulkan validation.
- `cargo run`: debug build with the Khronos validation layer on (needs the
  Vulkan SDK installed). Validation messages go through `log`; set `RUST_LOG`
  to change verbosity. The combat log prints at `info`.
- `cargo test`, `cargo clippy --all-targets`, `cargo fmt`. Tests live in the
  `tests` module at the bottom of `src/game/mod.rs`, plus UI tests in
  `src/game/ui.rs`, font tests in `src/game/font.rs` and city tests in
  `src/game/city.rs`.
- Shaders are GLSL in `shaders/`. `build.rs` compiles each one with `glslc` from
  `$VULKAN_SDK` (falling back to `PATH`) into `OUT_DIR`, and the renderer embeds
  the SPIR-V with `include_bytes!`.
- On Windows, close a running instance before rebuilding; the exe is locked
  while it runs.

## Layout

```
src/
  main.rs          entry point: logger + winit event loop
  app.rs           window, input events, frame pacing (165 FPS cap), builds each frame,
                   F5 borderless fullscreen toggle
  icon.rs          window/taskbar icon drawn in code (blue hex, white triangle)
  renderer/        general-purpose 2D Vulkan renderer, knows nothing about the game
    mod.rs         Renderer: setup, draw_frame(&[DrawBatch]), swapchain recreation, teardown
    instance.rs    instance + validation layer + debug messenger
    device.rs      GPU selection, logical device, queues
    swapchain.rs   swapchain + image views
    pipeline.rs    render pass (MSAA target resolved into the swapchain image)
                   + the single alpha-blended vertex-color pipeline
    msaa.rs        the multisampled color target, rebuilt with the swapchain;
                   pick_samples chooses the most the GPU supports (16/8, min 4)
    texture.rs     the coverage atlas (R8 + mips), uploaded once, bound as set 0
    buffer.rs      buffer + memory allocation
    sync.rs        per-frame semaphores and fences (2 frames in flight)
    vertex.rs      Vertex { pos, color, uv }; SOLID_UV marks untextured geometry
  game/            everything game-specific
    mod.rs         GameState, map/unit setup, shared queries (units_at, rival_of,
                   swap_partner, reachable_hexes), controls help text, tests
    orders.rs      player input and order planning (click, right-click, swap, ability toggle)
    group.rs       selecting several units (Alt-drag box, Alt-click) and group orders
    scenario.rs    testing aids: scenario pages (F1-F3) and the savestate (F6/F7)
    turn.rs        RESOLUTION_ORDER and simultaneous step resolution (moves, attacks)
    combat.rs      damage formula, retaliation rule, combat log helpers
    ability.rs     the four abilities and their tuning constants
    unit.rs        Team, UnitType, base stats, Unit (with state-aware stats())
    ai.rs          the Red AI
    hex.rs         axial hex math and HexGrid (terrain, plus rivers along hex edges)
    terrain.rs     tiles: base Terrain + hills + Feature (forest/jungle); yields,
                   passability, route cost, defense
    mapgen.rs      seeded random maps for the F4 world scenario
    fog.rs         fog of war: what the player sees and has explored
    camera.rs      top-down orthographic camera: pan, zoom, screen<->world
    draw.rs        world geometry: hexes, terrain symbols, move ghosts, attack arcs,
                   units, badges
    effects.rs     attack animations during playback: shots, hits, misses, damage
    city.rs        cities, logistics routes, citizens, growth, builds, settlers/workers
    ui.rs          screen-space UI: top bar, command tray, tooltips, hover info box
    mesh.rs        shape helpers: regular_polygon, quad, segment
    font.rs        TrueType text (Hack via fontdue): glyph atlas, UI and world text
shaders/
  mesh.vert        applies the batch's view-projection push constant
  mesh.frag        vertex color, times atlas coverage unless the UV is SOLID_UV
```

Text: `font.rs` rasterizes printable ASCII from the Hack font (bundled by the
`epaint_default_fonts` crate) once, into one R8 atlas with 4 mip levels, which
the renderer uploads at startup (`Renderer::new(window, font_atlas())`). UI text
uses glyphs rasterized at its exact pixel size (`font::ui(px)`, one of
`UI_SIZES`) and snaps to whole pixels; world text (`push_glyph`, `push_text`,
sized by capital-letter height) scales a 64px set and relies on the mips.

The frame loop: `App` calls `GameState::update(dt)`, then builds two batches:
world vertices (`build_vertices`) through `camera.view_proj`, and UI vertices
(`build_ui`) through `ui_projection`. It hands both to `Renderer::draw_frame`.
All geometry is rebuilt from game state every frame.

## Game rules as implemented

### City experiment (current default)
- The default scenario has a radius-six map, two cities, owned farms/mines/pastures,
  and preplaced dirt roads. F1/F2/F3/F4 start the combat, city, frontier or
  generated world scenario (`scenario.rs`); pressing the current one's key restarts it.
- Testing savestate (`scenario.rs`): F6 clones the whole `GameState` into
  `savestate`, F7 restores a copy (keeping the snapshot, and the camera if
  it's the same scenario). It survives scenario switches and lives only in
  memory. The faded DEBUG panel (`ui::debug_panel`, top-left) has buttons for
  all of these; the hovered-unit info box sits top-right to stay clear of it.
- Debug setting `instant_playback` (F8 or the panel), on by default: `update` resolves every
  pending step in one call instead of one per `STEP_INTERVAL`. Steps still run
  in `RESOLUTION_ORDER`, so outcomes don't change; all animations fire
  together. Kept across scenario switches and savestate loads.
- `city.rs` contains city state, weighted logistics routes, citizen assignments,
  food/growth/starvation, and stored production. Yields use quarter units and
  route costs use half-hex units. Enemy occupation blocks routes; alternatives
  are recalculated. Income is applied once after all eight combat steps.
- C opens the player's city; click tiles to assign/release citizens, A auto-assigns,
  Tab returns to units. Clicking one of your units in the city view
  selects it and leaves the view; so does clicking the city again or off the map.
  The panel shows income and the clicked tile's yield and delivery share.
- City hover or selection outlines worked tiles green (red if disrupted), with
  `mesh::polygon_outline` rings so the corners join cleanly.
  Tile badges show raw food as green grain and production as amber hammers,
  with numeric counts. Badges and delivery percentages appear only for the open
  city, while `show_yields` is on (Y or the tray's Yields button; on by
  default), covering its reachable/worked tiles. Hovering only outlines.
- `update_hover` (called each frame by `App`) tracks the map hex under the
  cursor, ignoring the UI, and how long it's rested there. After 0.75s a tile
  tooltip shows terrain or city, yields, defense, site, road, which city works
  it, its delivery share to the open city, and units on it.
- The turn waits on `GameState::pending()`: player units that still need
  orders, and player cities with nothing queued (`city_needs_build`). Citizen
  assignments never count. Space (or the End Turn button) ends the turn once
  both are zero; otherwise it selects what's still waiting. When the last unit
  is done, selection moves on to a city needing a build (`open_city`).
  Cities without units can still advance. Input is ignored during playback.
- Production is stored only. Construction, strategic materials, technology, site
  capture, and city combat are future slices. See `docs/controls.md` and the
  proposal in `docs/city-system.md`; proposal rules are not all implemented.
- City unit queues are now a first construction slice: with a city panel open,
  keys 1–4 queue melee/ranged/cavalry/siege. A unit completes once stored
  production reaches its listed cost and deploys to an open neighboring hex.
  F3 starts a frontier map with one T-marked settler per team; F founds the
  selected player's city, while the AI settles at its first resolution.
- Logistics uses weighted shortest paths, not radius or line distance. Enemy and
  contested hexes block every route; a longer off-road detour delivers less.
  The city tray shows growth percent
  and turns remaining. On growth or route disruption, citizen reconciliation
  keeps valid manual assignments and fills/replaces the affected slot.
- UI (`ui.rs`): each frame is laid out once into a `Layout` (panels, text,
  buttons) that's then drawn or hit-tested, so clicks match what's shown.
  Panels size themselves to their text (`PanelBuilder`), so nothing overlaps.
  - Top bar: turn, the latest `notice`, and the End Turn button, whose label
    (`end_turn_label`) names what's still waiting ("3 UNITS NEED ORDERS",
    "CHOOSE PRODUCTION") until it turns gold and reads END TURN.
  - Bottom-left command tray: the open city (stores, income, what it's
    building, growth meter, 1–4 build cards), or else the selected unit (stats
    with boosted values green and reduced red, notes, and buttons: Move, Attack,
    Swap, then its ability / Found City / Build Road + Improve, then Hold).
  - Move/Attack/Swap arm `ui_click_mode` for the next map click only (a held
    modifier overrides it). Pressing the button again or right-click
    disarms. The armed button has a bright border; queued orders turn their
    button gold; unusable ones are dimmed. Hex highlights follow the armed
    mode: no green move hexes while attacking, adjacent allies while swapping.
  - Every button has a hover tooltip, drawn clear of its panel. Hovering any
    unit on the map shows its stats in a box at the top-left.
  - Colors are linear but the swapchain is sRGB, so they display much lighter
    than their values suggest; dark panels need values around 0.01-0.05.

### Map
- Fog of war (`fog.rs`, debug toggle F9, on by default and kept across
  scenario switches and loads): the player's units (by `GameState::sight`),
  cities (3) and barracks (1) see hexes, unless a mountain stands on the
  line between (`in_line_of_sight` via `Hex::line_between`, tried nudged to
  either side of an edge; the mountain itself is visible). Every frame (`explore`, from
  `update`) each hex in sight is recorded in `memory` as a `Sighting`: other
  sides' units (clones plus letters), city and barracks (team, number,
  health), improvement and road. Three layers: in sight shows the live
  state; remembered hexes out of sight draw their `Sighting` (`map_view`,
  `push_remembered_units`) under a translucent grey veil with faint cloud puffs (`push_cloud_puffs`),
  outlined in a darker grey where they meet hexes in
  sight (`push_fog`, after the city map and remembered units, before orders
  and live units); nothing at all is drawn on never-seen hexes, so the
  background shows. Live units `Fog::shows` rejects are hidden,
  with their ghosts and attack-range highlights; hover info ignores them and
  tile tooltips describe remembered hexes from memory. The AI ignores it.
- Tiles (`terrain.rs`): a base `Terrain` (grassland, plains, desert, tundra,
  snow, marsh, mountains, coast, ocean, lake), a `hills` flag and an optional
  `Feature` (forest, jungle). `Tile::yields` starts from the ground's and
  adds hills (+1 production), forest (-1 food, +1 production) or jungle (+1
  each). `route_cost` is 2 (3 on snow or marsh) plus 1 each for hills and a
  feature; roads are 1. Defense adds up: hills +25%, forest or jungle +15%.
  `HexGrid::tile` gives the whole tile, `terrain` just the ground. Water and
  mountains are impassable to units; cities can work water (routes end on a
  water tile but never continue across it), never mountains. A city's
  manager (first worked tile) must be land. Unlisted hexes are plain plains;
  `Tile::HILLS` (plains hills) and `Tile::MOUNTAINS` build the fixed maps.
  Worker improvements add to the tile: mine +2 production on hills, lumber
  mill +1 production under a feature, farm +2 food elsewhere (not snow).
- Rivers are hex edges (`HexGrid::rivers`, pairs from `hex::edge`). Land
  beside a river or lake has fresh water: +1 food in `tile_yield`, on top of
  any site. They're drawn along the shared edge (`edge_corners`). They don't
  affect movement or combat yet.
- The F4 world (`mapgen.rs`, `WORLD_SHAPE`: a `Shape::Rectangle` of 61
  columns by about 36 rows, wider than tall, sized for four players) is
  generated from a `u32` seed with its own SplitMix64 RNG and value noise, so
  a seed always rebuilds the same map; the seed shows in the debug panel. A
  Pangea: an oval dome plus two noise layers sets the sea (42-52% water),
  land apart from the biggest mass sinks unless it's an islet of at most
  `MAX_ISLAND` (12) hexes, ridged noise picks mountain ranges and
  hills (the hills flag stays whatever ground the climate picks), small
  enclosed seas become lakes, rivers walk hex corners downhill
  from high ground to water (or pool into a lake), and latitude plus noise
  (temperature) and noise plus water (moisture) pick the ground: snow,
  tundra, desert, marsh (warm, very wet, never hills), grassland or plains.
  Forest grows on wetter grassland, plains and tundra (hills too), patchy
  from its own noise; jungle covers about three quarters of marsh. Thresholds
  use rank order, so each map has similar proportions. Starts are the pair
  on the continent's largest passable stretch, ideally a third of the map's
  width apart, with the best and most even yields within two hexes. Each
  side gets a settler, worker and scout; `fair_start` / `start_units` put
  the settler and worker on flat ground and the scout on hills, so starting
  vision is equal. The camera starts on Blue's settler. `HexGrid` shapes: `Hexagon { radius }` (the fixed scenarios) or
  `Rectangle { cols, rows }`; `edge_distance` is hexes in from the edge.
- The combat scenario (F1) is hand-built: a hex grid of radius 3 (flat-top, axial coordinates).
- Mountain ridges at (0,-3), (0,-2), (0,2), (0,3) leave a three-hex pass down
  the middle. Hills at (0,0) (center of the pass), (-2,2) and (2,-2), next to
  each side's ranged unit.
- Hills: the unit standing there gets +25% defense.
- Mountains: impassable. You can't enter them, path through them, or target them.

### Units
| Type | HP | Attack | Defense | Move | Range | Icon |
|---|---|---|---|---|---|---|
| Melee | 100 | 22 | 20 | 1 | 1 | triangle, M |
| Ranged | 75 | 24 | 10 | 1 | 2 | diamond, R |
| Cavalry | 100 | 24 | 14 | 2 | 1 | pentagon, C |
| Siege | 65 | 32 | 6 | 1 | 2 | upright square, S |
| Scout | 60 | 8 | 10 | 3 | 1 | small circle, X |

Every unit icon is its team color with a dark outline. Settlers (T) and
workers (W) are civilians: a hollow hexagon (pale center, team-colored rim),
with no attack-order badge. Map markers are primitives too: a city is a
crenellated tower showing its population, plus a gold G disc once it has a
granary; a barracks is a small house marked B, in its team color; an
improvement is a dark badge edged in its owner's color in the hex's top-left
corner, with rows (farm), a heap (mine), a fence (pasture) or logs (lumber
mill).

Sight (fog of war): 2 hexes, scouts and cavalry 3, +1 on hills.
`Unit::stats()` applies ability effects and siege deployment on top of these.
Everything that asks what a unit can do goes through it.

### Orders (planning)
- Left-click actions occur on release. Dragging at least 6 pixels pans the map
  and suppresses the click; middle-drag also pans. Losing focus or leaving the
  window cancels the gesture.
- Click one of your units to select it. Click a green hex to queue a move; click
  it again to cancel.
- Click an enemy in range to queue an attack on its hex. Shift-click attacks any
  hex in range, occupied or not. Attacks target *hexes*: whoever stands there
  when the attack resolves gets hit.
- A unit can queue a move and an attack; the attack range is measured from the
  planned destination. Changing or cancelling the move drops an attack that's
  no longer in range.
- Ctrl-click an adjacent ally to swap places (see below).
- Right-click queues a move to an open hex or an attack on an enemy (or
  disarms an armed action); Ctrl-right-click clears the selected unit's
  orders, including a hold.
- Q or the ability button toggles the selected unit's ability.
- Space (`hold_or_end_turn`) holds the selected unit (`Unit::holding`) if it
  still needs orders: it keeps whatever it has queued and gives up the rest
  of its turn. With nothing left waiting, Space ends the turn instead.
- G or the Guard button toggles `Unit::guarding`: like holding, but it lasts
  across turns (`end_turn` doesn't clear it), so the unit never comes back up
  in the turn order. Queuing any move, attack or swap wakes it, as do G and
  Ctrl-right-click. Guarding units get a white hex outline on the map.
- Selection flows through your units: the first unit needing orders is selected
  at the start of each turn, and once the selected unit is done, the next one
  is selected automatically. "Done" (`needs_orders` in `orders.rs`) means it
  is holding or guarding, or has a move queued (or can't move) and an attack queued (or
  can't attack). Enemies in range don't matter, since square attacks are
  always possible. A unit in a contested hex is always done. Selecting a unit
  by clicking never auto-advances, so a finished unit can be reselected to
  edit. Tab looks at the next unit without holding the current one.
- Whenever the game picks the unit (auto-advance, hold, Tab, turn start), the
  camera glides to it (`Camera::focus_on`); middle-drag panning cancels the
  glide.
- Movement is BFS through passable, unoccupied hexes, so units can't pass
  through each other or through mountains. Two allies can't head for the same hex.
- Resolution starts after a 0.6s pause so the last order is visible. Input is
  ignored while a turn plays out.
- F5 toggles borderless fullscreen on the window's current monitor.
- Holding Escape for a second quits (`App::quit_held_since`), with a
  "HOLD ESC TO QUIT" bar (`ui::quit_prompt`) while it's held. Escape does
  nothing else.

### Turn resolution (`turn.rs`)
The turn plays out in 10 steps, one every 0.6s, with the acting units flashing.
Steps where nobody acts are skipped.

1. Scout move  2. Cavalry move  3. Melee move  4. Ranged attack  5. Scout attack
6. Cavalry attack  7. Melee attack  8. Ranged move  9. Siege move  10. Siege attack

The badges on each unit show this: blue number = move order, red = attack order.

Everyone in a step acts simultaneously:
- **Moves:** all at once. A unit can enter a hex whose occupants are all
  leaving this step, so chains and rotations work. A move into a hex where
  someone stays is blocked. Two enemies can't swap by moving through each other.
  Allies can.
- **Collisions:** exactly two enemies moving onto the same hex both take it,
  and it becomes contested. Any other pile-up bounces everyone involved.
- **Attacks:** all use the board as it stood at the start of the step. Damage
  is summed and applied at the end, so a unit killed this step still gets its
  attack off. Two units attacking each other in the same step make one exchange
  of blows, not two attacks that each draw retaliation.
- Each mover's order is spent when its step runs, whether it got through or not.

### Contested hexes
- Both enemies occupy the hex. It turns orange, and the two units are drawn
  half-size, stacked with Blue on top.
- In their attack step every turn, the two automatically trade blows. They
  can't be ordered to attack anything else.
- Either can move out to give up the hex. Other units can still attack into it
  and hit their enemy there.

### Swaps
- Ctrl-click an adjacent ally. Both units get a move into the other's hex, and
  the pair is drawn linked by a line.
- The swap happens at whichever of the two moves first; the other is pulled
  along and skips its own move step.
- Clicking again, re-ordering either unit, or right-clicking cancels both
  halves. Not allowed for units in a contested hex or units that can't move.

### Groups (`group.rs`)
- Alt-drag a box to select the player's units drawn inside it (via
  `Camera::world_to_screen`); Alt-click adds or removes one unit. Two or more
  become `GameState::group` (with `selected` cleared); one is an ordinary
  selection. The group's hexes are highlighted and the tray summarizes it.
- Clicking a hex (or Move) converges: members' old moves are dropped, then,
  nearest to the target first, each takes the reachable hex closest to the
  target that no ally is heading for, staying put if it can't get closer.
  Members keep their own speeds, so the group doesn't hold formation. Normal
  pathing applies, so members can't step into each other's current hexes.
- Clicking an enemy (or Attack/Shift) has every member that can reach the hex
  attack it; clicking a target they all already attack calls it off.
- Space/Hold holds every member, G guards them all (or unguards if all are),
  Ctrl-right-click clears their orders, clicking one member selects just it.
- The group is cleared when a turn resolves, since indices shift as units die.
- Queued attacks are drawn as curved arrows (`push_attack_arc`) from the
  attacker, or its ghost if it moves first, to just short of the target. Each
  arrow is one ribbon (`mesh::polyline`) so pieces never overlap. Only the
  player's own attacks get arrows; the AI's plans stay hidden.
- When an attack resolves, `resolve_attacks` clears its `planned_attack` and
  plays effects (`effects.rs`, aged in `update`): the arrow shoots from
  attacker to target, then a burst on a hit, "MISS" on an empty hex, or "OUT
  OF RANGE" if the target moved away; every unit hurt (retaliation included)
  shows a rising damage number, "KILLED" if it died. Enemy attacks animate too.

### Abilities (`ability.rs`)
| Unit | Ability | Effect | Cooldown |
|---|---|---|---|
| Melee | Shield Wall | +50% defense this turn, can't move | 1 turn |
| Ranged | Volley | attack also hits enemies adjacent to the target, all hits 60% | 2 turns |
| Cavalry | Charge | +1 move, +50% attack this turn | 2 turns |
| Siege | Deploy / Pack Up | spend a turn setting up (no move or attack); deployed: +1 range, can't move; packing up takes a turn too | none |
| Scout | Lookout | stay put this turn; through the next turn, +2 sight | none |

A queued ability shows as a gold ring and deployed siege as a steel ring.
Cooldowns tick down at every turn end.

### Combat (`combat.rs`)
- Damage = `30 * e^((attack - defense) * 0.04) * random(0.8..1.2)`, clamped to
  1..100. Defense includes terrain and Shield Wall; attack includes Charge.
- Melee attacks (base range 1) draw retaliation from a defender that survives the hit.

### AI (`ai.rs`)
Each Red unit picks the enemy nearest on foot (walking distance around terrain),
attacks it if already in range, otherwise moves to the reachable hex with the
shortest remaining walk and attacks if that brings it into range. It skips hexes
a teammate already claimed, and units in a contested hex stay and fight. It
never uses abilities. Ties break by hex coordinates, so it's deterministic.

## Gotchas and decisions

- **glam 0.33 bug:** `camera::rh::proj::vulkan::orthographic` flips the Y scale
  but not the Y translation. `Camera::view_proj` builds the OpenGL projection and
  flips both terms itself.
- **SPIR-V alignment:** `include_bytes!` data has no alignment guarantee.
  `create_shader_module` copies it into a `Vec<u32>` instead of reinterpreting
  in place. The in-place version once broke release builds only.
- **Drop order:** `App` declares `renderer` before `window` so the Vulkan
  surface is destroyed before its window.
- **No depth buffer:** the scene is flat, so layering is draw order (painter's
  algorithm). `build_vertices` draws hexes, then markers, then units.
- **Coordinates:** world space has +Y up. UI space is pixels with the origin at
  the window's bottom-left and +Y up too, so the same shape and text helpers
  work in both. The cursor arrives top-left-origin and gets flipped in
  `click_ui`.
- **Units vector:** `units` only holds living units. Dead ones are removed with
  `retain` at the end of an attack step, which shifts indices, so code that
  spans a removal uses unit `id`s, not indices. `selected` is cleared before a
  turn resolves for the same reason.
- **Text sampling:** `mesh.frag` samples the atlas outside the solid-geometry
  branch so mip selection always has valid derivatives. The atlas pads glyphs
  by 8px so the smallest of its 4 mip levels doesn't bleed neighbors together.
- **Window:** opens at 80% of the primary monitor, centered; the city scenarios
  start with the camera on the whole map (`start_on_whole_map`).
- **Line endings:** files checked out from git have CRLF endings on this
  machine, so multi-line `sed`/`perl` substitutions silently miss; use a real
  editor (or the Edit tool) for those.
- **Frame pacing:** `App::about_to_wait` schedules redraws at 165 FPS with
  `ControlFlow::WaitUntil`.
- **Verification:** visual changes are checked by running under validation
  (`cargo run`), screenshots, and unit tests of layout and hit-testing. A
  throwaway AI-vs-AI loop in a scratch test module is a quick way to exercise
  rules end to end.

## History

1. Vulkan framework from scratch: window, instance, device, swapchain, triangle
   pipeline, resize handling, frame sync.
2. Spinning triangle, then a 3D prism with depth buffer and mouse rotation; 165 FPS cap.
3. Pivot to the hex combat prototype: dynamic per-frame vertex buffers, 2D
   orthographic camera, depth buffer removed. Hex grid, four unit types,
   Civ-style combat.
4. Single-letter unit labels (bitmap font), mouse-wheel zoom, middle-drag pan.
5. We-go turns: queued orders, then resolution. Moved from "all moves then all
   attacks" to the per-type resolution order. Grid shrunk to radius 3.
6. Undo by re-clicking; attacks target hexes (shift-click) so you can anticipate
   moves; staggered playback with highlight flashes.
7. Simple Red AI; player controls Blue only.
8. Fixes: fanned-out markers when several units target one hex; ghost previews
   for queued moves (RGBA vertex colors); cancelling a move drops the attack it
   enabled; SPIR-V alignment bug.
9. Path-blocking movement (BFS); units can't hop over each other.
10. Codebase cleanup: game module split into camera/turn/ai/draw; dead code
    removed; drop-order bug fixed.
11. Terrain: hills (+25% defense), mountains (impassable), center pass; AI uses
    walking distance.
12. Simultaneous resolution within a step; move/attack order badges.
13. Contested hexes replace enemy bounces; ally swaps (Ctrl-click); allies
    can't target the same hex; input moved into `orders.rs`.
14. Abilities (Shield Wall, Volley, Charge, Deploy) with an on-screen button;
    renderer draws multiple batches (world + UI); full bitmap font.
15. Project record (this file). Auto-select the next unit once the current one
    has moved and attacked; first unit selected each turn; Space to skip
    (ability moved to Q); camera glides to the unit the game selects.
16. Turns end automatically once every unit has acted; Enter removed. Space
    now holds a unit (keeps queued orders, forfeits the rest); Tab browses.
17. City system (Codex, `codex/city-system`): cities, logistics, growth, builds,
    settlers, workers; Enter ends planning again so cities can be planned.
18. UI merge and text: antialiased TrueType text through a glyph atlas (renderer
    gained textures); self-sizing panels fix overlapping city text; action
    buttons (Move/Attack/Swap/ability/Hold) with tooltips and armed states;
    hover info box; End Turn button; larger centered window, zoomed out.
19. Window/taskbar icon drawn in code; F5 toggles borderless fullscreen.
20. Worked-tile rings, click-a-unit to leave the city view, no route preview;
    yields only in the city view with a Y toggle; tile tooltip on hover.
21. Space ends the turn once nothing's waiting (Enter removed); the turn also
    waits for city builds, not citizens; Guard (G) skips a unit every turn.
    The End Turn button names what's waiting. Hold Escape to quit. MSAA, at
    the highest sample count the GPU supports.
22. Group orders: Alt-drag/Alt-click to select several units; they converge
    on a clicked hex or all attack a clicked enemy. Attack arcs replace the
    little target markers.
23. Testing aids: a savestate (F6 save, F7 load) and a faded DEBUG panel with
    buttons for the scenario pages and savestate.
24. Attack arrows as clean ribbons, player's only; resolved attacks animate
    (shot, hit burst / miss / out of range, damage numbers).
25. Debug toggle (F8) for instant turn playback.
26. Map generation: eleven tile types with their own yields, rivers along hex
    edges (fresh water +1 food), workable water, and a seeded random world
    on F4.
27. Hills and forest/jungle became modifiers on a base ground, with marsh
    and jungle added; the Scout unit (Lookout ability); fog of war with an
    F9 toggle; the vertex buffer grows as needed.
28. The world became a Pangea on a 61x36 rectangular map (`Shape`), sized
    for four players, starting each side with a settler, worker and scout.

## Open questions and ideas

- The AI never uses abilities. Deploying its siege on a good hex is the obvious
  first step.
- Cooldowns tick every turn whether or not the unit acted.
- Contests only form when two enemies arrive in the same step. A later arrival
  is just blocked; it could be allowed to charge in and contest instead.
- A unit locked in a contest still retaliates against third-party melee attackers.
- A move only checks its destination at resolution, not whether its planned
  path is still open.
- No line of sight: ranged and siege can shoot over mountains.
- Hills and forest cost the same to enter as plains, and rivers don't slow
  or penalize crossing units (Civ does both).
- Generated maps have no resources yet, and the AI settles wherever its
  settler starts. There are still only two sides on the four-player map.
- The AI ignores the fog of war, and AI scouts just fight. Units can't path
  through a hidden enemy, so the move range can hint at one in the fog.
- Swaps only work between adjacent units.
- No victory condition; F1–F3 restart a scenario.
