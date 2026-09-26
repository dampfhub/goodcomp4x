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
| Right-click map | Queue move to an open hex, or attack an enemy |
| Ctrl-right-click | Clear selected unit's orders |
| Q / ability button | Toggle ability |
| Space | Hold unit, keeping queued orders, then select next |
| Tab | Browse units without holding; leave city view |
| Enter / top status bar | End planning after all units have orders or hold |
| Wheel | Zoom |
| Left drag / middle drag | Pan the map |
| C / click your city center | Open city view |
| Click tile in city view | Assign citizen, or release an assigned citizen |
| A in city view | Auto-assign citizens, balancing food and production |
| Escape | Return from city view to units |
| F1 | Reset to original combat scenario; discards current match |
| F2 | Reset to city scenario; discards current match |
| F3 | Start the frontier scenario; both sides begin with a settler |
| F, with a selected settler | Found a city |
| 1 / 2 / 3 / 4 in city view | Queue melee / ranged / cavalry / siege |
| M / X | Choose Move / Attack from the selected unit's command tray |

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
delivery efficiency, and clicking a tile draws its cheapest available route.
The preview assumes current unit positions; combat can change the final harvest.

Each population works one tile and consumes 2 food per turn. The center adds
2 food and 1 production automatically. Excess food grows population; shortages
consume reserves and eventually reduce population. New citizens need assignments
(press A or select tiles). Production accumulates in a store in this first slice.

Enemy occupation blocks transport through that hex. Roads lower transport cost,
and goods reroute when possible. Opposing improvements and already assigned tiles
cannot be claimed through the city panel. Both cities use the same economy rules.
The yellow route shown after clicking a tile is that tile's cheapest logistics
path, not a road order. Routes use total terrain/road cost, never straight-line
distance, and cannot pass through an enemy or contested hex.

The city tray has a green growth meter. Its fill shows food progress toward the
next population; its label shows turns remaining. When population grows, the
city automatically adds its best reachable unclaimed tile. If conflict cuts off
a worked tile, that citizen is reassigned at the end of the turn; your other
manual assignments remain intact.

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

Cities and units share one compact command tray in the bottom-left. Select a
city to see its yields and four clickable build cards. Select a unit to see
Move, Attack, Ability, and—only for settlers—Found City. Every card shows its
shortcut inside it. Move and Attack remain selected until the next map click.
Workers are marked `W`. Move one onto a tile, then choose Build Road or Improve
Tile. Roads lower logistics cost; plains become farms and hills become mines.
These worker actions complete immediately in this first version.
Hover a build card to see its combat role and matching keyboard shortcut; click
it or press that key to queue it. The selected build is gold.
