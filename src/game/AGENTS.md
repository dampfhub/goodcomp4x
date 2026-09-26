# src/game/

Everything game-specific. The rules this code implements are written down in
`docs/game-rules.md` and the keys in `docs/controls.md`; change those in the same commit as the
behavior.

## Shape

`GameState` (in `mod.rs`) is the one struct holding the whole game. Each module below adds an
`impl GameState` block for its concern, so look for a method by concern, not by type.

| Module | Concern |
|---|---|
| `mod.rs` | `GameState` fields, scenario setup (`new`, `city_scenario`, `frontier_scenario`), shared queries (`units_at`, `rival_of`, `swap_partner`, `reachable_hexes`), `CONTROLS_HELP`, the main tests |
| `orders.rs` | player input and order planning: click, right-click, swap, ability toggle, hold, guard |
| `group.rs` | multi-unit selection (Alt-drag, Alt-click) and group orders |
| `turn.rs` | `RESOLUTION_ORDER`, `update(dt)`, simultaneous step resolution (moves, attacks) |
| `combat.rs` | damage formula, retaliation, combat log helpers |
| `ability.rs` | the four abilities and their tuning constants |
| `unit.rs` | `Team`, `UnitType`, base stats, `Unit` and its state-aware `stats()` |
| `ai.rs` | the Red AI |
| `city.rs` | cities, sites, roads, logistics routes, citizens, growth, build queues, buildings, settlers and workers |
| `hex.rs`, `terrain.rs` | axial hex math, `HexGrid`, terrain |
| `scenario.rs` | test scenarios (F1-F3), savestate (F6/F7), instant playback (F8) |
| `simulation.rs` | tests only: AI-vs-AI games in every scenario, board invariants checked each turn |
| `camera.rs` | orthographic camera: pan, zoom, glide, screen/world conversion |
| `draw.rs` | world geometry (`build_vertices`): hexes, terrain, ghosts, attack arcs, units, badges |
| `effects.rs` | attack animations during playback |
| `ui.rs` | screen-space UI (`build_ui`): top bar, command tray, tooltips, info box, debug panel |
| `mesh.rs`, `font.rs` | shape helpers; TrueType text and the glyph atlas |

## Invariants

- `units` holds living units only. Dead units are removed with `retain` at the end of an attack
  step, which shifts indices, so code that spans a removal uses unit `id`s, not indices.
  `selected` and `group` are cleared before a turn resolves for the same reason.
- Everything that asks what a unit can do goes through `Unit::stats()`, which applies abilities
  and siege deployment on top of the base table in `UnitType::stats()`. Don't read base stats
  directly.
- Attack steps read the board as it stood at the step's start and apply summed damage at its
  end; moves in a step are simultaneous. Keep new resolution logic in that shape (`turn.rs`).
- `layout()` builds a `Layout` (panels, text, buttons) from state. Drawing (`build_ui`), click
  hit-testing (`click_ui`) and hover detection (`update_hover`) each call it, so anything
  clickable must come from `layout()` to be clickable where it is drawn. Hover boxes, structure
  panels and tooltips are added only in `build_ui`: they are display-only and not hit-tested.
  Panels size themselves to their text (`PanelBuilder`).
- Text: `font.rs` rasterizes printable ASCII from the Hack font once into one R8 atlas with 4 mip
  levels, padding glyphs by 8 px so the smallest mip doesn't bleed neighbors together. UI text
  uses `font::ui(px)` with `px` one of `UI_SIZES` and snaps to whole pixels; world text
  (`push_text`, `push_glyph`) is sized by capital-letter height and scales a 64 px set.
- Colors are linear and the swapchain is sRGB: dark panels need values around 0.01-0.05.
- `Camera::view_proj` builds an OpenGL orthographic projection and flips Y itself, because
  glam 0.33's `vulkan::orthographic` flips the Y scale but not the translation.
- The AI must stay deterministic: ties break by hex coordinates.

## Recipes

- **New key:** a `KeyCode` arm in `App::window_event` (`src/app.rs`) calling a `GameState`
  method (a key that acts on release or while held needs its own arm, like Escape), a row in
  `docs/controls.md`, and a line in `CONTROLS_HELP` (printed at startup).
- **New unit button:** in `ui.rs`, a `UnitAction` variant, a `ButtonSpec` in the tray's button
  list, its tooltip text (the `UnitAction` match under `tooltip_lines`), and an arm in
  `click_ui`'s dispatch. Add a hit-test unit test.
- **Stat or tuning change:** `unit.rs` or `ability.rs`, then every place that states the number
  to players: `ability_text` in `ui.rs` (tooltips), `CONTROLS_HELP` in `mod.rs`, and the tables
  in `docs/game-rules.md`. Grep for the old value.
- **New scenario:** a `Scenario` variant (`scenario.rs`: `ALL`, `name`, `key`, `start`), its
  constructor in `mod.rs`, a key in `app.rs`; the debug panel lists `Scenario::ALL` itself.

## Tests

Each module's tests live in its own `#[cfg(test)] mod tests` (`mod.rs`, `city.rs` and `ui.rs`
hold most of them). Build a `GameState` from a scenario constructor, drive it through the same
methods input uses, and assert on state. `cargo test` needs no GPU or window. A new rule that
constrains the board (occupancy, HP, population...) belongs in `simulation.rs`'s
`check_invariants` too, so every scenario exercises it.
