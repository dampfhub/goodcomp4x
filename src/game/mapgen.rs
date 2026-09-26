//! Random maps for the world scenario (F4): a Pangea on a rectangular map
//! wider than tall, sized for a four-player game. Everything comes from one
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
//! 8. Starts: two hexes on the continent (the largest stretch of passable
//!    land), far apart, each with the best food and production nearby that
//!    the other can match.

use std::collections::{HashMap, HashSet, VecDeque};

use glam::Vec2;

use super::hex::{Hex, HexGrid, Shape, edge};
use super::terrain::{Feature, Terrain, Tile};

/// A generated world: 61 columns by about 36 rows, room for four players.
pub const WORLD_SHAPE: Shape = Shape::Rectangle { cols: 30, rows: 18 };

/// Land apart from the continent survives only as islands this small.
const MAX_ISLAND: usize = 12;

pub struct GeneratedMap {
    pub grid: HexGrid,
    /// Where each side starts: the left-hand one is Blue's.
    pub starts: [Hex; 2],
}

/// Builds a world map from `seed`.
pub fn generate(seed: u32) -> GeneratedMap {
    // A rare map has no two good starts; try the next variant.
    for attempt in 0..16u64 {
        let mut rng = Rng(u64::from(seed) << 8 | attempt);
        let (tiles, rivers) = shape_world(WORLD_SHAPE, &mut rng);
        let grid = HexGrid::shaped(WORLD_SHAPE, tiles).with_rivers(rivers);
        if let Some(starts) = pick_starts(&grid) {
            return GeneratedMap { grid, starts };
        }
    }
    GeneratedMap {
        grid: HexGrid::shaped(WORLD_SHAPE, [(Hex::new(0, 0), Terrain::Plains)]),
        starts: [Hex::new(-8, 4), Hex::new(8, -4)],
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
    let mut seen: HashSet<Corner> = HashSet::new();
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

    let mut rivers = HashSet::new();
    let mut river_corners: HashSet<Corner> = HashSet::new();
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
        let mut visited: HashSet<Corner> = HashSet::from([start]);
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

/// Two start hexes on the continent (the largest stretch of passable land):
/// as evenly good as possible, and far apart: a third of the map's width if
/// the land allows, closer if it doesn't.
/// Each is also a `fair_start`.
fn pick_starts(grid: &HexGrid) -> Option<[Hex; 2]> {
    let far = match grid.shape() {
        Shape::Hexagon { radius } => radius,
        Shape::Rectangle { cols, .. } => 2 * cols / 3,
    };
    let all: Vec<Hex> = grid.all_hexes().collect();
    let landmass = components(&all, |h| grid.is_passable(h))
        .into_iter()
        .max_by_key(Vec::len)?;
    let scored: Vec<(Hex, i32)> = landmass
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
    for min_distance in [far, far * 3 / 4, far / 2, 4] {
        let mut best: Option<(i32, [Hex; 2])> = None;
        for (i, &(a, score_a)) in scored.iter().enumerate() {
            for &(b, score_b) in &scored[i + 1..] {
                if a.distance(b) < min_distance {
                    continue;
                }
                let value = 2 * score_a.min(score_b) - (score_a - score_b).abs();
                if best.is_none_or(|(v, _)| value > v) {
                    best = Some((value, [a, b]));
                }
            }
        }
        if let Some((_, mut pair)) = best {
            pair.sort_by(|a, b| a.to_world().x.total_cmp(&b.to_world().x));
            return Some(pair);
        }
    }
    None
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
    let mut seen = HashSet::new();
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
        let (a, b) = (generate(7), generate(7));
        assert_eq!(fingerprint(&a), fingerprint(&b));
        assert_eq!(a.starts, b.starts);
        let mut rivers_a: Vec<_> = a.grid.rivers().collect();
        let mut rivers_b: Vec<_> = b.grid.rivers().collect();
        rivers_a.sort_by_key(|(x, y)| (x.q, x.r, y.q, y.r));
        rivers_b.sort_by_key(|(x, y)| (x.q, x.r, y.q, y.r));
        assert_eq!(rivers_a, rivers_b);
        assert_ne!(fingerprint(&a), fingerprint(&generate(8)));
    }

    #[test]
    fn starts_are_far_apart_on_connected_open_land() {
        for seed in 0..12 {
            let map = generate(seed);
            let [a, b] = map.starts;
            assert!(
                map.grid.is_passable(a) && map.grid.is_passable(b),
                "seed {seed}"
            );
            assert!(a.distance(b) >= 10, "seed {seed}: {a:?} {b:?}");
            assert!(a.to_world().x <= b.to_world().x, "blue starts on the left");
            let all: Vec<Hex> = map.grid.all_hexes().collect();
            let joined = components(&all, |h| map.grid.is_passable(h))
                .into_iter()
                .any(|land| land.contains(&a) && land.contains(&b));
            assert!(joined, "seed {seed}: starts on different landmasses");
        }
    }

    #[test]
    fn the_land_is_one_continent_with_only_small_islands() {
        for seed in 0..8 {
            let map = generate(seed);
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
        let map = generate(1);
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
            let map = generate(seed);
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
        let mut grounds = HashSet::new();
        let mut hilly_grounds = HashSet::new();
        let (mut water, mut hexes) = (0, 0);
        for seed in 0..10 {
            let map = generate(seed);
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
