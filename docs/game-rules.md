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
| F4 | World | a generated map (see World generation) for you and 4-6 AI sides, each with a city (or a settler) and a scout, and animal dens (see Animals); a new random seed every press |
| F12 | Siege | the Cities map with four Blue attackers against two Red gate defenders; the Red city's interior opens for testing |

Pressing F1-F3 restarts that scenario; F4 always makes a new map. The savestate (F6 save, F7
load) holds a copy of the whole game in memory; it survives scenario switches, loading keeps the
snapshot, and saving is refused mid-turn. Loading opens the view the snapshot was in (the map or
a city's interior) and keeps the camera where it is on the same map (the same scenario, and for
F4 the same world); a snapshot of another map brings its own camera. Loading
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
| Snow | 0 | 0 | can't be improved |
| Marsh | 1 | 0 | never hills |
| Mountains | 0 | 0 | impassable to units and routes; can't be worked or targeted |
| Coast, Lake | 2 | 0 | water |
| Ocean | 1 | 0 | water, away from the shore |

| Modifier | Effect |
|---|---|
| Hills | +1 production, +25% defense, +1 sight |
| Forest | -1 food (not below 0), +1 production, +15% defense |
| Jungle (marsh only) | +1 food, +1 production, +15% defense |

- **Water:** land units cannot enter it. Patrol Galleys, Landing Craft and Bombard Ships move only on water; attacks may cross the shoreline. Cities can work it: a route may end on a water
  tile but never continues across one.
- **Rivers** run along hex edges (World maps only). Land beside a river or a lake has fresh water:
  +1 food, on top of any improvement. A Canoe House makes the connected riverbank a transport
  corridor (see Buildings).
- **Terrain and goods:** terrain never slows goods. A delivery route counts only the hexes it
  crosses, a road step as half a hex (see Cities, Logistics).
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
- **World generation** (`mapgen.rs`): a rectangle of about 1,200 hexes per side, so about 111
  hexes wide by 67 tall for six (never smaller than for three). That's room on each side's share
  of the land for about 8 decent city sites 8 hexes apart, so a side fits 3-5 cities
  comfortably even with its neighbors contesting some, and more where the land is good.
  Generated from a `u32` seed
  with its own RNG, so a seed and side count always rebuild the same map, on every machine (the
  seed shows in the debug panel; `--seed N` on the command line picks one). A Pangea: 42-52%
  sea, one continent plus islets of at most 12 hexes, built in stages:
  - **Mountains** are about 4% of the land, in ranges: short chains one hex wide where plates of
    crust meet (the map is split among a dozen or more warped plates, and about two in three of
    their borders rise). A ridge wears down unevenly, so a border rises as several short ranges
    with gaps between (most under 10 hexes long). A range stays a hex back from the shore and has
    an open hex (a pass, on hills) about one in five; where three ranges meet, the hex they meet
    on is a pass too. A lone peak or two may stand apart.
    Mountains never wall land off: if a range cuts off a stretch of open land, the mountains on
    the shortest way out become hills.
  - **Hills** are 12-17% of the land: foothills beside about half of the mountain hexes' open
    neighbors, the rest in rolling uplands, more often well inland.
  - **Lakes:** small bodies of water cut off from the map's edge, plus a few lakes of one to three
    hexes well inland.
  - **Rivers** run along hex edges, always down to the lowest next corner (the ground rises with
    the steps to the sea, and more under hills and mountains). Each rises at a lake (at most one
    river leaves a lake) or in the mountains and their foothills, and ends at the sea, in a lake,
    or where it joins another river: rivers merge but never split, never run back into the lake
    they left (directly or through other rivers and lakes), and are at most about half the map's
    width long. A river stuck in a dip ends in a pool there, a new lake of one hex, where one
    fits (on open land ringed by open land, by no other river). About one per 45 hexes of land,
    rising at least three hexes apart.
  - **Climate:** colder toward the top and bottom of the map and beside mountains; wetter by
    fresh water and the sea, drier far inland. Snow, tundra, desert, marsh, grassland or plains
    follow; forest grows on wetter grassland, plains and tundra, and jungle on most marsh, both
    in patches.
  - **Sides:** the player (Blue) and 4-6 AI sides, picked by the seed, or as many as the AI Players
    setting (Next World) says (1-6). Each takes a start in `Team::ALL` order (Red, Green, Gold, Purple, Teal,
    Orange), Blue on any of them.
  - **Starts** are on the largest continent, on flat land that is not snow, desert or marsh, with
    open ground and hills next door, scored on nearby yields and fresh water. The set is
    scattered and then evened out: each start about as far from its nearest neighbor as the land
    allows when shared out evenly (some closer, some farther), with about equally good land, and
    the nearest neighbors about as far on foot too, around the ranges (the farthest at most 1.6
    times the closest, where some tried set allows it). Every start can walk to every other.
  - **Units:** each side starts with its city already founded (with a worker at home) or with a
    settler, as the Start With setting (Next World) says, and a scout on the neighboring hills. The camera
    starts on Blue's city or settler.
  - **Horses and Iron:** one of each within two to four hexes of every start (farther only if
    there's no room), nearer it than any other start, on the best ground for it nearby: horses on
    open flat grassland or plains, else open tundra; iron on foothills (hills beside mountains),
    else other hills or ground under mountains.
  - **Ruins and special tiles** go on contested ground: a hex about equally far on foot from the
    two starts nearest it (ruins within a step or so, special tiles within three), well away
    from every start, off the map's edge and reachable from every start. That keeps them out of
    pockets only one side can reach. There are about as many of each as sides, spread apart, and
    special tiles keep clear of ruins.
  - **Animal dens** (see Animals) go on forest, jungle or hills, at least 5 hexes from every
    start, off the map's edge, reachable from every start, clear of resources, special tiles and
    ruins, and at least 6 hexes apart: two a side at most, in a random order, of which a world
    takes as many as the Animals setting (Next World) says: none, one a side (the default) or
    two, the first of them, wolves and bears in turn.

## Units (`unit.rs`)

| Type | HP | Attack | Defense | Move | Range | Sight | Ability | Icon |
|---|---|---|---|---|---|---|---|---|
| Melee | 100 | 22 | 20 | 1 | 1 | 2 | Shield Wall | sword |
| Ranged | 75 | 24 | 10 | 1 | 2 | 2 | Volley | bow and arrow |
| Cavalry | 100 | 24 | 14 | 2 | 1 | 3 | Charge | horse head |
| Siege | 65 | 32 | 6 | 1 | 2 | 2 | Deploy | catapult |
| Scout | 60 | 8 | 10 | 3 | 1 | 3 | Lookout | spyglass |
| Armored | 140 | 30 | 28 | 1 | 1 | 2 | Shield Wall | heater shield |
| Patrol Galley | 115 | 23 | 17 | 3 | 1 | 3 | Lookout | sailboat |
| Landing Craft | 125 | 8 | 15 | 2 | - | 2 | Lookout | cargo boat |
| Bombard Ship | 105 | 30 | 12 | 2 | 3 | 2 | Lookout | gunship |
| Wolf Pack | 80 | 24 | 12 | 2 | 1 | 2 | - | wolf's head |
| Bear | 130 | 28 | 20 | 1 | 1 | 2 | - | bear's head |

`Unit::stats()` applies abilities and siege deployment on top of these; everything that asks
what a unit can do goes through it. Settlers (a planted flag) are civilians with the Melee body,
drawn as hollow hexagons with only a move badge; every other unit is a team-colored disc with its
pictogram. Settlers can be ordered to attack, though no badge shows it. Workers aren't units:
see Workers below. Wolf packs and bears are animals, owned by no side, on tan discs (see
Animals). Cavalry and
Armored are built at a Barracks on Horses or Iron, or supported by an adjacent Stable or Forge;
cities can't queue them. Stable-trained Cavalry get +1 move; Forge-trained Armored get +20% HP
and +15% defense. These upgrades stay with the unit, including in city interiors. A Field
Hospital heals nearby troops (see Buildings). A Harbor in a city whose center touches Coast or Ocean lets it build naval units; a
Landing Craft can carry four land units. Select a land troop and click an adjacent friendly craft
to plan boarding. Select the craft and click adjacent empty land to plan landing its first
passenger. Both happen after combat, so cargo sinks with its ship. A craft cannot attack.
Boarding and landing are orders like any other: boarding replaces the troop's move (both halves
of a swap), attack and queue, landing replaces the craft's move and queue (it lands from where it
is), each ends a hold, guard or alert and counts as the unit's orders for the turn, and any
other order (a move, attack, swap, queued turn, group move or attack, Alert, Ctrl-right-click)
calls either off. Disbanding a loaded craft disbands its passengers too; the first press says
how many.
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
- Another side's construction shows where the player sees the tile its worker stands on while
  at work there (arrived, with work left): the job's ring on the tile, or a wall or gate on its
  edge, in that side's color and named (BARRACKS, FARM, WALL), but with no turns left, which
  would give away its Workshop. Jobs it has queued or a worker is still walking to are its plans,
  not something on the ground, and don't show.
- Every frame, each hex in sight is recorded as last seen: cities, barracks (with their
  health), improvements, roads, structures and other sides' construction, so a building seen
  going up stays on the map as last seen until the tile is seen again. Units and workers aren't
  remembered, since they
  move: out of sight, the player knows of none anywhere. Planning goes around the walls and gates
  the player knows of. Remembered hexes out of sight draw that memory under a dark tint, keeping
  the terrain readable. Unexplored tiles lie under a dense cover of muted cumulus: soft-edged
  puffs lit from above, lower billows overlapping the shaded undersides of those behind them,
  over a dark fill so no gaps show. The clouds drift slowly on a steady wind and each puff
  gently billows; puffs fade out as they drift past the map's edge. The Fog setting swaps the
  clouds for a flat grey. The fog is drawn behind the map, so explored terrain simply covers it
  as sight grows. Never-seen terrain and objects are not drawn beneath
  the clouds.
- Enemy units out of sight are hidden, with their ghosts and hover info. An enemy in sight shows
  a move's ghost only if its destination is in sight too, and a fight over a hex out of sight
  doesn't turn it orange. Tile tooltips describe remembered hexes from memory. An enemy unit's
  hover panel gives its stats and state, not the player's instructions (boarding, unloading) or
  a landing craft's cargo. A command post breached or a city captured in its interior is
  announced only for the player's own cities and cities in sight.
- The player plans from what they know: in sight, the board as it is; out of sight, the memory.
  Terrain seen once is known for good (it never changes); a hex never seen counts as open ground
  for planning, whatever is really there, so a path planned into the fog goes straight through
  it (the move highlight, plain moves, queued moves and queued attacks all go by this). A move
  that turns out to run into terrain the player hadn't seen is turned back at resolution, as for
  an unseen wall. A unit out of sight, even one seen there before, doesn't shrink the move range, and clicking
  its hex plans a move, which then meets it at resolution. A remembered enemy barracks can
  still be attacked; an empty city center in sight cannot, but one out of sight can (nobody
  there is known of, and refusing it would give away that nobody is), and the attack misses if
  nobody is there when it comes.
- What the map and panels show follows the same rule: yields, and which hexes a city's or
  barracks' goods reach (badges, delivery percentages, tooltip, a Work Camp's CONNECTED or
  CUT OFF), use remembered
  cities and roads out of sight, and no units. A city or barracks shows its live hover panel only if it
  is the player's own or in sight. Of another side's city or barracks, the panel and the tile tooltip give
  only what's in sight (its population, a barracks' HP), or out of sight what the memory kept: not
  what the city delivers, what a Smelter makes or what its queues build, which hang on tiles,
  routes and plans the player can't see. Hovering an enemy city in sight rings only the tiles it works
  that are in sight, all as delivering: whether their goods arrive depends on its routes, which
  the player doesn't know. The economy itself runs on the real board: an enemy out of
  sight on a route hex that isn't a worked tile still cuts the goods behind it, and that shows as
  lost income, a red disrupted-tile ring, and the city view's "NO OPEN ROUTE WITHIN LOGISTICS
  BUDGET" notice, while the enemy itself stays hidden. That much is accepted as fair, like
  finding a tile pillaged.
- The AI plays under the same fog, by the same rules: each AI side sees what its units, cities,
  barracks, watchposts, outposts and workers see, and remembers what it has seen, and plans on
  that alone (see AI). A side's memory is part of the game, the same on every machine, updated as
  the AI plans that side's turn; F10 and the side this machine plays don't touch it. A side the
  AI takes over from a player who left starts remembering from then on.

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
  except an empty city center in sight. Units or workers standing on a city center remain
  attackable.
  **Attacks target hexes:** whoever stands there when the attack resolves gets hit.
- A unit can queue a move and an attack; the attack range is measured from the planned
  destination. Changing or cancelling the move drops an attack that is no longer in range.
- M and X (or the Move and Attack buttons) arm the next map click as a move or attack; with one
  armed, a right-click disarms it.
- Ctrl-click an adjacent ally to swap places (see Swaps).
- Ctrl-right-click clears the selected unit's orders, including its queue, a hold, a guard or
  an alert.
- Shift-left-click and Shift-right-click queue orders for later turns (see Order queues).
- Q (or the ability button) toggles the selected unit's ability.
- **Hold:** Space holds the selected unit if it still needs orders: it keeps what it has queued
  and gives up the rest of its turn. Space (or Hold) on a unit already holding stops the hold,
  keeping it selected and back in the turn order, and any new order to it (a move, attack,
  swap, queued turn) ends the hold too. A group holds all together, or stops holding if every
  member already is. With nothing left waiting, Space ends the turn. With a city or barracks
  view open, Space only closes it.
- **Guard:** G toggles `Unit::guarding`, like holding but lasting across turns, so the unit never
  comes back up in the turn order. Queuing any move, attack, swap, boarding or landing wakes it,
  as do G and
  Ctrl-right-click. Guarding units get a white hex outline.
- **Alert:** E (or the Alert button) toggles `Unit::alert`, a stance of its own beside Guard:
  the unit drops this turn's move and attack and its queue, stays put, is skipped in the turn
  order every turn, and costs nothing to keep. Each turn, in its own type's attack step, it
  attacks an enemy in its attack range (see Turn resolution). Any other order ends it: a move,
  attack, swap, queued turn, boarding, group move or attack, its ability, Guard, E again or
  Ctrl-right-click; Hold keeps it, and going on alert ends a guard. Melee, cavalry, armored and
  ranged troops can go on alert, and siege once set up (or setting up this turn: set it up with
  Q, then E; packing it up ends the alert). Scouts, ships and settlers can't ("ONLY MELEE,
  CAVALRY, ARMORED, RANGED AND SET-UP SIEGE CAN GO ON ALERT"). With a group, every member that
  can goes on alert, or all come off it if they all already are. Units on alert wear a red
  reticle: a ring with four diagonal ticks. The AI never puts a unit on alert; one it plans
  orders for (a side whose player left) comes off alert as the turn resolves.
- **Selection flow:** the first unit needing orders is selected at the start of each turn, and
  once the selected unit is done the next one is selected automatically. "Done" (`needs_orders`)
  means holding, guarding, on alert, following an order queue, boarding a craft or (a craft)
  landing a passenger, or having a move queued (or unable to
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
  every unfinished unit, opens a city if one still needs a build, and otherwise ends the turn,
  closing the city or Barracks view (a city interior stays open) and letting go of the unit
  selection and a manager being moved, in a network game as in a local one (there the player may
  select and open views again while it waits for the others). Nothing armed while planning outlives the turn: an action armed for the next map
  click (M, X, Swap), a Disband waiting to be confirmed or a click waiting to be repeated to
  replace a queue. With nothing left to order, a unit done with its orders lets go of the
  selection (and any armed action); it never ends the turn itself.
  Input, including UI clicks, is ignored while a turn plays out.

## Order queues (`order_queue.rs`)

- A unit's plan is a list of turns, each with at most one move and one attack: turn 1 is this
  turn's ordinary orders, later turns wait in its queue (`Unit::queued`).
- **Shift-left-click** adds every turn it takes to get to the clicked hex, starting where the
  plan leaves the unit. Each turn the unit moves to the hex it can reach that turn that is the
  shortest walk from the clicked hex, going around the terrain, walls and gates the player knows
  of (a hex never seen counts as open; see Fog of war) and allies standing still with no orders
  (the straight distance decides if there is no known way there; staying put wins ties). Of the
  hexes as far along the way, it takes the one nearest the clicked hex as the crow flies, so of the
  many equally short ways on hexes it keeps to the straightest. Turns are added
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
  turn (a siege setting up), it goes in the next turn. Out of range, nothing is queued; on
  water, only ships and ranged and siege land troops can attack (the notice says so). Like a
  queued move, it goes by what the player knows: a hex out of sight, even one never seen, is
  attacked as a seen one would be (see Fog of war).
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
- **Taking a move off:** a Shift-left-click on a hex the selection plans to move onto (a
  numbered stop of a queue, or a ghost, this turn's included) takes that move off instead of
  queuing, with every turn after it (each turn starts where the one before ended, so the later
  ones would start from the wrong hex); the turns before it stay. The turn keeps its attack if
  it's still in range from where the unit then stands; a later turn left with nothing goes too.
  If moves onto the hex come on several turns (or from several group members), the latest goes
  first; a tie goes to the first member selected. In a group, the member waits out the turns it
  lost, so its plan stays as long as the others'. A Shift-click on one of the player's units
  still adds it to the selection, and anywhere else still queues (`unqueue_move`). A plan left
  with nothing is no plan: the unit needs orders again.
- Queuing never moves selection on, so a unit (or group) can be given several turns in a row.
- **Not holding up the turn:** a unit following a queue counts as done (`needs_orders`), this
  turn and every turn it has queued orders for.
- **Cancelling:** any other order to the unit (a plain move or attack, a group move or attack, a
  swap, boarding, landing, its ability, Guard, Alert, Ctrl-right-click) drops its queue. Hold keeps it. This turn's
  orders stay (but for Alert, which drops them too), so the unit needs orders again unless the
  new order completes them. A map click that
  would replace a queue reaching past this turn (a plain move, attack or swap, for a unit or a
  group) needs the same click twice: the first only warns and outlines the hex, and any other
  click, a new selection or the turn ending forgets it (`confirm_queue_replace`).
- **Carrying over:** at the end of the turn, after each unit's `end_turn`, every unit with a
  queue takes its next turn's orders (`advance_queues`). That runs as the turn resolves, on
  every machine of a network game, so it goes by the real board only: the whole queue is
  dropped, with a notice ("MELEE STOPPED: ..."), and the unit needs orders, if it's in a
  contested hex, its target is out of range, or (for a queue kept as built, below) it isn't
  where the queue expected.
- **Going where it was sent:** a queue built by Shift-left-clicks alone remembers the hexes
  clicked (`Unit::waypoints`). As each turn's planning begins, it's planned again from where the
  unit stands, toward them in order, along the shortest way the player now knows
  (`replan_queues`): terrain the fog revealed, walls seen, allies parked in the way. So a path
  set through the fog follows what the fog reveals, a queue cut short by the queue limit keeps
  going where it was sent, and a move turned back is tried again from where the unit stands. A
  waypoint reached, or as near as the unit can get (an ally standing on it), is dropped; a
  queue with none left is done, and the unit needs orders again. In a group, members sent to
  the same hexes are planned together, in step. A queue whose next move runs into an enemy in
  sight stops, with a notice. This planning happens on the player's own machine as their turn
  begins, and goes in their plan like any other (in a network game, after the turn's start
  snapshot), so every machine resolves the same turn.
- **Kept as built:** a queued attack (Shift-right-click) makes the unit's plan fixed: it isn't
  planned again, and it's dropped with a notice as the player's turn begins if its next move is
  now known to be blocked by terrain or a wall, or an ally stands where it's going. Taking a
  move off (above) leaves a steered queue going to where the cut plan ends.
- The savestate (F6/F7) keeps queues, like every other order.

## Turn resolution (`turn.rs`)

After a 0.6 s pause (so the last order is visible), the turn plays out in 17 steps on land, one
every 0.6 s, with the acting units flashing (or all at once with instant playback). Steps where
nobody acts are skipped.

1. Scout move  2. Cavalry move  3. Wolf move  4. Melee move  5. Bear move  6. Ranged attack
7. Scout attack  8. Cavalry attack  9. Wolf attack  10. Melee attack  11. Bear attack
12. Ranged move  13. Siege move  14. Siege attack  15. Armored move  16. Armored attack
17. Workers

Ships move and attack after the Armored steps, before the workers. Animals act beside their like
(see Animals): wolves after cavalry, bears after melee. Workers go last, after every unit has
acted, so they're exposed (see Workers).

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
- **Alert fire:** a unit on alert (see Orders) acts in its own type's attack step, like a
  planned attack, at an enemy unit within its attack range as the step starts. So it hits
  whatever ended an earlier move step in range, this turn or a turn before, and one that comes
  in range only after its step (a siege moving in at step 13, say, against melee on alert) is
  fired on the next turn, if it's still there. The target is the nearest enemy unit, then the
  weakest (fewest HP), then the lowest hex (q, then r). It is chosen on the real board, fog or
  not, the same on every machine of a network game. It is only ever a unit: never a city or an
  empty city center, a barracks, a Coastal Battery or a worker (a unit standing on a city
  center is fired at, as a planned attack would hit it), and a ship only by a troop that can
  hit ships (ranged, siege). The attack is an ordinary one: melee draws retaliation, a unit
  locked in a contested hex fights its rival instead, and a siege setting up can't fire that
  turn. As the turn starts to resolve, a unit on alert with any other order for the turn (the
  AI's, for a side whose player left) or no longer able to be on alert (a siege packing up)
  comes off it.
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
  cities and Barracks idle this turn because the first item of their queue waits (rimmed red,
  counted as WAITING; clicking one opens it), settlers, then military units needing orders.
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
- Space/Hold holds every member, G guards them all (or unguards if all are), E puts every
  member that can on alert (or takes them off it if all are), Ctrl-right-click
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
  away. Every unit or structure hurt (retaliation, Coastal Battery fire and hits on a battery
  included) shows a rising damage number, or
  "KILLED". Enemy attacks animate too.
- **Damage preview:** hovering an enemy in sight that the selection (a unit or a group) can
  attack this turn, or that the player's units already attack, marks the health bars with what
  the turn's attacks on it would do: the target's loss as a pulsing segment with the number over
  the bar, and each attacker's retaliation on its own bar; a lethal hit rims the bar in red and
  reads LETHAL. It plays the attacks in their resolution steps with the same damage functions,
  so it is exact if the target stays and uses no ability, which the player can't know (the tile
  tooltip says IF IT STAYS). The same holds for Barracks, coastal batteries and, in a city's
  interior, fighters and the command post (with the post's shot back when it targets one of
  the attackers). Only what is in sight is previewed; queued (Shift) attacks aren't.

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

- Damage = `30 * e^((attack - defense) * 0.04)`, clamped to 1..100, with no random spread: the
  same fight always deals the same damage. Defense includes terrain and Shield Wall; attack
  includes Charge. Volley (60%) and attacks across the waterline (above) scale the result.
- Melee attacks (base range 1) draw retaliation from a defender that survives the hit (that
  one hit, not the step's sum). Two units trading blows across the waterline each take the
  other's shore-scaled damage.
- **Cities** cannot be attacked on the exterior map; capture happens by breaching and occupying
  the command post inside. **Barracks** have 220 HP and 25 defense and are removed (with their queue) at 0 HP; the city
  can then build a new one, at full HP. A structure is hit only when the attack hits no enemy unit
  at all (for a Volley, none on the target or its neighbors); Volley's 60% applies to structures
  too.

## Animals (`animals.rs`)

Aggressive neutral units that roam around dens and hunt whoever comes near, so the early map is
dangerous and an army pays. They belong to the wild, which is no side: no cities, stockpile, fog
or plan, never a player's seat, hostile to every side, and no side's AI plays it (World maps
only).

- **Kinds:** a Wolf Pack (fast and fierce: it runs down scouts and workers, and two of them
  bloody a lone troop) and a Bear (slow and tough: it beats a lone melee troop). See Units for
  their stats. On open ground a wolf pack's bite takes about 35 HP from a melee troop and 52
  from a scout, and a bear's about 41 from a melee troop. Each den keeps one kind: a wolf den
  or a bear den, marked with a dark brown rim and a paw print in the hex's bottom-left corner.
  Out of sight, dens show as last seen, and the tile tooltip names the den.
- **Territory:** an animal of a den never leaves the hexes around it: within 4 for a wolf pack,
  within 3 for a bear. A stray (see Dens breed) has no den and goes anywhere.
- **Hunting:** as its move step begins (wolves after cavalry, bears after melee; see Turn
  resolution), an animal goes for the nearest unit or worker out on the map within its hunting
  range of it (4 hexes for a wolf pack, 3 for a bear, 2 more for a stray) that it could strike
  from its territory (within its territory plus one of its den; anywhere for a stray), as close
  as its move and its territory allow; ties go to staying put, then the lowest hex (q, then r).
- **Roaming:** with nobody to hunt, it roams: it moves every turn it can, to a hex it can reach
  in its territory picked by a hash of the turn, the animal and the hex (a stray: of the hexes
  as far as it can go, so it ranges wide). That looks random but is the same on every machine,
  and draws nothing from the game's random numbers.
- **Attacking:** as its attack step begins, it attacks whoever is in its reach, in its territory
  or not: the nearest unit, then the weakest, then the lowest hex (q, then r); with no unit, a
  worker. It decides on the real board as its step begins, the same on every machine.
- **Never cities:** an animal, of a den or a stray, never enters or attacks a city center or
  anyone standing on one, never goes into a city's interior, and never captures anything: it
  doesn't step onto a worker (it attacks it), a worker that shares its hex is killed, and it
  never holds ruins (a side's count pauses while one stands on them). No city can be founded on
  a den.
- **Dens breed:** a den starts with one animal and keeps up to two of its own (three with many
  animals; see How many). Every 8 turns it adds another, on the den itself, or as soon after as
  it can: once nothing stands on the den. Below its cap the new animal is its own; at its cap it
  leaves as a **stray**, with no den, as long as the world has fewer strays than its limit: one
  for each side and each den a side (so one a side with few animals, two with many). At the
  limit, a full den waits until a stray dies. A den whose own animals die fills up again the
  same way. The tile tooltip says how many it keeps, how far they roam and hunt, what clearing
  it gives, when the next comes and whether it leaves as a stray, and how far strays hunt and
  how many the world keeps.
- **Hunting pays:** the side that kills an animal, of a den or a stray, gets +3 food (a wolf
  pack) or +5 food (a bear) for its stockpile at once; if several sides' blows land in the step
  that kills it, the one that dealt the most damage that step gets it (the earliest in
  `Team::ALL` on a tie).
- **Clearing a den:** a side that ends a turn with a unit (anything but a settler; scouts count)
  on a den clears it, for +6 food and 2 metal, before the turn's economy. The den is gone for
  good and adds no more animals, strays included; its own animals alive still roam and hunt
  around where it was. Its land is free to settle: from the next planning phase a city may be
  founded on its hex (see Founding), and the notice says so.
- **How many:** the Animals setting (Next World; see `controls.md`) gives the next world none,
  one den a side keeping up to two animals each (the default), or two dens a side keeping up to
  three each, with the stray limit above.
- Animals are seen like any enemy unit (and not remembered, as they move), attacked like one,
  and fired on by units on alert. A player can't select or order them, and a network plan that
  names one is refused.

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
  plans and worker jobs. Workers at home pass to the new owner, none of them held (see Workers,
  Recall). Field workers from that city return to another friendly city if one exists;
  otherwise they pass to the new owner, free to take its jobs.
  A worker sharing its hex with a new enemy is captured immediately after the ownership change.
  A unit of the old owner standing on the center (one that stepped onto it the turn it fell) is
  pushed off it: to the nearest hex within 2 (`PUSH_OFF_RANGE`) it can stand on with no unit, no
  city center and no other side's worker on it, walls ignored, the lowest q then r among equally
  near ones. With no such hex it is lost. A pushed unit loses its orders, queue, hold, guard and
  alert. So no unit ever stands on another side's city center.
  The exterior city remains non-attackable.
  A breached post changes its map marker and city panel to show that it is open for occupation.
- Red's interior fighters defend their own city's post by engaging intruders. When attacking an
  enemy city they head toward its post. Exterior Red defenders hold contested city gates and can
  attack nearby enemies, so their projected copies stay in the siege.
- F12 starts a siege practice position with four Blue troops at the Red city's gates (including
  a siege unit) against two Red defenders and the post. Order Blue attacks on the exterior as well
  as inside; without field orders, the defenders can kill gate troops before the post is breached.

## Cities (`city/`)

- **Founding** (`city/founding.rs`): F with a selected settler founds a city where it stands:
  only on passable land (not water), not on ruins or a den (a cleared den's hex is open), not
  in a hex an enemy contests (it would be left on the new city's center), and at least 6 hexes
  (`MIN_CITY_DISTANCE`) from every other city, any side's, whether you have seen it or not (the
  refusal says why). The new city starts at population 1 with nothing built, auto-assigns and
  opens. A side's first city
  comes with a worker at home, as a starting city does; any other city starts without one.
  Settlers come from the start (World, Start With: settler; Frontier) or from a city's queue
  (Settler, below).
- **Yields:** the city center gives 2 food and 1 wood on its own; each worked tile gives its
  food, wood and metal (see Goods) times its delivery share. No citizen works a city center or a
  tile a placed building stands on (`closed_to_citizens`): such a tile can't be assigned or take
  the manager, and a citizen already there moves off. Improvements (built by workers, see Workers): a mine on
  hills (+2 production), a lumber mill under forest or jungle (+1 production), otherwise a farm (+2
  food); snow can't be improved. The Cities scenario's preplaced farms (4/0), mines (0/4) and
  pastures (3/1) have fixed yields.
- **Logistics:** each worked tile's goods travel its shortest route to the city, counted in hexes
  moved: a step onto a road or a city hex, or along a Canoe House river, counts half a hex, and any
  other step one hex, whatever its terrain. Delivery is 100% at up to 1 hex, 75% at up to 2, 50%
  at up to 3 and 25% at up to 4; a tile farther than 4 is out of reach. So every tile beside a
  city delivers 100%, and roads extend reach: a tile 2 hexes out along a road delivers like one
  beside the city, and one 4 hexes out along it 75%. Enemy units, contested hexes, enemy cities and mountains block
  routes, and an enemy on the city blocks them all. Routes are recalculated every time they're
  used.
- **Managers and workers:** population is at most 28, worked as up to four **clusters**, each a
  manager (ringed in gold, on land) and up to six workers, each adjacent to its own manager. A
  city has a manager for each 7 citizens or part of that (citizens 1, 8, 15 and 22 bring one),
  so up to 4. Managers are marked `M`, or `M1` to `M4` once a city has several, and no manager
  stands beside another of its city's managers. A worked tile belongs to only one city and one
  cluster. A citizen with no tile it may work idles (a manager beside the city center, say, has
  five open tiles, not six). To move a manager, click it to pick it up (its workers leave the
  map with it; the other clusters stay), then click its destination: land in reach, not
  another cluster's tile, beside none of the other managers. Its workers keep their offsets
  where they can and are otherwise replaced by the best nearby tiles. Clicking the manager
  again puts it and its workers back.
- **Citizens:** click tiles to assign or release; A auto-assigns by the city's **priority
  order** of food, wood and metal (Food, Wood, Metal to start; the AI's cities keep it). Citizens are placed one at a
  time, the first manager first, each on the open tile worth the most: its delivered food, wood and
  metal, the first good in the order ×9, the second ×3 and the third ×1, ties going to the lower
  hex coordinates. The **food floor** holds whatever the order: until the food the city center,
  its Cannery and the tiles already taken deliver covers the citizens' upkeep plus 1, food counts
  as first and the other two keep their order. So the first manager, placed while the city is
  unfed, goes by food first unless its Cannery already feeds it. A citizen becomes a worker, beside a manager whose cluster has room (the
  first such cluster takes it); once no manager has room or an open tile beside it, and the
  population allows another manager, it becomes a new manager on the best open land tile
  beside none of the others. A click assigns the same way: a tile beside a manager with room
  takes a worker; open land clear of the managers takes a new manager when one is allowed.
  Changing the order (drag its chips, or click one to put it first)
  re-assigns. On growth or route disruption, reconciliation keeps valid manual assignments and
  fills an open slot the same way, by the order with the food floor, counting the food of the
  center, its Cannery and the tiles kept; a manual tile cut off by an enemy is remembered and returns
  when the route reopens, unless you changed it. A manager cut off is stood in for by the first
  of its workers that could manage, until its tile is back.
- **Losing a citizen** (starving, or a Settler costing one): an idle citizen goes first; then
  the last cluster's last worker, and a manager only once its cluster has no workers left.
  A city never goes below one citizen.
- **Stockpile** (`city/economy.rs`): each side has one store of food, wood and metal (top bar,
  with its change a turn), not one per city. Every city's delivered goods go into it at the
  turn's economy. A side starts with 10 food, 10 wood and 4 metal.
- **Food and upkeep:** each citizen eats 2 food a turn from the stockpile, so one city's farms can
  feed another. If the stockpile can't feed all of a side's citizens, its food empties and the
  side's largest city (the first on ties) loses a citizen (see Losing a citizen; never below 1).
- **Supply** (`city/supply.rs`): a hard cap on a side's army. Each city gives its side 3
  supply. Each of its first 7 citizens (its first manager cluster) gives 1 more, and past those
  every 2 citizens give 1, rounded down: a city of 4 gives 7, of 7 gives 10, of 9 gives 11, and
  a full city of 28 gives 20 (placeholder values, to be tuned). Every troop and ship alive,
  and every Scout, uses 1, passengers aboard a Landing Craft included; Settlers and workers
  use none. What counts against the cap is the side's units alive plus every troop, ship or
  Scout in its queues, paid for or not. Nothing that uses supply can be queued past the cap,
  in a city's queue or a Barracks' (its card is dimmed and says SUPPLY FULL, its tooltip the
  numbers). An item already queued also needs room when work on it would start: counting only
  the units alive and the items already started, it waits in place, unpaid, while the side has
  no room (its row says WAITS FOR SUPPLY) and the queue works the next item, like an item the
  stockpile can't pay for. One already started finishes. So losing a city or citizens (starving,
  a Settler) can leave a side over its cap: its units stay, and only new training stops until
  it has room again. The Cavalry and Armored deposit cap still applies too. The top bar shows
  the side's supply used and available (SUPPLY 5/7), red once it's all used; the city tray, the
  Barracks panel and each troop's and Scout's tooltip show it too.
- **Growth** is bought: Grow (9, or the city tray's Grow card) queues one more citizen, paid in
  food when work on it starts: 10 + 10 × (population + the Grows already paid for in that city),
  so a Grow queued behind another costs a citizen more by the time its turn comes. It takes 2
  turns in the city queue like any build. Nothing grows by itself, and no Grow goes past the
  cap of 28, counting every Grow queued.
- **Paying and the queue:** anything can be queued, whatever the stockpile holds (the other
  limits stay: a Harbor for ships, a deposit for Cavalry and Armored, the population cap for
  Grow, a Barracks for its troops, population 3 for a Settler, one Scout at a time, and
  supply for troops, ships and Scouts). A build is paid in full from the stockpile when work on it
  starts, not when it is queued. At each turn's economy, after income and upkeep, every queue
  works the first item in it that is already paid for or that the stockpile can pay for then,
  paying for it if it isn't; items before it that the stockpile can't pay for **wait** in place,
  unpaid, and the queue goes back to them as soon as it can pay. Queues pay in city order, each
  city's queue before its Barracks', so when several wait on one stockpile the first city's is
  paid first. Each item keeps its own work, which reordering carries with it; an unpaid item has
  none. A worked item moved out of the active slot shows SAVED on its queue row until it resumes.
  A waiting item's row is tinted red and says what it waits for ("WAITS, SHORT OF" and the
  missing resources, each with how much), the city tray, Barracks tray and hover panel name the
  first item's wait. When that queue works nothing else this turn, so that the item isn't
  getting built (the city **gathers while it waits**, see Gathering by itself; a Barracks is
  **idle**), those panels say so in a large red line ("GATHERING WHILE IT WAITS — MELEE WAITS,
  SHORT OF" the resources, or "IDLE — RANGED WAITS, ..." for a Barracks), the turn strip adds a
  red-rimmed chip for it, and End Turn counts it as WAITING. On the map, always, a tag rimmed in
  red over the building whose first item waits (the city's tower for its queue, the Barracks
  for its own) shows the item's pictogram (or GROW, GATHER) and what it waits for: each
  resource the stockpile is short of with how much ("-[wood]8"), SUPPLY, or POP 3. Holding Alt,
  a tag rimmed in gold over each of the player's cities and Barracks that works an item shows
  that item and its turns left. Other sides' buildings never show either. What waits is judged on the stockpile as this turn's
  economy will find it: what's there now, plus the turn's income, less the citizens' food, and
  less what the queues ahead start. A build or train card, and Grow, that the side can't pay for
  this turn is drawn short: a red rim and a red price, in both presentations; its tooltip (or the
  notice, for a key) says what the side is short of, but the card is never dimmed for it, and
  pressing it still queues the item. Taking an item out of a queue
  (its X, Backspace, or Clear) refunds its full price if it was paid for (a paid Grow refunds the
  dearest paid Grow's) and nothing if it wasn't; its work is lost either way. A captured city's
  queues and a destroyed Barracks' queue are lost, paid items unrefunded. Each build takes a
  fixed number of turns of work: a turn's work each economy it's worked, never past what it
  needs. Drag a row to reorder, click its X to remove; Backspace removes the head and PageDown
  swaps the first two. A city finishes at most one item a turn. A finished unit appears on an
  open neighboring hex (not one another unit is appearing on that turn). With no hex open, the
  city holds the unit until one opens, and banks no work for the rest of the queue meanwhile. A
  player city with an empty queue holds up the turn, since it can always Gather; one whose
  items all wait doesn't. Buildings and works placed for workers are still paid when placed
  (see Workers).
- **Gathering by itself:** a city whose own queue works nothing in a turn's economy (every
  item in it waits, for the stockpile, supply or citizens, or the queue is empty) gathers, as
  if Gather were chosen: the Gather yield comes into its side's stockpile once every queue has
  paid, as a chosen Gather's does, and the waiting items stay as they were. Its Barracks' queue
  doesn't count: a city whose Barracks trains while its own queue waits still gathers. A city
  that works anything (a build, a Grow, a chosen Gather, or a finished unit it holds for want
  of an open hex) doesn't. The rule is the same for every side, the AI's included, and each
  machine of a network game applies it itself. For the player's cities, as things stand this
  turn: the city tray says BUILDING GATHERING and the yield, the hover panel QUEUE:
  GATHERING, and the city queue's title CITY QUEUE - GATHERING THIS TURN.
- **Gather** (0, or its card beside Grow): free, one turn; when it's done, the side's stockpile
  gets 1 food, 1 wood and half a metal. A city that can't pay for anything, or has nothing it
  wants, gathers instead of standing idle, chosen or by itself (above).
- **Settlers and Scouts** come from a city's own queue (the town centre, never a Barracks), at
  their own pace: the half-speed rule for troops in a city center doesn't apply to them, and
  neither counts as a troop. A **Settler** (S, or its card) is dear and slow: 30 food and 10
  wood, 6 turns, and it takes one of the city's citizens when it's done (the last tile the city
  works is given up). Only a city of population 3 or more queues one (the card is dimmed, saying
  NEEDS POPULATION 3, below that) or works on it: the citizen isn't set aside while it's built,
  so the city keeps working it until the Settler is done, and a city that drops below 3 (it
  starved) leaves its Settler waiting in the queue, with its work (and its payment, if paid),
  its row saying WAITS FOR POP 3, while the queue works the next item; it goes on once the city
  has 3 again. A finished settler appears beside the city and founds a city (F) by the founding
  rules above. A **Scout** (4, or its card) is cheap and quick: 2 food and 4 wood, 2 turns. A
  city queues one Scout at a time (the card is dimmed, saying ONE SCOUT AT A TIME, while one
  is queued).
- **Production speeds builds** (the Debug panel's PROD SPEEDUP, off by default): a city's queue
  also gains a quarter turn of work a turn for each point of production (wood and metal) the city
  delivers, and a Barracks for each point delivered to it; the stockpile still gets those goods.
- **Prices and turns** (food / wood / metal, turns at a Barracks): Melee 3/9/0, 3; Ranged
  3/11/0, 3; Cavalry 5/6/5, 4; Siege 2/12/6, 4; Armored 5/3/11, 4; Patrol Galley 1/10/2, 3; Landing
  Craft 1/12/2, 4; Bombard Ship 1/12/6, 4; Worker 4/2/0, 2; Grow as above, 2; Gather free, 1. A city center
  trains land troops at half a Barracks' pace (twice the turns: a Melee takes 6); ships, which
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
    (Melee, Ranged, Cavalry, Siege, Armored), paid from the stockpile like the city's (when work
    on an item starts; one it can't pay for waits), training
    twice as fast as a city center, wherever the city's managers are (with production speeding
    builds, each manager beside the barracks adds its cluster's production, times their delivery
    share from the barracks). Only a Barracks trains Cavalry and Armored, and only one drawing
    on a deposit: Horses or Iron under it, or on or beside a Stable or Forge next to it. Each
    deposit a side's Barracks draw on allows 3 of that troop, counting those alive and queued, so
    a lost one can be replaced (the Debug panel's UNIT CAP: EVER counts every one ever trained
    instead, so a deposit runs out). A deposit an enemy unit stands on counts for nothing while
    it's there. The barracks panel shows each deposit kind's troops left, or why they're locked,
    and a locked card is dimmed with the reason in its tooltip. Troops from the ruins don't
    count. A unit appears next to the barracks (never on a worker out on the map).
  - **Mill:** worked tiles adjacent to it deliver all their food, if they can reach the city.
  - **Workshop:** a building placed on a site adjacent to one of its side's workshops takes its
    worker half the turns (rounded up; its price is unchanged).
  - **Canoe House:** must stand on a riverbank. Connected riverbank hexes act like roads for
    friendly delivery routes (half a hex a step between banks); walls, enemy occupation and the
    4-hex delivery limit still apply. This can bring several remote tiles into a city's reach at
    once.
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
  turn's work on each queue (paying for the item it starts, see Paying and the queue), then
  finished builds.

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
  (click one to show it on the map; Recall beside it; Release for those held at home) and its
  placed jobs (click to show on the
  map, drag to reorder, X to take one off for a refund).
- **Jobs:** a job waits in its city's list. A job a worker is out
  on shows as a bright gold ring (or edge) named with the job, and once the worker is at work
  with the turns left, like FARM [clock]2 (an improvement is named for what it becomes: farm, mine
  or lumber mill; a building by its name). The city panel names jobs the same way, with the tile:
  FARM · GRASSLAND · [clock]3. Placed jobs show on the map as faded gold rings (walls and gates as muted gold edges with
  rounded ends). A job keeps the work put into it when its worker leaves (see Progress below): on
  the map its name then adds the turns of work left, like FORT [clock]2, and in the city panel
  how much is done, like FORT · PLAINS · 2 OF [clock]4 DONE. A job needs explored open ground within workers' reach, no city there, and no
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
- **Idle workers** (at home, with no job in their city's list, or held there after a Recall)
  wait there, and never hold up the turn.
- **Walls and gates** stand on the edge between two hexes, not on a tile. Picked in the city's
  production list, each click on the map places one on the hex edge nearest the cursor (highlighted), and
  dragging places one on every edge the cursor passes.
  The worker builds it standing on whichever side is nearer its city (open, explored ground).
  One wall or gate per edge; an edge beside a city is fine.

  | Job | Work | Effect |
  |---|---|---|
  | Road | 2 turns | a dirt road: goods count a step onto it as half a hex (see Logistics) |
  | Improve | 3 turns | a mine, lumber mill or farm (see Yields) |
  | Wall | 2 turns | on an edge: no unit, worker or goods cross it, yours included |
  | Gate | 3 turns | on an edge: only your units, workers and goods cross it |
  | Outpost | 3 turns | you see 2 hexes around it |
  | Fort | 4 turns | your units in it get +50% defense (a placeholder) |
  | A building | its turns (halved beside a Workshop) | the building, on its site |

- **Going out:** in the Workers step, each city sends an idle worker (not a held one) out for
  each job at the top of its list, but none while an enemy unit stands where it would set out from
  (its city center or Work Camp); the job waits. A worker walks 1 hex a turn by the shortest way around
  impassable terrain, walls, others' gates, enemy units and enemy cities. Once on the tile it
  works the listed turns, less any work already in the job (starting the next turn), then takes
  the city's next job, or walks home when there is none. A job that became impossible is dropped
  (with a notice); a worker that can't reach its job gives up and heads home.
- **Danger:** out on the map a worker can be seen (your own see 1 hex around them). An enemy unit
  that moves onto its hex, or lands there from a landing craft, captures it: it joins the
  captor's nearest city (or is lost if the captor has none). An attack on its hex kills it when nothing else is there to hit. A unit
  standing on the same hex shields it from both, since it blocks the move and takes the hit.
  A captured or killed worker's job goes back to the top of its city's list.
- **Recall:** workers otherwise follow their jobs on their own, so each of your workers out on
  the map has a Recall button in its city's panel. A recalled worker drops its job (back to the top of the city's list) and walks straight
  home at its usual 1 hex a turn in the Workers step, taking no new job on the way. Home, it
  stays there, **held**, however many jobs its city lists, until you release it: the city panel
  counts held workers (WORKERS: 1 HOME (1 HELD), 0 OUT) and offers HELD AT HOME - RELEASE, one
  worker a click. A released worker goes out in that turn's Workers step to the job at the top of
  the list. A held worker is still one of the city's workers at home (safe, and counted for
  everything that counts them); the AI never holds workers, and a captured city's are not held.
- **Progress is kept:** the work put into a job stays with the job, not the worker. A worker
  recalled, captured or killed partway through leaves its job on its city's list with the turns
  of work it did, and whichever worker takes the job next (the same one or another) only works
  the turns left. To put an urgent job first, recall the worker, drag that job to the top of the
  list and release the worker once it's home (or let another worker take it); the half-built one
  waits with its work. A job's turns left are counted from its
  current work time, so a building beside a Workshop built meanwhile takes the halved time less
  the work done (always at least one turn). Taking a partly built job off the list still refunds
  its full price; the turns spent on it are lost.
- **Structures** are never destroyed or captured yet. Units plan moves around the walls and gates
  they know of; a move whose way is blocked by one (say, one not seen when it was planned), with
  no way around within the unit's move, is turned back at resolution. The fog remembers them like
  improvements.

## Interface (`src/game/ui/`)

- Panels dock in four corner zones and never overlap (`docs/ui-system.md`).
- **Top bar:** turn number, your stockpile (food, wood and metal, by icon, each with its change a turn:
  every city's delivery, less the citizens' food), your supply (SUPPLY used/available, red
  once it's all used), the latest notice, and the End Turn button, whose label names what is
  still waiting ("3 UNITS NEED ORDERS", "CHOOSE PRODUCTION") until it turns gold and reads END
  TURN, or END TURN · 2 WAITING while queues sit idle (its tooltip lists them).
- **Command tray** (bottom-left): with a city open, it shows population (n / 28), its citizens working and its managers, with several clusters a line per cluster (its manager's mark, workers and what they deliver), what the city delivers net
  of the food its citizens eat (a cluster whose manager is picked up counts for nothing until it's
  placed, here and in the top bar), the current build and its turns left, the priority chips (food, wood and
  metal icons, each with its rank), the Grow card
  (9), unit cards (1-3) and the Worker card (8), each with its price and turns (never dimmed for
  the price: what the stockpile can't pay for yet waits in the queue), the Yields button,
  the production list (units, buildings not yet built, and works for its workers, each with its
  price and turns), what the barracks is training with See Barracks, and its workers and placed
  jobs; the queue docks above it. With a barracks open, what it stands on, each deposit
  kind's Cavalry or Armored left (or why none), the side's supply, its five train cards (dimmed, saying SUPPLY FULL, when it's all used) and Open City, queue above. With a unit selected: stats (boosted values green, reduced
  red), notes, and buttons Move, Attack, Swap, then its ability (or Found City), then Hold,
  Guard, Alert (troops that can go on alert only; a siege not set up shows it dimmed), Clear
  Orders and Disband (press twice: the first press asks to confirm). The classic tray lays the
  icons out five to a row. Move, Attack and Swap arm the next map click only (a held modifier overrides it);
  pressing the button again or right-clicking disarms. The armed button has a bright border, a
  queued order turns its button gold, an unusable one is dimmed. Every button has a hover tooltip.
- **Hover:** hovering a unit shows its stats at the top-right; hovering a city or barracks shows
  a structure panel at the bottom-left instead (barracks HP; a city's population and what it delivers;
  the build worked and its turns left, and what the first item waits for, if it waits; of another side's,
  only its population or HP). Hovering a city also outlines its worked tiles, without yield badges. After the
  cursor rests on a hex for 0.75 s, a tooltip shows terrain or city, yields, defense, site, road,
  which city works it, its delivery share to the open city, and units on it.
- **City view:** C opens the first city needing a build (or your first city), and left-clicking
  your city opens that one. While open, map clicks manage tiles, except a click on one of your
  units' tokens, which selects the unit and closes the view (the rest of its hex still manages the
  tile), and a click on another of your cities or on a barracks, which opens that view (the open
  city's own center opens its interior); it closes on Tab, Space, Escape, or a click off the map.
  A barracks view closes the same way, and on a unit's token likewise; a click on a city or a
  barracks opens it, and C switches it to its city. Worked tiles are outlined green (managers' in gold, marked M or M1 to M4; red if disrupted).
  Hovering a manager draws a dotted line along its goods' route to the city: the cheapest
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
every other. The AI plays under the fog (see Fog of war): it plans only on what its side sees and
remembers, as the player does. An enemy unit or worker counts only while in sight; cities, ruins,
dens, walls and terrain count as last seen; ground never seen counts as open.
Each AI unit picks, of the enemy units in sight (animals aside: see below), the enemy workers alone in sight and ruins its
side knows of that no unit of its side holds or is heading for, and the enemy cities its side has
seen, the one nearest on foot (walking distance around the terrain, walls and others' gates and
cities it knows of; a city counts from its gates, and an enemy unit before a city as near).
Knowing of none, it explores: it heads for the nearest ground its side has never seen. With
nothing left to explore either, it heads straight for the nearest enemy in sight it can hit: a
ship on the water only if it's Ranged or Siege, which can shoot at one. It steps onto ruins it can
reach, and then holds them until they're claimed, attacking enemies in range from there. It steps
onto a worker it can reach this turn, capturing it; otherwise it attacks its target if already in
range, or moves to the reachable hex with the shortest remaining walk (staying put on a tie),
attacking if that brings it into range. Heading anywhere else than an enemy (ruins, a city, the
unknown), it attacks any enemy in sight in range of where it ends up. Next to an enemy city it
holds its ground there, its fighters going in through the gates (see City interiors). It skips hexes a
teammate already claimed, and units in a contested hex stay and fight. It never uses abilities,
never builds buildings, and ignores any player-controlled AI unit; its settlers go their own way (below).
An AI scout gathers what its side knows and stays alive, rather than fight:
- It never ends its move where an enemy it sees could reach and attack it next turn (the enemy's
  move and range by its type, a Cavalry's Charge and a deployed Siege's extra hex included), if
  it can help it. Hurt (under half health), it keeps a hex farther off still and heads for its
  side's nearest city.
- Of the safe hexes it can reach, it takes the nearest with an enemy worker alone on it
  (captured: a worker can't hit back) or ruins (which it holds while they stay safe).
- Otherwise it takes the one that brings the most into its sight that its side has never seen
  (counting double) or hasn't seen for 10 turns. Hills count, as they see farther, and hexes
  behind a mountain don't, as it wouldn't see them. On a tie it takes the one farthest from its
  side's cities, and a side's scouts don't count what another of them already heads to see, so
  they fan out. With nothing like that within reach, it heads for
  the nearest such ground.
- With no safe hex it's cornered: it gets out of reach of as many enemies as it can, as far as
  it can, and of those hexes the nearest its side's cities (so two scouts that meet on a hex part
  ways). Only if an enemy is still beside it does it attack, the nearest in range. It even slips
  out of a contested hex.

What a scout sees goes into its side's memory, which every AI decision plans on.
AI cities auto-assign citizens
at every end of planning. An AI queue gets one item when empty, and only one its side can pay for
this turn, counting the turn's income and what its other queues start, so it's paid and started
that turn; a queue whose items all wait anyway (for the stockpile, or for supply) is emptied
(they're unpaid, so nothing is lost) and planned again. It queues no troop, ship or Scout past
its supply: at the cap its Barracks stays idle and its cities grow or gather instead. An AI city with an empty queue trains a worker first when it has none. With a worker,
it places a Barracks for its workers to build, paid like the player's: on a Horses or Iron
deposit within 3 hexes (a kind it has none of first), else on the nearest open unworked tile
within 2. Its queue otherwise grows the city. A city without a Barracks trains Melee itself,
slowly, until the side has 2 units (scouts and settlers aside) per city, growing when it can't
pay. An idle AI Barracks trains Cavalry or Armored when its deposits allow and the side can pay,
else Melee, or Ranged for every two Melee. An AI city with a worker at home and an empty list places one job it can
pay for: an improvement on a tile it works, or else a road there. At a contested friendly city gate, AI
units hold position and attack an enemy in range. Ties break by hex coordinates, so it is
deterministic.

The AI builds Scouts and expands with Settlers:
- A side with no scout, alive or queued, trains one, after a city's first worker.
- **Expanding:** a city of population 4 or more whose side has no settler out or queued, can pay
  for one this turn and knows a site for a city, trains a Settler before it grows (after the
  Melee a city without a Barracks still wants). A site is a hex its side has seen that the
  founding rules allow as far as it knows: open land, no ruins or enemy in sight on it, 6 hexes
  from its own cities and every enemy city it has seen, and within 10 of its nearest city;
  searched up to 14 steps on foot, the best by what the land around it (2 hexes) yields as last
  seen, food counting double, less 2 a step to get there.
- **Settlers:** each turn a settler picks its site again from where it stands, walks toward it
  (keeping out of reach of enemies in sight where it can; no escort), and founds there once it
  stands on it. A site the rules refuse (a city its side hadn't seen stands too near) is
  remembered, and the side tries nowhere within 2 hexes of it again. A settler that knows no
  site waits.
- **First city:** a side with a settler and no city founds at once where the rules allow it,
  as before, and otherwise walks to the nearest site they allow; knowing none, it founds where
  it stands anyway.

The AI and animals (see Animals): it fights animals only where it expects to win, and to gain
more than it loses. For each den its side knows of (in sight or as last seen) it expects the
animals of the den's kind it sees within their territory of it, as they are, and unless it sees
the whole territory, more at full health up to as many as the den can have by then (one to start
and one more every 8 turns, up to the world's cap). Animals it sees that belong to no den it knows
of (their den unseen or cleared, or strays) make bands: those within 3 hexes of one another.
Against each den or band it plays the fight out by the combat formula, which has no random spread: each round
the animals strike first, each at the weakest troop, then each troop at the weakest animal left,
and a melee blow draws one back from a defender it doesn't kill; the animals under the cover of
the den's tile (a band's first animal's), its troops on open ground. Of its troops within 12 hexes
(not scouts, settlers or ships, nor units in a contested hex or holding ruins), it sends the
nearest few, up to 4, that kill them all within 6 rounds for the most gain: the den's spoils,
the strays it would send out if left alone (a worker's price for each of the 2 it would in 16
turns, in a world that keeps strays), its land if the side has a city within 10 hexes of it (a
quarter of a settler's price), and the animals' bounties, less the HP its troops lose (a dead
troop's all), a troop's full health worth its price (food, wood and metal alike); the fewest on
a tie, and none if no force gains.
The force gathers just outside the animals' reach (within 2 hexes of it), goes in together once
all of it is there or some of it is already in, fights the animals it sees, and steps onto the
den to clear it. A den or band it sends no force to it leaves alone: its other land units walk
around the animals' reach (a den's: its territory and a hex more; a band's: as far as each animal
could move and strike next turn; city centers aside, where no animal strikes), and one inside
gets out, or as far out as it can. Its scouts count that reach as threatened, and stand on a
den only if they expect no animal at it. Its side keeps clear of the territory of every den it
knows of (the hexes within 4 of it, as far as any animal roams): its settlers step around it
where they can, and it picks no city site in it; its workers take no job in it, nor in the reach
of animals it expects. Once it sees a den gone (cleared), it forgets it: its land is open to
its sites and workers again.

## Open questions

Known bugs link to their board item; the rest are design questions nobody has decided yet.

- The AI never uses abilities (its scouts never go on lookout) or builds buildings or
  structures, and its armies explore by walking to the nearest ground never seen, so without a
  scout's findings they find their enemies slowly.
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
  a first pass, slowed once for tempo (option B, `rts-economy.md` Round 7), and the late game
  has nothing to spend a growing stockpile on once cities are full (see `rts-economy.md`).
- No victory condition; F1-F4 restart a scenario. The Debug panel offers a Naval scenario
  with two coastal cities, prebuilt Harbors and Coastal Batteries, and ships ready to fight.
