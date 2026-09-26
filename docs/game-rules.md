# Game rules as implemented

What the prototype does today, checked against the code. Keep this current: a behavior change
updates this file in the same commit. Design that is not built yet lives in `city-system.md`;
keys are in `controls.md`. Amounts below are as displayed (the code stores food and production in
quarter units, so a displayed 12 is 48 in code).

## Scenarios (`scenario.rs`)

| Key | Scenario | Setup |
|---|---|---|
| F1 | Combat | radius-3 map with a mountain pass; one Melee, Ranged, Cavalry and Siege per side |
| F2 | Cities (default) | radius-6 map; the Combat units plus a city, a worker, owned farms/mines/pastures and dirt roads per side; Horses and Iron deposits |
| F3 | Frontier | radius-6 map; a settler (`T`), a warrior and a worker per side, no cities. Red's warrior is player-controlled, to test route cuts and contests without the AI |

Pressing the current scenario's key restarts it. The savestate (F6 save, F7 load) holds a copy of
the whole game in memory; it survives scenario switches, and loading keeps the snapshot (and the
camera, within the same scenario). Instant playback (F8) resolves every step of a turn at once, in
the same order, so outcomes don't change; it survives switches and loads. The faded DEBUG panel
(top-left) has buttons for all of these.

## Map

- Flat-top hex grid in axial coordinates.
- **Combat:** mountains at (0,-3), (0,-2), (0,2), (0,3) leave a three-hex pass down the middle;
  hills at (0,0), (-2,2) and (2,-2).
- **Cities:** the same three hills; mountains at (0,±2), (0,±3), (0,±4); Horses at (-2,0) and
  (2,0); Iron at (-2,1) and (2,-1).
- **Frontier:** hills at (-1,2) and (1,-2); mountains at (0,±3).
- **Hills:** +25% defense for the unit standing there.
- **Mountains:** impassable. Units can't enter them, path through them, or target them.

## Units (`unit.rs`)

| Type | HP | Attack | Defense | Move | Range | Ability | Icon |
|---|---|---|---|---|---|---|---|
| Melee | 100 | 22 | 20 | 1 | 1 | Shield Wall | triangle, M |
| Ranged | 75 | 24 | 10 | 1 | 2 | Volley | square, R |
| Cavalry | 100 | 24 | 14 | 2 | 1 | Charge | pentagon, C |
| Siege | 65 | 32 | 6 | 1 | 2 | Deploy | octagon, S |
| Horse | 100 | 28 | 16 | 3 | 1 | Charge | pentagon, H |
| Armored | 140 | 30 | 28 | 1 | 1 | Shield Wall | hexagon, A |

`Unit::stats()` applies abilities and siege deployment on top of these; everything that asks
what a unit can do goes through it. Settlers (`T`) and workers (`W`) use the Melee body. Workers
never attack (their attack step is skipped) but still retaliate. Horse and Armored are built only
at a Barracks standing on Horses or Iron respectively.

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
- Movement is a BFS through passable, unoccupied hexes: units can't pass through each other or
  mountains. Two allies can't head for the same hex.
- **Ending the turn:** `pending()` counts player units that still need orders and player cities
  with nothing queued; citizen assignments never count. Space selects what is still waiting (a
  unit, then a city needing a build) and ends the turn once nothing is. The End Turn button
  (`end_planning`) holds every unfinished unit, opens a city if one still needs a build, and
  otherwise ends the turn. Input is ignored while a turn plays out.

## Turn resolution (`turn.rs`)

After a 0.6 s pause (so the last order is visible), the turn plays out in 12 steps, one every
0.6 s, with the acting units flashing. Steps where nobody acts are skipped.

1. Cavalry move  2. Melee move  3. Ranged attack  4. Cavalry attack  5. Melee attack
6. Ranged move  7. Siege move  8. Siege attack  9. Horse move  10. Horse attack
11. Armored move  12. Armored attack

The badges on each unit show this: blue number = its move step, red = its attack step.

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
  (ability cooldown, siege setup, orders cleared).

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
  "MISS" on a hex with no enemy unit, or "OUT OF RANGE" if the target moved away. Every unit hurt
  (retaliation included) shows a rising damage number, or "KILLED". Enemy attacks animate too. An
  attack on an empty enemy city or barracks shows MISS but still deals its damage.

## Abilities (`ability.rs`)

| Ability | Units | Effect | Cooldown |
|---|---|---|---|
| Shield Wall | Melee, Armored | +50% defense this turn, can't move | 1 turn |
| Volley | Ranged | attack also hits enemies adjacent to the target, all hits at 60% | 2 turns |
| Charge | Cavalry, Horse | +1 move, +50% attack this turn | 2 turns |
| Deploy / Pack Up | Siege | spend a turn setting up (no move or attack); deployed: +1 range, can't move; packing up takes a turn too | none |

A queued ability shows as a gold ring and deployed siege as a steel ring. Cooldowns tick down at
every turn end.

## Combat (`combat.rs`)

- Damage = `30 * e^((attack - defense) * 0.04) * random(0.8..1.2)`, clamped to 1..100. Defense
  includes terrain and Shield Wall; attack includes Charge.
- Melee attacks (base range 1) draw retaliation from a defender that survives the hit.
- **Cities** have 320 HP and 30 defense and fire back at attackers within range 2 with attack 26.
  **Barracks** have 220 HP and 25 defense and are removed at 0 HP. Cities cannot be captured yet.

## Cities (`city.rs`)

- **Founding:** F with a selected settler, at least 3 hexes from any other city. The AI founds a
  city in place, at the start of any resolution where it has a settler and no city, without the
  3-hex rule.
- **Tile yields** (food/production): plains 2/1, hills 1/2, mountains 0/0; a farm 4/0, a mine
  1/4; preplaced sites carry their own. The city center adds 2 food and 1 production.
- **Logistics:** each worked tile's goods travel its cheapest route to the city. A step costs 1 on
  a road, 2 on plains, 3 on hills; routes longer than 8 don't exist. Delivery is 100% at cost 0-2,
  75% at 3-4, 50% at 5-6, 25% at 7-8. Enemy units, contested hexes and enemy cities block routes,
  and an enemy on the origin blocks them all. Routes are recalculated every time they're used.
- **Manager and workers:** population is at most 7: the manager (the first worked tile, a gold
  `M`) plus up to six workers, each adjacent to the manager. To move the manager, click it to pick
  it up, then click its destination; workers keep their offsets where they can and are otherwise
  replaced by the best nearby tiles.
- **Citizens:** click tiles to assign or release; A auto-assigns by the city's labor focus (Food,
  Production or Balanced). On growth or route disruption, reconciliation keeps valid manual
  assignments and fills or replaces the affected slot; a manual tile cut off by an enemy is
  remembered and returns when the route reopens, unless you changed it.
- **Food and growth:** each citizen eats 2 food a turn. Growth needs 10 + 5 × population stored
  food. A shortfall drops population by 1 (never below 1) and empties the store.
- **Production and the queue:** production accumulates only while something is queued (it resets
  to 0 on an empty queue, and when the first item is queued). The queue holds units and buildings;
  Backspace removes the head, PageDown promotes the next item, and each item has up/down/remove
  buttons. A finished unit appears on an open neighboring hex (the city holds it until one opens)
  and keeps leftover production; a finished building resets it to 0.
- **Costs:** Melee 12, Ranged 14, Cavalry 16, Siege 18, Horse 17, Armored 20, Granary 12,
  Barracks 16. Keys 1-4 queue Melee to Siege, 5 Granary, 6 Barracks.
- **Granary:** +2 food per turn.
- **Barracks:** queuing one starts site selection (any passable non-city tile); if no site was
  chosen by completion, selection reopens then. Confirm Barracks, available once it is complete,
  places it. It has its
  own view and queue (all six unit types) with its own production: the production of the tiles the
  manager's work group delivers to the barracks, counted only while the manager stands on the
  barracks tile. The city's own queue still receives that labor too. Horse needs the barracks on
  Horses, Armored on Iron. The barracks queue's production resets to 0 after each unit.
- **Workers** (`W`): R builds a dirt road on the worker's hex; I improves it (hills become a mine,
  anything else a farm). Both are instant and cost no turn.
- Economy runs once per turn, after the last combat step.

## Interface (`ui.rs`)

- **Top bar:** turn number, the latest notice, and the End Turn button, whose label names what is
  still waiting ("3 UNITS NEED ORDERS", "CHOOSE PRODUCTION") until it turns gold and reads END
  TURN.
- **Command tray** (bottom-left): with a city open, its stores, income, labor focus and Yields
  buttons, growth meter, unit and building cards, and barracks status (See Barracks, Change Barracks
  Site, Confirm Barracks), with the queue in a panel above. With a unit selected: stats (boosted values green,
  reduced red), notes, and buttons Move, Attack, Swap, then its ability (or Found City, or Build
  Road and Improve for workers, who get no Attack), then Hold and Guard. Move, Attack and Swap
  arm the next map click only (a held modifier overrides it); pressing the button again or
  right-clicking disarms. The armed button has a bright border, a queued order turns its button
  gold, an unusable one is dimmed. Every button has a hover tooltip, drawn clear of its panel.
- **Hover:** hovering a unit shows its stats in a box at the top-right; hovering a city shows a structure panel at
  the bottom-left instead (HP, growth progress, production, current build), and hovering a
  barracks shows its HP, production and current build. Hovering a city also outlines its worked
  tiles, without yield badges. After the cursor rests on a hex for 0.75 s, a tooltip shows terrain or city, yields,
  defense, site, road, which city works it, its delivery share to the open city, and units on it.
- **City view:** C (or left-clicking your city) opens it. While open, map clicks manage tiles and
  never select units; it closes on Tab, Space, Escape, or a click off the map. A barracks view
  closes on Space or Escape (not Tab). Worked tiles are
  outlined green (red if disrupted), with dotted links from the manager to its workers. With
  yields shown (Y or the Yields button; on by default), the open city's reachable and worked tiles
  show food (green grain) and production (amber hammers) with delivery percentages.
- Escape closes an open city or barracks view; otherwise holding it for a second quits, with a
  "HOLD ESC TO QUIT" bar. F5 toggles borderless fullscreen.

## AI (`ai.rs`)

Each Red unit picks the enemy unit nearest on foot (walking distance around terrain), attacks it
if already in range, and otherwise moves to the reachable hex with the shortest remaining walk,
attacking if that brings it into range. It skips hexes a teammate already claimed, and units in a
contested hex stay and fight. It never uses abilities, never attacks cities, and ignores its
settlers (after founding), workers, and any player-controlled Red unit. Red cities auto-assign
citizens at every end of planning and queue Melee whenever their queue is empty. Ties break by
hex coordinates, so it is deterministic.

## Open questions

- The AI never uses abilities. Deploying its siege on a good hex is the obvious first step.
- The AI never attacks cities or uses workers.
- Cities and barracks can be damaged but not captured.
- Cooldowns tick every turn whether or not the unit acted.
- Contests only form when two enemies arrive in the same step. A later arrival is just blocked; it
  could be allowed to charge in and contest instead.
- A unit locked in a contest still retaliates against third-party melee attackers.
- A move only checks its destination at resolution, not whether its planned path is still open.
- No line of sight: ranged and siege can shoot over mountains.
- Hills cost the same to enter as plains (Civ charges extra movement).
- Swaps only work between adjacent units.
- Horse (move 3) moves in step 9, after every other type except Armored.
- Worker roads and improvements are instant, can overwrite an enemy site, and are not blocked
  while a turn plays out.
- A destroyed Barracks stays in the city's built list, so it can never be rebuilt.
- The Barracks card says "place on a worked tile", but any passable non-city tile is accepted.
- MISS shows for a hit on an empty enemy city or barracks.
- Tab leaves the city view but not the barracks view.
- No victory condition; F1-F3 restart a scenario.
