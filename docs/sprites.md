# Pixel-art sprites

The map can be drawn two ways: the vector art (the default) or pixel-art sprites. The **Art**
setting (Escape, Map) switches between them in game, and `--art pixel` starts the game in pixel
art (`--art vector` in vector), screenshots included. Sprites are PNG files named by a tag, so
they can be drawn and swapped one at a time. Code: `src/game/pixel_art.rs`.

## Adding or replacing a sprite

1. Draw a PNG at the size the table below gives for its tag.
2. Save it as `assets/sprites/<tag>.png`, for example `assets/sprites/unit/pikeman.png`.
3. Start the game from the repository (`cargo run --release`): it reads the folder at startup,
   so no rebuild is needed. Restart the game to see a change.
4. Commit the file. A build compiles in whatever `assets/sprites/` holds, so the executable run
   on its own (without the folder beside it) has the sprites too; rebuild to update those copies.

Where the game looks: an `assets/sprites/` folder in the working directory, then one beside the
executable. If it finds a folder it uses only that folder; otherwise it uses the copies built in.
The log says which, and how many sprites it found (`pixel art: 57 of 57 sprites, from ...`). It
also warns about a PNG it can't read and about a file whose name isn't a tag (likely a typo).

A tag with no file is drawn in vector art, so deleting a file shows its vector art again. A
terrain tile needs every sprite it is made of: a forested tile whose ground has a sprite but
whose forest doesn't is drawn in vector art whole.

## The format

- **PNG**, any color type (RGBA, RGB, grey, palette), sRGB. Alpha is straight, not premultiplied;
  fully transparent pixels are empty.
- **Size.** Each tag has a size in texels (below). All sprites share one texel size on the map:
  a terrain tile's 32 texels span a hex. A PNG of another size is scaled into its tag's box,
  so keep the shape (the log warns if the shape differs, as it would look stretched).
- **Anchor: the center.** The middle of the image sits on the middle of the hex (terrain,
  features, units, the city, buildings) or of the icon's spot (icons sit in a hex's corners, as
  the vector icons do).
- **Team color: magenta.** Any pixel of pure magenta, of any brightness (red equal to blue, no
  green: `#FF00FF`, `#B400B4`, `#800080`), is painted in the side's color: `#FF00FF` is the
  side's color itself and darker magentas are darker shades of it. Units, the city and
  buildings use it; on terrain and icons, magenta shows as white. Avoid pure magenta for
  anything else.

## How it is drawn

- Texels are read one by one: no filtering and no mipmaps, so nothing blurs. Within one screen
  pixel of the edge between two texels the shader blends the two, so edges stay sharp but don't
  shimmer as the map pans. Zoomed all the way out a texel is a little under a screen pixel, and
  there it blends like ordinary bilinear filtering.
- Terrain tiles and features are snapped to one texel grid across the map. Units, the city,
  buildings and icons aren't, so moving units glide smoothly.
- What goes over the sprites stays as it is: hex borders, the fog's veil over remembered hexes,
  a selected unit's move and attack hexes (a tint over the tile), health bars, status rings,
  the damage preview, the population on a city, labels and yields.
- Still vector in pixel mode: roads, rivers, walls and structures, the yield chips, the city
  interior, and all screen UI (panels, the unit tray, the turn strip).

## Tags

| Tag | Size | What it is |
|---|---|---|
| `terrain/grassland` | 64x56 | Grassland |
| `terrain/plains` | 64x56 | Plains |
| `terrain/desert` | 64x56 | Desert |
| `terrain/tundra` | 64x56 | Tundra |
| `terrain/snow` | 64x56 | Snow |
| `terrain/marsh` | 64x56 | Marsh |
| `terrain/mountains` | 64x56 | Mountains |
| `terrain/coast` | 64x56 | Coast |
| `terrain/ocean` | 64x56 | Ocean |
| `terrain/lake` | 64x56 | Lake |
| `terrain/grassland_hills` | 64x56 | Grassland hills |
| `terrain/plains_hills` | 64x56 | Plains hills |
| `terrain/desert_hills` | 64x56 | Desert hills |
| `terrain/tundra_hills` | 64x56 | Tundra hills |
| `terrain/snow_hills` | 64x56 | Snow hills |
| `terrain/marsh_hills` | 64x56 | Marsh hills |
| `feature/forest` | 64x56 | Forest, drawn over the ground (hills or not): transparent where the ground shows |
| `feature/jungle` | 64x56 | Jungle, drawn over the ground the same way |
| `unit/melee` | 28x28 | Melee |
| `unit/ranged` | 28x28 | Ranged |
| `unit/cavalry` | 28x28 | Cavalry |
| `unit/siege` | 28x28 | Siege |
| `unit/scout` | 28x28 | Scout |
| `unit/armored` | 28x28 | Armored |
| `unit/pikeman` | 28x28 | Pikeman |
| `unit/patrol_galley` | 28x28 | Patrol galley |
| `unit/landing_craft` | 28x28 | Landing craft |
| `unit/bombard_ship` | 28x28 | Bombard ship |
| `unit/settler` | 28x28 | Settler |
| `unit/worker` | 28x28 | Worker (in the field) |
| `unit/wolf_pack` | 28x28 | Wolf pack (team color: the animals' brown) |
| `unit/bear` | 28x28 | Bear (team color: the animals' brown) |
| `building/city` | 32x32 | A city; the game writes its population over the middle |
| `building/barracks` | 28x28 | Barracks |
| `building/mill` | 28x28 | Mill |
| `building/workshop` | 28x28 | Workshop |
| `building/canoe_house` | 28x28 | Canoe House |
| `building/forge` | 28x28 | Forge |
| `building/stable` | 28x28 | Stable |
| `building/watchpost` | 28x28 | Watchpost |
| `building/field_hospital` | 28x28 | Field Hospital |
| `building/cannery` | 28x28 | Cannery |
| `building/work_camp` | 28x28 | Work Camp |
| `building/smelter` | 28x28 | Smelter |
| `building/railhead` | 28x28 | Railhead |
| `building/harbor` | 28x28 | Harbor |
| `building/coastal_battery` | 28x28 | Coastal Battery |
| `icon/horse_head` | 16x16 | Horses (resource) |
| `icon/ingot` | 16x16 | Iron (resource) |
| `icon/wheat` | 16x16 | Farm |
| `icon/ore_cart` | 16x16 | Mine |
| `icon/fence` | 16x16 | Pasture |
| `icon/logs` | 16x16 | Lumber mill |
| `icon/ruins` | 16x16 | Ruins (drawn 1.3 times as big) |
| `icon/fruit_tree` | 16x16 | Orchard (special tile, drawn 1.3 times as big) |
| `icon/stone_blocks` | 16x16 | Quarry (special tile, drawn 1.3 times as big) |
| `icon/paw` | 16x16 | Animal den (drawn 1.3 times as big) |
| `icon/food` | 16x16 | Food, in text (prices, stockpiles, yields) |
| `icon/wood` | 16x16 | Wood, in text |
| `icon/metal` | 16x16 | Metal, in text |
| `icon/clock` | 16x16 | Turns, in text |
| `ui/move` | 16x16 | Move order button |
| `ui/attack` | 16x16 | Attack order button |
| `ui/swap` | 16x16 | Swap order button |
| `ui/shield_wall` | 16x16 | Shield Wall ability button |
| `ui/volley` | 16x16 | Volley ability button |
| `ui/charge` | 16x16 | Charge ability button |
| `ui/deploy` | 16x16 | Deploy and Pack Up ability button |
| `ui/lookout` | 16x16 | Lookout ability button |
| `ui/hold` | 16x16 | Hold order button |
| `ui/guard` | 16x16 | Guard order button |
| `ui/alert` | 16x16 | Alert order button |
| `ui/clear` | 16x16 | Clear Orders button |
| `ui/disband` | 16x16 | Disband button |
| `ui/settle` | 16x16 | Found City (settle) button |
| `cloud/tile_1` | 64x56 | Cloud cover over an unexplored hex (Clouds test setting: Tiles), one of four picked per hex |
| `cloud/tile_2` | 64x56 | Cloud cover, the second |
| `cloud/tile_3` | 64x56 | Cloud cover, the third |
| `cloud/tile_4` | 64x56 | Cloud cover, the fourth |
| `cloud/puff_small` | 48x32 | A drifting cloud puff (Clouds test setting: Puffs), the smallest |
| `cloud/puff_medium` | 80x56 | A cloud puff, middle-sized |
| `cloud/puff_large` | 112x80 | A cloud puff, the largest |

The tests check that this table lists every tag at its size (`pixel_art.rs`), and that every
shipped file is a tag at its size.

## The dev art

The shipped sprites are placeholders drawn by `node tools/sprites/dev-art.mjs` (no
dependencies): speckled hex tiles, tokens like the vector ones (a team-colored disc, or a hexagon
for civilians, with a dark pictogram), houses marked with the vector badges' letters, and small
icons. Running it **overwrites every file it makes**, hand-drawn ones included, so once a tag
has real art, take it out of the script. `node tools/sprites/dev-art.mjs --sheet out.png
[prefix]` writes every sprite (or those whose tag starts with `prefix`, such as `unit/`) blown
up six times on one sheet, for looking them over, and touches nothing else.
