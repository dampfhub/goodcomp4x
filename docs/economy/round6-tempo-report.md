# The economy's tempo (#239)

> **Saved record.** This is the Round 6 report as the user read it on 2026-09-28, kept so that
> another option can be tried later. It describes the rules and numbers of `main` at 6f5c4c8.
> The user then chose option B, which is now in the game: see Round 7 in
> [rts-economy.md](../rts-economy.md#round-7-option-b-an-early-game).
> Beside it: [round6-how-it-works.md](round6-how-it-works.md) (the rules, from the code) and
> [round6-knobs.md](round6-knobs.md) (every knob, in every setting). The raw runs and chart data
> were not kept; the commands below reproduce them on a checkout of 6f5c4c8.

This report asks whether the game moves too fast, and where. I simulated AI-vs-AI games on `main` (6f5c4c8; the numbers are unchanged at 134305e) for 60 turns over 24 seeds (0-23), in two settings:
- the Cities scenario (F2), with 2 sides;
- the World (F4) with 1, 4, 5 and 6 AI sides.

Every side is played by the AI, the player's too, so `worldN` has N+1 sides. "Per side" means the mean over every side of every game.

This is an analysis for you to decide on. **Nothing in the game was retuned.** Each knob in section 4 was a local change, measured and then reverted.

A published page with charts: https://claude.ai/artifact/YCZyw9aV2JxJcTmA4hDN2Q (private to the owner).

Reproduce with:
```
SIM_SEEDS=24 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_ARMY_FIRST=1 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_TURNS=100 REPORT_GAMES=cities,world cargo test --release economy_report -- --ignored --nocapture
```
The other knobs are `REPORT_GAMES` (cities, world, world1-world6), `REPORT_JSON=<path>`, `REPORT_GAME_LINES=1`, `SIM_SPEEDUP=1` and `SIM_LIFETIME_CAP=1`.

## Summary

- **Yes, it is fast, and the speed is front-loaded.** In the World (4-6 AI sides), per side:
  - a Barracks is built by turn 5 and the first troop comes at turn 7;
  - the army reaches 5 by turn 17;
  - troops first fight on turn 10;
  - the city is full at 7 citizens by turn 27.
- **Turn 20 already has armies marching,** about 5-6 troops per side.
- **After about turn 30 the economy has nothing to do.**
  - Population is capped, and there is no Settler build.
  - Each city has one Barracks, which trains 98% of the time.
  - Stockpiles grow without end: about 100 of each resource by turn 40, and 600 food by turn 100.
  - The army holds at about 7 alive, because training (about 0.4 a turn) matches losses (0.3 a turn).
- **Queue time sets the pace, not prices.**
- **Growth, food and the army are separate tracks.** Each knob moves only its own track.
- **A supply limit caps the army wherever it is set.**

## 1. How the economy works now

The full text is in [round6-how-it-works.md](round6-how-it-works.md), written from the code with the constants named. The essentials:

- **The stockpile.** Each side has one stockpile, starting at 10 food, 10 wood and 4 metal (`STARTING_STOCK`). The economy runs once a turn, after the units and workers move: income comes in, citizens eat, queues pay for and work their items, and finished builds complete.
- **Income.**
  - The city center yields 2 food and 1 wood.
  - Each citizen, up to `MAX_CITY_POPULATION` = 7, works a tile. The first is the manager; the others work tiles next to it.
  - Terrain yields (food / production): grassland 3/0, plains 2/1, hills +1 production, forest −1 food and +1 production, fresh water +1 food.
  - Production splits into metal (hills +1, mine +2, Quarry +3) and wood (the rest).
  - Delivery shares by route cost (`delivered_share`): 4/4 at cost 2 or less (one hex), 3/4 at 3-4, 2/4 at 5-6, 1/4 at 7-8. A road step costs half a hex.
  - Mill: the tiles next to it deliver all their food. Cannery and Smelter: each collects from 3 remote sites.
  - Gather: free, 1 turn, gives 2 food, 2 wood and 1 metal.
- **Upkeep.** Each citizen eats 2 food a turn (`FOOD_PER_CITIZEN`). When food runs out, the largest city starves a citizen. Troops cost nothing to keep.
- **Paying and time.** An item is paid when work on it starts. `WORK_PER_TURN` means a build of n turns takes exactly n turns. Each city has two queues, its own and its one Barracks'. A city center trains troops at half speed (`CITY_TRAINING_SLOWDOWN` = 2).
- **Prices (food/wood/metal, turns at a Barracks).**
  - Troops: Melee 2/6/0, 2; Ranged 2/7/0, 2; Cavalry 3/4/3, 3; Siege 1/8/4, 3; Armored 3/2/7, 3.
  - Other queue builds: Worker 4/2/0, 2; Grow 5 + 5 × pop food, 2.
  - Buildings: Barracks 0/10/0, 3. The other buildings and worker jobs are as in [game-rules.md](../game-rules.md); the tables are in [round6-how-it-works.md](round6-how-it-works.md).
  - Workers walk 1 hex a turn to a job within 3 hexes.
- **Growth.** 1 → 7 costs 135 food in all. There is **no Settler build**, so a second city comes only by capture.
- **Military limits.** Cavalry and Armored are capped at 3 per Horses or Iron deposit (`UNITS_PER_DEPOSIT`). Nothing else limits the army.
- **The AI.**
  - City queue: a Worker if it has none; a Melee while it has no Barracks and fewer than 2 troops per city; else Grow; else Gather.
  - Barracks: always trains what it can pay for (Cavalry or Armored within the cap, else Melee or Ranged).
  - Workers: build one Barracks (placed on turn 1), then improve and road the worked tiles.
  - It builds no other building.

## 2. Measurements

### World, 4-6 AI sides, per side (mean)

| | turn 10 | turn 20 | turn 30 | turn 40 | turn 60 | turn 100 |
|---|---|---|---|---|---|---|
| Population (one city) | 3.0 | 5.4 | 6.7 | 7.0 | 7.0 | 7.0 |
| Army alive / trained | 2.1 / 2.0 | 5.4 / 5.9 | 6.7 / 10.1 | 6.7 / 14.2 | 7.1 / 22.3 | 8.1 / 38.6 |
| The same, army first | 2.2 / 2.0 | 6.4 / 6.9 | 8.7 / 12.5 | 9.8 / 18.5 | 11.0 / 31.0 | |
| Stockpile f/w/m | 13/7/7 | 18/28/25 | 37/62/59 | 92/107/96 | 242/205/178 | 598/425/349 |
| The same, army first | 12/5/6 | 18/8/21 | 29/17/47 | 59/31/75 | 165/64/141 | |
| Income f/w/m a turn | 10.9/2.6/1.0 | 16.1/4.4/2.6 | 18.3/5.1/3.4 | 19.7/5.3/3.8 | 21.3/5.6/4.0 | |
| Food the citizens eat a turn | 6.0 | 10.8 | 13.3 | 13.9 | 14.0 | |

Units gained by type, per side:

| | Melee | Ranged | Cavalry | Armored | Ruin recruits |
|---|---|---|---|---|---|
| Turn 20 | 2.2 | 1.0 | 1.5 | 1.2 | 0.2 |
| Turn 60 | 8.7 | 4.1 | 6.3 | 3.3 | 0.3 |

The AI never builds Siege or ships.

`world4`, `world5` and `world6` agree within a turn or two. `world1` (a duel on a World map) matches in economy, but its armies meet later and grow larger: 9 alive at turn 60.

### Events: median turn [10th-90th percentile]

| Event | World, 4-6 AI | World, 1 AI | Cities |
|---|---|---|---|
| Barracks placed / built | 1 / 5 [5-6] | 1 / 5 | 1 / 5 (never for the side that falls) |
| First troop | 7 [6-7] | 7 | 8 |
| First city at pop 3 / 4 / 5 / 6 / 7 | 9 / 11 / 16 / 21 / 27 [21-36] | 9 / 11 / 14 / 18 / 24 | 3 / 6 / 10 / 15 / 20 |
| Army of 3 / 5 / 10 | 11 / 17 / 31 (54% never reach 10) | 12 / 17 / 31 (46% never) | 1 / 11 / 26 |
| The same, army first: army of 5 / 10 | 15 / 27 (24% never) | 16 / 27 (10% never) | 10 / 19 |
| First contact (any unit within 3 hexes) | 4 | 6 | 1 |
| First fight (scouts included) | 5 | 7 | 2 |
| First fight between troops | 10 [7-11] | 21 [13-25] | 2 |
| Deaths a turn per side, from then on | 0.31 (army first 0.41) | 0.35 | 0.07 |
| Second city | only by capture: 7% of sides by turn 60 | never | the winner, turn 18 [14-20] |
| Eliminated | 5% of sides by turn 60 | never | 50% of sides (one per game), turn 18 |

### Where the turns go

**City queue (share of its turns).** It gathers most of the time: 52% at the population cap, where Grow is impossible, and 20% below it. The rest is Grow 20%, Melee 7% and Worker 1%. Nothing waits for its price, because the AI only queues what it can pay for. In army first: Melee 71%, Grow 20%, Gather 8%.

**Barracks.** It trains 98% of its turns. It is idle 2%; when idle, wood is short 89% of the time.

**Build times in practice:**
- Queue builds take exactly their listed turns, with no wait and no spawn delay. That holds in the World; the Cities exception is under "Cities" below.
- Worker jobs take longer than listed because the worker walks out first:

| Job | Turns in practice | Listed |
|---|---|---|
| Barracks | 4.5 | 3 |
| Improve | 3.6 | 3 |
| Road | 2.4 | 2 |

**Spending by turn 60, per side (f/w/m):**
- troops 56/115/43;
- growth 135/0/0, all of it done by turn 40;
- Barracks 0/10/0 and works 0/31/0.

Gather brought in 86/86/43 over the same turns, as much wood as three quarters of all the troop spending.

### Spread between sides (World)

- The top side's troops trained are 1.13-1.17 times the mean.
- Population is within 1.25 times the mean.
- Army alive spreads up to 2 times the mean by turn 60, by who fights whom.

There is no economic runaway. Runaways come from captures: 10 captures in 24 World games.

### Cities

- Both sides start with 4 troops, which fight on turn 2.
- A city falls in every game, at turn 18 [14-20].
- After that the winner's Barracks holds a finished unit for want of an open hex 60% of its turns: the AI has no target, so its units cluster around the Barracks.
- Cities numbers after turn 20 describe a lone winner. Treat Cities as a combat scenario.

### Variants

- `SIM_SPEEDUP=1` (production speeds builds): 24.8 troops trained by turn 60 against 22.3. It matters little now, because the AI's manager is rarely beside the Barracks.
- `SIM_LIFETIME_CAP=1`: 25.6 trained. Once a side has trained its 3 Cavalry and 3 Armored, its Barracks switches to Melee, which take 2 turns instead of 3.

## 3. Tempo: what a turn is worth

Per side in the World (4-6 AI):

| Turn | Pop | Income f/w/m | Citizens eat | Net food | Wood income in Melees a turn |
|---|---|---|---|---|---|
| 1 | 1 | 5.4/1.3/0.5 | 2 | 3.4 | 0.22 |
| 5 | 2 | 8.4/2.0/0.7 | 4 | 4.4 | 0.33 |
| 10 | 3 | 10.9/2.6/1.0 | 6 | 4.9 | 0.43 |
| 20 | 5.4 | 16.1/4.4/2.6 | 10.8 | 5.3 | 0.73 |
| 40 | 7 | 19.7/5.3/3.8 | 13.9 | 5.8 | 0.88, plus Gather's 0.33 |
| 60 | 7 | 21.3/5.6/4.0 | 14 | 7.3 | 0.93, plus Gather's 0.33 |

**Queue capacity per city:**
- a Barracks turns out 0.5 Melee a turn, or 0.33 Cavalry or Armored;
- the city queue turns out 0.25 Melee a turn;
- together, at most about 0.75 troops a turn.

**Before about turn 15, wood is the limit.** The Barracks eats the starting 10 wood, and a Melee costs about 3 turns of wood income.

**From about turn 20, queue time is the limit.** Income exceeds what the queues can spend. The AI then banks the surplus, and even a side spending everything on troops has wood left over.

**Growth.** Filling a city costs 135 food, about 27 turns of the early net food surplus, which is why cities fill at turns 24-29.

**Turns to a real army** (10 troops alive):
- the AI as it plays: turn 31, for the 46% of sides that ever get there;
- army first: turn 27.

**Turns to a second city:** never, except by capture. That structural fact is what ends growth around turn 30.

**Reference points to pick from:**
- **(A) Today, a skirmish.**
  - First troop at turn 7; army of 5 at turn 17; troops fighting at turn 10 (4-6 AI).
  - Full city at turn 27, then a static economy.
- **(B) An early game.**
  - At turn 20: 2-3 troops and scouting.
  - First troop around turn 9; army of 5 around turns 25-30; full city around turn 45.
  - The measured "combo-slow-all" lands here: army 3.4 at turn 20, first troop at turn 9, army of 5 at turn 23, full city at turn 43. It is troop prices x1.5, troop turns +1, Grow x2 and Gather halved.
- **(C) Civ-like.**
  - At turn 20: 1-2 units.
  - A real army after turn 40-50, and growth over 60 or more turns.
  - This needs about "troop turns x2" (army 2.9 at turn 20, first troop at turn 11), plus a Barracks at 20 wood and 5 turns, plus a squared Grow price. Those were measured one at a time, not together.

## 4. Options, not changes

The knobs were measured in the World (4-6 AI) over 24 seeds, then reverted. For each run the table gives:
- **AI:** the army at turns 20 / 60, troops trained by turn 60, population at turn 20, the turn the city is full at 7 (with the share that never is), and the stockpile at turn 40;
- **army first:** the army at turns 20 / 60, and troops trained by turn 60.

[round6-knobs.md](round6-knobs.md) has the Cities and 1v1 World tables too.

| Knob | AI: army 20 / 60 | trained 60 | pop 20 | pop 7 (never) | stock 40 f/w/m | army first: army 20 / 60 | trained 60 |
|---|---|---|---|---|---|---|---|
| none | 5.4 / 7.1 | 22.3 | 5.4 | 27 (1%) | 92/107/96 | 6.4 / 11.0 | 31.0 |
| troop prices x1.5 | 5.0 / 6.8 | 21.8 | 5.4 | 28 (3%) | 89/75/86 | 5.2 / 10.0 | 26.9 |
| troop prices x2 | 4.3 / 6.6 | 20.7 | 5.4 | 29 (2%) | 84/53/79 | 4.4 / 8.3 | 23.5 |
| troop turns +1 | 3.9 / 4.3 | 15.5 | 5.4 | 28 (2%) | 96/124/95 | 4.3 / 7.2 | 21.8 |
| troop turns x2 | 2.9 / 2.9 | 10.5 | 5.2 | 28 (1%) | 95/131/95 | 2.9 / 5.0 | 15.2 |
| Barracks 20 wood, 5 turns | 1.8 / 6.5 | 15.9 | 5.5 | 28 (2%) | 99/108/110 | 2.4 / 10.9 | 23.8 |
| Grow x2 (10 + 10 × pop) | 5.5 / 6.6 | 22.4 | 3.6 | 43 (8%) | 45/85/92 | 6.0 / 10.4 | 29.5 |
| Grow 5 × pop² | 5.4 / 6.9 | 22.3 | 4.0 | 55 (51%) | 65/95/95 | 6.5 / 10.6 | 30.3 |
| Grow takes 4 turns | 5.0 / 6.7 | 21.7 | 4.6 | 31 (3%) | 81/78/82 | 5.3 / 10.8 | 28.7 |
| citizens eat 3 food | 5.4 / 6.6 | 22.0 | 4.1 | 41 (39%) | 20/95/95 | 6.2 / 10.3 | 29.5 |
| Gather 1/1/0 | 5.4 / 7.1 | 22.9 | 5.4 | 29 (2%) | 80/80/79 | 6.2 / 10.9 | 30.5 |
| Gather gives nothing | 5.2 / 6.9 | 22.0 | 5.2 | 31 (3%) | 60/56/76 | 5.9 / 10.9 | 29.4 |
| delivery shares 4, 2, 1, 0 quarters | 5.5 / 7.0 | 22.3 | 5.1 | 31 (3%) | 58/98/95 | 6.5 / 10.6 | 30.6 |
| troops eat 0.5 food | 5.3 / 6.7 | 21.9 | 5.0 | 33 (14%) | 48/105/96 | 6.3 / 9.5 | 28.5 |
| troops eat 1 food | 5.2 / 5.6 | 19.8 | 4.3 | 37 (43%) | 28/99/92 | 6.1 / 6.5 | 21.9 |
| supply: 4 per city (#237) | 3.7 / 3.3 | 13.5 | 5.5 | 26 (1%) | 102/142/99 | 3.7 / 3.3 | 13.1 |
| supply: 1 per citizen (#237) | 4.3 / 4.9 | 18.1 | 5.4 | 27 (2%) | 97/126/97 | 4.0 / 5.9 | 19.4 |
| troop prices x1.5, turns +1 | 3.8 / 4.2 | 15.2 | 5.5 | 27 (3%) | 97/106/89 | 4.1 / 6.6 | 20.3 |
| the above, Grow x2, Gather halved | 3.4 / 4.6 | 14.9 | 3.5 | 43 (20%) | 41/58/64 | 3.6 / 6.7 | 18.7 |
| troops eat 1 food, supply 1 per citizen | 3.8 / 4.7 | 16.7 | 4.6 | 36 (18%) | 34/125/97 | 3.5 / 5.4 | 16.8 |

### What each knob does

- **Troop prices** (`BuildUnit::price`, `city/builds.rs`).
  - For the AI they barely matter (−7% at x2), because queue time binds and not the stockpile.
  - They bite a side spending everything on troops (−25% at x2), because wood then becomes its limit.
  - Use them to make armies a real trade-off for a player. They won't slow the AI.
- **Troop turns** (`BuildUnit::turns`, `CITY_TRAINING_SLOWDOWN`) are the most direct lever on the army: +1 turn is −30%, and x2 is −53%, for the AI and a player alike.
  - The cost: resources pile up even more, with nothing to spend them on.
- **Barracks price and turns** (`Building::price` and `turns`) move only the opening: army 1.8 instead of 5.4 at turn 20, caught up by turn 60.
  - With the Barracks unaffordable at once, the AI's city trains a Melee first, so the first troop comes at turn 4.
- **Grow price and turns** (`GROW_BASE`, `GROW_PER_CITIZEN`, `Build::turns`) set when cities fill:
  - Grow x2: turn 43;
  - a squared price: turn 55, with half the cities never full;
  - Grow takes 4 turns: turn 31.

  They don't touch the army, since troops and growth use different queues.
- **Citizen food and delivery shares** (`FOOD_PER_CITIZEN`, `delivered_share`) shrink the food surplus, and so growth. Citizens eating 3 food leaves 39% of cities short of 7. The army is untouched.
- **Gather** (`GATHER_YIELD`) only trims the pile, by 25-50 wood at turn 40, with no change to the tempo. It is the main idle income once a city is full.
- **Troop food upkeep** (new code in `upkeep`) starves cities before it trims armies, because the AI doesn't plan for it. At 1 food a troop, 43% of cities never reach 7; army first ends at 6.5 instead of 11.
  - It works as a limit only if the AI learns to budget for it.
- **A supply limit (#237)** caps armies cleanly where it is set: 4 per city gives about 3.3 alive, and 1 per citizen gives about 5.
  - It needs a sink beside it, because the stockpile grows even faster.
  - It was measured with each troop weighing 1 and scouts free. A troop past the limit waits, unpaid, in `pick_item`.
- **Structural options** (not measured: each needs new rules):
  - a Settler build, and expansion;
  - more production queues (a second Barracks, or production speeding builds);
  - no Gather at the population cap.

  These decide whether the game has anything to do after turn 30. Every slowdown knob above makes the late-game pile bigger.

## 5. Questions for you

1. **Target tempo: what should turn 20 look like?**
   - (A) Today: 5-6 troops per side, fighting from turn 10, full cities by turn 27.
   - (B) An early game: 2-3 troops and scouting, an army of 5 around turns 25-30, full cities around turn 45. "combo-slow-all" comes close.
   - (C) Civ-like: 1-2 units, a real army after turns 40-50, growth over 60+ turns.
2. **Slow troops by time or by cost?** Troop turns slow everyone; prices only bite a side that spends everything. Or both, as in "troop prices x1.5, turns +1": army 3.8 at turn 20, 4.2 at turn 60.
3. **Delay the first Barracks?** At 20 wood and 5 turns, the army at turn 20 drops from 5.4 to 1.8.
4. **Growth:** twice as long (Grow x2, full at turn 43), or a real investment (5 × pop², where half the cities stay under 7)?
5. **Supply (#237):** per city, per citizen, and with or without food upkeep? Upkeep needs the AI to budget for it.
6. **The late game:** should it get something to spend on (Settlers or expansion, more queues, no Gather at the cap), since every slowdown enlarges the pile?
7. **The Cities scenario:** keep its 4 starting troops per side? They decide the game by turn 18, before the economy matters.

## Answers so far (2026-09-28)

- Q1: **(B)**, tried as an experiment (the "combo-slow-all" knobs).
- Q6: Settlers (#249), costing a citizen, and a population cap of 28 with 4 managers (#250).
