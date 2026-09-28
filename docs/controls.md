# Controls

Run `cargo run --release`. The city scenario opens by default. You control Blue; Red plans its
orders when you end the turn. The key map in code is `App::window_event` in `src/app.rs`; the
rules behind each action are in `game-rules.md`. This file is the only description of the
controls: the game's startup log (`CONTROLS_HELP` in `src/game/mod.rs`) just points here.

The game shows its things by their After the Fall names (`apocalypse-theme.md`: an enclave for a
city, a garrison for a barracks, salvagers for workers and so on). This file names buttons and
settings as they read on screen, and otherwise uses the rules' own terms, like `game-rules.md`.

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
| Hold Alt | Show extra map info: each unit's turn order, and every explored tile's yields (in the worker menu with yields off, its city's delivery percentages too) |
| Left-click a hex with a group | Each member moves as close to it as it can get |
| Right-click a hex with a group | Every member in range attacks it (again to call it off) |
| Shift-left / Shift-right-click a hex with a group | Queue the turns for every member, so their queues stay the same length |
| Clear Orders button (group) / Ctrl-right-click | Clear every member's orders, queues, holds and guards |
| Click a chip in the turn strip | A city's: open the city. A group's: select all its units (listing them one by one below) and move the camera to them. A unit's: select just it |
| Shift-click / Ctrl-click a group or unit chip in the turn strip | Add its units to / take them out of the selection |
| F / Found Enclave button | Found a city with the selected settler |
| Escape | Let go of the selected unit or group |

A unit can queue a move and an attack; it attacks from the hex it moves to. Units can't move
through occupied hexes, and two allies can't head for the same hex. Once the selected unit has a
move and an attack queued (or can't do one of them), the game moves on to what needs you next,
in the turn strip's order: a city with nothing to build opens first, then the next unit
needing orders (settlers before the military) is selected and the camera glides to it. A turn,
and a new world, start the same way. A unit you select by clicking stays selected.

The turn strip ("need orders"; a panel that starts at the bottom of the screen, centered or as
near the middle as the other panels allow) shows a chip for everything you still have to see to
this turn, civilian tasks first: each city with nothing to build (its tower), your settlers, and
then your military units needing orders. Units are grouped by kind, one chip per kind with a count; a group of several
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
otherwise a small tag beside it, the clock and a number (clock 3: 3 turns), counts the turns of orders left. A plan of
this turn alone shows like plain orders (ghost and arrow). To cancel a unit's queued orders,
use Clear Orders (in the unit or group panel) or Ctrl-right-click, or just give it another
order.

## Workers

Workers live in their city and go out to build what the city places on the map: roads,
improvements, walls, gates, outposts and forts, and its buildings with a site (rules:
`game-rules.md`, Workers). Everything is placed from the open city.

| Control | Action |
| --- | --- |
| A work (WORKS) or building card in the city's production list, or R / I | Pick it to place (again to put it down): tiles your workers can reach (3 from a city or connected Work Camp, or next to a road) are lit and the rest dimmed. A dimmed card has no worker to build it, or its price can't be paid; its tooltip says which |
| Click or drag over tiles (a tile job picked) | Place it on each tile, paying its price (a ring under the cursor shows where, red where it can't go) |
| Click a lit tile (a building picked) | Place the building's site; a worker walks out and builds it |
| Click or drag along hex edges (a wall or gate picked) | Place one on each edge the cursor touches (highlighted under the cursor) |
| Escape or right-click (something picked) | Stop placing, leaving the city open |
| Click a placed job (city panel) | Show it on the map |
| Drag a placed job onto another (city panel) | Reorder the city's jobs |
| Click a placed job's X (city panel) | Take it off, refunding its price |
| Click a worker's row (city panel) | Show the worker on the map |
| Recall beside a worker's row (city panel) | Send that worker straight home; its job goes back on top of the list |
| 8 / Salvager card (city view) | Build a worker for the city |

## Turn

| Control | Action |
| --- | --- |
| Space with nothing left to do | End the turn |
| End Turn button | Hold every unfinished unit, then end the turn (or open a city that still needs a build) |

The End Turn button names what the turn is waiting on ("3 UNITS NEED ORDERS", "CHOOSE
PRODUCTION") and turns amber, reading END TURN, once nothing is. Clicks are ignored while a turn
plays out.

## Cities

| Control | Action |
| --- | --- |
| C | Open a city needing a build (or your first city); from a barracks view, its city |
| Left-click your city | Open its city view |
| Click a tile in the city view | Assign a citizen there, or release one |
| Click the manager (`M`), then a tile | Move the manager; workers follow where they can |
| A | Auto-assign citizens by the city's labor focus |
| Food / Wood / Scrap / Balanced buttons (tin can, log, gear, scale) | Set the labor focus (and re-assign) |
| Y / Menu > Enclave Yields | Show or hide tile yields around the open city; Menu opens Settings without closing the city |
| 1 / 2 / 3 | Queue Melee / Ranged / Siege in the city, paid from the stockpile (a barracks trains them twice as fast; Cavalry and Armored only train at a barracks on Horses / Iron) |
| 5 / 6 / 7 | Pick a Barracks / Mill / Workshop to place for the workers |
| Scroll inside the city production list | Browse unit, building and work cards in one list; coastal cards appear only in eligible cities |
| Build a Harbor in a coastal city, then use the city unit cards | Queue Patrol Galley, Landing Craft or Bombard Ship for sea deployment |
| Select a land troop, then click an adjacent friendly Landing Craft | Board it after combat (maximum four passengers) |
| Select a Landing Craft, then click adjacent open land | Land its first passenger after combat |
| Click a green Railhead while selecting a troop beside its city | Queue a one-turn transfer there if its road link is open |
| 8 | Queue a worker |
| 9 / Grow card | Queue one more citizen, paid in food (the city doesn't grow on its own) |
| 0 / Gather card | Spend a turn gathering: free, and 2 food, 2 wood and 1 metal come in when it's done |
| A dimmed build card | The stockpile can't pay for it, or (Cavalry, Armored) the barracks has no deposit or its cap is used up; its tooltip says why. Prices show as resource icons and amounts, turns after a clock |
| Drag a queue row onto another | Reorder the queue |
| Click a row's X | Remove it, refunding its price (removing the item in progress loses its progress) |
| Wheel over a long queue, or drag its scrollbar | Scroll the queue |
| Backspace | Remove the item being built |
| PageDown | Swap the first two queue items |
| See Garrison, or left-click your barracks with no view open | Open the barracks view (its own queue of all five unit types, and how many Cavalry and Armored its deposits still allow) |
| Click the city center while in city view | Enter that city's tactical interior map |
| V / Enclave Interior button | Open the selected or hovered city's interior; press again to return |
| Click a Blue troop, then another interior hex | Queue its independent move or attack on the map |
| Backspace in the interior | Clear the selected copy's orders |
| Escape / V / Return to Enclave in the interior | Return to the city view |
| Open Enclave (in the barracks view) | Go back to the city view |
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
| Hover a city or barracks | What the city delivers and its population, or the barracks' HP, and the current build, at the bottom-left |
| F5 | Toggle borderless fullscreen |
| Escape with nothing open | Open the settings menu (below), which has the Quit button; Escape again, or its Close button, closes it |
| F1 / F2 / F3 | Start the combat / city / frontier scenario (again to restart it) |
| F4 | Start a newly generated world, for you and the AI sides the AI Players setting asks for; every press makes a new map |
| F12 | Start a siege at Red's city with its interior open |
| NAVAL in the Debug panel | Start the coastal naval test scenario |
| F6 | Save a snapshot of the whole game (in memory only) |
| F7 | Load the snapshot; it is kept, so it can be loaded again |
| F8 | Toggle turn playback: every step at once (the default) or step by step (same outcome); also in the settings menu |
| F9 | Finish the open city's or barracks' current build at once (debug) |
| F10 | Toggle fog of war, the dust (on by default) |
| F11 | Switch between the ImGui and classic UI presentations |
| Ctrl (held, ImGui) | Show panel title bars and resize grips for rearranging |
| Ctrl+Shift+R in Enclave / Building or Troop (ImGui) | Reset that view's Debug placement to Default |

The faded DEBUG panel at the top-left has buttons for F1-F4, F12 and F6-F10 (F9 is FINISH
BUILD), PROD SPEEDUP, which switches the stockpile economy's variant where a city's production
speeds its builds, and UNIT CAP, which switches the Cavalry and Armored cap between counting
those alive and every one ever trained (`rts-economy.md`); the current scenario is amber, and a generated map shows its seed. Left clicks act on release, so a drag never issues an
order.

## Settings menu

Escape opens the settings menu once there's nothing else for it to close. Each press closes one
thing, in this order: the settings menu itself, a city interior, something being placed for a
city's workers, a city or barracks view, then the selection. The menu opens
in the middle of the screen, over the map; in the ImGui presentation, hold Ctrl to drag,
resize or dock it like the other panels. The game carries on while it's open.

The settings are grouped under headings (Turns, Map, Next World), each with its name on the
left and its control beside it; hover over either for what the setting does.

| Control | Action |
| --- | --- |
| Checkbox | Switch an on/off setting |
| Slider | Drag or click to pick a number; the slider shows the value |
| Buttons side by side | Pick one of a few named values; the current one is amber |
| Drop-down list | Pick one of a longer list of values |
| Close button, or Escape | Close the menu |
| Quit button | Close the game |

The classic presentation (F11) has no sliders, checkboxes or lists: an on/off setting there
is a pair of OFF / ON buttons, and a slider or list is a < / > pair beside its value (faded at
that end of the range).

| Setting | Control | Values |
| --- | --- | --- |
| Instant playback (Turns) | Checkbox | On (the default): a turn plays out all at once; off: step by step. The same switch as F8 |
| Queue limit (Turns) | Slider | 1 to 20 turns (6 by default): the most turns a unit can have queued, this turn included. A Shift-click toward a hex farther away queues the move as far as the limit goes, and once a queue is full, Shift-clicks add nothing to it until turns are played |
| Dust (Map) | Buttons | Clouds (the default) or solid grey: how unexplored land is hidden under fog of war |
| AI players (Next World) | Drop-down list | 4-6 by map (the default: picked by the map's seed), or 1 to 6: AI sides in the next world (F4) |
| Start with (Next World) | Buttons | Enclave (the default) or caravan: what every side in the next world (F4) starts with, beside its scout |

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
| `--scenario <name>` | Start in `combat`, `cities` (the default), `frontier`, `world`, `siege` (F12), or `naval` |
| `--seed <n>` | With `--scenario world`: generate map number `n` (the seed the debug panel shows) |
| `--size <W>x<H>` | Open the window at this size in pixels |
| `--screenshot <file>` | Draw the scenario's first moments in a hidden window, save a frame as a PNG (1600x900 unless `--size`), and exit; for checking visual changes without playing |

## Reading the map

- Holding Alt shows blue and red numbers on each unit: its move and attack ranks in the turn
  order.
- Tile yields sit below the unit spot: tin cans for food, logs for wood, then gears for
  metal, one per point laid out like a die's pips (3 a triangle, 4 a square, 5 a square with one in the
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
  ("2,3"). Otherwise only a clock tag at the unit's lower left shows (clock 3: 3 turns of orders
  left).
- Gold ring: queued ability. Steel ring: deployed siege. White hex outline: guarding. Orange hex:
  contested.
- Units are tokens in team color with a pictogram of what they are: a stop-sign shield and
  rebar spear (melee), a drawn compound bow (ranged), a rider on a horse (cavalry), a trebuchet
  with a tyre in its sling (siege), a bicycle (scout), a riot helmet over a riot shield (armored),
  a rowing boat (Patrol Galley), a flat barge (Landing Craft), a rusted hull with a catapult
  (Bombard Ship) on a disc. Settlers (a handcart with a flag) are hollow hexagons.
- Workers out on the map are small hollow hexagons with a hard hat and crowbar (tucked into a
  corner when a unit shares their hex), with a dotted line to the job they're walking to. A dark
  tag with a hard hat beside each of your cities counts its workers at home. Queued jobs are faded gold rings
  with the job's name.
- Structures: walls are tyre walls along hex edges (tyres stacked against sheet metal) with posts
  in their owner's color; a gate is a school bus across the middle with a stripe of its owner's
  color. An outpost is a watchfire (a fire in an oil drum on a platform in its owner's color) on
  its tile, a fort a bunker: a ring of sandbags and concrete blocks around it. Queued walls and
  gates are muted gold edges with rounded ends, so a run of them reads as one line.
- Cities are patched-up tower blocks in their owner's color showing their population, with HP
  bars; a barracks is a Quonset hut (the garrison) in its owner's color. Other buildings are
  sheet-metal plates in their own color with a pictogram: a wind pump (Mill), a wrench and
  hammer (Workshop), a boat on its stocks (Canoe House), an anvil (Forge), a horseshoe (Stable),
  a lattice radio mast (Watchpost), a cross (Field Hospital), stacked tins (Cannery), a tarp
  tent (Work Camp), a crucible pouring (Smelter), a handcar (Railhead), an anchor (Harbor), a
  harpoon on its mount (Coastal Battery). A building placed for the workers shows as a faded
  gold ring named with it until a worker builds it. Improvements are small icons in a hex's
  top-left corner: crop rows by a shack (farm), a pick in a scrap heap (mine), two pallets
  (pasture), a log on sawhorses with a pit saw (lumber mill). Resources are icons in the
  top-right corner: a horse's head (Horses), a rusted car wreck (Iron). Special tiles and ruins
  sit in the bottom-left corner: a gnarled fruit tree (Orchard), cinder blocks (Quarry), a
  supply crate with a hazard band (ruins). Brown lines are dirt roads, and blue lines between
  hexes are rivers.
- Terrain: two small peaks are hills, pines forest, round canopies jungle; dunes, grass tufts and
  reeds mark desert, tundra and marsh. A large snowy peak is a mountain (impassable) and waves
  are water (ships enter it, land units do not; cities can work it). Terrain defense and yields are in
  `game-rules.md`.
- Fog of war: never-seen hexes are under clouds; hexes you've seen but can't see now are under a
  grey veil and show the cities, improvements, roads and structures that were there when you
  last looked, but no units or workers.
- In the city view, green outlines are worked tiles (red if cut off), the gold ring marked `M` is
  the manager; hovering the manager draws a dotted line along the way its goods travel to the
  city. Green grain and amber hammers show food and production, with the share that reaches the
  city.
