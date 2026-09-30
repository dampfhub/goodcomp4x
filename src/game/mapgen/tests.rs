use super::preview::{city_sites, cluster_shape, crowded_mountains, default_sides};
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
    assert_eq!(a.dens, b.dens);
    assert!(!a.dens.is_empty());
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
                let own = within(start, 5).find(|&h| {
                    map.grid.resource(h) == Some(resource)
                        && map
                            .starts
                            .iter()
                            .enumerate()
                            .all(|(j, other)| j == i || h.distance(*other) > h.distance(start))
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

/// Each side's share of the land has room for its cities: on average about
/// `CITY_SITES_PER_SIDE` decent sites 8 hexes apart, and never much fewer.
#[test]
fn every_side_has_room_for_several_cities() {
    let seeds = 0..8;
    let mut per_side = Vec::new();
    for seed in seeds.clone() {
        let map = generate(seed, default_sides(seed));
        let reach = walking_distances(&map.grid, map.starts[0]);
        let sites = city_sites(&map, &reach, 8) as f32 / map.starts.len() as f32;
        assert!(sites >= 5.0, "seed {seed}: {sites} city sites a side");
        per_side.push(sites);
    }
    let mean = per_side.iter().sum::<f32>() / per_side.len() as f32;
    assert!(
        (mean - CITY_SITES_PER_SIDE).abs() < 1.0,
        "{mean} city sites a side on average"
    );
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

/// Counts over several maps, for the rules that hold on the whole rather
/// than on every map.
#[derive(Default)]
struct Totals {
    rivers: usize,
    /// Edges the rivers run along, and the straight distance from each
    /// source to its mouth.
    river_edges: usize,
    river_reach: f32,
    mountains: usize,
    /// Mountains in clusters of four or more.
    in_ranges: usize,
    /// Mountains with four or more mountains around them.
    crowded: usize,
    /// Dens placed, and the most there could have been.
    dens: usize,
    den_room: usize,
}

/// Checks the rules every generated world keeps (rivers, mountains,
/// starts) on the maps of `seeds`, each with as many sides as the world
/// scenario gives it by default. Split over a few tests so they run side by
/// side.
fn check_worlds(seeds: std::ops::Range<u32>) {
    let mut totals = Totals::default();
    let maps = seeds.len();
    for seed in seeds {
        let (map, rivers) = generate_with_rivers(seed, default_sides(seed));
        check_rivers(seed, &map, &rivers, &mut totals);
        check_mountains(seed, &map, &mut totals);
        check_starts(seed, &map);
        check_dens(seed, &map, &mut totals);
    }
    assert!(
        totals.rivers >= 40 * maps,
        "rivers should be common: {} in {maps} maps",
        totals.rivers
    );
    // Mostly a steady course: not much longer than the straight line.
    let winding = totals.river_edges as f32 / totals.river_reach;
    assert!(
        winding < 1.6,
        "rivers wind {winding} times the straight line"
    );
    let Totals {
        mountains,
        in_ranges,
        crowded,
        dens,
        den_room,
        ..
    } = totals;
    assert!(
        in_ranges as f32 > 0.65 * mountains as f32,
        "only {in_ranges} of {mountains} mountains in ranges of four or more"
    );
    assert!(
        (crowded as f32) < 0.02 * mountains as f32,
        "{crowded} of {mountains} mountains are in blobs"
    );
    // Nearly always as many as the most a world takes.
    assert!(
        dens as f32 >= 0.95 * den_room as f32,
        "only {dens} of {den_room} dens found room"
    );
}

#[test]
fn worlds_keep_the_rules_seeds_0_to_19() {
    check_worlds(0..20);
}

#[test]
fn worlds_keep_the_rules_seeds_20_to_39() {
    check_worlds(20..40);
}

#[test]
fn worlds_keep_the_rules_seeds_40_to_59() {
    check_worlds(40..60);
}

/// Every river rises at a lake or in the mountains or their foothills, runs
/// downhill along hex edges to the sea, a lake, or another river, merging but
/// never splitting, at a length that suits the map, and never runs back into
/// the lake it left, directly or through other rivers and lakes. At most one
/// river leaves each lake.
fn check_rivers(seed: u32, map: &GeneratedMap, rivers: &[River], totals: &mut Totals) {
    let grid = &map.grid;
    let all: Vec<Hex> = grid.all_hexes().collect();
    assert_eq!(
        river_edges(rivers),
        grid.rivers().collect::<HashSet<_>>(),
        "seed {seed}: the map's rivers are these courses"
    );
    let mut lake_of: HashMap<Hex, usize> = HashMap::default();
    for (i, lake) in components(&all, |h| grid.terrain(h) == Terrain::Lake)
        .into_iter()
        .enumerate()
    {
        lake_of.extend(lake.into_iter().map(|h| (h, i)));
    }
    let lakes_at = |c: &Corner| -> Vec<usize> {
        let mut lakes: Vec<usize> = c.iter().filter_map(|h| lake_of.get(h).copied()).collect();
        lakes.sort_unstable();
        lakes.dedup();
        lakes
    };
    let is_sea =
        |h: Hex| grid.contains(h) && matches!(grid.terrain(h), Terrain::Ocean | Terrain::Coast);
    let extent = all
        .iter()
        .fold(Vec2::ZERO, |m, h| m.max(h.to_world().abs()));
    let center = |c: &Corner| c.iter().map(|h| h.to_world()).sum::<Vec2>() / 3.0;
    let highland = |h: Hex| {
        let tile = grid.tile(h);
        tile.terrain == Terrain::Mountains
            || (tile.hills
                && h.neighbors()
                    .iter()
                    .any(|n| grid.contains(*n) && grid.terrain(*n) == Terrain::Mountains))
    };
    let mut flows_on: HashMap<Corner, usize> = HashMap::default();
    let mut outflow: HashMap<usize, usize> = HashMap::default();
    for (i, river) in rivers.iter().enumerate() {
        let length = river.len() - 1;
        assert!(
            (3..=longest_river(extent)).contains(&length),
            "seed {seed}: a river {length} edges long"
        );
        for pair in river.windows(2) {
            assert!(
                next_corners(pair[0]).contains(&pair[1]),
                "seed {seed}: a gap"
            );
            let (a, b) = shared_edge(pair[0], pair[1]);
            assert!(!grid.terrain(a).is_water() && !grid.terrain(b).is_water());
        }
        for &c in &river[..length] {
            let earlier = flows_on.insert(c, i);
            assert!(earlier.is_none(), "seed {seed}: rivers split at {c:?}");
        }
        let source = river[0];
        let source_lakes = lakes_at(&source);
        assert!(
            !source_lakes.is_empty() || source.iter().any(|&h| highland(h)),
            "seed {seed}: a river rising at {source:?}, by no lake or mountain"
        );
        assert!(source_lakes.len() <= 1);
        if let Some(&lake) = source_lakes.first() {
            let other = outflow.insert(lake, i);
            assert!(other.is_none(), "seed {seed}: two rivers leave lake {lake}");
        }
        totals.rivers += 1;
        totals.river_edges += length;
        totals.river_reach += center(&source).distance(center(&river[length]));
    }
    let mouth = |i: usize| -> Mouth {
        let end = *rivers[i].last().unwrap();
        if end.iter().any(|&h| is_sea(h)) {
            return Mouth::Sea;
        }
        let source_lakes = lakes_at(&rivers[i][0]);
        if let Some(&lake) = lakes_at(&end).iter().find(|l| !source_lakes.contains(l)) {
            return Mouth::Lake(lake);
        }
        let joined = flows_on.get(&end).copied().filter(|&j| j != i);
        Mouth::River(joined.unwrap_or_else(|| {
            panic!("seed {seed}: river {i} ends at {end:?}, by no sea, lake or river")
        }))
    };
    for (i, river) in rivers.iter().enumerate() {
        let mut next = mouth(i);
        let Some(&lake) = lakes_at(&river[0]).first() else {
            continue;
        };
        for _ in 0..=rivers.len() {
            next = match next {
                Mouth::Sea => break,
                Mouth::Lake(l) => {
                    assert_ne!(l, lake, "seed {seed}: river {i} runs back into its lake");
                    match outflow.get(&l) {
                        Some(&j) => mouth(j),
                        None => break,
                    }
                }
                Mouth::River(j) => mouth(j),
            };
        }
        assert!(
            matches!(next, Mouth::Sea | Mouth::Lake(_)),
            "seed {seed}: a loop"
        );
    }
}

/// Mountains are a small share of the land and come in ranges: long, thin
/// chains rather than blobs.
fn check_mountains(seed: u32, map: &GeneratedMap, totals: &mut Totals) {
    let grid = &map.grid;
    let all: Vec<Hex> = grid.all_hexes().collect();
    let land = all.iter().filter(|&&h| !grid.terrain(h).is_water()).count();
    let mountains = all
        .iter()
        .filter(|&&h| grid.terrain(h) == Terrain::Mountains)
        .count();
    let share = mountains as f32 / land as f32;
    assert!(
        (0.025..0.06).contains(&share),
        "seed {seed}: mountains are {share} of the land"
    );
    for cluster in components(&all, |h| grid.terrain(h) == Terrain::Mountains) {
        let (size, length, thickness) = cluster_shape(&cluster);
        assert!(
            thickness < 2.5,
            "seed {seed}: a lump of {size} mountains {length} long"
        );
        if size >= 4 {
            totals.in_ranges += size;
        }
    }
    totals.mountains += mountains;
    totals.crowded += crowded_mountains(grid);
}

/// Every start can walk to every other, the nearest neighbors on foot are
/// about as far for all, and the land around each is about as good.
fn check_starts(seed: u32, map: &GeneratedMap) {
    let reach = walking_distances(&map.grid, map.starts[0]);
    for start in &map.starts {
        assert!(
            reach.contains_key(start),
            "seed {seed}: a start the others can't walk to"
        );
    }
    let spread = walking_spread(&map.grid, &map.starts);
    assert!(
        spread <= 2.0,
        "seed {seed}: nearest neighbors on foot {spread} times as far for one start as another"
    );
    let scores: Vec<i32> = map
        .starts
        .iter()
        .map(|&s| start_score(&map.grid, s))
        .collect();
    let (lo, hi) = (scores.iter().min().unwrap(), scores.iter().max().unwrap());
    assert!(
        *hi as f32 <= 1.35 * *lo as f32,
        "seed {seed}: start scores {scores:?}"
    );
}

/// Animal dens keep their rules: on forest, jungle or hills every start can
/// walk to, `DEN_START_DISTANCE` from every start, off the edge, clear of
/// resources, special tiles and ruins, `DEN_GAP` apart; at least one a side,
/// for the fewest a world takes. Counts them into `totals`.
fn check_dens(seed: u32, map: &GeneratedMap, totals: &mut Totals) {
    let grid = &map.grid;
    let reach = walking_distances(grid, map.starts[0]);
    assert!(
        map.dens.len() >= map.starts.len(),
        "seed {seed}: {} dens for {} sides",
        map.dens.len(),
        map.starts.len()
    );
    assert!(map.dens.len() <= MAX_DENS_PER_SIDE * map.starts.len());
    for (i, &den) in map.dens.iter().enumerate() {
        let tile = grid.tile(den);
        assert!(
            grid.is_passable(den) && (tile.hills || tile.feature.is_some()),
            "seed {seed}: a den on {tile:?}"
        );
        assert!(reach.contains_key(&den), "seed {seed}: a den out of reach");
        assert!(
            grid.edge_distance(den) >= 1,
            "seed {seed}: a den on the edge"
        );
        assert!(grid.resource(den).is_none() && grid.special(den).is_none());
        assert!(map.ruins.iter().all(|r| r.distance(den) >= 2));
        for start in &map.starts {
            assert!(
                start.distance(den) >= DEN_START_DISTANCE,
                "seed {seed}: a den {} from a start",
                start.distance(den)
            );
        }
        for other in &map.dens[..i] {
            assert!(other.distance(den) >= DEN_GAP, "seed {seed}: dens crowd");
        }
    }
    totals.dens += map.dens.len();
    totals.den_room += MAX_DENS_PER_SIDE * map.starts.len();
}
/// A hash of everything a map holds, the same on every machine and build.
fn golden_hash(map: &GeneratedMap) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut add = |word: i64| {
        for byte in word.to_le_bytes() {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3);
        }
    };
    let grid = &map.grid;
    for h in grid.all_hexes() {
        let tile = grid.tile(h);
        add(i64::from(h.q));
        add(i64::from(h.r));
        add(tile.terrain as i64);
        add(i64::from(tile.hills));
        add(tile.feature.map_or(-1, |f| f as i64));
        add(grid.resource(h).map_or(-1, |r| r as i64));
        add(grid.special(h).map_or(-1, |s| s as i64));
    }
    let mut rivers: Vec<(Hex, Hex)> = grid.rivers().collect();
    rivers.sort_by_key(|(a, b)| (a.q, a.r, b.q, b.r));
    for (a, b) in rivers {
        for v in [a.q, a.r, b.q, b.r] {
            add(i64::from(v));
        }
    }
    for h in map.starts.iter().chain(&map.ruins).chain(&map.dens) {
        add(i64::from(h.q));
        add(i64::from(h.r));
    }
    hash
}

/// Pins what two seeds generate, so a change to any stage shows here and is
/// made on purpose (a network game's players must build the same map, so
/// such a change also bumps `PROTOCOL_VERSION`), and a refactor that should
/// change nothing can prove it.
#[test]
fn golden_maps_stay_the_same() {
    let hashes = [golden_hash(&generate(7, 5)), golden_hash(&generate(42, 7))];
    assert_eq!(
        hashes,
        [1683703251667493817, 9046025897880983391],
        "the maps changed: if on purpose, update the hashes and bump PROTOCOL_VERSION"
    );
}

/// A river leaving a lake mustn't drain back into it, however far round:
/// straight in, down a river it joins, or out through another lake.
#[test]
fn drains_into_follows_rivers_and_lakes_downstream() {
    // River 0 runs to the sea; river 1 leaves lake 1 for lake 0; river 2
    // joins river 1.
    let mouths = [Mouth::Sea, Mouth::Lake(0), Mouth::River(1)];
    let outflow: HashMap<usize, usize> = HashMap::from_iter([(1, 1)]);
    let drains = |mouth, lake| drains_into(mouth, lake, &mouths, &outflow);
    assert!(drains(Mouth::Lake(1), 1), "straight in");
    assert!(drains(Mouth::River(2), 0), "down two rivers");
    assert!(drains(Mouth::Lake(1), 0), "out through lake 1");
    assert!(!drains(Mouth::River(0), 0), "to the sea");
    assert!(!drains(Mouth::Lake(0), 1), "lake 0 has no outflow");
    assert!(!drains(Mouth::Sea, 0));
}
