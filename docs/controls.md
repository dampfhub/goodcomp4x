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
| Right-click map | Queue move to an open hex, or attack an enemy; with Move/Attack/Swap armed, disarm it instead |
| Ctrl-right-click | Clear selected unit's orders |
| Q / ability button | Toggle ability |
| Space / Hold button | Hold unit, keeping queued orders, then select next |
| Tab | Browse units without holding; leave city view |
| Enter / End Turn button | End planning; unfinished units automatically Hold |
| Wheel | Zoom |
| Left drag / middle drag | Pan the map |
| C / click your city center | Open city view |
| Click tile in city view | Assign citizen, or release an assigned citizen |
| A in city view | Auto-assign citizens, balancing food and production |
| Escape | Disarm Move/Attack/Swap, or return from city view to units |
| F1 | Reset to original combat scenario; discards current match |
| F2 | Reset to city scenario; discards current match |
| F3 | Start the frontier scenario; both sides begin with a settler |
| F5 | Toggle borderless fullscreen |
| F, with a selected settler | Found a city |
| 1 / 2 / 3 / 4 in city view | Queue melee / ranged / cavalry / siege |
| 5 / 6 in city view | Queue Granary / Barracks |
| M / X | Arm Move / Attack for the next map click (press again to disarm) |

A unit can queue a move and attack together. Attacks target tiles, and the
different unit types move and fire at different steps. Orders resolve only
after Enter. If a unit is unfinished, Enter selects it; use Space if it should
keep its existing orders and do nothing else.

Left clicks take effect when released. Moving at least 6 pixels while holding
the left button pans instead, without selecting a tile or issuing an order.

Abilities: melee Shield Wall improves defense but prevents movement; ranged
Volley deals reduced splash damage; cavalry Charge improves movement and attack;
siege Deploy spends a turn setting up for longer range, with Q again to pack up.

Cities appear as team-colored H squares. F/M/P label preplaced farms, mines,
and pastures; brown lines are dirt roads. Hover over a city or select it to see
green outlines around its worked tiles; red outlines mark disrupted assignments.
Only while hovering a city, small green grain and amber hammer icons show raw food and production
with numeric counts, including zero. Dark backgrounds keep them readable.
Icons are limited to that city's reachable and worked tiles. Selected-city outlines
remain visible after the pointer moves away. Hover-only percentages show
delivery efficiency. They assume current unit positions; combat can change the
final harvest.

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
Both sides also begin with a warrior. The red starting warrior is player
controlled in this scenario, so you can move either warrior to test route cuts,
contests, and city labor without fighting the AI for input.

The top bar shows the turn, how many units still need orders, the latest
notice, and an End Turn button (gold once every unit is ready).

Cities and units share one command tray in the bottom-left. Select a unit to
see its stats (boosted values green, reduced ones red) and a row of buttons:
Move, Attack, Swap, then its ability (or Found City for settlers, Build Road
and Improve for workers), and Hold. Each button shows its shortcut. Move,
Attack and Swap arm the next map click; the armed button has a bright border,
and pressing it again, right-clicking or Escape disarms it. A queued order
turns its button gold, and a button the unit can't use right now is dimmed.
Hover any button for a tooltip explaining it. Hover any unit on the map (either
team) to see its stats in a box at the top-left.

Workers are marked `W`. Move one onto a tile, then choose Build Road or Improve
Tile. Roads lower logistics cost; plains become farms and hills become mines.
These worker actions complete immediately in this first version.

Select a city to see its population, food and production, what it's building,
its growth meter, unit cards, and unique building cards. The Granary adds 2 food
per turn. Choose a Barracks site on any open land tile as soon as you start it,
or after it completes; click Confirm Barracks to finalize it. Move the gold
manager onto that tile to activate its own troop queue: it receives the
production of the manager's linked work group, while the city center's production
stays with the normal city queue. Hover a card to see its effect and cost; the
queued build is gold.

The city tray also has Food, Production, and Balanced labor focus buttons. They
set the default used by auto-assignment. A manually assigned tile cut off by an
enemy unit is kept reserved; its worker returns automatically when the route is
open again, unless you changed that assignment.
