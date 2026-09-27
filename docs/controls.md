# Controls

Run `cargo run --release`. The city scenario opens by default. You control Blue; Red plans its
orders when you end the turn. The key map in code is `App::window_event` in `src/app.rs`; the
rules behind each action are in `game-rules.md`. This file is the only description of the
controls: the game's startup log (`CONTROLS_HELP` in `src/game/mod.rs`) just points here.

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
| Q / ability button | Toggle the unit's ability (what each does: `game-rules.md`, Abilities) |
| Space / Hold button | Hold the unit this turn, keeping queued orders, and move on to what's next |
| G / Guard button | Guard: stay put and be skipped every turn until given an order (G again unguards) |
| Tab | Look at the next unit without holding this one; leave the city or barracks view |
| Alt-drag a box | Select every one of your units inside it as a group |
| Alt-click a unit | Add it to, or remove it from, the group |
| Hold Alt | Show extra map info: each unit's turn order, and every explored tile's yields |
| Click a hex with a group | Each member moves as close to it as it can get |
| Click an enemy with a group | Every member in range attacks it (again to call it off) |
| F / Found City button | Found a city with the selected settler |
| R / Build Road button | Build a dirt road under the selected worker |
| I / Improve button | Improve the worker's tile: a mine on hills, a lumber mill in forest or jungle, otherwise a farm (not on snow) |

A unit can queue a move and an attack; it attacks from the hex it moves to. Units can't move
through occupied hexes, and two allies can't head for the same hex. Once the selected unit has a
move and an attack queued (or can't do one of them), the next unit needing orders is selected
and the camera glides to it; a unit you select by clicking stays selected.

## Turn

| Control | Action |
| --- | --- |
| Space with nothing left to do | End the turn |
| End Turn button | Hold every unfinished unit, then end the turn (or open a city that still needs a build) |

The End Turn button names what the turn is waiting on ("3 UNITS NEED ORDERS", "CHOOSE
PRODUCTION") and turns gold, reading END TURN, once nothing is. Clicks are ignored while a turn
plays out.

## Cities

| Control | Action |
| --- | --- |
| C | Open a city needing a build (or your first city); from a barracks view, its city |
| Left-click your city | Open its city view |
| Click a tile in the city view | Assign a citizen there, or release one |
| Click the manager (`M`), then a tile | Move the manager; workers follow where they can |
| A | Auto-assign citizens by the city's labor focus |
| Food / Production / Balanced buttons | Set the labor focus (and re-assign) |
| Y / Yields button | Show or hide tile yields around the open city |
| 1 / 2 / 3 | Queue Melee / Ranged / Siege (Cavalry and Armored train at a barracks on Horses / Iron) |
| 4 / 5 / 6 / 7 | Queue Granary / Barracks / Mill / Workshop |
| Click a tile after queuing a Barracks, Mill or Workshop | Choose its site |
| Click a planned site's map badge | Move that site |
| Confirm button | Place a finished Barracks, Mill or Workshop on its site |
| Drag a queue row onto another | Reorder the queue |
| Click a row's X | Remove it (removing the item in progress loses its production) |
| Wheel over a long queue, or drag its scrollbar | Scroll the queue |
| Backspace | Remove the item being built |
| PageDown | Swap the first two queue items |
| See Barracks, or left-click your barracks with no view open | Open the barracks view (its own queue of all five unit types) |
| V / City Interior button | Open the selected or hovered city's tactical interior; press again to leave |
| Click a Blue copy, then another interior tile | Queue its independent move or attack |
| Backspace in the interior | Clear the selected copy's orders |
| Open City (in the barracks view) | Go back to the city view |
| Space / Escape / click off the map | Close the city or barracks view |
| Tab | Close the city or barracks view |

While a city view is open, map clicks manage tiles and never select units.

## Camera, game and testing

| Control | Action |
| --- | --- |
| Wheel | Zoom (except over a scrollable queue) |
| Left-drag (6 px or more) / middle-drag | Pan |
| Rest the cursor on a hex | After 0.75 s, a tooltip: terrain, yields, site, road, who works it, units |
| Hover a unit | Its stats, in a box at the top-right |
| Hover a city or barracks | Its HP, production and current build (and a city's growth), at the bottom-left |
| F5 | Toggle borderless fullscreen |
| Hold Escape for 1 s | Quit (a prompt shows while it's held) |
| F1 / F2 / F3 | Start the combat / city / frontier scenario (again to restart it) |
| F4 | Start a newly generated world; every press makes a new map |
| F12 | Start a siege at Red's city with its interior open |
| F6 | Save a snapshot of the whole game (in memory only) |
| F7 | Load the snapshot; it is kept, so it can be loaded again |
| F8 | Toggle turn playback: every step at once (the default) or step by step (same outcome) |
| F9 | Pay for the open city's or barracks' current build at once (debug; a building still needs its site and Confirm) |
| F10 | Toggle fog of war (on by default) |
| F11 | Switch between the ImGui and classic UI presentations |
| Ctrl (held, ImGui) | Show panel title bars and resize grips for rearranging |
| Ctrl+Shift+R in City / Building or Troop (ImGui) | Reset that view's Debug placement to Default |

The faded DEBUG panel at the top-left has buttons for F1-F4, F12 and F6-F10; the current scenario is
gold, and a generated map shows its seed. Left clicks act on release, so a drag never issues an
order.

## Command line

Flags go after `--`, e.g. `cargo run --release -- --scenario world --seed 42`; `--help` lists
them.

| Flag | Effect |
| --- | --- |
| `--scenario <name>` | Start in `combat`, `cities` (the default), `frontier`, `world` or `siege` (F12) |
| `--seed <n>` | With `--scenario world`: generate map number `n` (the seed the debug panel shows) |
| `--size <W>x<H>` | Open the window at this size in pixels |
| `--screenshot <file>` | Draw the scenario's first moments in a hidden window, save a frame as a PNG (1600x900 unless `--size`), and exit; for checking visual changes without playing |

## Reading the map

- Holding Alt shows blue and red numbers on each unit: its move and attack ranks in the turn
  order.
- Tile yields sit below the unit spot: wheat stalks for food, then hammers for production, one
  per point laid out like a die's pips (3 a triangle, 4 a square, 5 a square with one in the
  middle, 6 two rows of three); past 6, one icon and the number. The open city shows them on the tiles it reaches (Y hides them);
  holding Alt shows them on every explored tile that can be worked.
- Your queued attacks are orange arrows from the attacker (or its ghost) to the target; the AI's
  are hidden. During playback each arrow shoots to its target: a burst is a hit, grey MISS a hex
  with no enemy unit (a hit on an empty city or barracks still does damage), OUT OF RANGE a
  target that moved away. Damage numbers rise from every unit hurt.
- Gold ring: queued ability. Steel ring: deployed siege. White hex outline: guarding. Orange hex:
  contested.
- Units are tokens in team color with a pictogram of what they are: sword (melee), bow
  (ranged), horse head (cavalry), catapult (siege), spyglass (scout), shield (armored) on a
  disc. Settlers (a planted flag) and workers (a shovel) are hollow hexagons.
- Cities are crenellated towers showing their population, with a gold G disc once they have a
  granary, and HP bars; a barracks is a small house marked B, a Mill a green diamond marked M,
  a Workshop a blue diamond marked W. While you choose or move a site, a translucent diamond with
  the building's letter previews it. Improvements are small icons in a hex's top-left corner: a
  wheat stalk (farm), an ore cart (mine), a fence (pasture), stacked logs (lumber mill).
  Resources are icons in the top-right corner: a horse's head (Horses), an ingot (Iron). Brown
  lines are dirt roads, and blue lines between hexes are rivers.
- Terrain: two small peaks are hills, pines forest, round canopies jungle; dunes, grass tufts and
  reeds mark desert, tundra and marsh. A large snowy peak is a mountain (impassable) and waves
  are water (units can't enter it; cities can work it). Terrain defense and yields are in
  `game-rules.md`.
- Fog of war: never-seen hexes are blank; hexes you've seen but can't see now are under a grey
  veil with cloud puffs and show the cities, improvements and roads that were there when you last
  looked, but no units.
- In the city view, green outlines are worked tiles (red if cut off), the gold ring marked `M` is
  the manager,
  and dotted lines link it to its workers. Green grain and amber hammers show food and production,
  with the share that reaches the city.
