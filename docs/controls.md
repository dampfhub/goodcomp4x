# Controls

Run `cargo run --release`. The city scenario opens by default. You control Blue; Red plans its
orders when you end the turn. The key map in code is `App::window_event` in `src/app.rs`; the
rules behind each action are in `game-rules.md`.

## Units

| Control | Action |
| --- | --- |
| Left-click your unit | Select it |
| Click a green hex | Queue a move; click again to cancel |
| Click an enemy in range | Queue an attack on its hex (units, cities and barracks) |
| Shift-click a hex in range | Attack that hex, even if it is empty now |
| Ctrl-click an adjacent ally | Queue a swap |
| M / X (or Move / Attack buttons) | Arm Move / Attack for the next map click (again to disarm) |
| Right-click | Queue a move to an open hex or an attack on an enemy; with an action armed, disarm it |
| Ctrl-right-click | Clear the selected unit's orders, hold and guard |
| Q / ability button | Toggle the unit's ability |
| Space / Hold button | Hold the unit this turn, keeping queued orders, and move on to what's next |
| G / Guard button | Guard: stay put and be skipped every turn until given an order (G again unguards) |
| Tab | Look at the next unit without holding this one; leave the city view |
| Alt-drag a box | Select every one of your units inside it as a group |
| Alt-click a unit | Add it to, or remove it from, the group |
| Click a hex with a group | Each member moves as close to it as it can get |
| Click an enemy with a group | Every member in range attacks it (again to call it off) |
| F / Found City button | Found a city with the selected settler |
| R / Build Road button | Build a dirt road under the selected worker |
| I / Improve button | Improve the worker's tile: hills become a mine, anything else a farm |

## Turn

| Control | Action |
| --- | --- |
| Space with nothing left to do | End the turn |
| End Turn button | Hold every unfinished unit, then end the turn (or open a city that still needs a build) |

The End Turn button names what the turn is waiting on ("3 UNITS NEED ORDERS", "CHOOSE
PRODUCTION") and turns gold, reading END TURN, once nothing is.

## Cities

| Control | Action |
| --- | --- |
| C / left-click your city | Open the city view |
| Click a tile in the city view | Assign a citizen there, or release one |
| Click the manager (`M`), then a tile | Move the manager; workers follow where they can |
| A | Auto-assign citizens by the city's labor focus |
| Food / Production / Balanced buttons | Set the labor focus |
| Y / Yields button | Show or hide tile yields around the open city |
| 1 / 2 / 3 / 4 | Queue Melee / Ranged / Cavalry / Siege |
| 5 / 6 | Queue Granary / Barracks |
| Backspace | Remove the item being built |
| PageDown | Promote the next queued item |
| Queue item buttons | Move an item up or down, or remove it |
| Click a tile, then Confirm Barracks | Choose the Barracks site (asked when you queue it); Confirm Barracks places it once it is complete, and Change Barracks Site picks again |
| See Barracks / left-click your barracks | Open the barracks view (its own queue of all six unit types) |
| Space / Escape | Close the city or barracks view |
| Tab / click off the map | Close the city view |

While a city view is open, map clicks manage tiles and never select units.

## Camera, game and testing

| Control | Action |
| --- | --- |
| Wheel | Zoom |
| Left-drag (6 px or more) / middle-drag | Pan |
| Rest the cursor on a hex | After 0.75 s, a tooltip: terrain, yields, site, road, who works it, units |
| Hover a unit | Its stats, in a box at the top-right |
| Hover a city or barracks | Its HP, production and current build (and a city's growth), at the bottom-left |
| F5 | Toggle borderless fullscreen |
| Hold Escape for 1 s | Quit (a prompt shows while it's held) |
| F1 / F2 / F3 | Start the combat / city / frontier scenario (again to restart it) |
| F6 | Save a snapshot of the whole game (in memory only) |
| F7 | Load the snapshot; it is kept, so it can be loaded again |
| F8 | Toggle turn playback: step by step, or every step at once (same outcome) |

The faded DEBUG panel at the top-left has buttons for F1-F3 and F6-F8; the current scenario is
gold. Left clicks act on release, so a drag never issues an order.

## Reading the map

- Blue and red numbers on a unit are its move and attack steps in the turn order.
- Your queued attacks are orange arrows from the attacker (or its ghost) to the target; the AI's
  are hidden. During playback each arrow shoots to its target: a burst is a hit, grey MISS a hex
  with no enemy unit (a hit on an empty city or barracks still does damage), OUT OF RANGE a target that moved away. Damage numbers rise from every unit hurt.
- Gold ring: queued ability. Steel ring: deployed siege. White hex outline: guarding. Orange hex:
  contested.
- Cities are team-colored `H` squares with HP bars; a barracks is a `B` square. F/M/P label farms,
  mines and pastures; H and I mark Horses and Iron; brown lines are dirt roads. `T` is a settler,
  `W` a worker.
- In the city view, green outlines are worked tiles (red if cut off), the gold `M` is the manager,
  and dotted lines link it to its workers. Green grain and amber hammers show food and production,
  with the share that reaches the city.
