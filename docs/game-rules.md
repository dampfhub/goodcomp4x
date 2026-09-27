# Game rules as implemented

What the prototype does today, checked against the code. Keep this current: a behavior change
updates this file in the same commit. Design that is not built yet lives in `city-system.md`;
keys are in `controls.md`; screen-space panels in `ui-system.md`. Amounts below are as displayed
(the code stores food and production in quarter units, so a displayed 12 is 48 in code).

## Scenarios (`scenario.rs`)

| Key | Scenario | Setup |
|---|---|---|
| F1 | Combat | radius-3 map with a mountain pass; one Melee, Ranged, Cavalry and Siege per side; no cities |
| F2 | Cities (default) | radius-6 map; the Combat units plus a city, a worker, owned farms/mines/pastures and dirt roads per side; Horses and Iron deposits |
| F3 | Frontier | radius-6 map; a settler, a worker and a scout per side, no cities. Red's scout is player-controlled, to test route cuts and contests without the AI |
| F4 | World | a generated map (see World generation) with a settler, a worker and a scout per side; a new random seed every press |

Pressing F1-F3 restarts that scenario; F4 always makes a new map. The savestate (F6 save, F7
load) holds a copy of the whole game in memory; it survives scenario switches, loading keeps the
snapshot (and the camera, within the same scenario), and saving is refused mid-turn. Instant
playback (F8, on by default) resolves every step of a turn at once, in the same order, so
outcomes don't change. Fog of war (F10) is on by default. Both settings survive switches and
loads. The faded DEBUG panel (top-left) has buttons for all of these, shows a generated map's
seed, and has COMPLETE PRODUCTION (F9), which pays for the open city's or barracks' current
build at once: a unit appears if a neighboring hex is open, and a Barracks, Mill or Workshop
still needs its site and Confirm.

## Tiles (`terrain.rs`, `hex.rs`)

A tile is a base ground, optionally raised into hills and covered by a feature.

| Ground | Food | Production | Notes |
|---|---|---|---|
| Grassland | 3 | 0 | |
| Plains | 2 | 1 | the default for unlisted hexes |
| Desert | 0 | 1 | |
| Tundra | 1 | 1 | |
| Snow | 0 | 0 | can't be improved; routes cost more |
| Marsh | 1 | 0 | never hills; routes cost more |
| Mountains | 0 | 0 | impassable to units and routes; can't be worked or targeted |
| Coast, Lake | 2 | 0 | water |
| Ocean | 1 | 0 | water, away from the shore |

| Modifier | Effect |
|---|---|
| Hills | +1 production, +25% defense, +1 route cost, +1 sight |
| Forest | -1 food (not below 0), +1 production, +15% defense, +1 route cost |
| Jungle (marsh only) | +1 food, +1 production, +15% defense, +1 route cost |

- **Water:** units can't enter it or target it. Cities can work it: a route may end on a water
  tile but never continues across one.
- **Rivers** run along hex edges (World maps only). Land beside a river or a lake has fresh water:
  +1 food, on top of any improvement. Rivers affect nothing else yet.
- **Route cost** of entering a hex: 2 (3 on snow or marsh), +1 for hills, +1 for a feature; a road
  or city hex costs 1.
- **Defense** bonuses apply to units only: city and barracks defense are fixed.
- **Resources:** Horses and Iron give no yield; they let a Barracks standing on them train Cavalry
  or Armored. Only the Cities scenario places them.

## Maps

- **Combat:** mountains at (0,-3), (0,-2), (0,2), (0,3) leave a three-hex pass down the middle;
  hills at (0,0), (-2,2) and (2,-2).
- **Cities:** the same three hills; mountains at (0,±2), (0,±3), (0,±4); Horses at (-2,0) and
  (2,0); Iron at (-2,1) and (2,-1).
- **Frontier:** hills at (-1,2) and (1,-2); mountains at (0,±3).
- **World generation** (`mapgen.rs`): a rectangle about 61 hexes wide by 36 tall, generated from a
  `u32` seed with its own RNG, so a seed always rebuilds the same map (the seed shows in the debug
  panel; there is no way to type one in). A Pangea: 42-52% sea, one continent plus islets of at
  most 12 hexes, mountain ranges and hills by noise, lakes, rivers running downhill to water,
  climate by latitude and moisture, forest on wetter grassland, plains and tundra, jungle on about
  three quarters of marsh. The two starts are on the largest continent, far apart, on flat land
  that is not snow, desert or marsh, scored on nearby yields and fresh water; the left one is
  Blue's. Each side gets a settler on its start, a worker on flat ground next to it and a scout on
  neighboring hills, so starting sight is equal. The camera starts on Blue's settler.

## Units (`unit.rs`)

| Type | HP | Attack | Defense | Move | Range | Sight | Ability | Icon |
|---|---|---|---|---|---|---|---|---|
| Melee | 100 | 22 | 20 | 1 | 1 | 2 | Shield Wall | sword |
| Ranged | 75 | 24 | 10 | 1 | 2 | 2 | Volley | bow and arrow |
| Cavalry | 100 | 24 | 14 | 2 | 1 | 3 | Charge | horse head |
| Siege | 65 | 32 | 6 | 1 | 2 | 2 | Deploy | catapult |
| Scout | 60 | 8 | 10 | 3 | 1 | 3 | Lookout | spyglass |
| Armored | 140 | 30 | 28 | 1 | 1 | 2 | Shield Wall | heater shield |

`Unit::stats()` applies abilities and siege deployment on top of these; everything that asks
what a unit can do goes through it. Settlers (a planted flag) and workers (a shovel) are
civilians with the Melee body, drawn as hollow hexagons with only a move badge; every other unit
is a team-colored disc with its pictogram. Workers never attack (their attack step is skipped)
but still retaliate; settlers can be ordered to attack, though no badge shows it. Cavalry and
Armored are built only at a Barracks standing on Horses or Iron respectively; cities can't
queue them. Nothing heals.

## Fog of war (`fog.rs`)

- On by default; F10 or the debug panel toggles it.
- The player's units see 2 hexes (Scout and Cavalry 3), +1 on hills, +2 through the turn
  after a Lookout. The player's cities see 3 and barracks 1.
- A mountain strictly between two hexes blocks sight; the mountain itself is visible.
- Every frame, each hex in sight is recorded as last seen: cities and barracks (with their
  health), improvements and roads. Units aren't remembered, since they move: out of sight, the
  player knows of no unit anywhere. Remembered hexes out of sight draw that memory under a grey
  veil; never-seen hexes draw nothing.
- Enemy units out of sight are hidden, with their ghosts and hover info; tile tooltips describe
  remembered hexes from memory.
- The player plans from what they know: in sight, the board as it is; out of sight, the memory.
  A unit out of sight, even one seen there before, doesn't shrink the move range, and clicking
  its hex plans a move, which then meets it at resolution. A remembered enemy city or barracks
  can still be attacked.
- What the map and panels show follows the same rule: yields, and which hexes a city's or
  barracks' goods reach (badges, delivery percentages, tooltip, SELECTED TILE), use remembered
  cities and roads out of sight, and no units. A city or barracks shows its live hover panel only if it
  is the player's own or in sight. The economy itself runs on the real board, so income and the
  red disrupted-tile rings can still reflect an unseen enemy on a route.
- The AI ignores the fog.

## Orders (planning)

- Left-click acts on release. Dragging at least 6 pixels pans the map instead and suppresses the
  click; middle-drag also pans. Losing focus or leaving the window cancels the gesture.
- Click one of your units to select it. Click a green hex to queue a move; click it again to
  cancel. Left-clicking your own city or barracks hex opens its view instead, even with a unit
  selected, so a left click can't move a unit onto that hex or select a unit standing there.
- Click an enemy (unit, city or barracks) in range to queue an attack on its hex. Shift-click
  attacks any hex in range, occupied or not. **Attacks target hexes:** whoever stands there when
  the attack resolves gets hit.
- A unit can queue a move and an attack; the attack range is measured from the planned
  destination. Changing or cancelling the move drops an attack that is no longer in range.
- M and X (or the Move and Attack buttons) arm the next map click as a move or attack.
- Ctrl-click an adjacent ally to swap places (see Swaps).
- Right-click queues a move to an open hex or an attack on an enemy (or disarms an armed action);
  Ctrl-right-click clears the selected unit's orders, including a hold or guard.
- Q (or the ability button) toggles the selected unit's ability.
- **Hold:** Space holds the selected unit if it still needs orders: it keeps what it has queued
  and gives up the rest of its turn. With nothing left waiting, Space ends the turn. With a city
  or barracks view open, Space only closes it.
- **Guard:** G toggles `Unit::guarding`, like holding but lasting across turns, so the unit never
  comes back up in the turn order. Queuing any move, attack or swap wakes it, as do G and
  Ctrl-right-click. Guarding units get a white hex outline.
- **Selection flow:** the first unit needing orders is selected at the start of each turn, and
  once the selected unit is done the next one is selected automatically. "Done" (`needs_orders`)
  means holding or guarding, or having a move queued (or unable to move) and an attack queued (or
  unable to attack). Enemies in range don't matter, since hex attacks are always possible. A unit
  in a contested hex is always done. Selecting a unit by clicking never auto-advances, so a
  finished unit can be reselected to edit. Tab looks at the next unit without holding the current
  one. Whenever the game picks the unit, the camera glides to it; middle-drag cancels the glide.
- Movement is a BFS through passable, unoccupied hexes: units can't pass through each other,
  mountains or water. Two allies can't head for the same hex.
- **Ending the turn:** `pending()` counts player units that still need orders and player cities
  with nothing queued; citizen assignments never count. Space selects what is still waiting (a
  unit, then a city needing a build) and ends the turn once nothing is. The End Turn button
  (`end_planning`) holds every unfinished unit, opens a city if one still needs a build, and
  otherwise ends the turn. Input, including UI clicks, is ignored while a turn plays out.

## Turn resolution (`turn.rs`)

After a 0.6 s pause (so the last order is visible), the turn plays out in 12 steps, one every
0.6 s, with the acting units flashing (or all at once with instant playback). Steps where nobody
acts are skipped.

1. Scout move  2. Cavalry move  3. Melee move  4. Ranged attack  5. Scout attack
6. Cavalry attack  7. Melee attack  8. Ranged move  9. Siege move  10. Siege attack
11. Armored move  12. Armored attack

Holding Alt shows each unit's rank: blue number = its move among move steps, red = its attack
among attack steps.

Everyone in a step acts simultaneously:

- **Moves:** all at once. A unit can enter a hex whose occupants are all leaving this step, so
  chains and rotations work. A move into a hex where someone stays is blocked. Two enemies can't
  swap by moving through each other; allies can.
- **Collisions:** exactly two enemies moving onto the same hex both take it, and it becomes
  contested. Any other pile-up bounces everyone involved.
- **Attacks:** all use the board as it stood at the start of the step. Damage is summed and
  applied at the end, so a unit killed this step still gets its attack off. Two units attacking
  each other in the same step make one exchange of blows, not two attacks that each draw
  retaliation.
- Each mover's order is spent when its step runs, whether it got through or not.
- After the last step: city economy (income, growth, builds), then each unit's end of turn
  (ability cooldown, siege setup, Lookout, orders cleared).

## Contested hexes

- Both enemies occupy the hex. It turns orange, and the two units are drawn half-size, stacked
  with Blue on top.
- In their attack step every turn, the two automatically trade blows. They can't be ordered to
  attack anything else.
- Either can move out to give up the hex. Other units can still attack into it and hit their
  enemy there.

## Swaps

- Ctrl-click an adjacent ally. Both units get a move into the other's hex, drawn linked by a line.
- The swap happens at whichever of the two moves first; the other is pulled along and skips its
  own move step.
- Clicking again, re-ordering either unit, or right-clicking cancels both halves. Not allowed for
  units in a contested hex or units that can't move.

## Groups (`group.rs`)

- Alt-drag a box to select your units drawn inside it; Alt-click adds or removes one unit. Two or
  more become the group; one is an ordinary selection. The group's hexes are highlighted and the
  tray summarizes it.
- Clicking a hex (or Move) converges: members' old moves are dropped, then, nearest to the target
  first, each takes the reachable hex closest to the target that no ally is heading for, staying
  put if it can't get closer. Members keep their own speeds, so the group doesn't hold formation.
- Clicking an enemy (or Attack/Shift) has every member that can reach the hex attack it; clicking
  a target they all already attack calls it off.
- Space/Hold holds every member, G guards them all (or unguards if all are), Ctrl-right-click
  clears their orders, clicking one member selects just it.
- The group is cleared when a turn resolves.

## Attack display (`draw.rs`, `effects.rs`)

- Queued attacks are curved arrows from the attacker (or its ghost, if it moves first) to just
  short of the target. Only the player's own attacks get arrows; the AI's plans stay hidden.
- When an attack resolves, the arrow shoots from attacker to target, then shows a burst on a hit,
  "MISS" on a hex with no enemy unit, city or barracks, or "OUT OF RANGE" if the target moved
  away. Every unit or structure hurt (retaliation included) shows a rising damage number, or
  "KILLED". Enemy attacks animate too.

## Abilities (`ability.rs`)

| Ability | Units | Effect | Cooldown |
|---|---|---|---|
| Shield Wall | Melee, Armored | +50% defense this turn, can't move | 1 turn |
| Volley | Ranged | attack also hits enemies adjacent to the target, all hits at 60% | 2 turns |
| Charge | Cavalry | +1 move, +50% attack this turn | 2 turns |
| Deploy / Pack Up | Siege | spend a turn setting up (no move or attack); deployed: +1 range, can't move; packing up takes a turn too | none |
| Lookout | Scout | stay put this turn; +2 sight through the next turn | none |

A queued ability shows as a gold ring and deployed siege as a steel ring. Cooldowns tick down at
every turn end.

## Combat (`combat.rs`)

- Damage = `30 * e^((attack - defense) * 0.04) * random(0.8..1.2)`, clamped to 1..100. Defense
  includes terrain and Shield Wall; attack includes Charge.
- Melee attacks (base range 1) draw retaliation from a defender that survives the hit.
- **Cities** have 320 HP and 30 defense and fire back at every attacker within range 2 with attack
  26. **Barracks** have 220 HP and 25 defense and are removed (with their queue) at 0 HP; the city
  can then build a new one, at full HP. A structure is hit only when the attack hits no enemy unit
  at all (for a Volley, none on the target or its neighbors); Volley's 60% applies to structures
  too. Cities cannot be captured.

## Cities (`city.rs`)

- **Founding:** F with a selected settler, at least 3 hexes from any other city; the new city
  starts at population 1, auto-assigns and opens. The AI founds a city in place, at the start of
  any resolution where it has a settler and no city, without the 3-hex rule.
- **Yields:** the city center gives 2 food and 1 production; each worked tile gives its tile
  yield (above) times its delivery share. Improvements (worker, instant, no turn cost): a mine on
  hills (+2 production), a lumber mill under forest or jungle (+1 production), otherwise a farm (+2
  food); snow can't be improved. The Cities scenario's preplaced farms (4/0), mines (0/4) and
  pastures (3/1) have fixed yields.
- **Logistics:** each worked tile's goods travel its cheapest route to the city, summing the
  route costs above; routes costing more than 8 don't exist. Delivery is 100% at cost 0-2, 75% at
  3-4, 50% at 5-6, 25% at 7-8. Enemy units, contested hexes, enemy cities and mountains block
  routes, and an enemy on the city blocks them all. Routes are recalculated every time they're
  used.
- **Manager and workers:** population is at most 7: the manager (the first worked tile, ringed in
  gold and marked `M`, which must be land) plus up to six workers, each adjacent to the manager. To move the
  manager, click it to pick it up, then click its destination; workers keep their offsets where
  they can and are otherwise replaced by the best nearby tiles.
- **Citizens:** click tiles to assign or release; A auto-assigns by the city's labor focus (Food,
  Production, or Balanced, which picks food tiles until the city's food income covers upkeep plus
  1, then production). Setting a focus
  re-assigns. On growth or route disruption, reconciliation keeps valid manual assignments and
  fills or replaces the affected slot; a manual tile cut off by an enemy is remembered and returns
  when the route reopens, unless you changed it.
- **Food and growth:** each citizen eats 2 food a turn. Growth needs 10 + 5 × population stored
  food. A shortfall drops population by 1 (never below 1) and empties the store.
- **Production and the queue:** one production pool per city, which accumulates only while
  something is queued (it resets to 0 on an empty queue, when the first item is queued, and when
  the head is removed). Reordering keeps the pool, so progress moves to the new head; a finished
  building waiting for Confirm locks the head in place. Drag a row
  to reorder, click its X to remove; Backspace removes the head and PageDown swaps the first two.
  A finished unit appears on an open neighboring hex (the city holds it until one opens) and keeps
  leftover production. A player city with an empty queue holds up the turn.
- **Costs:** Melee 12, Ranged 14, Cavalry 16, Siege 18, Armored 20; Granary 12, Barracks 16,
  Mill 15, Workshop 20. Keys 1-3 queue Melee, Ranged and Siege (a city can't queue Cavalry or
  Armored), 4-7 Granary, Barracks, Mill, Workshop.
  One of each building per city.
- **Buildings:**
  - **Granary:** +2 food per turn. Completes when paid for.
  - **Barracks, Mill, Workshop** stand on a site: queuing one starts site selection (passable land
    you have explored, not a city, building or other planned site). Click the site's map badge to
    move it. When paid for, the building waits (blocking the queue) until you click Confirm in the
    tray. Completing any building resets production to 0.
  - **Barracks:** its own view and queue (all five unit types) with its own production pool, earned
    only while the city's manager stands on the barracks: each worked tile's production times its
    delivery share from the barracks. The city's own income still counts those tiles too. Cavalry
    needs the barracks on Horses, Armored on Iron. A unit appears next to the barracks, and the
    pool resets after each.
  - **Mill:** worked tiles adjacent to it deliver all their food, if they can reach the city.
  - **Workshop:** a planned Barracks, Mill or Workshop site adjacent to a workshop costs half.
- **Workers** (`W`): R builds a dirt road on the worker's hex; I improves it (see Yields), unless
  another team's site is there. Neither works while a turn plays out.
- Economy runs once per turn, after the last combat step.

## Interface (`ui.rs`, `ui/dock.rs`)

- Panels dock in four corner zones and never overlap (`docs/ui-system.md`).
- **Top bar:** turn number, the latest notice, and the End Turn button, whose label names what is
  still waiting ("3 UNITS NEED ORDERS", "CHOOSE PRODUCTION") until it turns gold and reads END
  TURN.
- **Command tray** (bottom-left): with a city open, it shows population, stores and rates, the
  current build, labor focus buttons, the growth meter, the selected tile, unit cards (1-3), the
  Yields button, cards for buildings not yet built (4-7), barracks status with See Barracks, and
  planned sites with Confirm once they are paid for; the queue docks above it. With a barracks open, its five train buttons and Open City, queue above. With a unit
  selected: stats (boosted values green, reduced red), notes, and buttons Move, Attack, Swap, then
  its ability (or Found City, or Build Road and Improve for workers, who get no Attack), then Hold
  and Guard. Move, Attack and Swap arm the next map click only (a held modifier overrides it);
  pressing the button again or right-clicking disarms. The armed button has a bright border, a
  queued order turns its button gold, an unusable one is dimmed. Every button has a hover tooltip.
- **Hover:** hovering a unit shows its stats at the top-right; hovering a city or barracks shows
  a structure panel at the bottom-left instead (HP, growth progress for a city, production,
  current build). Hovering a city also outlines its worked tiles, without yield badges. After the
  cursor rests on a hex for 0.75 s, a tooltip shows terrain or city, yields, defense, site, road,
  which city works it, its delivery share to the open city, and units on it.
- **City view:** C opens the first city needing a build (or your first city), and left-clicking
  your city opens that one. While open, map clicks manage tiles and never select units; it closes
  on Tab, Space, Escape, or a click off the map. A barracks view closes the same way; C switches
  it to its city. Worked tiles are outlined green (the manager's
  in gold; red if disrupted), with dotted links from the manager to its workers. With yields
  shown (Y or the Yields button; on by default), the open city's reachable and worked tiles show
  food (green grain) and production (amber hammers) with delivery percentages.
- Escape closes an open city or barracks view; otherwise holding it for a second quits, with a
  "HOLD ESC TO QUIT" bar. F5 toggles borderless fullscreen.

## AI (`ai.rs`)

Each Red unit picks the enemy unit nearest on foot (walking distance around terrain), attacks it
if already in range, and otherwise moves to the reachable hex with the shortest remaining walk,
attacking if that brings it into range. It skips hexes a teammate already claimed, and units in a
contested hex stay and fight. It never uses abilities, never attacks cities, never builds
buildings, ignores the fog, and ignores its civilians and any player-controlled Red unit; its
scouts fight like any other unit. Red cities auto-assign citizens at every end of planning and
queue Melee whenever their queue is empty. Ties break by hex coordinates, so it is deterministic.

## Open questions

Known bugs link to their board item; the rest are design questions nobody has decided yet.

- The AI never uses abilities, attacks cities, builds, or uses workers; it ignores the fog, and
  its scouts just fight.
- Cities and barracks can be damaged but not captured, and nothing heals.
- Cooldowns tick every turn whether or not the unit acted.
- Contests only form when two enemies arrive in the same step. A later arrival is just blocked; it
  could be allowed to charge in and contest instead.
- A unit locked in a contest still retaliates against third-party melee attackers.
- A move only checks its destination at resolution, not whether its planned path is still open,
  so a move planned through an unseen enemy passes it.
- Hills and forest cost the same to enter as plains, and rivers don't slow or penalize crossing
  units.
- Generated maps have no resources yet, and there are only two sides on the four-player map.
- Income, disrupted-tile rings and the city view's route notice react to enemies the player
  can't see (#37).
- Swaps only work between adjacent units.
- Worker roads and improvements are instant.
- No victory condition; F1-F4 restart a scenario.
