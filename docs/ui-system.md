# Screen-space UI panels

The shared UI content lives in `src/game/ui.rs`. The game starts with the
experimental ImGui presentation (`src/game/ui/imgui.rs`), and F11 switches
between it and the classic layout. The root `AGENTS.md` points contributors
here. Add new controls to the shared `PanelBuilder` content and route their
`Target` actions through `GameState::activate_target`, so both views work.

ImGui uses native windows, buttons, scrolling, drag/drop, hover tooltips, and
input capture. It is drawn at the end of the existing Vulkan render pass.
The world, tile overlays, selection rectangle, and quit hold prompt remain in
the game's renderer. Native window positions can be moved and resized while
the game is running. Queue panels show the full queue inside a scrollable
window; their rows share the same reorder and remove game actions as classic.

The classic layout uses reusable placement code in `src/game/ui/dock.rs`.

`PanelBuilder` is the content primitive. Add rows with `text`, `bar`, `gap`,
`buttons`, `compact_buttons`, or `queue_item`; `size()` measures the finished panel. A
`Layout` owns shapes, buttons, panel hit boxes, scroll regions, and a `Dock`.
`Layout::dock_panel(panel, Zone::BottomLeft)` places the measured panel and
registers its render and hit-test geometry together. Available zones are
`BottomLeft`, `BottomRight`, `TopLeft`, and `TopRight`.

Panels in a zone stack vertically from its corner. When that column fills,
the dock moves to another column. Zones share the same occupied rectangles,
so a new panel cannot overlap a panel from another zone. The top status bar
is reserved. Placement order determines priority: persistent command panels
are placed first, then the debug panel; transient hover panels are placed
afterward. Do not calculate offsets from panel counts or hardcode another
panel's dimensions.

```rust
let mut panel = PanelBuilder::default();
panel.text(SMALL, vec![("EXAMPLE".into(), LABEL_TEXT)]);
panel.bar(0.5);
layout.dock_panel(panel, Zone::BottomLeft);
```

For content that must scroll, ask the layout for
`remaining_height(Zone::BottomLeft, panel_width)` after the preceding panel
has been docked. Choose a visible row count from that measured space, then
build and dock the scrollable panel. The city and Barracks queues demonstrate
this. Their scrollbars and wheel hit regions are registered by the same panel
placement. `dock_panel` returns `None` when a whole panel cannot fit anywhere
inside the safe area; large content must choose a bounded or scrollable form.
`queue_item` registers its row body for drag reordering and a separate small X
button for removal. Route row dragging through `GameState::start_queue_drag_at`,
`update_queue_drag_at`, and `finish_queue_drag_at`, so the city and Barracks
queues share the same hit-testing behavior.

Cursor-following tooltips, button tooltips, the quit prompt, and the selection
box are overlays, so they use their own anchors. Ordinary status, control,
hover, and debug panels belong in a `Zone`.

Add a layout test when adding a new panel or zone behavior. Useful assertions
are that panel rectangles do not overlap, buttons remain inside their panel,
and the same target is clickable after the screen size or content changes.
