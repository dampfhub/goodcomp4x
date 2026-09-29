# Round 6: every knob, in every setting (#239)

> **Saved record** from the [Round 6 report](round6-tempo-report.md), section 4, which shows the
> World (4 to 6 AI) columns of these tables. It describes `main` at 6f5c4c8. Round 7 put one
> row, `combo-slow-all`, into the game (option B, in
> [rts-economy.md](../rts-economy.md#round-7-option-b-an-early-game)).

Each knob was a local edit to the code, measured, then reverted. Every run was
`SIM_SEEDS=24 REPORT_GAMES=cities,world1,world cargo test --release economy_report -- --ignored --nocapture`,
once as it is and once with `REPORT_ARMY_FIRST=1`. `world` is the World with 4 to 6 AI sides
(picked by the map's seed), `world1` the World with one, and `cities` the Cities scenario.

## What each knob changed

Prices are food / wood / metal, and turns are at a Barracks (a city center takes twice as long for
a land troop, `CITY_TRAINING_SLOWDOWN`). Files are under `src/game/city/`.

| Knob | The change |
|---|---|
| base | nothing |
| troop-price-x1.5 | `BuildUnit::price` (`builds.rs`), halves rounded up: Melee 3/9/0, Ranged 3/11/0, Cavalry 5/6/5, Siege 2/12/6, Armored 5/3/11 (from 2/6/0, 2/7/0, 3/4/3, 1/8/4, 3/2/7). Ships unchanged. |
| troop-price-x2 | the same prices doubled: Melee 4/12/0, Ranged 4/14/0, Cavalry 6/8/6, Siege 2/16/8, Armored 6/4/14 |
| troop-turns+1 | `BuildUnit::turns`: Melee and Ranged 3 (from 2); Cavalry, Siege and Armored 4 (from 3). The Patrol Galley keeps 3. |
| troop-turns-x2 | the same: Melee and Ranged 4; Cavalry, Siege and Armored 6 |
| barracks-20w-5t | `Building::price` and `turns`: the Barracks costs 0/20/0 and takes 5 turns (from 0/10/0, 3) |
| grow-x2 | `GROW_BASE` and `GROW_PER_CITIZEN` (`economy.rs`) 10 and 10 (from 5 and 5): a Grow costs 10 + 10 x pop food |
| grow-quadratic | `grow_price`: 5 x pop squared food (5, 20, 45, 80, 125, 180) |
| grow-turns-4 | `Build::turns`: a Grow takes 4 turns (from 2) |
| citizen-food-3 | `FOOD_PER_CITIZEN` (`economy.rs`, in quarters) 12 (from 8): each citizen eats 3 food a turn |
| gather-half | `GATHER_YIELD` (`builds.rs`) 1 food, 1 wood, 0 metal (from 2/2/1) |
| gather-none | `GATHER_YIELD` nothing (Gather stays the idle build) |
| delivery-steeper | `delivered_share` (`logistics.rs`): 4/4, 2/4, 1/4 and nothing at route costs up to 2, 3-4, 5-6 and 7-8 (from 4/4, 3/4, 2/4, 1/4) |
| troop-upkeep-0.5, troop-upkeep-1 | new code in `upkeep` (`economy.rs`): each troop alive (scouts and settlers aside) eats 0.5 or 1 food a turn |
| supply-4-per-city, supply-pop | new code in `pick_item` (`economy.rs`): a troop can't start while the side's troops alive plus paid troop items reach the limit, 4 per city or 1 per citizen (scouts aside) |
| combo-slow-troops | troop-price-x1.5 and troop-turns+1 |
| combo-slow-all | troop-price-x1.5, troop-turns+1, grow-x2 and gather-half (**option B**) |
| combo-upkeep-supply | troop-upkeep-1 and supply-pop |

## The tables

Per side, the mean over every side of every game. "First troop" and "pop 7 at" are median turns;
"never %" is the share of sides that never got there in 60 turns. "Deaths/turn/side" counts from
the game's first troop fight on.

### cities, the AI as it plays

| knob | first troop | army @20 | army @40 | army @60 | trained @20 | trained @60 | pop @10 | pop @20 | pop 7 at (never %) | stock @40 f/w/m | deaths/turn/side |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 8 | 3.6 | 7.1 | 7.3 | 3.7 | 7.6 | 4.3 | 5.3 | 20 (50) | 81/124/12 | 0.07 |
| troop-price-x1.5 | 8 | 3.6 | 7.4 | 7.5 | 3.7 | 7.6 | 3.8 | 4.9 | 21 (50) | 73/97/9 | 0.07 |
| troop-price-x2 | 8 | 3.5 | 7.3 | 7.6 | 3.5 | 7.6 | 3.8 | 5 | 30 (50) | 70/75/10 | 0.07 |
| troop-turns+1 | 10 | 2.8 | 6.6 | 7.4 | 2.6 | 7.4 | 4.3 | 5.3 | 20 (50) | 83/125/11 | 0.07 |
| troop-turns-x2 | 12 | 1.7 | 4.9 | 7.1 | 1.5 | 7 | 4.3 | 5.3 | 20 (50) | 87/127/12 | 0.07 |
| barracks-20w-5t | 13 | 2.8 | 6.5 | 7.2 | 2.3 | 6.9 | 4.2 | 5.6 | 18 (50) | 74/112/12 | 0.06 |
| grow-x2 | 9 | 3.7 | 6.9 | 7.7 | 3.6 | 8.9 | 3 | 4 | 45 (50) | 50/108/13 | 0.09 |
| grow-quadratic | 8 | 3.6 | 7 | 7.2 | 3.7 | 7.6 | 3 | 4 | 49 (97.9) | 68/118/15 | 0.07 |
| grow-turns-4 | 11 | 3.6 | 7.4 | 7.6 | 3.7 | 7.9 | 3.5 | 4.5 | 25 (50) | 79/89/9 | 0.07 |
| citizen-food-3 | 8 | 3.6 | 7 | 7.2 | 3.6 | 7.4 | 3.5 | 4 | 55 (52.1) | 19/120/14 | 0.07 |
| gather-half | 8 | 4 | 7.6 | 7.6 | 4.1 | 7.7 | 3.8 | 4.9 | 21 (50) | 65/91/3 | 0.07 |
| gather-none | 8 | 4.1 | 7.8 | 7.8 | 4.1 | 7.9 | 3.5 | 4.7 | 36 (50) | 54/63/3 | 0.07 |
| delivery-steeper | 8 | 3.6 | 7.1 | 7.3 | 3.7 | 7.6 | 4.3 | 5 | 21 (50) | 63/113/12 | 0.07 |
| troop-upkeep-0.5 | 8 | 3.6 | 7.1 | 7.3 | 3.7 | 7.7 | 3.5 | 4.6 | 41 (50) | 39/119/12 | 0.07 |
| troop-upkeep-1 | 8 | 3.6 | 6.9 | 6.9 | 3.7 | 7.3 | 3 | 4.1 | 57 (64.6) | 35/120/15 | 0.07 |
| supply-4-per-city | 8 | 2.4 | 4 | 4 | 2.5 | 4.1 | 4.3 | 5.3 | 20 (50) | 88/146/21 | 0.07 |
| supply-pop | 9 | 3.1 | 6.3 | 6.4 | 3.2 | 7 | 4.3 | 5.3 | 20 (50) | 81/127/11 | 0.08 |
| combo-slow-troops | 10 | 2.8 | 6.7 | 7.4 | 2.6 | 7.3 | 4.3 | 5.3 | 20 (50) | 73/101/8 | 0.07 |
| combo-slow-all | 10 | 2.4 | 6.1 | 8.4 | 2.7 | 10.1 | 2.8 | 3.8 | 53 (50) | 71/39/4 | 0.1 |
| combo-upkeep-supply | 11 | 2 | 5.3 | 6.4 | 2.8 | 8.5 | 3 | 4.3 | 28 (47.9) | 50/109/14 | 0.11 |

### world1, the AI as it plays

| knob | first troop | army @20 | army @40 | army @60 | trained @20 | trained @60 | pop @10 | pop @20 | pop 7 at (never %) | stock @40 f/w/m | deaths/turn/side |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 7 | 5.7 | 8.2 | 9 | 5.7 | 22.9 | 3 | 5.9 | 24 (0) | 137/101/98 | 0.35 |
| troop-price-x1.5 | 8 | 5.4 | 8.4 | 8.8 | 5.4 | 22.4 | 3.6 | 6 | 24 (0) | 133/70/89 | 0.35 |
| troop-price-x2 | 9 | 4.4 | 8 | 8.5 | 4.4 | 21 | 3.6 | 6 | 24 (0) | 128/48/82 | 0.33 |
| troop-turns+1 | 9 | 4.2 | 5.7 | 5.7 | 4.1 | 15.9 | 2.2 | 5.8 | 24 (0) | 138/118/96 | 0.27 |
| troop-turns-x2 | 11 | 3 | 3.7 | 3.8 | 3 | 10.8 | 2 | 5.6 | 24 (0) | 140/126/97 | 0.19 |
| barracks-20w-5t | 4 | 2 | 5.4 | 8.6 | 2 | 16.1 | 2.3 | 6 | 22 (0) | 144/108/113 | 0.19 |
| grow-x2 | 7 | 6 | 8.5 | 9.3 | 6.1 | 23.2 | 2.5 | 3.9 | 38 (0) | 63/79/94 | 0.35 |
| grow-quadratic | 7 | 6 | 8.3 | 8.8 | 6 | 23.1 | 3 | 4.1 | 47 (37.5) | 77/89/97 | 0.36 |
| grow-turns-4 | 8 | 5 | 8.2 | 9 | 5 | 21.8 | 2.3 | 5 | 28 (0) | 118/72/84 | 0.33 |
| citizen-food-3 | 7 | 5.9 | 8.3 | 8.9 | 6 | 22.9 | 2.9 | 4.5 | 34 (22.9) | 32/91/97 | 0.35 |
| gather-half | 7 | 5.8 | 8.6 | 9.3 | 5.9 | 23.4 | 3.2 | 5.9 | 24 (0) | 119/73/77 | 0.36 |
| gather-none | 7 | 5.5 | 8.2 | 9 | 5.6 | 22.2 | 3.2 | 5.7 | 26 (0) | 94/51/76 | 0.33 |
| delivery-steeper | 7 | 5.8 | 8.2 | 8.9 | 5.8 | 22.9 | 3 | 5.6 | 26 (0) | 91/88/94 | 0.35 |
| troop-upkeep-0.5 | 7 | 5.7 | 8.1 | 8.7 | 5.8 | 22.7 | 3 | 5.5 | 28 (10.4) | 72/100/98 | 0.35 |
| troop-upkeep-1 | 7 | 5.7 | 7.2 | 7.3 | 5.8 | 20.9 | 3 | 5 | 32 (39.6) | 32/95/96 | 0.34 |
| supply-4-per-city | 7 | 3.9 | 3.6 | 3.4 | 4 | 12.1 | 3 | 6 | 23 (0) | 150/142/102 | 0.22 |
| supply-pop | 7 | 5 | 5.7 | 5.6 | 5 | 17.6 | 3 | 6 | 23 (0) | 143/121/99 | 0.3 |
| combo-slow-troops | 9 | 3.9 | 5.8 | 5.5 | 3.9 | 15.7 | 3.4 | 6 | 24 (0) | 144/102/92 | 0.27 |
| combo-slow-all | 9 | 3.4 | 5.4 | 5.8 | 3.4 | 15.1 | 2.4 | 3.9 | 40 (8.3) | 56/56/65 | 0.25 |
| combo-upkeep-supply | 7 | 4.5 | 5.3 | 5.4 | 4.5 | 16.7 | 3 | 5.1 | 32 (8.3) | 48/122/101 | 0.28 |

### world, the AI as it plays

| knob | first troop | army @20 | army @40 | army @60 | trained @20 | trained @60 | pop @10 | pop @20 | pop 7 at (never %) | stock @40 f/w/m | deaths/turn/side |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 7 | 5.4 | 6.7 | 7.1 | 5.9 | 22.3 | 3 | 5.4 | 27 (1.4) | 92/107/96 | 0.32 |
| troop-price-x1.5 | 8 | 5 | 6.8 | 6.8 | 5.4 | 21.8 | 3.4 | 5.4 | 28 (2.7) | 89/75/86 | 0.32 |
| troop-price-x2 | 8 | 4.3 | 6.5 | 6.6 | 4.6 | 20.7 | 3.4 | 5.4 | 29 (2) | 84/53/79 | 0.3 |
| troop-turns+1 | 9 | 3.9 | 4.5 | 4.3 | 4.3 | 15.5 | 2.3 | 5.4 | 28 (2) | 96/124/95 | 0.24 |
| troop-turns-x2 | 11 | 2.9 | 3.1 | 2.9 | 3 | 10.5 | 2 | 5.2 | 28 (1.4) | 95/131/95 | 0.17 |
| barracks-20w-5t | 4 | 1.8 | 4.8 | 6.5 | 2.1 | 15.9 | 2.4 | 5.5 | 28 (2) | 99/108/110 | 0.2 |
| grow-x2 | 6 | 5.5 | 6.6 | 6.6 | 6.1 | 22.4 | 2.2 | 3.6 | 43 (8.2) | 45/85/92 | 0.33 |
| grow-quadratic | 7 | 5.4 | 6.7 | 6.9 | 6 | 22.3 | 3 | 4 | 55 (51) | 65/95/95 | 0.32 |
| grow-turns-4 | 8 | 5 | 6.7 | 6.7 | 5.4 | 21.7 | 2.3 | 4.6 | 31 (3.4) | 81/78/82 | 0.31 |
| citizen-food-3 | 7 | 5.4 | 6.6 | 6.6 | 6 | 22 | 2.7 | 4.1 | 41 (38.8) | 20/95/95 | 0.32 |
| gather-half | 7 | 5.4 | 7.1 | 7.1 | 6 | 22.9 | 3.2 | 5.4 | 29 (2) | 80/80/79 | 0.33 |
| gather-none | 7 | 5.2 | 6.8 | 6.9 | 5.8 | 22 | 3.1 | 5.2 | 31 (3.4) | 60/56/76 | 0.31 |
| delivery-steeper | 7 | 5.5 | 6.7 | 7 | 6 | 22.3 | 3 | 5.1 | 31 (2.7) | 58/98/95 | 0.32 |
| troop-upkeep-0.5 | 7 | 5.3 | 6.5 | 6.7 | 5.9 | 21.9 | 3 | 5 | 33 (13.6) | 48/105/96 | 0.32 |
| troop-upkeep-1 | 7 | 5.2 | 5.5 | 5.6 | 5.8 | 19.8 | 3 | 4.3 | 37 (42.9) | 28/99/92 | 0.3 |
| supply-4-per-city | 7 | 3.7 | 3.2 | 3.3 | 4.2 | 13.5 | 3 | 5.5 | 26 (1.4) | 102/142/99 | 0.22 |
| supply-pop | 7 | 4.3 | 4.9 | 4.9 | 4.9 | 18.1 | 3 | 5.4 | 27 (2) | 97/126/97 | 0.28 |
| combo-slow-troops | 9 | 3.8 | 4.4 | 4.2 | 4 | 15.2 | 3 | 5.5 | 27 (2.7) | 97/106/89 | 0.24 |
| combo-slow-all | 9 | 3.4 | 4.7 | 4.6 | 3.6 | 14.9 | 2.2 | 3.5 | 43 (19.7) | 41/58/64 | 0.23 |
| combo-upkeep-supply | 7 | 3.8 | 4.4 | 4.7 | 4.3 | 16.7 | 3 | 4.6 | 36 (17.7) | 34/125/97 | 0.25 |

### cities, army first

| knob | first troop | army @20 | army @40 | army @60 | trained @20 | trained @60 | pop @10 | pop @20 | pop 7 at (never %) | stock @40 f/w/m | deaths/turn/side |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 7 | 4.7 | 12.9 | 13.4 | 5 | 13.9 | 3.5 | 4.6 | 24 (91.7) | 57/29/3 | 0.08 |
| troop-price-x1.5 | 7 | 3 | 9.9 | 11.9 | 3.6 | 12.6 | 3.5 | 4.5 | 44 (79.2) | 48/13/4 | 0.08 |
| troop-price-x2 | 8 | 3.1 | 9.4 | 12.3 | 3.1 | 12.3 | 3.8 | 5 | 26 (50) | 44/12/6 | 0.07 |
| troop-turns+1 | 9 | 3.6 | 9.5 | 12.4 | 3.3 | 12.1 | 3 | 4.7 | 30 (50) | 65/50/3 | 0.06 |
| troop-turns-x2 | 11 | 2.9 | 7.4 | 11.9 | 2.5 | 11.6 | 3 | 4.5 | 35 (50) | 66/57/4 | 0.06 |
| barracks-20w-5t | 4 | 1.9 | 7.2 | 12.4 | 3.5 | 18.5 | 4 | 4.6 | 23 (43.8) | 111/18/2 | 0.17 |
| grow-x2 | 5 | 3.6 | 9 | 12.6 | 5 | 14.9 | 2.5 | 3.1 | 49 (87.5) | 45/32/3 | 0.11 |
| grow-quadratic | 7 | 4.3 | 11.5 | 12 | 4.6 | 13.3 | 3 | 3.5 | 58 (97.9) | 63/21/3 | 0.09 |
| grow-turns-4 | 11 | 4.1 | 10.2 | 12.8 | 4.1 | 13 | 3.5 | 4 | 26 (50) | 60/39/3 | 0.07 |
| citizen-food-3 | 7 | 4.3 | 12.6 | 13.1 | 4.6 | 13.7 | 3 | 4 | - (100) | 29/23/4 | 0.08 |
| gather-half | 7 | 4.3 | 11.8 | 12.3 | 4.6 | 13.7 | 3.5 | 4.6 | 28 (47.9) | 50/34/2 | 0.09 |
| gather-none | 7 | 4.1 | 11.5 | 12 | 4.5 | 13.4 | 3.5 | 4.6 | 35 (47.9) | 50/28/2 | 0.09 |
| delivery-steeper | 7 | 4.7 | 11.1 | 13.4 | 5 | 13.9 | 3.5 | 4.6 | 34 (50) | 48/31/3 | 0.08 |
| troop-upkeep-0.5 | 7 | 4.5 | 11.9 | 13.3 | 4.9 | 13.9 | 3.5 | 4.1 | 42 (95.8) | 40/30/3 | 0.08 |
| troop-upkeep-1 | 5 | 3.5 | 6.9 | 7.4 | 4.9 | 8.8 | 2 | 2.4 | - (100) | 27/55/14 | 0.09 |
| supply-4-per-city | 7 | 2.3 | 3.8 | 3.8 | 3.4 | 7.2 | 3.5 | 4.6 | 21 (45.8) | 70/76/3 | 0.13 |
| supply-pop | 9 | 3 | 6.5 | 7 | 3.8 | 8.1 | 3.5 | 4.6 | 31 (50) | 57/67/3 | 0.09 |
| combo-slow-troops | 9 | 3.3 | 8.7 | 12.1 | 3.4 | 12.4 | 3 | 4.5 | 27 (50) | 74/14/4 | 0.07 |
| combo-slow-all | 9 | 1.8 | 5.2 | 10.1 | 2.6 | 13.3 | 2 | 3.1 | 43 (47.9) | 70/15/4 | 0.12 |
| combo-upkeep-supply | 11 | 2 | 5.1 | 5.9 | 2.5 | 9.1 | 3 | 3.6 | 34 (95.8) | 51/52/4 | 0.12 |

### world1, army first

| knob | first troop | army @20 | army @40 | army @60 | trained @20 | trained @60 | pop @10 | pop @20 | pop 7 at (never %) | stock @40 f/w/m | deaths/turn/side |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 7 | 6.5 | 11.5 | 13.8 | 6.5 | 31 | 3 | 5.5 | 26 (0) | 97/27/78 | 0.43 |
| troop-price-x1.5 | 8 | 5.5 | 9.9 | 12.4 | 5.5 | 26.8 | 3.4 | 5.7 | 25 (2.1) | 101/12/69 | 0.36 |
| troop-price-x2 | 9 | 4.4 | 8.5 | 10.6 | 4.5 | 23.1 | 3.6 | 5.6 | 25 (2.1) | 104/7/70 | 0.33 |
| troop-turns+1 | 9 | 4.4 | 8.1 | 9.1 | 4.4 | 21.8 | 2.2 | 5.4 | 26 (0) | 101/50/80 | 0.33 |
| troop-turns-x2 | 11 | 3 | 5.7 | 5.9 | 3 | 15.3 | 2 | 5.3 | 29 (0) | 108/65/81 | 0.26 |
| barracks-20w-5t | 4 | 2.4 | 8.7 | 13.9 | 2.4 | 23.9 | 2.3 | 5.8 | 24 (0) | 113/47/101 | 0.25 |
| grow-x2 | 7 | 6.3 | 10.4 | 13.5 | 6.4 | 29.4 | 2.4 | 3.6 | 42 (22.9) | 51/15/69 | 0.4 |
| grow-quadratic | 7 | 6.7 | 10.8 | 13.8 | 6.8 | 30 | 3 | 3.9 | 51 (62.5) | 73/16/73 | 0.41 |
| grow-turns-4 | 8 | 5.2 | 10.5 | 14 | 5.3 | 28.5 | 2.3 | 5 | 28 (0) | 96/30/74 | 0.37 |
| citizen-food-3 | 7 | 6.5 | 10.9 | 13.6 | 6.6 | 29.9 | 2.8 | 4 | 37 (47.9) | 18/14/73 | 0.41 |
| gather-half | 7 | 6.5 | 11.8 | 14 | 6.6 | 30.5 | 3.1 | 5.6 | 26 (0) | 97/27/76 | 0.42 |
| gather-none | 7 | 6.1 | 10.8 | 13.5 | 6.2 | 28.9 | 3.2 | 5.6 | 28 (0) | 90/26/73 | 0.39 |
| delivery-steeper | 7 | 6.7 | 11.3 | 13.8 | 6.7 | 30.4 | 3 | 5.3 | 28 (8.3) | 60/19/75 | 0.42 |
| troop-upkeep-0.5 | 7 | 6.6 | 10.5 | 11.3 | 6.6 | 27.9 | 3 | 5 | 27 (41.7) | 40/24/77 | 0.42 |
| troop-upkeep-1 | 7 | 6.6 | 7.4 | 8 | 6.6 | 21.6 | 3 | 4.2 | 30 (75) | 14/38/77 | 0.34 |
| supply-4-per-city | 7 | 3.9 | 3.5 | 3.4 | 3.9 | 11.3 | 3 | 5.8 | 24 (0) | 114/90/88 | 0.21 |
| supply-pop | 7 | 4.4 | 6 | 6.1 | 4.5 | 17.8 | 3 | 5.7 | 25 (0) | 108/71/83 | 0.3 |
| combo-slow-troops | 9 | 4.2 | 7.4 | 8.3 | 4.1 | 20.2 | 3.2 | 5.3 | 26 (2.1) | 103/28/69 | 0.31 |
| combo-slow-all | 9 | 3.5 | 6.7 | 7.9 | 3.5 | 18.6 | 2.4 | 3.6 | 42 (22.9) | 56/17/59 | 0.29 |
| combo-upkeep-supply | 7 | 4.1 | 5.3 | 5.6 | 4.2 | 16.1 | 3 | 4.8 | 33 (29.2) | 32/70/83 | 0.27 |

### world, army first

| knob | first troop | army @20 | army @40 | army @60 | trained @20 | trained @60 | pop @10 | pop @20 | pop 7 at (never %) | stock @40 f/w/m | deaths/turn/side |
|---|---|---|---|---|---|---|---|---|---|---|---|
| base | 7 | 6.4 | 9.8 | 11 | 6.9 | 31 | 3 | 5 | 32 (5.4) | 59/31/75 | 0.41 |
| troop-price-x1.5 | 8 | 5.2 | 8.3 | 10 | 5.6 | 26.9 | 3.2 | 5.1 | 31 (8.2) | 64/14/69 | 0.35 |
| troop-price-x2 | 8 | 4.4 | 7.2 | 8.3 | 4.7 | 23.5 | 3.4 | 5.2 | 30 (9.5) | 68/10/68 | 0.32 |
| troop-turns+1 | 9 | 4.3 | 6.6 | 7.2 | 4.7 | 21.8 | 2.3 | 4.9 | 32 (3.4) | 69/56/75 | 0.31 |
| troop-turns-x2 | 11 | 2.9 | 4.6 | 5 | 3.1 | 15.2 | 2 | 4.7 | 31 (4.1) | 73/70/78 | 0.22 |
| barracks-20w-5t | 4 | 2.4 | 7.6 | 10.9 | 2.7 | 23.8 | 2.4 | 5.3 | 28 (4.8) | 78/50/98 | 0.27 |
| grow-x2 | 6 | 6 | 8.9 | 10.4 | 6.7 | 29.5 | 2.1 | 3.4 | 47 (32.7) | 43/16/70 | 0.39 |
| grow-quadratic | 7 | 6.5 | 9.3 | 10.6 | 7 | 30.3 | 3 | 3.7 | 55 (80.3) | 62/18/71 | 0.4 |
| grow-turns-4 | 8 | 5.3 | 9.1 | 10.8 | 5.8 | 28.7 | 2.3 | 4.6 | 33 (5.4) | 66/33/71 | 0.37 |
| citizen-food-3 | 7 | 6.2 | 8.8 | 10.3 | 6.7 | 29.5 | 2.5 | 3.6 | 48 (59.9) | 16/15/69 | 0.39 |
| gather-half | 7 | 6.2 | 9.7 | 10.9 | 6.8 | 30.5 | 3.1 | 5 | 32 (5.4) | 63/30/73 | 0.4 |
| gather-none | 7 | 5.9 | 9.2 | 10.9 | 6.5 | 29.4 | 3.1 | 4.9 | 33 (5.4) | 56/29/72 | 0.38 |
| delivery-steeper | 7 | 6.5 | 9.5 | 10.6 | 7.1 | 30.6 | 2.9 | 4.7 | 34 (16.3) | 39/23/73 | 0.41 |
| troop-upkeep-0.5 | 7 | 6.3 | 8.4 | 9.5 | 6.9 | 28.5 | 3 | 4.4 | 36 (46.9) | 26/27/72 | 0.39 |
| troop-upkeep-1 | 7 | 6.1 | 5.9 | 6.5 | 6.6 | 21.9 | 3 | 3.4 | 35 (78.9) | 13/38/67 | 0.32 |
| supply-4-per-city | 7 | 3.7 | 3.3 | 3.3 | 4.2 | 13.1 | 3 | 5.2 | 30 (2) | 72/89/84 | 0.21 |
| supply-pop | 7 | 4 | 5.6 | 5.9 | 4.4 | 19.4 | 3 | 5.2 | 30 (4.1) | 66/72/81 | 0.28 |
| combo-slow-troops | 9 | 4.1 | 6.2 | 6.6 | 4.4 | 20.3 | 2.8 | 4.8 | 32 (7.5) | 68/31/67 | 0.29 |
| combo-slow-all | 9 | 3.6 | 5.6 | 6.7 | 3.7 | 18.7 | 2.2 | 3.4 | 46 (36.7) | 47/17/59 | 0.26 |
| combo-upkeep-supply | 7 | 3.5 | 4.7 | 5.4 | 3.9 | 16.8 | 3 | 4.2 | 41 (36.7) | 24/69/81 | 0.24 |
