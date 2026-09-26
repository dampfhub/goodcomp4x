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
    pipeline.rs    render pass (4x MSAA target resolved into the swapchain image)
                   + the single alpha-blended vertex-color pipeline
    msaa.rs        the multisampled color target, rebuilt with the swapchain
    texture.rs     the coverage atlas (R8 + mips), uploaded once, bound as set 0
    buffer.rs      buffer + memory allocation
    sync.rs        per-frame semaphores and fences (2 frames in flight)
    vertex.rs      Vertex { pos, color, uv }; SOLID_UV marks untextured geometry
  game/            everything game-specific
    mod.rs         GameState, map/unit setup, shared queries (units_at, rival_of,
                   swap_partner, reachable_hexes), controls help text, tests
    orders.rs      player input and order planning (click, right-click, swap, ability toggle)
    turn.rs        RESOLUTION_ORDER and simultaneous step resolution (moves, attacks)
    combat.rs      damage formula, retaliation rule, combat log helpers
    ability.rs     the four abilities and their tuning constants
    unit.rs        Team, UnitType, base stats, Unit (with state-aware stats())
    ai.rs          the Red AI
    hex.rs         axial hex math and HexGrid (with terrain)
    terrain.rs     Plains / Hills / Mountains
    camera.rs      top-down orthographic camera: pan, zoom, screen<->world
    draw.rs        world geometry: hexes, terrain symbols, order markers, units, badges
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
  and preplaced dirt roads. F1 resets to the original combat scenario; F2 resets
  to cities. Both discard the running match.
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
- Hex grid of radius 3 (flat-top, axial coordinates).
- Mountain ridges at (0,-3), (0,-2), (0,2), (0,3) leave a three-hex pass down
  the middle. Hills at (0,0) (center of the pass), (-2,2) and (2,-2), next to
  each side's ranged unit.
- Hills: the unit standing there gets +25% defense.
- Mountains: impassable. You can't enter them, path through them, or target them.

### Units
| Type | HP | Attack | Defense | Move | Range | Icon |
|---|---|---|---|---|---|---|
| Melee | 100 | 22 | 20 | 1 | 1 | triangle, M |
| Ranged | 75 | 24 | 10 | 1 | 2 | square, R |
| Cavalry | 100 | 24 | 14 | 2 | 1 | pentagon, C |
| Siege | 65 | 32 | 6 | 1 | 2 | octagon, S |

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
The turn plays out in 8 steps, one every 0.6s, with the acting units flashing.
Steps where nobody acts are skipped.

1. Cavalry move  2. Melee move  3. Ranged attack  4. Cavalry attack
5. Melee attack  6. Ranged move  7. Siege move  8. Siege attack

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

### Abilities (`ability.rs`)
| Unit | Ability | Effect | Cooldown |
|---|---|---|---|
| Melee | Shield Wall | +50% defense this turn, can't move | 1 turn |
| Ranged | Volley | attack also hits enemies adjacent to the target, all hits 60% | 2 turns |
| Cavalry | Charge | +1 move, +50% attack this turn | 2 turns |
| Siege | Deploy / Pack Up | spend a turn setting up (no move or attack); deployed: +1 range, can't move; packing up takes a turn too | none |

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
    The End Turn button names what's waiting. Hold Escape to quit. 4x MSAA.

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
- Hills cost the same to enter as plains (Civ charges extra movement).
- Swaps only work between adjacent units.
- No victory condition; F1–F3 restart a scenario.
