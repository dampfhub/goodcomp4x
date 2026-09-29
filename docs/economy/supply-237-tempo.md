# The supply limit's effect on tempo (#237)

The supply limit (`docs/game-rules.md`, Supply; `src/game/city/supply.rs`) caps a side's army at
what its cities give: `SUPPLY_PER_CITY` (3) a city plus `SUPPLY_PER_CITIZEN` (1) a citizen.
Every troop, ship and Scout uses 1. This note records what the limit does to the tempo that
Round 7 (`rts-economy.md`) measured, and why those values were chosen. The user expects the
formula to change, so the numbers here are a baseline for the next values to compare with.

## How it was measured

Measured on `main` at 5663dbc, with and without the limit, using the same games as Round 7
(60 turns, seeds 0-23):

```
SIM_SEEDS=24 REPORT_GAMES=cities,world1,world cargo test --release economy_report -- --ignored --nocapture
```

"Army" is troops alive, with scouts and settlers left out, as `economy_report` counts it. Turns
are medians. "Never" is the share of sides that don't get there by turn 60.

## World, 4 to 6 AI sides, per side

| | no limit | 3 a city + 1 a citizen (chosen) | 2 a city + 1 a citizen |
|---|---|---|---|
| First troop | 9 | 9 | 9 |
| Army at turn 10 / 20 | 1.2 / 3.5 | 1.2 / 3.4 | 1.2 / 3.1 |
| Army at turn 40 / 60 | 6.4 / 8.9 | 5.0 / 7.7 | 4.6 / 8.1 |
| Troops trained by turn 20 / 40 / 60 | 3.6 / 8.9 / 16.1 | 3.5 / 7.2 / 14.2 | 3.1 / 6.5 / 13.6 |
| Army of 5 | 24 | 25 | 29 |
| Army of 10 (never) | 44 (48%) | 52 (61%) | 54 (56%) |
| Population at turn 20 | 3.1 | 3.2 | 3.2 |
| Stockpile f / w / m at turn 40 | 23 / 26 / 65 | 25 / 42 / 64 | 23 / 47 / 64 |

## The other games

- **World with 1 AI side**, no limit → 3 + 1:
  - first troop 9 → 9;
  - army at turns 20 / 40 / 60: 3.5 / 7.5 / 10.0 → 3.5 / 5.6 / 9.4;
  - army of 10 at turn 44 → 54 (38% → 50% never).
- **Cities:** unchanged at 3 + 1. Each side starts with 4 troops at population 2 (supply 5),
  fights from turn 2, and a city falls by turn 20: the cap doesn't change what either side
  trains. At 2 + 1 the Cities sides start at their cap (4 of 4), and the game plays out
  differently: first troop at turn 10 instead of 12, army 12 instead of 7 at turn 60.

## What it shows

- **The early game is unchanged.** The first troop still comes at turn 9, and the army at
  turn 20 is 3.4 against 3.5. Round 7's target for option B was 2-3 troops at turn 20 and
  an army of 5 around turns 25-30; the limit keeps both.
- **The limit bites from about turn 30.** With a Barracks training all game, the AI's army
  grows faster than its population. The cap holds it back: the army at turn 40 falls from
  6.4 to 5.0, and 61% of sides never reach 10, against 48%.
- **Growth is untouched.** Population at turn 20 is the same, and cities fill within two
  turns of when they did (turn 51 against 49). Troops and growth use different queues, and at
  the cap the AI's city grows or gathers instead of training.
- **Wood banks a little instead.** Wood at turn 40 is 42 against 26, because a capped
  Barracks sits idle. Round 6 saw the same: a cap needs something else to spend on.

## Why 3 a city and 1 a citizen

- **3 a city** covers what the AI aims for, `AI_ARMY_PER_CITY` (2) troops a city, plus its
  Scout. So a new city is never capped out of the AI's own plan.
- **2 a city** caps too early. The Cities scenario's sides start at the cap, and the World's
  army at turn 20 drops to 3.1.
- **1 a citizen** ties the cap to growth, which is what the user asked for. A citizen per supply
  point keeps the numbers whole, and at turn 20 (population about 3) it gives a cap of about 6,
  against an army of 3.5 and a scout.
- **The cap is loose for big cities.** A full city of 28 would give 31. Up to turn 60 no side
  gets near that (8.5 citizens a side at turn 60), but a longer game may want a smaller amount
  per citizen, or a cap per city.
