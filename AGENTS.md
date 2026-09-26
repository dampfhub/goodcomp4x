# Hex Combat Sandbox (riskofcivlike)

A prototype of combat and city rules for a Civ-like game, in Rust (edition 2024) on raw Vulkan
(`ash`) with `winit`. Blue (the player) plays an AI Red on a hex map. Turns are simultaneous: both
sides queue orders, then the turn resolves in a fixed order by unit type. One binary crate.

## Commands

| Task | Command |
|---|---|
| Test | `cargo test` (unit tests only, no GPU needed, under a second once built) |
| Lint | `cargo clippy --all-targets` |
| Format | `cargo fmt` |
| Play | `cargo run --release` (release skips Vulkan validation) |
| Debug with validation | `cargo run` (the validation layer comes with the Vulkan SDK; without it the game warns and runs unvalidated) |

Logging goes through `log`/`env_logger` at `info` by default, which includes the combat log;
`RUST_LOG` changes it.

Building needs `glslc`: `build.rs` compiles `shaders/` with `$VULKAN_SDK/bin/glslc`, falling back
to `PATH`.

## Rules

1. Done means `cargo test` passes, `cargo clippy --all-targets` adds no warnings, and `cargo fmt`
   has run. A visible change also needs evidence it looks right: a layout or hit-test unit test
   (`src/game/ui.rs` has examples), or running the game.
2. When behavior changes, update the doc that describes it in the same change: rules in
   `docs/game-rules.md`, keys in `docs/controls.md`, structure in `docs/architecture.md`, and
   the nearest `AGENTS.md` if an instruction there became wrong.
3. Files check out with CRLF line endings on Windows, so multi-line `sed`/`perl` substitutions
   silently match nothing. Make multi-line edits with an editor (the Edit tool), not a regex.
4. On Windows, close a running game before rebuilding: the exe is locked while it runs.
5. Fix small bugs you find in code you are already changing, with a test. File anything bigger
   on the work board (below) instead of leaving it in a comment.

## Where things are

| Path | What | Read first |
|---|---|---|
| `src/app.rs`, `src/main.rs` | window, input and key map, frame loop | `src/AGENTS.md` |
| `src/renderer/` | general 2D Vulkan renderer; knows nothing about the game | `src/renderer/AGENTS.md` |
| `src/game/` | all game state, rules, AI, drawing and UI | `src/game/AGENTS.md` |
| `shaders/` | GLSL, compiled by `build.rs` | `src/renderer/AGENTS.md` |
| `docs/` | architecture, rules, controls, design proposals, history | `docs/README.md` |
| `tools/` | repo tooling (commit-message lint) | the file's header comment |
| `.claude/skills/board/` | the work-board wrapper and its workflow | its `SKILL.md` |

A nested `AGENTS.md` holds the rules for its directory; read it before editing there.

## Verifying

- Game logic: a unit test in the module's `#[cfg(test)] mod tests`. For end-to-end rule checks,
  a throwaway AI-vs-AI loop in a scratch test module exercises whole turns quickly.
- Anything visual: run `cargo run` (validation on) and watch for validation errors in the log;
  UI layout and click targets are testable without a window (see `ui.rs` tests).
- The game has testing aids: F1-F3 restart a scenario, F6/F7 save and load a snapshot, F8 plays
  a turn instantly (`docs/controls.md`).

## Work board

Work is tracked on https://github.com/users/dampfhub/projects/1 through
`node .claude/skills/board/board.mjs` (no arguments prints usage; the `board` skill has the
workflow). Claim an item before starting, keep its `Files` current, and run `fences` before
working in parallel with another agent. Check `list --open` before filing something new.

## Commits and PRs

- One PR per change, with a description of what changed and how it was verified.
- Put `Closes #N` on one line. A line ending in `fix`/`close`/`resolve` (any tense) followed by a
  line starting `#N` also closes #N; `node tools/commit-msg-lint.mjs` catches that in commit
  messages.
