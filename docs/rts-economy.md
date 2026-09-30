# Stockpile economy (RTS-style)

Status: **experiment**, first slice built on branch `claude/rts-economy`. What is built is also in
`game-rules.md`; this file holds the reasoning, the numbers' first pass, what simulations
showed and what to try next.

## Goal

Replace the Civ-style per-city food store and production pool with an RTS-style economy: each side
accumulates resources in one stockpile and spends them to start things, which then take a fixed
number of turns. Growth stops being automatic: it is bought with food, so food is a choice between
more citizens and everything else that eats it (troops, workers).

## Model

- **Resources:** Food, Wood, Metal. They come from the existing tile yields, so the map, the
  labor rules and logistics stay as they are:
  - Food is the tile's food yield (farms, fresh water, Orchards, Cannery) as before.
  - A tile's production splits into **metal**, the part dug out of the ground (+1 for hills,
    +2 for a mine, +3 for a Quarry, never more than the tile's production), and **wood**, the
    rest (plains, tundra and desert ground, forest, jungle, lumber mills, pastures). The city
    center's 1 production is wood. A Smelter's output is metal.
- **Stockpile:** one per side (`GameState::stockpiles`, indexed by team), not per city. Every
  city's delivered food, wood and metal is added at each turn's economy, after the workers' step.
  Delivery shares apply as before (the falloff by hexes travelled, a road step counting half a
  hex, and the Mill; `game-rules.md`, Logistics). A side starts with 10 food, 10 wood and 4 metal.
- **Upkeep:** each citizen still eats 2 food a turn, now from the side's stockpile, so a farming
  city can feed a mining one. If the stockpile can't pay the whole side's upkeep, it empties and the
  side's largest city (lowest index on ties) loses a citizen (never below 1).
- **Paying** (round 5, #200): a build is paid in full when work on it starts, not when it is
  queued, so a player can queue what they can't afford yet and plan ahead. Anything can be
  queued (Harbors, deposits, the population cap and a Barracks still limit what). At each
  turn's economy, after income and upkeep, each queue works the first item that's paid for or
  that the stockpile can pay for then (`work_queues`), paying for it if it isn't; items ahead of
  it that it can't pay for wait, unpaid, and it goes back to them once it can. Queues pay in city
  order, each city's queue before its Barracks', so two cities waiting on one stockpile are
  served the same way on every machine. Taking an item off refunds its price if it was paid,
  and nothing if it wasn't. A captured city's queue and a destroyed Barracks' queue are lost,
  paid items with them. Worker jobs (roads, improvements, buildings on a site) are still paid
  when placed, as before: they reserve a tile and send a worker, so they stay as they are for
  now.
- **Time:** every build takes a fixed number of turns (`WORK_PER_TURN` work a turn). A queue
  works one item a turn; each item keeps its own work (`Queued`), so a skipped-to item that
  finishes leaves the waiting ones untouched, and reordering carries work with its item. Work
  never passes what an item needs: none carries over to the next. A Workshop halves the time
  of an adjacent building instead of its cost.
- **Barracks** (`city/barracks.rs`, second round): the primary military building. It trains
  land troops in their listed turns, whatever the manager does; a city center takes twice as
  long (`CITY_TRAINING_SLOWDOWN`), so it can still raise a Melee in an emergency. Only a
  Barracks trains Cavalry and Armored, and only one drawing on a deposit: Horses or Iron under
  it (or on or beside a Stable or Forge next to it). Each deposit a side's Barracks use allows
  `UNITS_PER_DEPOSIT` (3) of that troop. A deposit an enemy unit stands on counts for nothing,
  so deposits are worth taking and holding. The Barracks costs no metal (10 wood), so the first
  one is affordable at the start.
- **The cap counts units alive** (and queued) by default: a deposit keeps its worth all game,
  losses can be replaced, and taking or blocking an enemy's deposit cuts its cavalry at once.
  The alternative, a **lifetime** cap (every one ever trained; the deposit runs out, which pushes
  expansion to new deposits), is the Debug panel's UNIT CAP: EVER. The simulations check both
  (`SIM_LIFETIME_CAP=1`): a side never has more drawn troops than its deposits ever allowed.
  Ruins' Cavalry don't count against the cap.
- **Growth:** the Grow card (9) queues one citizen, costing `5 + 5 x population` food and 2
  turns. It's priced when it's paid, at the population then, counting the Grows already paid
  for in that city (their citizens are coming), so the card shows the price counting the Grows
  queued ahead of it. It shares the city queue, so growing also costs the city's build time. No
  food store, no automatic growth, no growth meter. Removing a paid Grow refunds the dearest
  paid Grow's price, so the ones left are paid for exactly what they cost now.
- **Ruins:** Harvest gives the side 8 food and Supplies 4 wood and 2 metal, into the stockpile.
- **Turn gating:** a player city with an empty queue holds up the turn (it can always Gather);
  one whose items all wait doesn't, and gathers by itself meanwhile (Round 9).
- **AI** (`plan_ai_cities`): a city with an empty queue queues one build it can pay for this
  turn, counting what its other queues start (`forecast`'s spare stockpile), so what it queues is
  paid and started this turn: a Worker if it has none; then Grow. A queue whose items all wait
  (the income it counted on didn't come) is emptied, at no cost as they're unpaid, and planned
  again, so an AI city never stands idle waiting. With a worker it places a Barracks for its workers to build, sited on a
  Horses or Iron deposit within 3 hexes when there is one (`ai_barracks_site`) and paid when
  placed. Until it has a Barracks it trains
  Melee itself (slowly) while the side has fewer than 2 units per city. An idle AI Barracks
  trains Cavalry or Armored when its cap and stockpile allow, else Melee, or Ranged for every two
  Melee. The AI's builds are paid by the same `work_queues` as the player's, so
  `simulation.rs` checks that no stockpile ever goes negative, that no unpaid item has work, and
  that cities both train troops and grow.
- **UI:** food, wood, metal and turns have icons (wheat, a log, an ingot, a clock;
  `map_icons.rs`), drawn on the map's yield pips and inline in text: an icon character
  (`FOOD_ICON` and the rest) in any UI string draws as its icon, in the classic font
  (`font::Face`) and in ImGui (`rich_text`, `rich_button`). The top bar shows the stockpile by
  icon with each resource's change a turn. The city tray shows what the city delivers and eats,
  the build worked with its turns left and what the first item waits for, a one-line Grow card,
  and cards whose hints are the price and turns in icons, never dimmed for the price (at a
  Barracks, dimmed when locked) but drawn short, with a red rim and price, when the side can't
  pay for them this turn; a card's tooltip says what the side is short of this turn.
  Queue rows show turns left, or, tinted red, what they wait for; a building whose first item
  waits has a tag over it on the map with the item and each missing resource's icon and amount
  (with Alt, a working one shows its item and turns left). Whether an item waits is judged
  on the stockpile as this turn's economy will find it (`forecast`: now, plus the turn's income,
  less the citizens' food and what the queues ahead start). Notices name prices in icons. The
  barracks panel shows each deposit kind's troops left, or why they're locked.

## First-pass numbers

| Build | Food | Wood | Metal | Turns (a land troop takes twice as long at a city center) |
|---|---|---|---|---|
| Melee | 2 | 6 | 0 | 2 |
| Ranged | 2 | 7 | 0 | 2 |
| Cavalry | 3 | 4 | 3 | 3 |
| Siege | 1 | 8 | 4 | 3 |
| Armored | 3 | 2 | 7 | 3 |
| Patrol Galley | 1 | 10 | 2 | 3 |
| Landing Craft | 1 | 12 | 2 | 4 |
| Bombard Ship | 1 | 12 | 6 | 4 |
| Worker | 4 | 2 | 0 | 2 |
| Grow | 5 + 5 x pop | | | 2 |
| Gather | free: +2 | +2 | +1 | 1 |
| Barracks | | 10 | | 3 |
| Mill, Canoe House, Watchpost | | 10 | | 3 |
| Workshop | | 10 | 4 | 4 |
| Forge | | 6 | 8 | 4 |
| Stable | 2 | 12 | | 4 |
| Field Hospital | 4 | 10 | 4 | 4 |
| Cannery | | 12 | 4 | 4 |
| Work Camp | 2 | 10 | 2 | 3 |
| Smelter | | 8 | 8 | 4 |
| Railhead | | 12 | 12 | 5 |
| Harbor | | 14 | | 4 |
| Coastal Battery | | 8 | 10 | 4 |

Basic troops are food and wood, so an early army needs no hills; metal is what advanced troops,
ships and some buildings are gated by. Workers cost food: they are people.

Round 7 (option B) has since raised the land troops' prices and turns, the Grow price and cut the
Gather yield; its table has the current values, and `game-rules.md` all of them.

## Variant: production speeds builds

Debug panel toggle **PROD SPEEDUP** (beside fog of war; off by default; kept across scenario
switches and loads, like fog), so both can be tried in one build. With it on, a city's queue gains `WORK_PER_TURN` plus its production
(the wood and metal it delivers this turn, in quarters, divided by 4) each turn: every whole
point of production adds a quarter turn of work. A Barracks adds the production delivered to it
(`barracks_income`) the same way. The price is still paid as work starts, so costs are
"starting the thing" and production is how fast it goes; work past what an item needs is lost,
not carried to the next. Production is not spent by this: the same wood and
metal still reach the stockpile.

## What the simulations showed

`cargo test economy_report -- --ignored --nocapture` printed each side's stockpile, population,
army and units trained every 5 turns of AI-vs-AI games (`SIM_SEEDS=8` for seeds 0-7,
`SIM_SPEEDUP=1` for the variant; since round 6 those lines need `REPORT_GAME_LINES=1`). Averages per side over seeds 0-7, 40 turns (the AI builds
only Melee, Workers and Grows, never buildings):

| | turn 10 | turn 20 | turn 30 | turn 40 |
|---|---|---|---|---|
| Cities, fixed time: food / wood / metal | 34 / 14 / 4 | 45 / 37 / 4 | 117 / 64 / 4 | 222 / 91 / 4 |
| Cities, fixed time: pop, units trained | 4.0, 3 | 6.3, 5.6 | 7.3, 10 | 7.8, 15 |
| Cities, speedup: food / wood / metal | 34 / 14 / 4 | 27 / 21 / 4 | 91 / 18 / 4 | 176 / 18 / 4 |
| Cities, speedup: pop, units trained | 4.0, 3 | 6.7, 8.5 | 7.4, 18 | 8.0, 28 |
| World, fixed time: food / wood / metal | 24 / 10 / 8 | 34 / 30 / 18 | 89 / 54 / 34 | 175 / 79 / 55 |
| World, fixed time: pop, units trained | 3.2, 2.8 | 5.8, 5.2 | 6.7, 9.2 | 6.9, 14 |
| World, speedup: food / wood / metal | 21 / 10 / 8 | 30 / 16 / 18 | 79 / 18 / 34 | 160 / 24 / 55 |
| World, speedup: pop, units trained | 3.4, 3.0 | 5.8, 7.8 | 6.7, 15 | 6.8, 23 |

(Population can pass 7 per side where a side captured a city.)

- **Early game flows.** For the first 10-15 turns wood is what runs short (a Melee's 6 wood
  against a city's 2-3 a turn), food buys a Grow every few turns, and every side grows and
  trains. Starts differ in what they lack: a hill-heavy World start stockpiles metal and waits
  on wood; a wood-poor one grows while it can't afford troops.
- **Fixed time makes the queue the bottleneck once cities grow.** From about turn 20 each city
  earns more than one queue can spend at a build every 2 turns, so food and wood pile up (200+
  food by turn 40) and cities reach the cap of 7 by turns 20-30. Spending is capped by time, not
  by resources, which is the opposite of the RTS feel wanted. Pricier troops don't change that:
  with Melee at 3 food and 8 wood (Ranged 3 and 9) the same games end with 80 wood instead of
  91 and 196 food instead of 222, so the numbers above stay the first pass.
- **The speedup variant spends what it earns.** With production speeding builds, the same games
  train about 1.8 times as many units, and wood stays low (15-25) all game: the side is
  resource-bound, which is the RTS feel. Growth is about as fast. Food still piles up once
  cities are full, because nothing but growth spends much of it.
- **Metal has no use for this AI**: Melee needs none, so Cities-scenario sides never earn any and
  World sides bank 50+. Metal only matters to a player building Siege, Armored, Cavalry, ships
  and the metal buildings.

### Second round: Barracks and the unit cap

Averages per side over seeds 0-7 (`economy_report`), fixed time, cap counting those alive:

| | turn 10 | turn 20 | turn 30 | turn 40 |
|---|---|---|---|---|
| Cities: food / wood / metal | 31 / 6 / 1 | 23 / 14 / 1 | 75 / 38 / 1 | 185 / 107 / 1 |
| Cities: units trained, Cavalry or Armored | 2.5, 1 | 7, 1 | 13, 1 | 19, 1 |
| World: food / wood / metal | 17 / 7 / 4 | 33 / 18 / 10 | 88 / 41 / 22 | 172 / 72 / 37 |
| World: units trained, Cavalry or Armored | 2.0, 0.7 | 5.9, 1.4 | 10.4, 2.2 | 14.9, 3.2 |

- **Every AI side builds its Barracks by turn 5**, on a deposit where it has one within 3 hexes.
  Wood is scarcer early (the Barracks' 10 wood), and total troops trained end about where they
  did with city training.
- **Metal, not the cap, limits Cavalry and Armored for the AI.** Cavalry's 3 metal and Armored's
  7 need worked hills: Cities sides earn no metal, so each trains one Cavalry from its starting
  metal and never reaches its cap of 3; World sides earn a little and train about 3 by turn 40.
  Metal now has a use, but the AI doesn't work hills for it: its labor focus picks food, then
  wood and metal alike.
- **Two latent bugs surfaced** and are fixed: a troop could appear on a worker out on the map
  (sharing a hex with an enemy's worker without capturing it), and a city captured in its
  interior could hand an outlying worker to its conqueror while one of its old side's units
  stood on it.

## Open questions and next experiments

- **The unit cap:** alive (the default) or lifetime? Alive keeps deposits worth holding all game;
  lifetime makes each deposit a one-off to spend, pushing expansion. Try both with the Debug
  toggle; 3 per deposit is a guess (World starts have one of each nearby).
- **Metal income:** give the AI (and the Metal labor focus) a reason to work hills and mines
  when it has a deposit to use, or make Iron itself yield metal.
- **Barracks and the manager:** training no longer needs the manager at the Barracks, and no
  citizen can stand on a building; with production speeding builds the manager beside it still adds
  its work group's production. If the manager should matter again, a manager bonus (say, a turn
  faster) is gentler than a pause.

- **Keep the speedup, or add queues?** The numbers favor production speeding builds (or,
  equivalently, shorter fixed times) so the stockpile, not the clock, limits spending. The
  alternative that keeps "fixed time" is more parallel queues: a second queue per city, or cheap
  production buildings (Barracks already is one) so a rich side spends by building more at once.
  Try both in play; the toggle is there for the first.
- **A food sink after growth:** army upkeep in food (say 1 food per 2 units a turn, starving
  like citizens), a Settler build that costs food and a citizen, or pricier troops in food. Army
  upkeep would also make growth-versus-army a running choice rather than an early one.
- **Growth price and time:** 5 + 5 x population food (15 at 2, 35 at 6) fills a city by turn
  20-30. Steeper (such as 5 x population squared) would make big cities a real investment.
- **Production double-dips in the variant:** a tile's wood and metal both fill the stockpile and
  speed its city. A cleaner split: production (the old hammers) only speeds builds, while wood
  and metal come from different tiles or improvements, or a city could spend stockpile goods to
  rush its build.
- **Should upkeep starve per city?** Pooled food lets one farming city feed the rest; if that is
  too forgiving, deliveries could feed their own city first.
- **Labor focus:** done in round 4: Food, Wood, Metal or Balanced, a focus per resource.
- **AI spending:** it builds a Barracks and trains Cavalry or Armored now, but no other building,
  and it should grow more when food piles up, to test the late game properly.
- **Refunds:** a paid item refunds in full; since round 5 an item is only paid once work on it
  starts, so parking resources in a queue means starting builds. A partial refund for an item
  in progress is still worth trying if players do that.

## Round 3: workers build what a city places

Worker jobs and buildings with a site became one thing: everything a city puts on the map
(roads, improvements, walls, gates, outposts, forts, and its buildings with a site) is placed
from its production list, needs a worker, is paid from the stockpile when placed (refunded if
taken off or dropped), and is built by a worker walking out to it within workers' reach. Prices
for the works: Road 0/2/0, Improve 0/4/0, Wall 0/3/0, Gate 0/3/2, Outpost 0/6/0, Fort 0/8/4.
The city queue keeps units, workers and Grow. The worker menu, its Sleep, and idle
workers holding up the turn are gone.

Open questions: whether the works' prices are right (roads at 2 wood each make long roads a real
cost); whether a building should take several workers, or go faster with more; and whether the
city queue and the workers now compete enough for the stockpile.

## Round 4: Gather

With builds costing resources, a broke city had nothing to do. **Gather** is a free, one-turn
city build that brings in 2 food, 2 wood and 1 metal (`GATHER_YIELD`): always possible, and
worth choosing on its own when metal is short. An empty city queue now always holds up the
turn. The Granary is gone (the last building built in the city queue). To try: whether the
yield should scale with the city (a share of its income, say) rather than be fixed.

## Round 5: pay when work starts (#200)

Players wanted to plan ahead: queue what they can't afford yet and have it wait. Now anything
can be queued, a build is paid when its city starts work on it, and a queue works the first
item it can pay for, skipping what it can't (see Paying above). How the open points were
settled:

- **Progress belongs to the item** (`Queued`: build, paid, work), not one number for the queue
  head: a skipped-to item that finishes leaves the waiting ones untouched, and reordering
  carries work with its item. Work never passes what an item needs, so none carries over to
  the next (it used to, in the production-speedup variant).
- **Refunds:** an item taken off refunds its price only if it was paid. A captured city's
  queues and a destroyed Barracks' are lost, paid items with them, as before.
- **Several cities on one stockpile:** queues pay in city order, each city's queue before its
  Barracks', which is deterministic for the simulations and network lockstep.
- **Grow's price** is set when it's paid, at the population then plus the Grows already paid
  for in the city. The card prices it counting every Grow queued ahead, which is what it will
  cost once they're done.
- **Worker jobs** are still paid when placed: they claim a tile and send a worker out, so a
  job the side can't pay for isn't placed.
- **Waiting** is judged on the stockpile as this turn's economy will find it (`forecast`):
  the stockpile now, plus the turn's income, less the citizens' food, less what the queues
  ahead of it start. A city whose first item (of its queue or its Barracks') waits shows a badge
  with the missing resources on the map; its waiting rows are tinted and say what they lack.
- **The AI** still queues only what it can pay for this turn, counting income and what its
  other queues start, one item per empty queue; a queue whose items all wait is emptied and
  planned again, so it never queues endlessly or stands idle.
- **Network play:** planning can't pay for anything, so `check_plan` accepts a plan's queues
  only if every unpaid item has no work and every paid item is one the city had paid for, with
  the work it had; spending is still accounted exactly, over paid items only.

To try: whether the forecast's counting of this turn's income surprises players when the income
doesn't come (a route cut during the turn), and whether a waiting item should hold up the turn.

`economy_report` over seeds 0-7, per side, before and after this round: units trained by turn 40
are about the same (Cities 11.9 and 11.9, World 14.7 and 14.1); the AI spends a little sooner,
since it counts the turn's income (Cities population 4.4 against 3.5 at turn 10), and World
sides bank less metal (40 against 87 at turn 40).

## Round 6: the economy's tempo (#239)

A playtest found the game moving too fast. This round measures the tempo and changes no rule.

The full report the user decided from is kept in `docs/economy/`:
[round6-tempo-report.md](economy/round6-tempo-report.md) (findings, options and questions),
[round6-how-it-works.md](economy/round6-how-it-works.md) (the rules then, from the code) and
[round6-knobs.md](economy/round6-knobs.md) (what each knob changed, and every knob in every
setting). This section is its summary.

`economy_report` (`src/game/simulation/economy.rs`) now measures, over many seeds:
- units trained by type and turn, army size, population and when each growth step lands;
- stockpiles, income, and where the resources go;
- first contact, first fight, and deaths a turn;
- build times in practice (queued, paid, worked, out), and what each queue did with its turns;
- the spread between sides.

Its knobs are environment variables, listed in its module comment:
- `REPORT_TURNS`;
- `REPORT_GAMES`: `cities`, `world`, or `world1` to `world6`;
- `REPORT_JSON`;
- `REPORT_ARMY_FIRST=1`: a city queue that would only gather trains a Melee instead. This measures what the banked resources could buy.
- `REPORT_GAME_LINES=1`.

The numbers below are from `main` at 6f5c4c8 (the same at 134305e), 60 turns, seeds 0-23 (24
games of each kind):

```
SIM_SEEDS=24 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_ARMY_FIRST=1 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_TURNS=100 REPORT_GAMES=cities,world cargo test --release economy_report -- --ignored --nocapture
```

Per side, in the World with 4 to 6 AI sides. `world4`, `world5` and `world6` agree within a
turn or two. "Army" is troops alive, scouts aside. "Trained" counts every troop the queues
turned out.

| | turn 10 | turn 20 | turn 30 | turn 40 | turn 60 |
|---|---|---|---|---|---|
| Population (one city) | 3.0 | 5.4 | 6.7 | 7.0 | 7.0 |
| Army alive / trained | 2.1 / 2.0 | 5.4 / 5.9 | 6.7 / 10.1 | 6.7 / 14.2 | 7.1 / 22.3 |
| The same, army first | 2.2 / 2.0 | 6.4 / 6.9 | 8.7 / 12.5 | 9.8 / 18.5 | 11.0 / 31.0 |
| Stockpile food / wood / metal | 13 / 7 / 7 | 18 / 28 / 25 | 37 / 62 / 59 | 92 / 107 / 96 | 242 / 205 / 178 |
| Income food / wood / metal a turn | 11 / 2.6 / 1.0 | 16 / 4.4 / 2.6 | 18 / 5.1 / 3.4 | 20 / 5.3 / 3.8 | 21 / 5.6 / 4.0 |
| Food the citizens eat a turn | 6 | 11 | 13 | 14 | 14 |

Events, median turn [10th-90th percentile]:

| Event | World, 4 to 6 AI sides | World, 1 AI side | Cities |
|---|---|---|---|
| Barracks placed / built | 1 / 5 [5-6] | 1 / 5 | 1 / 5 |
| First troop | 7 [6-7] | 7 | 8 |
| Population 3 / 4 / 5 / 6 / 7 | 9 / 11 / 16 / 21 / 27 [21-36] | 9 / 11 / 14 / 18 / 24 | 3 / 6 / 10 / 15 / 20 |
| Army of 5 / 10 | 17 / 31 (54% never reach 10) | 17 / 31 (46% never) | 11 / 26 (50% never) |
| First contact (any unit within 3 hexes) | 4 | 6 | 1 |
| First fight between troops (scouts aside) | 10 [7-11] | 21 [13-25] | 2 |
| Deaths a turn per side after that | 0.31 | 0.35 | 0.07 |
| A side's second city | only by capture: 7% of sides by turn 60 | never | the winner's, turn 18 |

Where the turns and resources go, World with 4 to 6 AI sides:

- **The Barracks trains 98% of its turns.** In practice every build takes its listed turns, with
  no turns waiting for its price, because the AI only queues what it can pay for.
  - Worker jobs take about 1.5 turns more than listed, as the worker walks out first.
  - The Barracks takes 4.5 turns instead of 3, Improve 3.6 instead of 3, Road 2.4 instead of 2.
- **The city queue gathers 72% of its turns.** Of those, 52% are at the population cap, where
  Grow is no longer possible, and 20% are below it. It grows 20% of its turns and trains 7%.
- **Gather becomes a side's second wood income once its city is full.** By turn 60 it has brought
  86 food, 86 wood and 43 metal. Troops cost 115 wood over the same turns.
- **Spending by turn 60, per side:**
  - troops: 56 food, 115 wood, 43 metal;
  - growth: 135 food, all of it by turn 40;
  - the Barracks and works: about 40 wood.
- **After turn 30 there is nothing left to buy.**
  - Population is capped.
  - There is no Settler build: a second city comes only by capture.
  - The Barracks is one queue, at a build every 2-3 turns.

  So the stockpile only grows: 600 food, 425 wood and 350 metal per side by turn 100. The army
  stays at 7-8 alive, because output (about 0.4 troops a turn) matches losses (0.3 a turn).
- **The spread between sides is small.** The side that trained most has 1.1-1.2 times the
  mean. Army size spreads more, to 2 times the mean by turn 60, by who fights whom.
- **Cities is decided early.**
  - Each side starts with 4 troops, and they fight from turn 2.
  - One city falls in every game, at turn 18 [14-20].
  - After that, the winner's Barracks holds a finished unit for want of an open hex 60% of its
    turns, because the AI has no enemy to walk to.

  Cities numbers past turn 20 describe a lone winner.
- **The variants:**
  - `SIM_SPEEDUP=1` changes little now: 25 troops trained by turn 60 against 22. The AI's
    manager is seldom beside its Barracks.
  - `SIM_LIFETIME_CAP=1` trains more, 26. Once a side has its 3 Cavalry and 3 Armored, its
    Barracks switch to Melee, which take 2 turns instead of 3.

### Knobs tried

Each knob was a local change: measured, then reverted. None is in the game. The runs used
`SIM_SEEDS=24 REPORT_GAMES=cities,world1,world`, with and without `REPORT_ARMY_FIRST=1`.

The table shows the World with 4 to 6 AI sides, per side:
- **AI:** army at turns 20 / 60, troops trained by turn 60, population at turn 20, the turn the
  city reaches 7 (and the share that never does), and the stockpile at turn 40;
- **army first:** army at turns 20 / 60, and troops trained by turn 60.

| Knob | AI: army 20 / 60 | trained | pop 20 | pop 7 (never) | stock 40 f / w / m | army first: army 20 / 60 | trained |
|---|---|---|---|---|---|---|---|
| none | 5.4 / 7.1 | 22.3 | 5.4 | 27 (1%) | 92 / 107 / 96 | 6.4 / 11.0 | 31.0 |
| troop prices x1.5 | 5.0 / 6.8 | 21.8 | 5.4 | 28 (3%) | 89 / 75 / 86 | 5.2 / 10.0 | 26.9 |
| troop prices x2 | 4.3 / 6.6 | 20.7 | 5.4 | 29 (2%) | 84 / 53 / 79 | 4.4 / 8.3 | 23.5 |
| troop turns +1 | 3.9 / 4.3 | 15.5 | 5.4 | 28 (2%) | 96 / 124 / 95 | 4.3 / 7.2 | 21.8 |
| troop turns x2 | 2.9 / 2.9 | 10.5 | 5.2 | 28 (1%) | 95 / 131 / 95 | 2.9 / 5.0 | 15.2 |
| Barracks 20 wood, 5 turns | 1.8 / 6.5 | 15.9 | 5.5 | 28 (2%) | 99 / 108 / 110 | 2.4 / 10.9 | 23.8 |
| Grow x2 (10 + 10 x pop) | 5.5 / 6.6 | 22.4 | 3.6 | 43 (8%) | 45 / 85 / 92 | 6.0 / 10.4 | 29.5 |
| Grow 5 x pop squared | 5.4 / 6.9 | 22.3 | 4.0 | 55 (51%) | 65 / 95 / 95 | 6.5 / 10.6 | 30.3 |
| Grow takes 4 turns | 5.0 / 6.7 | 21.7 | 4.6 | 31 (3%) | 81 / 78 / 82 | 5.3 / 10.8 | 28.7 |
| citizens eat 3 food | 5.4 / 6.6 | 22.0 | 4.1 | 41 (39%) | 20 / 95 / 95 | 6.2 / 10.3 | 29.5 |
| Gather 1 / 1 / 0 | 5.4 / 7.1 | 22.9 | 5.4 | 29 (2%) | 80 / 80 / 79 | 6.2 / 10.9 | 30.5 |
| Gather gives nothing | 5.2 / 6.9 | 22.0 | 5.2 | 31 (3%) | 60 / 56 / 76 | 5.9 / 10.9 | 29.4 |
| delivery shares 4, 2, 1, 0 quarters | 5.5 / 7.0 | 22.3 | 5.1 | 31 (3%) | 58 / 98 / 95 | 6.5 / 10.6 | 30.6 |
| troops eat 0.5 food | 5.3 / 6.7 | 21.9 | 5.0 | 33 (14%) | 48 / 105 / 96 | 6.3 / 9.5 | 28.5 |
| troops eat 1 food | 5.2 / 5.6 | 19.8 | 4.3 | 37 (43%) | 28 / 99 / 92 | 6.1 / 6.5 | 21.9 |
| supply: 4 troops per city | 3.7 / 3.3 | 13.5 | 5.5 | 26 (1%) | 102 / 142 / 99 | 3.7 / 3.3 | 13.1 |
| supply: 1 troop per citizen | 4.3 / 4.9 | 18.1 | 5.4 | 27 (2%) | 97 / 126 / 97 | 4.0 / 5.9 | 19.4 |
| troop prices x1.5, turns +1 | 3.8 / 4.2 | 15.2 | 5.5 | 27 (3%) | 97 / 106 / 89 | 4.1 / 6.6 | 20.3 |
| the above, Grow x2, Gather halved | 3.4 / 4.6 | 14.9 | 3.5 | 43 (20%) | 41 / 58 / 64 | 3.6 / 6.7 | 18.7 |
| troops eat 1 food, supply 1 per citizen | 3.8 / 4.7 | 16.7 | 4.6 | 36 (18%) | 34 / 125 / 97 | 3.5 / 5.4 | 16.8 |

What the knobs did:

- **Troop prices barely matter while queue time is the limit.**
  - Doubling them takes 7% off the AI's troops. The AI only waits 1-2 turns longer for its
    first troop.
  - A side spending everything on troops loses 25% at x2, because wood becomes its limit.
- **Troop turns set the output directly.**
  - +1 turn takes 30% off; x2 takes 53% off.
  - Food, wood and metal pile up faster, with nothing to spend them on.
- **The Barracks' price and turns move only the opening.**
  - At 20 wood and 5 turns, the army at turn 20 falls from 5.4 to 1.8, then catches up by
    turn 60.
  - The first troop comes sooner, at turn 4: the AI can't afford the Barracks at once, so its
    city trains a Melee first.
- **Growth knobs move only growth.**
  - Grow x2 puts full cities at turn 43.
  - A squared price leaves half the cities short of 7 at turn 60.
  - Troop output doesn't change, since troops and growth use different queues.
- **Citizen food and delivery shares shrink the food surplus, which slows growth.** With
  citizens at 3 food, 39% of cities never reach 7. Neither touches the army.
- **Gather only trims the stockpile.** Half Gather or none takes 25-50 wood off by turn 40, with
  no change to the tempo.
- **Troop upkeep in food starves cities rather than armies, as the AI plays.**
  - The AI doesn't plan for upkeep, so its largest city loses citizens. At 1 food a troop, 43%
    of cities never reach 7.
  - A side spending everything on troops ends with 6.5 instead of 11.
- **A supply limit caps the army wherever it is set, and leaves the rest of the economy alone.**
  - 4 per city holds the army at 3-4; 1 per citizen holds it at 5-6.
  - The stockpile grows faster either way, so a cap needs a resource sink beside it.

Open for the user to choose (#239): the target tempo, and which knobs to turn. The measurement
code stays, so a retune can be checked against these tables. The user chose option B, an early
game: Round 7.

## Round 7: option B ("an early game")

The user chose option B from Round 6 (#239), as an experiment: the measured `combo-slow-all`
knob. That is land troop prices x1.5, troop turns +1, Grow x2 and Gather halved. It is now the
game's tuning, in commit 4acc76b. The Round 6 report it was chosen from is kept in
[economy/round6-tempo-report.md](economy/round6-tempo-report.md), with the exact edit of every
other knob in [economy/round6-knobs.md](economy/round6-knobs.md), so another option can be tried
later.

### What changed

Prices are food / wood / metal. Turns are at a Barracks. Files are under `src/game/`.

| Constant | File | Old | New |
|---|---|---|---|
| `BuildUnit::price`, Melee | `city/builds.rs` | 2 / 6 / 0 | 3 / 9 / 0 |
| `BuildUnit::price`, Ranged | `city/builds.rs` | 2 / 7 / 0 | 3 / 11 / 0 |
| `BuildUnit::price`, Cavalry | `city/builds.rs` | 3 / 4 / 3 | 5 / 6 / 5 |
| `BuildUnit::price`, Siege | `city/builds.rs` | 1 / 8 / 4 | 2 / 12 / 6 |
| `BuildUnit::price`, Armored | `city/builds.rs` | 3 / 2 / 7 | 5 / 3 / 11 |
| `LIGHT_TROOP_TURNS` (Melee, Ranged; new, was a literal in `BuildUnit::turns`) | `city/builds.rs` | 2 | 3 |
| `HEAVY_TROOP_TURNS` (Cavalry, Siege, Armored; new, was a literal shared with the Patrol Galley) | `city/builds.rs` | 3 | 4 |
| `GROW_BASE` | `city/economy.rs` | 5 | 10 |
| `GROW_PER_CITIZEN` | `city/economy.rs` | 5 | 10 |
| `GATHER_YIELD` | `city/builds.rs` | 2 / 2 / 1 (`Stock::whole(2, 2, 1)`) | 1 / 1 / 0.5 (in quarters, 4 / 4 / 2) |
| `PROTOCOL_VERSION` | `multiplayer.rs` | 16 | 17 |

- **x1.5 rounds halves up**, as the knob run did: Ranged's 10.5 wood is 11, Cavalry's 4.5 food
  and metal are 5, Siege's 1.5 food is 2, Armored's 4.5 food is 5 and its 10.5 metal is 11.
- **Troop turns +1 is at the Barracks.** `CITY_TRAINING_SLOWDOWN` (`city/barracks.rs`) stays 2,
  so a city center takes twice the new turns: 6 for a Melee or Ranged, 8 for a Siege. The Patrol
  Galley keeps its 3 turns, and ships keep their prices.
- **Grow** costs 10 + 10 x pop food, so growing from 1 to 7 costs 270 food instead of 135.
- **Gather's half metal differs from the knob run.** The run rounded it down to none, because
  `Stock::whole` takes whole units. The stockpile keeps quarters, so the game gives the exact
  half.
  - In the Cities scenario, neither city works a metal tile, so Gather is its only metal.
  - At none, a Cities side's metal stays at the 4 it starts with. That is below a Cavalry's
    new 5 and an Armored's 11, so it can never train either. Round 6's run shows it too: Cities
    metal at turn 40 was 4.
  - `simulation.rs` checks that Cities trains a Cavalry or Armored, and that check failed.
  - In the World the half metal changes nothing that matters: see "Gather 1 / 1 / 0" below.
- **Unchanged:** every other price and time, `STARTING_STOCK`, `FOOD_PER_CITIZEN`, the
  Barracks, the population cap of 7, and the AI.
- **The rules docs follow:** `game-rules.md` (prices and turns, Grow, Gather) and
  `controls.md` (the Gather card). So do the tests that pinned a Melee's price and turns
  (`build_cards_show_prices_and_queue_what_the_stockpile_cannot_pay_yet`,
  `the_barracks_panel_shows_each_deposits_cap_and_why_a_troop_is_locked`).
  `economy_ticks_once_into_the_stockpile_and_preserves_quarters` now tops up the stockpile for
  its Siege, which costs more than the Cities scenario starts with.

**To revert**, run `git revert 4acc76b`. It will conflict on `PROTOCOL_VERSION`: set that to
main's value plus one. Don't set it back to 16, because builds with option B already use 17. Or
set by hand:
- the five troop prices back to the Old column;
- `LIGHT_TROOP_TURNS` = 2 and `HEAVY_TROOP_TURNS` = 3 (the Patrol Galley keeps its own 3);
- `GROW_BASE` = `GROW_PER_CITIZEN` = 5;
- `GATHER_YIELD` = `Stock::whole(2, 2, 1)`;
- the numbers in `game-rules.md` and `controls.md`, and the two UI tests' pinned Melee price and
  turns.

To try one of Round 6's other options instead, [round6-knobs.md](economy/round6-knobs.md) says
which constants each touched.

### Measured

Commands (60 turns, seeds 0-23; the same games as the Round 6 knob runs, so `world` is the World
with 4 to 6 AI sides):

```
SIM_SEEDS=24 REPORT_GAMES=cities,world1,world cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_GAMES=cities,world1,world REPORT_ARMY_FIRST=1 cargo test --release economy_report -- --ignored --nocapture
SIM_SEEDS=24 REPORT_TURNS=100 REPORT_GAMES=cities,world cargo test --release economy_report -- --ignored --nocapture
```

**The baseline moved.** `main` has changed since Round 6:
- the AI plans only on what its side has seen (#235);
- its scouts explore and keep out of harm's way (#234);
- a city's priority order replaced the labor focus (#247), which moved these numbers by a tenth
  or two at most.

So each run was also made on `main` just before this change was merged in, at 718acad, as "main
before". There, fights start later (turn 12 instead of 10) and fewer troops die, so armies grow
larger: 11.9 alive at turn 60 instead of 7.1.

The columns:
- **R6 base:** Round 6's baseline, at 6f5c4c8.
- **R6 combo:** Round 6's `combo-slow-all` run, the prediction.
- **main before:** `main` at 718acad.
- **option B:** this round, merged with 718acad.

World, 4 to 6 AI sides, per side. Turns are medians; "never" is the share of sides that don't
get there by turn 60.

| The AI as it plays | R6 base | R6 combo | main before | option B |
|---|---|---|---|---|
| First troop | 7 | 9 | 7 | 9 |
| Army at turn 10 / 20 | 2.2 / 5.4 | 1.3 / 3.4 | 2.1 / 6.0 | 1.3 / 3.7 |
| Army at turn 40 / 60 | 6.7 / 7.1 | 4.7 / 4.6 | 10.4 / 11.9 | 6.8 / 7.7 |
| Troops trained by turn 60 | 22.3 | 14.9 | 23.1 | 15.0 |
| Army of 5 | 17 | 23 | 17 | 23 |
| Army of 10 (never) | 31 (54%) | 43 (90%) | 28 (19%) | 42 (52%) |
| Population at turn 20 | 5.4 | 3.5 | 4.9 | 3.1 |
| City full at 7 (never) | 27 (1%) | 43 (20%) | 30 (9%) | 47 (42%) |
| Stockpile f / w / m at turn 40 | 92 / 107 / 96 | 41 / 58 / 64 | 60 / 86 / 91 | 29 / 41 / 66 |
| Stockpile f / w / m at turn 60 | 242 / 205 / 178 | 131 / 130 / 123 | 169 / 174 / 171 | 82 / 98 / 129 |
| First fight between troops | 10 | 11 | 12 | 14 |

| Army first (`REPORT_ARMY_FIRST=1`) | R6 base | R6 combo | main before | option B |
|---|---|---|---|---|
| First troop | 7 | 9 | 7 | 9 |
| Army at turn 10 / 20 | 2.2 / 6.4 | 1.3 / 3.6 | 2.3 / 6.9 | 1.3 / 3.7 |
| Army at turn 40 / 60 | 9.8 / 11.0 | 5.6 / 6.7 | 13.9 / 16.8 | 7.5 / 9.5 |
| Troops trained by turn 60 | 31.0 | 18.7 | 30.4 | 17.7 |
| Army of 5 | 15 | 23 | 15 | 23 |
| Army of 10 (never) | 27 (25%) | 39 (67%) | 25 (5%) | 38 (39%) |
| City full at 7 (never) | 32 (5%) | 46 (37%) | 35 (19%) | 48 (55%) |
| Stockpile f / w / m at turn 40 | 59 / 31 / 75 | 47 / 17 / 59 | 39 / 18 / 70 | 39 / 10 / 57 |
| Stockpile f / w / m at turn 60 | 165 / 64 / 142 | 107 / 48 / 115 | 111 / 42 / 135 | 71 / 27 / 113 |
| First fight between troops | 10 | 11 | 12 | 14 |

100 turns, World with 4 to 6 AI sides, the AI as it plays. The Round 6 column is its `final-100`
run, at 6f5c4c8:

| | R6 base | main before | option B |
|---|---|---|---|
| Army at turn 60 / 100 | 7.1 / 8.1 | 11.9 / 12.3 | 7.7 / 8.0 |
| Troops trained by turn 100 | 38.6 | 39.8 | 26.6 |
| City full at 7 (never by turn 100) | 28 (1%) | 30 (6%) | 52 (20%) |
| Stockpile f / w / m at turn 100 | 598 / 425 / 349 | 420 / 377 / 340 | 276 / 254 / 266 |

The other games, the AI as it plays, main before → option B:
- **World with 1 AI side:**
  - first troop 7 → 9;
  - army at turns 20 / 60: 5.9 / 11.6 → 3.7 / 8.4;
  - city full 26 → 40 (23% never);
  - first troop fight 22 → 26.
- **Cities:**
  - first troop 8 → 9;
  - army at turns 20 / 60: 4.4 / 8.9 → 3.5 / 8.5;
  - city full 27 → 45;
  - troops still fight on turn 2, and a city still falls in 22 of the 24 games, at turn 10
    (11 before), so half the sides never fill a city.

**Gather 1 / 1 / 0**, the exact knob, was measured too, in the World with 4 to 6 AI sides. Both
runs here are on `main` at a054502, before #247. Against option B's half metal:
- army at turn 60: 8.0 against 7.5;
- trained by turn 60: 15.0 against 15.0;
- city full: 46 against 46;
- metal at turn 60: 121 against 136.

In Cities its metal stayed at 4 all game, and it trained no Cavalry or Armored.

### What it shows

- **The early game lands where Round 6 predicted.**
  - The first troop comes at turn 9.
  - The army is 3.7 at turn 20 (the prediction: 3.4) and reaches 5 at turn 23 (23).
  - Troops first fight at turn 14.
  - Round 6's target for option B was 2-3 troops at turn 20, an army of 5 around turns 25-30,
    and full cities around turn 45. The army at 20 is a little above that, and the army of 5 a
    little early.
- **The cut in troops matches too.** Trained by turn 60 falls 35% from main before (the
  prediction: 33% from Round 6's baseline).
- **Late armies are larger than predicted, because the baseline moved, not option B.**
  - Since #235 and #234, troops fight later and die less.
  - Option B brings the army at turn 60 from 11.9 to 7.7, about where Round 6's baseline was.
  - The prediction was 4.6, from a baseline of 7.1.
- **Growth is slower than predicted.**
  - Cities are full at turn 47, with 42% short of 7 at turn 60 (the prediction: 43, and 20%).
  - By turn 100, 20% are still short.
  - Main before also grows slower than Round 6 did: population 4.9 at turn 20 against 5.4.
- **The stockpile still only grows,** at about half the rate. At turn 60 it holds 82 food, 98
  wood and 129 metal, and at turn 100, 276 / 254 / 266. Metal piles up most. The late game still
  has nothing to spend on; Settlers (#249) and the larger population cap (#250) are meant to be
  that.

Checked with `cargo test` and `SIM_SEEDS=16 cargo test --release simulation` (every scenario's
invariants hold, and the Cities check for a Cavalry or Armored passes with the half metal).

## Round 8: the supply limit (#237)

A hard cap on each side's army, which the user settled in #237: supply used against supply
available, as in an RTS. Its cities give it (for now 3 a city, 1 for each of a city's first 7
citizens and 1 for every 2 past them), and every troop, ship and Scout uses 1. It only stops new training; the rule is in `game-rules.md`
(Supply), the code in `city/supply.rs`.

Measured against `main` at 5663dbc (60 turns, seeds 0-23, World with 4 to 6 AI sides): the
first troop still comes at turn 9 and the army at turn 20 is 3.4 against 3.5, but the army at
turn 40 falls from 6.4 to 5.0 and at turn 60 from 8.9 to 7.7. Growth is unchanged, and a
capped Barracks leaves wood in the stockpile. The tables, and why 3 and 1, are in
[economy/supply-237-tempo.md](economy/supply-237-tempo.md).

## Round 9: a city with nothing to work gathers (#334)

From a playtest: a city whose queued item waits for the stockpile let the player end the turn
and then sat idle. Now a city whose own queue works nothing in a turn's economy gathers by
itself, as if Gather were chosen (`auto_gather`, in `work_queues`; the rule is in
`game-rules.md`, Gathering by itself). The panels say so for the player's cities
(`gathers_this_turn`), and `simulation.rs` checks every turn that each city either worked its
queue or gathered, never both nor neither.

Measured with `economy_report` against `main` at 602d11b (60 turns, seeds 0-7; World with 1,
4, 5 and 6 AI sides):

- **The AI's own play hardly changes.** It queues only what it can pay for (else a Gather), so
  its cities gather by themselves in at most 1% of their turns (a Settler whose income didn't
  come, a captured city's empty queue). First troop (turn 9-12), army at turn 20 (3.0-3.5) and
  growth (population 3.0-3.5 at turn 20) are the same before and after.
- **Queueing ahead is what it pays off for.** `REPORT_QUEUE_AHEAD=1` has every side queue a
  Melee (or a Grow) where the AI would gather, whatever the stockpile holds, as a player
  planning ahead does; those queues wait in 18-34% of city turns. With the rule off and on:

| game | first troop | army at turn 20 | trained by turn 40 | population at turn 20 |
|---|---|---|---|---|
| cities | 14 / 14 | 3.0 / 3.5 | 6.0 / 8.5 | 3.0 / 3.5 |
| world1 | 10 / 11 | 2.8 / 2.9 | 6.3 / 6.6 | 3.3 / 3.2 |
| world4 | 11 / 11 | 2.5 / 2.9 | 6.2 / 6.4 | 3.0 / 3.0 |
| world5 | 10 / 10 | 2.9 / 3.2 | 6.2 / 6.6 | 3.0 / 3.0 |
| world6 | 11 / 11 | 2.4 / 3.0 | 6.0 / 6.6 | 3.1 / 3.1 |

Waiting no longer costs the turns it waits: the army at turn 20 is 0.1-0.6 larger, and the
first troop comes about when it did. Queueing ahead is still slower than paying as you go (the
AI's army at turn 20 is 3.2-3.5 in the same worlds), since a waiting head holds up the queue.
