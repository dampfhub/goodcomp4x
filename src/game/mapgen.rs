//! Random maps for the world scenario (F4): a Pangea on a rectangular map
//! wider than tall, sized for the number of players. Everything comes from one
//! seed, so a seed always rebuilds the same map, on every machine: a network
//! game's guests build the host's map from its seed. So all randomness is
//! `Rng` and `Noise`, decisions never hang on hash-map order, and the float
//! math is only what IEEE 754 rounds the same everywhere (`+ - * /`, `sqrt`,
//! `floor`), never `sin`, `exp` or `powf` (`powi` neither, to be safe).
//!
//! The recipe, a function per stage (`shape_world` runs 1-8):
//! 1. Land and sea (`raise_continent`): an oval dome, bent by warping noise
//!    and roughened by two layers of fractal value noise into bays,
//!    peninsulas and inland seas. The lowest 42-52% of hexes (varying per
//!    map) is sea. The biggest landmass stays; other land sinks, unless it's
//!    a small island of at most `MAX_ISLAND` hexes.
//! 2. Mountain ranges (`raise_ranges`) where plates of crust push together:
//!    short chains one hex wide along the borders between plates, with gaps
//!    between, broken by passes (and one where ranges meet), off the shore;
//!    only about 4% of the land.
//! 3. Hills (`roll_hills`): foothills along the ranges, and rolling uplands.
//!    Hills are a modifier, so whatever ground the climate gives them stays
//!    hilly.
//! 4. Lakes (`fill_lakes`): small bodies of water cut off from the map's edge,
//!    and a few more well inland.
//! 5. Passes (`open_passes`): mountains that wall off a stretch of land open
//!    up, so all of a landmass's open ground is connected.
//! 6. Rivers (`carve_rivers`) run along hex edges, downhill, from a lake or
//!    the mountains and their foothills to the sea, a lake, or another river;
//!    never back into the lake they left. One stuck in a dip ends in a pool,
//!    a new lake of one hex, where one fits.
//! 7. Climate (`set_climate`): temperature falls toward the top and bottom
//!    of the map and by mountains; moisture comes from noise, fresh water and
//!    the sea, less far inland. Together they pick the ground: snow, tundra,
//!    desert, marsh, grassland or plains.
//! 8. Vegetation (`set_climate` too): forest on the wetter grassland, plains
//!    and tundra (hills included), and jungle on most marsh, both in patches.
//! 9. Starts (`pick_starts`): one hex per side on the continent (the largest
//!    stretch of passable land), scattered and then evened out so each is
//!    about as far from its nearest neighbor, in a line and on foot, as the
//!    land allows if shared out evenly, with about as good food and
//!    production nearby as the others.
//! 10. Horses and iron (`place_start_resources`): one of each within a few
//!     hexes of every start, nearer it than any other start, on the ground
//!     that suits it best.
//! 11. Contested ground: ruins, and special tiles that yield more, go where
//!     two starts are about as far on foot, well away from both, so no side
//!     has them to itself.
//! 12. Animal dens (`place_dens`): on forest, jungle and hills, at least
//!     `DEN_START_DISTANCE` from every start, spread apart, in a random
//!     order; the world takes as many as its Animals setting asks for, from
//!     the first (`animals.rs`).
//!
//! A new kind of thing on the map is a stage of its own after the ones it
//! depends on, called from `generate` if it needs the starts.
//! `preview.rs` (tests only) draws whole maps and measures many of them.

use std::collections::VecDeque;

use glam::Vec2;

use super::fast_hash::{HashMap, HashSet};
use super::hex::{Hex, HexGrid, Shape, edge};
use super::terrain::{Feature, Resource, Special, Terrain, Tile};

/// Sides the smallest world is sized for: fewer still get this much room.
const MIN_WORLD_SIDES: usize = 3;
/// Decent city sites 8 hexes apart that a side's share of the land holds on
/// average (`preview.rs` counts them). A city reaches tiles up to 4 hexes
/// off, so cities 8 apart barely share any. Neighbors contest part of a
/// side's share, so this leaves room for 3-5 cities comfortably, and more
/// where the land is good.
const CITY_SITES_PER_SIDE: f32 = 8.0;
/// Hexes of map (land and sea, about half each) for each decent city site
/// 8 apart, as measured over many maps.
const HEXES_PER_CITY_SITE: f32 = 150.0;
/// Hexes of map for each side.
const HEXES_PER_SIDE: f32 = CITY_SITES_PER_SIDE * HEXES_PER_CITY_SITE;
/// The world's columns over its rows (30 by 18 before worlds were sized by
/// city sites): about 1.4 times as wide as tall on screen.
const WORLD_ASPECT: f32 = 30.0 / 18.0;

/// The world's size for `sides` players: `HEXES_PER_SIDE` for each (six get
/// about 111 by 67 hexes).
pub fn world_shape(sides: usize) -> Shape {
    let hexes = sides.max(MIN_WORLD_SIDES) as f32 * HEXES_PER_SIDE;
    // `cols` either side of the middle and `rows` above and below: about
    // 2 cols by 2 rows hexes.
    let rows = (hexes / (4.0 * WORLD_ASPECT)).sqrt();
    Shape::Rectangle {
        cols: (rows * WORLD_ASPECT).round() as i32,
        rows: rows.round() as i32,
    }
}

/// Land apart from the continent survives only as islands this small.
const MAX_ISLAND: usize = 12;

/// Tries at scattering the starts; the most even spread wins.
const START_TRIALS: usize = 24;
/// Swaps tried to even out each scattered set of starts.
const START_SWAPS: usize = 160;

pub struct GeneratedMap {
    pub grid: HexGrid,
    /// One start per side, the player's first: about as far from its nearest
    /// neighbor as every other start is from its own, and about as good.
    /// Each has horses and iron a few hexes away.
    pub starts: Vec<Hex>,
    /// Ruins to fight over, each about as far on foot from the two starts
    /// nearest it and close to none.
    pub ruins: Vec<Hex>,
    /// Sites for animal dens, `MAX_DENS_PER_SIDE` a side (fewer if the land
    /// runs out), in the order to take them: any first few are spread
    /// across the map too.
    pub dens: Vec<Hex>,
}

/// The most animal dens a world has for each side: its Animals setting
/// takes all of them, or fewer (`animals.rs`).
pub const MAX_DENS_PER_SIDE: usize = 2;
/// How near a start a den may be, in hexes.
pub const DEN_START_DISTANCE: i32 = 5;
/// How near each other dens may be, in hexes.
const DEN_GAP: i32 = 6;

/// Builds a world map for `sides` players from `seed`.
pub fn generate(seed: u32, sides: usize) -> GeneratedMap {
    generate_with_rivers(seed, sides).0
}

/// `generate`, keeping each river's course from source to mouth too.
fn generate_with_rivers(seed: u32, sides: usize) -> (GeneratedMap, Vec<River>) {
    let sides = sides.max(1);
    let shape = world_shape(sides);
    // A rare map has no good set of starts; try the next variant.
    for attempt in 0..16u64 {
        let mut rng = Rng(u64::from(seed) << 8 | attempt);
        let (tiles, rivers) = shape_world(shape, &mut rng);
        let mut grid = HexGrid::shaped(shape, tiles).with_rivers(river_edges(&rivers));
        let Some((starts, spacing)) = pick_starts(&grid, sides, &mut rng) else {
            continue;
        };
        place_start_resources(&mut grid, &starts, &mut rng);
        let distances: Vec<HashMap<Hex, i32>> = starts
            .iter()
            .map(|&start| walking_distances(&grid, start))
            .collect();
        let ruins = place_ruins(&grid, &starts, &distances, spacing, &mut rng);
        place_specials(&mut grid, &starts, &distances, &ruins, spacing, &mut rng);
        let dens = place_dens(&grid, &starts, &distances, &ruins, &mut rng);
        let map = GeneratedMap {
            grid,
            starts,
            ruins,
            dens,
        };
        return (map, rivers);
    }
    let map = GeneratedMap {
        grid: HexGrid::shaped(shape, [(Hex::new(0, 0), Terrain::Plains)]),
        starts: (0..sides as i32).map(|i| Hex::new(4 * i - 8, 0)).collect(),
        ruins: Vec::new(),
        dens: Vec::new(),
    };
    (map, Vec::new())
}

type Tiles = HashMap<Hex, Tile>;

/// A river: the corners it runs through, from its source to its mouth.
type River = Vec<Corner>;

/// The land and sea, before starts and resources: every hex's tile, and the
/// rivers.
fn shape_world(shape: Shape, rng: &mut Rng) -> (Tiles, Vec<River>) {
    let mut draft = Draft::new(shape);
    raise_continent(&mut draft, rng);
    let inland = sea_distance(&draft);
    raise_ranges(&mut draft, &inland, rng);
    roll_hills(&mut draft, &inland, rng);
    fill_lakes(&mut draft, &inland, rng);
    open_passes(&mut draft);
    let rivers = carve_rivers(&mut draft, &inland, rng);
    mark_coasts(&mut draft);
    set_climate(&mut draft, &inland, &river_edges(&rivers), rng);
    (draft.tiles, rivers)
}

/// A map in the making: its shape, and every hex's tile so far.
struct Draft {
    shape: Shape,
    blank: HexGrid,
    hexes: Vec<Hex>,
    /// How far the farthest hex center is from the middle, along x and y.
    extent: Vec2,
    tiles: Tiles,
}

impl Draft {
    /// All sea.
    fn new(shape: Shape) -> Self {
        let blank = HexGrid::shaped::<Tile>(shape, []);
        let hexes: Vec<Hex> = blank.all_hexes().collect();
        let extent = hexes
            .iter()
            .fold(Vec2::ZERO, |m, h| m.max(h.to_world().abs()));
        let tiles = hexes
            .iter()
            .map(|&h| (h, Tile::from(Terrain::Ocean)))
            .collect();
        Self {
            shape,
            blank,
            hexes,
            extent,
            tiles,
        }
    }

    fn on_map(&self, hex: Hex) -> bool {
        self.blank.contains(hex)
    }

    fn on_rim(&self, hex: Hex) -> bool {
        self.blank.edge_distance(hex) == 0
    }

    /// The ground of a hex on the map.
    fn terrain(&self, hex: Hex) -> Terrain {
        self.tiles[&hex].terrain
    }

    fn is_land(&self, hex: Hex) -> bool {
        self.on_map(hex) && !self.terrain(hex).is_water()
    }

    fn is_mountain(&self, hex: Hex) -> bool {
        self.on_map(hex) && self.terrain(hex) == Terrain::Mountains
    }

    fn is_sea(&self, hex: Hex) -> bool {
        self.on_map(hex) && matches!(self.terrain(hex), Terrain::Ocean | Terrain::Coast)
    }

    fn is_lake(&self, hex: Hex) -> bool {
        self.on_map(hex) && self.terrain(hex) == Terrain::Lake
    }

    /// Land a unit can walk on.
    fn is_open(&self, hex: Hex) -> bool {
        self.is_land(hex) && !self.is_mountain(hex)
    }

    fn land(&self) -> Vec<Hex> {
        self.hexes
            .iter()
            .copied()
            .filter(|&h| self.is_land(h))
            .collect()
    }

    fn set(&mut self, hex: Hex, tile: Tile) {
        self.tiles.insert(hex, tile);
    }
}

/// Stage 1, land and sea: an oval dome over the middle of the map, bent out
/// of shape by warping noise and carved by two layers of fractal noise into
/// bays, peninsulas and inland seas. The lowest 42-52% (varying per map) is
/// sea. The biggest landmass stays; other land sinks unless it's a small
/// island.
fn raise_continent(draft: &mut Draft, rng: &mut Rng) {
    let (coast, detail, warp_x, warp_y) = (
        Noise(rng.next()),
        Noise(rng.next()),
        Noise(rng.next()),
        Noise(rng.next()),
    );
    let extent = draft.extent;
    let elevation: Vec<f32> = draft
        .hexes
        .iter()
        .map(|h| {
            let p = h.to_world();
            let warp = Vec2::new(warp_x.fbm(p / 14.0), warp_y.fbm(p / 14.0)) - Vec2::splat(0.5);
            let dome = 1.0 - ((p + 12.0 * warp) / extent).length();
            dome + 0.7 * (coast.fbm(p / 9.0) - 0.5) + 0.25 * (detail.fbm(p / 3.5) - 0.5)
        })
        .collect();
    let sea_level = quantile(elevation.iter().copied(), 0.42 + 0.1 * rng.unit());
    for (i, &height) in elevation.iter().enumerate() {
        if height >= sea_level {
            draft.set(draft.hexes[i], Tile::from(Terrain::Plains));
        }
    }
    let mut masses = components(&draft.hexes, |h| draft.is_land(h));
    masses.sort_by_key(|m| std::cmp::Reverse(m.len()));
    for mass in masses.iter().skip(1).filter(|m| m.len() > MAX_ISLAND) {
        for &h in mass {
            draft.set(h, Tile::from(Terrain::Ocean));
        }
    }
}

/// Steps from each hex to the sea or the map's edge: 0 on the water, 1 on
/// the shore, more inland.
fn sea_distance(draft: &Draft) -> HashMap<Hex, i32> {
    let mut steps: HashMap<Hex, i32> = HashMap::default();
    let mut queue = VecDeque::new();
    for &h in &draft.hexes {
        if !draft.is_land(h) {
            steps.insert(h, 0);
        } else if draft.on_rim(h) || h.neighbors().iter().any(|n| !draft.is_land(*n)) {
            steps.insert(h, 1);
            queue.push_back(h);
        }
    }
    while let Some(h) = queue.pop_front() {
        let next = steps[&h] + 1;
        for n in h.neighbors() {
            if draft.on_map(n) && !steps.contains_key(&n) {
                steps.insert(n, next);
                queue.push_back(n);
            }
        }
    }
    steps
}

/// Hexes of map per plate of crust (`raise_ranges`).
const HEXES_PER_PLATE: usize = 150;
/// The chance that two plates meeting push together and raise a range.
const RANGE_CHANCE: f32 = 0.65;
/// The chance that a hex along a range stays open, as a pass: about one in
/// five, so ranges rarely run far without a way through.
const PASS_CHANCE: f32 = 0.2;
/// How unevenly a ridge wears down along its length: noise this strong,
/// against the 0 to 1 of how hard its plates push, breaks every border into
/// short ranges where it runs high, and gaps between.
const WEAR: f32 = 1.5;
/// The size of the wear's bumps, in world units: a few hexes.
const WEAR_SCALE: f32 = 2.0;

/// Stage 2, mountain ranges, where plates of crust push together. The map is
/// split among plates (a dozen or more), each the hexes nearest its center,
/// warped so the borders between them curve. About two in three borders
/// rise, each by its own amount, and the hexes along a border (one hex wide,
/// on one side of it) are the ridge: the rising borders first, the others
/// only where those fall short. Only the highest 4.8-6.8% of the land
/// (varying per map) is ridge, and a ridge wears down unevenly (`WEAR`), so
/// ranges come in short, thin chains with gaps between. About one hex in
/// five stays open as a pass, as does the hex where three ranges meet, so
/// about 4% of the land ends up mountains; ranges keep a hex back from the
/// shore. A lone peak or two may rise elsewhere.
fn raise_ranges(draft: &mut Draft, inland: &HashMap<Hex, i32>, rng: &mut Rng) {
    let (warp_x, warp_y, wear) = (Noise(rng.next()), Noise(rng.next()), Noise(rng.next()));
    let extent = draft.extent;
    let plates = (draft.hexes.len() / HEXES_PER_PLATE).max(6);
    let centers: Vec<Vec2> = (0..plates)
        .map(|_| {
            let (x, y) = (rng.unit(), rng.unit());
            extent * Vec2::new(2.0 * x - 1.0, 2.0 * y - 1.0)
        })
        .collect();
    // How hard each pair of plates pushes together: 0 to 1 where they rise,
    // below 0 where they don't, so those borders only take up the ridge if
    // the rising ones are too short for the share.
    let mut push = vec![-1.0f32; plates * plates];
    for a in 0..plates {
        for b in a + 1..plates {
            let (rises, height) = (rng.unit() < RANGE_CHANCE, rng.unit());
            let height = if rises { height } else { height - 1.0 };
            push[a * plates + b] = height;
            push[b * plates + a] = height;
        }
    }
    let plate: HashMap<Hex, usize> = draft
        .hexes
        .iter()
        .map(|&h| {
            let p = h.to_world();
            let warp = Vec2::new(warp_x.fbm(p / 10.0), warp_y.fbm(p / 10.0)) - Vec2::splat(0.5);
            let q = p + 8.0 * warp;
            let nearest = (0..plates)
                .min_by(|&a, &b| {
                    centers[a]
                        .distance_squared(q)
                        .total_cmp(&centers[b].distance_squared(q))
                })
                .expect("at least one plate");
            (h, nearest)
        })
        .collect();

    let land = draft.land();
    // The ridge: hexes on the lower-numbered plate's side of a border,
    // highest first.
    let mut ridge: Vec<(Hex, f32)> = land
        .iter()
        .filter(|&&h| inland[&h] >= 2)
        .filter_map(|&h| {
            let own = plate[&h];
            let height = h
                .neighbors()
                .into_iter()
                .filter(|&n| draft.on_map(n) && plate[&n] > own)
                .map(|n| push[own * plates + plate[&n]])
                .fold(f32::NEG_INFINITY, f32::max);
            height
                .is_finite()
                .then(|| (h, height + WEAR * wear.fbm(h.to_world() / WEAR_SCALE)))
        })
        .collect();
    ridge.sort_by(|a, b| {
        b.1.total_cmp(&a.1)
            .then((a.0.q, a.0.r).cmp(&(b.0.q, b.0.r)))
    });
    let share = 0.048 + 0.02 * rng.unit();
    let wanted = (land.len() as f32 * share) as usize;
    for &(h, _) in ridge.iter().take(wanted) {
        let pass = rng.unit() < PASS_CHANCE;
        draft.set(h, if pass { Tile::HILLS } else { Tile::MOUNTAINS });
    }
    // Where three ranges meet, the hex they meet on opens as a pass, so
    // ranges don't knot together into a lump.
    let knots: Vec<Hex> = land
        .iter()
        .copied()
        .filter(|&h| draft.is_mountain(h) && range_arms(draft, h) >= 3)
        .collect();
    for h in knots {
        draft.set(h, Tile::HILLS);
    }
    // What the passes and the lowest ridges leave of a range as a lone hex
    // wears down to a hill.
    for stump in components(&draft.hexes, |h| draft.is_mountain(h)) {
        if let [h] = stump[..] {
            draft.set(h, Tile::HILLS);
        }
    }

    // A lone peak or two, well inland and away from the ranges.
    for _ in 0..rng.below(3) {
        let h = land[rng.below(land.len())];
        if inland[&h] >= 3 && !within(h, 3).any(|n| draft.is_mountain(n)) {
            draft.set(h, Tile::MOUNTAINS);
        }
    }
}

/// How many separate runs of mountains `hex`'s neighbors form, going round
/// it: 2 in the middle of a range, 3 or more where ranges meet.
fn range_arms(draft: &Draft, hex: Hex) -> usize {
    let around = hex
        .neighbors()
        .map(|n| draft.on_map(n) && draft.is_mountain(n));
    (0..6)
        .filter(|&i| around[i] && !around[(i + 5) % 6])
        .count()
}

/// The chance that open land beside a mountain is a foothill.
const FOOTHILL_CHANCE: f32 = 0.5;

/// Stage 3, hills, 12-17% of the land (varying per map): foothills along
/// the ranges, and the rest in rolling uplands from their own noise, more
/// often well inland. Hills are a modifier, so whatever ground the climate gives
/// them stays hilly.
fn roll_hills(draft: &mut Draft, inland: &HashMap<Hex, i32>, rng: &mut Rng) {
    let rolling = Noise(rng.next());
    let land = draft.land();
    let open: Vec<Hex> = land
        .iter()
        .copied()
        .filter(|&h| !draft.is_mountain(h))
        .collect();
    for &h in &open {
        let by_range = h.neighbors().iter().any(|n| draft.is_mountain(*n));
        if by_range && rng.unit() < FOOTHILL_CHANCE {
            draft.set(h, Tile::HILLS);
        }
    }
    let share = 0.12 + 0.05 * rng.unit();
    let wanted = (land.len() as f32 * share) as usize;
    let flat: Vec<Hex> = open
        .iter()
        .copied()
        .filter(|h| !draft.tiles[h].hills)
        .collect();
    let more = wanted.saturating_sub(open.len() - flat.len());
    let cut = 1.0 - more as f32 / flat.len().max(1) as f32;
    let upland = ranks(&flat, |h| {
        0.75 * rolling.fbm(h.to_world() / 4.0) + 0.25 * inland[&h].min(8) as f32 / 8.0
    });
    for h in flat {
        if upland[&h] > cut {
            draft.set(h, Tile::HILLS);
        }
    }
}

/// Stage 4, lakes: a small body of water cut off from the map's edge is a
/// lake, and a few more lakes of one to three hexes lie well inland, apart.
fn fill_lakes(draft: &mut Draft, inland: &HashMap<Hex, i32>, rng: &mut Rng) {
    let lake = Tile::from(Terrain::Lake);
    for body in components(&draft.hexes, |h| !draft.is_land(h)) {
        if body.len() <= 10 && !body.iter().any(|&h| draft.on_rim(h)) {
            for h in body {
                draft.set(h, lake);
            }
        }
    }
    let land = draft.land();
    let fits = |draft: &Draft, h: Hex, inland_by: i32| draft.is_open(h) && inland[&h] >= inland_by;
    let mut sites: Vec<Hex> = land
        .iter()
        .copied()
        .filter(|&h| fits(draft, h, 3))
        .collect();
    rng.shuffle(&mut sites);
    let wanted = 1 + land.len() / 400 + rng.below(3);
    let mut placed = 0;
    for site in sites {
        if placed == wanted {
            break;
        }
        if !fits(draft, site, 3) || within(site, 5).any(|h| draft.is_lake(h)) {
            continue;
        }
        let size = 1 + rng.below(3);
        let mut pool = vec![site];
        draft.set(site, lake);
        while pool.len() < size {
            let shore: Vec<Hex> = pool
                .iter()
                .flat_map(|h| h.neighbors())
                .filter(|&h| fits(draft, h, 2))
                .collect();
            if shore.is_empty() {
                break;
            }
            let next = shore[rng.below(shore.len())];
            draft.set(next, lake);
            pool.push(next);
        }
        placed += 1;
    }
}

/// Stage 5, passes: mountains never wall land off. While a stretch of open
/// land is cut off from the rest of its landmass, the mountains on the
/// shortest way out of the smallest such stretch sink to hills.
fn open_passes(draft: &mut Draft) {
    loop {
        let open = components(&draft.hexes, |h| draft.is_open(h));
        if open.len() < 2 {
            return;
        }
        let mut mass_of: HashMap<Hex, usize> = HashMap::default();
        for (i, mass) in components(&draft.hexes, |h| draft.is_land(h))
            .into_iter()
            .enumerate()
        {
            mass_of.extend(mass.into_iter().map(|h| (h, i)));
        }
        let mut stretches = vec![0; mass_of.values().max().map_or(0, |m| m + 1)];
        for stretch in &open {
            stretches[mass_of[&stretch[0]]] += 1;
        }
        let Some(pocket) = open
            .iter()
            .filter(|o| stretches[mass_of[&o[0]]] > 1)
            .min_by_key(|o| o.len())
        else {
            return;
        };
        let inside: HashSet<Hex> = pocket.iter().copied().collect();
        let mut came_from: HashMap<Hex, Hex> = HashMap::default();
        let mut queue: VecDeque<Hex> = pocket.iter().copied().collect();
        let mut exit = None;
        'search: while let Some(h) = queue.pop_front() {
            for n in h.neighbors() {
                if !draft.is_land(n) || inside.contains(&n) || came_from.contains_key(&n) {
                    continue;
                }
                came_from.insert(n, h);
                if draft.is_mountain(n) {
                    queue.push_back(n);
                } else {
                    exit = Some(n);
                    break 'search;
                }
            }
        }
        let Some(mut h) = exit else {
            return;
        };
        while let Some(&back) = came_from.get(&h) {
            if inside.contains(&back) {
                break;
            }
            draft.set(back, Tile::HILLS);
            h = back;
        }
    }
}

/// A corner where three hexes meet, as those hexes in sorted order.
type Corner = [Hex; 3];

fn corner(mut hexes: [Hex; 3]) -> Corner {
    hexes.sort_by_key(|h| (h.q, h.r));
    hexes
}

/// The three corners one edge away from `c`.
fn next_corners(c: Corner) -> [Corner; 3] {
    [(0, 1, 2), (1, 2, 0), (0, 2, 1)].map(|(i, j, k)| {
        let (a, b, away) = (c[i], c[j], c[k]);
        // Two adjacent hexes share two neighbors: `away`, and the one across
        // the edge from it.
        let across = a
            .neighbors()
            .into_iter()
            .find(|n| *n != away && n.distance(b) == 1)
            .expect("adjacent hexes share two neighbors");
        corner([a, b, across])
    })
}

/// The edge between two corners one edge apart: the two hexes they share.
fn shared_edge(a: Corner, b: Corner) -> (Hex, Hex) {
    let mut shared = a.into_iter().filter(|h| b.contains(h));
    let (x, y) = (shared.next(), shared.next());
    edge(
        x.expect("neighboring corners share two hexes"),
        y.expect("neighboring corners share two hexes"),
    )
}

/// Every edge the rivers run along.
fn river_edges(rivers: &[River]) -> HashSet<(Hex, Hex)> {
    rivers
        .iter()
        .flat_map(|river| river.windows(2).map(|w| shared_edge(w[0], w[1])))
        .collect()
}

/// Where a river ends.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mouth {
    Sea,
    /// A lake, by its index among the lakes.
    Lake(usize),
    /// Another river, by its index, which it joins.
    River(usize),
}

/// Whether water leaving through `mouth` ever reaches `lake`: straight in,
/// down a river it joins, or out through another lake.
fn drains_into(
    mut mouth: Mouth,
    lake: usize,
    mouths: &[Mouth],
    outflow: &HashMap<usize, usize>,
) -> bool {
    loop {
        mouth = match mouth {
            Mouth::Sea => return false,
            Mouth::Lake(l) if l == lake => return true,
            Mouth::Lake(l) => match outflow.get(&l) {
                Some(&river) => mouths[river],
                None => return false,
            },
            Mouth::River(river) => mouths[river],
        };
    }
}

/// Hexes of land per river (`carve_rivers`).
const LAND_PER_RIVER: usize = 45;

/// The most edges a river on a map `extent` wide (see `Draft`) runs along:
/// about half the map's width in hexes.
fn longest_river(extent: Vec2) -> usize {
    (extent.x * 0.45) as usize
}

/// Stage 6, rivers: they run along hex edges, from corner to corner, always
/// down to the lowest next corner. The ground's height is its steps from the
/// sea, higher under hills and mountains and roughened by noise, so a river
/// heads for the nearest shore, winding a little. It rises at a lake (at most one river
/// flows out of each) or in the mountains and their foothills, and flows
/// until it reaches the sea, a lake, or another river, which it joins; so
/// rivers merge but never split. A river never runs back into the lake it
/// left, directly or down other rivers and lakes. One that gets stuck in a
/// dip fills it as a pool, a new lake of one hex (`pool_site`), where one
/// fits; one that can't, or would run on longer than suits the map, or stay
/// under three edges long, isn't made. About one river per `LAND_PER_RIVER`
/// hexes of land, their sources at least three hexes apart.
fn carve_rivers(draft: &mut Draft, inland: &HashMap<Hex, i32>, rng: &mut Rng) -> Vec<River> {
    let lift = Noise(rng.next());
    let height: HashMap<Hex, f32> = draft
        .hexes
        .iter()
        .map(|&h| {
            let tile = draft.tiles[&h];
            let steps = inland[&h] as f32;
            let rough = 0.9 * (lift.fbm(h.to_world() / 4.0) - 0.5);
            let height = match tile.terrain {
                Terrain::Ocean | Terrain::Coast => -1.0,
                Terrain::Lake => steps - 0.5,
                Terrain::Mountains => steps + 2.0 + rough,
                _ if tile.hills => steps + 0.6 + rough,
                _ => steps + rough,
            };
            (h, height)
        })
        .collect();
    let on_map = |c: &Corner| c.iter().all(|&h| draft.on_map(h));
    let corner_height = |c: &Corner| c.iter().map(|h| height[h]).sum::<f32>() / 3.0;
    let mut lake_of: HashMap<Hex, usize> = HashMap::default();
    let mut lakes = 0;
    for lake in components(&draft.hexes, |h| draft.is_lake(h)) {
        lake_of.extend(lake.into_iter().map(|h| (h, lakes)));
        lakes += 1;
    }

    // Sources: corners on a lake's shore, between two land hexes, and
    // corners of mountains and foothills with no water and some open land.
    let mut sources: Vec<(Corner, Option<usize>)> = Vec::new();
    let mut seen: HashSet<Corner> = HashSet::default();
    for &h in &draft.hexes {
        let tile = draft.tiles[&h];
        let lake = lake_of.get(&h).copied();
        let highland = tile.terrain == Terrain::Mountains
            || (tile.hills && h.neighbors().iter().any(|n| draft.is_mountain(*n)));
        if lake.is_none() && !highland {
            continue;
        }
        let around = h.neighbors();
        for i in 0..6 {
            let c = corner([h, around[i], around[(i + 1) % 6]]);
            if !on_map(&c) {
                continue;
            }
            let fits = match lake {
                Some(_) => c.iter().filter(|&&x| x != h).all(|&x| draft.is_land(x)),
                None => c.iter().all(|&x| draft.is_land(x)) && c.iter().any(|&x| draft.is_open(x)),
            };
            if fits && seen.insert(c) {
                sources.push((c, lake));
            }
        }
    }
    rng.shuffle(&mut sources);

    let land = draft.hexes.iter().filter(|&&h| draft.is_land(h)).count();
    let wanted = land / LAND_PER_RIVER + rng.below(3);
    let longest = longest_river(draft.extent);
    let mut rivers: Vec<River> = Vec::new();
    let mut mouths: Vec<Mouth> = Vec::new();
    let mut on_river: HashMap<Corner, usize> = HashMap::default();
    // Every hex a river's corners touch.
    let mut by_river: HashSet<Hex> = HashSet::default();
    let mut outflow: HashMap<usize, usize> = HashMap::default();
    let mut pools: Vec<Hex> = Vec::new();
    for (source, lake) in sources {
        if rivers.len() >= wanted {
            break;
        }
        if lake.is_some_and(|l| outflow.contains_key(&l))
            || on_river.contains_key(&source)
            || rivers.iter().any(|r| r[0][0].distance(source[0]) < 3)
            // By a pool a river left earlier, which isn't its lake.
            || source
                .iter()
                .any(|h| lake_of.get(h).is_some_and(|&l| Some(l) != lake))
        {
            continue;
        }
        let touches_lake =
            |c: &Corner, lake: usize| c.iter().any(|h| lake_of.get(h) == Some(&lake));
        let mut path = vec![source];
        let mouth = loop {
            let here = *path.last().expect("a path starts at its source");
            if path.len() > 1 {
                if here.iter().any(|&h| draft.is_sea(h)) {
                    break Some(Mouth::Sea);
                }
                if let Some(&l) = here.iter().find_map(|h| lake_of.get(h)) {
                    break Some(Mouth::Lake(l));
                }
                if let Some(&river) = on_river.get(&here) {
                    break Some(Mouth::River(river));
                }
            }
            if path.len() > longest {
                break None;
            }
            let level = corner_height(&here);
            let next = next_corners(here)
                .into_iter()
                .filter(|c| on_map(c) && lake.is_none_or(|l| !touches_lake(c, l)))
                .map(|c| (c, corner_height(&c)))
                .filter(|&(_, h)| h < level)
                .min_by(|a, b| a.1.total_cmp(&b.1));
            match next {
                Some((c, _)) => path.push(c),
                None => break None,
            }
        };
        // Stuck in a dip: the river fills it, as a pool.
        let pool = if mouth.is_none() && path.len() <= longest {
            pool_site(draft, &path, &height, &lake_of, &by_river).map(|(site, keep)| {
                path.truncate(keep);
                site
            })
        } else {
            None
        };
        let Some(mouth) = mouth.or(pool.map(|_| Mouth::Lake(lakes))) else {
            continue;
        };
        if path.len() < 4 || lake.is_some_and(|l| drains_into(mouth, l, &mouths, &outflow)) {
            continue;
        }
        if let Some(h) = pool {
            lake_of.insert(h, lakes);
            lakes += 1;
            pools.push(h);
        }
        let id = rivers.len();
        for &c in &path {
            on_river.entry(c).or_insert(id);
            by_river.extend(c);
        }
        if let Some(l) = lake {
            outflow.insert(l, id);
        }
        rivers.push(path);
        mouths.push(mouth);
    }
    for h in pools {
        draft.set(h, Tile::from(Terrain::Lake));
    }
    rivers
}

/// Where a river stuck in a dip at the end of `path` can end in a new pool
/// of one hex, and how many of its corners it keeps, up to the first that
/// touches the pool: one of the hexes at its last corner, the lowest that
/// fits. A pool is open land ringed by open land (so it touches no other
/// water and cuts nothing off) that no river runs by yet, and the river
/// stays at least three edges long.
fn pool_site(
    draft: &Draft,
    path: &[Corner],
    height: &HashMap<Hex, f32>,
    lake_of: &HashMap<Hex, usize>,
    by_river: &HashSet<Hex>,
) -> Option<(Hex, usize)> {
    let open = |h: Hex| draft.is_open(h) && !lake_of.contains_key(&h);
    let mut sites = *path.last()?;
    sites.sort_by(|a, b| height[a].total_cmp(&height[b]));
    sites.into_iter().find_map(|site| {
        let keep = path.iter().position(|c| c.contains(&site))? + 1;
        let fits = keep >= 4
            && open(site)
            && site.neighbors().into_iter().all(open)
            && !by_river.contains(&site);
        fits.then_some((site, keep))
    })
}

/// What's left of the sea is coast next to land, open ocean beyond.
fn mark_coasts(draft: &mut Draft) {
    for i in 0..draft.hexes.len() {
        let h = draft.hexes[i];
        if draft.terrain(h) == Terrain::Ocean && h.neighbors().iter().any(|n| draft.is_land(*n)) {
            draft.set(h, Tile::from(Terrain::Coast));
        }
    }
}

/// Stage 7, climate, for all land but mountains and lakes, hills included:
/// temperature falls toward the top and bottom of the map, and by
/// mountains, with noise on top; moisture is noise, plus a boost by rivers,
/// lakes and the sea, and less far inland. Together they pick the ground:
/// snow, tundra, desert, marsh, grassland or plains.
///
/// Stage 8, vegetation: forest grows on the wetter grassland, plains and
/// tundra (hills included), in patches from its own noise; jungle covers
/// most marsh, in patches too.
fn set_climate(
    draft: &mut Draft,
    inland: &HashMap<Hex, i32>,
    rivers: &HashSet<(Hex, Hex)>,
    rng: &mut Rng,
) {
    let (heat_noise, wet_noise, growth_noise) =
        (Noise(rng.next()), Noise(rng.next()), Noise(rng.next()));
    let extent = draft.extent;
    let open: Vec<Hex> = draft
        .hexes
        .iter()
        .copied()
        .filter(|&h| draft.terrain(h) == Terrain::Plains)
        .collect();
    let heat = ranks(&open, |h| {
        let latitude = 1.0 - h.to_world().y.abs() / extent.y;
        let chill = if h.neighbors().iter().any(|n| draft.is_mountain(*n)) {
            0.06
        } else if draft.tiles[&h].hills {
            0.02
        } else {
            0.0
        };
        0.6 * latitude + 0.4 * heat_noise.fbm(h.to_world() / 5.0) - chill
    });
    let grid = HexGrid::shaped(draft.shape, draft.tiles.clone()).with_rivers(rivers.clone());
    let wet = ranks(&open, |h| {
        let fresh = if grid.has_fresh_water(h) { 0.2 } else { 0.0 };
        let sea = if h.neighbors().iter().any(|n| draft.is_sea(*n)) {
            0.1
        } else {
            0.0
        };
        let dry = 0.006 * inland[&h].min(10) as f32;
        wet_noise.fbm(h.to_world() / 5.0) + fresh + sea - dry
    });
    let growth = ranks(&open, |h| growth_noise.fbm(h.to_world() / 4.0));
    for &h in &open {
        let (t, w, g) = (heat[&h], wet[&h], growth[&h]);
        let tile = draft.tiles.get_mut(&h).expect("open hexes are on the map");
        tile.terrain = if t < 0.05 {
            Terrain::Snow
        } else if t < 0.17 {
            Terrain::Tundra
        } else if t > 0.6 && w < 0.28 {
            Terrain::Desert
        } else if t > 0.5 && w > 0.88 && !tile.hills {
            Terrain::Marsh
        } else if w > 0.56 {
            Terrain::Grassland
        } else {
            Terrain::Plains
        };
        tile.feature = match tile.terrain {
            Terrain::Marsh if g > 0.2 => Some(Feature::Jungle),
            Terrain::Grassland | Terrain::Plains | Terrain::Tundra
                if 0.55 * w + 0.45 * g > 0.66 =>
            {
                Some(Feature::Forest)
            }
            _ => None,
        };
    }
}

/// One start per side on the continent (the largest stretch of passable
/// land), with the spacing they aim for: the distance between neighbors if
/// the land were shared out evenly. Each start is a `fair_start` on good
/// ground. The set is scattered at random and then evened out
/// (`start_cost`): every start about `spacing` from its nearest neighbor, some
/// closer and some farther, and about as good as the others. Of the sets
/// tried, the best whose nearest neighbors are also about as far on foot
/// (`walking_spread`, around the ranges) wins. The player's start, first, is
/// any of them.
fn pick_starts(grid: &HexGrid, sides: usize, rng: &mut Rng) -> Option<(Vec<Hex>, f32)> {
    let all: Vec<Hex> = grid.all_hexes().collect();
    let landmass = components(&all, |h| grid.is_passable(h))
        .into_iter()
        .max_by_key(Vec::len)?;
    let candidates: Vec<(Hex, i32)> = landmass
        .iter()
        .copied()
        .filter(|h| {
            grid.edge_distance(*h) >= 2
                && !matches!(
                    grid.terrain(*h),
                    Terrain::Snow | Terrain::Desert | Terrain::Marsh
                )
                && fair_start(grid, *h)
        })
        .map(|h| (h, start_score(grid, h)))
        .collect();
    if candidates.len() < sides {
        return None;
    }
    // Hexes pack at about 0.87 d^2 of area per point d apart.
    let spacing = 1.075 * (landmass.len() as f32 / sides as f32).sqrt();
    for tolerance in [0.75, 0.6, 0.45] {
        let min_gap = ((spacing * tolerance) as i32).max(4);
        let mut tried: Vec<(f32, Vec<usize>)> = Vec::new();
        for _ in 0..START_TRIALS {
            let Some(mut set) = scatter_starts(&candidates, sides, min_gap, rng) else {
                continue;
            };
            let mut cost = start_cost(&candidates, &set, spacing);
            for _ in 0..START_SWAPS {
                let slot = rng.below(set.len());
                let pick = rng.below(candidates.len());
                if set.contains(&pick)
                    || set.iter().enumerate().any(|(i, &other)| {
                        i != slot && candidates[pick].0.distance(candidates[other].0) < min_gap
                    })
                {
                    continue;
                }
                let old = std::mem::replace(&mut set[slot], pick);
                let new_cost = start_cost(&candidates, &set, spacing);
                if new_cost < cost {
                    cost = new_cost;
                } else {
                    set[slot] = old;
                }
            }
            tried.push((cost, set));
        }
        tried.sort_by(|a, b| a.0.total_cmp(&b.0));
        let hexes = |set: &[usize]| -> Vec<Hex> { set.iter().map(|&i| candidates[i].0).collect() };
        let even = tried
            .iter()
            .find(|(_, set)| walking_spread(grid, &hexes(set)) <= START_WALK_SPREAD);
        if let Some((_, set)) = even.or(tried.first()) {
            let mut starts = hexes(set);
            starts.sort_by_key(|h| (h.q, h.r));
            let player = rng.below(starts.len());
            starts.swap(0, player);
            return Some((starts, spacing));
        }
    }
    None
}

/// How many times farther on foot one start's nearest neighbor may be than
/// another's, for a set of starts to count as even (`pick_starts`).
const START_WALK_SPREAD: f32 = 1.6;

/// The walking distance from the start whose nearest neighbor is farthest on
/// foot to that neighbor, over the same for the start whose nearest is
/// closest. Unreachable counts as very far.
fn walking_spread(grid: &HexGrid, starts: &[Hex]) -> f32 {
    let distances: Vec<HashMap<Hex, i32>> =
        starts.iter().map(|&s| walking_distances(grid, s)).collect();
    let nearest = (0..starts.len()).map(|i| {
        (0..starts.len())
            .filter(|&j| j != i)
            .map(|j| {
                distances[i]
                    .get(&starts[j])
                    .copied()
                    .unwrap_or(i32::MAX / 2)
            })
            .min()
            .unwrap_or(0)
    });
    let (lo, hi) = nearest.fold((i32::MAX, 0), |(lo, hi), d| (lo.min(d), hi.max(d)));
    hi as f32 / lo.max(1) as f32
}

/// `sides` candidates taken in random order, each at least `min_gap` from
/// those already taken, or `None` if they run out first.
fn scatter_starts(
    candidates: &[(Hex, i32)],
    sides: usize,
    min_gap: i32,
    rng: &mut Rng,
) -> Option<Vec<usize>> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    rng.shuffle(&mut order);
    let mut set: Vec<usize> = Vec::with_capacity(sides);
    for i in order {
        if set
            .iter()
            .all(|&j| candidates[i].0.distance(candidates[j].0) >= min_gap)
        {
            set.push(i);
            if set.len() == sides {
                return Some(set);
            }
        }
    }
    None
}

/// How far a set of starts is from the ideal (lower is better): each start's
/// distance to its nearest neighbor should be `spacing`, their scores equal,
/// and higher.
fn start_cost(candidates: &[(Hex, i32)], set: &[usize], spacing: f32) -> f32 {
    let hexes: Vec<Hex> = set.iter().map(|&i| candidates[i].0).collect();
    let scores: Vec<f32> = set.iter().map(|&i| candidates[i].1 as f32).collect();
    let spread = if hexes.len() < 2 {
        0.0
    } else {
        hexes
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let nearest = hexes
                    .iter()
                    .enumerate()
                    .filter(|&(j, _)| j != i)
                    .map(|(_, b)| a.distance(*b))
                    .min()
                    .unwrap_or(0) as f32;
                let off = (nearest - spacing) / spacing;
                off * off
            })
            .sum::<f32>()
            / hexes.len() as f32
    };
    let mean = (scores.iter().sum::<f32>() / scores.len() as f32).max(1.0);
    let (low, high) = scores
        .iter()
        .fold((f32::MAX, f32::MIN), |(lo, hi), &s| (lo.min(s), hi.max(s)));
    spread + 0.5 * (high - low) / mean - 0.1 * mean / 100.0
}

/// Horses and iron for every start, each within a few hexes of it and
/// nearer it than any other start, on the best ground for it there is
/// nearby: horses on open grassland or plains, else open tundra; iron in
/// foothills, else on other hills or under mountains; anywhere open if
/// nothing suits.
fn place_start_resources(grid: &mut HexGrid, starts: &[Hex], rng: &mut Rng) {
    for (i, &start) in starts.iter().enumerate() {
        let taken: Vec<Hex> = std::iter::once(start)
            .chain(
                start_units(grid, start)
                    .map(|(worker, scout)| [worker, scout])
                    .into_iter()
                    .flatten(),
            )
            .collect();
        let own = |h: Hex| {
            starts
                .iter()
                .enumerate()
                .all(|(j, &other)| j == i || h.distance(other) > h.distance(start))
        };
        let kinds: [(Resource, [Ground; 2]); 2] = [
            (Resource::Horses, [pasture, horse_ground]),
            (Resource::Iron, [foothill, iron_ground]),
        ];
        for (resource, grounds) in kinds {
            let options = |reach: i32, fits: Option<Ground>| -> Vec<Hex> {
                within(start, reach)
                    .filter(|&h| {
                        h.distance(start) >= 2
                            && grid.is_passable(h)
                            && !taken.contains(&h)
                            && grid.resource(h).is_none()
                            && own(h)
                            && fits.is_none_or(|fits| fits(grid, h))
                    })
                    .collect()
            };
            let spot = grounds
                .into_iter()
                .flat_map(|ground| [(3, Some(ground)), (4, Some(ground))])
                .chain([(3, None), (5, None)])
                .map(|(reach, fits)| options(reach, fits))
                .find(|options| !options.is_empty())
                .map(|options| options[rng.below(options.len())]);
            if let Some(spot) = spot {
                grid.set_resource(spot, resource);
            }
        }
    }
}

/// Whether a hex suits a resource.
type Ground = fn(&HexGrid, Hex) -> bool;

/// Open flat grassland or plains, the best ground for horses.
fn pasture(grid: &HexGrid, hex: Hex) -> bool {
    horse_ground(grid, hex) && grid.terrain(hex) != Terrain::Tundra
}

/// Open flat ground, for horses.
fn horse_ground(grid: &HexGrid, hex: Hex) -> bool {
    let tile = grid.tile(hex);
    !tile.hills
        && tile.feature.is_none()
        && matches!(
            tile.terrain,
            Terrain::Grassland | Terrain::Plains | Terrain::Tundra
        )
}

fn by_mountains(grid: &HexGrid, hex: Hex) -> bool {
    hex.neighbors()
        .into_iter()
        .any(|n| grid.contains(n) && grid.terrain(n) == Terrain::Mountains)
}

/// Hills beside mountains, the best ground for iron.
fn foothill(grid: &HexGrid, hex: Hex) -> bool {
    grid.tile(hex).hills && by_mountains(grid, hex)
}

/// Hills, or ground under mountains, for iron.
fn iron_ground(grid: &HexGrid, hex: Hex) -> bool {
    grid.tile(hex).hills || by_mountains(grid, hex)
}

/// Every hex within `radius` of `center`, on the map or not.
fn within(center: Hex, radius: i32) -> impl Iterator<Item = Hex> {
    (-radius..=radius).flat_map(move |dq| {
        ((-radius).max(-dq - radius)..=radius.min(-dq + radius))
            .map(move |dr| Hex::new(center.q + dq, center.r + dr))
    })
}

/// Steps on foot from `origin` to every passable hex it connects to.
fn walking_distances(grid: &HexGrid, origin: Hex) -> HashMap<Hex, i32> {
    let mut distances = HashMap::from_iter([(origin, 0)]);
    let mut queue = VecDeque::from([origin]);
    while let Some(hex) = queue.pop_front() {
        let next = distances[&hex] + 1;
        for n in hex.neighbors() {
            if grid.is_passable(n) && !distances.contains_key(&n) {
                distances.insert(n, next);
                queue.push_back(n);
            }
        }
    }
    distances
}

/// A hex's walking distances to every start, nearest first, or `None` if
/// some start can't walk there.
fn distances_to_starts(distances: &[HashMap<Hex, i32>], hex: Hex) -> Option<Vec<i32>> {
    let mut steps: Vec<i32> = distances
        .iter()
        .map(|map| map.get(&hex).copied())
        .collect::<Option<_>>()?;
    steps.sort_unstable();
    Some(steps)
}

/// Contested ground: hexes about as far on foot from the two starts nearest
/// them (within `slack` steps, a little more farther out), at least `near`
/// from any start and no farther than `far`, off the map's edge and free of
/// anything else. Being as close to a second start is what keeps a hex out
/// of a pocket only one side can reach. Each comes with how uneven it is,
/// for sorting: less uneven is better, and a hex nearly as close to a third
/// start is better still.
fn contested_hexes(
    grid: &HexGrid,
    starts: &[Hex],
    distances: &[HashMap<Hex, i32>],
    near: i32,
    far: i32,
    slack: i32,
) -> Vec<(Hex, i32)> {
    if starts.len() < 2 {
        return Vec::new();
    }
    grid.all_hexes()
        .filter(|&h| {
            grid.is_passable(h) && grid.edge_distance(h) >= 2 && grid.resource(h).is_none()
        })
        .filter_map(|h| {
            let steps = distances_to_starts(distances, h)?;
            let (nearest, second) = (steps[0], steps[1]);
            let uneven = second - nearest;
            let third = steps.get(2).map_or(0, |&third| (third - nearest) / 3);
            (nearest >= near && nearest <= far && uneven <= slack + nearest / 8)
                .then_some((h, 2 * uneven + third))
        })
        .collect()
}

/// Picks up to `wanted` of `options` (hex, unevenness), the most even first,
/// in random order among equals, at least `gap` apart and from `avoid`.
fn pick_spread(
    mut options: Vec<(Hex, i32)>,
    wanted: usize,
    gap: i32,
    avoid: &[Hex],
    rng: &mut Rng,
) -> Vec<Hex> {
    rng.shuffle(&mut options);
    options.sort_by_key(|&(_, uneven)| uneven);
    let mut picked: Vec<Hex> = Vec::new();
    for (hex, _) in options {
        if picked.len() == wanted {
            break;
        }
        if picked
            .iter()
            .chain(avoid)
            .all(|other| other.distance(hex) >= gap)
        {
            picked.push(hex);
        }
    }
    picked
}

/// Ruins: about one per side, each nearly equidistant on foot from the two
/// starts nearest it, well away from every start and from each other.
fn place_ruins(
    grid: &HexGrid,
    starts: &[Hex],
    distances: &[HashMap<Hex, i32>],
    spacing: f32,
    rng: &mut Rng,
) -> Vec<Hex> {
    let near = ((spacing * 0.35) as i32).max(5);
    let far = ((spacing * 0.9) as i32).max(near + 2);
    let options = contested_hexes(grid, starts, distances, near, far, 1);
    let wanted = starts.len() + rng.below(2);
    let gap = ((spacing * 0.6) as i32).max(4);
    pick_spread(options, wanted, gap, starts, rng)
}

/// Special tiles (`Special`): about one per side, of every kind in turn,
/// between the starts like the ruins but with more leeway, away from the
/// ruins and each other.
fn place_specials(
    grid: &mut HexGrid,
    starts: &[Hex],
    distances: &[HashMap<Hex, i32>],
    ruins: &[Hex],
    spacing: f32,
    rng: &mut Rng,
) {
    let near = ((spacing * 0.3) as i32).max(4);
    let far = ((spacing * 1.1) as i32).max(near + 2);
    let options: Vec<(Hex, i32)> = contested_hexes(grid, starts, distances, near, far, 3)
        .into_iter()
        .filter(|&(h, _)| ruins.iter().all(|r| r.distance(h) >= 3))
        .collect();
    let wanted = starts.len() + rng.below(2);
    let gap = ((spacing * 0.5) as i32).max(4);
    let first = rng.below(Special::ALL.len());
    for (i, hex) in pick_spread(options, wanted, gap, starts, rng)
        .into_iter()
        .enumerate()
    {
        grid.set_special(hex, Special::ALL[(first + i) % Special::ALL.len()]);
    }
}

/// Animal dens: up to `MAX_DENS_PER_SIDE` a side, on forest, jungle or
/// hills that every start can walk to, at least `DEN_START_DISTANCE` from
/// every start, off the map's edge, clear of resources, special tiles and
/// ruins, and `DEN_GAP` apart. In a random order, so the first few of them
/// are spread over the map as well as all of them.
fn place_dens(
    grid: &HexGrid,
    starts: &[Hex],
    distances: &[HashMap<Hex, i32>],
    ruins: &[Hex],
    rng: &mut Rng,
) -> Vec<Hex> {
    let options: Vec<(Hex, i32)> = grid
        .all_hexes()
        .filter(|&h| {
            let tile = grid.tile(h);
            grid.is_passable(h)
                && (tile.hills || tile.feature.is_some())
                && grid.edge_distance(h) >= 1
                && grid.resource(h).is_none()
                && grid.special(h).is_none()
                && ruins.iter().all(|r| r.distance(h) >= 2)
                && starts.iter().all(|s| s.distance(h) >= DEN_START_DISTANCE)
                && distances.iter().all(|map| map.contains_key(&h))
        })
        .map(|h| (h, 0))
        .collect();
    // All as good: `pick_spread` puts them in a random order.
    let wanted = MAX_DENS_PER_SIDE * starts.len();
    pick_spread(options, wanted, DEN_GAP, &[], rng)
}

/// Whether every side starting at `site` sees the same: the settler and
/// worker on flat ground, the scout on hills (see `start_units`), with room
/// around them.
fn fair_start(grid: &HexGrid, site: Hex) -> bool {
    let open: Vec<Hex> = site
        .neighbors()
        .into_iter()
        .filter(|n| grid.is_passable(*n))
        .collect();
    !grid.tile(site).hills && open.len() >= 4 && start_units(grid, site).is_some()
}

/// Where a side starting at `site` puts its worker (flat ground) and scout
/// (hills), both next to the settler.
pub fn start_units(grid: &HexGrid, site: Hex) -> Option<(Hex, Hex)> {
    let open = || {
        site.neighbors()
            .into_iter()
            .filter(|n| grid.is_passable(*n))
    };
    let worker = open().find(|n| !grid.tile(*n).hills)?;
    let scout = open().find(|n| grid.tile(*n).hills)?;
    Some((worker, scout))
}

/// How good a city site is: the yields within two hexes, fresh water, and a
/// shore.
fn start_score(grid: &HexGrid, site: Hex) -> i32 {
    let mut score = 0;
    for dq in -2..=2 {
        for dr in (-2).max(-dq - 2)..=2.min(-dq + 2) {
            let h = Hex::new(site.q + dq, site.r + dr);
            if !grid.contains(h) {
                continue;
            }
            let (food, production) = grid.tile(h).yields();
            let fresh = i32::from(grid.has_fresh_water(h));
            score += 3 * (food + fresh) + 2 * production;
        }
    }
    if grid.has_fresh_water(site) {
        score += 6;
    }
    score
}

/// Connected groups of hexes matching `keep`.
fn components(hexes: &[Hex], keep: impl Fn(Hex) -> bool) -> Vec<Vec<Hex>> {
    let members: HashSet<Hex> = hexes.iter().copied().filter(|h| keep(*h)).collect();
    let mut seen = HashSet::default();
    let mut groups = Vec::new();
    for &h in hexes {
        if !members.contains(&h) || !seen.insert(h) {
            continue;
        }
        let mut group = vec![h];
        let mut queue = VecDeque::from([h]);
        while let Some(next) = queue.pop_front() {
            for n in next.neighbors() {
                if members.contains(&n) && seen.insert(n) {
                    group.push(n);
                    queue.push_back(n);
                }
            }
        }
        groups.push(group);
    }
    groups
}

/// Each hex's position in `value` order, from 0 (lowest) to 1 (highest), so
/// thresholds pick fixed shares of the map whatever the noise's spread.
fn ranks(hexes: &[Hex], value: impl Fn(Hex) -> f32) -> HashMap<Hex, f32> {
    let mut sorted: Vec<(Hex, f32)> = hexes.iter().map(|&h| (h, value(h))).collect();
    sorted.sort_by(|a, b| {
        a.1.total_cmp(&b.1)
            .then((a.0.q, a.0.r).cmp(&(b.0.q, b.0.r)))
    });
    let last = (sorted.len().max(2) - 1) as f32;
    sorted
        .into_iter()
        .enumerate()
        .map(|(i, (h, _))| (h, i as f32 / last))
        .collect()
}

fn quantile(values: impl Iterator<Item = f32>, share: f32) -> f32 {
    let mut values: Vec<f32> = values.collect();
    values.sort_by(f32::total_cmp);
    values[((values.len() - 1) as f32 * share) as usize]
}

/// SplitMix64: small, fast, and the same on every platform and crate version.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix(self.0)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Uniform in 0..n.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }

    /// Puts `items` in a random order.
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            items.swap(i, self.below(i + 1));
        }
    }
}

fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Smooth value noise with a few octaves, roughly in [0, 1].
struct Noise(u64);

impl Noise {
    fn lattice(&self, x: i32, y: i32) -> f32 {
        let h = mix(self.0 ^ mix((x as u64) << 32 ^ (y as u32 as u64)));
        (h >> 40) as f32 / (1u64 << 24) as f32
    }

    fn value(&self, p: Vec2) -> f32 {
        let (x0, y0) = (p.x.floor(), p.y.floor());
        let (fx, fy) = (p.x - x0, p.y - y0);
        let smooth = |t: f32| t * t * (3.0 - 2.0 * t);
        let (sx, sy) = (smooth(fx), smooth(fy));
        let (x0, y0) = (x0 as i32, y0 as i32);
        let top = lerp(self.lattice(x0, y0), self.lattice(x0 + 1, y0), sx);
        let bottom = lerp(self.lattice(x0, y0 + 1), self.lattice(x0 + 1, y0 + 1), sx);
        lerp(top, bottom, sy)
    }

    fn fbm(&self, p: Vec2) -> f32 {
        let (mut sum, mut amplitude, mut total, mut frequency) = (0.0, 1.0, 0.0, 1.0);
        for octave in 0..4 {
            // Offset each octave so their lattices don't line up.
            let shift = Vec2::splat(octave as f32 * 17.3);
            sum += amplitude * self.value(p * frequency + shift);
            total += amplitude;
            amplitude *= 0.5;
            frequency *= 2.0;
        }
        sum / total
    }
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod preview;
#[cfg(test)]
mod tests;
