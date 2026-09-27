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
| Right-click a hex in range | Attack it this turn: an enemy unit or barracks, or an empty hex someone may step into; empty city centers cannot be targeted |
| Shift-left-click a hex | Queue every turn of moves it takes to get there (around obstacles), after anything already queued, until the unit's queue holds as many turns as the queue limit (settings menu) allows |
| Shift-right-click a hex | Queue an attack on it: in the queue's last turn if that turn has none yet and it's in range, else in one more turn |
| Ctrl-click an adjacent ally | Queue a swap |
| M / X (or Move / Attack buttons) | Arm Move / Attack for the next map click (again to disarm) |
| Right-click (armed) | Disarm the armed action |
| Ctrl-right-click | Clear the selected unit's orders, queue, hold and guard |
| Q / ability button | Toggle the unit's ability (what each does: `game-rules.md`, Abilities) |
| Space / Hold button | Hold the unit this turn, keeping queued orders, and move on to what's next; on a unit already holding, stop holding (it's back in the turn order). Any new order also ends a hold |
| Clear Orders button | Clear the selected unit's (or group's) orders, queue, hold and guard, like Ctrl-right-click |
| G / Guard button | Guard: stay put and be skipped every turn until given an order (G again unguards) |
| Delete / Disband button, twice | Remove the selected unit for good (the first press asks to confirm) |
| Tab | Look at the next unit without holding this one; leave the city or barracks view |
| Left-drag a box on the map | Select every one of your units inside it (two or more become a group) |
| Shift-left-drag a box | Add the units inside it to the selection |
| Shift-click one of your units | Add it to the selection |
| Ctrl-click a unit in the group | Take it out of the group (with one unit selected, Ctrl-click swaps instead) |
| Hold Alt | Show extra map info: each unit's turn order, and every explored tile's yields |
| Left-click a hex with a group | Each member moves as close to it as it can get |
| Right-click a hex with a group | Every member in range attacks it (again to call it off) |
| Shift-left / Shift-right-click a hex with a group | Queue the turns for every member, so their queues stay the same length |
| Clear Orders button (group) / Ctrl-right-click | Clear every member's orders, queues, holds and guards |
| Click a chip in the turn strip | A city's: open the city. A group's: select all its units (listing them one by one below) and move the camera to them. A unit's: select just it |
| Shift-click / Ctrl-click a group or unit chip in the turn strip | Add its units to / take them out of the selection |
| F / Found City button | Found a city with the selected settler |
| Escape | Let go of the selected unit or group (and close the tile panel) |

A unit can queue a move and an attack; it attacks from the hex it moves to. Units can't move
through occupied hexes, and two allies can't head for the same hex. Once the selected unit has a
move and an attack queued (or can't do one of them), the game moves on to what needs you next,
in the turn strip's order: a city with nothing to build opens first, then the next unit
needing orders (settlers before the military) is selected and the camera glides to it. A turn,
and a new world, start the same way. A unit you select by clicking stays selected.

The turn strip ("need orders"; a panel that starts at the bottom of the screen, centered or as
near the middle as the other panels allow) shows a chip for everything you still have to see to
this turn, civilian tasks first: each city with nothing to build (its tower), your settlers,
and then your military units needing orders. Workers aren't listed and never hold up the turn. Units are grouped by kind, one chip per kind with a count; a group of several
that you select lists its units one by one on a second row, to pick from or take out. Selected
units, and the open city, are framed; a unit leaves the strip once it has its orders (or holds,
guards or follows a queue), and a city once it has a build. Like
the other panels, hold Ctrl to drag, resize or dock it.

Shift-clicks on a hex build an order queue over several turns (rules: `game-rules.md`, Order
queues); a Shift-click on one of your own units adds it to the selection instead.
Selection stays on the unit while you queue; let go of it (Escape, Tab, or click another unit)
when done. A queued unit doesn't hold up the turn, and any other order (a plain click or
right-click, swap, ability, guard, Ctrl-right-click) cancels its queue; Hold keeps it. While a
unit or group with a queue past this turn is selected, a plain click or right-click on the map
only warns ("CLICK AGAIN TO REPLACE ITS QUEUE") and outlines the hex in orange; the same click
again replaces the queue, so selecting a unit to look at its plan and clicking away can't wipe
it. Buttons and keys (Clear Orders, Guard, the ability) act at once. A plan
reaching past this turn shows as turn numbers only while it is selected or the cursor is on it;
otherwise a small `NT` tag beside it (`3T`: 3 turns) counts the turns of orders left. A plan of
this turn alone shows like plain orders (ghost and arrow). To cancel a unit's queued orders,
use Clear Orders (in the unit or group panel) or Ctrl-right-click, or just give it another
order.

## Workers

Workers live in their city and go out to do the jobs you queue (rules: `game-rules.md`,
Workers).

| Control | Action |
| --- | --- |
| Click a tile with nothing selected | Open its tile panel: terrain, whose workers would go, and a button per job. A white ring marks the tile |
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
| Escape while choosing a site | Cancel it: the building comes back out of the queue (the city stays open) |
| Its card (or key) while a finished building has no site | Resume choosing the site |
| Click a planned site's map badge | Move that site |
| Confirm button | Place a finished Barracks, Mill or Workshop on its site |
| Drag a queue row onto another | Reorder the queue |
| Click a row's X | Remove it (removing the item in progress loses its production) |
| Wheel over a long queue, or drag its scrollbar | Scroll the queue |
| Backspace | Remove the item being built |
| PageDown | Swap the first two queue items |
| See Barracks, or left-click your barracks with no view open | Open the barracks view (its own queue of all five unit types) |
| Click the city center while in city view | Enter that city's tactical interior map |
| V / City Interior button | Open the selected or hovered city's interior; press again to return |
| Click a Blue troop, then another interior hex | Queue its independent move or attack on the map |
| Backspace in the interior | Clear the selected copy's orders |
| Escape / V / Return to City in the interior | Return to the city view |
| Open City (in the barracks view) | Go back to the city view |
| Space / Escape / click off the map | Close the city or barracks view |
| Tab | Close the city or barracks view |

While a city view is open, map clicks manage tiles and never select field units. Clicking the
city center enters the interior map; its clicks control only the interior copies.

## Camera, game and testing

| Control | Action |
| --- | --- |
| Wheel | Zoom (except over a scrollable queue) |
| Middle-drag | Pan (a left-drag draws a selection box instead) |
| Rest the cursor on a hex | After 0.75 s, a tooltip: terrain, yields, site, road, who works it, units |
| Hover a unit | Its stats, in a box at the top-right |
| Hover a city or barracks | Its production and current build (plus city growth or barracks HP), at the bottom-left |
| F5 | Toggle borderless fullscreen |
| Escape with nothing open | Open the settings menu (below), which has the Quit button; Escape again, or its Close button, closes it |
| F1 / F2 / F3 | Start the combat / city / frontier scenario (again to restart it) |
| F4 | Start a newly generated world, for you and the AI sides the World AI setting asks for; every press makes a new map |
| F12 | Start a siege at Red's city with its interior open |
| F6 | Save a snapshot of the whole game (in memory only) |
| F7 | Load the snapshot; it is kept, so it can be loaded again |
| F8 | Toggle turn playback: every step at once (the default) or step by step (same outcome); also in the settings menu |
| F9 | Pay for the open city's or barracks' current build at once (debug; a building still needs its site and Confirm) |
| F10 | Toggle fog of war (on by default) |
| F11 | Switch between the ImGui and classic UI presentations |
| Ctrl (held, ImGui) | Show panel title bars and resize grips for rearranging |
| Ctrl+Shift+R in City / Building or Troop (ImGui) | Reset that view's Debug placement to Default |

The faded DEBUG panel at the top-left has buttons for F1-F4, F12 and F6-F10; the current scenario is
gold, and a generated map shows its seed. Left clicks act on release, so a drag never issues an
order.

## Settings menu

Escape opens the settings menu once there's nothing else for it to close. Each press closes one
thing, in this order: the settings menu itself, a city interior, a site being chosen, a city or
barracks view, wall or gate placement, then the selection and the tile panel. The menu opens
in the middle of the screen, over the map; in the ImGui presentation, hold Ctrl to drag,
resize or dock it like the other panels. The game carries on while it's open.

| Control | Action |
| --- | --- |
| < / > beside a setting | Step it down / up (a button is faded at that end of the setting's range) |
| Close button, or Escape | Close the menu |
| Quit button | Close the game |

| Setting | Values |
| --- | --- |
| Turn playback | All at once (the default) or step by step, the same switch as F8 |
| Queue limit | 1 to 20 turns (6 by default): the most turns a unit can have queued, this turn included. A Shift-click toward a hex farther away queues the move as far as the limit goes, and once a queue is full, Shift-clicks add nothing to it until turns are played |
| Fog | Clouds (the default) or solid grey: how unexplored land is hidden under fog of war |
| World AI | 4-6 by map (the default: picked by the map's seed), or 1 to 6: AI sides in the next world (F4) |
| World start | City (the default) or settler: what every side in the next world (F4) starts with, beside its scout |

Settings, and whether the menu is open, stay as they are across scenario switches (F1-F4, F12)
and loads (F7).

Settings are also kept between sessions, saved as soon as one changes. On quitting, the game
also saves the window's size (and whether it's maximized), the UI presentation (F11), and the
ImGui panels as you arranged them: where each is, its size, which are docked or collapsed, and
the boxes. The next session opens the same way. They're kept in `%APPDATA%\riskofcivlike`
(`~/.config/riskofcivlike` elsewhere); delete that folder to start over from the defaults.
Screenshot mode (`--screenshot`) ignores it.

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
  with no enemy unit, worker or barracks (an empty city center cannot be targeted), OUT OF RANGE a
  target that moved away. Damage numbers rise from every unit hurt.
- A unit whose order queue reaches past this turn (one queued for this turn only shows the
  usual ghost and arrow): while it is selected (alone or in a group) or under the cursor, a
  line in team color runs from it along its moves, with each turn's number (1 is this turn) in
  a dark disc on the hex it moves to, ending in its faded ghost where the plan leaves it (the
  last number just under the ghost), and an orange-rimmed number on each queued attack's arrow.
  Each unit's plan is its own: where several units stop on one hex, their numbers fan out
  around it instead of merging, and a hex or arrow one unit uses on several turns lists them
  ("2,3"). Otherwise only an `NT` tag at the unit's lower left shows (`3T`: 3 turns of orders
  left).
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
- Fog of war: never-seen hexes are under clouds; hexes you've seen but can't see now are under a
  grey veil and show the cities, improvements, roads and structures that were there when you
  last looked, but no units or workers.
- In the city view, green outlines are worked tiles (red if cut off), the gold ring marked `M` is
  the manager; hovering the manager draws a dotted line along the way its goods travel to the
  city. Green grain and amber hammers show food and production, with the share that reaches the
  city.
