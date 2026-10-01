# The economy as it stands: Round 11 report (#239)

> **Saved record.** This report describes the rules and numbers of `main` at f4e720f, with
> tile yields capped (#368), cutting forests, delivery at 100% to 2 hexes and 75% to 4 (#367),
> cities gathering by themselves (#334), supply (#237), unit prices of two goods each (#374)
> and the Pikeman (#375). It is analysis only: **nothing in the game was retuned.** Each knob in
> section 8 was a local change, measured and then dropped. The rounds before it are in
> [rts-economy.md](../rts-economy.md), and the Round 6 report it follows in
> [round6-tempo-report.md](round6-tempo-report.md).

All games are AI against AI, every side the AI's, the player's included, over 24 seeds (0-23).
`worldN` is the World (F4) with N AI sides, so N+1 sides in all. "Per side" means the mean over
every side of every game. Turns of events are medians, with the share of sides that never got
there in brackets. World 5 and 6 are within about 0.3 of World 4 on every number below, unless a
table shows them.

Reproduce the base numbers with:
```
SIM_SEEDS=24 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_ARMY_FIRST=1 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_QUEUE_AHEAD=1 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_TURNS=100 REPORT_GAMES=cities,world1,world4 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_ANIMALS=0 cargo test --release economy_report -- --ignored --nocapture   (and =2)
SIM_SEEDS=24 REPORT_TURNS=60 cargo test --release animal_report -- --ignored --nocapture
```
Two measures came from a local addition that isn't in the tree: each side's third city, and its
start's land (farmland, dry open ground, forest and hills within 2 hexes) against how it did.
The knobs were local edits read from environment variables, so one build served every run.

## Summary

- **Tempo.** In the World with 4 AI sides, a side's first troop comes at turn 8. It has an army
  of 3.8 at turn 20, 5.0 at turn 40 and 6.1 at turn 60. Its population is 2.6, 3.7 and 5.4 at the
  same turns, and it founds a second city at turn 47 (43% never by turn 60, 11% never by 100),
  and a third at turn 68 (26% never by 100). At turn 100 it has 3.3 cities and 12.8 citizens.
- **What limits what:**
  - **Food limits growth, all game.** A citizen on an unimproved food tile (2 food) only feeds
    itself, so a city's surplus is its center's 2 food and its farms', and farms need fresh
    water. The food stockpile never rises past about 20. The city queue gathers 67% of its turns
    for want of the food for a Grow.
  - **Supply limits the army,** then metal. An AI Barracks is idle 40% of its turns: 74% of those
    at the supply cap, 26% short of the metal for a Melee or Ranged.
  - **Wood is in surplus** since #374: 21 at turn 20, 57 at turn 60 and 143 at turn 100.
  - **Metal** stays low until the army is capped, then piles up: 3 at turn 20, 19 at turn 60, 47 at
    turn 100.
- **What the AI builds**, per side by turn 60:
  - troops: 5.4 Melee, 3.4 Ranged, 1.0 Cavalry, 0.4 Pikemen and 0.3 Armored;
  - no Siege, no ships, and no building but one Barracks a city;
  - 1.3 Settlers, 1.7 Workers and 5 Grows.
  - Its workers build roads and improvements, and cut about half a forest per side.
- **Queueing ahead** like a player, with a Melee always queued at the city center, does worse
  than the AI: an army of 2.8 / 3.7 / 3.9 at turns 20 / 40 / 60. The city queue pays before the
  Barracks, so the scarce metal goes on Melee at the city center's half pace, and the Barracks
  sits idle 85% of its turns, 70% of those short of metal.
- **Animals** cost a side about 2 troops by turn 60, and the army at turn 60 is 6.2 without them
  against 6.1 with them; growth is the same. Strays do most of the late killing.
- **The bigger maps** are not what limits expansion: about 8 decent city sites a side, and 3.3
  cities by turn 100. Growth is the limit.
- **The four questions**, each answered in section 7:
  1. **Grow or upkeep?** Lower the upkeep, to 1.5 food a citizen. Then a citizen on a 2-food tile
     nets half a food, so growth no longer rests on the center and farms alone. Cities by turn
     100 go from 3.3 to 4.1. A cheaper Grow (10 + 5 a citizen) grows the capital as much but
     founds no more cities by turn 100.
  2. **A Pasture on dry open ground?** Yes, at +1 food. Second cities that never come by turn 60
     fall from 43% to 14%, and the AI's workers find twice the improvements to build.
  3. **Is the spread in farmland a problem?** It is a modest one. Sides with 2-3 farmable tiles
     reach population 4.9 at turn 60, against 7.7 with 8 or more, and 46% of them found no second
     city by turn 60, against 7%. Armies don't differ, as supply and metal cap them. A +1 Pasture
     nearly closes the gap: 6.5 against 8.1, and 14% against 7%.
  4. **Cut Forest (+10 wood, 2 turns, 2 food)?** The numbers are reasonable, and they don't
     matter much now. Wood is in surplus, so the AI cuts a spare forest only when short of wood,
     about half a time per side by turn 60. Halving or raising the wood, the turns or the food
     moves nothing measurable.
- **Recommendations** (section 8): a Pasture of +1 food on dry open ground, and upkeep of 1.5
  food a citizen, together. They give:
  - population 9.0 at turn 60 and 23 at turn 100;
  - 2.4 cities at turn 60 and 4.3 at turn 100, with a third city at turn 59;
  - an army of 7.4 at turn 60;
  - a first troop at turn 9 and the same army at turn 20.

  Keep Cut Forest, supply and the troop prices as they are.

## 1. How the economy works now

The rules are in [game-rules.md](../game-rules.md); the constants that matter here:

- **Tiles** give at most 2 goods unimproved (`UNIMPROVED_YIELD_CAP`).
  - Grassland and plains give 2 food, forest 2 wood, tundra and marsh 1 food and 1 wood, desert
    1 wood.
  - Hills give 1 metal and 1 of the tile's main good.
  - Fresh water adds a food only where the tile has room.
  - Special tiles add 1 past the cap.
- **The city center** gives 2 food and 1 wood, uncapped.
- **Improvements** (`workers.rs`, `improvement`) go past the cap:
  - a farm, +2 food, only with fresh water;
  - a mine, +2 metal, on hills;
  - a lumber mill, +1 wood, under forest or jungle.

  Dry open ground and snow take none.
- **Cut Forest**: 2 food, 2 turns of a worker, then +10 wood once (`CUT_FOREST_*`). The forest
  becomes its ground: plains with 2 food, where a farm can follow by fresh water.
- **Delivery** (`delivered_share`): all of a tile's goods within 2 hexes of route, 75% through 4
  hexes, none farther. A road step counts half a hex.
- **Upkeep**: 2 food a citizen a turn (`FOOD_PER_CITIZEN`). **Grow**: 10 + 10 × population food,
  2 turns (`GROW_BASE`, `GROW_PER_CITIZEN`). **Gather**: 1 food, 1 wood and half a metal, 1 turn;
  a city whose queue works nothing gathers by itself.
- **Supply** (`city/supply.rs`): 3 a city, 1 for each of its first 7 citizens, then 1 for every 2
  more. A city of 4 gives 7. Every troop, ship and Scout uses 1.
- **Prices** (food / wood / metal, turns at a Barracks; a city center takes twice as long):

  | Melee | Pikeman | Ranged | Cavalry | Siege | Armored | Scout | Worker | Settler |
  |---|---|---|---|---|---|---|---|---|
  | 5/0/2, 3 | 6/0/0, 3 | 0/9/2, 3 | 6/0/5, 4 | 0/12/6, 4 | 6/0/10, 4 | 2/4/0, 2 | 4/2/0, 2 | 30/10/0, 6, a citizen |

  A Barracks costs 10 wood. A side starts with 10 food, 10 wood and 4 metal.
- **Animals** (`animals.rs`): one den a side by default (wolves or bears), each keeping 2
  animals and adding one every 8 turns. At its cap a den sends the new one out as a stray, up to
  one stray a side. Killing an animal pays 3 food (wolves) or 5 (bears), and clearing a den
  pays 6 food and 2 metal.
- **Maps** (`mapgen.rs`, `world_shape`): about 1,200 hexes a side, room for about 8 decent city
  sites a side, 8 apart. Cities stand at least 6 apart.

## 2. Tempo

### World with 4 AI sides, per side (mean)

| Turn | Population | Cities | Army | Trained | Stockpile f / w / m | Income f / w / m | Upkeep |
|---|---|---|---|---|---|---|---|
| 1 | 1.0 | 1.0 | 0.0 | 0.0 | 13 / 2 / 5 | 4.0 / 1.0 / 0.0 | 2.0 |
| 10 | 2.0 | 1.0 | 1.0 | 1.0 | 10 / 11 / 3 | 5.9 / 1.0 / 0.0 | 4.0 |
| 20 | 2.6 | 1.0 | 3.8 | 3.7 | 14 / 21 / 3 | 8.3 / 1.1 / 0.1 | 5.3 |
| 30 | 3.5 | 1.0 | 4.8 | 5.3 | 21 / 30 / 6 | 10.3 / 1.3 / 0.3 | 6.9 |
| 40 | 3.7 | 1.0 | 5.0 | 6.5 | 18 / 38 / 11 | 11.1 / 1.3 / 0.5 | 7.4 |
| 50 | 4.6 | 1.4 | 5.0 | 8.0 | 15 / 46 / 15 | 13.8 / 1.9 / 0.6 | 9.1 |
| 60 | 5.4 | 1.7 | 6.1 | 10.6 | 20 / 57 / 19 | 16.6 / 2.2 / 0.5 | 10.8 |
| 80 | 8.7 | 2.5 | 9.6 | 18.2 | 19 / 90 / 28 | 25.9 / 3.4 / 1.0 | 17.4 |
| 100 | 12.8 | 3.3 | 14.8 | 30.0 | 19 / 143 / 47 | 36.3 / 4.8 / 1.8 | 25.5 |

The stockpile at turn 1 is after the Barracks (10 wood, placed on turn 1) and the first turn's
income. Income is what the cities deliver before the citizens eat; Gather comes on top of it.

### Events, by game

| | World 1 AI | World 4 | World 5 | World 6 | Cities |
|---|---|---|---|---|---|
| First troop | 8 | 8 | 8 | 8 | 9 |
| Army at turn 20 / 40 / 60 | 3.8 / 5.6 / 7.8 | 3.8 / 5.0 / 6.1 | 3.8 / 4.8 / 6.0 | 3.6 / 4.8 / 6.0 | 3.5 / 6.5 / 10.0 |
| Trained by turn 60 | 10.6 | 10.6 | 10.8 | 11.2 | 8.5 |
| Army of 5 / of 10 | 25 / 56 (71%) | 24 / 57 (83%) | | | 13 / 29 |
| Population at turn 20 / 40 / 60 | 2.7 / 4.1 / 6.8 | 2.6 / 3.7 / 5.4 | 2.5 / 3.8 / 5.5 | 2.6 / 3.8 / 6.1 | 3.5 / 5.5 / 7.5 |
| First city at population 3 / 4 / 5 | 17 / 28 / 41 (67%) | 19 / 29 / 45 (86%) | | | 8 / 13 / 53 |
| Second city (never by 60, by 100) | 45 (21%, 6%) | 47 (43%, 11%) | 47 (37%, 15%) | 45 (33%, 13%) | (a capture) |
| Third city (never by 100) | 63 (19%) | 68 (26%) | 66 (33%) | 67 (28%) | |
| Cities at turn 60 / 100 | 2.1 / 3.8 | 1.7 / 3.3 | 1.7 | 1.8 | 1.5 |
| First fight between troops (side) | 27 | 23 | | | 2 |

- **Against Round 10** (the "and cutting" column, before #374 and #375): the army at turns 20 /
  40 / 60 was 3.4 / 4.9 / 5.5 and is 3.8 / 5.0 / 6.1. Population was 2.4 / 3.7 / 5.0 and is
  2.6 / 3.7 / 5.4. The second city came at turn 51 (47% never) and comes at 47 (43%).
- **Cities is a combat scenario.** One side falls in every game, at turn 25; its numbers after
  that are the winner's. It is left out of the rest of this report but for the knob tables.
- **The army keeps growing to turn 100** (14.8), because it follows the population: supply is
  3 a city plus about 1 a citizen.

## 3. Which good limits what, and when

| Turns | What holds a side back |
|---|---|
| 1-8 | **Wood, then time.** The Barracks takes the starting 10 wood on turn 1, and the first troop waits for it to be built (turn 5) and trained (3 turns). Food for the first Grow (20) comes by turn 5. |
| 8-20 | **Metal**, for the troops: the stockpile holds 3 metal from turn 10 to 20, and each Melee or Ranged takes 2 of it. Gather is most of the metal (half a metal a gathering turn). |
| 20 on | **Supply**, for the army: 74% of an idle Barracks' turns are at the cap, and the rest (26%) short of metal. In World with 1 AI side it's 63% and 37%. |
| All game | **Food**, for growth: a Grow costs 10 + 10 × population food, a Settler 30. The food stockpile stays at 14-22. |
| Never | **Wood**: 57 at turn 60 and 143 at turn 100. |

- **What was spent by turn 60**, per side (food / wood / metal):
  - troops 41 / 34 / 28;
  - growth and Settlers 202 / 14 / 0;
  - workers 7 / 4 / 0;
  - buildings (Barracks) 0 / 17 / 0;
  - worker jobs 1 / 17 / 0.

  Gather brought in 46 / 46 / 23: half the metal the side had.
- **Food is the whole growth economy.** At turn 40 a side delivers 11.1 food and eats 7.4: a
  surplus of 3.7 a turn, while growing from 3 to 4 costs 40.
  - That is why the city queue gathers 67% of its turns: it can't pay for a Grow.
  - Growth does speed up once a side has a second city, from turn 47: population 5.4 at turn 60,
    8.7 at 80 and 12.8 at 100.
- **Metal holds the army back early, supply later.** From about turn 20 an army sits at the
  supply its cities give (3 a city and 1 a citizen, about 6-7 for one city of 3-4). Metal is
  short in between, and right after losses.
- **Wood has only small uses left** since a Melee costs none: Ranged, Settlers, Scouts, roads and
  improvements. The AI spends 34 on troops and 52 on everything else by turn 60, and banks the
  rest.

## 4. What the AI builds

Per side, World with 4 AI sides:

| | by turn 20 | by turn 40 | by turn 60 |
|---|---|---|---|
| Melee | 2.0 | 3.4 | 5.4 |
| Ranged | 1.1 | 2.2 | 3.4 |
| Cavalry (deposit) | 0.5 | 0.7 | 1.0 |
| Armored | 0.0 | 0.1 | 0.3 |
| Pikeman | 0.1 | 0.2 | 0.4 |
| Ruins' Cavalry | 0.2 | 0.3 | 0.3 |
| Siege, ships | 0 | 0 | 0 |

- **City queue, share of its turns:** Gather 67%, Grow 15%, Settler 12%, Worker 5%, Scout 1%.
  Nothing waits for its price, as the AI queues only what it can pay for this turn.
- **Builds finished by turn 60, per side:** 5.0 Grows, 1.3 Settlers, 1.7 Workers and 0.4 Scouts.
- **Barracks:** it trains 60% of its turns and is idle 40%.
- **Buildings:** a Barracks in each city, and no other. No Mill, Workshop, Stable, Forge,
  Smelter or Harbor is ever placed.
- **Worker jobs done by turn 60, per side:** 3.7 roads, 2.1 improvements, 1.5 Barracks and 0.5
  forests cut. In Round 10 a side cut 2.6 forests. It cuts a spare forest only while it has less
  wood than a Ranged costs, which now it rarely does.
- **Pikemen** come only once an enemy Cavalry is in sight, and Cavalry are few, so they stay rare.

## 5. Queueing ahead, and spending on troops first

`REPORT_QUEUE_AHEAD=1` plays every side's cities as a player planning ahead might: where the AI
would gather, the city queues a Melee (else a Grow) whatever the stockpile holds, and gathers by
itself while it waits. `REPORT_ARMY_FIRST=1` trains a Melee at the city center wherever the AI
would gather and can pay for one.

| World 4 AI | The AI as it plays | Army first | Queueing ahead |
|---|---|---|---|
| First troop | 8 | 6 | 6 |
| Army at turn 20 / 40 / 60 | 3.8 / 5.0 / 6.1 | 3.7 / 4.6 / 5.3 | 2.8 / 3.7 / 3.9 |
| Trained by turn 60 | 10.6 | 9.5 | 6.9 |
| Population at turn 20 / 40 / 60 | 2.6 / 3.7 / 5.4 | 2.3 / 3.6 / 5.2 | 2.1 / 3.5 / 5.0 |
| Second city (never) | 47 (43%) | 48 (40%) | 51 (50%) |
| Barracks idle (at supply / short of metal) | 40% (74% / 26%) | 59% (54% / 46%) | 85% (30% / 70%) |

- **Queueing ahead costs a third of the army,** because every city's queue pays before its
  Barracks' (`work_queues`).
  - A Melee waiting at the city center takes the metal as soon as it comes, for a troop that
    takes 6 turns, where the Barracks would take 3.
  - The Barracks then sits idle for metal.
  - The city works its Melee 44% of its turns and gathers only while the Melee waits (28%), so
    less metal comes in.
- **A player who queues ahead should queue troops at the Barracks** and growth at the city. The
  pay order doesn't tell them so. A small rule could, such as a Barracks paying before its own
  city's queue, or the city tray warning that a troop there is slower. That's for the user to
  decide; it isn't measured here.
- **Army first gets the first troop in at turn 6, and nothing else.** The army at turn 60 is
  smaller (5.3 against 6.1). The early Melee costs growth, and supply then follows the smaller
  population.

## 6. Animals, strays and the bigger maps

**Animals, through `economy_report`, World with 4 AI sides:**

| Animals setting | None | One den a side (default) | Two dens a side |
|---|---|---|---|
| Army at turn 20 / 40 / 60 | 4.0 / 5.3 / 6.2 | 3.8 / 5.0 / 6.1 | 3.7 / 4.7 / 5.2 |
| Population at turn 60 | 5.5 | 5.4 | 5.4 |
| Second city (never) | 47 (36%) | 47 (43%) | 47 (36%) |
| Animals killed (24 games) | 0 | 219 | 411 |

**Animals, through `animal_report`** (World with 4-6 AI sides, 60 turns, per world of about 6
sides):
- **On the map:** each den keeps its 2 animals, about 12 animals by turn 10. Strays come from
  turn 20, about 6 by turn 30, the limit.
- **What the sides lose to animals:**
  - turns 1-20: 0.5 troops, 0.3 scouts and 0.3 workers;
  - turns 21-60: 10.8 troops, 1.3 scouts and 3.0 workers. 57% of those troops died with a stray
    beside them.

  So a side loses about 1.9 troops to animals by turn 60, against 10.6 trained. With two dens a
  side it loses about 2.9.
- **What the sides win:** they kill 12.5 animals a world, about 2 a side, worth about 8 food.
  They clear 1.2 of the 6 dens by turn 60.
- **Effect on the economy:** animals barely touch it. The bounty is small, and losses come late,
  when supply caps the army anyway. Their real cost is the troops a side keeps home.

**The bigger maps** give a side room for about 8 decent city sites (Round 10's `map_stats`), and
a side has 1.7 cities at turn 60 and 3.3 at turn 100. The gap is growth, not room.
- A Settler needs a city of 3 and 30 food, and a city of 3 comes at turn 19. The AI trains a
  Settler from population 4, which comes at turn 29.
- A second city comes at turn 47, and a third at turn 68.
- The user's aim of 3-5 cities a side is met only at about turn 100, and only by three sides in
  four.

## 7. The four questions

### 1. Should the Grow price or the food upkeep drop?

The upkeep should, to 1.5 food a citizen.

| World 4 AI | Today | Grow 5 + 5 × pop | Grow 10 + 5 × pop | Grow 5 + 10 × pop | Upkeep 1.5 | Upkeep 1 |
|---|---|---|---|---|---|---|
| Population at turn 20 / 40 / 60 | 2.6 / 3.7 / 5.4 | 3.7 / 5.0 / 7.5 | 3.2 / 4.5 / 7.4 | 3.0 / 4.0 / 6.1 | 3.0 / 4.3 / 7.8 | 3.6 / 5.5 / 10.7 |
| First city at population 5 (never by 60) | 45 (86%) | 29 (8%) | 38 (51%) | 44 (75%) | 39 (57%) | 35 (31%) |
| Second city (never by 60) | 47 (43%) | 47 (36%) | 44 (22%) | 46 (30%) | 43 (11%) | 38 (7%) |
| Cities at turn 60 / 100 | 1.7 / 3.3 | 1.7 | 1.9 / 3.3 | 1.9 | 2.3 / 4.1 | 2.6 |
| Population at turn 100 | 12.8 | | 15.9 | | 19.9 | |
| Army at turn 20 / 60 | 3.8 / 6.1 | 3.8 / 6.4 | 3.8 / 6.6 | 3.8 / 6.2 | 3.9 / 7.5 | 3.9 / 8.6 |
| Stockpile at turn 60, f / w / m | 20 / 57 / 19 | 13 / 70 / 34 | 13 / 61 / 25 | 18 / 57 / 21 | 18 / 52 / 20 | 25 / 56 / 23 |

- **Grow and upkeep work differently.**
  - A cheaper Grow makes each step up cheaper. But a citizen on an unimproved 2-food tile still
    eats all it brings in, so a city's surplus is still only its center's and its farms'. The
    capital grows faster, and that's all: Grow 10 + 5 brings population at turn 60 to 7.4, but
    cities at turn 100 stay at 3.3.
  - At 1.5 food, every citizen on a 2-food tile adds half a food. Growth compounds, and that's
    what expansion needs. The first city grows a little less than with Grow 10 + 5, but sides
    found more cities: 2.3 at turn 60, 4.1 at turn 100, and only 11% with no second city by
    turn 60.
- **Upkeep 1 is too much.** Population 10.7 at turn 60 is twice today's, and the Barracks then
  waits for metal 71% of its idle turns, not supply.
- **Both together** (Grow 10 + 5 and upkeep 1.5) are too much as well: population 10.8 at turn
  60, a second city at turn 38.
- **The army follows the population through supply,** so cheaper growth brings a larger army.
  Upkeep 1.5 gives 7.5 at turn 60 against 6.1, the same 3.8-3.9 at turn 20, and a first troop
  at turn 9 against 8. Nothing early changes.

### 2. Should dry open ground get a Pasture?

Yes, at +1 food. Measured as an improvement like the others: Improve's price (4 wood) and
turns, on any open ground where a farm can't go.

| World 4 AI | Today | Pasture +1 food | Pasture +2 food |
|---|---|---|---|
| Population at turn 20 / 40 / 60 | 2.6 / 3.7 / 5.4 | 2.9 / 3.7 / 6.6 | 2.9 / 4.1 / 7.8 |
| Second city (never by 60) | 47 (43%) | 46 (14%) | 45 (13%) |
| Third city (never by 100) | 68 (26%) | 66 (19%) | |
| Cities at turn 60 / 100 | 1.7 / 3.3 | 2.1 / 3.8 | 2.2 |
| Army at turn 60 | 6.1 | 6.6 | 6.6 |
| Wood at turn 60 | 57 | 48 | 47 |
| Improvements built (24 games) | 249 | 520 | 586 |

- **Why it works:**
  - Dry open ground is the commonest land by a start: 5.4 tiles within 2 hexes, against 4.4
    farmable.
  - Today it's a dead end: a citizen there breaks even, and nothing can be built on it.
  - With a Pasture, a citizen there feeds itself and half of another.
  - Above all, it lets a **new** city grow. A new city's site rarely has much farmland, which is
    why second cities stall today.
- **It spends the surplus wood:** the AI's workers build twice the improvements.
- **Why +1 and not +2:** +2 makes every open tile a farm, which removes the reason to settle by
  rivers. +1 keeps a farm (4 food) worth twice a Pasture (3 food).

### 3. Is the farmland spread between starts a problem?

A modest one, for growth and expansion only, and a +1 Pasture nearly closes it.

Farmable tiles within 2 hexes of a start, over 480 sides (World 1, 4, 5 and 6): mean 4.4. 35%
of sides have 2-3, and 6% have 8 or more. Within one game the side with the most has 2.2 times
the side with the least on average, and up to 7 times.

| Farmland by the start | Sides | Population at turn 40 / 60 | Food income at turn 40 | Army at turn 60 | Cities at turn 60 | Second city (never by 60) | Third city (never by 100) |
|---|---|---|---|---|---|---|---|
| 2-3 | 170 | 3.7 / 4.9 | 10.5 | 6.0 | 1.6 | 48 (46%) | 69 (34%) |
| 4-5 | 187 | 3.8 / 5.9 | 11.8 | 6.0 | 1.8 | 45 (35%) | 66 (26%) |
| 6-7 | 93 | 3.9 / 6.6 | 13.0 | 6.3 | 1.9 | 45 (25%) | 64 (25%) |
| 8+ | 30 | 4.3 / 7.7 | 15.1 | 8.0 | 2.3 | 46 (7%) | 62 (13%) |
| 2-3, with a +1 Pasture | 170 | 3.7 / 6.5 | 12.3 | 6.5 | 2.1 | 47 (14%) | 66 (18%) |
| 8+, with a +1 Pasture | 30 | 4.3 / 8.1 | 15.7 | 8.0 | 2.4 | 45 (7%) | 61 (7%) |

- **The spread shows late.** Population at turn 40 differs by 0.6 between the poorest and the
  richest starts, and at turn 60 by 2.8. The correlation of farmland with population at turn 60
  is 0.27, and with food income at turn 40 0.31.
- **Armies don't differ** (6.0 against 6.3, but for the few richest), because supply and metal
  cap them.
- **Within one game**, the side with the most farmland against the side with the least:
  - population at turn 60 6.6 against 5.2;
  - no second city by turn 60 for 24% against 42%;
  - the same army.
- **With a +1 Pasture**, the poorest starts nearly catch up: population 6.5 against 8.1 at
  turn 60, and the share with no second city by turn 60 is even (14% against 7%). Dry open
  ground then helps rather than hurts: its correlation with population at turn 60 goes from
  -0.21 to -0.12.
- **The alternative,** evening out farmland in `mapgen.rs` (raising `START_FARMLAND` from 2,
  say), wasn't measured. It would cost the start placement some freedom, and a Pasture does the
  job in play.

### 4. Are the Cut Forest numbers reasonable (+10 wood, 2 turns, 2 food)?

Yes, and today they hardly matter.

| World 4 AI | Today | 6 wood | 15 wood | 1 turn | 3 turns | 0 food | 4 food |
|---|---|---|---|---|---|---|---|
| Army at turn 20 / 40 / 60 | 3.8 / 5.0 / 6.1 | 3.8 / 5.0 / 6.1 | 3.8 / 5.0 / 6.1 | 3.8 / 5.0 / 6.1 | 3.8 / 5.1 / 6.1 | 3.8 / 5.0 / 5.9 | 3.8 / 5.0 / 6.0 |
| Population at turn 60 | 5.4 | 5.4 | 5.4 | 5.5 | 5.4 | 5.4 | 5.4 |
| Wood at turn 60 | 57 | 56 | 59 | 57 | 56 | 59 | 58 |
| Forests cut (24 games) | 56 | 58 | 53 | 57 | 55 | 60 | 53 |

- **The AI cuts few forests now:** 56 in 24 games, against 318 in Round 10.
  - It cuts a forest it works where a farm could then go, or any forest while short of wood
    (less than a Ranged's 9).
  - Since a Melee needs no wood (#374), it is rarely short. No variant moves anything.
- **For a player, the numbers are fair:**
  - 10 wood is a Ranged and change, or 10 turns of a city's Gather, for 2 food and a worker's 3
    turns or so, walking included;
  - after that the tile is plains, with 2 food where it had 2 wood.
- **Its real worth is the land,** not the wood, while wood is in surplus.
- **Keep the numbers.** Look at them again if wood ever runs short, for instance if the AI
  starts building other buildings.

## 8. Knobs, measured

Each knob was one change, against the same 24 seeds. World with 4 AI sides; "never" is by turn
60.

| Knob | First troop | Army at 20 / 40 / 60 | Population at 20 / 40 / 60 | Cities at 60 | Second city (never) | Stockpile at 60, f / w / m | Barracks idle (supply / metal) |
|---|---|---|---|---|---|---|---|
| none (today) | 8 | 3.8 / 5.0 / 6.1 | 2.6 / 3.7 / 5.4 | 1.7 | 47 (43%) | 20 / 57 / 19 | 40% (74 / 26) |
| **A. Pasture +1 food** | 8 | 3.8 / 5.1 / 6.6 | 2.9 / 3.7 / 6.6 | 2.1 | 46 (14%) | 19 / 48 / 18 | 38% (64 / 36) |
| Pasture +2 food | 8 | 3.8 / 5.0 / 6.6 | 2.9 / 4.1 / 7.8 | 2.2 | 45 (13%) | 19 / 47 / 19 | 38% (51 / 49) |
| **B. Upkeep 1.5 food** | 9 | 3.9 / 5.2 / 7.5 | 3.0 / 4.3 / 7.8 | 2.3 | 43 (11%) | 18 / 52 / 20 | 36% (53 / 47) |
| Upkeep 1 food | 9 | 3.9 / 4.9 / 8.6 | 3.6 / 5.5 / 10.7 | 2.6 | 38 (7%) | 25 / 56 / 23 | 34% (29 / 71) |
| Grow 10 + 5 × pop | 9 | 3.8 / 5.2 / 6.6 | 3.2 / 4.5 / 7.4 | 1.9 | 44 (22%) | 13 / 61 / 25 | 36% (58 / 41) |
| Grow 5 + 5 × pop | 9 | 3.8 / 5.4 / 6.4 | 3.7 / 5.0 / 7.5 | 1.7 | 47 (36%) | 13 / 70 / 34 | 35% (54 / 45) |
| **A + B (recommended)** | 9 | 3.9 / 5.1 / 7.4 | 3.0 / 4.7 / 9.0 | 2.4 | 41 (10%) | 23 / 47 / 20 | 38% (44 / 56) |
| A + Grow 10 + 5 | 9 | 3.7 / 5.1 / 7.3 | 3.3 / 4.7 / 8.8 | 2.2 | 42 (13%) | 15 / 52 / 25 | 36% (44 / 54) |
| B + Grow 10 + 5 | 9 | 3.8 / 5.2 / 8.4 | 3.9 / 5.8 / 10.8 | 2.4 | 38 (8%) | 16 / 66 / 27 | 34% (30 / 70) |
| Supply 4 a city | 8 | 4.0 / 5.7 / 6.3 | 2.6 / 3.7 / 5.3 | 1.6 | 47 (40%) | 18 / 58 / 18 | 36% (62 / 38) |
| Supply 5 a city | 8 | 4.0 / 6.3 / 6.6 | 2.6 / 3.7 / 5.3 | 1.7 | 48 (41%) | 18 / 56 / 17 | 33% (48 / 52) |
| Melee and Ranged 1 metal | 8 | 4.0 / 5.2 / 6.8 | 2.6 / 3.7 / 5.5 | 1.8 | 47 (35%) | 17 / 55 / 22 | 33% (97 / 3) |
| Delivery 50% at 3-4 hexes | 8 | 3.8 / 5.1 / 5.9 | 2.6 / 3.7 / 5.4 | 1.7 | 47 (42%) | 19 / 62 / 25 | 40% (75 / 25) |
| Delivery 100% to 4 hexes | 8 | 3.8 / 4.7 / 4.9 | 2.4 / 3.6 / 4.1 | 1.4 | 55 (60%) | 15 / 84 / 18 | 42% (86 / 14) |
| No animals | 8 | 4.0 / 5.3 / 6.2 | 2.6 / 3.8 / 5.5 | 1.7 | 47 (36%) | 17 / 60 / 19 | 41% (80 / 20) |
| Two dens a side | 8 | 3.7 / 4.7 / 5.2 | 2.6 / 3.8 / 5.4 | 1.7 | 47 (36%) | 18 / 54 / 17 | 38% (62 / 38) |

The recommended pair over 100 turns, and in the other worlds:

| | Today | A + B |
|---|---|---|
| World 4: population / cities / army at turn 100 | 12.8 / 3.3 / 14.8 | 23.2 / 4.3 / 21.7 |
| World 4: third city (never by 100) | 68 (26%) | 59 (11%) |
| World 4: stockpile at turn 100, f / w / m | 19 / 143 / 47 | 27 / 179 / 79 |
| World 1: army at 20 / 60, population at 60, cities at 60 / 100 | 3.8 / 7.8, 6.8, 2.1 / 3.8 | 3.6 / 9.6, 10.7, 2.7 / 4.8 |
| World 5: army at 60, population at 60, second city (never) | 6.0, 5.5, 47 (37%) | 7.5, 8.9, 42 (11%) |
| World 6: army at 60, population at 60, second city (never) | 6.0, 6.1, 45 (33%) | 7.3, 9.1, 41 (10%) |
| World 4, queueing ahead: army at 60, population at 60 | 3.9, 5.0 | 4.7, 7.5 |
| World 4, army first: army at 60, population at 60 | 5.3, 5.2 | 6.4, 8.4 |

### Recommendations

1. **A Pasture on dry open ground, +1 food** (Q2 and Q3; recommended). It's an improvement like a
   farm: Improve's price and turns, on open ground with no fresh water (not snow). It closes
   most of the farmland gap between starts, lets new cities grow, and gives wood a use.
   - Options: none (today), +1, or +2. +2 erases the reason to settle by rivers.
   - To change: `improvement` in `workers.rs`, `game-rules.md` (Yields, Workers), and the
     Improve card's text.
2. **Citizens eat 1.5 food, not 2** (Q1; recommended, together with 1). It keeps the opening as it
   is (first troop at turn 9, army 3.9 at turn 20). It makes growth compound, so sides expand:
   2.4 cities at turn 60 and 4.3 at turn 100 with the Pasture, which meets the 3-5 cities the
   maps are sized for.
   - Options: 2 (today), 1.5, 1. A Grow of 10 + 5 a citizen grows the capital instead, and founds
     no more cities by turn 100.
   - To change: `FOOD_PER_CITIZEN` (in quarters, 8 → 6), which the food floor and the panels use
     too.
3. **Keep Cut Forest as it is** (Q4). No variant moved anything while wood is in surplus.
4. **Keep supply at 3 a city.** It's what caps the army from turn 20, and with the growth
   recommended above it rises with the citizens anyway: an army of 7.4 at turn 60. 4 or 5 a city
   moves only the middle game (army at turn 40: 5.7 or 6.3).
5. **Keep the troop prices of #374.** At 1 metal a Melee, metal never limits the army (97% of
   idle Barracks-turns at supply), which is what #374 set out to make it.

Not knobs, but worth deciding:
- **The late game banks wood and metal:** 143 wood and 47 metal at turn 100, and 179 / 79 with
  the recommendations. The AI builds no building but its Barracks, and no Siege or ships. The
  bank grows until it learns to spend it, on Mills, Workshops and Smelters, say.
- **Queueing troops at the city center starves the Barracks of metal** (section 5). The city
  pays first, for a troop that takes twice as long.
- **Delivery at 100% to 4 hexes did worse, not better,** which isn't what the rule's meaning
  suggests. With every tile in reach delivering alike, the citizens' choice between tiles of
  equal value falls to hex coordinates, and likely spreads them out; this wasn't looked into.
  The 75% band, as it is, works as a pull toward the city.
