use super::barracks::{CITY_TRAINING_SLOWDOWN, UNITS_PER_DEPOSIT};
use super::economy::{WORK_PER_TURN, grow_price};
use super::*;
use crate::game::workers::{JobKind, WorkerJob};

/// The builds in a queue, in order.
fn builds<B: Copy>(queue: &[Queued<B>]) -> Vec<B> {
    queue.iter().map(|q| q.build).collect()
}

#[test]
fn roads_improve_delivery_and_enemy_occupation_blocks_the_site() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    let tile = Hex::new(-1, 0);
    assert_eq!(g.routes(0).costs[&tile], 6);
    for q in -3..=-1 {
        g.roads.insert(Hex::new(q, 0));
    }
    assert_eq!(g.routes(0).costs[&tile], 3);
    g.units.push(super::super::unit::Unit::new(
        100,
        tile,
        Team::Red,
        super::super::unit::UnitType::Melee,
    ));
    assert!(!g.routes(0).costs.contains_key(&tile));
}

/// A lone Blue city on plain ground at the origin of a radius-6 map where
/// every other hex is `tile(hex)`: no units, roads, sites or other cities.
fn lone_city_on(tile: impl Fn(Hex) -> Tile) -> GameState {
    use crate::game::hex::HexGrid;
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    g.sites.clear();
    let origin = Hex::new(0, 0);
    g.cities = vec![City::new(0, Team::Blue, origin)];
    let hexes: Vec<Hex> = HexGrid::new(6, [(origin, Tile::default())])
        .all_hexes()
        .collect();
    g.grid = HexGrid::new(
        6,
        hexes.into_iter().map(|h| {
            if h == origin {
                (h, Tile::default())
            } else {
                (h, tile(h))
            }
        }),
    );
    g
}

/// #198: a tile's share depends on the hexes its goods travel, never on the
/// terrain: every tile beside a city delivers 100%, forested hills, snow
/// and marsh included.
#[test]
fn every_tile_beside_a_city_delivers_everything_whatever_its_terrain() {
    use crate::game::terrain::{Feature, Terrain};
    let rough = [
        Tile {
            terrain: Terrain::Plains,
            hills: true,
            feature: Some(Feature::Forest),
        },
        Tile {
            terrain: Terrain::Grassland,
            hills: true,
            feature: None,
        },
        Tile {
            terrain: Terrain::Snow,
            hills: true,
            feature: None,
        },
        Tile {
            terrain: Terrain::Marsh,
            hills: false,
            feature: Some(Feature::Jungle),
        },
        Tile {
            terrain: Terrain::Tundra,
            hills: false,
            feature: Some(Feature::Forest),
        },
        Terrain::Desert.into(),
    ];
    let around: Vec<Hex> = Hex::new(0, 0).neighbors().into_iter().collect();
    let g = lone_city_on(|h| {
        around
            .iter()
            .position(|&n| n == h)
            .map_or(Tile::default(), |i| rough[i])
    });
    let routes = g.routes(0);
    for (hex, tile) in around.iter().zip(rough) {
        let cost = routes.costs[hex];
        assert_eq!(
            delivered_share(cost),
            4,
            "{} beside the city costs {cost}",
            tile.name()
        );
    }

    // The city's income agrees: all of a forested-hills tile's goods arrive.
    let mut g = g;
    let forested_hills = around[0];
    g.cities[0].clusters = one_cluster(&[forested_hills]);
    let (food, wood, metal) = g.tile_goods(forested_hills);
    assert!(wood + metal > 0);
    assert_eq!(
        g.income(0),
        Stock {
            food: 8 + food * 4,
            wood: 4 + wood * 4,
            metal: metal * 4,
        }
    );
}

/// #198: shares fall off by hexes travelled, 100/75/50/25% at one to four
/// hexes, the same over rough ground as over open ground; five hexes out
/// is beyond reach.
#[test]
fn shares_fall_off_by_hexes_travelled_not_terrain() {
    use crate::game::terrain::{Feature, Terrain};
    let forested_hills = Tile {
        terrain: Terrain::Plains,
        hills: true,
        feature: Some(Feature::Forest),
    };
    for g in [
        lone_city_on(|_| Tile::default()),
        lone_city_on(|_| forested_hills),
        lone_city_on(|_| Terrain::Marsh.into()),
    ] {
        let routes = g.routes(0);
        let shares: Vec<Option<i32>> = (1..=5)
            .map(|q| {
                routes
                    .costs
                    .get(&Hex::new(q, 0))
                    .map(|&cost| delivered_share(cost) * 25)
            })
            .collect();
        assert_eq!(
            shares,
            [Some(100), Some(75), Some(50), Some(25), None],
            "on {}",
            g.grid.tile(Hex::new(1, 0)).name()
        );
    }
}

/// #198: a road step counts as half a hex, so a road extends a city's reach
/// over any ground: two hexes out along a road deliver like one hex out.
#[test]
fn a_road_step_counts_as_half_a_hex() {
    use crate::game::terrain::Terrain;
    let mut g = lone_city_on(|_| Tile {
        terrain: Terrain::Snow,
        hills: true,
        feature: None,
    });
    for q in 1..=4 {
        g.roads.insert(Hex::new(q, 0));
    }
    let routes = g.routes(0);
    let share = |q: i32, r: i32| {
        routes
            .costs
            .get(&Hex::new(q, r))
            .map(|&cost| delivered_share(cost) * 25)
    };
    // Along the road: half a hex a step.
    assert_eq!(share(2, 0), Some(100));
    assert_eq!(share(4, 0), Some(75));
    // Off its end, one hex more, and the road reaches past four hexes.
    assert_eq!(share(5, 0), Some(50));
    assert_eq!(share(6, 0), Some(25));
    // A tile beside the road: the road's steps, then one off-road hex.
    assert_eq!(share(2, -1), Some(75));
}

#[test]
fn canoe_house_turns_its_connected_river_into_a_transport_corridor() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    let bank = Hex::new(-3, 0);
    let middle = Hex::new(-2, 0);
    let destination = Hex::new(-1, 0);
    g.grid = g
        .grid
        .clone()
        .with_rivers(crate::game::fast_hash::HashSet::from_iter([
            crate::game::hex::edge(bank, middle),
            crate::game::hex::edge(middle, destination),
        ]));
    let before = g.routes(0).costs[&destination];
    assert!(g.site_available(0, Building::CanoeHouse, bank));
    assert!(!g.site_available(0, Building::CanoeHouse, Hex::new(-4, 1)));
    g.cities[0]
        .extra_buildings
        .insert(Building::CanoeHouse, bank);
    assert!(g.routes(0).costs[&destination] < before);
    assert_eq!(g.routes(0).costs[&destination], 4);
}

#[test]
fn forge_and_stable_unlock_and_upgrade_troops_at_an_off_resource_barracks() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let barracks = Hex::new(-3, 1);
    let horses = Hex::new(-2, 0);
    let iron = Hex::new(-2, 1);
    assert_eq!(g.grid.resource(barracks), None);
    assert!(g.site_available(0, Building::Stable, horses));
    assert!(g.site_available(0, Building::Forge, iron));
    g.cities[0].barracks = Some(barracks);
    assert!(!g.barracks_can_train(0, BuildUnit::Cavalry));
    assert!(!g.barracks_can_train(0, BuildUnit::Armored));
    g.cities[0].extra_buildings.insert(Building::Stable, horses);
    g.cities[0].extra_buildings.insert(Building::Forge, iron);
    assert!(g.barracks_can_train(0, BuildUnit::Cavalry));
    assert!(g.barracks_can_train(0, BuildUnit::Armored));
    g.cities[0].barracks_queue = vec![Queued::worked(
        BuildUnit::Cavalry,
        BuildUnit::Cavalry.work(),
    )];
    g.complete_builds();
    let cavalry = g
        .units
        .iter()
        .find(|u| u.unit_type == UnitType::Cavalry)
        .unwrap();
    assert_eq!(cavalry.training_upgrade, Some(Resource::Horses));
    assert_eq!(cavalry.stats().move_range, 3);
    g.units.clear();
    g.cities[0].barracks_queue = vec![Queued::worked(
        BuildUnit::Armored,
        BuildUnit::Armored.work(),
    )];
    g.complete_builds();
    let armored = g
        .units
        .iter()
        .find(|u| u.unit_type == UnitType::Armored)
        .unwrap();
    assert_eq!(armored.training_upgrade, Some(Resource::Iron));
    assert!(armored.max_hp() > UnitType::Armored.stats().max_hp);
}

#[test]
fn remote_cannery_collects_food_beyond_city_reach_but_not_through_an_enemy() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.sites.clear();
    g.cities[0].clusters.clear();
    let cannery = Hex::new(0, 0);
    let farm = Hex::new(1, 0);
    assert!(!g.routes(0).costs.contains_key(&farm));
    g.sites.insert(
        farm,
        Site {
            team: Team::Blue,
            food: 16,
            production: 0,
            label: "FARM",
        },
    );
    let before = g.income(0).food;
    g.cities[0]
        .extra_buildings
        .insert(Building::Cannery, cannery);
    assert_eq!(g.income(0).food - before, g.tile_yield(farm).0 * 4);
    g.units
        .push(Unit::new(900, farm, Team::Red, UnitType::Melee));
    assert_eq!(g.income(0).food, before);
}

#[test]
fn remote_smelter_collects_unworked_mines_beyond_city_reach() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fog_of_war = false;
    g.sites.clear();
    g.cities[0].clusters.clear();
    let smelter = Hex::new(0, 0);
    let mine = Hex::new(1, 0);
    assert!(g.site_available(0, Building::Smelter, smelter));
    assert!(!g.routes(0).costs.contains_key(&mine));
    g.sites.insert(
        mine,
        Site {
            team: Team::Blue,
            food: 0,
            production: 16,
            label: "MINE",
        },
    );
    let before = g.income(0).production();
    g.cities[0]
        .extra_buildings
        .insert(Building::Smelter, smelter);
    assert_eq!(g.income(0).production() - before, 64);
    g.units
        .push(Unit::new(900, mine, Team::Red, UnitType::Melee));
    assert_eq!(g.income(0).production(), before);
}

#[test]
fn a_road_connected_railhead_moves_a_city_troop_across_the_map_in_one_turn() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fog_of_war = false;
    g.roads.clear();
    let terminal = Hex::new(0, 0);
    for q in -3..=0 {
        g.roads.insert(Hex::new(q, 0));
    }
    g.cities[0]
        .extra_buildings
        .insert(Building::Railhead, terminal);
    g.units
        .push(Unit::new(900, Hex::new(-4, 1), Team::Blue, UnitType::Melee));
    assert!(g.rail_connected(0, None));
    g.try_queue_move(0, terminal);
    assert_eq!(g.units[0].planned_move, Some(terminal));
    g.resolve_step(UnitType::Melee, crate::game::turn::Phase::Move);
    assert_eq!(g.units[0].pos, terminal);
    g.units[0].pos = Hex::new(-4, 1);
    g.try_queue_move(0, terminal);
    g.units
        .push(Unit::new(901, Hex::new(-2, 0), Team::Red, UnitType::Melee));
    g.resolve_step(UnitType::Melee, crate::game::turn::Phase::Move);
    assert_eq!(g.units[0].pos, Hex::new(-4, 1));
    g.units.pop();
    g.roads.remove(&Hex::new(-2, 0));
    assert!(!g.rail_connected(0, None));
    g.try_queue_move(0, terminal);
    assert_eq!(g.units[0].planned_move, None);
}

#[test]
fn field_hospital_heals_two_nearby_survivors_in_both_layers() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let site = Hex::new(-3, 0);
    g.cities[0]
        .extra_buildings
        .insert(Building::FieldHospital, site);
    for (id, pos, hp) in [
        (10, Hex::new(-2, 0), 30.0),
        (11, Hex::new(-3, 1), 40.0),
        (12, Hex::new(-4, 1), 50.0),
    ] {
        let mut unit = Unit::new(id, pos, Team::Blue, UnitType::Melee);
        unit.hp = hp;
        unit.interior_hp = hp;
        g.units.push(unit);
    }
    g.resolve_economy();
    assert_eq!(g.units.iter().find(|u| u.id == 10).unwrap().hp, 50.0);
    assert_eq!(
        g.units.iter().find(|u| u.id == 11).unwrap().interior_hp,
        60.0
    );
    assert_eq!(g.units.iter().find(|u| u.id == 12).unwrap().hp, 50.0);
}

#[test]
fn hill_watchpost_reveals_distant_hexes() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let post = Hex::new(0, 0);
    let distant = Hex::new(5, 0);
    assert!(g.grid.tile(post).hills);
    assert!(!g.fog().sees(distant));
    g.cities[0]
        .extra_buildings
        .insert(Building::Watchpost, post);
    assert!(g.fog().sees(distant));
}
#[test]
fn economy_ticks_once_into_the_stockpile_and_preserves_quarters() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].clusters.clear();
    let tile = Hex::new(-1, 0);
    g.roads.clear();
    g.cities[0].clusters = one_cluster(&[tile]);
    // The center's 2 food and 1 wood, and half of the plains' 2 food and
    // 1 wood, which is a long haul away.
    assert_eq!(
        g.income(0),
        Stock {
            food: 12,
            wood: 6,
            metal: 0
        }
    );
    // The turn waits until the city has something to build, since the
    // stockpile could pay for something.
    g.end_planning();
    assert!(!g.is_resolving());
    g.queue_build(0, Build::Unit(BuildUnit::Siege));
    // A Siege costs more wood and metal than the scenario starts with.
    g.stockpiles[Team::Blue.index()] += BuildUnit::Siege.price();
    let before = g.stock(Team::Blue);
    let upkeep = g.upkeep(Team::Blue);
    g.end_planning();
    g.update(1.0);
    assert_eq!(
        g.cities[0].queue[0].progress, WORK_PER_TURN,
        "one turn of work"
    );
    // Income in, upkeep out, and the Siege paid for as work on it starts.
    let price = BuildUnit::Siege.price();
    assert_eq!(
        g.stock(Team::Blue),
        Stock {
            food: before.food + 12 - upkeep - price.food,
            wood: before.wood + 6 - price.wood,
            metal: before.metal - price.metal
        }
    );
    g.update(10.0);
    assert_eq!(
        g.cities[0].queue[0].progress, WORK_PER_TURN,
        "the economy ran once"
    );
}

#[test]
fn builds_are_paid_when_work_starts_and_refunded_only_if_paid() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let start = g.stock(Team::Blue);
    g.queue_selected_city_unit(BuildUnit::Melee);
    assert_eq!(builds(&g.cities[0].queue), [Build::Unit(BuildUnit::Melee)]);
    assert!(!g.cities[0].queue[0].paid);
    assert_eq!(g.stock(Team::Blue), start, "nothing is paid while queued");
    // Taken off unpaid, it refunds nothing.
    g.remove_selected_city_queue_item(0);
    assert_eq!(g.stock(Team::Blue), start);
    assert!(g.notice.contains("NOTHING TO REFUND"), "{}", g.notice);

    // The turn's economy pays for it as work on it starts, from the
    // stockpile with the turn's income in and the citizens fed.
    g.queue_selected_city_unit(BuildUnit::Melee);
    let expected = g.expected_stock(Team::Blue);
    let red = g.expected_stock(Team::Red);
    g.resolve_economy();
    assert!(g.cities[0].queue[0].paid);
    assert_eq!(g.cities[0].queue[0].progress, WORK_PER_TURN);
    assert_eq!(g.stock(Team::Blue), expected - BuildUnit::Melee.price());
    assert_eq!(g.stock(Team::Red), red, "only the buyer pays");
    // Taken off paid, it refunds its price; its work is lost.
    g.remove_selected_city_queue_item(0);
    assert_eq!(g.stock(Team::Blue), expected);
    assert!(g.notice.ends_with("- REFUNDED"), "{}", g.notice);

    // Short of wood, it's queued all the same, and waits unpaid; the notice
    // says what for.
    g.stockpiles[Team::Blue.index()] = Stock::whole(20, 0, 0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    let short = g
        .expected_stock(Team::Blue)
        .shortfall(BuildUnit::Melee.price());
    assert!(short.wood > 0 && short.food == 0);
    assert!(
        g.notice
            .contains(&format!("WAITS FOR {}", stock_icons(short))),
        "{}",
        g.notice
    );
    let expected = g.expected_stock(Team::Blue);
    g.resolve_economy();
    assert_eq!(
        g.cities[0].queue[0],
        Queued::new(Build::Unit(BuildUnit::Melee))
    );
    assert_eq!(g.stock(Team::Blue), expected, "nothing spent");

    // Broke, a city still has something to do: gather, for free.
    g.cities[0].queue.clear();
    g.stockpiles[Team::Blue.index()] = Stock::default();
    assert!(g.city_needs_build(0));
    g.queue_selected_city_gather();
    assert_eq!(builds(&g.cities[0].queue), [Build::Gather]);
    assert_eq!(g.stock(Team::Blue), Stock::default());
    assert!(!g.city_needs_build(0));
}

#[test]
fn a_queue_skips_what_it_cannot_pay_for_and_goes_back_once_it_can() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    // Wood for a Melee but no metal for the Siege ahead of it.
    g.stockpiles[Team::Blue.index()] = Stock::whole(20, 20, 0);
    let siege = Build::Unit(BuildUnit::Siege);
    let melee = Build::Unit(BuildUnit::Melee);
    g.queue_selected_city_unit(BuildUnit::Siege);
    assert!(g.notice.contains("WAITS FOR"), "{}", g.notice);
    g.queue_selected_city_unit(BuildUnit::Melee);
    let forecast = g.forecast(Team::Blue);
    let lane = forecast.lane(0, Lane::City).unwrap();
    assert_eq!(lane.worked, Some(1), "the forecast skips to the Melee");
    let short = g.waiting_items(lane);
    assert_eq!(short.len(), 1);
    assert_eq!(short[0].0, 0);
    assert!(short[0].1.metal > 0, "the Siege waits for metal");
    assert_eq!(g.city_waits_for(&forecast, 0), Some(short[0].1));

    let expected = g.expected_stock(Team::Blue);
    g.resolve_economy();
    assert_eq!(g.cities[0].queue[0], Queued::new(siege), "it waits, unpaid");
    assert_eq!(g.cities[0].queue[1], Queued::worked(melee, WORK_PER_TURN));
    assert_eq!(g.stock(Team::Blue), expected - BuildUnit::Melee.price());

    // Metal arrives: the Siege, first in the queue, is paid for and worked
    // the next turn, and the Melee keeps the work it had.
    g.stockpiles[Team::Blue.index()].metal += BuildUnit::Siege.price().metal;
    let expected = g.expected_stock(Team::Blue);
    g.resolve_economy();
    assert_eq!(g.cities[0].queue[0], Queued::worked(siege, WORK_PER_TURN));
    assert_eq!(g.cities[0].queue[1], Queued::worked(melee, WORK_PER_TURN));
    assert_eq!(g.stock(Team::Blue), expected - BuildUnit::Siege.price());
    assert_eq!(g.city_waits_for(&g.forecast(Team::Blue), 0), None);
}

#[test]
fn cities_waiting_on_one_stockpile_pay_in_city_order_then_queue_order() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities.push(City::new(2, Team::Blue, Hex::new(-2, -3)));
    let other = g.cities.len() - 1;
    // Nothing comes in, and enough wood for one Melee: city 0 pays first,
    // its city queue before its Barracks', and the others wait.
    for c in [0, other] {
        g.cities[c].clusters.clear();
        g.cities[c].population = 1;
    }
    let melee = Build::Unit(BuildUnit::Melee);
    g.cities[0].barracks = Some(Hex::new(-3, 1));
    g.queue_barracks(0, BuildUnit::Melee);
    g.queue_build(0, melee);
    g.queue_build(other, melee);
    let income = g.side_income(Team::Blue);
    g.stockpiles[Team::Blue.index()] = Stock {
        food: 80,
        wood: BuildUnit::Melee.price().wood - income.wood,
        metal: 0,
    };
    let forecast = g.forecast(Team::Blue);
    assert_eq!(forecast.lane(0, Lane::City).unwrap().worked, Some(0));
    assert_eq!(forecast.lane(0, Lane::Barracks).unwrap().worked, None);
    assert_eq!(forecast.lane(other, Lane::City).unwrap().worked, None);
    assert!(
        g.city_waits_for(&forecast, 0).is_some(),
        "its Barracks waits"
    );
    assert!(g.city_waits_for(&forecast, other).is_some());

    g.resolve_economy();
    assert!(g.cities[0].queue[0].paid);
    assert!(!g.cities[0].barracks_queue[0].paid);
    assert!(!g.cities[other].queue[0].paid);
    assert_eq!(g.stock(Team::Blue).wood, 0);

    // With wood for one more, the Barracks, next in order, pays.
    g.stockpiles[Team::Blue.index()].wood = BuildUnit::Melee.price().wood - income.wood;
    g.resolve_economy();
    assert!(g.cities[0].barracks_queue[0].paid);
    assert!(!g.cities[other].queue[0].paid, "still waiting");
}

#[test]
fn the_forecast_is_what_the_economy_then_does() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].barracks = Some(Hex::new(-3, 1));
    let builds = [
        Build::Unit(BuildUnit::Siege),
        Build::Grow,
        Build::Worker,
        Build::Unit(BuildUnit::Melee),
    ];
    // Nothing here finishes in one turn, so every item stays where it was.
    let troops = [BuildUnit::Ranged, BuildUnit::Siege, BuildUnit::Melee];
    for seed in 0..40_usize {
        let mut game = g.clone();
        for k in 0..(seed % 4 + 1) {
            game.queue_build(0, builds[(seed * 7 + k * 3) % builds.len()]);
            game.queue_barracks(0, troops[(seed + k) % troops.len()]);
        }
        let wood = (seed % 5) as i32 * 3;
        game.stockpiles[Team::Blue.index()] = Stock::whole(10, wood, (seed % 3) as i32 * 2);
        let forecast = game.forecast(Team::Blue);
        let before = (
            game.cities[0].queue.clone(),
            game.cities[0].barracks_queue.clone(),
        );
        game.resolve_economy();
        for lane in [Lane::City, Lane::Barracks] {
            let predicted = forecast.lane(0, lane).unwrap();
            let len = match lane {
                Lane::City => before.0.len(),
                Lane::Barracks => before.1.len(),
            };
            for index in 0..len {
                let paid = match lane {
                    Lane::City => game.cities[0].queue.get(index).map(|q| q.paid),
                    Lane::Barracks => game.cities[0].barracks_queue.get(index).map(|q| q.paid),
                };
                // The item worked is paid for (or finished and gone); the
                // ones ahead of it wait unpaid.
                if Some(index) == predicted.worked {
                    assert_ne!(paid, Some(false), "seed {seed} {lane:?} {index}");
                } else if predicted.worked.is_none_or(|worked| index < worked) {
                    assert_eq!(paid, Some(false), "seed {seed} {lane:?} {index}");
                }
            }
        }
        assert!(game.stock(Team::Blue).wood >= 0 && game.stock(Team::Blue).metal >= 0);
    }
}

#[test]
fn the_ai_queues_only_what_it_can_pay_for_this_turn_and_never_waits() {
    let mut g = GameState::city_scenario();
    let red = g.cities.iter().position(|c| c.team == Team::Red).unwrap();
    // Broke, with a Siege it can't pay for waiting at the head: the AI
    // takes it off (unpaid, so it costs nothing) and plans again.
    g.stockpiles[Team::Red.index()] = Stock::default();
    g.cities[red].queue = vec![Queued::new(Build::Unit(BuildUnit::Siege))];
    g.plan_ai_turn(Team::Red);
    let forecast = g.forecast(Team::Red);
    for lane in [Lane::City, Lane::Barracks] {
        let lane = forecast.lane(red, lane).unwrap();
        assert!(g.lane_len(red, lane.lane) <= 1, "one item a queue");
        assert!(g.waiting_items(lane).is_empty(), "{:?} waits", lane.lane);
    }
    assert_eq!(g.cities[red].queue.len(), 1, "it found something to do");
    assert_eq!(g.stock(Team::Red), Stock::default(), "nothing paid yet");
    // Planning again adds nothing to a queue that's busy.
    let queue = g.cities[red].queue.clone();
    g.plan_ai_turn(Team::Red);
    assert_eq!(g.cities[red].queue, queue);
}

#[test]
fn a_build_takes_its_fixed_turns_whatever_the_city_produces() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let siege = Build::Unit(BuildUnit::Siege);
    g.cities[0].queue = vec![Queued::prepaid(siege)];
    // A city center trains troops at half a Barracks' pace.
    assert_eq!(
        g.city_build_turns(0, siege),
        BuildUnit::Siege.turns() * CITY_TRAINING_SLOWDOWN
    );
    for turn in 1..g.city_build_turns(0, siege) {
        g.resolve_economy();
        assert_eq!(g.cities[0].queue.len(), 1, "not done after {turn} turns");
    }
    g.resolve_economy();
    assert!(g.cities[0].queue.is_empty(), "done after its turns");
    assert!(g.units.iter().any(|u| u.unit_type == UnitType::Siege));
}

#[test]
fn production_speeds_builds_when_the_variant_is_on() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.production_speedup = true;
    let production = g.income(0).production();
    assert!(production > 0);
    g.cities[0].queue = vec![Queued::prepaid(Build::Unit(BuildUnit::Siege))];
    g.resolve_economy();
    assert_eq!(
        g.cities[0].queue[0].progress,
        WORK_PER_TURN + production / 4
    );
    g.production_speedup = false;
    g.resolve_economy();
    assert_eq!(
        g.cities[0].queue[0].progress,
        2 * WORK_PER_TURN + production / 4,
        "off, a turn does its fixed work"
    );
}

#[test]
fn growth_is_bought_with_food_when_it_starts_and_takes_its_turns() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let population = g.cities[0].population;
    g.stockpiles[Team::Blue.index()].food = 1000;
    g.queue_selected_city_growth();
    g.queue_selected_city_growth();
    assert_eq!(g.stock(Team::Blue).food, 1000, "nothing paid while queued");
    // A third would be priced for the bigger city the two ahead make.
    assert_eq!(g.queue_price(0, Build::Grow), grow_price(population + 2));
    // The first is paid as work on it starts, at the population now.
    let food = g.expected_stock(Team::Blue).food;
    g.resolve_economy();
    assert!(g.cities[0].queue[0].paid && !g.cities[0].queue[1].paid);
    assert_eq!(g.stock(Team::Blue).food, food - grow_price(population).food);
    // The second, unpaid, refunds nothing taken off.
    let food = g.stock(Team::Blue).food;
    g.remove_selected_city_queue_item(1);
    assert_eq!(g.stock(Team::Blue).food, food);
    assert_eq!(g.cities[0].population, population, "not grown yet");
    g.resolve_economy();
    assert_eq!(g.cities[0].population, population + 1);
    assert_eq!(g.cities[0].working(), population + 1, "the citizen works");

    // No Grow past the cap, counting those queued.
    g.cities[0].population = MAX_CITY_POPULATION - 1;
    g.queue_selected_city_growth();
    assert_eq!(builds(&g.cities[0].queue), [Build::Grow]);
    g.queue_selected_city_growth();
    assert_eq!(builds(&g.cities[0].queue), [Build::Grow], "full");
    assert!(!g.can_grow(0));
}

#[test]
fn a_grow_is_priced_when_paid_a_citizen_up_for_each_grow_already_paid() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let population = g.cities[0].population;
    g.stockpiles[Team::Blue.index()].food = 1000;
    // A Grow under way, and one dragged ahead of it: the city works the
    // unpaid one first, and pays for it as the second citizen coming.
    g.cities[0].queue = vec![
        Queued::worked(Build::Grow, WORK_PER_TURN),
        Queued::new(Build::Grow),
    ];
    g.move_selected_city_queue_item(1, true);
    let food = g.expected_stock(Team::Blue).food;
    g.resolve_economy();
    assert!(g.cities[0].queue.iter().all(|q| q.paid));
    assert_eq!(g.cities[0].queue[0].progress, WORK_PER_TURN);
    assert_eq!(
        g.cities[0].queue[1].progress, WORK_PER_TURN,
        "the other keeps its work"
    );
    assert_eq!(
        g.stock(Team::Blue).food,
        food - grow_price(population + 1).food
    );
    // Either taken off refunds the dearer price, leaving the other paid for
    // the population now.
    let food = g.stock(Team::Blue).food;
    g.remove_selected_city_queue_item(1);
    assert_eq!(
        g.stock(Team::Blue).food,
        food + grow_price(population + 1).food
    );
}

#[test]
fn a_side_that_cannot_feed_its_citizens_starves_its_largest_city() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities.push(City {
        population: 3,
        ..City::new(2, Team::Blue, Hex::new(-2, -3))
    });
    g.stockpiles[Team::Blue.index()].food = 0;
    g.feed_citizens();
    assert_eq!(g.stock(Team::Blue).food, 0, "the stockpile empties");
    assert_eq!(g.cities[0].population, 2, "the smaller city keeps its own");
    assert_eq!(g.cities[2].population, 2, "the largest loses one");
    assert_eq!(g.cities[1].population, 2, "the other side ate");
    // Never below one.
    g.cities[0].population = 1;
    g.cities[2].population = 1;
    g.feed_citizens();
    assert_eq!((g.cities[0].population, g.cities[2].population), (1, 1));
}

/// A land tile with no road, city or unit that a worker can improve.
#[test]
fn workers_after_the_manager_must_be_adjacent_to_it() {
    let mut g = GameState::city_scenario();
    g.cities[0].clusters.clear();
    let manager = Hex::new(-1, 0);
    let nearby = Hex::new(-1, 1);
    let distant = Hex::new(-3, 0);
    assert!(g.may_assign(0, manager));
    g.cities[0].clusters = one_cluster(&[manager]);
    assert!(g.may_assign(0, nearby));
    assert!(!g.may_assign(0, distant));
}

#[test]
fn blocked_worked_tile_is_restored_after_the_unit_leaves() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let manager = Hex::new(-2, 0);
    let blocked_worker = Hex::new(-1, 0);
    g.cities[0].clusters = one_cluster(&[manager, blocked_worker]);
    g.cities[0].remembered = g.cities[0].clusters.clone();
    g.units
        .push(Unit::new(777, blocked_worker, Team::Red, UnitType::Melee));

    g.reconcile_citizens(0);
    assert!(!g.cities[0].works(blocked_worker));
    assert!(cluster_tiles(&g.cities[0].remembered).any(|h| h == blocked_worker));

    g.units.clear();
    g.reconcile_citizens(0);
    assert!(g.cities[0].works(blocked_worker));
}
#[test]
fn assignments_obey_population() {
    let mut g = GameState::city_scenario();
    g.auto_assign_city(0);
    assert_eq!(g.cities[0].working(), 2);
    assert!(!g.may_assign(1, g.cities[0].clusters[0].manager));
}

#[test]
fn frontier_settler_founds_city_and_city_spends_production_on_unit() {
    let mut g = GameState::frontier_scenario();
    g.found_city_selected();
    assert_eq!(g.cities.len(), 1);
    assert_eq!(g.settlers.len(), 1, "the opposing settler remains");
    g.queue_selected_city_unit(BuildUnit::Melee);
    g.cities[0].queue[0] = Queued::worked(
        Build::Unit(BuildUnit::Melee),
        g.city_build_work(0, Build::Unit(BuildUnit::Melee)),
    );
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    let finished = g.units.last().unwrap();
    assert_eq!(finished.team, Team::Blue);
    assert_eq!(finished.unit_type, UnitType::Melee);
    assert_eq!(finished.pos.distance(g.cities[0].pos), 1);
}

#[test]
fn gathering_is_free_and_fills_the_stockpile_in_a_turn() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let start = g.stock(Team::Blue);
    g.queue_selected_city_gather();
    assert_eq!(g.stock(Team::Blue), start, "free");
    assert_eq!(g.city_build_turns(0, Build::Gather), 1);
    g.cities[0].queue[0] = Queued::worked(Build::Gather, Build::Gather.work());
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(g.stock(Team::Blue), start + GATHER_YIELD);
}

#[test]
fn barracks_trains_whatever_the_manager_does_and_faster_than_the_city() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let manager = Hex::new(-1, 0);
    let worker = Hex::new(-1, 1);
    g.cities[0].clusters = one_cluster(&[manager, worker]);
    g.cities[0].remembered = g.cities[0].clusters.clone();
    g.cities[0].barracks = Some(manager);
    g.cities[0].barracks_queue = vec![Queued::prepaid(BuildUnit::Siege)];
    g.cities[0].queue = vec![Queued::prepaid(Build::Unit(BuildUnit::Siege))];
    g.resolve_economy();
    assert_eq!(g.cities[0].barracks_queue[0].progress, WORK_PER_TURN);
    assert_eq!(
        g.cities[0].queue[0].progress, WORK_PER_TURN,
        "the city and Barracks both work"
    );
    assert_eq!(
        g.city_build_work(0, Build::Unit(BuildUnit::Siege)),
        CITY_TRAINING_SLOWDOWN * BuildUnit::Siege.work(),
        "but the city needs twice the work"
    );

    // The manager elsewhere doesn't pause it.
    swap_manager(&mut g.cities[0].clusters[0]);
    g.cities[0].remembered = g.cities[0].clusters.clone();
    g.resolve_economy();
    assert_eq!(g.cities[0].barracks_queue[0].progress, 2 * WORK_PER_TURN);
    g.cities[0].barracks_queue[0].progress = WORK_PER_TURN;

    // With production speeding builds, the Barracks adds what's delivered
    // to it, with its own delivery falloff.
    swap_manager(&mut g.cities[0].clusters[0]);
    g.cities[0].remembered = g.cities[0].clusters.clone();
    g.production_speedup = true;
    let barracks_income = g.barracks_income(0);
    assert!(barracks_income > 0);
    g.resolve_economy();
    assert_eq!(
        g.cities[0].barracks_queue[0].progress,
        2 * WORK_PER_TURN + barracks_income / 4
    );
}

#[test]
fn end_turn_holds_unfinished_player_units() {
    let mut g = GameState::new();
    assert!(g.next_unit_needing_orders(None).is_some());
    g.end_planning();
    assert!(
        g.units
            .iter()
            .filter(|u| u.team == PLAYER_TEAM)
            .all(|u| u.holding)
    );
    assert!(g.is_resolving());
}

#[test]
fn a_barracks_is_placed_for_a_worker_who_walks_out_and_builds_it() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.explore();
    let site = Hex::new(-2, 0);
    let before = g.stock(Team::Blue);
    assert!(place_building(&mut g, Building::Barracks, site));
    let job = WorkerJob::on_tile(site, JobKind::Build(Building::Barracks));
    assert_eq!(g.cities[0].worker_jobs, vec![job]);
    assert_eq!(
        g.placing_job, None,
        "one of each: placed, it's done placing"
    );
    assert_eq!(g.stock(Team::Blue), before - Building::Barracks.price());
    assert!(
        g.cities[0].queue.is_empty(),
        "the city queue isn't involved"
    );
    // A second can't be placed while this one waits.
    g.queue_selected_city_building(Building::Barracks);
    assert_eq!(g.placing_job, None);
    assert_eq!(g.notice, "BARRACKS IS ALREADY PLACED FOR THIS CITY");
    build_all(&mut g);
    assert_eq!(g.cities[0].barracks, Some(site));
    assert!(g.cities[0].built.contains(&Building::Barracks));
}

#[test]
fn a_barracks_may_stand_on_an_unworked_tile_as_its_card_says() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.explore();
    let city = g.cities[0].pos;
    let unworked = g
        .grid
        .all_hexes()
        .find(|&h| {
            h.distance(city) <= 3
                && !g.cities.iter().any(|c| c.works(h))
                && g.site_available(0, Building::Barracks, h)
        })
        .expect("an open, unworked land tile in reach");
    assert!(place_building(&mut g, Building::Barracks, unworked));
    let card = Building::Barracks.description();
    assert!(!card.contains("WORKED TILE"), "{card}");
    assert!(card.contains("OPEN LAND"), "{card}");
}

#[test]
fn a_destroyed_barracks_can_be_rebuilt() {
    use super::super::turn::Phase;
    let mut g = GameState::city_scenario();
    g.units.clear();
    let old_site = Hex::new(-2, 0);
    g.cities[0].barracks = Some(old_site);
    g.cities[0].built.push(Building::Barracks);
    g.cities[0].barracks_hp = 1.0;
    g.cities[0].barracks_queue = vec![Queued::prepaid(BuildUnit::Melee)];
    g.open_barracks(0);
    g.units.push(Unit::new(
        901,
        old_site.neighbors()[0],
        Team::Red,
        UnitType::Ranged,
    ));
    g.units[0].planned_attack = Some(old_site);
    g.resolve_step(UnitType::Ranged, Phase::Attack);
    assert_eq!(g.cities[0].barracks, None);
    assert!(!g.cities[0].built.contains(&Building::Barracks));
    assert!(g.cities[0].barracks_queue.is_empty());
    assert_eq!(g.selected_barracks, None, "its view closes with it");

    g.units.clear();
    g.explore();
    let site = Hex::new(-1, 0);
    assert!(place_building(&mut g, Building::Barracks, site));
    build_all(&mut g);
    assert_eq!(g.cities[0].barracks, Some(site));
    assert_eq!(g.cities[0].barracks_hp, BARRACKS_MAX_HP);
}

#[test]
fn reordering_city_and_barracks_queues_keeps_work_with_each_build() {
    let mut g = GameState::city_scenario();
    g.selected_city = Some(0);
    g.cities[0].queue = vec![
        Queued::worked(Build::Unit(BuildUnit::Melee), WORK_PER_TURN),
        Queued::new(Build::Unit(BuildUnit::Ranged)),
    ];
    g.move_selected_city_queue_head(false);
    assert_eq!(g.cities[0].queue[0].build, Build::Unit(BuildUnit::Ranged));
    assert_eq!(g.cities[0].queue[0].progress, 0);
    assert_eq!(g.cities[0].queue[1].progress, WORK_PER_TURN);
    g.move_selected_city_queue_item(1, true);
    assert_eq!(g.cities[0].queue[0].build, Build::Unit(BuildUnit::Melee));
    assert_eq!(g.cities[0].queue[0].progress, WORK_PER_TURN);

    g.cities[0].barracks_queue = vec![
        Queued::worked(BuildUnit::Melee, WORK_PER_TURN),
        Queued::new(BuildUnit::Ranged),
    ];
    g.move_selected_barracks_queue_item(0, false);
    assert_eq!(g.cities[0].barracks_queue[0].build, BuildUnit::Ranged);
    assert_eq!(g.cities[0].barracks_queue[0].progress, 0);
    assert_eq!(g.cities[0].barracks_queue[1].progress, WORK_PER_TURN);
    g.move_selected_barracks_queue_item(1, true);
    assert_eq!(g.cities[0].barracks_queue[0].build, BuildUnit::Melee);
    assert_eq!(g.cities[0].barracks_queue[0].progress, WORK_PER_TURN);
}

#[test]
fn city_queue_completes_in_order_and_can_be_reordered_or_removed() {
    let mut g = GameState::city_scenario();
    g.fund(Team::Blue);
    g.units.clear();
    g.selected_city = Some(0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    g.queue_selected_city_unit(BuildUnit::Ranged);
    assert_eq!(
        builds(&g.cities[0].queue),
        [
            Build::Unit(BuildUnit::Melee),
            Build::Unit(BuildUnit::Ranged)
        ]
    );
    g.move_selected_city_queue_item(1, true);
    assert_eq!(g.cities[0].queue[0].build, Build::Unit(BuildUnit::Ranged));
    g.remove_selected_city_queue_item(1);
    assert_eq!(builds(&g.cities[0].queue), [Build::Unit(BuildUnit::Ranged)]);
    g.cities[0].queue[0] = Queued::worked(
        Build::Unit(BuildUnit::Ranged),
        g.city_build_work(0, Build::Unit(BuildUnit::Ranged)),
    );
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    assert!(
        g.units
            .iter()
            .any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged)
    );
}

#[test]
fn a_placed_building_taken_off_the_list_is_refunded() {
    let mut g = GameState::city_scenario();
    g.explore();
    let before = g.stock(Team::Blue);
    assert!(place_building(&mut g, Building::Barracks, Hex::new(-2, 0)));
    g.remove_worker_job(0);
    assert!(g.cities[0].worker_jobs.is_empty());
    assert_eq!(g.stock(Team::Blue), before);
    assert!(
        place_building(&mut g, Building::Barracks, Hex::new(-2, 0)),
        "placeable again"
    );
}

#[test]
fn placing_a_building_leaves_the_city_queue_alone() {
    let mut g = GameState::city_scenario();
    g.fund(Team::Blue);
    g.explore();
    g.selected_city = Some(0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    assert!(place_building(&mut g, Building::Barracks, Hex::new(-2, 0)));
    assert_eq!(builds(&g.cities[0].queue), [Build::Unit(BuildUnit::Melee)]);
}

#[test]
fn mill_restores_food_delivery_only_within_city_reach() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    let worked = Hex::new(-1, 0);
    assert_eq!(g.routes(0).costs.get(&worked), Some(&6));
    g.cities[0].clusters = one_cluster(&[worked]);
    let food_before = g.income(0).food;
    let food_yield = g.tile_yield(worked).0;
    g.cities[0].mill = Some(Hex::new(-2, 0));
    assert_eq!(g.income(0).food, food_before + food_yield * 2);

    let out_of_reach = g
        .grid
        .all_hexes()
        .filter(|&h| g.grid.is_passable(h) && !g.routes(0).costs.contains_key(&h))
        .find(|&h| h.neighbors().into_iter().any(|n| g.grid.is_passable(n)))
        .expect("an open tile beyond the logistics cutoff");
    let mill_site = out_of_reach
        .neighbors()
        .into_iter()
        .find(|&h| g.grid.is_passable(h))
        .unwrap();
    g.cities[0].clusters = one_cluster(&[out_of_reach]);
    g.cities[0].mill = Some(mill_site);
    assert_eq!(
        g.income(0).food,
        8,
        "a mill cannot bypass the hard route cutoff"
    );
}

#[test]
fn a_workshop_halves_an_adjacent_buildings_work() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let workshop = Hex::new(-2, 0);
    let barracks = Hex::new(-1, 0);
    assert!(g.site_available(0, Building::Workshop, workshop));
    assert!(g.site_available(0, Building::Barracks, barracks));
    g.cities[0].workshop = Some(workshop);
    let beside = WorkerJob::on_tile(barracks, JobKind::Build(Building::Barracks));
    let far = WorkerJob::on_tile(Hex::new(-4, 2), JobKind::Build(Building::Barracks));
    assert_eq!(g.job_turns(Team::Blue, beside), 2, "3 turns, halved up");
    assert_eq!(g.job_turns(Team::Blue, far), 3);
    // Roads and the like aren't buildings: a Workshop doesn't hurry them.
    let road = WorkerJob::on_tile(barracks, JobKind::Road);
    assert_eq!(g.job_turns(Team::Blue, road), JobKind::Road.turns());
}

#[test]
fn placed_buildings_cannot_share_a_site() {
    let mut g = GameState::city_scenario();
    g.explore();
    let site = Hex::new(-2, 0);
    g.cities[0].mill = Some(site);
    assert!(!place_building(&mut g, Building::Workshop, site));
    assert!(g.cities[0].worker_jobs.is_empty());
    assert_eq!(g.notice, "SITE IS ALREADY CLAIMED BY A CITY OR BUILDING");
    assert_eq!(
        g.placing_job,
        Some(JobKind::Build(Building::Workshop)),
        "still placing, for another tile"
    );
}

#[test]
fn debug_completion_only_finishes_the_selected_production_lane() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    g.cities[0].queue = vec![Queued::new(Build::Gather)];
    g.cities[1].queue = vec![Queued::new(Build::Gather); 2];
    g.debug_complete_current_production();
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(
        g.cities[1].queue.len(),
        2,
        "the other city's lane is untouched"
    );

    g.cities[0].barracks = Some(Hex::new(-1, 0));
    g.selected_city = None;
    g.selected_barracks = Some(0);
    g.cities[0].barracks_queue = vec![Queued::new(BuildUnit::Melee)];
    g.debug_complete_current_production();
    assert!(g.cities[0].barracks_queue.is_empty());
    assert!(
        g.units
            .iter()
            .any(|u| u.team == PLAYER_TEAM && u.unit_type == UnitType::Melee)
    );
}

#[test]
fn barracks_queue_is_independent_and_completes_in_order() {
    let mut g = GameState::city_scenario();
    g.fund(Team::Blue);
    g.units.clear();
    g.selected_city = Some(0);
    let site = Hex::new(-1, 0);
    g.cities[0].barracks = Some(site);
    g.cities[0].clusters = one_cluster(&[site]);
    g.queue_selected_barracks_unit(BuildUnit::Melee);
    g.queue_selected_barracks_unit(BuildUnit::Ranged);
    g.move_selected_barracks_queue_item(1, true);
    assert_eq!(g.cities[0].barracks_queue[0].build, BuildUnit::Ranged);
    g.cities[0].barracks_queue[0] = Queued::worked(BuildUnit::Ranged, BuildUnit::Ranged.work());
    g.complete_builds();
    assert_eq!(builds(&g.cities[0].barracks_queue), [BuildUnit::Melee]);
    assert!(
        g.units
            .iter()
            .any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged)
    );
}

#[test]
fn resource_units_require_a_barracks_on_the_matching_resource() {
    let mut g = GameState::city_scenario();
    g.selected_city = Some(0);
    g.cities[0].barracks = Some(Hex::new(-2, 0));
    assert!(g.barracks_can_train(0, BuildUnit::Cavalry));
    assert!(!g.barracks_can_train(0, BuildUnit::Armored));
    g.queue_selected_barracks_unit(BuildUnit::Cavalry);
    assert_eq!(builds(&g.cities[0].barracks_queue), [BuildUnit::Cavalry]);
    g.cities[0].barracks = Some(Hex::new(-2, 1));
    assert!(g.barracks_can_train(0, BuildUnit::Armored));
    assert!(!g.barracks_can_train(0, BuildUnit::Cavalry));
}

/// A Barracks on Horses with plenty in the stockpile, and nothing else on
/// the map.
fn horse_barracks() -> GameState {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fund(Team::Blue);
    g.selected_city = Some(0);
    g.cities[0].barracks = Some(Hex::new(-2, 0));
    g.cities[0].built.push(Building::Barracks);
    g
}

/// Finishes the Barracks' queue, a troop a call, each walking off so the
/// next has room.
fn train_all(g: &mut GameState) {
    while let Some(build) = g.cities[0].barracks_queue.first().map(|q| q.build) {
        g.cities[0].barracks_queue[0] = Queued::worked(build, build.work());
        g.complete_builds();
        for unit in &mut g.units {
            unit.pos = Hex::new(unit.pos.q, 6 - unit.id as i32 % 4);
        }
    }
}

#[test]
fn a_horses_deposit_allows_three_cavalry_alive_at_once() {
    let mut g = horse_barracks();
    assert_eq!(
        g.special_cap(Team::Blue, Resource::Horses),
        UNITS_PER_DEPOSIT
    );
    for _ in 0..UNITS_PER_DEPOSIT {
        g.queue_selected_barracks_unit(BuildUnit::Cavalry);
    }
    assert_eq!(g.cities[0].barracks_queue.len(), UNITS_PER_DEPOSIT);
    // Queued ones count: a fourth is refused, unpaid.
    let stock = g.stock(Team::Blue);
    g.queue_selected_barracks_unit(BuildUnit::Cavalry);
    assert_eq!(g.cities[0].barracks_queue.len(), UNITS_PER_DEPOSIT);
    assert_eq!(g.stock(Team::Blue), stock);
    assert!(g.notice.contains("CAP REACHED"), "{}", g.notice);
    train_all(&mut g);
    let cavalry: Vec<u32> = g
        .units
        .iter()
        .filter(|u| u.drawn_from == Some(Resource::Horses))
        .map(|u| u.id)
        .collect();
    assert_eq!(cavalry.len(), UNITS_PER_DEPOSIT);
    assert!(g.barracks_lock(0, BuildUnit::Cavalry).is_some());
    // One dies: with the cap on those alive, another can be trained.
    g.units.retain(|u| u.id != cavalry[0]);
    assert_eq!(g.barracks_lock(0, BuildUnit::Cavalry), None);
    // With the lifetime cap, the deposit is spent.
    g.toggle_lifetime_special_cap();
    assert!(g.barracks_lock(0, BuildUnit::Cavalry).is_some());
    assert_eq!(
        g.special_trained[Team::Blue.index()][Resource::Horses.index()],
        UNITS_PER_DEPOSIT as u32
    );
    // Armored stay locked: no Iron under or beside this Barracks.
    let reason = g.barracks_lock(0, BuildUnit::Armored).unwrap();
    assert!(reason.contains("IRON"), "{reason}");
    // A basic troop never is.
    assert_eq!(g.barracks_lock(0, BuildUnit::Melee), None);
}

#[test]
fn an_enemy_on_the_deposit_takes_its_cap_away() {
    let mut g = horse_barracks();
    g.units
        .push(Unit::new(900, Hex::new(-2, 0), Team::Red, UnitType::Melee));
    assert_eq!(g.special_cap(Team::Blue, Resource::Horses), 0);
    let reason = g.barracks_lock(0, BuildUnit::Cavalry).unwrap();
    assert!(reason.contains("ENEMY"), "{reason}");
    g.units.clear();
    assert_eq!(
        g.special_cap(Team::Blue, Resource::Horses),
        UNITS_PER_DEPOSIT
    );
}

#[test]
fn the_ai_puts_its_barracks_on_a_deposit_and_trains_there() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fund(Team::Red);
    g.plan_ai_turn(Team::Red);
    let red = 1;
    // Its workers build it, placed and paid like the player's.
    let job = g.cities[red]
        .worker_jobs
        .iter()
        .find(|j| j.kind == JobKind::Build(Building::Barracks))
        .copied()
        .expect("a Barracks placed for its workers");
    let site = job.hex;
    assert!(g.grid.resource(site).is_some(), "{site:?} is no deposit");
    for _ in 0..30 {
        if g.cities[red].barracks.is_some() {
            break;
        }
        g.resolve_workers();
    }
    assert_eq!(g.cities[red].barracks, Some(site));
    g.plan_ai_turn(Team::Red);
    let special = g.cities[red].barracks_queue.first().map(|q| q.build);
    assert!(
        special.is_some_and(|b| b.required_resource() == g.grid.resource(site)),
        "{special:?}"
    );
}

#[test]
fn structure_menu_can_be_dismissed_without_selecting_a_unit() {
    let mut g = GameState::city_scenario();
    g.open_city(0);
    assert!(g.exit_structure_menu());
    assert_eq!(g.selected_city, None);
    assert!(!g.exit_structure_menu());
}

#[test]
fn priority_scores_weigh_the_order_nine_three_one_with_food_first_until_fed() {
    let order = Priorities([Good::Metal, Good::Food, Good::Wood]);
    assert!(order.is_order());
    // Food 1, wood 2, metal 3: metal ×9, food ×3, wood ×1.
    assert_eq!(order.score((1, 2, 3), true), 3 * 9 + 3 + 2);
    // Unfed, food goes first and the others keep their order: food ×9,
    // metal ×3, wood ×1.
    assert_eq!(order.score((1, 2, 3), false), 9 + 3 * 3 + 2);
    // The default is Food, Wood, Metal, fed or not.
    let default = Priorities::default();
    assert_eq!(default.0, [Good::Food, Good::Wood, Good::Metal]);
    assert_eq!(
        default.score((1, 2, 3), true),
        default.score((1, 2, 3), false)
    );
    // A tile of the first good beats one with some of everything else.
    assert!(order.score((0, 0, 1), true) > order.score((2, 2, 0), true));
    // Reordering: to the front, and a drag either way.
    assert_eq!(
        order.with_first(Good::Wood).0,
        [Good::Wood, Good::Metal, Good::Food]
    );
    assert_eq!(order.moved(0, 2).0, [Good::Food, Good::Wood, Good::Metal]);
    assert_eq!(order.moved(2, 1).0, [Good::Metal, Good::Wood, Good::Food]);
    assert_eq!(order.moved(1, 1), order);
    assert_eq!(order.moved(0, 5), order, "out of range: unchanged");
    assert!(!Priorities([Good::Food, Good::Food, Good::Metal]).is_order());
}

/// The food, wood and metal `city`'s center and worked tiles deliver.
fn worked_goods(g: &GameState, city: usize) -> (i32, i32, i32) {
    let routes = g.routes(city);
    g.cities[city]
        .worked()
        .map(|h| g.delivered_goods(city, h, routes.costs[&h]))
        .fold((8, 4, 0), |(f, w, m), (a, b, c)| (f + a, w + b, m + c))
}

#[test]
fn a_citys_priority_order_picks_its_tiles_and_the_food_floor_holds() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].population = CLUSTER_SIZE;
    let upkeep = CLUSTER_SIZE as i32 * FOOD_PER_CITIZEN;
    let orders = [
        [Good::Food, Good::Wood, Good::Metal],
        [Good::Food, Good::Metal, Good::Wood],
        [Good::Wood, Good::Food, Good::Metal],
        [Good::Wood, Good::Metal, Good::Food],
        [Good::Metal, Good::Food, Good::Wood],
        [Good::Metal, Good::Wood, Good::Food],
    ]
    .map(Priorities);
    let goods: Vec<(Priorities, (i32, i32, i32))> = orders
        .into_iter()
        .map(|order| {
            g.cities[0].priorities = order;
            g.auto_assign_city(0);
            assert!(g.cities[0].working() > 1, "{order:?}");
            (order, worked_goods(&g, 0))
        })
        .collect();
    let most_food = goods.iter().map(|(_, (f, _, _))| *f).max().unwrap();
    assert!(most_food >= upkeep + 4, "the city can feed itself");
    for &(order, (food, wood, metal)) in &goods {
        // Fed whatever the order: upkeep and one more.
        assert!(food >= upkeep + 4, "{order:?}: {food}");
        // Its first good: as much of it as any order gets.
        let most =
            |pick: fn(&(i32, i32, i32)) -> i32| goods.iter().map(|(_, g)| pick(g)).max().unwrap();
        let first = match order.0[0] {
            Good::Food => (food, most(|g| g.0)),
            Good::Wood => (wood, most(|g| g.1)),
            Good::Metal => (metal, most(|g| g.2)),
        };
        assert_eq!(first.0, first.1, "{order:?}: {:?}", (food, wood, metal));
    }
    // Setting the order from the city view reassigns by it.
    g.open_city(0);
    g.set_selected_city_priorities(orders[5]);
    assert_eq!(worked_goods(&g, 0), goods[5].1);
    g.prioritize_selected_city(Good::Wood);
    assert_eq!(g.cities[0].priorities, orders[3]);
    assert_eq!(worked_goods(&g, 0), goods[3].1);
    assert_eq!(g.notice, "CITY PRIORITIES: WOOD > METAL > FOOD");
}

#[test]
fn a_new_citizen_takes_the_tile_the_priority_order_ranks_best() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    for first in [Good::Wood, Good::Metal, Good::Food] {
        g.cities[0].population = 4;
        g.cities[0].priorities = Priorities::default().with_first(first);
        g.auto_assign_city(0);
        let before = g.cities[0].worked().collect::<Vec<_>>();
        // It grows: reconciling fills the new slot by the order (food
        // first if the tiles kept don't feed it), keeping the others.
        g.cities[0].population += 1;
        let fed = g.is_fed(0, worked_goods(&g, 0).0);
        let routes = g.routes(0);
        let score = |g: &GameState, h: Hex| {
            let goods = g.delivered_goods(0, h, routes.costs[&h]);
            g.cities[0].priorities.score(goods, fed)
        };
        let best = routes
            .costs
            .keys()
            .filter(|h| before[0].distance(**h) == 1 && g.may_assign(0, **h))
            .map(|&h| score(&g, h))
            .max()
            .expect("a free tile by the manager");
        g.reconcile_citizens(0);
        let worked = g.cities[0].worked().collect::<Vec<_>>();
        assert_eq!(worked.len(), 5, "{first:?}");
        assert_eq!(worked[..before.len()], before[..], "{first:?}");
        assert_eq!(score(&g, worked[before.len()]), best, "{first:?}");
    }
}
#[test]
fn no_citizen_works_a_city_center_or_a_building_tile() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.explore();
    let center = g.cities[0].pos;
    // The manager beside both the city center and a Barracks.
    let manager = center.neighbors()[0];
    let barracks = *center
        .neighbors()
        .iter()
        .find(|n| **n != manager && n.distance(manager) == 1)
        .unwrap();
    g.cities[0].barracks = Some(barracks);
    g.cities[0].population = MAX_CITY_POPULATION;
    g.cities[0].clusters = one_cluster(&[manager]);
    g.cities[0].remembered = one_cluster(&[manager, center, barracks]);
    g.reconcile_citizens(0);
    let worked: Vec<Hex> = g.cities[0].worked().collect();
    assert_eq!(worked[0], manager);
    assert!(!worked.contains(&center), "{worked:?}");
    assert!(!worked.contains(&barracks), "{worked:?}");
    assert!(worked.len() > 1, "the others found open tiles");
    // Nor by hand, nor as the manager.
    assert!(!g.may_assign(0, center) && !g.may_assign(0, barracks));
    assert!(!g.may_be_manager(0, 0, barracks));
    g.open_city(0);
    g.city_click(barracks);
    assert!(!g.cities[0].works(barracks));
    assert_eq!(g.notice, "A BUILDING STANDS THERE - NO CITIZEN CAN WORK IT");
    // A city center's yield comes in whoever works what.
    g.cities[0].clusters.clear();
    assert_eq!(g.income(0).food, 8, "2 food from the center alone");
}

#[test]
fn city_menu_keeps_barracks_clicks_in_manager_assignment_context() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.explore();
    let manager = Hex::new(-2, 0);
    let barracks = Hex::new(-1, 0);
    g.cities[0].clusters = one_cluster(&[manager]);
    g.cities[0].barracks = Some(barracks);
    g.open_city(0);

    g.city_click(manager);
    assert_eq!(g.moving_manager, Some((0, 0)));
    g.city_click(barracks);

    // A building covers its tile: the manager can't go there, and the
    // click stays with the city rather than opening the Barracks.
    assert_eq!(
        g.cities[0].clusters.first().map(|c| c.manager),
        Some(manager)
    );
    assert_eq!(g.moving_manager, Some((0, 0)));
    assert_eq!(g.selected_city, Some(0));
    assert_eq!(g.selected_barracks, None);
}

/// Plays one turn of city 0's economy and returns the work its queue did and how many of
/// its (Blue) units came out of it. Newly finished units walk away at once, so the city
/// always has an open hex beside it unless the test blocks them.
fn economy_turn(g: &mut GameState, blockers: &[u32]) -> (i32, usize) {
    let blue = |g: &GameState| g.units.iter().filter(|u| u.team == Team::Blue).count();
    let income = g.work_rate(g.income(0).production());
    let before = blue(g);
    g.resolve_economy();
    let finished = blue(g) - before;
    g.units
        .retain(|u| u.team != Team::Blue || blockers.contains(&u.id));
    (income, finished)
}

/// #54: with production per turn below a unit's cost, a queue of units comes out at the
/// rate production allows. A finished unit that waits for an open hex doesn't bank the
/// production earned meanwhile, which used to empty the rest of the queue one unit a turn.
#[test]
fn a_queue_of_units_completes_at_the_rate_production_allows() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    g.cities[0].clusters = one_cluster(&[Hex::new(-3, 0)]);
    g.cities[0].queue = vec![Queued::prepaid(Build::Unit(BuildUnit::Melee)); 6];
    let cost = g.city_build_work(0, Build::Unit(BuildUnit::Melee));

    // Open hexes: the first unit takes several turns, as its cost allows.
    let (mut earned, mut turns) = (0, 0);
    loop {
        let (income, finished) = economy_turn(&mut g, &[]);
        assert!(
            income < cost,
            "the test needs production below a unit's cost"
        );
        earned += income;
        turns += 1;
        if finished > 0 {
            assert_eq!(finished, 1);
            assert!(
                earned >= cost,
                "a unit finished with {earned} of {cost} production"
            );
            break;
        }
    }
    assert!(turns > 1, "a unit finished in {turns} turn");
    assert_eq!(
        (earned - cost, g.cities[0].queue[0].progress),
        (0, 0),
        "no work was left over, and the next unit starts from nothing"
    );

    // Every open hex beside the city is taken: the next unit waits in the city.
    let blockers: Vec<u32> = g.cities[0]
        .pos
        .neighbors()
        .into_iter()
        .filter(|&hex| g.grid.is_passable(hex))
        .enumerate()
        .map(|(n, hex)| {
            let id = 1000 + n as u32;
            g.units
                .push(Unit::new(id, hex, Team::Blue, UnitType::Melee));
            id
        })
        .collect();
    let mut blocked_earnings = 0;
    while blocked_earnings < 5 * cost {
        let (income, finished) = economy_turn(&mut g, &blockers);
        blocked_earnings += income;
        assert_eq!(finished, 0, "no hex is open");
    }
    assert_eq!(
        g.cities[0].queue[0].progress, cost,
        "the waiting unit is finished, and nothing more is banked"
    );
    assert!(g.cities[0].queue[1..].iter().all(|q| q.progress == 0));

    // A hex opens: the waiting unit comes out, and each after it takes its cost again.
    g.units.retain(|u| u.team != Team::Blue);
    let (mut earned, mut finished) = (0, 0);
    for _ in 0..6 {
        let (income, done) = economy_turn(&mut g, &[]);
        earned += income;
        finished += done;
        assert!(
            (finished as i32 - 1) * cost <= earned,
            "{finished} units came out of {earned} production after the wait"
        );
    }
    assert!(finished >= 1, "the waiting unit came out");
}

/// A city and a barracks beside it that finish units the same turn with only one hex open
/// between them don't both put a unit on it.
#[test]
fn a_city_and_its_barracks_never_finish_units_onto_one_hex() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let city = g.cities[0].pos;
    let barracks = Hex::new(-3, 0);
    assert_eq!(city.distance(barracks), 1);
    g.cities[0].barracks = Some(barracks);
    let shared: Vec<Hex> = city
        .neighbors()
        .into_iter()
        .filter(|h| h.distance(barracks) == 1 && g.grid.is_passable(*h))
        .collect();
    let open = shared[0];
    for (n, hex) in city
        .neighbors()
        .into_iter()
        .chain(barracks.neighbors())
        .filter(|&h| h != open && g.grid.is_passable(h))
        .enumerate()
    {
        if !g.is_occupied(hex) {
            g.units
                .push(Unit::new(1000 + n as u32, hex, Team::Blue, UnitType::Melee));
        }
    }
    g.cities[0].queue = vec![Queued::worked(
        Build::Unit(BuildUnit::Melee),
        g.city_build_work(0, Build::Unit(BuildUnit::Melee)),
    )];
    g.cities[0].barracks_queue = vec![Queued::worked(BuildUnit::Ranged, BuildUnit::Ranged.work())];
    g.complete_builds();
    assert_eq!(g.units_at(open).count(), 1, "one unit on the open hex");
    assert_eq!(
        builds(&g.cities[0].barracks_queue),
        [BuildUnit::Ranged],
        "the barracks waits for a hex"
    );
}

#[test]
fn coastal_construction_requires_the_city_center_to_touch_the_sea() {
    let mut inland = GameState::city_scenario();
    inland.fund(Team::Blue);
    inland.selected_city = Some(0);
    assert!(!inland.city_is_coastal(0));
    for building in [Building::Harbor, Building::CoastalBattery] {
        inland.queue_selected_city_building(building);
        assert_eq!(inland.placing_job, None);
        assert_eq!(
            inland.notice,
            "ONLY COASTAL CITIES CAN BUILD NAVAL BUILDINGS"
        );
        assert!(!inland.site_available(0, building, Hex::new(-2, 1)));
    }
    inland.cities[0]
        .extra_buildings
        .insert(Building::Harbor, Hex::new(-2, 1));
    inland.queue_selected_city_unit(BuildUnit::PatrolGalley);
    assert!(
        !inland.cities[0]
            .queue
            .iter()
            .any(|q| q.build == Build::Unit(BuildUnit::PatrolGalley))
    );

    let mut coastal = GameState::naval_scenario();
    coastal.fund(Team::Blue);
    coastal.selected_city = Some(0);
    assert!(coastal.city_is_coastal(0));
    assert!(coastal.cities[0].placed_site(Building::Harbor).is_some());
    coastal.queue_selected_city_unit(BuildUnit::LandingCraft);
    assert!(
        coastal.cities[0]
            .queue
            .iter()
            .any(|q| q.build == Build::Unit(BuildUnit::LandingCraft))
    );
}

#[test]
fn rejected_building_sites_name_the_requirement_and_keep_placement_active() {
    for (building, reason) in [
        (Building::Stable, "NEEDS HORSES ON OR NEXT TO THE TILE"),
        (Building::Forge, "NEEDS IRON ON OR NEXT TO THE TILE"),
        (Building::CanoeHouse, "NEEDS A RIVERBANK TILE"),
        (
            Building::Smelter,
            "NEEDS HILLS OR IRON ON OR NEXT TO THE TILE",
        ),
    ] {
        let mut game = GameState::city_scenario();
        game.fund(Team::Blue);
        game.explore();
        let city = game.cities[0].pos;
        let site = game
            .grid
            .all_hexes()
            .find(|&hex| {
                hex.distance(city) <= 3
                    && game.site_available(0, Building::Barracks, hex)
                    && game.ai_site_issue(0, building, hex) == Some(reason)
            })
            .expect("open land in reach lacking this building's required feature");
        assert!(!place_building(&mut game, building, site));
        assert_eq!(game.notice, reason);
        assert_eq!(game.placing_job, Some(JobKind::Build(building)));
        assert!(game.cities[0].worker_jobs.is_empty());
    }
}

#[test]
fn a_building_whose_site_went_bad_is_dropped_and_refunded() {
    let mut game = GameState::city_scenario();
    game.units.clear();
    game.explore();
    let site = Hex::new(-2, 0);
    let before = game.stock(Team::Blue);
    assert!(place_building(&mut game, Building::Barracks, site));
    // Something else goes up there before the worker leaves.
    game.cities[0].mill = Some(site);
    game.resolve_workers();
    assert!(game.cities[0].worker_jobs.is_empty());
    assert_eq!(game.cities[0].barracks, None);
    assert_eq!(game.stock(Team::Blue), before);
    assert!(game.notice.contains("DROPPED, REFUNDED"), "{}", game.notice);
}

/// Places `building` for city 0's workers at `site`, as the player does:
/// its card in the open city, then a click on the tile. Returns whether it
/// was placed.
fn place_building(g: &mut GameState, building: Building, site: Hex) -> bool {
    g.selected_city = Some(0);
    g.queue_selected_city_building(building);
    g.place_job_at(site, None)
}

/// Plays city 0's workers' part of turns until nothing it placed is left
/// waiting or under way.
fn build_all(g: &mut GameState) {
    for _ in 0..30 {
        let busy = !g.cities[0].worker_jobs.is_empty()
            || g.field_workers
                .iter()
                .any(|w| w.home == 0 && w.job.is_some());
        if !busy {
            return;
        }
        g.resolve_workers();
    }
    panic!("city 0's workers never finished");
}

#[test]
fn reconciliation_never_assigns_a_citizen_to_a_city_center() {
    let mut game = GameState::city_scenario();
    let city = 0;
    let center = game.cities[city].pos;
    let manager = center.neighbors()[0];
    game.cities[city].population = 7;
    game.cities[city].clusters = one_cluster(&[manager, center]);
    game.cities[city].remembered = one_cluster(&[manager, center]);
    game.reconcile_citizens(city);
    assert!(!game.cities[city].works(center));
    assert!(!game.may_be_manager(city, 0, center));
    assert!(
        game.cities[city]
            .worked()
            .skip(1)
            .all(|h| h.distance(manager) == 1)
    );
}

/// A finished troop never appears on a worker out on the map: it would share
/// the hex with an enemy's without capturing it.
#[test]
fn a_finished_troop_does_not_appear_on_a_worker() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let city = g.cities[0].pos;
    let open: Vec<Hex> = city
        .neighbors()
        .into_iter()
        .filter(|&h| g.grid.is_passable(h))
        .collect();
    for (n, &hex) in open.iter().enumerate().skip(1) {
        g.units
            .push(Unit::new(1000 + n as u32, hex, Team::Blue, UnitType::Melee));
    }
    g.field_workers.push(crate::game::workers::FieldWorker {
        id: 10_000,
        team: Team::Red,
        home: 1,
        base: g.cities[1].pos,
        pos: open[0],
        job: None,
        work_left: None,
        recalled: false,
    });
    g.cities[0].queue = vec![Queued::worked(
        Build::Unit(BuildUnit::Melee),
        g.city_build_work(0, Build::Unit(BuildUnit::Melee)),
    )];
    g.complete_builds();
    assert_eq!(
        g.units_at(open[0]).count(),
        0,
        "the worker's hex stays clear"
    );
    assert_eq!(g.cities[0].queue.len(), 1, "the troop waits");
}

/// City 0 of the Cities scenario, open, with a mixed queue, some of it paid
/// for (as the turns' economies would have), with work done on its head,
/// and a Barracks with troops queued, the first paid for and under way.
fn city_with_full_queues() -> GameState {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fund(Team::Blue);
    g.cities[0].barracks = Some(Hex::new(-2, 0));
    g.selected_city = Some(0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    g.queue_selected_city_growth();
    g.queue_selected_city_worker();
    g.queue_selected_city_growth();
    g.queue_selected_city_gather();
    // Paid for: the Melee under way and both Grows.
    for (index, progress) in [(0, 1), (1, 0), (3, 0)] {
        let item = &mut g.cities[0].queue[index];
        *item = Queued::worked(item.build, progress);
    }
    for build in [BuildUnit::Ranged, BuildUnit::Melee, BuildUnit::Siege] {
        g.queue_selected_barracks_unit(build);
    }
    g.cities[0].barracks_queue[0] = Queued::worked(BuildUnit::Ranged, 1);
    assert_eq!(g.cities[0].queue.len(), 5);
    assert_eq!(g.cities[0].barracks_queue.len(), 3);
    g
}

#[test]
fn clearing_a_queue_refunds_it_as_taking_each_item_off_would() {
    let start = city_with_full_queues();

    // The city queue: its X from the head down, from the tail up, or
    // Clear, all end on the same stockpile.
    let mut from_head = start.clone();
    while !from_head.cities[0].queue.is_empty() {
        from_head.remove_selected_city_queue_item(0);
    }
    let mut from_tail = start.clone();
    while let Some(last) = from_tail.cities[0].queue.len().checked_sub(1) {
        from_tail.remove_selected_city_queue_item(last);
    }
    let mut cleared = start.clone();
    cleared.clear_selected_city_queue();
    assert!(cleared.cities[0].queue.is_empty());
    assert_eq!(cleared.stock(Team::Blue), from_head.stock(Team::Blue));
    assert_eq!(cleared.stock(Team::Blue), from_tail.stock(Team::Blue));
    assert!(cleared.stock(Team::Blue).food > start.stock(Team::Blue).food);
    assert_eq!(
        cleared.notice,
        "CLEARED THE CITY QUEUE - 3 PAID OF 5 REFUNDED"
    );
    assert_eq!(
        cleared.cities[0].barracks_queue, start.cities[0].barracks_queue,
        "the Barracks keeps its own"
    );

    // The Barracks queue, from its own view.
    let mut one_by_one = start.clone();
    one_by_one.open_barracks(0);
    while !one_by_one.cities[0].barracks_queue.is_empty() {
        one_by_one.remove_selected_barracks_queue_item(0);
    }
    let mut cleared = start.clone();
    cleared.open_barracks(0);
    cleared.clear_selected_barracks_queue();
    assert!(cleared.cities[0].barracks_queue.is_empty());
    assert_eq!(cleared.stock(Team::Blue), one_by_one.stock(Team::Blue));
    assert_eq!(cleared.cities[0].queue, start.cities[0].queue);

    // Clearing an empty queue does nothing, and says nothing.
    cleared.notice.clear();
    cleared.clear_selected_barracks_queue();
    assert_eq!(cleared.notice, "");
}

#[test]
fn no_queue_changes_while_a_turn_plays_out() {
    let mut g = city_with_full_queues();
    g.resolve_turn();
    assert!(g.is_resolving());
    let (queue, barracks, stock) = (
        g.cities[0].queue.clone(),
        g.cities[0].barracks_queue.clone(),
        g.stock(Team::Blue),
    );
    g.selected_city = Some(0);
    g.clear_selected_city_queue();
    g.remove_selected_city_queue_item(0);
    g.remove_selected_city_queue_head();
    g.clear_selected_barracks_queue();
    g.remove_selected_barracks_queue_item(0);
    assert_eq!(g.cities[0].queue, queue);
    assert_eq!(g.cities[0].barracks_queue, barracks);
    assert_eq!(g.stock(Team::Blue), stock);
}

#[test]
fn a_cleared_queue_makes_a_plan_that_passes_the_checks() {
    use crate::game::{NetMessage, PROTOCOL_VERSION, Settings};
    let mut host = GameState::host_game(2, &Settings::default());
    let (seat, welcome) = host.welcome(&NetMessage::Hello {
        version: PROTOCOL_VERSION,
    });
    let seat = seat.expect("seated");
    let mut guest = GameState::join_game(&welcome).expect("joins");
    let city = guest.cities.iter().position(|c| c.team == seat).unwrap();
    // The same queues on both machines as a turn's planning begins, as an
    // earlier turn would leave them: the city's, some paid for and work done
    // on its head, and its Barracks'.
    // The Barracks beside it on no tile its citizens work, or the plan
    // would be refused for those tiles (the world is new every run).
    let c = &guest.cities[city];
    let barracks = c
        .pos
        .neighbors()
        .into_iter()
        .find(|h| guest.grid.is_passable(*h) && !c.works(*h));
    for game in [&mut host, &mut guest] {
        let c = &mut game.cities[city];
        c.barracks = barracks;
        c.queue = vec![
            Queued::worked(Build::Unit(BuildUnit::Melee), 1),
            Queued::prepaid(Build::Grow),
            Queued::new(Build::Worker),
            Queued::prepaid(Build::Grow),
        ];
        c.barracks_queue = vec![
            Queued::worked(BuildUnit::Ranged, 1),
            Queued::new(BuildUnit::Melee),
        ];
        game.finish_lockstep_turn();
    }
    let untouched = guest.team_plan(seat);

    guest.open_city(city);
    guest.clear_selected_city_queue();
    guest.open_barracks(city);
    guest.clear_selected_barracks_queue();
    let plan = guest.team_plan(seat);
    let c = plan.cities.iter().find(|c| c.pos == guest.cities[city].pos);
    let c = c.expect("its city");
    assert!(c.queue.is_empty() && c.barracks_queue.is_empty());
    assert!(plan.stock.food > untouched.stock.food, "refunded");

    // Emptied without the refund, the checks would catch it.
    let mut unpaid = plan.clone();
    unpaid.stock = untouched.stock;
    assert!(
        host.clone()
            .receive(seat, NetMessage::Plan(unpaid))
            .is_err()
    );
    host.receive(seat, NetMessage::Plan(plan))
        .expect("a cleared queue is a sound plan");
}

/// The window the city-view click tests click in.
const CLICK_SCREEN: glam::Vec2 = glam::Vec2::new(1600.0, 900.0);

/// City 0 open, a manager at (-2, 0), and one Blue melee unit, alone on the
/// map, on (-1, 0): a tile the city may put its second citizen to work on.
fn city_with_a_unit_on_a_workable_tile() -> (GameState, usize, Hex) {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.explore();
    let tile = Hex::new(-1, 0);
    g.cities[0].clusters = one_cluster(&[Hex::new(-2, 0)]);
    g.cities[0].remembered = g.cities[0].clusters.clone();
    g.units
        .push(Unit::new(900, tile, Team::Blue, UnitType::Melee));
    g.open_city(0);
    assert!(g.may_assign(0, tile) && g.routes(0).costs.contains_key(&tile));
    (g, 0, tile)
}

/// A left click at world `point`, as the ImGui presentation passes a click
/// on the map through (`handle_map_click`), or through the classic UI's
/// panels first (`handle_click`).
fn click_at(g: &mut GameState, point: glam::Vec2, classic: bool) {
    let cursor = g.camera.world_to_screen(point, CLICK_SCREEN);
    let mode = crate::game::orders::ClickMode::Normal;
    if classic {
        g.handle_click(cursor, CLICK_SCREEN, mode);
    } else {
        g.handle_map_click(cursor, CLICK_SCREEN, mode);
    }
}

/// Off a full-size unit's token but on its hex (a flat-top hex reaches 1.0
/// across and 0.87 up; the token about 0.39).
const OFF_TOKEN: glam::Vec2 = glam::Vec2::new(0.6, 0.0);

/// #203: with a city open, a click on one of your units' tokens selects the
/// unit and closes the city, in both presentations.
#[test]
fn clicking_a_units_token_in_the_city_view_selects_it_and_closes_the_city() {
    for classic in [false, true] {
        let (mut g, unit, tile) = city_with_a_unit_on_a_workable_tile();
        let worked = g.cities[0].worked().collect::<Vec<_>>();
        click_at(&mut g, tile.to_world(), classic);
        assert_eq!(g.selected, Some(unit), "classic: {classic}");
        assert_eq!(g.selected_city, None);
        assert_eq!(
            g.cities[0].worked().collect::<Vec<_>>(),
            worked,
            "no citizen moved"
        );
    }
}

/// #203: a click on the same hex off the token is still the city's: it puts
/// a citizen to work there, and a second releases it.
#[test]
fn clicking_a_units_tile_off_its_token_still_assigns_a_citizen() {
    for classic in [false, true] {
        let (mut g, _, tile) = city_with_a_unit_on_a_workable_tile();
        let point = tile.to_world() + OFF_TOKEN;
        assert_eq!(Hex::from_world(point), tile);
        click_at(&mut g, point, classic);
        assert_eq!(g.selected, None, "classic: {classic}");
        assert_eq!(g.selected_city, Some(0));
        assert!(g.cities[0].works(tile), "{}", g.notice);
        click_at(&mut g, point, classic);
        assert!(!g.cities[0].works(tile));
        assert_eq!(g.selected_city, Some(0));
    }
}

/// #203: the same rule in the Barracks view: the token selects the unit and
/// closes the Barracks; the rest of the hex leaves it open.
#[test]
fn clicking_a_units_token_in_the_barracks_view_selects_it_and_closes_it() {
    let (mut g, unit, tile) = city_with_a_unit_on_a_workable_tile();
    g.cities[0].barracks = Some(Hex::new(-3, 1));
    g.open_barracks(0);
    assert_eq!(g.selected_barracks, Some(0));
    click_at(&mut g, tile.to_world() + OFF_TOKEN, false);
    assert_eq!(g.selected_barracks, Some(0));
    assert_eq!(g.selected, None);
    assert_eq!(g.notice, "BARRACKS MENU - PRESS ESC OR SPACE TO EXIT");
    click_at(&mut g, tile.to_world(), false);
    assert_eq!(g.selected, Some(unit));
    assert_eq!(g.selected_barracks, None);
    assert_eq!(g.selected_city, None);
}

/// #203: the token is hit as drawn. A military disc reaches about 0.39 from
/// the hex center every way; a settler's pointy-top hexagon about 0.38 up
/// to its corner but only 0.33 to its sides. In a contested hex the
/// player's unit is drawn at half size above the center, the rival below.
#[test]
fn the_token_click_follows_the_drawn_token() {
    let (mut g, unit, tile) = city_with_a_unit_on_a_workable_tile();
    let center = tile.to_world();
    for angle in (0..16).map(|i| i as f32 / 16.0 * std::f32::consts::TAU) {
        let direction = glam::Vec2::from_angle(angle);
        assert!(g.unit_token_contains(unit, center + direction * 0.37));
        assert!(!g.unit_token_contains(unit, center + direction * 0.41));
    }
    let id = g.units[unit].id;
    g.settlers.insert(id);
    assert!(g.unit_token_contains(unit, center + glam::Vec2::new(0.0, 0.36)));
    assert!(!g.unit_token_contains(unit, center + glam::Vec2::new(0.36, 0.0)));
    g.settlers.clear();

    // A Red unit on the same hex: the two share it as half-size tokens.
    g.units
        .push(Unit::new(901, tile, Team::Red, UnitType::Melee));
    let (above, scale) = g.unit_layout(unit);
    assert!(above.y > center.y && scale < 1.0);
    click_at(&mut g, center, false);
    assert_eq!(g.selected, None, "the center is between the two tokens");
    assert_eq!(g.selected_city, Some(0));
    click_at(&mut g, center + (center - above), false);
    assert_eq!(g.selected, None, "the rival's token is not the player's");
    assert_eq!(g.selected_city, Some(0));
    click_at(&mut g, above, false);
    assert_eq!(g.selected, Some(unit));
    assert_eq!(g.selected_city, None);
}

/// #203: placing a worker job and moving the manager keep every map click,
/// on a token or not; picking up the manager is a click off the token.
#[test]
fn placing_and_manager_clicks_are_not_taken_by_unit_tokens() {
    // Placing a road: the click on the token places it.
    let (mut g, _, tile) = city_with_a_unit_on_a_workable_tile();
    g.fund(Team::Blue);
    g.arm_worker_job(JobKind::Road);
    assert_eq!(g.placing_job, Some(JobKind::Road));
    click_at(&mut g, tile.to_world(), false);
    assert_eq!(g.selected, None);
    assert_eq!(g.selected_city, Some(0));
    assert!(
        g.cities[0]
            .worker_jobs
            .iter()
            .any(|job| job.hex == tile && job.kind == JobKind::Road),
        "{}",
        g.notice
    );

    // The manager's tile, with a unit on it: off the token picks the
    // manager up; then a click on a unit's token is where it goes.
    let (mut g, _, tile) = city_with_a_unit_on_a_workable_tile();
    let manager = g.cities[0].clusters[0].manager;
    g.units
        .push(Unit::new(902, manager, Team::Blue, UnitType::Melee));
    click_at(&mut g, manager.to_world() + OFF_TOKEN, false);
    assert_eq!(g.moving_manager, Some((0, 0)), "{}", g.notice);
    assert_eq!(g.selected, None);
    assert!(g.may_be_manager(0, 0, tile));
    click_at(&mut g, tile.to_world(), false);
    assert_eq!(g.moving_manager, None);
    assert_eq!(
        g.cities[0].clusters.first().map(|c| c.manager),
        Some(tile),
        "{}",
        g.notice
    );
    assert_eq!(g.selected, None);
    assert_eq!(g.selected_city, Some(0));

    // Inside the city, its own clicks.
    let (mut g, _, tile) = city_with_a_unit_on_a_workable_tile();
    g.open_city_interior(0);
    assert!(g.city_click_at(tile, tile.to_world()));
    assert_eq!(g.selected, None);
    assert_eq!(g.interior_view, Some(0));
}

/// Swaps a cluster's manager with its first worker.
fn swap_manager(cluster: &mut Cluster) {
    std::mem::swap(&mut cluster.manager, &mut cluster.workers[0]);
}

/// `g`'s city `city` at `population`, its citizens auto-assigned.
fn grown(g: &mut GameState, city: usize, population: usize) {
    g.cities[city].population = population;
    g.auto_assign_city(city);
}

/// The cluster rules hold for `city`: no more managers than its population
/// allows, each on land and beside none of the others, each worker beside
/// its own manager, six at most, and no tile twice.
fn assert_clusters_hold(g: &GameState, city: usize) {
    let c = &g.cities[city];
    assert!(c.clusters.len() <= c.managers_allowed(), "{:?}", c.clusters);
    assert!(c.working() <= c.capacity());
    let mut seen = crate::game::fast_hash::HashSet::default();
    for (k, cluster) in c.clusters.iter().enumerate() {
        assert!(!g.grid.terrain(cluster.manager).is_water(), "M{}", k + 1);
        for other in &c.clusters[..k] {
            assert!(other.manager.distance(cluster.manager) > 1, "M{}", k + 1);
        }
        assert!(cluster.workers.len() <= WORKERS_PER_MANAGER);
        for w in &cluster.workers {
            assert_eq!(w.distance(cluster.manager), 1, "M{}: {w:?}", k + 1);
        }
    }
    for h in c.worked() {
        assert!(seen.insert(h), "{h:?} twice");
    }
}

#[test]
fn a_city_has_a_manager_for_each_seven_citizens_or_part_up_to_four() {
    assert_eq!(MAX_CITY_POPULATION, 28);
    let allowed: Vec<usize> = [1, 7, 8, 14, 15, 21, 22, 28, 40]
        .into_iter()
        .map(managers_for)
        .collect();
    assert_eq!(allowed, [1, 1, 2, 2, 3, 3, 4, 4, 4]);
}

#[test]
fn auto_assign_fills_a_cluster_then_starts_the_next_on_the_best_open_land() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let routes = g.routes(0);
    for population in [7, 8, 15, 22, 28] {
        grown(&mut g, 0, population);
        let c = &g.cities[0];
        assert_eq!(c.clusters.len(), managers_for(population), "{population}");
        assert_eq!(c.remembered, c.clusters, "remembered as assigned");
        assert_clusters_hold(&g, 0);
        // A cluster is full, or has no open tile beside its manager, before
        // the next begins; and a citizen idles only when no cluster has
        // room and no new manager may start.
        let open_beside = |m: Hex| {
            m.neighbors()
                .into_iter()
                .any(|h| routes.costs.contains_key(&h) && g.is_open(0, h))
        };
        for cluster in &c.clusters[..c.clusters.len() - 1] {
            assert!(
                cluster.workers.len() == WORKERS_PER_MANAGER || !open_beside(cluster.manager),
                "{population}: {cluster:?}"
            );
        }
        if c.working() < population {
            assert!(
                c.clusters.iter().all(|cluster| {
                    cluster.workers.len() == WORKERS_PER_MANAGER || !open_beside(cluster.manager)
                }),
                "{population}"
            );
        }
    }
    // A new manager is the open land tile the order ranks best, clear of the
    // others; ties by coordinates.
    grown(&mut g, 0, 7);
    let first = g.cities[0].clusters.clone();
    g.cities[0].population = 8;
    g.reconcile_citizens(0);
    let c = &g.cities[0];
    assert_eq!(c.clusters[0], first[0], "the first cluster stays");
    let new = c.clusters[1].manager;
    let routes = g.routes(0);
    let food = worked_goods(&g, 0).0 - g.delivered_goods(0, new, routes.costs[&new]).0;
    let fed = g.is_fed(0, food);
    let score = |h: Hex| {
        c.priorities
            .score(g.delivered_goods(0, h, routes.costs[&h]), fed)
    };
    let best = routes
        .costs
        .keys()
        .filter(|&&h| {
            (h == new || g.is_open(0, h))
                && !g.grid.terrain(h).is_water()
                && first[0].manager.distance(h) > 1
        })
        .map(|&h| (-score(h), h.q, h.r))
        .min()
        .unwrap();
    assert_eq!((-score(new), new.q, new.r), best);
}

#[test]
fn clicks_add_a_worker_beside_a_manager_or_start_a_cluster_when_allowed() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fog_of_war = false;
    grown(&mut g, 0, 9);
    g.open_city(0);
    // Free a slot: release a worker of the first cluster.
    let released = g.cities[0].clusters[0].workers[0];
    g.city_click(released);
    assert_eq!(g.notice, "CITIZEN UNASSIGNED");
    assert_eq!(g.cities[0].working(), 8);
    assert!(!cluster_tiles(&g.cities[0].remembered).any(|h| h == released));
    // A tile beside no manager, with the managers the city may have: refused.
    let away = |g: &GameState, land: bool| {
        g.routes(0)
            .costs
            .keys()
            .copied()
            .filter(|&h| {
                g.is_open(0, h)
                    && (!land || !g.grid.terrain(h).is_water())
                    && g.cities[0]
                        .clusters
                        .iter()
                        .all(|c| c.manager.distance(h) > 2)
            })
            .min_by_key(|h| (h.q, h.r))
            .expect("an open tile away from the managers")
    };
    let far = away(&g, false);
    g.city_click(far);
    assert!(
        g.notice.contains("ANOTHER MANAGER AT 15 CITIZENS"),
        "{}",
        g.notice
    );
    assert!(!g.cities[0].works(far));
    // Back beside its manager, it's a worker again.
    g.city_click(released);
    assert_eq!(g.notice, "CITIZEN ASSIGNED");
    assert_eq!(g.cities[0].cluster_of(released), Some(0));
    // With a third manager allowed and a slot free, a click on open land
    // clear of the managers starts one; beside a manager, it can't.
    g.cities[0].population = 15;
    let clusters = g.cities[0].clusters.len();
    assert_eq!(clusters, 2);
    let beside = g.cities[0].clusters[1]
        .manager
        .neighbors()
        .into_iter()
        .find(|&h| {
            g.is_open(0, h) && !g.grid.terrain(h).is_water() && g.cluster_with_room(0, h).is_none()
        });
    if let Some(beside) = beside {
        g.city_click(beside);
        assert_eq!(
            g.notice, "A MANAGER WORKS LAND, NOT BESIDE ANOTHER MANAGER",
            "{beside:?}"
        );
    }
    let land_far = away(&g, true);
    g.city_click(land_far);
    assert_eq!(g.notice, "MANAGER ASSIGNED - A NEW CLUSTER");
    assert_eq!(g.cities[0].clusters.len(), clusters + 1);
    assert_eq!(g.cities[0].clusters.last().unwrap().manager, land_far);
    assert_eq!(g.cities[0].remembered, g.cities[0].clusters);
    assert_clusters_hold(&g, 0);
}

#[test]
fn moving_a_manager_takes_its_own_cluster_and_leaves_the_others() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fog_of_war = false;
    grown(&mut g, 0, 12);
    let before = g.cities[0].clusters.clone();
    assert_eq!(before.len(), 2);
    let old = before[1].manager;
    // Not beside the other manager, or onto another cluster's tile.
    assert!(!g.may_be_manager(0, 1, before[0].workers[0]));
    assert!(!g.may_be_manager(0, 1, before[0].manager));
    // Somewhere it may go (land in reach, clear of the first manager and
    // its cluster) where one of its workers' offsets lands on an open tile.
    let routes = g.routes(0);
    let shift_to = |to: Hex, h: Hex| Hex::new(h.q + to.q - old.q, h.r + to.r - old.r);
    let to = routes
        .costs
        .keys()
        .copied()
        .filter(|&h| {
            g.may_be_manager(0, 1, h)
                && !before[1].tiles().any(|t| t == h)
                && before[1].workers.iter().any(|&w| {
                    let t = shift_to(h, w);
                    routes.costs.contains_key(&t) && g.is_open(0, t)
                })
        })
        .min_by_key(|h| (h.distance(old), h.q, h.r))
        .expect("somewhere to move it");
    g.open_city(0);
    g.city_click(old);
    assert_eq!(g.moving_manager, Some((0, 1)));
    g.city_click(to);
    assert_eq!(g.moving_manager, None);
    let after = &g.cities[0].clusters;
    assert_eq!(after[0], before[0], "the first cluster stays put");
    assert_eq!(after[1].manager, to);
    // Its workers kept their offsets where they could.
    let shift = |h: Hex| Hex::new(h.q + to.q - old.q, h.r + to.r - old.r);
    let kept = before[1]
        .workers
        .iter()
        .filter(|&&w| after[1].workers.contains(&shift(w)))
        .count();
    assert!(kept > 0, "{before:?} -> {after:?}");
    assert_clusters_hold(&g, 0);
}

#[test]
fn a_city_losing_citizens_loses_the_last_clusters_workers_then_its_manager() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    // Three clusters, 7, 7 and 2 citizens: where doesn't matter here.
    let cluster = |q: i32, citizens: usize| Cluster {
        manager: Hex::new(q, 0),
        workers: Hex::new(q, 0).neighbors()[..citizens - 1].to_vec(),
    };
    g.cities[0].population = 16;
    g.cities[0].clusters = vec![cluster(-4, 7), cluster(0, 7), cluster(4, 2)];
    g.cities[0].remembered = g.cities[0].clusters.clone();
    let sizes = |g: &GameState| -> Vec<usize> {
        g.cities[0].clusters.iter().map(Cluster::citizens).collect()
    };
    let last_worker = g.cities[0].clusters[2].workers[0];
    assert!(g.remove_citizen(0));
    assert_eq!(sizes(&g), [7, 7, 1], "the last cluster's worker goes");
    assert!(!g.cities[0].works(last_worker));
    assert!(g.remove_citizen(0));
    assert_eq!(sizes(&g), [7, 7], "then its manager");
    assert!(g.remove_citizen(0));
    assert_eq!(sizes(&g), [7, 6], "then the next cluster's workers");
    assert_eq!(g.cities[0].population, 13);
    assert_eq!(g.cities[0].remembered, g.cities[0].clusters);
    // An idle citizen goes before any that works.
    g.cities[0].clusters[1].workers.pop();
    g.cities[0].remembered = g.cities[0].clusters.clone();
    assert!(g.remove_citizen(0));
    assert_eq!(sizes(&g), [7, 5]);
    // Starving uses the same rule, and never goes below one citizen.
    g.stockpiles[Team::Blue.index()].food = 0;
    g.feed_citizens();
    assert_eq!(g.cities[0].population, 11);
    assert_eq!(sizes(&g), [7, 4]);
    grown(&mut g, 0, 1);
    assert!(!g.remove_citizen(0));
    assert_eq!(sizes(&g), [1]);
}

#[test]
fn a_blocked_manager_is_stood_in_for_until_its_tile_clears() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    grown(&mut g, 0, 10);
    let first = g.cities[0].clusters[0].clone();
    // An enemy on the first manager's tile cuts it off: its first worker
    // that can manage stands in, and the cluster rules still hold.
    g.units
        .push(Unit::new(777, first.manager, Team::Red, UnitType::Melee));
    g.reconcile_citizens(0);
    assert!(!g.cities[0].works(first.manager));
    assert_clusters_hold(&g, 0);
    g.units.clear();
    g.reconcile_citizens(0);
    assert_eq!(g.cities[0].clusters[0], first, "back as it was");
    assert_clusters_hold(&g, 0);
}

#[test]
fn settlers_and_scouts_have_their_prices_and_the_city_queue_trains_them_at_full_pace() {
    assert_eq!(Build::Settler.price(), Stock::whole(30, 10, 0));
    assert_eq!(Build::Settler.turns(), 6);
    assert_eq!(Build::Scout.price(), Stock::whole(2, 4, 0));
    assert_eq!(Build::Scout.turns(), 2);
    let g = GameState::city_scenario();
    // Not troops: a city center isn't slower at them than a Barracks.
    for build in [Build::Settler, Build::Scout] {
        assert_eq!(g.city_build_turns(0, build), build.turns(), "{build:?}");
    }
}

#[test]
fn a_settler_needs_a_city_of_three_and_a_city_queues_one_scout_at_a_time() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    assert_eq!(g.cities[0].population, 2);
    g.queue_selected_city_settler();
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(
        g.notice,
        format!("SETTLER: NEEDS POPULATION {SETTLER_MIN_POPULATION}")
    );
    g.cities[0].population = SETTLER_MIN_POPULATION;
    g.queue_selected_city_settler();
    assert_eq!(builds(&g.cities[0].queue), [Build::Settler]);

    g.queue_selected_city_scout();
    g.queue_selected_city_scout();
    assert_eq!(builds(&g.cities[0].queue), [Build::Settler, Build::Scout]);
    assert_eq!(g.notice, "SCOUT: ONE SCOUT AT A TIME");
}

#[test]
fn a_finished_settler_takes_a_citizen_and_a_scout_takes_none() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].population = 3;
    g.auto_assign_city(0);
    assert_eq!(g.cities[0].working(), 3);
    g.cities[0].queue = vec![Queued::worked(Build::Settler, Build::Settler.work())];
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(g.cities[0].population, 2, "the settler took a citizen");
    assert_eq!(g.cities[0].working(), 2);
    let settler = g.units.last().unwrap();
    assert!(g.settlers.contains(&settler.id));
    assert_eq!(settler.team, Team::Blue);
    assert_eq!(settler.pos.distance(g.cities[0].pos), 1);

    g.cities[0].queue = vec![Queued::worked(Build::Scout, Build::Scout.work())];
    g.complete_builds();
    let scout = g.units.last().unwrap();
    assert_eq!(scout.unit_type, UnitType::Scout);
    assert!(!g.settlers.contains(&scout.id));
    assert_eq!(g.cities[0].population, 2);
}

#[test]
fn a_settler_below_the_population_it_needs_waits_and_keeps_its_work() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.fund(Team::Blue);
    // Paid and half done when the city starved down to two.
    let half = Build::Settler.work() / 2;
    g.cities[0].queue = vec![
        Queued::worked(Build::Settler, half),
        Queued::new(Build::Gather),
    ];
    assert!(g.waits_for_citizens(0, 0));
    let forecast = g.forecast(Team::Blue);
    let lane = forecast.lane(0, Lane::City).unwrap();
    assert_eq!(lane.worked, Some(1), "the queue works what comes next");
    assert!(
        g.waiting_items(lane).is_empty(),
        "not waiting for the stockpile"
    );
    g.work_queues(&[(WORK_PER_TURN, WORK_PER_TURN), (0, 0)]);
    assert_eq!(g.cities[0].queue[0], Queued::worked(Build::Settler, half));
    // Even finished, it waits for the citizen it takes.
    g.cities[0].queue[0].progress = Build::Settler.work();
    g.complete_builds();
    assert_eq!(builds(&g.cities[0].queue), [Build::Settler]);
    assert!(g.settlers.is_empty());
    // With a third citizen, out it comes.
    g.cities[0].population = 3;
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(g.settlers.len(), 1);
    assert_eq!(g.cities[0].population, 2);
}

/// The Cities scenario with a Blue settler selected on `pos`, and nothing
/// else on the map.
fn settler_on(pos: Hex) -> GameState {
    let mut g = GameState::city_scenario();
    g.units = vec![Unit::new(900, pos, Team::Blue, UnitType::Melee)];
    g.settlers.insert(900);
    g.selected = Some(0);
    g
}

#[test]
fn a_city_is_founded_six_hexes_from_any_other_and_without_a_worker() {
    assert_eq!(MIN_CITY_DISTANCE, 6);
    // Blue's city stands at (-4, 0), Red's at (4, 0).
    let (blue, red) = (Hex::new(-4, 0), Hex::new(4, 0));
    for (pos, near) in [(Hex::new(-4, 5), blue), (Hex::new(4, -5), red)] {
        assert_eq!(pos.distance(near), MIN_CITY_DISTANCE - 1);
        let mut g = settler_on(pos);
        g.found_city_selected();
        assert_eq!(g.cities.len(), 2, "{pos:?}: too close");
        assert!(g.notice.contains("TOO CLOSE"), "{}", g.notice);
        assert_eq!(g.settlers.len(), 1);
    }
    for (pos, near) in [(Hex::new(-4, 6), blue), (Hex::new(4, -6), red)] {
        assert_eq!(pos.distance(near), MIN_CITY_DISTANCE);
        let mut g = settler_on(pos);
        g.found_city_selected();
        assert_eq!(g.cities.len(), 3, "{pos:?}: {}", g.notice);
        let city = &g.cities[2];
        assert_eq!((city.pos, city.team, city.population), (pos, Team::Blue, 1));
        assert_eq!(city.workers, 0, "a new city starts without a worker");
        assert!(g.settlers.is_empty() && g.units.is_empty());
        assert_eq!(g.selected_city, Some(2));
    }
}

#[test]
fn a_side_s_first_city_comes_with_a_worker() {
    let mut g = GameState::frontier_scenario();
    g.found_city_selected();
    assert_eq!(g.cities.len(), 1);
    assert_eq!(g.cities[0].workers, 1);
}

#[test]
fn no_city_is_founded_on_water_or_ruins() {
    use crate::game::ruins::{Ruin, RuinReward};
    use crate::game::terrain::Terrain;
    let coast = Hex::new(0, 6);
    let mut g = lone_city_on(|h| {
        if h == coast {
            Tile {
                terrain: Terrain::Coast,
                hills: false,
                feature: None,
            }
        } else {
            Tile::default()
        }
    });
    g.ruins = vec![Ruin::new(Hex::new(6, 0), RuinReward::ALL[0])];
    for pos in [coast, Hex::new(6, 0)] {
        assert!(g.founding_issue(pos).is_some(), "{pos:?}");
    }
    assert_eq!(g.founding_issue(Hex::new(-6, 0)), None);
}
