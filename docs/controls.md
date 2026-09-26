# Playing the prototype

Run `cargo run --release`. The city scenario opens by default. You control blue;
red queues its orders when you end planning. There is no save system yet.

| Control | Action |
| --- | --- |
| Left-click friendly unit | Select it |
| Click green tile | Queue movement; repeat to cancel |
| Click enemy in range | Queue attack |
| Shift-click tile in range | Attack that tile, even if currently empty |
| Ctrl-click adjacent ally | Queue a swap |
| Alt-drag a box | Select every one of your units inside it as a group |
| Alt-click a unit | Add it to, or remove it from, the selection |
| Click a hex with a group | Each member moves as close to it as it can get |
| Click an enemy with a group | Every member in range attacks it (again to call it off) |
| Right-click map | Queue move to an open hex, or attack an enemy; with Move/Attack/Swap armed, disarm it instead |
| Ctrl-right-click | Clear selected unit's orders, hold and guard |
| Q / ability button | Toggle ability |
| Space / Hold button | Hold unit for this turn, keeping queued orders, then select next |
| G / Guard button | Guard: the unit stays put and is skipped every turn until given an order (G again unguards) |
| Space with nothing left to do / End Turn button | End the turn, once every unit has orders, holds or guards and every city has a build |
| Tab | Browse units without holding; leave city view |
| Enter / End Turn button | Holds unfinished units, then ends the turn when every city has a build |
| Wheel | Zoom |
| Left drag / middle drag | Pan the map |
| C / click your city center | Open city view |
| Click tile in city view | Assign citizen, or release an assigned citizen |
| A in city view | Auto-assign citizens, balancing food and production |
| Y / Yields button in city view | Show or hide tile yields around the city |
| Rest the cursor on a hex | After 0.75s, a tooltip shows its terrain, yields, and what's on it |
| Hold Escape for 1s | Quit the game (a prompt shows while it's held) |
| F1 | Start the original combat scenario (again to restart it) |
| F2 | Start the city scenario (again to restart it) |
| F3 | Start the frontier scenario, a settler each (again to restart it) |
| F4 | Start a randomly generated world; every press makes a new map |
| F6 | Save a snapshot of the whole game (testing savestate) |
| F7 | Load the snapshot; it's kept, so it can be loaded again |
| F8 | Toggle turn playback: step by step, or every step at once (same outcome) |
| F9 / debug Complete Production | Finish the active city build or Barracks training immediately |
| F10 | Toggle fog of war (debug) |

The faded DEBUG panel at the top-left has buttons for F1-F4 and F6-F10; the
current scenario is gold, and a generated map shows its seed. The savestate lives only until the game closes and
survives switching scenarios; loading it returns to its scenario.
| F5 | Toggle borderless fullscreen |
| F, with a selected settler | Found a city |
| 1 / 2 / 3 / 4 in city view | Queue melee / ranged / cavalry / siege |
| 5 / 6 / 7 / 8 in city view | Queue Granary / Barracks / Mill / Workshop |
| M / X | Arm Move / Attack for the next map click (press again to disarm) |

A unit can queue a move and attack together. Attacks target tiles, and the
different unit types move and fire at different steps. Orders resolve only
when the turn ends. Space holds the selected unit if it still needs orders, and
moves on to whatever's next: another unit, then any of your cities with nothing
queued to build. Once nothing is left, Space ends the turn. Moving citizens
never holds the turn up.

Left clicks take effect when released. Moving at least 6 pixels while holding
the left button pans instead, without selecting a tile or issuing an order.

Abilities: melee Shield Wall improves defense but prevents movement; ranged
Volley deals reduced splash damage; cavalry Charge improves movement and attack;
siege Deploy spends a turn setting up for longer range, with Q again to pack up.

Your queued attacks show as orange arrows from the attacker (or its ghost) to
the target; the AI's don't. As the turn plays out, each attack's arrow shoots
to its target: a burst means it hit, a grey MISS that the hex was empty, and
OUT OF RANGE that the target moved away first. Damage numbers rise from every
unit hurt, attackers taking retaliation included.

Cities appear as team-colored crenellated towers showing their population, with
a gold G disc once they have a granary; a barracks is a small house marked B.
Improvements (farms, mines, pastures, lumber mills) show as small badges in a
hex's top-left corner, edged in their owner's color: crop rows, an ore heap, a
fence, or logs. Brown lines are dirt roads. Settlers (T) and workers (W) are
drawn hollow, so civilians stand apart from fighters. Hover over a city or select it to see
green outlines around its worked tiles; red outlines mark disrupted assignments.
With a city open and yields shown (Y or the Yields button toggles them; on by
default), small green grain and amber hammer icons show raw food and production
with numeric counts, including zero, and percentages show delivery efficiency.
Icons are limited to that city's reachable and worked tiles. The percentages
assume current unit positions; combat can change the final harvest.

Rest the cursor on any hex for a moment for a tooltip: its terrain (or city),
yields, defense bonus, site and road, which city works it, what share reaches
the open city, and the units on it.

Each population works one tile and consumes 2 food per turn. The center adds
2 food and 1 production automatically. Excess food grows population; shortages
consume reserves and eventually reduce population. New citizens need assignments
(press A or select tiles). Production accumulates in a store in this first slice.

Enemy occupation blocks transport through that hex. Roads lower transport cost,
and goods reroute when possible. Opposing improvements and already assigned tiles
cannot be claimed through the city panel. Both cities use the same economy rules.
Each tile's goods travel its cheapest logistics path. Routes use total
terrain/road cost, never straight-line distance, and cannot pass through an
enemy or contested hex.

In the city view, clicking one of your units selects it and closes the view;
so does clicking the city again or clicking off the map.

The city tray has a green growth meter. Its fill shows food progress toward the
next population; its label shows turns remaining. When population grows, the
city automatically adds its best reachable unclaimed tile. If conflict cuts off
a worked tile, that citizen is reassigned at the end of the turn; your other
manual assignments remain intact.

The gold `M` tile is the city manager; green-outlined tiles are its workers and
the dotted links show their relationship. Click a non-adjacent reachable tile
to move the manager. Each worker tries to keep its same axial offset from the
manager; unavailable worker positions are replaced by the best nearby eligible
tiles automatically.

This is the economic experiment, not yet the full design: city defense/capture,
site capture, road construction, build queues, science, strategic resources, and
luxuries are not implemented. City centers currently remain economic markers;
enemy occupation can interrupt external gathering but cannot capture them.

In the frontier scenario, settlers use a `T` marker. Found your city with F,
open it with C, then choose a unit with 1–4. Production accumulates at the end
of each turn and a finished unit appears on an open adjacent hex. If every
adjacent hex is occupied, the city keeps the completed build until one opens.
Both sides also begin with a scout. The red starting scout is player
controlled in this scenario, so you can move either scout to test route cuts,
contests, and city labor without fighting the AI for input.

The top bar shows the turn, the latest notice, and on the right a button naming
what the turn is waiting on ("3 UNITS NEED ORDERS", "CHOOSE PRODUCTION"); click
it to jump there. Once nothing is waiting it turns gold and reads END TURN.

Cities and units share one command tray in the bottom-left. Select a unit to
see its stats (boosted values green, reduced ones red) and a row of buttons:
Move, Attack, Swap, then its ability (or Found City for settlers, Build Road
and Improve for workers), and Hold. Each button shows its shortcut. Move,
Attack and Swap arm the next map click; the armed button has a bright border,
and pressing it again or right-clicking disarms it. A queued order
turns its button gold, and a button the unit can't use right now is dimmed.
Hover any button for a tooltip explaining it. Hover any unit on the map (either
team) to see its stats in a box at the top-left.

Workers are marked `W`. Move one onto a tile, then choose Build Road or Improve
Tile. Roads lower logistics cost; plains become farms and hills become mines.
These worker actions complete immediately in this first version.

Select a city to see its population, food and production, what it's building,
its growth meter, unit cards, and unique building cards. The Granary adds 2 food
per turn. Barracks, Mill, and Workshop each need a site on open land; select
one when queuing it, click its map badge to move the site if needed, and confirm it once production
is ready. A Mill makes adjacent reachable tiles deliver 100% of their food,
but cannot extend the city's hard logistics cutoff. A Workshop lets an adjacent
building be confirmed at half its normal production cost. Moving it away before
confirmation removes the discount and resumes construction at normal cost. Move the gold manager
onto a Barracks to activate its own troop queue: it receives the production of
the manager's linked work group, while the city center's production stays with
the normal city queue. Hover a card to see its effect and cost; the queued build
is gold.
Long city and Barracks queues show as many items as fit above the tray, up to
the space below the top bar (four at 1600×900).
Scroll over the box or drag its scrollbar to reach later items. Drag a queue
row onto another row to reorder it, or click its small X to remove it.

The city tray also has Food, Production, and Balanced labor focus buttons. They
set the default used by auto-assignment. A manually assigned tile cut off by an
enemy unit is kept reserved; its worker returns automatically when the route is
open again, unless you changed that assignment.

## Map tiles

The F4 world is generated from a seed (`src/game/mapgen.rs`) on a rectangular
map about 61 hexes wide by 36 tall, sized for four players (there are still
only two sides). It's a Pangea: one continent, with at most a few small
islands, ringed by sea, with mountain ranges, hills, rivers running downhill along hex edges
to the sea or into lakes, and climate bands that are colder toward the top and
bottom of the map. Both sides start on the continent, far apart, on sites of
similar quality, with a settler, a worker and a scout each. The settler and
worker always start on flat ground and the scout on hills, so both sides begin
seeing the same. The
camera starts on your settler.

A tile is a base ground, optionally raised into hills and covered by forest
or jungle:

| Ground | Food | Production | Marks |
| --- | --- | --- | --- |
| Grassland | 3 | 0 | |
| Plains | 2 | 1 | |
| Desert | 0 | 1 | dunes |
| Tundra | 1 | 1 | grass tufts |
| Snow | 0 | 0 | can't be improved |
| Marsh | 1 | 0 | reeds; goods cost more to carry through |
| Mountains | - | - | impassable, can't be worked |
| Coast / Lake | 2 | 0 | water: units can't enter, cities can work it |
| Ocean | 1 | 0 | water, away from the shore |

| Modifier | Effect | Found on |
| --- | --- | --- |
| Hills (two small peaks) | +1 production, +25% defense, +1 route cost, +1 sight | any land but marsh |
| Forest (pines) | -1 food, +1 production, +15% defense, +1 route cost | grassland, plains, tundra, hills included |
| Jungle (round canopies) | +1 food, +1 production, +15% defense, +1 route cost | marsh only |

On hills, forest or jungle is drawn along the top of the hex. Rivers are the
blue lines between hexes. Land beside a river or a lake has fresh water and
gets +1 food. Goods can be brought in from a water tile but never carried
across water. A city's first citizen, the manager, has to work land. Workers
build a mine (+2 production) on hills, a lumber mill (+1 production) in forest
or jungle, or a farm (+2 food) elsewhere. Hills, forest and rivers don't slow
units yet.

## Scouts and fog of war

The scout (small circle, `X`) moves 3 but barely fights: 60 HP, attack 8, defense
10. Scouts move first in a turn and attack right after ranged units. Its
ability, Lookout (Q), keeps it in place for the turn; through the next turn it
sees 2 hexes farther.

Fog of war is on by default. Units see 2 hexes (scouts and cavalry 3, +1 on
hills), cities 3, barracks 1. Mountains block sight: you can see a mountain, but
not the hexes behind it. It has two layers. Never-seen hexes are solid
black: you know nothing about them. Hexes you have seen before but can't see
now sit under a grey veil, outlined in darker grey where they meet what you can see, with faint cloud puffs,
and show them as they were when you last saw them:
enemy units where they stood, cities and barracks with the health they had,
improvements and roads, and their tooltips describe them that way. F10 or the debug panel's FOG button turns it off. The AI ignores
the fog.
