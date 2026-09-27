# Architecture

One binary crate (`vulkan_engine` in `Cargo.toml`). Three layers, each depending only on the one
below it:

```
src/app.rs      window, input, frame loop          (winit)
   |  calls GameState methods; passes DrawBatches down
src/game/       all game state, rules, AI, drawing, UI
   |  produces Vec<Vertex> + a view-projection per batch
src/renderer/   general 2D Vulkan renderer          (ash)
```

The renderer knows nothing about the game, and the game never touches Vulkan: its only output is
vertex lists. File-level maps and the rules for editing each layer are in `src/AGENTS.md`,
`src/game/AGENTS.md` and `src/renderer/AGENTS.md`.

## Repository layout

```
AGENTS.md            agent instructions for the whole repo (nested ones under src/)
README.md            what this is and how to run it
Cargo.toml, build.rs crate manifest (rust-version 1.92); build.rs compiles shaders/ with glslc
src/main.rs          logger, command line, event loop
src/cli.rs           command-line flags (--scenario, --seed, --screenshot, --size)
src/app.rs           App: window, input -> GameState calls, frame pacing (165 FPS), F5 fullscreen
src/screenshot.rs    screenshot mode: settle, read a frame back, write a PNG, quit
src/icon.rs          window/taskbar icon drawn in code
src/renderer/        Vulkan setup, swapchain, MSAA, the single pipeline, coverage atlas
src/game/            GameState and everything game-specific
shaders/             mesh.vert (view-projection), mesh.frag (color x atlas coverage or distance field)
assets/fonts/        IBM Plex Mono SemiBold, embedded by font.rs (OFL)
docs/                architecture, rules, controls, design proposals, history (docs/README.md)
tools/               board/ (work-board wrapper + config), commit-msg-lint.mjs
.agents/skills/      agent skills, read by Codex (canonical)
.claude/skills/      pointers to those skills, read by Claude Code
.github/             CI workflow, PR template
```

## A frame

`App::window_event` on `RedrawRequested`:

1. `game.update(dt)`: camera glide, effect ages, fog-of-war memory (`explore`), and, while a
   turn is resolving, the next step (or every step, with instant playback).
2. `game.update_hover_imgui` or `game.update_hover`: which map hex the cursor rests on.
3. `game.build_vertices()`: world geometry, drawn with `game.camera.view_proj(size)`.
4. The default ImGui presentation builds dockable windows from shared panel content. F11
   selects the classic `game.build_ui(size, cursor)` presentation instead.
5. `renderer.draw_frame(&[world, optional classic UI], optional ImGui data)`.

World and classic UI vertices are rebuilt from `GameState` each frame. ImGui retains window
layout state so the player's panel positions survive view changes.
While a city interior is open, `build_vertices` draws its tactical grid and
copies in place of the exterior world; the exterior camera is restored on exit.

## A turn

1. **Planning.** Input calls `GameState` methods in `orders.rs`, `group.rs`, `city/` and
   `ui/`, which queue orders on units and builds on cities. `pending()` counts what still needs
   attention; the End Turn button names it.
2. **End of planning.** Space with nothing waiting and the End Turn button both call
   `end_planning` (`city/view.rs`), which holds unfinished units, may open a city still needing a build
   and stop there, auto-assigns Red's citizens, then calls `resolve_turn` (`turn.rs`): selection
   is cleared, the AI plans (`plan_ai_turn`, `ai.rs`), and every step of `RESOLUTION_ORDER` is
   queued.
3. **Resolution** (`update`, `turn.rs`): one step every `STEP_INTERVAL` (0.6 s), or all at once
   with instant playback (F8). Each step resolves one unit type's moves or attacks
   simultaneously; `effects.rs` animates attacks; dead units are removed at the end of an attack
   step.
4. **End of turn:** `resolve_city_interiors` (`city/interior.rs`) projects adjacent field troops,
   resolves their separate tactical orders and any command-post capture. Then `resolve_economy`
   (`city/citizens.rs`) applies city income, growth and builds; each
   unit's `end_turn` starts or ticks its ability cooldown, finishes a siege setup or pack-up,
   sets or clears Lookout, and clears its orders; then selection moves to the first unit needing orders, or else the first
   city needing a build (`select_next_or_end_turn`).

The rules each step applies are in `game-rules.md`.

## Testing aids

Scenarios (F1 combat, F2 cities, F3 frontier, F4 a generated world, F12 siege) are constructors on
`GameState`; the savestate (F6
save, F7 load) clones the whole `GameState`. Both live in `scenario.rs` and in memory only. Unit
tests build a scenario and drive the same methods input does, so no window or GPU is needed;
`simulation.rs` plays whole AI-vs-AI games that way and checks invariants every turn.

`GameState.rng` (a `Xoshiro256PlusPlus`, `Clone` for the savestate) rolls damage and picks each
F4 world's map seed. The game seeds it from entropy; tests seed it (`seed_rng`), and it carries
across scenario switches, so a seed replays the same game, map included. Loading a savestate
keeps the current RNG, so retrying a save rolls afresh. `mapgen.rs` has its own RNG, seeded by
the map seed.

Screenshot mode (`--screenshot out.png`, `src/screenshot.rs`) is the visual counterpart: it
draws a scenario's opening frame in a hidden window and saves it as a PNG. The renderer only
offers "copy the next frame back" (`capture_next_frame`, `take_captured_frame`); the app decides
when, and writes the file.
