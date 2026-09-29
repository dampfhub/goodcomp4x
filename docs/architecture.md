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
src/cli.rs           command-line flags (--scenario, --seed, --screenshot, --size, --host/--join)
src/net/             a network game's link: TCP, sealed with a key from the join code (docs/multiplayer.md)
src/app.rs           App: window, input -> GameState calls, frame pacing (monitor rate, at most 165 FPS), F5 fullscreen
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

1. `game.update(dt)`: camera glide, effect and turn-transition ages, fog-of-war memory
   (`explore`), and, while a turn is resolving, the next step (or every step, with instant
   playback).
2. `game.update_hover_imgui` or `game.update_hover`: which map hex the cursor rests on.
3. `game.build_vertices_into(buffer)`: world geometry, drawn with `game.camera.view_proj(size)`.
4. The default ImGui presentation builds dockable windows from shared panel content. F11
   selects the classic `game.build_ui_into(size, cursor, buffer)` presentation instead.
5. `renderer.draw_frame(&[world, optional classic UI], optional ImGui data)`.

World and classic UI vertices are rebuilt from `GameState` each frame, into buffers `App`
keeps between frames (a busy map is over 100k vertices, and filling fresh memory each frame cost
as much as building it). Per-hex layers draw only hexes the camera may show (`may_show`,
`draw.rs`); map icons and unit pictograms are triangulated once and then placed
(`mesh::place`). Barrier edges and route labels are emitted in coordinate order so
shared wall posts keep a stable painter order. ImGui retains window
layout state so the player's panel positions survive view changes.
The turn transition (`transition.rs`) is presentation only: as `update` resolves steps it
notes where each unit and worker the player sees is drawn (`before_steps`), and afterwards
anything that moved glides from that spot to its new one over `GLIDE_TIME` (`drawn_unit_layout`,
`worker_glide`); a finished turn starts the cue, a moment's dimming of the map
(`push_turn_dim`, under the units) and the turn number flashing gold in both presentations
(`turn_number_color`). Resolution, hit tests and the lockstep read the real positions, a copy
of the game (savestate, a network turn's start) starts settled, and with no time passing
(screenshot mode's opening position, tests calling `update(0.0)`) nothing moves.

`cargo test --release perf_report -- --ignored --nocapture` (`src/game/perf.rs`) times each
stage of a frame and a turn on a busy six-AI world; run it before and after a change that might
cost frame time.
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
   In a network game `end_planning` sends the side's plan instead (`submit_plan`,
   `docs/multiplayer.md`). While it waits for the others' plans, `is_resolving` refuses every
   change to the plan, but looking (selecting, opening views) waits only for `is_playing_out`,
   and the panels show the controls that would change the plan disabled
   (`PanelBuilder::freeze_plan`). The End Turn button then takes the plan back
   (`take_back_turn`) until the host resolves the turn.
3. **Resolution** (`update`, `turn.rs`): one step every `STEP_INTERVAL` (0.6 s), or all at once
   with instant playback (F8). Each unit step resolves one unit type's moves or attacks
   simultaneously; `effects.rs` animates attacks; `transition.rs` glides what moved; dead units are removed at the end of an attack
   step, and enemy workers caught by a move are captured. `city/rail.rs` checks road connectivity
   for one-turn transfers from a city ring to its remote Railhead; a blocked line fails at this
   step. The last step, `resolve_workers`
   (`workers.rs`), sends cities' idle workers out to their queued jobs and walks, works or brings
   home every worker on the map. A connected Work Camp can be their base for nearby jobs.
4. **End of turn:** `resolve_coastal_batteries` targets ships from placed batteries, then
   `resolve_transport` unloads and boards surviving Landing Craft passengers. Ships use
   domain-aware reachability and the same unit order pipeline; Harbors spawn them onto water
   from the city queue. `resolve_city_interiors` (`city/interior.rs`) projects adjacent field troops,
   resolves their separate tactical orders and any command-post capture. Then `resolve_economy`
   (`city/citizens.rs`) adds every city's food, wood and metal (`income`, including local
   Cannery and Smelter collection, `city/logistics.rs`) to its side's stockpile
   (`GameState::stockpiles`, `city/economy.rs`), feeds the citizens from it, has each queue pay
   for the item it starts and gives it a turn's work (`work_queues`: the first item paid for or
   affordable, city by city in order, each city's queue before its Barracks'; `work_rate`) and
   completes builds, growth included; builds were queued unpaid (`queue_build`), by the
   player's clicks or the AI's `plan_ai_cities`; each
   unit's `end_turn` starts or ticks its ability cooldown, finishes a siege setup or pack-up,
   sets or clears Lookout, and clears its orders; `advance_queues` (`order_queue.rs`) gives each
   unit with a queue its next turn's orders, dropping queues that no longer fit; then selection
   moves to the first unit needing orders, or else the first city needing a build
   (`select_next_needing_attention`). Last, the turn cue starts (`start_transition`, see A frame).

The rules each step applies are in `game-rules.md`.

## Player settings

`Settings` (`src/game/settings.rs`) holds the player's options, owned by `GameState` as
`settings`, with defaults in `Settings::default`. Game code reads its fields directly. The
settings menu Escape opens (`press_escape`) lists every `Setting` from `Setting::ALL` under its
`group`'s heading, each an integer in its `range` changed with the `control` it names (a
checkbox, a slider or a choice of named values), so a new setting is a field and its `Setting`
entry in that one file; both UI presentations pick it up (`docs/ui-system.md`). `switch_scenario` and
`load_state` carry the settings, and whether the menu is open, over into the new game.

## Between sessions (`src/persist.rs`)

The session is kept in text files in the config folder (`%APPDATA%\riskofcivlike`, or
`$XDG_CONFIG_HOME` / `~/.config` `/riskofcivlike`), each prepared in a process-specific temporary file and synced before replacement. If preparation fails, the previous file stays intact; an in-place write is used only when rename is unsupported:

- `settings.txt`: `Settings::to_text`, a `key value` line per setting (`Setting::key`). `App`
  saves it whenever the text changes, and builds the first game with it (`Scenario::new_game`),
  so a world started from the command line uses the saved world settings.
- `layout.txt`: saved on quitting (the window's close button or the settings menu's Quit): the
  presentation, the window's normal size and whether it's maximized (`SavedWindow` in
  `app.rs`), then `ImGuiLayoutState::to_text`: every panel slot's geometry, the boxes, and the
  Debug and Selection panels' placements the player chose.
- `network.txt`: `NetMenu::to_text`, what the settings menu's Multiplayer page had typed (the
  port, the host's address and the number of players; not the join code). Saved on hosting,
  joining and quitting, and read into the first game's menu.
- `imgui.ini`: ImGui's own settings (`save_ini_settings`), which hold the dock nodes and which
  panel is docked in which; panels no longer set `NO_SAVED_SETTINGS`. Loaded into the context
  before the first frame. A box's dockspace id comes from its window title, so panels saved in
  a box land back in it.

Every reader skips what it doesn't understand, so an old or damaged file loads with defaults
for the rest. Screenshot mode neither reads nor writes any of it, so shots stay repeatable.

## Testing aids

Scenarios (F1 combat, F2 cities, F3 frontier, F4 a generated world, F12 siege, Debug Naval) are constructors on
`GameState`; the savestate (F6
save, F7 load) clones the whole `GameState`. Both live in `scenario.rs` and in memory only. Unit
tests build a scenario and drive the same methods input does, so no window or GPU is needed;
`simulation.rs` plays whole AI-vs-AI games that way and checks invariants every turn.

`GameState.rng` (a `Xoshiro256PlusPlus`, `Clone` for the savestate) picks each F4 world's
map seed; combat has no random spread (`combat.rs`), so the same orders always fight the same
way. The game seeds it from entropy; tests seed it (`seed_rng`), and it carries across scenario
switches, so a seed replays the same game, map included. `mapgen.rs` has its own RNG, seeded by
the map seed.

Screenshot mode (`--screenshot out.png`, `src/screenshot.rs`) is the visual counterpart: it
draws a scenario's opening frame in a hidden window and saves it as a PNG. The renderer only
offers "copy the next frame back" (`capture_next_frame`, `take_captured_frame`); the app decides
when, and writes the file.
