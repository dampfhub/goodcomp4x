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
  `tests` module at the bottom of `src/game/mod.rs`, plus one in `src/game/ui.rs`.
- Shaders are GLSL in `shaders/`. `build.rs` compiles each one with `glslc` from
  `$VULKAN_SDK` (falling back to `PATH`) into `OUT_DIR`, and the renderer embeds
  the SPIR-V with `include_bytes!`.
- On Windows, close a running instance before rebuilding; the exe is locked
  while it runs.

## Layout

```
src/
  main.rs          entry point: logger + winit event loop
  app.rs           window, input events, frame pacing (165 FPS cap), builds each frame
  renderer/        general-purpose 2D Vulkan renderer, knows nothing about the game
    mod.rs         Renderer: setup, draw_frame(&[DrawBatch]), swapchain recreation, teardown
    instance.rs    instance + validation layer + debug messenger
    device.rs      GPU selection, logical device, queues
    swapchain.rs   swapchain + image views
    pipeline.rs    render pass + the single alpha-blended vertex-color pipeline
    buffer.rs      buffer + memory allocation
    sync.rs        per-frame semaphores and fences (2 frames in flight)
    vertex.rs      Vertex { pos, color, surface } with procedural material coordinates
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
    draw.rs        textured terrain, groves, miniature surfaces, order markers, badges
    ui.rs          screen-space UI: the ability button
    mesh.rs        shape helpers: regular_polygon, quad, segment
    font.rs        5x7 bitmap font (A-Z, 0-9, + - %) drawn as quads
shaders/
  mesh.vert        applies the batch's view-projection push constant
  mesh.frag        terrain relief and ray-marched miniature materials; unlit UI
```

The frame loop: `App` calls `GameState::update(dt)`, then builds two batches:
world vertices (`build_vertices`) through `camera.view_proj`, and UI vertices
(`build_ui`) through `ui_projection`. It hands both to `Renderer::draw_frame`.
All geometry is rebuilt from game state every frame.

## Game rules as implemented

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
- Click one of your units to select it. Click a green hex to queue a move; click
  it again to cancel.
- Click an enemy in range to queue an attack on its hex. Shift-click attacks any
  hex in range, occupied or not. Attacks target *hexes*: whoever stands there
  when the attack resolves gets hit.
- A unit can queue a move and an attack; the attack range is measured from the
  planned destination. Changing or cancelling the move drops an attack that's
  no longer in range.
- Ctrl-click an adjacent ally to swap places (see below).
- Right-click clears the selected unit's orders, including a hold.
- Q or the on-screen button toggles the selected unit's ability.
- Space holds the selected unit (`Unit::holding`): it keeps whatever it has
  queued and gives up the rest of its turn.
- Selection flows through your units: the first unit needing orders is selected
  at the start of each turn, and once the selected unit is done, the next one
  is selected automatically. "Done" (`needs_orders` in `orders.rs`) means it
  is holding, or has a move queued (or can't move) and an attack queued (or
  can't attack). Enemies in range don't matter, since square attacks are
  always possible. A unit in a contested hex is always done. Selecting a unit
  by clicking never auto-advances, so a finished unit can be reselected to
  edit. Tab looks at the next unit without holding the current one.
- Whenever the game picks the unit (auto-advance, hold, Tab, turn start), the
  camera glides to it (`Camera::focus_on`); middle-drag panning cancels the
  glide.
- Movement is BFS through passable, unoccupied hexes, so units can't pass
  through each other or through mountains. Two allies can't head for the same hex.
- There is no end-turn key: the turn resolves by itself
  (`select_next_or_end_turn`) once no unit needs orders, after a 0.6s pause so
  the last order is visible. That includes a turn that starts with every unit
  contested. Input is ignored while a turn plays out.

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
- **Font:** glyph bitmaps are `#[rustfmt::skip]` so they stay readable as pictures.
- **Frame pacing:** `App::about_to_wait` schedules redraws at 165 FPS with
  `ControlFlow::WaitUntil`.
- **Verification:** there's no screenshot tooling, so visual changes are checked
  by running under validation (`cargo run`) and by unit tests of the math. A
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
- No victory condition or restart; the game just runs until one side is gone.
- The project has no git commits yet.

## Realistic rendering pass

Terrain now uses world-anchored procedural grass, soil, rock and snow, with
height-field normals, directional sunlight and local terrain shadows. Mountain
hexes contain irregular intersecting ridges; small conifer groves mark plains.
The view remains orthographic and terrain relief is shaded, not a navigable 3D
mesh. No downloaded assets, textures, or additional runtime dependencies are required.

Units are ray-marched volumetric miniatures: shield-and-sword infantry, bowmen,
mounted cavalry and timber siege engines. Steel, brass, wood and team cloth
have distinct reflectance, contact occlusion and soft self-shadowing. Class
letters, order badges, health bars, transparent move ghosts, and ability rings
remain visible. Tactical ranges use thin outlines and light tints instead of
replacing terrain with solid colors. The initial camera frames the whole map;
subsequent automatic selections still glide to their unit.

Vertex.surface stores local XY coordinates, a material kind (0 unlit, 1 terrain,
2 miniature), and a variant. Both GLSL stages and Vulkan vertex attributes must
stay in sync with this layout. UI bypasses material lighting. Fine material
grain fades with screen-space derivatives to reduce zoom shimmer.

The dark command HUD shows the turn, planning/resolution phase, controls and
ability status. Presentation-finished semaphores are allocated per swapchain
image and recreated with the swapchain; acquisition semaphores and fences
remain per frame slot. Suboptimal acquisition is consumed before recreation.
