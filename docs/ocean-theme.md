# Ocean theme (experiment, branch `claude/ocean-theme`)

A reskin of the whole game as an undersea war between two schools of sea creatures. **Rules,
numbers and code identifiers don't change**: `UnitType::Melee` is still `Melee` in code and
`docs/game-rules.md` still describes the rules in their original terms. Only what the player
sees changes: display names, colors, pictograms, map icons and the look of the map. This file is
the glossary: every part of the reskin uses these names so they match.

## The world, flipped

Units are sea creatures, so the sea floor is the ground they swim over and **dry land is what
they can't enter**: the old water tiles become beaches and islands that cities still harvest.

| Code | Old name | Ocean name | Look |
|---|---|---|---|
| `Terrain::Grassland` | GRASSLAND | SEAGRASS | lush green-teal meadow, swaying blades |
| `Terrain::Plains` | PLAINS | SANDBANK | warm pale sand, ripples |
| `Terrain::Desert` | DESERT | SILT | dull grey-brown, barren |
| `Terrain::Tundra` | TUNDRA | COLD SHELF | slate blue-grey |
| `Terrain::Snow` | SNOW | ICE SHELF | pale ice blue-white |
| `Terrain::Marsh` | MARSH | MUD FLATS | olive-brown murk |
| `Terrain::Mountains` | MOUNTAINS | SEAMOUNT | dark basalt spires (impassable) |
| `Terrain::Coast` | COAST | BEACH | dry sand above the waterline, foam edge (impassable, workable) |
| `Terrain::Ocean` | OCEAN | ISLAND | sand with a palm tree (impassable, workable) |
| `Terrain::Lake` | LAKE | TIDE POOL | bright turquoise pool ringed by rock |
| hills (`Tile::hills`) | ... HILLS | ... REEF | raised coral ridges |
| `Feature::Forest` | FOREST | KELP FOREST | tall swaying kelp stalks |
| `Feature::Jungle` | JUNGLE | CORAL THICKET | branching bright coral |
| rivers (hex edges) | RIVER, FRESH WATER | CURRENT, UPWELLING | a lighter streaming band with flow dashes |
| fog of war | fog | MURK | dark deep-water haze |

## Creatures (units)

| Code | Old name | Ocean name | Pictogram |
|---|---|---|---|
| `Melee` | MELEE | SWORDFISH | swordfish in profile, long bill |
| `Ranged` | RANGED | PUFFERFISH | round spiky puffer |
| `Cavalry` | CAVALRY | SEAHORSE | seahorse, curled tail |
| `Siege` | SIEGE | OCTOPUS | octopus, curling arms (ink artillery) |
| `Armored` | ARMORED | CRAB | crab with raised claws |
| `Scout` | SCOUT | DOLPHIN | leaping dolphin |
| `Settler` | SETTLER | SEA TURTLE | turtle carrying a flag on its shell |
| workers | WORKER | SHRIMP | little shrimp |

Abilities: Shield Wall -> SHELL WALL, Volley -> SPINE VOLLEY, Charge -> RAM, Deploy -> INK
BARRAGE, Lookout -> ECHOLOCATE.

## Resources, improvements, yields

| Old | Ocean | Icon |
|---|---|---|
| HORSES | SEAHORSES | a seahorse head (top-right corner) |
| IRON | PEARLS | a pearl in an open oyster |
| farm | KELP FARM | a kelp frond |
| mine | CORAL QUARRY | a chunk of cut coral / a pick in coral |
| pasture | SEAHORSE PEN | a ring of coral posts |
| lumber mill | DRIFTWOOD | a sunken log |
| food | FISH | a small fish (yield pips) |
| production | SHELLS | a scallop shell (yield pips) |

## Cities, buildings, structures

| Old | Ocean |
|---|---|
| CITY | REEF (a reef city: coral towers) |
| GRANARY | CLAM LARDER |
| BARRACKS | SHIPWRECK (troops train in a sunken wreck) |
| MILL | WHIRLPOOL MILL |
| WORKSHOP | SUNKEN FORGE |
| ROAD (job) | SEA LANE |
| IMPROVE (job) | IMPROVE |
| WALL | CORAL WALL |
| GATE | SPONGE GATE |
| OUTPOST | LANTERN POST (a lanternfish lamp) |
| FORT | CLAM FORT (a giant clam) |

## Palette

Deep navy background, sea-glass UI. Panels are dark blue-green; the gold accent becomes a pale
pearl/aqua. Team colors stay readable: Blue becomes a bright tropical cyan-blue, Red a coral
orange-red. Remember colors are linear and the swapchain is sRGB (dark panels are around
0.01-0.05).

Window title: "Reef Wars".
