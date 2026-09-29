# How the economy worked in Round 6 (#239)

> **Saved record:** section 1 of the [Round 6 report](round6-tempo-report.md), in full. It
> describes `main` at 6f5c4c8. Round 7 (option B, in
> [rts-economy.md](../rts-economy.md#round-7-option-b-an-early-game)) has since changed the land
> troops' prices and turns, the Grow price and the Gather yield. The current rules are in
> [game-rules.md](../game-rules.md).

This is written from the code on `main` at 6f5c4c8. Constants are named with their files so each can be checked. The game keeps amounts in quarters. Every number here is in whole units.

### One stockpile per side, three resources

- **Stockpile.** Each side has one store of food, wood and metal (`GameState::stockpiles`, `city/economy.rs`). It starts at `STARTING_STOCK` = 10 food, 10 wood, 4 metal.
- **When the economy runs.** Once per turn, after the units' steps and the workers' step (`resolve_economy`, `city/citizens.rs`), in this order:
  1. Every city delivers its income.
  2. The citizens eat (`feed_citizens`).
  3. Each queue pays for the item it starts, and works it (`work_queues`).
  4. Finished builds complete (`complete_builds`).

### Income: tiles, focus and delivery (`city/logistics.rs`, `city/citizens.rs`)

**The city center** yields 2 food and 1 wood on its own (`raw_yield`, `income`).

**Citizens.** A city has `population` citizens, at most `MAX_CITY_POPULATION` = 7, and each works one tile.
- The first citizen is the **manager**. It can stand on any tile the city's routes reach.
- Every other citizen must work a tile next to the manager (`may_assign`).
- A worked tile's yield is its terrain (`Tile::yields`, `terrain.rs`), or the site on it, plus:
  - 1 food for fresh water;
  - a special's bonus: Orchard +3 food, Quarry +3 production.

| Terrain | Food / production |
|---|---|
| Grassland | 3 / 0 |
| Plains | 2 / 1 |
| Tundra | 1 / 1 |
| Desert | 0 / 1 |
| Marsh | 1 / 0 |
| Coast, Lake | 2 / 0 |
| Ocean | 1 / 0 |

Modifiers: hills +1 production; forest −1 food and +1 production; jungle +1 food and +1 production.

**Improvements** are worker jobs (`improvement`, `workers.rs`): a Mine on hills (+2 production), a Lumber Mill in forest or jungle (+1), or a Farm anywhere else (+2 food).

**Wood or metal.** A tile's production splits into metal and wood (`metal_yield`).
- Metal is what is dug from the ground: +1 for hills, +2 for a mine, +3 for a Quarry, never more than the tile's production.
- The rest is wood.

**Labor focus** (`auto_assign_city`): Food, Wood, Metal or Balanced. The AI plays Balanced.
- Balanced takes food until the city's citizens are fed (+1 food to spare).
- Once they are fed, it weighs wood and metal alike.

**Delivery shares** (`delivered_share`): goods travel from the tile to the city center.
- One hex off-road costs 2, and a road step costs 1 (`HEX_STEP`, `ROAD_STEP`). A Canoe House makes a river count as road.
- The share delivered depends on the route's cost:

| Route cost | Share delivered |
|---|---|
| 2 or less | 4/4 |
| 3 to 4 | 3/4 |
| 5 to 6 | 2/4 |
| 7 to 8 | 1/4 |

- `MAX_ROUTE_COST` = 8, which is four hexes off-road.
- A **Mill** makes the tiles next to it deliver all their food (`mill_food_share`).

**Collectors.** A **Cannery** collects food, and a **Smelter** metal, from up to 3 owned, unworked sites within 3 hexes (improved sites for the Cannery, mines for the Smelter). The share is 4/4, 3/4 or 2/4 by distance (`remote_site_income`).

**Barracks income** (`barracks_income`) only matters with the PROD SPEEDUP variant. It is off by default.

**Gather** (`GATHER_YIELD`, `city/builds.rs`): a free 1-turn city build. When it's done it adds 2 food, 2 wood and 1 metal. The AI gathers whenever it has nothing else to buy, which includes every turn once a city is at the population cap.

**Ruins** (`ruins.rs`): held for `RUIN_HOLD_TURNS` = 3 turns, they give one of:
- Harvest: +8 food;
- Supplies: +4 wood and +2 metal;
- Recruits: a free Cavalry.

### Upkeep (`city/economy.rs`)

- Each citizen eats `FOOD_PER_CITIZEN` = 2 food a turn, from the side's stockpile.
- If the side can't pay all of it, the food store empties and the side's largest city loses a citizen.
- **Troops, workers and buildings cost nothing once made.**

### Paying and time (`city/economy.rs`, round 5 / #200)

- Anything can be queued. An item is paid in full on the turn work on it starts. A queue works the first item that is already paid, or that the stockpile can pay for then. The items it can't afford wait, unpaid.
- Queues pay city by city, and each city's queue pays before its Barracks' queue.
- Every build takes fixed turns: `WORK_PER_TURN` = 4 quarters of work a turn, and a build of `n` turns needs `4n`.
- **Each city has two queues:** its own and, once built, its Barracks'. Each works one item a turn. Nothing else adds build capacity: there is one Barracks per city.
- A city center trains land troops at half speed: `CITY_TRAINING_SLOWDOWN` = 2 (`city/barracks.rs`).
- **Worker jobs** (roads, improvements, walls, buildings on a site) are paid when placed. The worker walks out, at `WORKER_MOVE` = 1 hex a turn and within `WORKER_REACH` = 3 hexes of a city, and works the job's turns.

### Prices and turns (`city/builds.rs`, `workers.rs`)

Prices are food / wood / metal. Turns are at a Barracks; a city center takes twice as long for a land troop.

**Troops and other queue builds**

| Build | Price | Turns |
|---|---|---|
| Melee | 2 / 6 / 0 | 2 |
| Ranged | 2 / 7 / 0 | 2 |
| Cavalry | 3 / 4 / 3 | 3 |
| Siege | 1 / 8 / 4 | 3 |
| Armored | 3 / 2 / 7 | 3 |
| Patrol Galley | 1 / 10 / 2 | 3 |
| Landing Craft | 1 / 12 / 2 | 4 |
| Bombard Ship | 1 / 12 / 6 | 4 |
| Worker | 4 / 2 / 0 | 2 |
| Grow | 5 + 5 × pop food | 2 |
| Gather | free, gives 2 / 2 / 1 | 1 |

**Buildings and worker jobs**

| Build | Price | Turns of work |
|---|---|---|
| Barracks | 0 / 10 / 0 | 3 |
| Mill, Canoe House, Watchpost | 0 / 10 / 0 | 3 |
| Workshop | 0 / 10 / 4 | 4 |
| Forge | 0 / 6 / 8 | 4 |
| Stable | 2 / 12 / 0 | 4 |
| Field Hospital | 4 / 10 / 4 | 4 |
| Cannery | 0 / 12 / 4 | 4 |
| Work Camp | 2 / 10 / 2 | 3 |
| Smelter | 0 / 8 / 8 | 4 |
| Railhead | 0 / 12 / 12 | 5 |
| Harbor | 0 / 14 / 0 | 4 |
| Coastal Battery | 0 / 8 / 10 | 4 |
| Road | 0 / 2 / 0 | 2 |
| Improve | 0 / 4 / 0 | 3 |
| Wall | 0 / 3 / 0 | 2 |
| Gate | 0 / 3 / 2 | 3 |
| Outpost | 0 / 6 / 0 | 3 |
| Fort | 0 / 8 / 4 | 4 |

A Workshop halves the turns of a building next to it.

### Growth (`grow_price`, `city/economy.rs`)

- A Grow costs `GROW_BASE + GROW_PER_CITIZEN × pop` = 5 + 5 × pop food, and takes 2 turns in the city queue.
- It is priced when it is paid, at the population then.
- Growing from 1 to 7 costs 10 + 15 + 20 + 25 + 30 + 35 = **135 food** in all. From 2 to 7, as in the Cities scenario, it costs 125.
- **There is no other way to add a city:** no Settler build. A side has one city unless it captures one.

### Military limits (`city/barracks.rs`)

- Cavalry and Armored need a Barracks drawing on a Horses or Iron deposit. Each deposit allows `UNITS_PER_DEPOSIT` = 3 of them, alive and queued.
- **Nothing limits the rest of the army** but the stockpile and queue time. #237 proposes a supply limit.

### What the AI does (`ai.rs`, `workers.rs`)

Each empty queue gets one item the side can pay for this turn (`forecast`'s spare). A queue whose items all wait is emptied and planned again.

**City queue**, in order of preference:
1. a Worker, if the city has none;
2. a Melee, while the city has no Barracks and the side has fewer than `AI_ARMY_PER_CITY` = 2 troops per city;
3. a Grow;
4. otherwise a Gather.

**Barracks queue:** it trains whenever it can pay:
- Cavalry or Armored when the cap and the stockpile allow;
- else Melee, or Ranged for every two Melee.

**Workers:** the AI places a Barracks with its first worker, on a deposit within 3 hexes if there is one. The workers then improve the worked tiles and road them.

**No other buildings:** the AI builds nothing but the Barracks, never expands and never founds a second city.

**Units** head for the nearest enemy or unclaimed ruins.
