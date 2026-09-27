# Controls

Run `cargo run --release`. The city scenario opens by default. You control Blue; Red plans its
orders when you end the turn. The key map in code is `App::window_event` in `src/app.rs`; the
rules behind each action are in `game-rules.md`. This file is the only description of the
controls: the game's startup log (`CONTROLS_HELP` in `src/game/mod.rs`) just points here.

## Units

| Control | Action |
| --- | --- |
| Left-click your unit | Select it |
| Left-click a green hex | Move there this turn; click again to cancel |
| Right-click a hex in range | Attack it this turn: an enemy unit, city or barracks, or an empty hex someone may step into |
| Shift-left-click a hex | Queue one more turn moving toward it (as close as the unit gets that turn) |
| Shift-right-click a hex | Queue an attack on it: in the queue's last turn if that turn has none yet and it's in range, else in one more turn |
| Ctrl-click an adjacent ally | Queue a swap |
| M / X (or Move / Attack buttons) | Arm Move / Attack for the next map click (again to disarm) |
| Right-click (armed) | Disarm the armed action |
| Ctrl-right-click | Clear the selected unit's orders, queue, hold and guard |
| Q / ability button | Toggle the unit's ability (what each does: `game-rules.md`, Abilities) |
| Space / Hold button | Hold the unit this turn, keeping queued orders, and move on to what's next |
| G / Guard button | Guard: stay put and be skipped every turn until given an order (G again unguards) |
| Delete / Disband button, twice | Remove the selected unit for good (the first press asks to confirm) |
| Tab | Look at the next unit without holding this one; leave the city or barracks view |
| Alt-drag a box | Select every one of your units inside it as a group |
| Alt-click a unit | Add it to, or remove it from, the group |
| Hold Alt | Show extra map info: each unit's turn order, and every explored tile's yields |
| Left-click a hex with a group | Each member moves as close to it as it can get |
| Right-click a hex with a group | Every member in range attacks it (again to call it off) |
| Shift-left / Shift-right-click with a group | Queue one more turn for every member, so their queues stay the same length |
| F / Found City button | Found a city with the selected settler |
| Escape | Let go of the selected unit or group (and close the tile panel) |

A unit can queue a move and an attack; it attacks from the hex it moves to. Units can't move
through occupied hexes, and two allies can't head for the same hex. Once the selected unit has a
move and an attack queued (or can't do one of them), the next unit needing orders is selected
and the camera glides to it; a unit you select by clicking stays selected.

Shift-clicks build an order queue over several turns (rules: `game-rules.md`, Order queues).
Selection stays on the unit while you queue; let go of it (Escape, Tab, or click another unit)
when done. A queued unit doesn't hold up the turn, and any other order (a plain click or
right-click, swap, ability, guard, Ctrl-right-click) cancels its queue; Hold keeps it. Its plan
shows as turn numbers only while it is selected or the cursor is on it; otherwise a small `>N`
tag beside it counts the turns left.

## Workers

Workers live in their city and go out to do the jobs you queue (rules: `game-rules.md`,
Workers).

| Control | Action |
| --- | --- |
| Click a tile with nothing selected | Open its tile panel: terrain, whose workers would go, and a button per job |
| Road / Improve / Outpost / Fort buttons | Queue that job on the tile, for the open city or else your nearest one |
| R / I | Queue a road / an improvement on the tile in the tile panel |
| Wall / Gate buttons | Arm wall or gate placement on hex edges |
| Click near a hex edge (armed) | Queue a wall or gate on that edge (highlighted under the cursor) |
| Drag across hex edges (armed) | Queue one on every edge the cursor passes |
| Escape or right-click (armed) | Stop placing walls or gates |
| Escape | Close the tile panel |
| 8 / Worker button (city view) | Build a worker for the city |
| Drag a worker job onto another (city view) | Reorder the city's worker jobs |
| Click a worker job's X (city view) | Remove the job |
| Recall on a worker's row (city view), or Recall Worker in the tile panel of its tile | Send that worker straight home; its job goes back on top of the list |

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
| 8 | Queue a worker |
| Click a tile after queuing a Barracks, Mill or Workshop | Choose its site |
| Its card (or key) again while it's queued with no site | Resume choosing the site (leaving the city stops it) |
| Click a planned site's map badge | Move that site |
| Confirm button | Place a finished Barracks, Mill or Workshop on its site |
| Drag a queue row onto another | Reorder the queue |
| Click a row's X | Remove it (removing the item in progress loses its production) |
| Wheel over a long queue, or drag its scrollbar | Scroll the queue |
| Backspace | Remove the item being built |
| PageDown | Swap the first two queue items |
| See Barracks, or left-click your barracks with no view open | Open the barracks view (its own queue of all five unit types) |
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
| Hold Escape for 1 s | Quit (a prompt shows while it's held), once no view, selection or tile panel is open for it to close |
| F1 / F2 / F3 | Start the combat / city / frontier scenario (again to restart it) |
| F4 | Start a newly generated world; every press makes a new map |
| F6 | Save a snapshot of the whole game (in memory only) |
| F7 | Load the snapshot; it is kept, so it can be loaded again |
| F8 | Toggle turn playback: every step at once (the default) or step by step (same outcome) |
| F9 | Pay for the open city's or barracks' current build at once (debug; a building still needs its site and Confirm) |
| F10 | Toggle fog of war (on by default) |
| F11 | Switch between the ImGui and classic UI presentations |
| Ctrl (held, ImGui) | Show panel title bars and resize grips for rearranging |
| Ctrl+Shift+R in City / Building or Troop (ImGui) | Reset that view's Debug placement to Default |

The faded DEBUG panel at the top-left has buttons for F1-F4 and F6-F10; the current scenario is
gold, and a generated map shows its seed. Left clicks act on release, so a drag never issues an
order.

## Command line

Flags go after `--`, e.g. `cargo run --release -- --scenario world --seed 42`; `--help` lists
them.

| Flag | Effect |
| --- | --- |
| `--scenario <name>` | Start in `combat`, `cities` (the default), `frontier` or `world` (F1-F4) |
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
- A unit following an order queue shows no ghost. While it is selected (alone or in a group) or
  under the cursor, a line in team color runs along its moves, with each turn's number (1 is
  this turn) in a dark disc on the hex it moves to, and an orange-rimmed number on each queued
  attack's arrow; a hex or arrow used on several turns lists them ("2,3"). Otherwise only a
  `>N` tag at the unit's lower left shows, N being the turns of orders it has left.
- Gold ring: queued ability. Steel ring: deployed siege. White hex outline: guarding. Orange hex:
  contested.
- Units are tokens in team color with a pictogram of what they are: sword (melee), bow
  (ranged), horse head (cavalry), catapult (siege), spyglass (scout), shield (armored) on a
  disc. Settlers (a planted flag) are hollow hexagons.
- Workers out on the map are small hollow hexagons with a shovel (tucked into a corner when a
  unit shares their hex), with a dotted line to the job they're walking to. A dark tag with a
  shovel beside each of your cities counts its workers at home. Queued jobs are faded gold rings
  with the job's name.
- Structures: walls are stone bands along hex edges with posts in their owner's color; a gate is
  a wall with a door in its owner's color in the middle. An outpost is a watchtower on its tile,
  a fort a ring of stakes around it. Queued walls and gates are faded gold edges.
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
  veil with cloud puffs and show the cities, improvements, roads and structures that were there
  when you last looked, but no units or workers.
- In the city view, green outlines are worked tiles (red if cut off), the gold ring marked `M` is
  the manager; hovering the manager draws a dotted line along the way its goods travel to the
  city. Green grain and amber hammers show food and production, with the share that reaches the
  city.
