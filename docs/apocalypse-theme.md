# After the Fall theme (experiment, branch `claude/apocalypse-theme`)

A reskin of the whole game as the world after a modern collapse. The grid is gone, the fuel ran
out and the cities fell, and the survivors have rebuilt a middle-ages way of life out of what was
left behind. They fight with scrap-metal blades and salvaged compound bows, ride horses again and
farm the overgrown suburbs. The game is its first age, the **Salvage Age**. Later ages would climb
back up through the old ones, with rediscovered steam, rail and industry, and a few buildings
already hint at that climb (the Handcar Depot, the Cannery).

**Rules, numbers and code identifiers don't change.** `UnitType::Melee` is still `Melee` in code,
and `docs/game-rules.md` still describes the rules in their original terms. Only what the player
sees changes: display names, messages, tooltips, colours, pictograms, map icons and the look of
the map. This file is the glossary, and every part of the reskin uses these names so they match.
An earlier experiment, `claude/ocean-theme`, did the same for an undersea theme, and its commits
show where each kind of thing lives.

## The look

The old world shows through everywhere, reclaimed by nature: cracked asphalt with grass through
it, rusted car bodies, leaning power pylons, broken towers on the horizon and vines over
everything. The palette is **desaturated and weathered**: olive and moss greens, rust orange,
concrete and ash greys, dusty khaki, and bleached bone. Hazard amber stands out as the accent. The
new world's own work looks hand-made from salvage: plank and sheet-metal roofs, tyre walls, rope,
and flags cut from road signs.

## Terrain

| Code | Old name | New name | Look |
|---|---|---|---|
| `Terrain::Grassland` | GRASSLAND | OVERGROWTH | lush wild green, faint lines of old field rows or fences |
| `Terrain::Plains` | PLAINS | SPRAWL | dry yellow-olive grass over cracked grey paving, a few slab outlines |
| `Terrain::Desert` | DESERT | WASTELAND | bleached, cracked earth in a rust-tan colour, an odd bleached bone or tyre |
| `Terrain::Tundra` | TUNDRA | ASH FLATS | grey-brown ash, sparse dead stalks |
| `Terrain::Snow` | SNOW | DEAD ZONE | pale sickly grey-white, glassy and cracked, nothing grows |
| `Terrain::Marsh` | MARSH | DROWNED TOWN | murky olive water with roof peaks and posts sticking out |
| `Terrain::Mountains` | MOUNTAINS | DEAD CITY | a jagged skyline of broken, dark concrete towers (impassable) |
| `Terrain::Coast` | COAST | SHALLOWS | grey-green water with an oily sheen and a half-sunk wreck now and then |
| `Terrain::Ocean` | OCEAN | DEEP WATER | dark slate-blue |
| `Terrain::Lake` | LAKE | RESERVOIR | still water with a straight concrete edge |
| hills (`Tile::hills`) | ... HILLS | ... RUBBLE | mounds of broken concrete and rebar |
| `Feature::Forest` | FOREST | WILDWOOD | dense regrown trees, a pylon or rooftop poking through |
| `Feature::Jungle` | JUNGLE | KUDZU | vines smothering everything in a green blanket |
| rivers (hex edges) | RIVER | RIVER | murky brown-green water |
| fog of war | fog | DUST | brown-grey haze |

So a tile name reads, for example, "WASTELAND RUBBLE + KUDZU". The only message that names snow,
"NOTHING GROWS ON SNOW", becomes "NOTHING GROWS IN THE DEAD ZONE".

## Units

| Code | Old name | New name | Pictogram |
|---|---|---|---|
| `Melee` | MELEE | SCRAPPER | a fighter with a rebar spear behind a stop-sign shield |
| `Ranged` | RANGED | BOWMAN | a salvaged compound bow, drawn |
| `Cavalry` | CAVALRY | OUTRIDER | a rider on a horse, a scarf flying |
| `Siege` | SIEGE | TREBUCHET | a trebuchet made from crane parts, with a car tyre in its sling |
| `Armored` | ARMORED | RIOT GUARD | a riot helmet with its visor down, behind a riot shield |
| `Scout` | SCOUT | CYCLIST | a bicycle |
| `Settler` | SETTLER | CARAVAN | a handcart piled with bundles, with a flag |
| workers | WORKER | SALVAGER | a hard hat (or a figure in one, with a crowbar) |
| `PatrolGalley` | PATROL GALLEY | SKIFF | a rowing boat with oars out |
| `LandingCraft` | LANDING CRAFT | BARGE | a flat river barge |
| `BombardShip` | BOMBARD SHIP | RUST HULK | a rusted hull with a catapult on its deck |

Descriptions follow suit. The Scrapper is a "TOUGH CLOSE FIGHTER" and the Riot Guard is "HEAVY
SCRAP-PLATED INFANTRY" (it was "HEAVY IRON INFANTRY").

**Abilities:**

| Old | New |
|---|---|
| SHIELD WALL | BARRICADE |
| VOLLEY | ARROW RAIN |
| CHARGE | RIDE DOWN |
| DEPLOY / PACK UP | SET UP / PACK UP |
| LOOKOUT | BINOCULARS |

`src/game/ui/action_icons.rs` maps ability names to icons by their text, so it must match.

## Resources, specials, improvements, yields

| Old | New | Icon |
|---|---|---|
| HORSES (resource) | HORSES | a horse head: feral herds |
| IRON (resource) | WRECKS | a rusted car body: the old world's scrap |
| ORCHARD (special) | FERAL ORCHARD | a gnarled fruit tree |
| QUARRY (special) | RUBBLE PIT | stacked concrete blocks |
| FARM | HOMESTEAD | furrowed rows by a shack |
| MINE | SCRAP DIG | a pick in a heap of scrap |
| PASTURE | CORRAL | a fence of pallets |
| LUMBER MILL | SAWPIT | a log on trestles with a saw |
| FOOD (stock, yield) | FOOD | a tin can (yield pips) |
| WOOD | WOOD | a log |
| METAL | SCRAP | a gear or a bent piece of rebar |

So every message that names metal says scrap, for example "NEEDS HILLS OR WRECKS ON OR NEXT TO
THE TILE".

## Cities, buildings, structures

| Old | New | Look |
|---|---|---|
| CITY | ENCLAVE | a walled huddle of salvaged huts around a patched-up old tower |
| BARRACKS | GARRISON | |
| MILL | WINDMILL | |
| WORKSHOP | TINKER SHOP | |
| CANOE HOUSE | BOATYARD | |
| FORGE | SCRAP FORGE | |
| STABLE | STABLE | |
| WATCHPOST | RADIO TOWER | an old lattice mast, rigged as a lookout |
| FIELD HOSPITAL | CLINIC | |
| CANNERY | CANNERY | |
| WORK CAMP | WORK CAMP | |
| SMELTER | SMELTER | |
| RAILHEAD | HANDCAR DEPOT | old rails, run again with hand-pumped carts |
| HARBOR | DOCKS | |
| COASTAL BATTERY | HARPOON BATTERY | |
| ROAD (job) | ROAD | the old road, cleared |
| IMPROVE (job) | IMPROVE | |
| WALL | TYRE WALL | stacked tyres and sheet metal |
| GATE | BUS GATE | a school bus rolled across the gap |
| OUTPOST | WATCHFIRE | a fire in an oil drum on a platform |
| FORT | BUNKER | a concrete bunker, sandbagged |
| RUINS (map feature) | CACHE | a pre-fall supply stash: a crate or hatch. "RUINS CLAIMED" becomes "CACHE LOOTED" |

"CITY" appears in many messages and the city view's labels ("C OPENS YOUR CITY", "A CITY STANDS
HERE", "CITIZENS AUTO ASSIGNED"). Each of them says ENCLAVE instead, and a citizen can stay a
citizen. ENCLAVE is longer than CITY, so check that the text still fits (the UI layout tests
catch the classic UI's overflows).

## Sides

Team names and team colours stay as they are (BLUE, RED...), so sides stay easy to tell apart
and the multiplayer and AI text still makes sense. Tune a team colour only if it no longer reads
against the new map.

## UI palette

Panels look like weathered sheet metal: dark warm grey-brown, around 0.02 to 0.05 linear. Text is
an off-white bone colour. The gold accent becomes a **hazard amber**, and warnings are a rust
red. Colours are linear and the swapchain is sRGB, so dark panels are small numbers.

The window title is "After the Fall".
