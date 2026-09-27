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
src/icon.rs          window/taskbar icon (pixels from src/icon_art.rs)
src/icon_art.rs      the icon drawn in code; build.rs also embeds it in the Windows exe
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

1. **Planning.** Input calls `GameState` methods in `orders.rs`, `group.rs`, `order_queue.rs`,
   `city/` and `ui/`, which queue orders on units and builds on cities. Left clicks move, right
   clicks attack, and Shift-clicks add turns to a unit's order queue (`Unit::queued`, the turns
   after this one). `pending()` counts what still needs attention; the End Turn button names it.
2. **End of planning.** Space with nothing waiting and the End Turn button both call
   `end_planning` (`city/view.rs`), which holds unfinished units, may open a city still needing a build
   and stop there, auto-assigns the AI sides' citizens, then calls `resolve_turn` (`turn.rs`): selection
   is cleared, every AI side plans (`plan_ai_turn` for each of `ai_teams`, `ai.rs`), and every
   step of `RESOLUTION_ORDER` is
   queued as a `Step::Units`, followed by `Step::Workers`.
3. **Resolution** (`update`, `turn.rs`): one step every `STEP_INTERVAL` (0.6 s), or all at once
   with instant playback (F8). Each unit step resolves one unit type's moves or attacks
   simultaneously; `effects.rs` animates attacks; dead units are removed at the end of an attack
   step, and enemy workers caught by a move are captured. The last step, `resolve_workers`
   (`workers.rs`), sends cities' idle workers out to their queued jobs and walks, works or brings
   home every worker on the map.
4. **End of turn:** `resolve_city_interiors` (`city/interior.rs`) projects adjacent field troops,
   resolves their separate tactical orders and any command-post capture. Then `resolve_economy`
   (`city/citizens.rs`) applies city income, growth and builds; each
   unit's `end_turn` starts or ticks its ability cooldown, finishes a siege setup or pack-up,
   sets or clears Lookout, and clears its orders; `advance_queues` (`order_queue.rs`) gives each
   unit with a queue its next turn's orders, dropping queues that no longer fit; then selection
   moves to the first unit needing orders, or else the first city needing a build
   (`select_next_or_end_turn`).

The rules each step applies are in `game-rules.md`.

## Player settings

`Settings` (`src/game/settings.rs`) holds the player's options, owned by `GameState` as
`settings`, with defaults in `Settings::default`. Game code reads its fields directly. The
settings menu Escape opens (`press_escape`) lists every `Setting` from `Setting::ALL`, each an
integer stepped through its `range`, so a new setting is a field and its `Setting` entry in that
one file; both UI presentations pick it up (`docs/ui-system.md`). `switch_scenario` and
`load_state` carry the settings, and whether the menu is open, over into the new game.

## Between sessions (`src/persist.rs`)

The session is kept in text files in the config folder (`%APPDATA%\riskofcivlike`, or
`$XDG_CONFIG_HOME` / `~/.config` `/riskofcivlike`), each written whole through a temporary file:

- `settings.txt`: `Settings::to_text`, a `key value` line per setting (`Setting::key`). `App`
  saves it whenever the text changes, and builds the first game with it (`Scenario::new_game`),
  so a world started from the command line uses the saved world settings.
- `layout.txt`: saved on quitting (the window's close button or the settings menu's Quit): the
  presentation, the window's normal size and whether it's maximized (`SavedWindow` in
  `app.rs`), then `ImGuiLayoutState::to_text`: every panel slot's geometry, the boxes, and the
  Debug and Selection panels' placements the player chose.
- `imgui.ini`: ImGui's own settings (`save_ini_settings`), which hold the dock nodes and which
  panel is docked in which; panels no longer set `NO_SAVED_SETTINGS`. Loaded into the context
  before the first frame. A box's dockspace id comes from its window title, so panels saved in
  a box land back in it.

Every reader skips what it doesn't understand, so an old or damaged file loads with defaults
for the rest. Screenshot mode neither reads nor writes any of it, so shots stay repeatable.

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
