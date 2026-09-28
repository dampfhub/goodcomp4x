---
name: board
description: Use when filing, claiming, updating or querying work items (tasks, bugs, follow-ups, open questions) on the project's GitHub Projects board, when triaging issues, or before starting parallel agents, to check which items share a file footprint. Wraps GitHub Projects v2 via `board.mjs`.
---

# The work board

One kanban board holds every unit of work: the GitHub Project named in
`tools/board/config.json` (for this repo, https://github.com/users/dampfhub/projects/1).
Drive it with the wrapper, never raw `gh`: `node tools/board/board.mjs <command>`
(no arguments prints usage). It finds its config relative to its own location, so it works from any
cwd and from a worktree.

Everything project-specific lives in the config: `repo`, `projectOwner` (+ `projectOwnerType`
`user` or `organization`), `projectNumber`, `defaultBranch`, the `selectFields` and `textFields`,
which fields `file` requires (`requiredOnFile`), the `list` columns, and the labels. Each field
becomes a flag: lowercase, spaces as dashes (`Area` → `--area`). `board.mjs setup` compares the
live project and repo against the config; `setup --apply` links the repo, creates missing fields
and labels, and adds missing options to an existing select, keeping every existing option's id so
no item loses its value. It never deletes or renames anything; what it can't fix it reports as
MANUAL.

## Before filing: triage

A duplicate splits one finding's history in two. `list --open` (never piped through `head`: it
truncates the newest items, the likeliest duplicates), then `show` the likely matches. Cross-reference
by file footprint, then by acceptance. Covered → update that item (`set`, or `note` with the new
evidence). New → `file` with the schema below. **One item is at most one agent's work**: split on
different acceptance observables, different areas, or an order that must be coordinated (a native
dependency, not prose).

## Working an item

1. `claim <#>` before reading code. It sets In Progress, which is what makes `fences` see you.
   Once your branch exists, `set <#> --branch <name>`. `show <#>` / `deps <#>` carry
   `Acceptance`, `Files`, and what blocks it. Pick work by `list --open --priority P0`, then P1.
2. `fences` before your first edit and again after any board change. The conflict unit is the file.
   Record your neighbours' `Files` when you claim; the value of a later re-read is the diff.
3. Keep `Files` equal to `git diff --name-only <default-branch>...HEAD` (three dots)
   (`set <#> --files "a; b"`). Every write replaces the whole field; `set` prints what was there and
   flags any path that dropped out.
4. **Fix small bugs you meet in your footprint now**, on your branch, with a test. File an item only
   for what you cannot fix in this change. Work that belongs to someone else: file it yourself, now,
   not as a sentence in a report nobody routes.
5. `note <#> --body-file <f>` comments without touching Status. Always `--body-file` for anything
   with backticks or code: a shell mangles them in `--body`.
6. Blocked → `block <#> --by <#N>`, never prose. Hierarchy → `sub <parent#> --add <#N>`, used
   narrowly (a piece of work no single item closes; an item too big mid-flight), never decomposed in
   advance.
7. Never move your own item to Done. Done means merged into the default branch and verified.

## Merging

- Put `Closes #N` in the PR description or merge commit, **on one line**. GitHub reads a newline as
  whitespace, so a line ending in `fix`/`close`/`resolve` (any tense) followed by a line starting
  with `#N` also closes #N, invisibly. `node tools/commit-msg-lint.mjs` refuses that shape in commit
  messages (default range `origin/<default-branch>..HEAD`; `--file <body> --declared 1` asserts a
  merge body closes exactly one item). It does not read PR descriptions: check those by eye.
- A non-draft PR merges itself once CI passes (`AGENTS.md`, Commits and PRs), so evidence for
  `done` comes from the CI run on `main` that follows.
- After merging and running the tests: `done <#> --body-file <evidence>` (commit, test output, what
  you checked by hand). If a PR's `Closes #N` already closed the issue, `done` still records the
  evidence and sets Status.
- Retroactively, for an already-pushed commit: `link <#> --commit <sha>`. Never rewrite pushed
  history to add a trailer.
- A duplicate or obsolete item: `drop <#> --of <#N>` or `drop <#> --reason "…"`. It closes as
  **not planned**, never `done`.
- Sweep what the change left behind onto the board: every deferred finding, every follow-up.

## Schema

Type is a **label**, from the config's `labels`: `enhancement` (planned work), `bug`, `follow-up`
(found while doing another item), `question` (an open design or rules question, like those listed
at the end of `docs/game-rules.md`), `documentation`, `refactor` (behavior-preserving
restructuring such as a file split: no rules, controls or `docs/rules` change, and the test count
is unchanged, so AGENTS.md rule 3's doc updates don't apply beyond paths that moved).

- `Status`: Todo · In Progress · Done. No "blocked" or "in review": blocking is a native dependency,
  and work under review is still In Progress.
- `Acceptance` (required by `file`): the one observable that decides done. Never restated smaller;
  if it is wrong, say so on the item and change it deliberately.
- `Files`: `;`-separated paths, required if the item is ready to start. Name files, not globs:
  `fences` compares paths by prefix, so a glob matches nothing and hides every clash beneath it. A
  `(note)` after a path is ignored. A pure tracking parent says `(tracking only)`.
- `Priority` (required by `file`): what to pick up next. `list --priority P1` filters by it.
  - **P0**: main is broken, a crash or data loss players can hit, or it blocks agents working
    on the board. Drop other work for it.
  - **P1**: next up: a rule, fog or UI bug players see, or tooling that speeds up every agent.
  - **P2**: backlog: cleanups, polish, measured-small performance, decisions nobody waits on.

  A blocked item takes the priority it will have once unblocked; `deps <#>` shows the blockers.
- `Area` (required by `file`): where the fix lands, by the table below. An item spanning areas
  takes the one holding most of its `Files`.
- `Difficulty` (required by `file`), in agent terms: **S** one file, one sitting; **M** a few
  files, one session; **L** one PR across a subsystem; **XL** too big for one agent: split it
  into sub-issues (`sub`) before anyone claims it.
- `Risk` (required by `file`): Low · Medium · High, how likely the change breaks something
  its tests don't cover.
- `Branch`: the branch doing the work, `set <#> --branch <name>` when you claim; it links a
  stale In Progress item to its code.

| Area | Where |
|---|---|
| COMBAT | `src/game/combat.rs`, `ability.rs`, `unit.rs`, `effects.rs`; naval combat and batteries |
| ORDERS | `src/game/orders.rs`, `order_queue.rs`, `group.rs`, `turn.rs` (planning and resolution) |
| CITY | `src/game/city/` (economy, builds, citizens, interior, founding, rail) |
| WORKERS | `src/game/workers.rs` (Worker units, jobs, roads, structures, passability) |
| AI | `src/game/ai.rs` |
| UI | `src/game/ui/`, `city/view.rs`, `camera.rs`, `settings.rs`, `scenario.rs` (F-key scenes, snapshots) |
| RENDER | `src/renderer/`, `shaders/`, `src/game/draw.rs`, `mesh.rs`, `font.rs`, `map_icons.rs`, `unit_icons.rs` |
| MAP | `src/game/hex.rs`, `terrain.rs`, `mapgen.rs`, `fog.rs`, `ruins.rs` |
| PLATFORM | `src/app.rs`, `main.rs`, `cli.rs`, `persist.rs`, `icon.rs`, `icon_art.rs`, `screenshot.rs`, `build.rs` (window, input, key map, CLI, saved files) |
| TOOLING | `tools/`, `.github/`, `.agents/`, `.claude/`, `Cargo.toml`, `src/game/simulation.rs` and test support |
| DOCS | `docs/`, `README.md`, the `AGENTS.md` files, when the change is only docs |

Body template (`--body-file`): what and why, with the observation that motivates it; how to check it
is done; what could regress; what is deliberately left out.

## Files, the fence

- It is capped at 1024 characters (`board.mjs` refuses longer). Past about a dozen paths, keep full
  paths for files that already existed (another item could hold those) and summarize files your
  branch created (nobody else can hold them).
- It names where the **fix** lands, not where the defect shows. "Add a check for X" touches the
  checker, not every X.
- It is a collision fence, not an index or an architecture claim.

## Comments are the record

A comment on an item outlives the branch that produced it, so write it self-contained: the
measurement, the command, its output and exit code inline, and which commit or branch it was taken
on. Never quote a count you expect as if you had measured it; label an expectation as one.

## Backlog triage

For each open item, one verdict, on evidence you cite: **obsolete** (the thing it describes is gone,
or later work already did it: `git log --grep`, grep the tree) → `drop --reason`; **duplicate** →
`drop --of`; **cheap and real** → fix it on a branch, one commit per item; **live** → leave it, and
`note` a correction if its title or Acceptance is now wrong. When unsure, leave it and say why.

## Gotchas

- `gh project item-add` caches its lookup and succeeds once per process; a loop silently drops every
  item after the first. The wrapper uses `gh api graphql` instead. Never truncate the output of a
  command that creates something: it can hide a silent failure the same way.
- `gh` may be missing from Git Bash's PATH on Windows; the wrapper finds it in the usual install
  locations. A 403 or "resource not accessible" means the token needs the `project` scope
  (`gh auth refresh -s project`, interactive: ask the user to run it).
- Board access is separate from repo access. A public project is readable by anyone, and write
  access to the linked repo grants nothing on it: "does not have the correct permissions to execute
  `CreateProjectV2Field`" (or any other project mutation) means the project owner must add you
  under the project's Settings > Manage access (Write for items and field values, Admin for
  fields). `setup` prints which you have.
- Project numbers are per owner. The wrapper matches both owner and number, and ignores items from
  other repos on a shared board.
- After the repo is renamed or transferred, `list` and `fences` see none of its issues (they filter
  by the config's `repo`) and print a warning naming the repos they did find; `setup` names the new
  name. Update `repo` in `tools/board/config.json`.

## Speed

`show` and `deps` fetch one issue directly. `list` and `fences` read a 10-minute snapshot at
`tools/board/cache.json` (gitignored), which every mutating command patches; `--fresh` bypasses
it and `cache` reports its age. `BOARD_TRACE=1` prints each GitHub call's duration. `selftest` runs
the offline fixtures.
