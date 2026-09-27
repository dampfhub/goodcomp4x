# src/

`main.rs` starts the logger and the `winit` event loop. `app.rs` owns the window, the renderer and
the `GameState`, turns input into `GameState` method calls, and builds each frame. `icon.rs`
draws the window/taskbar icon in code.

## Frame and input flow

- Each redraw, `App` calls `game.update(dt)` and updates hover, then builds world vertices.
  The classic presentation adds a `game.build_ui(..)` batch; the default ImGui presentation
  builds native windows from the same panel content. `Renderer::draw_frame` draws the world,
  optional classic UI, and ImGui data in order. F11 switches presentations.
- The key map is the `KeyCode` match in `App::window_event`; most arms call one `GameState`
  method. Escape has its own `KeyboardInput` arm because it acts on press and release (close a
  view, or hold to quit); F5 is handled by `App` itself.
- Left clicks act on release, and only if the cursor moved less than the 6-pixel drag threshold
  (otherwise it was a pan): `handle_click`, or with Alt held, `toggle_in_selection` /
  `select_in_box`. Right clicks act on press: `handle_context_click`.
- Frame pacing: `about_to_wait` schedules redraws at 165 FPS with `ControlFlow::WaitUntil`.
- The window opens at 80% of the primary monitor, centered; the city scenarios start with the
  camera on the whole map (`start_on_whole_map`). F5 toggles borderless fullscreen.

## Invariants

- `App` declares `renderer` before `window` so the renderer (and its Vulkan surface) drops
  first. Keep that field order.
- World space has +Y up. UI space is pixels with the origin at the window's bottom-left, +Y up,
  so shape and text helpers work in both. The cursor arrives top-left-origin; `click_ui` flips it.
- There is no depth buffer: layering is draw order (painter's algorithm).
