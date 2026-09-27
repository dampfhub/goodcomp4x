# Hex Combat Sandbox (riskofcivlike)

A prototype of combat and city rules for a Civ-like game, in Rust (edition 2024) on raw Vulkan
(`ash`) with `winit`. Blue (the player) plays an AI Red on a hex map. Turns are simultaneous: both
sides queue orders, then the turn resolves in a fixed order by unit type. One binary crate.

## Commands

| Task | Command |
|---|---|
| Test | `cargo test` (no GPU needed, about a second once built) |
| Lint | `cargo clippy --all-targets -- -D warnings` |
| Format | `cargo fmt` (CI runs `cargo fmt --check`) |
| Play | `cargo run --release` (release skips Vulkan validation) |
| Debug with validation | `cargo run` (the validation layer comes with the Vulkan SDK; without it the game warns and runs unvalidated) |
| Repo tool self-tests | `node tools/board/board.mjs selftest`, `node tools/commit-msg-lint.mjs --self-test` |

Building needs Rust 1.92+ and `glslc`: `build.rs` compiles `shaders/` with
`$VULKAN_SDK/bin/glslc`, falling back to `PATH`. Logging goes through `log`/`env_logger` at
`info` by default, which includes the combat log; `RUST_LOG` changes it.

## Rules

1. Done means `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
   all pass: CI (`.github/workflows/ci.yml`) runs exactly these on every PR. A visible change also
   needs evidence it looks right: a layout or hit-test unit test (`src/game/ui/tests.rs` has examples),
   or running the game.
2. Before editing in a directory that has its own `AGENTS.md` (see the table below), read it.
   Some agents load those files automatically and some don't.
3. When behavior changes, update the doc that describes it in the same change: rules in
   `docs/game-rules.md`, keys in `docs/controls.md`, structure in `docs/architecture.md`, and
   the nearest `AGENTS.md` if an instruction there became wrong.
4. Files check out with CRLF line endings on Windows, so multi-line `sed`/`perl` substitutions
   silently match nothing. Make multi-line edits with an editor (the Edit tool), not a regex.
5. On Windows, close a running game before rebuilding: the exe is locked while it runs.
6. Fix small bugs you find in code you are already changing, with a test. File anything bigger
   on the work board (below) instead of leaving it in a comment.
7. Keep agent tooling agent-neutral: instructions in `AGENTS.md`, skills in `.agents/skills/`
   (Codex reads them there) with a pointer copy in `.claude/skills/` (Claude Code reads them
   there; `board.mjs selftest` checks the two match), scripts in `tools/`. Anything that must hold
   for everyone goes in CI or a script, not in one agent's settings or hooks.
8. The ImGui presentation is active by default, with F11 switching to the classic UI.
   Shared panel content and actions must work in both; read `docs/ui-system.md` before UI edits.

## Where things are

| Path | What | Read first |
|---|---|---|
| `src/app.rs`, `src/main.rs` | window, input and key map, frame loop | `src/AGENTS.md` |
| `src/renderer/` | general 2D Vulkan renderer; knows nothing about the game | `src/renderer/AGENTS.md` |
| `src/game/` | all game state, rules, AI, drawing and UI | `src/game/AGENTS.md` |
| `shaders/` | GLSL, compiled by `build.rs` | `src/renderer/AGENTS.md` |
| `docs/` | architecture, rules, controls, design proposals, history | `docs/README.md` |
| `tools/board/` | work-board wrapper (`board.mjs`) and its config | `.agents/skills/board/SKILL.md` |
| `tools/commit-msg-lint.mjs` | refuses commit messages that close an issue by accident | its header comment |
| `.agents/skills/`, `.claude/skills/` | agent skills (canonical, and pointers for Claude Code) | the skill's `SKILL.md` |
| `.github/` | CI workflow and PR template | |

## Verifying

- Game logic: a unit test in the module's `#[cfg(test)] mod tests`. Whole-game behavior:
  `src/game/simulation.rs` plays AI against AI in every scenario and checks the board's
  invariants each turn; extend its invariants when you add a rule. Its games are seeded (a few
  fixed seeds by default), and a failure prints the seed: `SIM_SEED=<seed> cargo test simulation`
  replays exactly that game (`$env:SIM_SEED=<seed>` first in PowerShell), and `SIM_SEEDS=<n>`
  plays seeds `0..n` to hunt for failures.
- Anything visual: run `cargo run` (validation on) and watch for validation errors in the log;
  UI layout and click targets are testable without a window (see `src/game/ui/tests.rs`).
- In-game testing aids: F1-F4 restart a scenario (F4 generates a new map), F6/F7 save and load a
  snapshot, F8 toggles instant turn playback, F9 finishes the current build, F10 toggles fog of
  war (`docs/controls.md`).

## Work board

Work is tracked on https://github.com/users/dampfhub/projects/1 through
`node tools/board/board.mjs` (no arguments prints usage; the `board` skill has the workflow).
Claim an item before starting, keep its `Files` current, and run `fences` before working in
parallel with another agent. Check `list --open` before filing something new.

## Commits and PRs

- One PR per change, using the PR template: what changed, how it was verified, what it closes.
- A PR merges itself: CI's `merge` job merges it once `rust`, `msrv` and `tools` pass, deletes
  its branch, then runs CI on `main`. To keep a PR open (for review, or while still working), open it as a draft
  (`gh pr create --draft`, or `gh pr ready --undo <#>`); marking it ready runs CI and merges it.
- Put `Closes #N` on one line. A line ending in `fix`/`close`/`resolve` (any tense) followed by a
  line starting `#N` also closes #N; `node tools/commit-msg-lint.mjs` catches that in commit
  messages, and CI runs it on every PR's commits.
