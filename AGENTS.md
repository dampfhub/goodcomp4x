# Project guidance for coding agents

Before adding or changing screen-space UI, read [docs/ui-system.md](docs/ui-system.md).
Build shared panel content with `PanelBuilder` in `src/game/ui.rs`. The
experimental ImGui presentation in `src/game/ui/imgui.rs` is active by default;
also keep the classic presentation through `Layout::dock_panel` and the `Zone`
anchors functional while F11 comparison is available. See the UI system doc
for how each presentation renders and dispatches shared actions.

Run `cargo fmt`, `cargo test`, and `cargo build --release` after UI changes.
