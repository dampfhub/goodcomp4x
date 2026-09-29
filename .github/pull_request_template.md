## What changed

<!-- The behavior change, and why. -->

## How it was verified

<!-- Commands run and their results; for visual changes, the test or screenshot that shows it. -->

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` pass
- [ ] Docs updated where behavior changed (`docs/game-rules.md`, `docs/controls.md`, `AGENTS.md`)

- [ ] CI monitored after the latest push: all required checks pass for the current PR head
      (continue monitoring and fix failures after every update; a draft's skipped merge job is expected).
      If merged, verify the resulting `main` CI before marking the board issue Done.

## Board

<!-- One line, keyword and number together, e.g. "Closes #12". Leave empty if nothing closes. -->
