use super::barracks::{CITY_TRAINING_SLOWDOWN, UNITS_PER_DEPOSIT};
use super::economy::{WORK_PER_TURN, grow_price};
use super::*;
use crate::game::map_icons::WOOD_ICON;
use crate::game::workers::{JobKind, WorkerJob};

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

#[test]
fn canoe_house_turns_its_connected_river_into_a_transport_corridor() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    let bank = Hex::new(-3, 0);
    let middle = Hex::new(-2, 0);
    let destination = Hex::new(-1, 0);
    g.grid = g.grid.clone().with_rivers(std::collections::HashSet::from([
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
    g.cities[0].barracks_queue = vec![BuildUnit::Cavalry];
    g.cities[0].barracks_progress = BuildUnit::Cavalry.work();
    g.complete_builds();
    let cavalry = g
        .units
        .iter()
        .find(|u| u.unit_type == UnitType::Cavalry)
        .unwrap();
    assert_eq!(cavalry.training_upgrade, Some(Resource::Horses));
    assert_eq!(cavalry.stats().move_range, 3);
    g.units.clear();
    g.cities[0].barracks_queue = vec![BuildUnit::Armored];
    g.cities[0].barracks_progress = BuildUnit::Armored.work();
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
    g.cities[0].worked.clear();
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
    g.cities[0].worked.clear();
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
    g.cities[0].worked.clear();
    let tile = Hex::new(-1, 0);
    g.roads.clear();
    g.cities[0].worked.push(tile);
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
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
    let before = g.stock(Team::Blue);
    let upkeep = g.upkeep(Team::Blue);
    g.end_planning();
    g.update(1.0);
    assert_eq!(g.cities[0].progress, WORK_PER_TURN, "one turn of work");
    assert_eq!(
        g.stock(Team::Blue),
        Stock {
            food: before.food + 12 - upkeep,
            wood: before.wood + 6,
            metal: before.metal
        }
    );
    g.update(10.0);
    assert_eq!(g.cities[0].progress, WORK_PER_TURN, "the economy ran once");
}

#[test]
fn builds_are_paid_when_queued_refunded_when_removed_and_refused_when_short() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let start = g.stock(Team::Blue);
    g.queue_selected_city_unit(BuildUnit::Melee);
    assert_eq!(g.cities[0].queue, vec![Build::Unit(BuildUnit::Melee)]);
    assert_eq!(g.stock(Team::Blue), start - BuildUnit::Melee.price());
    assert_eq!(g.stock(Team::Red), start, "only the buyer pays");
    g.remove_selected_city_queue_item(0);
    assert_eq!(g.stock(Team::Blue), start, "a full refund");

    // Short of wood: nothing is queued or spent, and the notice says why.
    g.stockpiles[Team::Blue.index()] = Stock::whole(20, 1, 0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(g.stock(Team::Blue), Stock::whole(20, 1, 0));
    assert!(
        g.notice.contains(&format!("SHORT OF {WOOD_ICON}5")),
        "{}",
        g.notice
    );
    // Broke, a city still has something to do: gather, for free.
    g.stockpiles[Team::Blue.index()] = Stock::default();
    assert!(g.city_needs_build(0));
    g.queue_selected_city_gather();
    assert_eq!(g.cities[0].queue, vec![Build::Gather]);
    assert_eq!(g.stock(Team::Blue), Stock::default());
    assert!(!g.city_needs_build(0));
}

#[test]
fn a_build_takes_its_fixed_turns_whatever_the_city_produces() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let siege = Build::Unit(BuildUnit::Siege);
    g.cities[0].queue = vec![siege];
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
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
    g.resolve_economy();
    assert_eq!(g.cities[0].progress, WORK_PER_TURN + production / 4);
    g.production_speedup = false;
    g.resolve_economy();
    assert_eq!(
        g.cities[0].progress,
        2 * WORK_PER_TURN + production / 4,
        "off, a turn does its fixed work"
    );
}

#[test]
fn growth_is_bought_with_food_and_takes_its_turns() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let population = g.cities[0].population;
    g.stockpiles[Team::Blue.index()].food = 1000;
    g.queue_selected_city_growth();
    g.queue_selected_city_growth();
    // The second Grow is priced for the bigger city the first makes.
    assert_eq!(
        g.stock(Team::Blue).food,
        1000 - grow_price(population).food - grow_price(population + 1).food
    );
    // Removing either refunds the dearer price: the one left grows the
    // city from its size now.
    g.remove_selected_city_queue_item(0);
    assert_eq!(g.stock(Team::Blue).food, 1000 - grow_price(population).food);
    for _ in 0..Build::Grow.turns() {
        assert_eq!(g.cities[0].population, population, "not grown yet");
        g.resolve_economy();
    }
    assert_eq!(g.cities[0].population, population + 1);
    assert_eq!(
        g.cities[0].worked.len(),
        population + 1,
        "the citizen works"
    );

    // No Grow past the cap, counting those queued.
    g.cities[0].population = MAX_CITY_POPULATION - 1;
    g.queue_selected_city_growth();
    assert_eq!(g.cities[0].queue, vec![Build::Grow]);
    g.queue_selected_city_growth();
    assert_eq!(g.cities[0].queue, vec![Build::Grow], "full");
    assert!(!g.can_grow(0));
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
    g.cities[0].worked.clear();
    let manager = Hex::new(-1, 0);
    let nearby = Hex::new(-1, 1);
    let distant = Hex::new(-3, 0);
    assert!(g.may_assign(0, manager));
    g.cities[0].worked.push(manager);
    assert!(g.may_assign(0, nearby));
    assert!(!g.may_assign(0, distant));
}

#[test]
fn blocked_worked_tile_is_restored_after_the_unit_leaves() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let manager = Hex::new(-2, 0);
    let blocked_worker = Hex::new(-1, 0);
    g.cities[0].worked = vec![manager, blocked_worker];
    g.cities[0].remembered_worked = g.cities[0].worked.clone();
    g.units
        .push(Unit::new(777, blocked_worker, Team::Red, UnitType::Melee));

    g.reconcile_citizens(0);
    assert!(!g.cities[0].worked.contains(&blocked_worker));
    assert!(g.cities[0].remembered_worked.contains(&blocked_worker));

    g.units.clear();
    g.reconcile_citizens(0);
    assert!(g.cities[0].worked.contains(&blocked_worker));
}
#[test]
fn assignments_obey_population() {
    let mut g = GameState::city_scenario();
    g.auto_assign_city(0);
    assert_eq!(g.cities[0].worked.len(), 2);
    assert!(!g.may_assign(1, g.cities[0].worked[0]));
}

#[test]
fn frontier_settler_founds_city_and_city_spends_production_on_unit() {
    let mut g = GameState::frontier_scenario();
    g.found_city_selected();
    assert_eq!(g.cities.len(), 1);
    assert_eq!(g.settlers.len(), 1, "the opposing settler remains");
    g.queue_selected_city_unit(BuildUnit::Melee);
    g.cities[0].progress = g.city_build_work(0, Build::Unit(BuildUnit::Melee));
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
    g.cities[0].progress = Build::Gather.work();
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
    g.cities[0].worked = vec![manager, worker];
    g.cities[0].remembered_worked = g.cities[0].worked.clone();
    g.cities[0].barracks = Some(manager);
    g.cities[0].barracks_queue = vec![BuildUnit::Siege];
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
    g.resolve_economy();
    assert_eq!(g.cities[0].barracks_progress, WORK_PER_TURN);
    assert_eq!(
        g.cities[0].progress, WORK_PER_TURN,
        "the city and Barracks both work"
    );
    assert_eq!(
        g.city_build_work(0, Build::Unit(BuildUnit::Siege)),
        CITY_TRAINING_SLOWDOWN * BuildUnit::Siege.work(),
        "but the city needs twice the work"
    );

    // The manager elsewhere doesn't pause it.
    g.cities[0].worked.swap(0, 1);
    g.cities[0].remembered_worked = g.cities[0].worked.clone();
    g.resolve_economy();
    assert_eq!(g.cities[0].barracks_progress, 2 * WORK_PER_TURN);
    g.cities[0].barracks_progress = WORK_PER_TURN;

    // With production speeding builds, the Barracks adds what's delivered
    // to it, with its own delivery falloff.
    g.cities[0].worked.swap(0, 1);
    g.cities[0].remembered_worked = g.cities[0].worked.clone();
    g.production_speedup = true;
    let barracks_income = g.barracks_income(0);
    assert!(barracks_income > 0);
    g.resolve_economy();
    assert_eq!(
        g.cities[0].barracks_progress,
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
                && !g.cities.iter().any(|c| c.worked.contains(&h))
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
    g.cities[0].barracks_queue = vec![BuildUnit::Melee];
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
fn city_queue_completes_in_order_and_can_be_reordered_or_removed() {
    let mut g = GameState::city_scenario();
    g.fund(Team::Blue);
    g.units.clear();
    g.selected_city = Some(0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    g.queue_selected_city_unit(BuildUnit::Ranged);
    assert_eq!(
        g.cities[0].queue,
        vec![
            Build::Unit(BuildUnit::Melee),
            Build::Unit(BuildUnit::Ranged)
        ]
    );
    g.move_selected_city_queue_item(1, true);
    assert_eq!(g.cities[0].queue[0], Build::Unit(BuildUnit::Ranged));
    g.remove_selected_city_queue_item(1);
    assert_eq!(g.cities[0].queue, vec![Build::Unit(BuildUnit::Ranged)]);
    g.cities[0].progress = g.city_build_work(0, Build::Unit(BuildUnit::Ranged));
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
    assert_eq!(g.cities[0].queue, vec![Build::Unit(BuildUnit::Melee)]);
}

#[test]
fn mill_restores_food_delivery_only_within_city_reach() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    let worked = Hex::new(-1, 0);
    assert_eq!(g.routes(0).costs.get(&worked), Some(&6));
    g.cities[0].worked = vec![worked];
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
    g.cities[0].worked = vec![out_of_reach];
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
    g.cities[0].queue = vec![Build::Gather];
    g.cities[1].queue = vec![Build::Gather, Build::Gather];
    g.cities[1].progress = 0;
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
    g.cities[0].barracks_queue = vec![BuildUnit::Melee];
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
    g.cities[0].worked = vec![site];
    g.queue_selected_barracks_unit(BuildUnit::Melee);
    g.queue_selected_barracks_unit(BuildUnit::Ranged);
    g.move_selected_barracks_queue_item(1, true);
    assert_eq!(g.cities[0].barracks_queue[0], BuildUnit::Ranged);
    g.cities[0].barracks_progress = BuildUnit::Ranged.work();
    g.complete_builds();
    assert_eq!(g.cities[0].barracks_queue, vec![BuildUnit::Melee]);
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
    assert_eq!(g.cities[0].barracks_queue, vec![BuildUnit::Cavalry]);
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
    while let Some(&build) = g.cities[0].barracks_queue.first() {
        g.cities[0].barracks_progress = build.work();
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
    let special = g.cities[red].barracks_queue.first().copied();
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
    g.cities[0].worked = vec![manager];
    g.cities[0].remembered_worked = vec![manager, center, barracks];
    g.reconcile_citizens(0);
    let worked = &g.cities[0].worked;
    assert_eq!(worked[0], manager);
    assert!(!worked.contains(&center), "{worked:?}");
    assert!(!worked.contains(&barracks), "{worked:?}");
    assert!(worked.len() > 1, "the others found open tiles");
    // Nor by hand, nor as the manager.
    assert!(!g.may_assign(0, center) && !g.may_assign(0, barracks));
    assert!(!g.may_be_manager(0, barracks));
    g.open_city(0);
    g.city_click(barracks);
    assert!(!g.cities[0].worked.contains(&barracks));
    assert_eq!(g.notice, "A BUILDING STANDS THERE - NO CITIZEN CAN WORK IT");
    // A city center's yield comes in whoever works what.
    g.cities[0].worked.clear();
    assert_eq!(g.income(0).food, 8, "2 food from the center alone");
}

#[test]
fn city_menu_keeps_barracks_clicks_in_manager_assignment_context() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.explore();
    let manager = Hex::new(-2, 0);
    let barracks = Hex::new(-1, 0);
    g.cities[0].worked = vec![manager];
    g.cities[0].barracks = Some(barracks);
    g.open_city(0);

    g.city_click(manager);
    assert_eq!(g.moving_manager, Some(0));
    g.city_click(barracks);

    // A building covers its tile: the manager can't go there, and the
    // click stays with the city rather than opening the Barracks.
    assert_eq!(g.cities[0].worked.first(), Some(&manager));
    assert_eq!(g.moving_manager, Some(0));
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
    g.cities[0].worked = vec![Hex::new(-3, 0)];
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Melee); 6];
    g.cities[0].progress = 0;
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
        g.cities[0].progress,
        earned - cost,
        "the leftover carries over"
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
        g.cities[0].progress, cost,
        "the waiting unit is paid for, and nothing more is banked"
    );

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
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Melee)];
    g.cities[0].progress = g.city_build_work(0, Build::Unit(BuildUnit::Melee));
    g.cities[0].barracks_queue = vec![BuildUnit::Ranged];
    g.cities[0].barracks_progress = BuildUnit::Ranged.work();
    g.complete_builds();
    assert_eq!(g.units_at(open).count(), 1, "one unit on the open hex");
    assert_eq!(
        g.cities[0].barracks_queue,
        vec![BuildUnit::Ranged],
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
            .contains(&Build::Unit(BuildUnit::PatrolGalley))
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
            .contains(&Build::Unit(BuildUnit::LandingCraft))
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
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Melee)];
    g.cities[0].progress = g.city_build_work(0, Build::Unit(BuildUnit::Melee));
    g.complete_builds();
    assert_eq!(
        g.units_at(open[0]).count(),
        0,
        "the worker's hex stays clear"
    );
    assert_eq!(g.cities[0].queue.len(), 1, "the troop waits");
}
