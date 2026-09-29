//! Tests only: looking at generated worlds without the game. Two ignored
//! tests, run by hand when changing `mapgen.rs`:
//!
//! - `map_previews` draws whole maps, fog-free, as PNGs: set `MAPGEN_DUMP` to
//!   a directory and `MAPGEN_SEEDS` to a comma-separated list of seeds
//!   (default `1,2,3,7,42`; a range like `0..8` works too), then
//!   `cargo test --release map_previews -- --ignored`.
//! - `map_stats` prints measurements over many seeds (`MAPGEN_SEEDS`, by
//!   default `0..50`): terrain shares, mountain clusters and
//!   their shape, range lengths and how far mountains make walking around
//!   them, rivers, start spacing on foot and start yields, world sizes and
//!   the decent city sites each side's share of the land holds:
//!   `cargo test --release map_stats -- --ignored --nocapture`.
//!
//! A map has as many sides as the world scenario gives that seed by default.

use std::fs::File;
use std::io::BufWriter;

use glam::Vec2;

use super::*;

/// Sides on a seed's map, as `Settings::world_ai_for` picks them by default
/// (the player and four to six AI).
pub fn default_sides(seed: u32) -> usize {
    5 + (seed % 3) as usize
}

/// The seeds in `MAPGEN_SEEDS`, or `default`: a comma-separated list, or a
/// range like `0..50`.
fn seeds(default: &str) -> Vec<u32> {
    let text = std::env::var("MAPGEN_SEEDS").unwrap_or_else(|_| default.to_string());
    if let Some((from, to)) = text.split_once("..") {
        return (from.trim().parse().unwrap()..to.trim().parse().unwrap()).collect();
    }
    text.split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

/// Pixels per world unit (a hex's corner radius), unless `MAPGEN_SCALE` says.
const SCALE: f32 = 9.0;

fn scale() -> f32 {
    std::env::var("MAPGEN_SCALE")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(SCALE)
}

fn ground_color(tile: Tile) -> [u8; 3] {
    match tile.terrain {
        Terrain::Grassland => [96, 158, 64],
        Terrain::Plains => [184, 170, 92],
        Terrain::Desert => [232, 212, 146],
        Terrain::Tundra => [146, 150, 128],
        Terrain::Snow => [240, 242, 246],
        Terrain::Marsh => [86, 124, 104],
        Terrain::Mountains => [92, 72, 60],
        Terrain::Coast => [44, 88, 156],
        Terrain::Ocean => [22, 42, 92],
        Terrain::Lake => [70, 150, 214],
    }
}

fn shade(color: [u8; 3], by: f32) -> [u8; 3] {
    color.map(|c| (c as f32 * by).min(255.0) as u8)
}

#[test]
#[ignore = "writes PNGs of whole maps for looking at by hand"]
fn map_previews() {
    let dir = std::path::PathBuf::from(
        std::env::var("MAPGEN_DUMP").expect("set MAPGEN_DUMP to the directory to write to"),
    );
    std::fs::create_dir_all(&dir).unwrap();
    for seed in seeds("1,2,3,7,42") {
        let (map, rivers) = generate_with_rivers(seed, default_sides(seed));
        let path = dir.join(format!("map-{seed}.png"));
        let marks: Vec<(Vec2, [u8; 3])> = rivers
            .iter()
            .flat_map(|river| {
                let center = |c: &Corner| c.iter().map(|h| h.to_world()).sum::<Vec2>() / 3.0;
                [
                    (center(&river[0]), [0, 255, 0]),
                    (center(river.last().unwrap()), [255, 140, 0]),
                ]
            })
            .collect();
        write_png(&map, &marks, &path);
        println!("wrote {}", path.display());
    }
}

fn write_png(map: &GeneratedMap, marks: &[(Vec2, [u8; 3])], path: &std::path::Path) {
    let grid = &map.grid;
    let (min, max) = grid.bounds();
    let (min, max) = (min - Vec2::splat(1.2), max + Vec2::splat(1.2));
    let scale = scale();
    let size = (max - min) * scale;
    let (width, height) = (size.x as u32, size.y as u32);
    let sqrt3 = 3f32.sqrt();
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let p = min + Vec2::new(x as f32 + 0.5, y as f32 + 0.5) / scale;
            let hex = Hex::from_world(p);
            let mut color = [12, 12, 16];
            if grid.contains(hex) {
                let tile = grid.tile(hex);
                let local = p - hex.to_world();
                let r = local.length();
                color = ground_color(tile);
                if tile.hills && r < 0.5 {
                    color = shade(color, 0.72);
                }
                match tile.feature {
                    Some(Feature::Forest) if r < 0.65 && (local.x * 3.0).rem_euclid(1.0) < 0.4 => {
                        color = [34, 84, 36];
                    }
                    Some(Feature::Jungle) if r < 0.65 && (local.x * 3.0).rem_euclid(1.0) < 0.4 => {
                        color = [20, 96, 80];
                    }
                    _ => {}
                }
                if let Some(resource) = grid.resource(hex)
                    && (local - Vec2::new(-0.35, -0.35)).length() < 0.2
                {
                    color = match resource {
                        Resource::Horses => [250, 250, 250],
                        Resource::Iron => [0, 0, 0],
                    };
                }
                if grid.special(hex).is_some() && (local - Vec2::new(0.35, -0.35)).length() < 0.22 {
                    color = [230, 60, 220];
                }
                if map.ruins.contains(&hex) && r < 0.35 {
                    color = [250, 220, 40];
                }
                if let Some(i) = map.starts.iter().position(|&s| s == hex)
                    && r < 0.55
                {
                    color = if i == 0 {
                        [255, 255, 255]
                    } else {
                        [230, 30, 30]
                    };
                }
                // Rivers along the edges.
                for n in hex.neighbors() {
                    if !grid.has_river(hex, n) {
                        continue;
                    }
                    let axis = (n.to_world() - hex.to_world()).normalize();
                    let along = local.dot(axis);
                    let across = local.dot(Vec2::new(-axis.y, axis.x)).abs();
                    if along > sqrt3 / 2.0 - 0.14 && across < 0.55 {
                        color = [40, 120, 255];
                    }
                }
                // River sources (green) and mouths (orange).
                for &(at, mark) in marks {
                    if (p - at).length() < 0.3 {
                        color = mark;
                    }
                }
                // Hex outlines, faintly.
                if r > 0.97 {
                    color = shade(color, 0.85);
                }
            }
            rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    let file = File::create(path).unwrap();
    let mut encoder = png::Encoder::new(BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().unwrap();
    writer.write_image_data(&rgba).unwrap();
    writer.finish().unwrap();
}

/// A mountain cluster's size, its length (the most hexes apart two of its
/// hexes are, plus one) and its thickness (its size over its length: 1 for a
/// single file of hexes).
pub fn cluster_shape(cluster: &[Hex]) -> (usize, i32, f32) {
    let length = cluster
        .iter()
        .flat_map(|a| cluster.iter().map(move |b| a.distance(*b)))
        .max()
        .unwrap_or(0)
        + 1;
    (cluster.len(), length, cluster.len() as f32 / length as f32)
}

/// Mountains with four or more mountains around them: a range one or two
/// hexes wide has next to none, a blob has many.
pub fn crowded_mountains(grid: &HexGrid) -> usize {
    grid.all_hexes()
        .filter(|&h| grid.terrain(h) == Terrain::Mountains)
        .filter(|h| {
            h.neighbors()
                .iter()
                .filter(|n| grid.contains(**n) && grid.terrain(**n) == Terrain::Mountains)
                .count()
                >= 4
        })
        .count()
}

/// The highest over the lowest of `values`.
fn spread(values: impl Iterator<Item = i32>) -> f32 {
    let (lo, hi) = values.fold((i32::MAX, i32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    hi as f32 / lo.max(1) as f32
}

/// The spacings `map_stats` packs city sites at. The rules allow cities
/// `MIN_CITY_DISTANCE` (6) apart, but founded cities mostly stand 7-8 apart,
/// so their land doesn't overlap much.
const SITE_SPACINGS: [i32; 2] = [7, 8];
/// A decent city site scores at least this share of its map's mean start
/// (`start_score`: food, production and fresh water within two hexes).
const DECENT_SITE_SHARE: f32 = 0.75;

/// How many cities fit on the land the starts can walk to (`reach`), each
/// on a decent site and at least `spacing` from every other: the starts
/// first, then the best sites greedily. Any site the rules allow counts
/// (`founding_issue`: open land, no ruins or den on it).
pub fn city_sites(map: &GeneratedMap, reach: &HashMap<Hex, i32>, spacing: i32) -> usize {
    let grid = &map.grid;
    let mean_start = map
        .starts
        .iter()
        .map(|&s| start_score(grid, s))
        .sum::<i32>() as f32
        / map.starts.len() as f32;
    let mut sites: Vec<(i32, Hex)> = reach
        .keys()
        .copied()
        .filter(|h| !map.ruins.contains(h) && !map.dens.contains(h))
        .map(|h| (start_score(grid, h), h))
        .filter(|&(score, _)| score as f32 >= DECENT_SITE_SHARE * mean_start)
        .collect();
    sites.sort_by_key(|&(score, h)| (std::cmp::Reverse(score), h.q, h.r));
    let mut cities = map.starts.clone();
    for (_, h) in sites {
        if cities.iter().all(|c| c.distance(h) >= spacing) {
            cities.push(h);
        }
    }
    cities.len()
}

/// Origins per map for the mountains' detour (`mountain_detours`).
const DETOUR_ORIGINS: usize = 24;
/// Pairs nearer than this, in hexes, aren't counted in the detour.
const DETOUR_MIN_DISTANCE: i32 = 8;

/// How much farther mountains make walking across the land the starts can
/// walk to (`reach`): for pairs of hexes at least `DETOUR_MIN_DISTANCE`
/// apart, from `DETOUR_ORIGINS` origins spread over it, the steps on foot
/// over the steps if mountains could be crossed (water still can't).
fn mountain_detours(grid: &HexGrid, reach: &HashMap<Hex, i32>) -> Vec<f32> {
    let mut land: Vec<Hex> = reach.keys().copied().collect();
    land.sort_by_key(|h| (h.q, h.r));
    let over_mountains = |origin: Hex| {
        let mut steps = HashMap::from_iter([(origin, 0)]);
        let mut queue = VecDeque::from([origin]);
        while let Some(hex) = queue.pop_front() {
            let next = steps[&hex] + 1;
            for n in hex.neighbors() {
                if grid.contains(n) && !grid.terrain(n).is_water() && !steps.contains_key(&n) {
                    steps.insert(n, next);
                    queue.push_back(n);
                }
            }
        }
        steps
    };
    let mut ratios = Vec::new();
    for i in 0..DETOUR_ORIGINS {
        let origin = land[i * land.len() / DETOUR_ORIGINS];
        let (walk, over) = (walking_distances(grid, origin), over_mountains(origin));
        for (&h, &steps) in &walk {
            if h.distance(origin) >= DETOUR_MIN_DISTANCE {
                ratios.push(steps as f32 / over[&h] as f32);
            }
        }
    }
    ratios
}

#[test]
#[ignore = "prints measurements over many seeds"]
fn map_stats() {
    let seeds = seeds("0..50");
    let mut shares: HashMap<String, usize> = HashMap::default();
    let (mut land, mut mountains, mut hills, mut cut_off) = (0usize, 0usize, 0usize, 0usize);
    let (mut crowded, mut lakes) = (0usize, 0usize);
    let mut clusters: Vec<(usize, i32, f32)> = Vec::new();
    let (mut river_edges, mut maps) = (0usize, 0usize);
    let (mut river_count, mut river_length, mut river_longest) = (0usize, 0usize, 0usize);
    let (mut from_lakes, mut to_sea, mut to_lake) = (0usize, 0usize, 0usize);
    let (mut nearest, mut scores, mut yields) = (Vec::new(), Vec::new(), Vec::new());
    let mut generating = std::time::Duration::ZERO;
    let (mut land_per_side, mut sites, mut detours) = (Vec::new(), Vec::new(), Vec::new());
    let (mut range_lengths, mut longest_ranges) = (Vec::new(), Vec::new());
    let mut map_hexes = Vec::new();

    for &seed in &seeds {
        let started = std::time::Instant::now();
        let (map, rivers) = generate_with_rivers(seed, default_sides(seed));
        generating += started.elapsed();
        let grid = &map.grid;
        maps += 1;
        let all: Vec<Hex> = grid.all_hexes().collect();
        for &h in &all {
            let tile = grid.tile(h);
            lakes += usize::from(tile.terrain == Terrain::Lake);
            if tile.terrain.is_water() {
                continue;
            }
            land += 1;
            mountains += usize::from(tile.terrain == Terrain::Mountains);
            hills += usize::from(tile.hills);
            let mut key = format!("{:?}", tile.terrain);
            if let Some(f) = tile.feature {
                key = format!("{key}+{f:?}");
            }
            *shares.entry(key).or_default() += 1;
        }
        // Open land on the continent that the starts can't walk to.
        let continent = components(&all, |h| !grid.terrain(h).is_water())
            .into_iter()
            .max_by_key(Vec::len)
            .unwrap();
        let reach = walking_distances(grid, map.starts[0]);
        let sides = map.starts.len() as f32;
        map_hexes.push(all.len() as f32 / sides);
        land_per_side.push(reach.len() as f32 / sides);
        sites.push(SITE_SPACINGS.map(|spacing| city_sites(&map, &reach, spacing) as f32 / sides));
        detours.extend(mountain_detours(grid, &reach));

        cut_off += continent
            .iter()
            .filter(|&&h| grid.is_passable(h) && !reach.contains_key(&h))
            .count();
        for cluster in components(&all, |h| grid.terrain(h) == Terrain::Mountains) {
            clusters.push(cluster_shape(&cluster));
        }
        let lengths: Vec<i32> = components(&all, |h| grid.terrain(h) == Terrain::Mountains)
            .iter()
            .map(|c| cluster_shape(c))
            .filter(|c| c.0 >= 4)
            .map(|c| c.1)
            .collect();
        longest_ranges.push(lengths.iter().copied().max().unwrap_or(0) as f32);
        range_lengths.extend(lengths.iter().map(|&l| l as f32));

        crowded += crowded_mountains(grid);
        river_edges += grid.rivers().count();
        for river in rivers {
            let touches = |c: &Corner, what: fn(Terrain) -> bool| {
                c.iter().any(|&h| grid.contains(h) && what(grid.terrain(h)))
            };
            let end = river.last().unwrap();
            river_count += 1;
            river_length += river.len() - 1;
            river_longest = river_longest.max(river.len() - 1);
            from_lakes += usize::from(touches(&river[0], |t| t == Terrain::Lake));
            to_sea += usize::from(touches(end, |t| {
                matches!(t, Terrain::Ocean | Terrain::Coast)
            }));
            to_lake += usize::from(
                !touches(end, |t| matches!(t, Terrain::Ocean | Terrain::Coast))
                    && touches(end, |t| t == Terrain::Lake),
            );
        }
        nearest.push(walking_spread(grid, &map.starts));
        scores.push(spread(map.starts.iter().map(|&s| start_score(grid, s))));
        yields.push(spread(map.starts.iter().map(|&s| {
            within(s, 2)
                .filter(|h| grid.contains(*h))
                .map(|h| {
                    let (f, p) = grid.tile(h).yields();
                    f + p
                })
                .sum()
        })));
    }
    let per_map = generating.as_secs_f32() * 1000.0 / maps as f32;
    println!("{maps} maps, {per_map:.1} ms each to generate");
    let pct = |n: usize| 100.0 * n as f32 / land as f32;
    println!(
        "land: mountains {:.1}%, hills {:.1}%, open land on the continent cut off {:.2}%; \
         {:.1} lake hexes per map",
        pct(mountains),
        pct(hills),
        pct(cut_off),
        lakes as f32 / maps as f32
    );
    let mut shares: Vec<_> = shares.into_iter().collect();
    shares.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    for (k, n) in shares {
        println!("  {k:<20} {:5.1}%", pct(n));
    }
    clusters.sort_by_key(|c| std::cmp::Reverse(c.0));
    let singles = clusters.iter().filter(|c| c.0 == 1).count();
    let big: Vec<_> = clusters.iter().filter(|c| c.0 >= 4).collect();
    let thickness = big.iter().map(|c| c.2).sum::<f32>() / big.len().max(1) as f32;
    let in_big = big.iter().map(|c| c.0).sum::<usize>();
    println!(
        "mountain clusters: {:.1} per map, {singles} single peaks in all; clusters of 4+ hold \
         {:.0}% of mountains, mean thickness {thickness:.2}; mountains with 4+ mountain \
         neighbors: {:.1}%",
        clusters.len() as f32 / maps as f32,
        100.0 * in_big as f32 / mountains as f32,
        100.0 * crowded as f32 / mountains as f32,
    );
    println!(
        "largest clusters (size, length, thickness): {:?}",
        &clusters[..clusters.len().min(8)]
    );
    println!(
        "rivers: {:.1} per map, {:.1} edges per map; length mean {:.1}, longest {}; \
         rising at lakes {}, in highland {}; ending at the sea {}, in a lake {}, joining \
         another {}",
        river_count as f32 / maps as f32,
        river_edges as f32 / maps as f32,
        river_length as f32 / river_count.max(1) as f32,
        river_longest,
        from_lakes,
        river_count - from_lakes,
        to_sea,
        to_lake,
        river_count - to_sea - to_lake,
    );
    let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len() as f32;
    let worst = |v: &[f32]| v.iter().copied().fold(0.0, f32::max);
    for (what, v) in [
        ("nearest start on foot", &nearest),
        ("start_score", &scores),
        ("food + production within 2", &yields),
    ] {
        println!(
            "starts, {what}, highest/lowest per map: mean {:.2}, worst {:.2}",
            mean(v),
            worst(v)
        );
    }
    let least = |v: &[f32]| v.iter().copied().fold(f32::MAX, f32::min);
    let sizes: Vec<String> = (3..=7)
        .map(|sides| match world_shape(sides) {
            Shape::Rectangle { cols, rows } => {
                format!("{sides}: {}x{}", 2 * cols + 1, 2 * rows + 1)
            }
            shape => format!("{sides}: {shape:?}"),
        })
        .collect();
    println!("world size by sides (columns x rows): {}", sizes.join(", "));
    println!(
        "per side: {:.0} hexes of map, {:.0} of land the starts can walk to (least {:.0})",
        mean(&map_hexes),
        mean(&land_per_side),
        least(&land_per_side),
    );
    for (i, spacing) in SITE_SPACINGS.iter().enumerate() {
        let per_side: Vec<f32> = sites.iter().map(|s| s[i]).collect();
        println!(
            "decent city sites per side, {spacing} apart: mean {:.1}, least {:.1}, most {:.1}",
            mean(&per_side),
            least(&per_side),
            worst(&per_side),
        );
    }
    println!(
        "ranges (clusters of 4+): length mean {:.1}; longest per map mean {:.1}, most {}",
        mean(&range_lengths),
        mean(&longest_ranges),
        worst(&longest_ranges),
    );
    let longer = detours.iter().filter(|&&r| r >= 1.25).count();
    println!(
        "walking detour from mountains ({DETOUR_MIN_DISTANCE}+ hexes apart): mean {:.3}, \
         {:.1}% of pairs 25%+ farther, worst {:.2}",
        mean(&detours),
        100.0 * longer as f32 / detours.len() as f32,
        worst(&detours),
    );
}
