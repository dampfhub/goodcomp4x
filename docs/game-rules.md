# Game rules as implemented

What the prototype does today, checked against the code. Keep this current: a behavior change
updates this file in the same commit. Design that is not built yet lives in `city-system.md`;
keys are in `controls.md`; screen-space panels in `ui-system.md`. Amounts below are as displayed
(the code stores food, wood, metal and build work in quarter units, so a displayed 12 is 48 in
code). The stockpile economy is an experiment: its reasoning, first-pass numbers and findings are
in `rts-economy.md`.

## Scenarios (`scenario.rs`)

| Key | Scenario | Setup |
|---|---|---|
| F1 | Combat | radius-3 map with a mountain pass; one Melee, Ranged, Cavalry and Siege per side; no cities |
| F2 | Cities (default) | radius-6 map; the Combat units plus a city (with one worker at home), owned farms/mines/pastures and dirt roads per side; Horses and Iron deposits |
| F3 | Frontier | radius-6 map; a settler and a scout per side, no cities. Red's scout is player-controlled, to test route cuts and contests without the AI |
| F4 | World | a generated map (see World generation) for you and 4-6 AI sides, each with a city (or a settler) and a scout; a new random seed every press |
| F12 | Siege | the Cities map with four Blue attackers against two Red gate defenders; the Red city's interior opens for testing |

Pressing F1-F3 restarts that scenario; F4 always makes a new map. The savestate (F6 save, F7
load) holds a copy of the whole game in memory; it survives scenario switches, loading keeps the
snapshot (and the camera, within the same scenario), and saving is refused mid-turn. Loading
doesn't restore the dice: combat rolls carry on from the current game, so a retry can go
differently. Instant
playback (F8, or Instant Playback in the settings menu; on by default) resolves every step of a
turn at once, in the same order, so outcomes don't change. Fog of war (F10) is on by default.
Both settings, and every other player setting (`settings.rs`), survive switches and loads. The faded DEBUG panel (top-left) has buttons for all of these, shows a generated map's
seed, and has FINISH BUILD (F9), which finishes the open city's or barracks' current
build at once: a unit appears if a neighboring hex is open. (Buildings with a site are built by
workers, not the queue.) Two toggles try the economy experiment's alternatives, both
off by default and surviving switches and loads: PROD SPEEDUP, beside fog, where production
speeds builds (see Cities), and UNIT CAP, beside Finish Build, whether the Cavalry and Armored
cap counts those ALIVE (the default) or every one EVER trained (see Barracks).

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

- **Water:** land units cannot enter it. Patrol Galleys, Landing Craft and Bombard Ships move only on water; attacks may cross the shoreline. Cities can work it: a route may end on a water
  tile but never continues across one.
- **Rivers** run along hex edges (World maps only). Land beside a river or a lake has fresh water:
  +1 food, on top of any improvement. A Canoe House makes the connected riverbank a transport
  corridor (see Buildings).
- **Route cost** of entering a hex: 2 (3 on snow or marsh), +1 for hills, +1 for a feature; a road
  or city hex costs 1.
- **Defense** bonuses apply to units only; barracks defense is fixed.
- **Goods:** a tile's food is food; its production splits into **metal**, what's dug out of the
  ground (+1 for hills, +2 for a mine, +3 for a Quarry, never more than the tile's production),
  and **wood**, the rest (plains, tundra and desert ground, forest, jungle, lumber mills,
  pastures). The map's yield pips (wheat, a log, an ingot) and the tile tooltip show food,
  wood and metal.
- **Resources:** Horses and Iron give no yield. Only a Barracks on them trains Cavalry or
  Armored, 3 per deposit (see Barracks); an adjacent Stable or Forge on or beside the matching
  deposit also gives a Barracks that deposit, and upgrades the troop. The Cities scenario places both resources, and every World start has one of each
  nearby.
- **Special tiles** (World maps only; `Special`, `terrain.rs`): land worth scouting for and
  fighting over, marked with a gold rim and an icon in the hex's bottom-left corner. An Orchard
  yields +3 food and a Quarry +3 production (metal) when worked, on top of the tile's own yield and any
  improvement. The kinds and numbers are a first pass.
- **Ruins** (World maps only; `ruins.rs`): a one-use tile, marked with a stone rim and broken
  columns. A side claims ruins by holding their hex with military units (anything but a settler;
  scouts count) at the end of 3 turns; the ruins then give their reward at once and are gone.
  While the hex is contested, or empty, the count pauses; when another side takes it, the count
  starts over for that side. Pips beside the ruins show the count in the holder's color. Each
  ruin has one reward, shown in its tooltip: a Cavalry unit beside the ruins (Recruits), +4 wood
  and 2 metal (Supplies) or +8 food (Harvest) for the claimant's stockpile; a side without a
  city gets the Cavalry. Rewards are claimed before the turn's economy. Out of sight, ruins show as last seen.

## Maps

- **Combat:** mountains at (0,-3), (0,-2), (0,2), (0,3) leave a three-hex pass down the middle;
  hills at (0,0), (-2,2) and (2,-2).
- **Cities:** the same three hills; mountains at (0,±2), (0,±3), (0,±4); Horses at (-2,0) and
  (2,0); Iron at (-2,1) and (2,-1).
- **Frontier:** hills at (-1,2) and (1,-2); mountains at (0,±3).
- **World generation** (`mapgen.rs`): a rectangle whose area grows in proportion
  to the number of sides (about 61 hexes wide by 36 tall per two and a half sides, so about 93 by
  57 for six; never smaller than for three), generated from a `u32` seed
  with its own RNG, so a seed and side count always rebuild the same map (the seed shows in the
  debug panel; there is no way to type one in). A Pangea: 42-52% sea, one continent plus islets of
  at most 12 hexes, mountain ranges and hills by noise, lakes, rivers running downhill to water,
  climate by latitude and moisture, forest on wetter grassland, plains and tundra, jungle on about
  three quarters of marsh.
  - **Sides:** the player (Blue) and 4-6 AI sides, picked by the seed, or as many as the AI Players
    setting (Next World) says (1-6). Each takes a start in `Team::ALL` order (Red, Green, Gold, Purple, Teal,
    Orange), Blue on any of them.
  - **Starts** are on the largest continent, on flat land that is not snow, desert or marsh, with
    open ground and hills next door, scored on nearby yields and fresh water. The set is
    scattered and then evened out: each start about as far from its nearest neighbor as the land
    allows when shared out evenly (some closer, some farther), with about equally good land.
  - **Units:** each side starts with its city already founded (with a worker at home) or with a
    settler, as the Start With setting (Next World) says, and a scout on the neighboring hills. The camera
    starts on Blue's city or settler.
  - **Horses and Iron:** one of each within two to four hexes of every start (farther only if
    there's no room), nearer it than any other start: horses on open flat ground, iron on hills
    or under mountains where there are some.
  - **Ruins and special tiles** go on contested ground: a hex about equally far on foot from the
    two starts nearest it (ruins within a step or so, special tiles within three), well away
    from every start, off the map's edge and reachable from every start. That keeps them out of
    pockets only one side can reach. There are about as many of each as sides, spread apart, and
    special tiles keep clear of ruins.

## Units (`unit.rs`)

| Type | HP | Attack | Defense | Move | Range | Sight | Ability | Icon |
|---|---|---|---|---|---|---|---|---|
| Melee | 100 | 22 | 20 | 1 | 1 | 2 | Shield Wall | stop-sign shield and rebar spear |
| Ranged | 75 | 24 | 10 | 1 | 2 | 2 | Volley | compound bow, drawn |
| Cavalry | 100 | 24 | 14 | 2 | 1 | 3 | Charge | rider on a galloping horse |
| Siege | 65 | 32 | 6 | 1 | 2 | 2 | Deploy | trebuchet with a tyre in its sling |
| Scout | 60 | 8 | 10 | 3 | 1 | 3 | Lookout | bicycle |
| Armored | 140 | 30 | 28 | 1 | 1 | 2 | Shield Wall | riot helmet over a riot shield |
| Patrol Galley | 115 | 23 | 17 | 3 | 1 | 3 | Lookout | rowing boat with oars out |
| Landing Craft | 125 | 8 | 15 | 2 | � | 2 | Lookout | flat river barge |
| Bombard Ship | 105 | 30 | 12 | 2 | 3 | 2 | Lookout | rusted hull with a catapult |

`Unit::stats()` applies abilities and siege deployment on top of these; everything that asks
what a unit can do goes through it. Settlers (a handcart with a flag) are civilians with the Melee body,
drawn as hollow hexagons with only a move badge; every other unit is a team-colored disc with its
pictogram. Settlers can be ordered to attack, though no badge shows it. Workers aren't units:
see Workers below. Cavalry and
Armored are built at a Barracks on Horses or Iron, or supported by an adjacent Stable or Forge;
cities can't queue them. Stable-trained Cavalry get +1 move; Forge-trained Armored get +20% HP
and +15% defense. These upgrades stay with the unit, including in city interiors. A Field
Hospital heals nearby troops (see Buildings). A Harbor in a city whose center touches Coast or Ocean lets it build naval units; a
Landing Craft can carry four land units. Select a land troop and click an adjacent friendly craft
to plan boarding. Select the craft and click adjacent empty land to plan landing its first
passenger. Both happen after combat, so cargo sinks with its ship. A craft cannot attack.
Patrol Galleys fight ships well but deal 35% damage to land troops; Bombard Ships attack from
three hexes. Land melee troops cannot attack ships; Ranged deal 40% and Siege 60% damage to
ships. These attack restrictions apply to direct, group, queued, AI and resolving orders.
Shore and ship attacks do not draw melee retaliation across the waterline.

## Fog of war (`fog.rs`)

- On by default; F10 or the debug panel toggles it.
- The player's units see 2 hexes (Scout and Cavalry 3), +1 on hills, +2 through the turn
  after a Lookout. The player's cities see 3, barracks 1, outposts 2, Watchposts 4 (5 from hills),
  and workers out on the map 1.
- Every tile a player's city works is always in sight, however far and whatever mountains stand
  in the way, so an enemy standing on one is seen. That includes a tile whose citizen a cut route
  moved elsewhere until it reopens (see Cities), so an enemy that ends a turn on a worked tile
  stays in sight. Only the tile itself is seen, not the hexes around it.
- A mountain strictly between two hexes blocks sight; the mountain itself is visible.
- Every frame, each hex in sight is recorded as last seen: cities, barracks (with their
  health), improvements, roads and structures. Units and workers aren't remembered, since they
  move: out of sight, the player knows of none anywhere. Planning goes around the walls and gates
  the player knows of. Remembered hexes out of sight draw that memory under a dark tint, keeping
  the terrain readable. Unexplored tiles lie under a dense cover of muted cumulus: soft-edged
  puffs lit from above, lower billows overlapping the shaded undersides of those behind them,
  over a dark fill so no gaps show. The clouds drift slowly on a steady wind and each puff
  gently billows; puffs fade out as they drift past the map's edge. The Fog setting swaps the
  clouds for a flat grey. The fog is drawn behind the map, so explored terrain simply covers it
  as sight grows. Never-seen terrain and objects are not drawn beneath
  the clouds.
- Enemy units out of sight are hidden, with their ghosts and hover info; tile tooltips describe
  remembered hexes from memory.
- The player plans from what they know: in sight, the board as it is; out of sight, the memory.
  A unit out of sight, even one seen there before, doesn't shrink the move range, and clicking
  its hex plans a move, which then meets it at resolution. A remembered enemy barracks can
  still be attacked; an empty city center cannot.
- What the map and panels show follows the same rule: yields, and which hexes a city's or
  barracks' goods reach (badges, delivery percentages, tooltip), use remembered
  cities and roads out of sight, and no units. A city or barracks shows its live hover panel only if it
  is the player's own or in sight. The economy itself runs on the real board: an enemy out of
  sight on a route hex that isn't a worked tile still cuts the goods behind it, and that shows as
  lost income, a red disrupted-tile ring, and the city view's "NO OPEN ROUTE WITHIN LOGISTICS
  BUDGET" notice, while the enemy itself stays hidden. That much is accepted as fair, like
  finding a tile pillaged.
- The AI ignores the fog.

## Orders (planning)

- Left-click acts on release. Dragging at least 6 pixels from the map draws a selection box
  instead and suppresses the click (see Groups); middle-drag pans. Losing focus or leaving the
  window cancels the gesture.
- **Left click moves, right click attacks.** Click one of your units to select it. Left-click a
  green hex to queue a move; click it again to cancel. Left-clicking a hex with an enemy the
  player can see only says "RIGHT-CLICK TO ATTACK". Left-clicking your own city or barracks hex
  opens its view instead, even with a unit selected, so a left click can't move a unit onto that
  hex or select a unit standing there.
- Right-click any hex in range to queue an attack on it, occupied or not (again to cancel),
  except an empty city center. Units or workers standing on a city center remain attackable.
  **Attacks target hexes:** whoever stands there when the attack resolves gets hit.
- A unit can queue a move and an attack; the attack range is measured from the planned
  destination. Changing or cancelling the move drops an attack that is no longer in range.
- M and X (or the Move and Attack buttons) arm the next map click as a move or attack; with one
  armed, a right-click disarms it.
- Ctrl-click an adjacent ally to swap places (see Swaps).
- Ctrl-right-click clears the selected unit's orders, including its queue, a hold or a guard.
- Shift-left-click and Shift-right-click queue orders for later turns (see Order queues).
- Q (or the ability button) toggles the selected unit's ability.
- **Hold:** Space holds the selected unit if it still needs orders: it keeps what it has queued
  and gives up the rest of its turn. Space (or Hold) on a unit already holding stops the hold,
  keeping it selected and back in the turn order, and any new order to it (a move, attack,
  swap, queued turn) ends the hold too. A group holds all together, or stops holding if every
  member already is. With nothing left waiting, Space ends the turn. With a city or barracks
  view open, Space only closes it.
- **Guard:** G toggles `Unit::guarding`, like holding but lasting across turns, so the unit never
  comes back up in the turn order. Queuing any move, attack or swap wakes it, as do G and
  Ctrl-right-click. Guarding units get a white hex outline.
- **Selection flow:** the first unit needing orders is selected at the start of each turn, and
  once the selected unit is done the next one is selected automatically. "Done" (`needs_orders`)
  means holding, guarding or following an order queue, or having a move queued (or unable to
  move) and an attack queued (or unable to attack). Enemies in range don't matter, since hex attacks are always possible. A unit
  in a contested hex is always done. Selecting a unit by clicking never auto-advances, so a
  finished unit can be reselected to edit. Tab looks at the next unit without holding the current
  one. Whenever the game picks the unit, the camera glides to it; middle-drag cancels the glide.
- Movement is a BFS through passable, unoccupied hexes: units can't pass through each other,
  mountains or, for land troops, water. Ships instead move through water only. Two allies can't head for the same hex.
- A connected Railhead adds its tile as a distant, one-turn move for troops at their city center
  or in its adjacent ring. The move still resolves with normal occupancy and collision rules.
- **Ending the turn:** `pending()` counts player units that still need orders and player cities
  with nothing queued; citizen assignments and idle workers never count. Space selects what is
  still waiting (a city needing a build, then a unit, settlers first, as the turn strip lists
  them) and ends the turn once nothing is. A unit done with its orders moves on the same way, and
  so does the start of every turn and of a new world. The End Turn button (`end_planning`) holds
  every unfinished unit, opens a city if one still needs a build, and otherwise ends the turn.
  Input, including UI clicks, is ignored while a turn plays out.

## Order queues (`order_queue.rs`)

- A unit's plan is a list of turns, each with at most one move and one attack: turn 1 is this
  turn's ordinary orders, later turns wait in its queue (`Unit::queued`).
- **Shift-left-click** adds every turn it takes to get to the clicked hex, starting where the
  plan leaves the unit. Each turn the unit moves to the hex it can reach that turn that is the
  shortest walk from the clicked hex, going around terrain and known walls and gates (the
  straight distance decides if there is no way there; staying put wins ties). Turns are added
  until nobody can get any closer (a queue toward an enemy in sight stops next to it), but no
  unit's plan grows past the **queue limit** setting (6 turns by default, 1 to 20, this turn
  included; see `controls.md`, Settings menu). A hex farther away than that is queued as far
  along the way as the limit allows, and the notice says so ("QUEUED UP TO THE [clock]6 LIMIT").
  A full queue takes no more turns, moves or attacks ("QUEUE FULL - [clock]6 LIMIT"), except an
  attack that fits into its last turn; each turn played frees one. This turn's move plans around the
  units the player can see, like a plain move; later turns plan around terrain and known walls and gates only (units will have moved),
  and never end on an ally's planned hex for that turn, on a hex an enemy in sight stands on, or
  on a hex an ally leaves only in a later step of that turn (see Turn resolution). A click that
  gets nobody closer (the hex already reached, say) queues nothing.
- **Shift-right-click** adds an attack on the hex: into the plan's last turn if nobody who could
  make it from there already attacks that turn, otherwise into a new turn spent standing still.
  Range counts from where the plan has the unit that turn, with its later-turn stats (no ability;
  a siege that sets up this turn is deployed). With nothing planned and no attack possible this
  turn (a siege setting up), it goes in the next turn. Out of range, nothing is queued.
- **Groups:** a Shift-click with a group selected queues for every member, and afterwards
  their plans all have the same number of turns. For a move, each member continues from the end
  of its own plan: one with a shorter plan (say, just added to the selection) starts moving at
  once, and turns already queued stay as they were. Members that arrive first or can't get
  closer wait at the end, the members nearest the target choose their hexes first, and no
  member ends a turn where another member's plan has it. A move keeps adding turns until no
  member can get any closer, with the limit counted for each member from the end of its own
  plan: a member that had three turns queued, with a limit of 6, ends up with nine, and one
  that had none moves for six turns and then waits for it. An attack goes in at the end of the group's plans, which members
  with shorter plans reach by waiting.
- Queuing never moves selection on, so a unit (or group) can be given several turns in a row.
- **Not holding up the turn:** a unit following a queue counts as done (`needs_orders`), this
  turn and every turn it has queued orders for.
- **Cancelling:** any other order to the unit (a plain move or attack, a group move or attack, a
  swap, its ability, Guard, Ctrl-right-click) drops its queue. Hold keeps it. This turn's orders
  stay, so the unit needs orders again unless the new order completes them. A map click that
  would replace a queue reaching past this turn (a plain move, attack or swap, for a unit or a
  group) needs the same click twice: the first only warns and outlines the hex, and any other
  click, a new selection or the turn ending forgets it (`confirm_queue_replace`).
- **Carrying over:** at the end of the turn, after each unit's `end_turn`, every unit with a
  queue takes its next turn's orders (`advance_queues`). The whole queue is dropped, with a
  notice ("MELEE STOPPED: ..."), and the unit needs orders, if the turn no longer fits: the unit
  isn't where the queue expected (a move was blocked), it's in a contested hex, its way or
  destination is blocked by terrain or a known wall, an enemy it can see or an ally stands on
  the destination, or its target is out of range.
- The savestate (F6/F7) keeps queues, like every other order.

## Turn resolution (`turn.rs`)

After a 0.6 s pause (so the last order is visible), the turn plays out in 13 steps, one every
0.6 s, with the acting units flashing (or all at once with instant playback). Steps where nobody
acts are skipped.

1. Scout move  2. Cavalry move  3. Melee move  4. Ranged attack  5. Scout attack
6. Cavalry attack  7. Melee attack  8. Ranged move  9. Siege move  10. Siege attack
11. Armored move  12. Armored attack  13. Workers

Workers go last, after every unit has acted, so they're exposed (see Workers).

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
- After the last unit and worker step: Coastal Batteries fire, then landing craft unload and
  board their passengers; city interior battles and city economy (income, growth, builds) follow.
  Then each unit's end of turn
  (ability cooldown, siege setup, Lookout, orders cleared), then units with an order queue take
  their next turn's orders (see Order queues).

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
- Clicking again, giving either unit a new move, or Ctrl-right-clicking cancels both halves. Not
  allowed for units in a contested hex or units that can't move. A swap drops both units' order
  queues.

## Groups (`group.rs`)

- Left-drag a box to select your units drawn inside it (Shift-drag adds them to the
  selection); Shift-click adds one unit, and Ctrl-click takes a member back out. Two or more
  become the group; one is an ordinary selection. The group's hexes are highlighted and the tray
  summarizes it, with a Clear Orders button that drops every member's orders and queues (as
  Ctrl-right-click does).
- The turn strip (`ui/roster.rs`, a panel starting at the bottom center) lists what the
  player still has to see to this turn, civilian tasks first: cities with an empty queue,
  settlers, then military units needing orders.
  Units are grouped by kind (settlers apart), each group in the order its first unit comes in
  unit order, with a count. Clicking a city's chip opens it. Clicking a group selects all its
  units and moves the camera to the first, and while any of them is selected, a second row
  lists them one by one. Clicking a unit's chip selects just it; Shift-click adds a chip's units
  to the selection and Ctrl-click takes them out (leaving at least one). Selected units and the
  open city are framed. Research will join it when there is any.
- Left-clicking a hex (or Move) converges: members' old moves are dropped, then, nearest to the
  target first, each takes the reachable hex closest to the target that no ally is heading for,
  staying put if it can't get closer. Members keep their own speeds, so the group doesn't hold
  formation. An enemy's hex is moved toward like any other.
- Right-clicking a hex (or Attack) has every member that can reach the hex attack it; clicking a
  target they all already attack calls it off.
- Either drops every member's order queue. Shift-clicks queue turns for the whole group instead
  (see Order queues).
- Space/Hold holds every member, G guards them all (or unguards if all are), Ctrl-right-click
  clears their orders, clicking one member selects just it.
- The group is cleared when a turn resolves.

## Attack display (`draw.rs`, `effects.rs`)

- Queued attacks are curved arrows from the attacker (or its ghost, if it moves first) to just
  short of the target. Only the player's own attacks get arrows; the AI's plans stay hidden.
- A unit whose order queue reaches past this turn shows its whole plan only while it is
  selected or hovered: moves as a line with each turn's number, ending in the unit's ghost where
  the plan leaves it, and attacks as arrows numbered by turn. Each unit's plan is drawn on its
  own; where several stop on one hex their numbers fan out rather than merge. Otherwise a tag
  with the clock and a number counts its turns of orders left. A queue of this turn alone draws like plain
  orders: a ghost and an attack arrow, no numbers.
- When an attack resolves, the arrow shoots from attacker to target, then shows a burst on a hit,
  "MISS" on a hex with no enemy unit, worker or barracks, or "OUT OF RANGE" if the target moved
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
- **Cities** cannot be attacked on the exterior map; capture happens by breaching and occupying
  the command post inside. **Barracks** have 220 HP and 25 defense and are removed (with their queue) at 0 HP; the city
  can then build a new one, at full HP. A structure is hit only when the attack hits no enemy unit
  at all (for a Volley, none on the target or its neighbors); Volley's 60% applies to structures
  too.

## City interiors (`city/interior.rs`)

- While in a city view, click that city's center hex to enter its separate tactical map. V also
  opens the selected or hovered city's interior. Each city has a 19-hex grid (radius 2) with a
  fixed command post at the center. Escape or V returns to the city view.
- Each combat unit on one of the six exterior hexes neighboring that city projects a separate
  fighter through the corresponding outer gate. Settlers and workers do not project. The copy
  has its own position and orders. Exterior and interior HP are separate: nonfatal damage in
  either layer leaves the other HP bar unchanged, and interior HP persists if the unit leaves and
  re-enters a gate. If either HP bar reaches zero, the unit dies in both layers. Moving away
  removes its interior presence until it returns.
- Units cannot move onto an enemy city center on the exterior map; they fight for the six
  surrounding gates and capture the command post inside. A unit already standing on a city center
  can still be attacked there.
- Click a Blue copy on the interior map, then click an open hex to queue its move or an enemy
  to queue an attack. The map outlines valid moves in green and attacks in red. Range uses that
  unit type's move and attack stats. Click the command post to attack it when within range.
  Backspace clears the selected copy's orders.
  Field orders and interior orders resolve independently in the same global turn: field combat
  first, then one simultaneous interior move/attack step for every city, then city economy.
- The post starts with 80 HP, 18 defense and a range-2 retaliation against one attacker
  per turn at attack 12. Damage follows the normal combat formula. Once its HP reaches zero, move
  a hostile interior fighter onto the center hex to capture the city. Capture changes ownership,
  restores the post and clears its production queue, building
  plans and worker jobs. Workers at home pass to the new owner. Field workers from that city
  return to another friendly city if one exists; otherwise they pass to the new owner.
  A worker sharing its hex with a new enemy is captured immediately after the ownership change.
  The exterior city remains non-attackable.
  A breached post changes its map marker and city panel to show that it is open for occupation.
- Red's interior fighters defend their own city's post by engaging intruders. When attacking an
  enemy city they head toward its post. Exterior Red defenders hold contested city gates and can
  attack nearby enemies, so their projected copies stay in the siege.
- F12 starts a siege practice position with four Blue troops at the Red city's gates (including
  a siege unit) against two Red defenders and the post. Order Blue attacks on the exterior as well
  as inside; without field orders, the defenders can kill gate troops before the post is breached.

## Cities (`city/`)

- **Founding:** F with a selected settler, at least 3 hexes from any other city; the new city
  starts at population 1, auto-assigns and opens. The AI founds a city in place, at the start of
  any resolution where it has a settler and no city, without the 3-hex rule.
- **Yields:** the city center gives 2 food and 1 wood on its own; each worked tile gives its
  food, wood and metal (see Goods) times its delivery share. No citizen works a city center or a
  tile a placed building stands on (`closed_to_citizens`): such a tile can't be assigned or take
  the manager, and a citizen already there moves off. Improvements (built by workers, see Workers): a mine on
  hills (+2 production), a lumber mill under forest or jungle (+1 production), otherwise a farm (+2
  food); snow can't be improved. The Cities scenario's preplaced farms (4/0), mines (0/4) and
  pastures (3/1) have fixed yields.
- **Logistics:** each worked tile's goods travel its cheapest route to the city, summing the
  route costs above; routes costing more than 8 don't exist. Delivery is 100% at cost 0-2, 75% at
  3-4, 50% at 5-6, 25% at 7-8. Enemy units, contested hexes, enemy cities and mountains block
  routes, and an enemy on the city blocks them all. Routes are recalculated every time they're
  used.
- **Manager and workers:** population is at most 7: the manager (the first worked tile, ringed in
  gold and marked `M`, which must be land) plus up to six workers, each adjacent to the manager.
  A worked tile belongs to only one city. To move the
  manager, click it to pick it up (its workers leave the map with it), then click its
  destination; workers keep their offsets where they can and are otherwise replaced by the best
  nearby tiles. Clicking the manager again puts it and its workers back.
- **Citizens:** click tiles to assign or release; A auto-assigns by the city's labor focus: Food,
  Wood or Metal, each favoring tiles that deliver the most of it, or Balanced (the default), which
  picks food tiles until the city's food income covers upkeep plus 1, then wood and metal alike. Setting a focus
  re-assigns. On growth or route disruption, reconciliation keeps valid manual assignments and
  fills or replaces the affected slot; a manual tile cut off by an enemy is remembered and returns
  when the route reopens, unless you changed it.
- **Stockpile** (`city/economy.rs`): each side has one store of food, wood and metal (top bar,
  with its change a turn), not one per city. Every city's delivered goods go into it at the
  turn's economy. A side starts with 10 food, 10 wood and 4 metal.
- **Food and upkeep:** each citizen eats 2 food a turn from the stockpile, so one city's farms can
  feed another. If the stockpile can't feed all of a side's citizens, its food empties and the
  side's largest city (the first on ties) loses a citizen (never below 1).
- **Growth** is bought: Grow (9, or the city tray's Grow card) queues one more citizen for
  5 + 5 × population food (counting the Grows already queued ahead of it), taking 2 turns in
  the city queue like any build. Nothing grows by itself, and no Grow goes past the cap of 7.
- **Paying and the queue:** a build is paid in full from the stockpile when it is queued; a card
  the side can't afford is dimmed, and its tooltip (or the notice, for a key) says what the side
  is short of. Taking an item out of a queue (its X, Backspace, or a building dropped for want of
  a site) refunds its full price (a Grow refunds the dearest queued Grow's). A captured city's
  queues and a destroyed Barracks' queue are lost, unrefunded. Each build then takes a fixed
  number of turns at the head of its queue: the queue's progress gains a turn's work each
  economy, and resets to 0 on an empty queue, when the first item is queued, and when the head
  is removed. Reordering keeps the progress, so it moves to the new head. Drag a row to reorder, click its X to remove;
  Backspace removes the head and PageDown swaps the first two. A city finishes at most one item
  a turn. A finished unit appears on an open neighboring hex (not one another unit is appearing on
  that turn). With no hex open, the city holds the unit until one opens, and banks no work for the
  rest of the queue meanwhile. A player city with an empty queue holds up the turn, since it can
  always Gather.
- **Gather** (0, or its card beside Grow): free, one turn; when it's done, the side's stockpile
  gets 2 food, 2 wood and 1 metal. A city that can't pay for anything, or has nothing it wants,
  gathers instead of standing idle.
- **Production speeds builds** (the Debug panel's PROD SPEEDUP, off by default): a city's queue
  also gains a quarter turn of work a turn for each point of production (wood and metal) the city
  delivers, and a Barracks for each point delivered to it; the stockpile still gets those goods.
- **Prices and turns** (food / wood / metal, turns at a Barracks): Melee 2/6/0, 2; Ranged
  2/7/0, 2; Cavalry 3/4/3, 3; Siege 1/8/4, 3; Armored 3/2/7, 3; Patrol Galley 1/10/2, 3; Landing
  Craft 1/12/2, 4; Bombard Ship 1/12/6, 4; Worker 4/2/0, 2; Grow as above, 2; Gather free, 1. A city center
  trains land troops at half a Barracks' pace (twice the turns: a Melee takes 4); ships, which
  only a city with a Harbor builds, take their own turns. Barracks 0/10/0,
  3; Mill, Canoe House and Watchpost 0/10/0, 3; Workshop 0/10/4, 4; Forge 0/6/8, 4; Stable
  2/12/0, 4; Field Hospital 4/10/4, 4; Cannery 0/12/4, 4; Work Camp 2/10/2, 3; Smelter 0/8/8, 4;
  Railhead 0/12/12, 5; Harbor 0/14/0, 4; Coastal Battery 0/8/10, 4. Cards, tooltips, queue rows
  and notices show a price as each resource's icon and amount (the map's wheat, log and ingot)
  and the turns after a clock icon. Keys 1-3 queue Melee, Ranged and Siege (a city can't queue
  Cavalry or Armored), 5-7 pick a Barracks, Mill or Workshop to place, 8 a Worker, 9 a Grow
  and 0 a Gather. The rest are in the city's scrollable production list. One of each building per
  city.
- **Buildings:**
  - **Every building** stands on a site, and the city's workers build them there
    like any other job (see Workers): its card picks it to place, a click on a lit tile within
    workers' reach places it (passable land you have explored, not a city, building or
    structure, and no other job on the tile), paying its price then, and a worker walks out
    and builds it over its turns. It never enters the city queue. A site click that breaks a
    building's own rule explains it (such as Horses for a Stable, Iron for a Forge, or a riverbank
    for a Canoe House) and keeps it picked. One placed can't be placed again until it's done or
    taken off the city's job list (refunded).
  - **Barracks** (`city/barracks.rs`): the side's military building. Its own view and queue
    (Melee, Ranged, Cavalry, Siege, Armored), paid from the stockpile like the city's, training
    twice as fast as a city center, wherever the city's manager is (with production speeding
    builds, the manager beside the barracks adds its worked tiles' production, times their delivery
    share from the barracks). Only a Barracks trains Cavalry and Armored, and only one drawing
    on a deposit: Horses or Iron under it, or on or beside a Stable or Forge next to it. Each
    deposit a side's Barracks draw on allows 3 of that troop, counting those alive and queued, so
    a lost one can be replaced (the Debug panel's UNIT CAP: EVER counts every one ever trained
    instead, so a deposit runs out). A deposit an enemy unit stands on counts for nothing while
    it's there. The barracks panel shows each deposit kind's troops left, or why they're locked,
    and a locked or unaffordable card is dimmed with the reason in its tooltip. Troops from the
    ruins don't count. A unit appears next to the barracks (never on a worker out on the map),
    and the progress resets after each.
  - **Mill:** worked tiles adjacent to it deliver all their food, if they can reach the city.
  - **Workshop:** a building placed on a site adjacent to one of its side's workshops takes its
    worker half the turns (rounded up; its price is unchanged).
  - **Canoe House:** must stand on a riverbank. Connected riverbank hexes act like roads for
    friendly delivery routes (cost 1 between banks); walls, enemy occupation and the 8-cost
    delivery limit still apply. This can bring several remote tiles into a city's reach at once.
  - **Forge and Stable:** must stand on or adjacent to Iron or Horses respectively. If also
    adjacent to a Barracks, they give it the matching deposits on or beside them (and their
    cap), even when the Barracks is off the resource. Forge-trained Armored have +20% HP and +15% defense; Stable-trained Cavalry
    have +1 move. Units trained before the building was placed retain their original stats.
  - **Watchpost:** sees 4 hexes, or 5 from hills, through ordinary sight lines. It does not
    need a worker, unlike an outpost.
  - **Field Hospital:** after each turn's economy, heals the two most injured friendly units
    within 2 hexes by 20 HP in both their field and city-interior health bars. Each unit can
    receive this healing only once per turn even if two hospitals overlap.
  - **Cannery:** collects food from up to three owned, unworked improved sites within 3 hexes,
    even outside city delivery range. Those sites need a local open route to the Cannery, but
    no onward route to the city. The food share is 100% at distance 1, 75% at 2, and 50% at 3;
    only the three highest-yield eligible sites contribute. Enemy occupation cuts delivery.
  - **Work Camp:** workers assigned jobs within 3 hexes start from and return to this building
    while it has a delivery route to its city. They still walk, work and face capture normally;
    Recall sends a worker to the city center. If the route is cut, new assignments start at the
    city and returning workers head there instead. It uses the city's existing worker job list.
  - **Smelter:** must stand on or beside hills or Iron. It collects production, as metal, from up to three
    owned, unworked mines within 3 hexes using the same local-route and 100/75/50% distance
    rules as the Cannery, even beyond city delivery range. It feeds the city queue, not Barracks.
  - **Harbor:** only in a city whose center touches Coast or Ocean; placed on land next to sea water. It unlocks all three ships in the city queue and
    spawns them onto an open neighboring water tile. Ships take their turns in the city queue;
    a finished one waits when every adjacent water tile is occupied.
  - Finished units and ruin recruits never spawn onto an enemy city center or a field worker.
    A finished build waits until a safe neighboring tile opens.
  - **Coastal Battery:** only in a city whose center touches Coast or Ocean; placed on land next to sea water. It automatically attacks the nearest
    hostile ship within 2 hexes after unit combat, dealing a 28-attack strike. It has 150 HP,
    can be bombarded and rebuilt if destroyed. Its health bar appears over its badge.
  - **Railhead:** the city center acts as its origin terminal, so no second building is needed
    beside the city. An unbroken chain of roads from city center to Railhead lets a friendly
    land unit on the center or an adjacent hex move directly to the Railhead in one turn. The
    Railhead is the only destination; it must be unoccupied. Walls, gates and enemy occupation
    can cut the link, including after an order was planned. The prototype uses existing roads
    as the rail corridor rather than adding separate track jobs.
- **Worker** (8; 4 food, 2 wood, 2 turns): adds a worker to the city's pool (see Workers).
- Economy runs once per turn, after the workers' step: income into the stockpile, upkeep, a
  turn's work on each queue, then finished builds.

## Workers (`workers.rs`)

- **Pool:** each city keeps its workers at home, off the map, where nothing can touch them. A new
  city starts with one; the city queue builds more (8, for 4 food and 2 wood). A tag on each of your cities counts the
  workers at home. A connected Work Camp can be the departure and return point for nearby jobs;
  the worker returns to the same city pool.
- **Placing** (`workers.rs`): everything a worker builds is placed from its city's production
  list, under WORKS (Road, Improve, Wall, Gate, Outpost, Fort; R and I pick roads and
  improvements) and BUILDINGS (those with a site). A city needs a worker, at home or out, to place
  anything, and its side must pay the price, taken when placed: Road 0/2/0, Improve 0/4/0, Wall
  0/3/0, Gate 0/3/2, Outpost 0/6/0, Fort 0/8/4 (food / wood / metal), and a building its own
  price. Picking one lights the tiles workers can reach and dims the rest; then click or drag
  over tiles, or along hex edges for walls and gates. A ring under the cursor shows where a tile
  job would go (red where it can't); a work stays picked for more until Escape, picking it again,
  or leaving the city, and a building is done once placed. The city panel lists its workers
  (click one to show it on the map; Recall beside it) and its placed jobs (click to show on the
  map, drag to reorder, X to take one off for a refund).
- **Jobs:** a job waits in its city's list. A job a worker is out
  on shows as a bright gold ring (or edge) named with the job, and once the worker is at work
  with the turns left, like FARM [clock]2 (an improvement is named for what it becomes: farm, mine
  or lumber mill; a building by its name). The city panel names jobs the same way, with the tile:
  FARM · GRASSLAND · [clock]3. Placed jobs show on the map as faded gold rings (walls and gates as muted gold edges with
  rounded ends). A job needs explored open ground within workers' reach, no city there, and no
  other job on the tile, queued or under way: a tile takes one job at a time (walls and gates,
  on its edges, aside). Improvements and outposts or forts also can't go on a placed building or
  on another improvement, and an outpost or fort can't go on another one. A job dropped or
  abandoned before it's done (it became impossible, or its worker can't reach it) is refunded.
- **Reach:** workers go up to 3 tiles from one of their side's cities or Work Camps (a camp
  counts once connected to its city), or anywhere on or next to a road (any road: roads belong to no one), so a line of roads carries the reach out as far
  as it goes. A wall or gate counts from the tile the worker stands on to build it. The AI's
  workers keep to the same reach. A job out of reach can't be queued, and one under way that
  falls out of reach is abandoned. The player-facing reach tint and job checks use
  current sight or the last observed roads, sites, and structures; unseen enemy changes
  take effect during resolution without revealing themselves while planning (a building placed
  on a site an unseen building took is dropped, refunded, when its worker gets there).
- **Idle workers** (at home, with no job in their city's list) wait there, and never hold up
  the turn.
- **Walls and gates** stand on the edge between two hexes, not on a tile. Picked in the city's
  production list, each click on the map places one on the hex edge nearest the cursor (highlighted), and
  dragging places one on every edge the cursor passes.
  The worker builds it standing on whichever side is nearer its city (open, explored ground).
  One wall or gate per edge; an edge beside a city is fine.

  | Job | Work | Effect |
  |---|---|---|
  | Road | 2 turns | a dirt road |
  | Improve | 3 turns | a mine, lumber mill or farm (see Yields) |
  | Wall | 2 turns | on an edge: no unit, worker or goods cross it, yours included |
  | Gate | 3 turns | on an edge: only your units, workers and goods cross it |
  | Outpost | 3 turns | you see 2 hexes around it |
  | Fort | 4 turns | your units in it get +50% defense (a placeholder) |
  | A building | its turns (halved beside a Workshop) | the building, on its site |

- **Going out:** in the Workers step, each city sends an idle worker out for each job at the top
  of its list. A worker walks 1 hex a turn by the shortest way around impassable terrain,
  walls, others' gates, enemy units and enemy cities. Once on the tile it works the listed turns
  (starting the next turn), then takes the city's next job, or walks home when there is none. A
  job that became impossible is dropped (with a notice); a worker that can't reach its job gives
  up and heads home.
- **Danger:** out on the map a worker can be seen (your own see 1 hex around them). An enemy unit
  that moves onto its hex captures it: it joins the captor's nearest city (or is lost if the
  captor has none). An attack on its hex kills it when nothing else is there to hit. A unit
  standing on the same hex shields it from both, since it blocks the move and takes the hit.
  A captured or killed worker's job goes back to the top of its city's list.
- **Recall:** workers otherwise follow their jobs on their own, so each of your workers out on
  the map has a Recall button in its city's panel. A recalled worker drops its job (back to the top of the city's list) and walks straight
  home at its usual 1 hex a turn in the Workers step, taking no new job on the way.
- **Structures** are never destroyed or captured yet. Units plan moves around the walls and gates
  they know of; a move whose way is blocked by one (say, one not seen when it was planned), with
  no way around within the unit's move, is turned back at resolution. The fog remembers them like
  improvements.

## Interface (`src/game/ui/`)

- Panels dock in four corner zones and never overlap (`docs/ui-system.md`).
- **Top bar:** turn number, your stockpile (food, wood and metal, by icon, each with its change a turn:
  every city's delivery, less the citizens' food), the latest notice, and the End Turn button, whose label names what is
  still waiting ("3 UNITS NEED ORDERS", "CHOOSE PRODUCTION") until it turns gold and reads END
  TURN.
- **Command tray** (bottom-left): with a city open, it shows population, what the city delivers
  and its citizens eat, the current build and its turns left, labor focus buttons, the Grow card
  (9), unit cards (1-3) and the Worker card (8), each with its price and turns and dimmed when the
  stockpile can't pay, the Yields button,
  the production list (units, buildings not yet built, and works for its workers, each with its
  price and turns), what the barracks is training with See Barracks, and its workers and placed
  jobs; the queue docks above it. With a barracks open, what it stands on, each deposit
  kind's Cavalry or Armored left (or why none), its five train cards and Open City, queue above. With a unit selected: stats (boosted values green, reduced
  red), notes, and buttons Move, Attack, Swap, then its ability (or Found City), then Hold,
  Guard and Disband (press twice: the first press asks to confirm). Move, Attack and Swap arm the next map click only (a held modifier overrides it);
  pressing the button again or right-clicking disarms. The armed button has a bright border, a
  queued order turns its button gold, an unusable one is dimmed. Every button has a hover tooltip.
- **Hover:** hovering a unit shows its stats at the top-right; hovering a city or barracks shows
  a structure panel at the bottom-left instead (barracks HP; a city's population and what it delivers;
  the current build and its turns left). Hovering a city also outlines its worked tiles, without yield badges. After the
  cursor rests on a hex for 0.75 s, a tooltip shows terrain or city, yields, defense, site, road,
  which city works it, its delivery share to the open city, and units on it.
- **City view:** C opens the first city needing a build (or your first city), and left-clicking
  your city opens that one. While open, map clicks manage tiles and never select units; it closes
  on Tab, Space, Escape, or a click off the map. A barracks view closes the same way; C switches
  it to its city. Worked tiles are outlined green (the manager's in gold; red if disrupted).
  Hovering the manager draws a dotted line along its goods' route to the city: the cheapest
  route, as you know the board. With yields shown (Y or the Yields button; on by default), the open city's reachable and worked tiles show
  food (wheat), wood (a log) and metal (an ingot) with delivery percentages. Alt shows every
  explored tile's yields, and while something is being placed with yields off, the open city's
  delivery percentages too. A tile a building stands
  on shows neither (a city center keeps its yields); a job's name on a tile showing a percentage
  sits just under it.
- Escape closes the settings menu, or else an open city or barracks view, or else lets go of the
  selected unit or group (something being placed for a city's workers stops first);
  with none of those open, it opens the
  settings menu, whose Quit button closes the game. F5 toggles borderless fullscreen.

## AI (`ai.rs`)

Every side but Blue is played by the AI, in `Team::ALL` order, and every side is at war with
every other. Each AI unit picks the enemy unit or worker, or the unclaimed ruins no unit of its
side holds or is heading for, nearest on foot (walking distance around terrain, walls and others'
gates). It steps onto ruins it can reach, and then holds them until they're claimed, attacking
enemies in range from there. It steps onto a worker it can reach this turn, capturing it; otherwise
it attacks its target if already in range, or moves to the reachable hex with the shortest
remaining walk, attacking if that brings it into range. It skips hexes a teammate already
claimed, and units in a contested hex stay and fight. It never uses abilities, never attacks
cities, never builds buildings, ignores the fog, and ignores its civilians and any
player-controlled AI unit; its scouts fight like any other unit. AI cities auto-assign citizens
at every end of planning. AI queues buy from their side's stockpile, or wait a turn if it
can't pay. An AI city with an empty queue trains a worker first when it has none. With a worker,
it places a Barracks for its workers to build, paid like the player's: on a Horses or Iron
deposit within 3 hexes (a kind it has none of first), else on the nearest open unworked tile
within 2. Its queue otherwise grows the city. A city without a Barracks trains Melee itself,
slowly, until the side has 2 units (scouts and settlers aside) per city, growing when it can't
pay. An idle AI Barracks trains Cavalry or Armored when its deposits allow and the side can pay,
else Melee, or Ranged for every two Melee. An AI city with a worker at home and an empty list places one job it can
pay for: an improvement on a tile it works, or else a road there. At a contested friendly city gate, AI
units hold position and attack an enemy in range. Ties break by hex coordinates, so it is
deterministic.

## Open questions

Known bugs link to their board item; the rest are design questions nobody has decided yet.

- The AI never uses abilities, attacks cities or builds buildings or structures; it ignores the
  fog, and its scouts just fight.
- Forts' +50% defense is a placeholder; what forts should really give is undecided.
- Structures can't be destroyed or captured, and a wall or gate can go on any edge next to explored
  ground.
- Barracks can be damaged but not captured, and nothing heals.
- Cooldowns tick every turn whether or not the unit acted.
- Contests only form when two enemies arrive in the same step. A later arrival is just blocked; it
  could be allowed to charge in and contest instead.
- A unit locked in a contest still retaliates against third-party melee attackers.
- A move only checks its destination at resolution, not whether its planned path is still open,
  so a move planned through an unseen enemy passes it.
- Hills and forest cost the same to enter as plains, and rivers don't slow or penalize crossing
  units.
- Ruin rewards, special tile kinds and their numbers are placeholders. Every side is at war with
  every other; there's no diplomacy, and the AI sides fight each other as readily as Blue.
- Swaps only work between adjacent units.
- An order queue only stops for an enemy standing on its next destination (or blocking the
  move); it doesn't stop when an enemy merely comes into sight, and it can't queue abilities,
  swaps or holds for later turns.
- The stockpile economy's prices, times, growth cost and the Barracks' 3 troops per deposit are
  a first pass, and the late game has nothing to spend a growing stockpile on once cities are
  full (see `rts-economy.md`).
- No victory condition; F1-F4 restart a scenario. The Debug panel offers a Naval scenario
  with two coastal cities, prebuilt Harbors and Coastal Batteries, and ships ready to fight.
