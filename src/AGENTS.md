# src/

`main.rs` starts the logger, parses the command line (`cli.rs`: `--scenario`, `--seed`,
`--screenshot`, `--size`, `--host`/`--port`, `--join`/`--code`), sets up a network game
(`net/`: hosting listens; joining connects, runs the encrypted handshake and receives the game
before the window opens), and
runs the `winit` event loop; its exit code is `App::into_result`. `App` pumps the network
session once a frame. Everything `net/` receives is untrusted: keep every message sealed
(`net/secure.rs`) and the frame limit, and add a check in `GameState::receive` / `check_plan`
for anything new a message carries, with a case in the randomized plan test
(`docs/multiplayer.md`, Security). Don't hand-roll cryptography: use the RustCrypto crates.
`app.rs` owns the window, the renderer and the `GameState`, turns input into `GameState` method
calls, and builds each frame. It also carries out what the settings menu's Multiplayer page asks
(`GameState::take_net_request`: host, join on a thread of its own, leave, or copy to the
clipboard), and while a text field has the keys (`App::typing`) they go to it (the key map's `TYPING` keys, `game/keys.rs`), not to the game.
`clipboard.rs` is the system clipboard as text (Win32 through `windows-sys` under
`cfg(windows)`, a stub elsewhere); `App` uses it for the classic fields' Ctrl+V/C/X and the
COPY buttons, and gives it to ImGui (`set_clipboard_backend`). The game never touches it, or
the network: `App` finds the host's LAN address (`net::lan_address`) and passes it in. `screenshot.rs` is screenshot mode. `persist.rs` keeps settings
and layout between sessions (`docs/architecture.md`, Between sessions); screenshot mode skips it. `icon_art.rs` draws the
game's icon in code (std only); `icon.rs` hands it to the window (title bar and taskbar), and
`build.rs` includes `icon_art.rs` to embed it in the Windows executable as a `.res` the MSVC
linker takes. On Windows the taskbar button needs two more things (`icon.rs`, `app.rs`): the
process claims its own application id before any window exists
(`claim_taskbar_identity`), so the button shows the window's icon rather than one derived
from the executable, and the icons are set again as new handles a second after the window
shows (`refresh_icons`), because Windows doesn't always redraw the button with icons set as
the window is created (it did once the window was minimized and restored).

## Frame and input flow

- Each redraw, `App` calls `game.update(dt)` and `game.animate_clouds(dt)` (skipped in
  screenshot mode, so shots stay reproducible) and updates hover, then builds world vertices. The camera receives the actual viewport size before
  cloud animation and geometry culling, including ultrawide windows.
  The classic presentation adds a `game.build_ui(..)` batch; the default ImGui presentation
  builds native windows from the same panel content. `Renderer::draw_frame` draws the world,
  optional classic UI, and ImGui data in order. F11 switches presentations.
- The key map is `src/game/keys.rs`: `App::window_event` turns a key press into a `keys::Key`
  (`game_key`), looks up its `Command` (`keys::command_for`, in `PLAYING`, or `TYPING` while a
  text field has the keys) and carries it out (`App::carry_out`, `App::type_key`); most commands
  call one `GameState` method. Escape (`Command::Back`) calls `GameState::press_escape` (close
  the settings menu, a view or the selection, or else open the settings menu); F5 and F11 are
  handled by `App` itself. The UI names keys from the same map (`Command::key`).
  The settings menu's Quit button sets a flag (`GameState::quit_requested`) that the next frame
  checks before closing the window.
- Left clicks act on release, and only if the cursor moved less than the 6-pixel drag threshold:
  `handle_click` (the modifiers pick its `ClickMode`: Shift queues, or adds a clicked unit to
  the selection; Ctrl swaps, or takes a clicked group member out). A longer drag that started
  on the map is a selection box: `select_in_box` (Shift adds). Only the middle button pans.
  Right clicks act on press: `handle_context_click` (attack; Shift queues, Ctrl clears).
- Frame pacing: `about_to_wait` schedules redraws with `ControlFlow::WaitUntil`, spaced by
  `frame_interval`: the refresh rate of the window's monitor (read again when the window moves
  or changes scale), at most 165 FPS, or 30 FPS while the window doesn't have the focus
  (`App::focused`). Screenshot mode always draws at the focused rate.
- The world and classic UI vertex buffers live on `App` and are refilled each frame
  (`build_vertices_into`, `build_ui_into`), keeping their memory.
- The window opens at 80% of the primary monitor (or `--size`), centered, and can't be
  made smaller than `MIN_WINDOW_SIZE` (logical pixels, `game/ui/mod.rs`), which the ImGui
  status bar fits; the city scenarios
  start with the camera on the whole map (`start_on_whole_map`). F5 toggles borderless
  fullscreen.
- Screenshot mode (`--screenshot out.png`): the window is created hidden, which gets no
  `RedrawRequested`, so `about_to_wait` calls `App::redraw` itself; input events are ignored.
  After each frame `Screenshot::after_frame` counts frames, then (after `SETTLE_FRAMES` and
  `SETTLE_TIME`) asks the renderer to `capture_next_frame`, takes it with
  `take_captured_frame`, writes the PNG (`png` crate) and quits. `--world-overlay` with
  `--screenshot` disables fog, holds the details overlay and zooms all the way out
  to reproduce the worst-case world view. A failure or the 20 s timeout
  quits through `App::fail`, which `main` turns into a non-zero exit.

## Invariants

- `App` declares `renderer` before `window` so the renderer (and its Vulkan surface) drops
  first. Keep that field order.
- World space has +Y up. UI space is pixels with the origin at the window's bottom-left, +Y up,
  so shape and text helpers work in both. The cursor arrives top-left-origin; `click_ui` flips it.
- There is no depth buffer: layering is draw order (painter's algorithm).
