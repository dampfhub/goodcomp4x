# Project guidance for coding agents

Before adding or changing screen-space UI, read [docs/ui-system.md](docs/ui-system.md).
Build panels with `PanelBuilder` and place them through `Layout::dock_panel` in
`src/game/ui.rs`. Use the `Zone` anchors from `src/game/ui/dock.rs`; avoid
manually positioning persistent panels. Rendering and hit testing must use the
same layout. Cursor-anchored tooltips and modal overlays are exceptions.

Run `cargo fmt`, `cargo test`, and `cargo build --release` after UI changes.
