//! Random maps for the world scenario (F4): a Pangea on a rectangular map
//! wider than tall, sized for the number of players. Everything comes from one
//! seed, so a seed always rebuilds the same map.
//!
//! The recipe:
//! 1. Elevation: an oval dome filling most of the map, roughened by two
//!    layers of fractal value noise into bays, peninsulas and inland seas.
//!    The lowest share of hexes (42-52%, varying per map) becomes water.
//! 2. One continent: the biggest landmass stays; other land sinks, unless
//!    it's a small island of at most `MAX_ISLAND` hexes.
//! 3. Relief: a ridged noise field mixed with elevation picks the most rugged
//!    land for mountain ranges, and the next most rugged for hills. Hills are
//!    a modifier, so whatever ground the climate gives them stays hilly.
//! 4. Water bodies: small ones cut off from the map's edge become lakes, and
//!    a few more lakes are dropped into low ground.
//! 5. Rivers run along hex edges, starting beside high ground and always
//!    stepping to the lowest neighboring corner, until they reach water, join
//!    another river, or get stuck (where they end in a lake).
//! 6. Climate: temperature falls toward the top and bottom of the map with
//!    noise on top; moisture is noise plus a boost near rivers, lakes and the
//!    sea. Together they pick the ground: snow, tundra, desert, marsh,
//!    grassland or plains.
//! 7. Vegetation: forest grows on the wetter grassland, plains and tundra
//!    (hills included), in patches from its own noise; jungle covers most
//!    marsh.
//! 8. Starts: one hex per side on the continent (the largest stretch of
//!    passable land), scattered and then evened out so each is about as far
//!    from its nearest neighbor as the land allows if shared out evenly, with
//!    about as good food and production nearby as the others.
//! 9. Horses and iron: one of each within a few hexes of every start, nearer
//!    it than any other start.
//! 10. Contested ground: ruins, and special tiles that yield more, go where
//!     two starts are about as far on foot, well away from both, so no side
//!     has them to itself.

use std::collections::VecDeque;

use glam::Vec2;

use super::fast_hash::{HashMap, HashSet};
use super::hex::{Hex, HexGrid, Shape, edge};
use super::terrain::{Feature, Resource, Special, Terrain, Tile};

/// Sides the smallest world is sized for: fewer still get this much room.
const MIN_WORLD_SIDES: usize = 3;
/// How many sides share the area of the base world (61 by about 36 hexes):
/// each side gets `1 / SIDES_PER_BASE_WORLD` of it, so the map grows in
/// proportion to the number of sides.
const SIDES_PER_BASE_WORLD: f32 = 2.5;

/// The world's size for `sides` players: the base 61 columns by about 36
/// rows for two and a half of them, growing in area with every side so each
/// has about as much land (six get about 93 by 57).
pub fn world_shape(sides: usize) -> Shape {
    let scale = (sides.max(MIN_WORLD_SIDES) as f32 / SIDES_PER_BASE_WORLD).sqrt();
    Shape::Rectangle {
        cols: (30.0 * scale).round() as i32,
        rows: (18.0 * scale).round() as i32,
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
}

/// Builds a world map for `sides` players from `seed`.
pub fn generate(seed: u32, sides: usize) -> GeneratedMap {
    let sides = sides.max(1);
    let shape = world_shape(sides);
    // A rare map has no good set of starts; try the next variant.
    for attempt in 0..16u64 {
        let mut rng = Rng(u64::from(seed) << 8 | attempt);
        let (tiles, rivers) = shape_world(shape, &mut rng);
        let mut grid = HexGrid::shaped(shape, tiles).with_rivers(rivers);
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
        return GeneratedMap {
            grid,
            starts,
            ruins,
        };
    }
    GeneratedMap {
        grid: HexGrid::shaped(shape, [(Hex::new(0, 0), Terrain::Plains)]),
        starts: (0..sides as i32).map(|i| Hex::new(4 * i - 8, 0)).collect(),
        ruins: Vec::new(),
    }
}

type Tiles = HashMap<Hex, Tile>;

fn shape_world(shape: Shape, rng: &mut Rng) -> (Tiles, HashSet<(Hex, Hex)>) {
    let blank = HexGrid::shaped::<Tile>(shape, []);
    let hexes: Vec<Hex> = blank.all_hexes().collect();
    let on_map = |h: Hex| blank.contains(h);
    let on_rim = |h: Hex| blank.edge_distance(h) == 0;
    let extent = hexes
        .iter()
        .fold(Vec2::ZERO, |m, h| m.max(h.to_world().abs()));
    let (height_noise, detail_noise, ridge_noise, heat_noise, wet_noise, growth_noise) = (
        Noise(rng.next()),
        Noise(rng.next()),
        Noise(rng.next()),
        Noise(rng.next()),
        Noise(rng.next()),
        Noise(rng.next()),
    );

    // 1. Elevation: a dome over the middle of the map, so the land gathers
    // into one mass, with noise carving its coast.
    let elevation: HashMap<Hex, f32> = hexes
        .iter()
        .map(|&h| {
            let p = h.to_world();
            let dome = 1.0 - (p / extent).length();
            let coast = height_noise.fbm(p / 9.0) - 0.5;
            let detail = detail_noise.fbm(p / 3.5) - 0.5;
            (h, dome + 0.7 * coast + 0.25 * detail)
        })
        .collect();
    let sea_level = quantile(elevation.values().copied(), 0.42 + 0.1 * rng.unit());
    let mut tiles: Tiles = hexes
        .iter()
        .map(|&h| {
            let ground = if elevation[&h] < sea_level {
                Terrain::Ocean
            } else {
                Terrain::Plains
            };
            (h, Tile::from(ground))
        })
        .collect();

    // 2. One continent, plus at most a few small islands.
    let mut masses = components(&hexes, |h| !tiles[&h].terrain.is_water());
    masses.sort_by_key(|m| std::cmp::Reverse(m.len()));
    for mass in masses.iter().skip(1).filter(|m| m.len() > MAX_ISLAND) {
        for h in mass {
            tiles.insert(*h, Tile::from(Terrain::Ocean));
        }
    }
    let land: Vec<Hex> = hexes
        .iter()
        .copied()
        .filter(|h| !tiles[h].terrain.is_water())
        .collect();

    // 3. Relief: ridged noise draws lines of high ground, so mountains come
    // in ranges rather than one lump at the highest point.
    let height_rank = ranks(&land, |h| elevation[&h]);
    let ridge_rank = ranks(&land, |h| {
        1.0 - (2.0 * ridge_noise.fbm(h.to_world() / 6.0) - 1.0).abs()
    });
    let rugged = ranks(&land, |h| 0.4 * height_rank[&h] + 0.6 * ridge_rank[&h]);
    let mountain_share = 0.04 + 0.03 * rng.unit();
    let hill_share = 0.12 + 0.06 * rng.unit();
    for &h in &land {
        let r = rugged[&h];
        if r > 1.0 - mountain_share {
            tiles.insert(h, Tile::MOUNTAINS);
        } else if r > 1.0 - mountain_share - hill_share {
            tiles.insert(h, Tile::HILLS);
        }
    }

    // 4. Small enclosed water bodies are lakes; so are a few dips in the land.
    let lake = Tile::from(Terrain::Lake);
    for body in components(&hexes, |h| tiles[&h].terrain.is_water()) {
        if body.len() <= 10 && !body.iter().any(|&h| on_rim(h)) {
            for h in body {
                tiles.insert(h, lake);
            }
        }
    }
    let lowland: Vec<Hex> = land
        .iter()
        .copied()
        .filter(|h| {
            tiles[h] == Tile::from(Terrain::Plains)
                && height_rank[h] < 0.4
                && !on_rim(*h)
                && h.neighbors()
                    .iter()
                    .all(|n| on_map(*n) && !tiles[n].terrain.is_water())
        })
        .collect();
    for _ in 0..rng.below(2 + land.len() / 300) {
        if !lowland.is_empty() {
            tiles.insert(lowland[rng.below(lowland.len())], lake);
        }
    }

    // 5. Rivers.
    let wanted = land.len() / 60 + rng.below(4);
    let rivers = carve_rivers(&blank, wanted, &hexes, &elevation, &mut tiles, rng);

    // What's left of the sea is coast next to land, open ocean beyond.
    for &h in &hexes {
        if tiles[&h].terrain == Terrain::Ocean
            && h.neighbors()
                .iter()
                .any(|n| on_map(*n) && !tiles[n].terrain.is_water())
        {
            tiles.insert(h, Tile::from(Terrain::Coast));
        }
    }

    // 6. Climate for all land but mountains, hills included.
    let open: Vec<Hex> = hexes
        .iter()
        .copied()
        .filter(|h| tiles[h].terrain == Terrain::Plains)
        .collect();
    let heat = ranks(&open, |h| {
        let latitude = 1.0 - h.to_world().y.abs() / extent.y;
        0.6 * latitude + 0.4 * heat_noise.fbm(h.to_world() / 5.0)
    });
    let grid = HexGrid::shaped(shape, tiles.clone()).with_rivers(rivers.clone());
    let wet = ranks(&open, |h| {
        let fresh = if grid.has_fresh_water(h) { 0.2 } else { 0.0 };
        let sea = h
            .neighbors()
            .iter()
            .any(|n| on_map(*n) && matches!(tiles[n].terrain, Terrain::Ocean | Terrain::Coast));
        wet_noise.fbm(h.to_world() / 5.0) + fresh + if sea { 0.1 } else { 0.0 }
    });
    let growth = ranks(&open, |h| growth_noise.fbm(h.to_world() / 3.0));
    for &h in &open {
        let (t, w) = (heat[&h], wet[&h]);
        let tile = tiles.get_mut(&h).expect("open hexes are on the map");
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

        // 6. Vegetation.
        tile.feature = match tile.terrain {
            Terrain::Marsh if rng.unit() < 0.75 => Some(Feature::Jungle),
            Terrain::Grassland | Terrain::Plains | Terrain::Tundra
                if 0.6 * w + 0.4 * growth[&h] > 0.66 =>
            {
                Some(Feature::Forest)
            }
            _ => None,
        };
    }
    (tiles, rivers)
}

/// A corner where three hexes meet, as those hexes in sorted order.
type Corner = [Hex; 3];

fn corner(mut hexes: [Hex; 3]) -> Corner {
    hexes.sort_by_key(|h| (h.q, h.r));
    hexes
}

/// The three corners one edge away from `c`, each with the edge between them.
fn next_corners(c: Corner) -> [(Corner, (Hex, Hex)); 3] {
    [(0, 1, 2), (1, 2, 0), (0, 2, 1)].map(|(i, j, k)| {
        let (a, b, away) = (c[i], c[j], c[k]);
        // Two adjacent hexes share two neighbors: `away`, and the one across
        // the edge from it.
        let across = a
            .neighbors()
            .into_iter()
            .find(|n| *n != away && n.distance(b) == 1)
            .expect("adjacent hexes share two neighbors");
        (corner([a, b, across]), edge(a, b))
    })
}

fn carve_rivers(
    blank: &HexGrid,
    wanted: usize,
    hexes: &[Hex],
    elevation: &HashMap<Hex, f32>,
    tiles: &mut Tiles,
    rng: &mut Rng,
) -> HashSet<(Hex, Hex)> {
    let on_map = |h: Hex| blank.contains(h);
    let high = |t: &Tile| t.hills || t.terrain == Terrain::Mountains;
    let height = |c: &Corner, tiles: &Tiles| -> f32 {
        c.iter()
            .map(|h| match tiles.get(h) {
                Some(t) if t.terrain == Terrain::Mountains => elevation[h] + 0.15,
                Some(t) if t.hills => elevation[h] + 0.05,
                Some(_) => elevation[h],
                None => -1.0,
            })
            .sum::<f32>()
            / 3.0
    };
    let touches_water =
        |c: &Corner, tiles: &Tiles| c.iter().any(|h| !on_map(*h) || tiles[h].terrain.is_water());

    // Sources: corners beside hills or mountains, away from water.
    let mut sources: Vec<Corner> = Vec::new();
    let mut seen: HashSet<Corner> = HashSet::default();
    for &h in hexes {
        if !high(&tiles[&h]) {
            continue;
        }
        let around = h.neighbors();
        for i in 0..6 {
            let c = corner([h, around[i], around[(i + 1) % 6]]);
            if !touches_water(&c, tiles) && seen.insert(c) {
                sources.push(c);
            }
        }
    }

    let mut rivers = HashSet::default();
    let mut river_corners: HashSet<Corner> = HashSet::default();
    let mut used_sources: Vec<Hex> = Vec::new();
    let mut tries = 0;
    while used_sources.len() < wanted && !sources.is_empty() && tries < 120 {
        tries += 1;
        let start = sources.swap_remove(rng.below(sources.len()));
        if used_sources.iter().any(|s| s.distance(start[0]) < 4) || river_corners.contains(&start) {
            continue;
        }
        let mut path = vec![start];
        let mut edges = Vec::new();
        let mut visited: HashSet<Corner> = HashSet::from_iter([start]);
        let mut current = start;
        let mut ends_in_lake = true;
        for _ in 0..50 {
            if touches_water(&current, tiles) || river_corners.contains(&current) {
                ends_in_lake = false;
                break;
            }
            let next = next_corners(current)
                .into_iter()
                .filter(|(c, _)| !visited.contains(c))
                .map(|(c, e)| (c, e, height(&c, tiles) + 0.03 * rng.unit()))
                .min_by(|a, b| a.2.total_cmp(&b.2));
            let Some((c, e, _)) = next else { break };
            visited.insert(c);
            path.push(c);
            edges.push(e);
            current = c;
        }
        if edges.len() < 3 {
            continue;
        }
        if ends_in_lake {
            // Stuck in a dip: pool the water in its lowest flat hex.
            let pool = current
                .into_iter()
                .filter(|h| on_map(*h) && tiles[h] == Tile::from(Terrain::Plains))
                .min_by(|a, b| elevation[a].total_cmp(&elevation[b]));
            match pool {
                Some(h) => {
                    tiles.insert(h, Tile::from(Terrain::Lake));
                }
                None => continue,
            }
        }
        used_sources.push(start[0]);
        river_corners.extend(path);
        rivers.extend(edges);
    }
    // A river only runs between two land hexes.
    rivers.retain(|(a, b)| !tiles[a].terrain.is_water() && !tiles[b].terrain.is_water());
    rivers
}

/// One start per side on the continent (the largest stretch of passable
/// land), with the spacing they aim for: the distance between neighbors if
/// the land were shared out evenly. Each start is a `fair_start` on good
/// ground. The set is scattered at random and then evened out
/// (`start_cost`): every start about `spacing` from its nearest neighbor, some
/// closer and some farther, and about as good as the others. The player's
/// start, first, is any of them.
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
        let mut best: Option<(f32, Vec<usize>)> = None;
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
            if best.as_ref().is_none_or(|(c, _)| cost < *c) {
                best = Some((cost, set));
            }
        }
        if let Some((_, set)) = best {
            let mut starts: Vec<Hex> = set.iter().map(|&i| candidates[i].0).collect();
            starts.sort_by_key(|h| (h.q, h.r));
            let player = rng.below(starts.len());
            starts.swap(0, player);
            return Some((starts, spacing));
        }
    }
    None
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
    for i in (1..order.len()).rev() {
        order.swap(i, rng.below(i + 1));
    }
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
                ((nearest - spacing) / spacing).powi(2)
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
/// nearer it than any other start: horses on open flat ground, iron on hills
/// or under mountains where there are some, anywhere open otherwise.
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
        let kinds: [(Resource, Ground); 2] = [
            (Resource::Horses, horse_ground),
            (Resource::Iron, iron_ground),
        ];
        for (resource, fits) in kinds {
            let options = |reach: i32, strict: bool| -> Vec<Hex> {
                within(start, reach)
                    .filter(|&h| {
                        h.distance(start) >= 2
                            && grid.is_passable(h)
                            && !taken.contains(&h)
                            && grid.resource(h).is_none()
                            && own(h)
                            && (!strict || fits(grid, h))
                    })
                    .collect()
            };
            let spot = [(3, true), (4, true), (3, false), (5, false)]
                .into_iter()
                .map(|(reach, strict)| options(reach, strict))
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

/// Hills, or ground under mountains, for iron.
fn iron_ground(grid: &HexGrid, hex: Hex) -> bool {
    grid.tile(hex).hills
        || hex
            .neighbors()
            .into_iter()
            .any(|n| grid.contains(n) && grid.terrain(n) == Terrain::Mountains)
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
    for i in (1..options.len()).rev() {
        options.swap(i, rng.below(i + 1));
    }
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
mod tests {
    use super::*;

    fn fingerprint(map: &GeneratedMap) -> Vec<(Hex, Tile)> {
        map.grid
            .all_hexes()
            .map(|h| (h, map.grid.tile(h)))
            .collect()
    }

    #[test]
    fn a_seed_always_builds_the_same_map() {
        let (a, b) = (generate(7, 5), generate(7, 5));
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a.starts, b.starts);
        assert_eq!(a.ruins, b.ruins);
        let extras = |map: &GeneratedMap| -> Vec<_> {
            map.grid
                .all_hexes()
                .map(|h| (h, map.grid.resource(h), map.grid.special(h)))
                .collect()
        };
        assert_eq!(extras(&a), extras(&b));
        let mut rivers_a: Vec<_> = a.grid.rivers().collect();
        let mut rivers_b: Vec<_> = b.grid.rivers().collect();
        rivers_a.sort_by_key(|(x, y)| (x.q, x.r, y.q, y.r));
        rivers_b.sort_by_key(|(x, y)| (x.q, x.r, y.q, y.r));
        assert_eq!(rivers_a, rivers_b);
        assert_ne!(fingerprint(&a), fingerprint(&generate(8, 5)));
    }

    /// The spacing a map's starts aim for (see `pick_starts`).
    fn spacing(map: &GeneratedMap) -> f32 {
        let all: Vec<Hex> = map.grid.all_hexes().collect();
        let land = components(&all, |h| map.grid.is_passable(h))
            .into_iter()
            .map(|c| c.len())
            .max()
            .unwrap();
        1.075 * (land as f32 / map.starts.len() as f32).sqrt()
    }

    #[test]
    fn every_side_gets_a_start_neither_crowded_nor_isolated() {
        for sides in [2, 5, 7] {
            for seed in 0..8 {
                let map = generate(seed, sides);
                let starts = &map.starts;
                assert_eq!(starts.len(), sides, "seed {seed}");
                let all: Vec<Hex> = map.grid.all_hexes().collect();
                let land = components(&all, |h| map.grid.is_passable(h))
                    .into_iter()
                    .find(|land| land.contains(&starts[0]))
                    .unwrap();
                let spacing = spacing(&map);
                for (i, a) in starts.iter().enumerate() {
                    assert!(map.grid.is_passable(*a), "seed {seed}");
                    assert!(
                        land.contains(a),
                        "seed {seed}: starts on different landmasses"
                    );
                    let nearest = starts
                        .iter()
                        .enumerate()
                        .filter(|&(j, _)| j != i)
                        .map(|(_, b)| a.distance(*b) as f32)
                        .fold(f32::MAX, f32::min);
                    if sides > 1 {
                        assert!(
                            nearest >= (0.45 * spacing).max(4.0),
                            "{sides} sides, seed {seed}: a start {nearest} from its nearest, \
                             spacing {spacing}"
                        );
                        assert!(
                            nearest <= 2.0 * spacing,
                            "{sides} sides, seed {seed}: a start {nearest} from its nearest, \
                             spacing {spacing}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_start_has_horses_and_iron_close_by() {
        for seed in 0..8 {
            let map = generate(seed, 6);
            for (i, &start) in map.starts.iter().enumerate() {
                for resource in [Resource::Horses, Resource::Iron] {
                    let own =
                        within(start, 5).find(|&h| {
                            map.grid.resource(h) == Some(resource)
                                && map.starts.iter().enumerate().all(|(j, other)| {
                                    j == i || h.distance(*other) > h.distance(start)
                                })
                        });
                    let own =
                        own.unwrap_or_else(|| panic!("seed {seed}: no {resource:?} near a start"));
                    assert!(own.distance(start) >= 2, "not under the city");
                    assert!(map.grid.is_passable(own));
                }
            }
        }
    }

    #[test]
    fn ruins_and_special_tiles_lie_between_starts_and_near_none() {
        let (mut ruins, mut specials) = (0, 0);
        for seed in 0..8 {
            let map = generate(seed, 5);
            let distances: Vec<HashMap<Hex, i32>> = map
                .starts
                .iter()
                .map(|&s| walking_distances(&map.grid, s))
                .collect();
            let spacing = spacing(&map);
            let check = |hex: Hex, near: i32, slack: i32, what: &str| {
                assert!(map.grid.is_passable(hex), "seed {seed}: {what} off land");
                assert!(
                    map.grid.edge_distance(hex) >= 2,
                    "seed {seed}: {what} at the edge"
                );
                let steps = distances_to_starts(&distances, hex)
                    .unwrap_or_else(|| panic!("seed {seed}: some start can't reach the {what}"));
                assert!(
                    steps[0] >= near,
                    "seed {seed}: {what} {} from a start",
                    steps[0]
                );
                assert!(
                    steps[1] - steps[0] <= slack + steps[0] / 8,
                    "seed {seed}: {what} {steps:?} steps from the starts: one side's alone"
                );
            };
            for &ruin in &map.ruins {
                check(ruin, ((spacing * 0.35) as i32).max(5), 1, "ruins");
            }
            let special_hexes: Vec<Hex> = map
                .grid
                .all_hexes()
                .filter(|&h| map.grid.special(h).is_some())
                .collect();
            for &hex in &special_hexes {
                check(hex, ((spacing * 0.3) as i32).max(4), 3, "special tile");
                assert!(map.ruins.iter().all(|r| r.distance(hex) >= 3));
            }
            ruins += map.ruins.len();
            specials += special_hexes.len();
        }
        assert!(
            ruins >= 16,
            "about one ruin per side: {ruins} in 8 maps of 5"
        );
        assert!(
            specials >= 16,
            "about one special tile per side: {specials} in 8 maps"
        );
    }

    #[test]
    fn the_world_grows_with_the_players() {
        let area = |sides| match world_shape(sides) {
            Shape::Rectangle { cols, rows } => cols * rows,
            Shape::Hexagon { radius } => radius * radius,
        };
        assert_eq!(area(1), area(3), "three players' worth at least");
        assert!(area(7) > area(5) && area(5) > area(4) && area(4) > area(3));
        // Each side gets about as much room however many there are.
        let per_side = |sides: usize| area(sides) as f32 / sides as f32;
        assert!((per_side(6) / per_side(4) - 1.0).abs() < 0.1);
        // Six sides get well over the base world.
        assert!(area(6) as f32 > 2.2 * (30 * 18) as f32);
    }

    #[test]
    fn the_land_is_one_continent_with_only_small_islands() {
        for seed in 0..8 {
            let map = generate(seed, 4);
            let all: Vec<Hex> = map.grid.all_hexes().collect();
            let mut masses = components(&all, |h| !map.grid.terrain(h).is_water());
            masses.sort_by_key(|m| std::cmp::Reverse(m.len()));
            assert!(masses[0].len() > 600, "seed {seed}: continent too small");
            assert!(
                masses[1..].iter().all(|m| m.len() <= MAX_ISLAND),
                "seed {seed}: a second big landmass"
            );
            for start in map.starts {
                assert!(
                    masses[0].contains(&start),
                    "seed {seed}: start off the continent"
                );
            }
        }
    }

    #[test]
    fn the_world_is_wider_than_tall() {
        let map = generate(1, 4);
        let extent = map
            .grid
            .all_hexes()
            .fold(Vec2::ZERO, |m, h| m.max(h.to_world().abs()));
        assert!(extent.x > 1.3 * extent.y, "{extent}");
        assert!(map.grid.all_hexes().count() > 2000);
    }

    #[test]
    fn rivers_run_between_adjacent_land_hexes() {
        let mut total = 0;
        for seed in 0..8 {
            let map = generate(seed, 4);
            for (a, b) in map.grid.rivers() {
                assert_eq!(a.distance(b), 1);
                assert!(!map.grid.terrain(a).is_water() && !map.grid.terrain(b).is_water());
                assert!(map.grid.has_fresh_water(a) && map.grid.has_fresh_water(b));
                total += 1;
            }
        }
        assert!(
            total > 30,
            "rivers should be common, got {total} edges in 8 maps"
        );
    }

    #[test]
    fn maps_mix_every_kind_of_tile() {
        let mut grounds = HashSet::default();
        let mut hilly_grounds = HashSet::default();
        let (mut water, mut hexes) = (0, 0);
        for seed in 0..10 {
            let map = generate(seed, 4);
            for h in map.grid.all_hexes() {
                let tile = map.grid.tile(h);
                let ground = format!("{:?}", tile.terrain);
                if tile.hills {
                    assert!(tile.terrain.is_passable(), "hills are land");
                    hilly_grounds.insert(ground.clone());
                }
                match tile.feature {
                    Some(Feature::Jungle) => assert_eq!(tile.terrain, Terrain::Marsh),
                    Some(Feature::Forest) => assert!(matches!(
                        tile.terrain,
                        Terrain::Grassland | Terrain::Plains | Terrain::Tundra
                    )),
                    None => {}
                }
                if tile.feature == Some(Feature::Forest) && tile.hills {
                    grounds.insert("ForestedHills".to_string());
                }
                if let Some(feature) = tile.feature {
                    grounds.insert(format!("{feature:?}"));
                }
                grounds.insert(ground);
                water += usize::from(tile.terrain.is_water());
                hexes += 1;
            }
        }
        for t in [
            "Grassland",
            "Plains",
            "Desert",
            "Tundra",
            "Snow",
            "Marsh",
            "Mountains",
            "Coast",
            "Ocean",
            "Lake",
            "Forest",
            "Jungle",
            "ForestedHills",
        ] {
            assert!(grounds.contains(t), "no {t} in 10 maps");
        }
        for t in ["Grassland", "Plains", "Desert", "Tundra"] {
            assert!(hilly_grounds.contains(t), "no {t} hills in 10 maps");
        }
        let share = water as f32 / hexes as f32;
        assert!((0.35..0.65).contains(&share), "water share {share}");
    }
}
