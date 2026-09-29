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
| Screenshot | `cargo run -- --screenshot out.png --scenario cities` (hidden window, writes one frame, exits; needs a GPU) |
| Repo tool self-tests | `node tools/board/board.mjs selftest`, `node tools/commit-msg-lint.mjs --self-test` |

Building needs Rust 1.92+ and `glslc`: `build.rs` compiles `shaders/` with
`$VULKAN_SDK/bin/glslc`, falling back to `PATH`. Logging goes through `log`/`env_logger` at
`info` by default, which includes the combat log; `RUST_LOG` changes it.

## Rules

1. Done means `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test`
   all pass: CI (`.github/workflows/ci.yml`) runs exactly these on every PR. A visible change also
   needs evidence it looks right: a layout or hit-test unit test (`src/game/ui/tests.rs` has examples),
   a screenshot you have looked at (see Verifying), or running the game.
2. Before editing in a directory that has its own `AGENTS.md` (see the table below), read it.
   Some agents load those files automatically and some don't.
3. When behavior changes, update the doc that describes it in the same change: rules in
   `docs/game-rules.md`, keys in `docs/controls.md`, structure in `docs/architecture.md`, and
   the nearest `AGENTS.md` if an instruction there became wrong.
4. Text files are LF everywhere, Windows checkouts included: `.gitattributes` sets
   `eol=lf`, so `sed`/`perl` substitutions behave as on Linux. Save new files with LF endings
   (Git converts CRLF on commit anyway). A checkout made before the rule came in may still
   have CRLF files; `git rm -r --cached . -q` then `git reset --hard` rewrites them as LF.
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
| `src/cli.rs`, `src/screenshot.rs` | command-line flags; screenshot mode | `src/AGENTS.md` |
| `src/net/` | a network game's link: TCP, encrypted and authenticated by the join code (`secure.rs`); everything arriving is untrusted | `docs/multiplayer.md` |
| `src/renderer/` | general 2D Vulkan renderer; knows nothing about the game | `src/renderer/AGENTS.md` |
| `src/game/` | all game state, rules, AI, drawing and UI | `src/game/AGENTS.md` |
| `src/game/ui/` | screen-space UI, both presentations (ImGui and classic) | `src/game/ui/AGENTS.md` |
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
- Anything visual: take a screenshot and look at it (an agent can open the PNG with its
  image-reading tool): `cargo run -- --screenshot out.png --scenario <combat|cities|frontier|world>`
  renders the scenario in a hidden window (no input, no focus), waits about a second for it to
  settle, writes one frame as a PNG and exits 0 (non-zero, with the reason, if it fails or takes
  over 20 s). `--seed N` fixes the world's map and `--size 1280x720` the window size (default
  1600x900); the same arguments give the same image, so before/after shots compare directly.
  Write screenshots outside the repo (or delete them). It's a debug build, so check its log for
  validation errors too. It shows the scenario's opening position only; for anything past that,
  run the game. UI layout and click targets are testable without a window (see
  `src/game/ui/tests.rs`).
- In-game testing aids: F1-F4 restart a scenario (F4 generates a new map), F6/F7 save and load a
  snapshot, F8 toggles instant turn playback, F9 finishes the current build, F10 toggles fog of
  war (`docs/controls.md`).

## Work board

Work is tracked on https://github.com/users/dampfhub/projects/1 through
`node tools/board/board.mjs` (no arguments prints usage; the `board` skill has the workflow).
Pick unblocked work by Priority (`list --open --priority P0`, then P1). Claim an item before
starting, keep its `Files` current, and run `fences` before working in parallel with another
agent. Check `list --open` before filing something new.

## Commits and PRs

- One PR per change, using the PR template: what changed, how it was verified, what it closes.
- After creating a PR or pushing any update, start monitoring its CI immediately
  (`gh pr checks <number> --watch --interval 10`, or equivalent live checks). Keep
  monitoring while doing other work, and inspect failures as they appear. Fix their
  cause, rerun the relevant local checks, push, and watch again until every required
  check succeeds for the latest pushed commit. Confirm the PR head SHA still matches
  the commit checked before reporting completion; a passing older run is not evidence.
  Do not leave a failing or pending PR for the user to discover. If an external blocker
  prevents success, report the failing check, evidence, and blocker explicitly. An
  intentionally skipped draft-only merge job is expected; do not mark a draft ready
  just to run it. If the PR merges, also watch the resulting CI on `main` through success
  before marking its board issue Done. Track unrelated failures on the board and resolve
  or explicitly report them; do not retry repeatedly just to get a green run.
- A PR merges itself: CI's `merge` job merges it once `rust`, `msrv` and `tools` pass, deletes
  its branch, then runs CI on `main`. To keep a PR open (for review, or while still working), open it as a draft
  (`gh pr create --draft`, or `gh pr ready --undo <#>`); marking it ready runs CI and merges it.
- Put `Closes #N` on one line: the merge job closes the issues a merged PR links that way. A line ending in `fix`/`close`/`resolve` (any tense) followed by a
  line starting `#N` also closes #N; `node tools/commit-msg-lint.mjs` catches that in commit
  messages, and CI runs it on every PR's commits.
