use super::*;
use crate::game::terrain::Terrain;

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
fn economy_ticks_once_with_no_units_and_preserves_quarters() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].worked.clear();
    let tile = Hex::new(-1, 0);
    g.roads.clear();
    g.cities[0].worked.push(tile);
    assert_eq!(g.income(0), (12, 6));
    // The turn waits until the city has something to build; siege costs
    // more than one turn's production, so none is spent.
    g.end_planning();
    assert!(!g.is_resolving());
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
    g.end_planning();
    g.update(1.0);
    assert_eq!(g.cities[0].production, 6);
    g.update(10.0);
    assert_eq!(g.cities[0].production, 6);
}

#[test]
fn active_build_accumulates_production_and_population_is_capped_at_seven() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].worked = vec![Hex::new(-1, 0)];
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
    g.resolve_economy();
    assert!(g.cities[0].production > 0);

    g.cities[0].population = MAX_CITY_POPULATION;
    g.cities[0].food = 10_000;
    g.resolve_economy();
    assert_eq!(g.cities[0].population, MAX_CITY_POPULATION);
}

/// A land tile with no road, city or unit that a worker can improve.
fn open_tile(g: &GameState) -> Hex {
    g.grid
        .all_hexes()
        .find(|&h| {
            g.grid.is_passable(h)
                && g.grid.tile(h).terrain != Terrain::Snow
                && !g.roads.contains(&h)
                && g.cities.iter().all(|c| c.pos != h)
                && g.units.iter().all(|u| u.pos != h)
        })
        .unwrap()
}

/// Selects the player's worker, moved to `at`.
fn select_worker_at(g: &mut GameState, at: Hex) -> usize {
    let i = g
        .units
        .iter()
        .position(|u| u.team == PLAYER_TEAM && g.workers.contains(&u.id))
        .unwrap();
    g.units[i].pos = at;
    g.selected = Some(i);
    i
}

#[test]
fn worker_actions_wait_for_the_turn_to_finish_playing() {
    use super::super::turn::Phase;
    let mut g = GameState::city_scenario();
    let at = open_tile(&g);
    g.sites.remove(&at);
    select_worker_at(&mut g, at);
    g.pending_steps.push_back((UnitType::Melee, Phase::Move));
    assert!(g.is_resolving());
    g.build_worker_road_selected();
    g.improve_worker_tile_selected();
    assert!(!g.roads.contains(&at));
    assert!(!g.sites.contains_key(&at));

    g.pending_steps.clear();
    g.build_worker_road_selected();
    g.improve_worker_tile_selected();
    assert!(g.roads.contains(&at));
    assert_eq!(g.sites[&at].team, PLAYER_TEAM);
}

#[test]
fn a_worker_cannot_improve_over_an_enemy_site() {
    let mut g = GameState::city_scenario();
    let at = open_tile(&g);
    select_worker_at(&mut g, at);
    g.sites.insert(
        at,
        Site {
            team: Team::Red,
            food: 3,
            production: 0,
            label: "FARM",
        },
    );
    g.improve_worker_tile_selected();
    assert_eq!(g.sites[&at].team, Team::Red);
    assert_eq!(g.sites[&at].food, 3);
}

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
fn growth_starvation_and_assignments_obey_population() {
    let mut g = GameState::city_scenario();
    g.cities[0].food = 100;
    g.resolve_economy();
    assert_eq!(g.cities[0].population, 3);
    g.cities[0].food = -100;
    g.resolve_economy();
    assert_eq!(
        g.cities[0].population, 2,
        "starvation loses at most one population per turn"
    );
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
    g.cities[0].production = BuildUnit::Melee.cost();
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    let finished = g.units.iter().find(|u| u.team == Team::Blue).unwrap();
    assert_eq!(finished.unit_type, UnitType::Melee);
    assert_eq!(finished.pos.distance(g.cities[0].pos), 1);
}

#[test]
fn granary_is_unique_and_adds_two_food_per_turn() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.cities[0].worked.clear();
    assert_eq!(g.income(0).0, 8);
    g.cities[0].queue = vec![Build::Building(Building::Granary)];
    g.cities[0].production = Building::Granary.cost();
    g.complete_builds();
    assert!(g.cities[0].built.contains(&Building::Granary));
    assert_eq!(g.income(0).0, 16);
    g.selected_city = Some(0);
    g.queue_selected_city_building(Building::Granary);
    assert!(g.cities[0].queue.is_empty());
}

#[test]
fn barracks_has_its_own_delivery_falloff_when_manager_is_on_site() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let manager = Hex::new(-1, 0);
    let worker = Hex::new(-1, 1);
    g.cities[0].worked = vec![manager, worker];
    g.cities[0].barracks = Some(manager);
    g.cities[0].barracks_queue = vec![BuildUnit::Melee];
    g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
    let (_, total_production) = g.income(0);
    let barracks_production = g.barracks_income(0);
    g.resolve_economy();
    assert_eq!(g.cities[0].barracks_production, barracks_production);
    assert_eq!(
        g.cities[0].production, total_production,
        "the city and Barracks both receive active group production"
    );

    g.cities[0].worked.swap(0, 1);
    let stored = g.cities[0].barracks_production;
    g.resolve_economy();
    assert_eq!(g.cities[0].barracks_production, stored);
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
fn barracks_site_can_be_chosen_before_completion_and_needs_confirmation() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    g.queue_selected_city_building(Building::Barracks);
    let site = Hex::new(-2, 0);
    assert!(g.grid.is_passable(site));
    g.city_click(site);
    assert_eq!(
        g.cities[0].planned_sites.get(&Building::Barracks).copied(),
        Some(site)
    );
    assert_eq!(
        g.placing_building, None,
        "choosing a site exits Barracks placement mode"
    );
    let normal_city_click = Hex::new(-1, 0);
    g.city_click(normal_city_click);
    assert_ne!(
        g.cities[0].planned_sites.get(&Building::Barracks).copied(),
        Some(normal_city_click),
        "a later city click must not move the planned Barracks"
    );
    g.cities[0].production = Building::Barracks.cost();
    g.complete_builds();
    assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
    assert!(g.cities[0].barracks.is_none());
    g.confirm_building(Building::Barracks);
    assert_eq!(g.cities[0].barracks, Some(site));
}

#[test]
fn a_barracks_may_stand_on_an_unworked_tile_as_its_card_says() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let unworked = g
        .grid
        .all_hexes()
        .find(|&h| {
            !g.cities.iter().any(|c| c.worked.contains(&h))
                && g.site_available(0, Building::Barracks, h)
        })
        .expect("an open, unworked land tile");
    g.queue_selected_city_building(Building::Barracks);
    g.city_click(unworked);
    assert_eq!(
        g.cities[0].planned_sites.get(&Building::Barracks),
        Some(&unworked)
    );
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
    g.selected_city = Some(0);
    g.queue_selected_city_building(Building::Barracks);
    assert_eq!(
        g.cities[0].queue.last(),
        Some(&Build::Building(Building::Barracks))
    );
    let site = Hex::new(-1, 0);
    g.city_click(site);
    g.cities[0]
        .queue
        .retain(|&b| b == Build::Building(Building::Barracks));
    g.cities[0].production = Building::Barracks.cost();
    g.complete_builds();
    g.confirm_building(Building::Barracks);
    assert_eq!(g.cities[0].barracks, Some(site));
    assert_eq!(g.cities[0].barracks_hp, BARRACKS_MAX_HP);
}

#[test]
fn city_queue_completes_in_order_and_can_be_reordered_or_removed() {
    let mut g = GameState::city_scenario();
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
    g.cities[0].production = BuildUnit::Ranged.cost();
    g.complete_builds();
    assert!(g.cities[0].queue.is_empty());
    assert!(
        g.units
            .iter()
            .any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged)
    );
}

#[test]
fn removing_a_queued_barracks_clears_its_placement_preview() {
    let mut g = GameState::city_scenario();
    g.selected_city = Some(0);
    g.queue_selected_city_building(Building::Barracks);
    g.city_click(Hex::new(-2, 0));
    assert!(
        g.cities[0]
            .planned_sites
            .get(&Building::Barracks)
            .copied()
            .is_some()
    );
    g.remove_selected_city_queue_item(0);
    assert!(
        g.cities[0]
            .planned_sites
            .get(&Building::Barracks)
            .copied()
            .is_none()
    );
    assert_eq!(g.placing_building, None);
}

#[test]
fn queued_barracks_opens_site_selection_behind_another_build() {
    let mut g = GameState::city_scenario();
    g.selected_city = Some(0);
    g.queue_selected_city_unit(BuildUnit::Melee);
    g.queue_selected_city_building(Building::Barracks);
    assert_eq!(g.placing_building, Some((0, Building::Barracks)));
    let site = Hex::new(-2, 0);
    g.city_click(site);
    assert_eq!(
        g.cities[0].planned_sites.get(&Building::Barracks).copied(),
        Some(site)
    );
    assert_eq!(g.placing_building, None);
}

#[test]
fn planned_barracks_site_can_change_before_final_confirmation() {
    let mut g = GameState::city_scenario();
    g.selected_city = Some(0);
    g.queue_selected_city_building(Building::Barracks);
    let original = Hex::new(-2, 0);
    let revised = Hex::new(-1, 0);
    g.city_click(original);
    g.change_selected_building_site(Building::Barracks);
    assert_eq!(g.placing_building, Some((0, Building::Barracks)));
    g.city_click(revised);
    assert_eq!(
        g.cities[0].planned_sites.get(&Building::Barracks).copied(),
        Some(revised)
    );
    assert!(g.cities[0].barracks.is_none());
}

#[test]
fn mill_restores_food_delivery_only_within_city_reach() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.roads.clear();
    let worked = Hex::new(-1, 0);
    assert_eq!(g.routes(0).costs.get(&worked), Some(&6));
    g.cities[0].worked = vec![worked];
    let food_before = g.income(0).0;
    let food_yield = g.tile_yield(worked).0;
    g.cities[0].mill = Some(Hex::new(-2, 0));
    assert_eq!(g.income(0).0, food_before + food_yield * 2);

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
        g.income(0).0,
        8,
        "a mill cannot bypass the hard route cutoff"
    );
}

#[test]
fn workshop_allows_early_confirmation_of_adjacent_buildings() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let workshop = Hex::new(-2, 0);
    let barracks = Hex::new(-1, 0);
    assert!(g.site_available(0, Building::Workshop, workshop));
    assert!(g.site_available(0, Building::Barracks, barracks));
    g.cities[0].workshop = Some(workshop);
    g.queue_selected_city_building(Building::Barracks);
    g.city_click(barracks);
    assert_eq!(
        g.city_build_cost(0, Build::Building(Building::Barracks)),
        Building::Barracks.cost() / 2
    );
    g.cities[0].production = Building::Barracks.cost() / 2 - 1;
    g.confirm_building(Building::Barracks);
    assert_eq!(g.cities[0].barracks, None);
    g.cities[0].production += 1;
    g.confirm_building(Building::Barracks);
    assert_eq!(g.cities[0].barracks, Some(barracks));
    assert!(g.cities[0].queue.is_empty());
    assert_eq!(g.cities[0].production, 0);
}

#[test]
fn moving_a_half_built_site_away_from_workshop_resumes_construction() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let workshop = Hex::new(-2, 0);
    let adjacent = Hex::new(-1, 0);
    let distant = g
        .grid
        .all_hexes()
        .find(|&hex| hex.distance(workshop) > 1 && g.site_available(0, Building::Barracks, hex))
        .expect("open site outside workshop reach");
    g.cities[0].workshop = Some(workshop);
    g.queue_selected_city_building(Building::Barracks);
    g.city_click(adjacent);
    let half = Building::Barracks.cost() / 2;
    g.cities[0].production = half;
    g.complete_builds();
    assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
    assert_eq!(
        g.cities[0].queue.first(),
        Some(&Build::Building(Building::Barracks))
    );
    g.change_selected_building_site(Building::Barracks);
    g.city_click(distant);
    assert_eq!(g.cities[0].pending_building, None);
    assert_eq!(g.cities[0].production, half);
    assert_eq!(
        g.city_build_cost(0, Build::Building(Building::Barracks)),
        Building::Barracks.cost()
    );
    g.confirm_building(Building::Barracks);
    assert_eq!(g.cities[0].barracks, None);
    assert_eq!(
        g.cities[0].queue.first(),
        Some(&Build::Building(Building::Barracks))
    );
    g.cities[0].production = Building::Barracks.cost();
    g.complete_builds();
    assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
    g.confirm_building(Building::Barracks);
    assert_eq!(g.cities[0].barracks, Some(distant));
    assert!(g.cities[0].queue.is_empty());
}

#[test]
fn placed_buildings_cannot_share_a_site() {
    let mut g = GameState::city_scenario();
    g.selected_city = Some(0);
    let site = Hex::new(-2, 0);
    g.cities[0].mill = Some(site);
    g.queue_selected_city_building(Building::Workshop);
    g.city_click(site);
    assert!(!g.cities[0].planned_sites.contains_key(&Building::Workshop));
    assert_eq!(g.placing_building, Some((0, Building::Workshop)));
}

#[test]
fn debug_completion_only_finishes_the_selected_production_lane() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    g.queue_selected_city_building(Building::Mill);
    g.city_click(Hex::new(-2, 0));
    g.cities[1].queue = vec![Build::Building(Building::Granary)];
    g.cities[1].production = Building::Granary.cost();
    g.debug_complete_current_production();
    assert_eq!(g.cities[0].pending_building, Some(Building::Mill));
    assert_eq!(g.cities[1].queue, vec![Build::Building(Building::Granary)]);
    assert!(!g.cities[1].built.contains(&Building::Granary));

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
    assert_eq!(g.cities[0].pending_building, Some(Building::Mill));
}

#[test]
fn barracks_queue_is_independent_and_completes_in_order() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    g.selected_city = Some(0);
    let site = Hex::new(-1, 0);
    g.cities[0].barracks = Some(site);
    g.cities[0].worked = vec![site];
    g.queue_selected_barracks_unit(BuildUnit::Melee);
    g.queue_selected_barracks_unit(BuildUnit::Ranged);
    g.move_selected_barracks_queue_item(1, true);
    assert_eq!(g.cities[0].barracks_queue[0], BuildUnit::Ranged);
    g.cities[0].barracks_production = BuildUnit::Ranged.cost();
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

#[test]
fn structure_menu_can_be_dismissed_without_selecting_a_unit() {
    let mut g = GameState::city_scenario();
    g.open_city(0);
    assert!(g.exit_structure_menu());
    assert_eq!(g.selected_city, None);
    assert!(!g.exit_structure_menu());
}

#[test]
fn city_menu_keeps_barracks_clicks_in_manager_assignment_context() {
    let mut g = GameState::city_scenario();
    g.units.clear();
    let manager = Hex::new(-2, 0);
    let barracks = Hex::new(-1, 0);
    g.cities[0].worked = vec![manager];
    g.cities[0].barracks = Some(barracks);
    g.open_city(0);

    g.city_click(manager);
    assert_eq!(g.moving_manager, Some(0));
    g.city_click(barracks);

    assert_eq!(g.cities[0].worked.first(), Some(&barracks));
    assert_eq!(g.selected_city, Some(0));
    assert_eq!(g.selected_barracks, None);
}
