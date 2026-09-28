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
  one whose items all wait doesn't.
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
  Barracks, dimmed when locked); a card's tooltip says what the side is short of this turn.
  Queue rows show turns left, or, tinted red, what they wait for; a waiting city has a badge
  over its tower on the map with the missing resources' icons. Whether an item waits is judged
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

`cargo test economy_report -- --ignored --nocapture` prints each side's stockpile, population,
army and units trained every 5 turns of AI-vs-AI games (`SIM_SEEDS=8` for seeds 0-7,
`SIM_SPEEDUP=1` for the variant). Averages per side over seeds 0-7, 40 turns (the AI builds
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
