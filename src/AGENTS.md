# src/

`main.rs` starts the logger, parses the command line (`cli.rs`: `--scenario`, `--seed`,
`--screenshot`, `--size`) and runs the `winit` event loop; its exit code is `App::into_result`.
`app.rs` owns the window, the renderer and the `GameState`, turns input into `GameState` method
calls, and builds each frame. `screenshot.rs` is screenshot mode. `icon.rs` draws the
window/taskbar icon in code.

## Frame and input flow

- Each redraw, `App` calls `game.update(dt)` and `game.update_hover(..)`, then builds two
  batches: world vertices (`game.build_vertices()`, through `game.camera.view_proj`) and UI
  vertices (`game.build_ui(..)`, through `ui_projection`), and passes both to
  `Renderer::draw_frame`. All geometry is rebuilt from game state every frame; nothing is cached
  on the GPU except the font atlas.
- The key map is the `KeyCode` match in `App::window_event`; most arms call one `GameState`
  method. Escape has its own `KeyboardInput` arm because it acts on press and release (close a
  view, or hold to quit); F5 is handled by `App` itself.
- Left clicks act on release, and only if the cursor moved less than the 6-pixel drag threshold
  (otherwise it was a pan): `handle_click`, or with Alt held, `toggle_in_selection` /
  `select_in_box`. Right clicks act on press: `handle_context_click`.
- Frame pacing: `about_to_wait` schedules redraws at 165 FPS with `ControlFlow::WaitUntil`.
- The window opens at 80% of the primary monitor (or `--size`), centered; the city scenarios
  start with the camera on the whole map (`start_on_whole_map`). F5 toggles borderless
  fullscreen.
- Screenshot mode (`--screenshot out.png`): the window is created hidden, which gets no
  `RedrawRequested`, so `about_to_wait` calls `App::redraw` itself; input events are ignored.
  After each frame `Screenshot::after_frame` counts frames, then (after `SETTLE_FRAMES` and
  `SETTLE_TIME`) asks the renderer to `capture_next_frame`, takes it with
  `take_captured_frame`, writes the PNG (`png` crate) and quits. A failure or the 20 s timeout
  quits through `App::fail`, which `main` turns into a non-zero exit.

## Invariants

- `App` declares `renderer` before `window` so the renderer (and its Vulkan surface) drops
  first. Keep that field order.
- World space has +Y up. UI space is pixels with the origin at the window's bottom-left, +Y up,
  so shape and text helpers work in both. The cursor arrives top-left-origin; `click_ui` flips it.
- There is no depth buffer: layering is draw order (painter's algorithm).
