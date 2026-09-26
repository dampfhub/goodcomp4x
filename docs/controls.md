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
| Ctrl-right-click | Clear selected unit's orders, hold and guard |
| Q / ability button | Toggle ability |
| Space / Hold button | Hold unit for this turn, keeping queued orders, then select next |
| G / Guard button | Guard: the unit stays put and is skipped every turn until given an order (G again unguards) |
| Space with nothing left to do / End Turn button | End the turn, once every unit has orders, holds or guards and every city has a build |
| Tab | Browse units without holding; leave city view |
| Wheel | Zoom |
| Left drag / middle drag | Pan the map |
| C / click your city center | Open city view |
| Click tile in city view | Assign citizen, or release an assigned citizen |
| A in city view | Auto-assign citizens, balancing food and production |
| Y / Yields button in city view | Show or hide tile yields around the city |
| Rest the cursor on a hex | After 0.75s, a tooltip shows its terrain, yields, and what's on it |
| Hold Escape for 1s | Quit the game (a prompt shows while it's held) |
| F1 | Reset to original combat scenario; discards current match |
| F2 | Reset to city scenario; discards current match |
| F3 | Start the frontier scenario; both sides begin with a settler |
| F5 | Toggle borderless fullscreen |
| F, with a selected settler | Found a city |
| 1 / 2 / 3 / 4 in city view | Queue melee / ranged / cavalry / siege |
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

Cities appear as team-colored H squares. F/M/P label preplaced farms, mines,
and pastures; brown lines are dirt roads. Hover over a city or select it to see
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
its growth meter, and four build cards. Hover a card to see what the unit does
and costs; click it or press its number to queue it. The queued build is gold.
