use super::builder::{ButtonSpec, Row};
use super::text::{end_turn_label, signed_quantity, wrap};
use super::*;

use crate::game::PLAYER_TEAM;
use crate::game::city::Build;
use crate::game::orders::ClickMode;
use crate::game::unit::{Team, Unit, UnitType};

const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

#[test]
fn hovering_a_button_shows_its_tooltip() {
    let mut game = GameState::city_scenario();
    game.select_city();
    let card = button_cursor(&game, Target::Build(BuildUnit::Siege));
    let plain = game.build_ui(SCREEN, None).len();
    assert!(game.build_ui(SCREEN, Some(card)).len() > plain);
}

#[test]
fn hovering_an_enemy_describes_it() {
    let mut game = GameState::new();
    game.fog_of_war = false;
    let enemy = game.units.iter().find(|u| u.team == Team::Red).unwrap();
    let empty = hex_cursor(&game, Hex::new(0, 1));
    let plain = game.build_ui(SCREEN, Some(empty)).len();
    let hovered = game.build_ui(SCREEN, Some(hex_cursor(&game, enemy.pos)));
    assert!(hovered.len() > plain);
}

#[test]
fn hovering_an_enemy_out_of_sight_shows_nothing() {
    let game = GameState::new();
    let enemy = game.units.iter().find(|u| u.team == Team::Red).unwrap();
    assert!(
        !game.fog().sees(enemy.pos),
        "the combat map starts with Red unseen"
    );
    let empty = hex_cursor(&game, Hex::new(0, 1));
    let plain = game.build_ui(SCREEN, Some(empty)).len();
    let hovered = game.build_ui(SCREEN, Some(hex_cursor(&game, enemy.pos)));
    assert_eq!(hovered.len(), plain);
}

#[test]
fn ui_projection_puts_the_origin_at_the_bottom_left() {
    let clip = |p: Vec2| {
        ui_projection(SCREEN)
            .project_point3(p.extend(0.0))
            .truncate()
    };
    // Vulkan clip space has Y = +1 at the bottom of the window.
    assert!(clip(Vec2::ZERO).abs_diff_eq(Vec2::new(-1.0, 1.0), 1e-5));
    assert!(clip(SCREEN).abs_diff_eq(Vec2::new(1.0, -1.0), 1e-5));
}

/// Where to click, in window pixels, to press the button for `target`.
fn button_cursor(game: &GameState, target: Target) -> Vec2 {
    let layout = game.layout(SCREEN);
    let button = layout
        .buttons
        .iter()
        .find(|b| b.target == target)
        .expect("button shown");
    to_ui((button.min + button.max) / 2.0, SCREEN)
}

/// Where `hex` is drawn, in window pixels.
fn hex_cursor(game: &GameState, hex: Hex) -> Vec2 {
    let camera = &game.camera;
    let offset = (hex.to_world() - camera.center) / camera.half_height;
    let ndc = Vec2::new(offset.x * SCREEN.y / SCREEN.x, -offset.y);
    (ndc + 1.0) / 2.0 * SCREEN
}

fn find(game: &GameState, unit_type: UnitType) -> usize {
    game.units
        .iter()
        .position(|u| u.team == Team::Blue && u.unit_type == unit_type)
        .unwrap()
}

#[test]
fn attack_button_arms_a_square_attack_for_the_next_click() {
    let mut game = GameState::new();
    let melee = find(&game, UnitType::Melee);
    assert_eq!(game.selected, Some(melee));
    // An empty hex the melee could otherwise move to.
    let target = Hex::new(-1, 0);

    let attack = Target::Unit(UnitAction::Attack);
    game.handle_click(button_cursor(&game, attack), SCREEN, ClickMode::Normal);
    assert_eq!(game.ui_click_mode, Some(ClickMode::Attack));
    game.handle_click(hex_cursor(&game, target), SCREEN, ClickMode::Normal);
    assert_eq!(game.units[melee].planned_attack, Some(target));
    assert_eq!(game.units[melee].planned_move, None);
    assert_eq!(game.ui_click_mode, None, "armed for one click only");
}

#[test]
fn pressing_an_armed_button_again_disarms_it() {
    let mut game = GameState::new();
    let swap = Target::Unit(UnitAction::Swap);
    game.handle_click(button_cursor(&game, swap), SCREEN, ClickMode::Normal);
    assert_eq!(game.ui_click_mode, Some(ClickMode::Swap));
    game.handle_click(button_cursor(&game, swap), SCREEN, ClickMode::Normal);
    assert_eq!(game.ui_click_mode, None);
}

#[test]
fn hold_button_holds_the_selected_unit() {
    let mut game = GameState::new();
    let first = game.selected.unwrap();
    let hold = Target::Unit(UnitAction::Hold);
    game.handle_click(button_cursor(&game, hold), SCREEN, ClickMode::Normal);
    assert!(game.units[first].holding);
    assert_ne!(game.selected, Some(first));
}

#[test]
fn debug_panel_saves_loads_and_switches_scenarios() {
    let mut game = GameState::new();
    let unit = game.selected.unwrap();
    let start = game.units[unit].pos;
    let load = Target::LoadState;
    let load_state = |game: &GameState| {
        let layout = game.layout(SCREEN);
        layout
            .buttons
            .iter()
            .find(|b| b.target == load)
            .unwrap()
            .state
    };
    assert_eq!(
        load_state(&game),
        ButtonState::Disabled,
        "nothing saved yet"
    );

    game.handle_click(
        button_cursor(&game, Target::SaveState),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(load_state(&game), ButtonState::Ready);
    game.units[unit].pos = Hex::new(0, 1);
    game.handle_click(button_cursor(&game, load), SCREEN, ClickMode::Normal);
    assert_eq!(game.units[unit].pos, start);

    let cities = Target::Scenario(Scenario::Cities);
    game.handle_click(button_cursor(&game, cities), SCREEN, ClickMode::Normal);
    assert_eq!(game.scenario, Scenario::Cities);
}

#[test]
fn build_card_queues_its_unit() {
    let mut game = GameState::city_scenario();
    game.select_city();
    let siege = Target::Build(BuildUnit::Siege);
    game.handle_click(button_cursor(&game, siege), SCREEN, ClickMode::Normal);
    let city = game.selected_city.unwrap();
    assert_eq!(game.cities[city].queue, vec![Build::Unit(BuildUnit::Siege)]);
}

#[test]
fn city_interior_map_fits_and_its_tiles_issue_orders() {
    let mut game = GameState::siege_scenario();
    let layout = game.layout(SCREEN);
    for tile in [
        Hex::new(-2, 0),
        Hex::new(-1, 0),
        Hex::new(0, 0),
        Hex::new(2, 0),
    ] {
        let cursor = hex_cursor(&game, tile);
        assert!(cursor.x > 0.0 && cursor.x < SCREEN.x);
        assert!(cursor.y > 0.0 && cursor.y < SCREEN.y);
        assert!(
            !layout.covers(to_ui(cursor, SCREEN)),
            "interior tile hidden by a panel"
        );
    }
    game.handle_click(
        hex_cursor(&game, Hex::new(-2, 0)),
        SCREEN,
        ClickMode::Normal,
    );
    game.handle_click(
        hex_cursor(&game, Hex::new(-1, 0)),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(
        game.cities[1]
            .interior
            .fighters
            .iter()
            .find(|f| f.source_id == 0)
            .unwrap()
            .planned_move,
        Some(Hex::new(-1, 0))
    );
}

#[test]
fn breached_post_panel_explicitly_prompts_occupation() {
    let mut game = GameState::siege_scenario();
    game.cities[1].interior.core_hp = 0.0;
    let text = panel_strings(|panel| game.interior_tray(1, panel));
    assert_shows(&text, "POST BREACHED  0/80 HP");
    assert_shows(&text, "MOVE A BLUE TROOP ONTO THE POST TO CAPTURE");

    game.cities[0].interior.core_hp = 0.0;
    let home = panel_strings(|panel| game.interior_tray(0, panel));
    assert_shows(&home, "KEEP RED OFF THE CENTER TO PREVENT CAPTURE");
}

/// The city scenario with the player's city open and the camera settled on it.
fn city_view() -> GameState {
    let mut game = GameState::city_scenario();
    game.select_city();
    game.update(10.0);
    game
}

#[test]
fn barracks_map_click_locks_site_and_exits_placement() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    let button = button_cursor(&game, Target::Building(Building::Barracks));
    game.handle_click(button, SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_building, Some((city, Building::Barracks)));

    // Use visible map tiles so this exercises UI hit testing as well as
    // city placement, rather than calling city_click directly.
    let sites: Vec<_> = (-8..=8)
        .flat_map(|q| (-8..=8).map(move |r| Hex::new(q, r)))
        .filter(|&hex| {
            let cursor = hex_cursor(&game, hex);
            game.grid.is_passable(hex)
                && !game.cities.iter().any(|c| c.pos == hex)
                && (0.0..SCREEN.x).contains(&cursor.x)
                && (0.0..SCREEN.y).contains(&cursor.y)
                && !game.layout(SCREEN).covers(to_ui(cursor, SCREEN))
        })
        .take(2)
        .collect();
    assert_eq!(sites.len(), 2);
    game.handle_click(hex_cursor(&game, sites[0]), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_building, None);
    assert_eq!(
        game.cities[city]
            .planned_sites
            .get(&Building::Barracks)
            .copied(),
        Some(sites[0])
    );
    game.update_hover(Some(hex_cursor(&game, sites[1])), SCREEN, 0.1);
    game.handle_click(hex_cursor(&game, sites[1]), SCREEN, ClickMode::Normal);
    assert_eq!(
        game.cities[city]
            .planned_sites
            .get(&Building::Barracks)
            .copied(),
        Some(sites[0])
    );
    assert_eq!(game.placing_building, None);
}

/// The state of `building`'s card in the open city's tray.
fn building_card(game: &GameState, building: Building) -> ButtonState {
    game.layout(SCREEN)
        .buttons
        .iter()
        .find(|b| b.target == Target::Building(building))
        .expect("building card shown")
        .state
}

/// Plays the turn out to the next planning phase.
fn play_turn(game: &mut GameState) {
    game.end_planning();
    assert!(game.is_resolving());
    while game.is_resolving() {
        game.update(10.0);
    }
}

#[test]
fn ending_the_turn_while_choosing_a_site_leaves_no_preview_on_the_map() {
    // #51: End Turn closed the city but kept its site placement, so the
    // preview followed the cursor with no city open.
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    // Something else to build, so the city isn't left empty-handed.
    game.queue_selected_city_worker();
    let card = button_cursor(&game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(game.site_placement(), Some((city, Building::Barracks)));
    play_turn(&mut game);
    assert_eq!(game.selected_city, None);
    assert_eq!(game.site_placement(), None);
    assert_eq!(game.placing_building, None);
    assert!(
        !game.cities[city]
            .queue
            .contains(&Build::Building(Building::Barracks)),
        "a Barracks with no site is taken back out of the queue"
    );
}

#[test]
fn a_building_finished_with_its_city_closed_picks_its_site_when_reopened() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    // A Barracks queued with no site and not being placed (the queue of a
    // save from before sites were required, say).
    game.cities[city]
        .queue
        .push(Build::Building(Building::Barracks));
    assert!(game.exit_structure_menu());
    game.cities[city].production = Building::Barracks.cost();
    play_turn(&mut game);
    assert_eq!(
        game.cities[city].pending_building,
        Some(Building::Barracks),
        "paid for, the Barracks waits for a site"
    );
    // #51: finishing it during the turn started placement with no city open.
    assert_eq!(game.placing_building, None);
    assert_eq!(game.site_placement(), None);

    // Opening the city resumes choosing the site, and the card isn't dead.
    game.open_city(city);
    game.update(10.0);
    assert_eq!(game.site_placement(), Some((city, Building::Barracks)));
    assert_ne!(
        building_card(&game, Building::Barracks),
        ButtonState::Disabled
    );
    let site = Hex::new(-2, 0);
    assert!(game.site_available(city, Building::Barracks, site));
    game.city_click(site);
    game.confirm_building(Building::Barracks);
    assert_eq!(game.cities[city].barracks, Some(site));
}

#[test]
fn escape_while_choosing_a_site_cancels_the_building() {
    // #52, and after it: clicking the Barracks card and pressing Escape
    // left the Barracks queued with no site, its card highlighted.
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    let barracks_queued = |game: &GameState| {
        game.cities[city]
            .queue
            .contains(&Build::Building(Building::Barracks))
    };
    assert_eq!(building_card(&game, Building::Barracks), ButtonState::Ready);
    let card = button_cursor(&game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert!(barracks_queued(&game));

    // The first Escape only cancels choosing the site, and the Barracks.
    assert!(game.exit_structure_menu());
    assert_eq!(game.selected_city, Some(city), "the city stays open");
    assert_eq!(game.site_placement(), None);
    assert!(!barracks_queued(&game));
    assert_eq!(building_card(&game, Building::Barracks), ButtonState::Ready);

    // End Turn mid-placement cancels it, and the city, left with nothing to
    // build, asks for something instead of ending the turn.
    let card = button_cursor(&game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    game.end_planning();
    assert!(!game.is_resolving());
    assert_eq!(game.selected_city, Some(city));
    assert!(!barracks_queued(&game));

    // Closing the city mid-placement cancels it too.
    let card = button_cursor(&game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    game.leave_city_view();
    game.open_city(city);
    game.update(10.0);
    assert!(!barracks_queued(&game));
    assert_eq!(game.site_placement(), None);
    assert_eq!(building_card(&game, Building::Barracks), ButtonState::Ready);

    // With its site chosen, the card is spent until the Barracks is removed.
    let card = button_cursor(&game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    game.city_click(Hex::new(-2, 0));
    assert!(game.exit_structure_menu());
    assert_eq!(game.selected_city, None, "nothing to cancel: Escape closes");
    assert!(
        barracks_queued(&game),
        "a Barracks with a site stays queued"
    );
    game.open_city(city);
    assert_eq!(
        building_card(&game, Building::Barracks),
        ButtonState::Disabled
    );
    let at = game.cities[city]
        .queue
        .iter()
        .position(|&b| b == Build::Building(Building::Barracks))
        .unwrap();
    game.remove_selected_city_queue_item(at);
    assert_eq!(building_card(&game, Building::Barracks), ButtonState::Ready);
}

#[test]
fn opening_another_view_drops_the_site_placement() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.queue_selected_city_building(Building::Mill);
    assert_eq!(game.site_placement(), Some((city, Building::Mill)));
    game.open_barracks(city);
    assert_eq!(game.site_placement(), None);
    assert_eq!(game.placing_building, None);
    assert!(
        !game.cities[city]
            .queue
            .contains(&Build::Building(Building::Mill))
    );
}

#[test]
fn mill_and_workshop_cards_use_shared_placement_controls() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    for (building, site) in [
        (Building::Mill, Hex::new(-2, 0)),
        (Building::Workshop, Hex::new(-1, 0)),
    ] {
        game.handle_click(
            button_cursor(&game, Target::Building(building)),
            SCREEN,
            ClickMode::Normal,
        );
        assert_eq!(game.placing_building, Some((city, building)));
        game.city_click(site);
        assert_eq!(game.cities[city].planned_sites.get(&building), Some(&site));
        let layout = game.layout(SCREEN);
        assert!(
            !layout
                .buttons
                .iter()
                .any(|b| b.target == Target::ConfirmBuilding(building))
        );
    }
}

#[test]
fn planned_building_badge_can_move_without_stealing_citizen_clicks() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    let worked = game.cities[city].worked[0];
    game.queue_selected_city_building(Building::Mill);
    game.city_click(worked);
    let edge = game
        .camera
        .world_to_screen(worked.to_world() + Vec2::new(0.5, 0.0), SCREEN);
    game.handle_click(edge, SCREEN, ClickMode::Normal);
    assert_eq!(game.moving_manager, Some(city));
    game.moving_manager = None;
    game.handle_click(hex_cursor(&game, worked), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_building, Some((city, Building::Mill)));
    let revised = Hex::new(-2, 0);
    game.handle_click(hex_cursor(&game, revised), SCREEN, ClickMode::Normal);
    assert_eq!(
        game.cities[city].planned_sites.get(&Building::Mill),
        Some(&revised)
    );
}

#[test]
fn debug_completion_reveals_confirm_only_when_ready() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.queue_selected_city_building(Building::Workshop);
    game.city_click(Hex::new(-2, 0));
    assert!(
        !game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::ConfirmBuilding(Building::Workshop))
    );
    game.handle_click(
        button_cursor(&game, Target::CompleteProduction),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(game.cities[city].pending_building, Some(Building::Workshop));
    let confirm = Target::ConfirmBuilding(Building::Workshop);
    assert!(
        game.layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == confirm && b.state == ButtonState::Ready)
    );
    game.handle_click(button_cursor(&game, confirm), SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[city].workshop, Some(Hex::new(-2, 0)));
}

#[test]
fn city_queue_scroll_keeps_the_box_bounded_and_targets_the_visible_items() {
    let mut game = city_view();
    for _ in 0..8 {
        game.queue_selected_city_unit(BuildUnit::Melee);
    }
    let layout = game.layout(SCREEN);
    let scroll = layout.queue_scrollbars.first().expect("queue scrollbar");
    assert_eq!(scroll.kind, QueueKind::City);
    assert!(scroll.panel_max.y - scroll.panel_min.y < 330.0);
    assert!(scroll.panel_max.y < SCREEN.y - TOP_BAR_HEIGHT);
    let shown = |layout: &Layout, index| {
        layout
            .buttons
            .iter()
            .any(|b| b.target == Target::CityQueueRemove(index))
    };
    // The first rows that fit, then nothing past them.
    let visible = (0..8).take_while(|&i| shown(&layout, i)).count();
    assert!((2..8).contains(&visible), "{visible} rows visible");
    assert!(!shown(&layout, visible));

    let cursor = to_ui((scroll.panel_min + scroll.panel_max) / 2.0, SCREEN);
    assert!(game.scroll_queue_at(cursor, SCREEN, -1.0));
    let layout = game.layout(SCREEN);
    assert!(shown(&layout, 1));
    assert!(shown(&layout, visible));
    assert!(
        !layout
            .buttons
            .iter()
            .any(|b| b.target == Target::CityQueueRemove(0))
    );

    let scroll = layout.queue_scrollbars.first().unwrap();
    let bottom = to_ui(
        Vec2::new(
            (scroll.track_min.x + scroll.track_max.x) / 2.0,
            scroll.track_min.y,
        ),
        SCREEN,
    );
    assert!(game.drag_queue_scrollbar_at(bottom, SCREEN, false));
    assert_eq!(game.city_queue_scroll, 8 - visible, "scrolled to the end");
    let remove = button_cursor(&game, Target::CityQueueRemove(7));
    game.handle_click(remove, SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[game.selected_city.unwrap()].queue.len(), 7);
}

#[test]
fn barracks_queue_uses_the_same_scroll_window() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.open_barracks(city);
    game.cities[city].barracks_queue = vec![BuildUnit::Melee; 12];
    let layout = game.layout(SCREEN);
    let scroll = layout.queue_scrollbars.first().expect("barracks scrollbar");
    assert_eq!(scroll.kind, QueueKind::Barracks);
    let cursor = to_ui((scroll.panel_min + scroll.panel_max) / 2.0, SCREEN);
    game.scroll_queue_at(cursor, SCREEN, -2.0);
    assert!(
        game.layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::BarracksQueueRemove(3))
    );
    assert!(
        !game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::BarracksQueueRemove(0))
    );
}

#[test]
fn queue_rows_drag_to_reorder_and_x_removes_without_dragging() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].queue = vec![
        Build::Unit(BuildUnit::Melee),
        Build::Unit(BuildUnit::Ranged),
        Build::Unit(BuildUnit::Siege),
    ];
    let layout = game.layout(SCREEN);
    let row_cursor = |index| {
        let row = layout
            .queue_items
            .iter()
            .find(|row| row.index == index)
            .unwrap();
        to_ui(
            Vec2::new(
                (row.min.x + row.body_max_x) / 2.0,
                (row.min.y + row.max.y) / 2.0,
            ),
            SCREEN,
        )
    };
    let from = row_cursor(2);
    let to = row_cursor(0);
    assert!(game.start_queue_drag_at(from, SCREEN));
    game.update_queue_drag_at(to, SCREEN);
    assert_eq!(game.queue_drag.unwrap().target, Some(0));
    game.finish_queue_drag_at(to, SCREEN);
    assert_eq!(game.cities[city].queue[0], Build::Unit(BuildUnit::Siege));
    assert!(game.queue_drag.is_none());

    let x = button_cursor(&game, Target::CityQueueRemove(1));
    assert!(!game.start_queue_drag_at(x, SCREEN));
    game.handle_click(x, SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[city].queue.len(), 2);
}

#[test]
fn ready_city_building_cannot_be_dragged_out_of_first_place() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].queue = vec![
        Build::Building(Building::Workshop),
        Build::Unit(BuildUnit::Melee),
    ];
    game.cities[city].pending_building = Some(Building::Workshop);
    let layout = game.layout(SCREEN);
    let cursor = |index| {
        let row = layout
            .queue_items
            .iter()
            .find(|row| row.index == index)
            .unwrap();
        to_ui(
            Vec2::new(
                (row.min.x + row.body_max_x) / 2.0,
                (row.min.y + row.max.y) / 2.0,
            ),
            SCREEN,
        )
    };
    assert!(!game.start_queue_drag_at(cursor(0), SCREEN));
    assert!(game.start_queue_drag_at(cursor(1), SCREEN));
    game.finish_queue_drag_at(cursor(0), SCREEN);
    assert_eq!(
        game.cities[city].queue[0],
        Build::Building(Building::Workshop)
    );
}

#[test]
fn barracks_queue_rows_use_the_same_drag_and_remove_targets() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.open_barracks(city);
    game.cities[city].barracks_queue = vec![BuildUnit::Melee, BuildUnit::Ranged];
    let layout = game.layout(SCREEN);
    let cursor = |index| {
        let row = layout
            .queue_items
            .iter()
            .find(|row| row.index == index)
            .unwrap();
        to_ui(
            Vec2::new(
                (row.min.x + row.body_max_x) / 2.0,
                (row.min.y + row.max.y) / 2.0,
            ),
            SCREEN,
        )
    };
    assert!(game.start_queue_drag_at(cursor(1), SCREEN));
    game.finish_queue_drag_at(cursor(0), SCREEN);
    assert_eq!(game.cities[city].barracks_queue[0], BuildUnit::Ranged);
    let x = button_cursor(&game, Target::BarracksQueueRemove(1));
    assert!(!game.start_queue_drag_at(x, SCREEN));
    game.handle_click(x, SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[city].barracks_queue, vec![BuildUnit::Ranged]);
}

#[test]
fn queue_row_count_tracks_available_screen_height() {
    let mut game = city_view();
    for _ in 0..12 {
        game.queue_selected_city_unit(BuildUnit::Melee);
    }
    let mut counts = Vec::new();
    for height in [820.0, 900.0, 1200.0] {
        let screen = Vec2::new(1600.0, height);
        let layout = game.layout(screen);
        let count = layout
            .buttons
            .iter()
            .filter(|b| matches!(b.target, Target::CityQueueRemove(_)))
            .count();
        let queue = layout.queue_scrollbars.first().unwrap();
        assert!(queue.panel_max.y <= height - TOP_BAR_HEIGHT - QUEUE_TOP_GAP);
        let debug_button = layout
            .buttons
            .iter()
            .find(|b| b.target == Target::Scenario(Scenario::Cities))
            .unwrap();
        let center = (debug_button.min + debug_button.max) / 2.0;
        let debug = layout
            .panels
            .iter()
            .find(|&&(min, max)| contains(min, max, center))
            .unwrap();
        assert!(
            !Rect {
                min: queue.panel_min,
                max: queue.panel_max
            }
            .overlaps(
                Rect {
                    min: debug.0,
                    max: debug.1
                },
                0.0
            )
        );
        counts.push(count);
    }
    assert_eq!(counts[1], 4);
    assert!(counts[0] < counts[1]);
    assert!(counts[2] > counts[1]);
}

#[test]
fn docked_panel_buttons_share_the_rendered_hit_box() {
    let mut layout = Layout::for_screen(SCREEN);
    let mut panel = PanelBuilder::default();
    panel.text(TITLE, vec![("ADDED PANEL".into(), TEXT)]);
    panel.gap(37.0);
    panel.buttons(vec![ButtonSpec {
        target: Target::ToggleYields,
        label: "ACTION".into(),
        hint: "Y".into(),
        state: ButtonState::Ready,
        armed: false,
    }]);
    let rect = layout.dock_panel(panel, Zone::BottomLeft).unwrap();
    let button = layout
        .buttons
        .iter()
        .find(|b| b.target == Target::ToggleYields)
        .unwrap();
    let center = (button.min + button.max) / 2.0;
    assert!(contains(rect.min, rect.max, center));
    assert_eq!(
        layout.button_at(center).unwrap().target,
        Target::ToggleYields
    );
    assert!(layout.covers(center));

    let mut next = PanelBuilder::default();
    next.text(BODY, vec![("ANOTHER PANEL".into(), TEXT)]);
    let next_rect = layout.dock_panel(next, Zone::BottomLeft).unwrap();
    assert!(next_rect.min.y >= rect.max.y + GAP);
}

#[test]
fn city_clicks_do_not_select_units_without_exiting_city_view() {
    let mut game = city_view();
    // One of the player's units drawn clear of the top bar and the tray.
    let unit = (0..game.units.len())
        .find(|&i| {
            let cursor = hex_cursor(&game, game.units[i].pos);
            game.units[i].team == Team::Blue
                && (100.0..500.0).contains(&cursor.y)
                && (0.0..SCREEN.x).contains(&cursor.x)
        })
        .expect("a unit in view");
    game.handle_click(
        hex_cursor(&game, game.units[unit].pos),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(game.selected_city.is_some());
    assert_ne!(game.selected, Some(unit));
}

#[test]
fn clicking_the_open_city_enters_interior_and_escape_returns() {
    let mut game = city_view();
    let exterior_camera = game.camera.clone();
    let city = game.cities[game.selected_city.unwrap()].pos;
    game.handle_click(hex_cursor(&game, city), SCREEN, ClickMode::Normal);
    assert_eq!(game.interior_view, Some(0));
    assert_eq!(game.selected_city, None);
    assert!(game.exit_structure_menu());
    assert_eq!(game.interior_view, None);
    assert_eq!(game.selected_city, Some(0));
    assert_eq!(game.camera.center, exterior_camera.center);
    assert_eq!(game.camera.half_height, exterior_camera.half_height);
}

#[test]
fn yields_show_only_for_the_open_city_and_toggle() {
    let mut game = city_view();
    let shown = game.build_vertices().len();
    game.handle_click(
        button_cursor(&game, Target::ToggleYields),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(!game.show_yields);
    assert!(game.build_vertices().len() < shown, "badges hidden");
    game.toggle_yields();
    assert_eq!(game.build_vertices().len(), shown);

    // Hovering a city without opening it no longer shows its yields.
    let city = game.cities[game.selected_city.unwrap()].pos;
    assert!(game.exit_structure_menu());
    game.update_hover(Some(hex_cursor(&game, city)), SCREEN, 0.0);
    assert!(game.hovered_city.is_some());
    assert_eq!(game.yields_city(), None);
    assert!(game.build_vertices().len() < shown);
}

#[test]
fn resting_on_a_tile_shows_its_tooltip_after_a_delay() {
    let mut game = GameState::new();
    let cursor = hex_cursor(&game, Hex::new(0, 1));
    game.update_hover(Some(cursor), SCREEN, 0.0);
    let before = game.build_ui(SCREEN, Some(cursor)).len();
    game.update_hover(Some(cursor), SCREEN, TILE_TOOLTIP_DELAY);
    assert_eq!(game.hovered_tile, Some(Hex::new(0, 1)));
    assert!(game.build_ui(SCREEN, Some(cursor)).len() > before);

    // Moving to another hex starts the wait over.
    let elsewhere = hex_cursor(&game, Hex::new(1, 1));
    game.update_hover(Some(elsewhere), SCREEN, 0.1);
    assert_eq!(game.hover_seconds, 0.0);
}

#[test]
fn hovering_an_enemy_city_or_barracks_out_of_sight_shows_no_live_panel() {
    let mut game = GameState::city_scenario();
    game.units.clear();
    game.selected = None;
    let red = game
        .cities
        .iter()
        .position(|city| city.team == Team::Red)
        .unwrap();
    let barracks = game.cities[red].pos.neighbors()[0];
    game.cities[red].barracks = Some(barracks);
    let blue = game
        .cities
        .iter()
        .position(|c| c.team == Team::Blue)
        .unwrap();
    for hex in [game.cities[red].pos, barracks] {
        assert!(
            !game.fog().sees(hex),
            "only Blue's city sees, and not this far"
        );
        game.hovered_tile = Some(hex);
        let fogged = game.layout_with_hover(SCREEN, None).0.panels.len();
        game.fog_of_war = false;
        let clear = game.layout_with_hover(SCREEN, None).0.panels.len();
        game.fog_of_war = true;
        assert_eq!(clear, fogged + 1, "{hex:?}");
    }
    // Seen once and remembered, it's still out of sight: no live panel.
    let red_pos = game.cities[red].pos;
    game.units.push(Unit::new(
        50,
        red_pos.neighbors()[3],
        Team::Blue,
        UnitType::Scout,
    ));
    game.explore();
    game.units.clear();
    assert!(game.is_explored(red_pos) && !game.fog().sees(red_pos));
    game.hovered_tile = Some(red_pos);
    let fogged = game.layout_with_hover(SCREEN, None).0.panels.len();
    game.hovered_tile = None;
    assert_eq!(fogged, game.layout_with_hover(SCREEN, None).0.panels.len());

    // The player's own city always has its panel.
    game.hovered_tile = Some(game.cities[blue].pos);
    let own = game.layout_with_hover(SCREEN, None).0.panels.len();
    game.hovered_tile = None;
    assert_eq!(own, game.layout_with_hover(SCREEN, None).0.panels.len() + 1);
}

#[test]
fn the_tooltip_shows_an_unseen_hex_as_last_seen() {
    let (mut game, _, far) = crate::game::fog::tests::remembered_route_hex();
    let read =
        |game: &GameState| line_strings(game.tile_tooltip_lines(far).into_iter().map(|(_, l)| l));
    let before = read(&game);
    assert!(
        before.iter().any(|s| s.contains("REACHES CITY")),
        "{before:?}"
    );

    // Red moves in and farms the hex, all out of sight.
    game.units
        .push(Unit::new(51, far, Team::Red, UnitType::Melee));
    game.sites.insert(
        far,
        super::super::city::Site {
            team: Team::Red,
            food: 9,
            production: 9,
            label: "FARM",
        },
    );
    assert_eq!(read(&game), before);
}

fn line_strings(lines: impl IntoIterator<Item = Line>) -> Vec<String> {
    lines
        .into_iter()
        .map(|line| line.into_iter().map(|(s, _)| s).collect())
        .collect()
}

fn panel_strings(fill: impl FnOnce(&mut PanelBuilder)) -> Vec<String> {
    let mut panel = PanelBuilder::default();
    fill(&mut panel);
    line_strings(panel.rows.into_iter().filter_map(|row| match row {
        Row::Text(_, line) => Some(line),
        _ => None,
    }))
}

fn assert_shows(text: &[String], expected: &str) {
    assert!(
        text.iter().any(|s| s.contains(expected)),
        "{expected} not in {text:?}"
    );
}

#[test]
fn every_panel_shows_production_per_turn_in_displayed_units() {
    let mut game = GameState::city_scenario();
    // Nothing is left to see the barracks tile once the units are gone.
    game.fog_of_war = false;
    game.units.clear();
    let manager = Hex::new(-1, 0);
    game.cities[0].worked = vec![manager, Hex::new(-1, 1)];
    game.cities[0].barracks = Some(manager);
    let (_, city_income) = game.income(0);
    let barracks_income = game.barracks_income(0);
    // Stored in quarters: a raw value would read four times too high.
    assert!(city_income > 4 && barracks_income > 4);
    let city_rate = signed_quantity(city_income);
    let barracks_rate = signed_quantity(barracks_income);

    let tray = panel_strings(|panel| game.city_tray(0, panel));
    assert_shows(&tray, &format!("{city_rate} PER TURN"));
    let hover = panel_strings(|panel| game.structure_hover_panel(0, false, panel));
    assert_shows(&hover, &format!("{city_rate} PROD/T"));
    let city_tooltip = line_strings(
        game.tile_tooltip_lines(game.cities[0].pos)
            .into_iter()
            .map(|(_, line)| line),
    );
    assert_shows(&city_tooltip, &format!("{city_rate} PRODUCTION"));

    let barracks_tray = panel_strings(|panel| game.barracks_tray(0, panel));
    assert_shows(&barracks_tray, &format!("{barracks_rate} PROD/T"));
    let barracks_hover = panel_strings(|panel| game.structure_hover_panel(0, true, panel));
    assert_shows(&barracks_hover, &format!("{barracks_rate} PROD/T"));
    let barracks_tooltip = line_strings(
        game.tile_tooltip_lines(manager)
            .into_iter()
            .map(|(_, line)| line),
    );
    assert_shows(&barracks_tooltip, &format!("{barracks_rate} PROD/T"));
}

#[test]
fn clicks_on_a_panel_do_not_reach_the_map() {
    let mut game = GameState::new();
    let selected = game.selected;
    // The top bar's left end, away from any button.
    game.handle_click(Vec2::new(4.0, 4.0), SCREEN, ClickMode::Normal);
    assert_eq!(game.selected, selected);
}

#[test]
fn end_turn_button_names_what_is_waiting() {
    // Production first, then idle workers, then units: the turn's order.
    assert_eq!(end_turn_label((3, 1, 2)), "CHOOSE PRODUCTION");
    assert_eq!(end_turn_label((0, 2, 0)), "2 CITIES NEED PRODUCTION");
    assert_eq!(end_turn_label((3, 0, 1)), "WORKER NEEDS A JOB");
    assert_eq!(end_turn_label((3, 0, 2)), "2 WORKERS NEED JOBS");
    assert_eq!(end_turn_label((3, 0, 0)), "3 UNITS NEED ORDERS");
    assert_eq!(end_turn_label((1, 0, 0)), "UNIT NEEDS ORDERS");
    assert_eq!(end_turn_label((0, 0, 0)), "END TURN");

    let game = GameState::new();
    let layout = game.layout(SCREEN);
    let button = layout.buttons.iter().find(|b| b.target == Target::EndTurn);
    assert_eq!(button.unwrap().label, "4 UNITS NEED ORDERS");
}

#[test]
fn wrap_breaks_between_words() {
    assert_eq!(wrap("AB CD EF", 5), vec!["AB CD", "EF"]);
    assert_eq!(wrap("ABCDEFG HI", 5), vec!["ABCDEFG", "HI"]);
}

/// An empty tile near Blue's city in the Cities scenario, with nothing
/// selected, so a click on it opens its tile panel.
fn empty_tile_near_blue_city() -> (GameState, Hex) {
    let mut game = GameState::city_scenario();
    game.explore();
    game.clear_selection();
    let city = game.cities[0].pos;
    let hex = game
        .grid
        .all_hexes()
        .filter(|&h| h.distance(city) == 2 && game.grid.is_passable(h))
        .filter(|&h| !game.roads.contains(&h) && !game.is_occupied(h))
        .filter(|&h| game.is_explored(h) && game.cities.iter().all(|c| c.pos != h))
        .min_by_key(|h| (h.q, h.r))
        .unwrap();
    (game, hex)
}

#[test]
fn the_worker_menu_places_a_picked_job_on_the_map() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.toggle_worker_mode();
    let layout = game.layout(SCREEN);
    for kind in JobKind::ALL {
        let button = layout
            .buttons
            .iter()
            .find(|b| b.target == Target::WorkerJob(kind))
            .expect("every job has a button");
        assert!(
            layout
                .panels
                .iter()
                .any(|&(min, max)| contains(min, max, button.min) && contains(min, max, button.max)),
            "{kind:?} sits inside its panel"
        );
    }
    // With nothing picked, a map click places nothing. (The map clicks go
    // straight to the map: the menu's panel may cover the tile.)
    game.handle_map_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert!(game.cities[0].worker_jobs.is_empty());
    let road = Target::WorkerJob(JobKind::Road);
    game.handle_click(button_cursor(&game, road), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, Some(JobKind::Road));
    let armed = |game: &GameState| {
        game.layout(SCREEN)
            .buttons
            .iter()
            .find(|b| b.target == road)
            .is_some_and(|b| b.armed)
    };
    assert!(armed(&game), "the button shows it's picked");
    game.handle_map_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[0].worker_jobs.len(), 1);
    assert_eq!(game.cities[0].worker_jobs[0].hex, hex);
    assert_eq!(game.cities[0].worker_jobs[0].kind, JobKind::Road);
    assert!(armed(&game), "still picked, for the next one");
    // Picking it again puts it down.
    game.handle_click(button_cursor(&game, road), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, None);
}

#[test]
fn a_plain_click_on_an_empty_tile_opens_nothing() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.handle_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert_eq!(game.selected, None);
    assert!(
        !game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| matches!(b.target, Target::WorkerJob(_))),
        "workers take orders from the worker menu only"
    );
}

#[test]
fn a_wall_is_placed_by_dragging_along_hex_edges() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.toggle_worker_mode();
    let wall = Target::WorkerJob(JobKind::Wall);
    game.handle_click(button_cursor(&game, wall), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, Some(JobKind::Wall));
    let armed = game
        .layout(SCREEN)
        .buttons
        .into_iter()
        .find(|b| b.target == wall);
    assert!(
        armed.is_some_and(|b| b.armed),
        "the button shows it's armed"
    );

    // Drag across the midpoints of three of the hex's edges, as the mouse
    // would pass over them.
    let edge_cursor = |game: &GameState, n: Hex| {
        let world = (hex.to_world() + n.to_world()) / 2.0;
        game.camera.world_to_screen(world, SCREEN)
    };
    let sides: Vec<Hex> = hex
        .neighbors()
        .into_iter()
        .filter(|&n| game.grid.contains(n))
        .take(3)
        .collect();
    for &n in &sides {
        assert!(game.paint_job_at(edge_cursor(&game, n), SCREEN, false));
        // Passing the same edge again adds nothing.
        game.paint_job_at(edge_cursor(&game, n), SCREEN, false);
    }
    let jobs = &game.cities[0].worker_jobs;
    assert_eq!(jobs.len(), 3);
    for (job, n) in jobs.iter().zip(&sides) {
        assert_eq!(job.kind, JobKind::Wall);
        let placed = crate::game::hex::edge(job.hex, job.across.unwrap());
        assert_eq!(placed, crate::game::hex::edge(hex, *n));
    }

    // A press on the panel itself doesn't place anything through it.
    assert!(!game.paint_job_at(button_cursor(&game, wall), SCREEN, true));

    // Escape stops placing but leaves the worker menu open.
    game.press_escape();
    assert_eq!(game.placing_job, None);
    assert!(game.worker_mode);
    let n = sides[0];
    assert!(!game.paint_job_at(edge_cursor(&game, n), SCREEN, false));
}

#[test]
fn a_worker_out_can_be_recalled_from_the_worker_menu() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.placing_job = Some(JobKind::Fort);
    assert!(game.place_job_at(hex, None));
    game.placing_job = None;
    // Two hexes out, one a turn.
    game.resolve_workers();
    game.resolve_workers();
    let id = game.field_workers[0].id;
    assert_eq!(game.field_workers[0].pos, hex);

    game.toggle_worker_mode();
    let recall = Target::RecallWorker(id);
    game.handle_click(button_cursor(&game, recall), SCREEN, ClickMode::Normal);
    assert!(game.field_workers[0].recalled);
    assert!(
        !game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == recall),
        "once recalled, there's nothing left to press"
    );
}

#[test]
fn the_disband_button_asks_then_removes_the_unit() {
    let mut game = GameState::city_scenario();
    let idx = game.selected.expect("a unit starts selected");
    let id = game.units[idx].id;
    let disband = Target::Unit(UnitAction::Disband);
    let layout = game.layout(SCREEN);
    let button = layout.buttons.iter().find(|b| b.target == disband).unwrap();
    assert!(
        layout
            .panels
            .iter()
            .any(|&(min, max)| contains(min, max, button.min) && contains(min, max, button.max))
    );
    game.handle_click(button_cursor(&game, disband), SCREEN, ClickMode::Normal);
    let asking = game.layout(SCREEN);
    let button = asking.buttons.iter().find(|b| b.target == disband).unwrap();
    assert!(button.armed && button.label == "CONFIRM?");
    game.handle_click(button_cursor(&game, disband), SCREEN, ClickMode::Normal);
    assert!(game.units.iter().all(|u| u.id != id));
}

#[test]
fn the_worker_menu_lists_its_citys_jobs_and_removes_them() {
    let (mut game, hex) = empty_tile_near_blue_city();
    place_road_and_fort(&mut game, hex);
    game.toggle_worker_mode();
    let remove = Target::WorkerJobRemove(0);
    game.handle_click(button_cursor(&game, remove), SCREEN, ClickMode::Normal);
    let jobs = &game.cities[0].worker_jobs;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, JobKind::Fort);

    // The city panel still builds workers, and sends you to the menu.
    game.toggle_worker_mode();
    game.select_city();
    game.queue_selected_city_worker();
    assert_eq!(game.cities[0].queue.last(), Some(&Build::Worker));
    let layout = game.layout(SCREEN);
    for target in [Target::BuildWorker, Target::WorkerMode] {
        assert!(
            layout.buttons.iter().any(|b| b.target == target),
            "{target:?}"
        );
    }
    assert!(
        !layout.buttons.iter().any(|b| b.target == remove),
        "the city panel no longer lists jobs"
    );
}

/// A road on `hex` and a fort next door (a tile takes one job at a time).
fn place_road_and_fort(game: &mut GameState, hex: Hex) {
    game.placing_job = Some(JobKind::Road);
    assert!(game.place_job_at(hex, None));
    let next_door = hex
        .neighbors()
        .into_iter()
        .find(|&h| game.job_unavailable(h, JobKind::Fort).is_none())
        .expect("a tile for a fort");
    game.placing_job = Some(JobKind::Fort);
    assert!(game.place_job_at(next_door, None));
    game.placing_job = None;
}

#[test]
fn worker_jobs_reorder_by_dragging() {
    let (mut game, hex) = empty_tile_near_blue_city();
    place_road_and_fort(&mut game, hex);
    game.toggle_worker_mode();
    game.reorder_queue(QueueKind::Workers, 1, 0);
    let kinds: Vec<_> = game.cities[0].worker_jobs.iter().map(|j| j.kind).collect();
    assert_eq!(kinds, [JobKind::Fort, JobKind::Road]);
}

#[test]
fn the_city_panel_opens_the_worker_menu_and_done_closes_it() {
    let mut game = GameState::city_scenario();
    game.select_city();
    game.handle_click(
        button_cursor(&game, Target::WorkerMode),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(game.worker_mode);
    assert_eq!(game.selected_city, None);
    let text = panel_strings(|panel| game.worker_menu(panel));
    assert_shows(&text, "WORKERS");
    assert_shows(&text, "3 TILES FROM A CITY");
    game.handle_click(
        button_cursor(&game, Target::WorkerMode),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(!game.worker_mode);
}

#[test]
fn idle_workers_wait_in_the_turn_order_until_they_get_a_job_or_sleep() {
    let mut game = GameState::city_scenario();
    game.explore();
    let city = game.cities[0].id;
    let workers = RosterKey::Workers(city);
    assert!(roster_keys(&game).contains(&workers), "idle at home");
    assert_eq!(game.pending().2, 1);
    // Its chip opens the worker menu on its city.
    game.handle_click(roster_cursor(&game, workers), SCREEN, ClickMode::Normal);
    assert!(game.worker_mode);
    assert_eq!(game.worker_menu_city, Some(0));
    // Sleep rests them for the turn, and the menu closes.
    game.handle_click(
        button_cursor(&game, Target::SleepWorkers),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(game.cities[0].workers_resting);
    assert!(!game.worker_mode);
    assert_eq!(game.pending().2, 0);
    assert!(!roster_keys(&game).contains(&workers));
    // A job does the same.
    game.cities[0].workers_resting = false;
    let hex = empty_tile_near_blue_city().1;
    game.placing_job = Some(JobKind::Road);
    assert!(game.place_job_at(hex, None));
    assert_eq!(game.pending().2, 0);
}

#[test]
fn clicking_a_worker_job_or_a_worker_shows_it_on_the_map() {
    let (mut game, hex) = empty_tile_near_blue_city();
    place_road_and_fort(&mut game, hex);
    game.toggle_worker_mode();
    // A click on the road's row, not a drag, takes the camera to it.
    let layout = game.layout(SCREEN);
    let row = layout
        .queue_items
        .iter()
        .find(|item| item.kind == QueueKind::Workers && item.index == 0)
        .expect("the road's row");
    let cursor = to_ui(
        Vec2::new(row.min.x + 20.0, (row.min.y + row.max.y) / 2.0),
        SCREEN,
    );
    assert!(game.start_queue_drag_at(cursor, SCREEN));
    game.finish_queue_drag_at(cursor, SCREEN);
    game.camera.update(10.0);
    assert!(game.camera.center.distance(hex.to_world()) < 0.01);
    let kinds: Vec<_> = game.cities[0].worker_jobs.iter().map(|j| j.kind).collect();
    assert_eq!(kinds, [JobKind::Road, JobKind::Fort], "nothing reordered");

    // A worker's row takes the camera to the worker.
    game.toggle_worker_mode();
    game.resolve_workers();
    let worker = game.field_workers[0].clone();
    game.toggle_worker_mode();
    game.handle_click(
        button_cursor(&game, Target::ShowWorker(worker.id)),
        SCREEN,
        ClickMode::Normal,
    );
    game.camera.update(10.0);
    assert!(game.camera.center.distance(worker.pos.to_world()) < 0.01);
    assert!(!game.field_workers[0].recalled, "showing isn't recalling");
}

#[test]
fn worker_job_rows_name_the_build_its_tile_and_its_turns() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.placing_job = Some(JobKind::Improve);
    assert!(game.place_job_at(hex, None));
    game.placing_job = None;
    game.toggle_worker_mode();
    let mut panel = PanelBuilder::default();
    game.worker_menu(&mut panel);
    let labels: Vec<String> = panel
        .rows
        .into_iter()
        .filter_map(|row| match row {
            Row::QueueItem(item) => Some(item.label),
            _ => None,
        })
        .collect();
    let tile = game.grid.tile(hex).name();
    let build = if game.grid.tile(hex).hills {
        "MINE"
    } else {
        "FARM"
    };
    assert_eq!(labels, [format!("{build} · {tile} · 3T")]);
}

/// Where to click, in window pixels, on `key`'s chip in the turn strip.
fn roster_cursor(game: &GameState, key: RosterKey) -> Vec2 {
    let layout = game.layout(SCREEN);
    let &(min, max, _) = layout
        .roster_chips
        .iter()
        .find(|chip| chip.2 == key)
        .expect("chip shown");
    to_ui((min + max) / 2.0, SCREEN)
}

/// The turn strip's chips, in order: what each stands for, whether it's
/// framed as selected, and its count.
fn roster(game: &GameState) -> Vec<(RosterKey, bool, usize)> {
    game.layout(SCREEN)
        .shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::UnitChip { chip, .. } => Some((chip.key, chip.selected, chip.count)),
            _ => None,
        })
        .collect()
}

fn roster_keys(game: &GameState) -> Vec<RosterKey> {
    roster(game).into_iter().map(|c| c.0).collect()
}

#[test]
fn the_turn_strip_lists_civilian_tasks_first_then_unit_groups() {
    // The Cities scenario: Blue's city has nothing queued and a worker idle
    // at home, and one unit of each military kind.
    let mut game = GameState::city_scenario();
    let city = game
        .cities
        .iter()
        .find(|c| c.team == PLAYER_TEAM)
        .unwrap()
        .id;
    let keys = roster_keys(&game);
    assert_eq!(keys[0], RosterKey::Production(city), "{keys:?}");
    assert_eq!(keys[1], RosterKey::Workers(city), "{keys:?}");
    let groups: Vec<UnitType> = keys[2..]
        .iter()
        .map(|key| match key {
            RosterKey::Group(unit_type, false) => *unit_type,
            other => panic!("expected military groups, got {other:?}"),
        })
        .collect();
    let blue: Vec<UnitType> = game
        .units
        .iter()
        .filter(|u| u.team == PLAYER_TEAM)
        .map(|u| u.unit_type)
        .collect();
    assert_eq!(groups, blue, "one group per kind, in unit order");

    // A settler comes ahead of the military, wherever it is in unit order.
    let last = game
        .units
        .iter()
        .rposition(|u| u.team == PLAYER_TEAM)
        .unwrap();
    let settler = game.units[last].id;
    game.settlers.insert(settler);
    let unit_type = game.units[last].unit_type;
    assert_eq!(roster_keys(&game)[2], RosterKey::Group(unit_type, true));

    // The production chip opens the city, and is framed while it's open.
    game.handle_click(
        roster_cursor(&game, RosterKey::Production(city)),
        SCREEN,
        ClickMode::Normal,
    );
    let open = game.selected_city.expect("the city opened");
    assert_eq!(game.cities[open].id, city);
    assert!(roster(&game)[0].1 && !roster(&game)[1].1 && !roster(&game)[2].1);

    // Once it has a build, it leaves the strip.
    game.cities[open].queue.push(Build::Unit(BuildUnit::Melee));
    let keys = roster_keys(&game);
    assert!(
        !keys.iter().any(|k| matches!(k, RosterKey::Production(_))),
        "{keys:?}"
    );
}

#[test]
fn a_group_chip_counts_its_units_and_opens_a_row_of_them() {
    let mut game = GameState::new();
    let blue: Vec<usize> = (0..game.units.len())
        .filter(|&i| game.is_player_controlled(i))
        .collect();
    // Three melee and one siege.
    for &i in &blue[..3] {
        game.units[i].unit_type = UnitType::Melee;
    }
    game.units[blue[3]].unit_type = UnitType::Siege;
    let melee = RosterKey::Group(UnitType::Melee, false);
    let siege = RosterKey::Group(UnitType::Siege, false);
    game.clear_selection();
    assert_eq!(
        roster(&game),
        vec![(melee, false, 3), (siege, false, 1)],
        "one chip per kind, counting its units"
    );

    // Clicking a group selects all of it and lists its units one by one.
    game.handle_click(roster_cursor(&game, melee), SCREEN, ClickMode::Normal);
    assert_eq!(game.selection(), blue[..3].to_vec());
    let ids: Vec<u32> = blue[..3].iter().map(|&i| game.units[i].id).collect();
    let row: Vec<RosterKey> = ids.iter().map(|&id| RosterKey::Unit(id)).collect();
    assert_eq!(roster_keys(&game)[2..], row[..], "the open row");
    assert!(roster(&game)[2..].iter().all(|c| c.1), "all framed");

    // From the row: Ctrl-click takes one out, a plain click picks one.
    game.handle_click(roster_cursor(&game, row[0]), SCREEN, ClickMode::Swap);
    assert_eq!(game.selection(), blue[1..3].to_vec());
    assert!(!roster(&game)[2].1, "no longer framed");
    game.handle_click(roster_cursor(&game, row[2]), SCREEN, ClickMode::Normal);
    assert_eq!(game.selection(), vec![blue[2]]);
    assert_eq!(
        roster_keys(&game).len(),
        5,
        "the row stays while one is selected"
    );

    // Shift-clicking another group adds all of it; Ctrl-clicking takes it out.
    game.handle_click(roster_cursor(&game, siege), SCREEN, ClickMode::QueueMove);
    assert_eq!(game.selection(), vec![blue[2], blue[3]]);
    game.handle_click(roster_cursor(&game, siege), SCREEN, ClickMode::Swap);
    assert_eq!(game.selection(), vec![blue[2]]);

    // Selecting something else closes the row.
    game.handle_click(roster_cursor(&game, siege), SCREEN, ClickMode::Normal);
    assert_eq!(game.selection(), vec![blue[3]]);
    assert_eq!(roster_keys(&game), vec![melee, siege]);
    assert!(
        game.units.iter().all(|u| !u.has_queue()),
        "nothing got orders"
    );
}

#[test]
fn a_unit_with_orders_leaves_its_group() {
    let mut game = GameState::new();
    let blue: Vec<usize> = (0..game.units.len())
        .filter(|&i| game.is_player_controlled(i))
        .collect();
    for &i in &blue {
        game.units[i].unit_type = UnitType::Melee;
    }
    let melee = RosterKey::Group(UnitType::Melee, false);
    assert_eq!(roster(&game)[0].2, blue.len());
    game.set_selection(vec![blue[0]]);
    game.hold_selected_unit();
    assert_eq!(roster(&game)[0].0, melee);
    assert_eq!(roster(&game)[0].2, blue.len() - 1);
}

#[test]
fn the_turn_strip_sits_at_the_bottom_and_clears_every_panel() {
    // Alone at the bottom, it's centered.
    let mut game = GameState::new();
    game.clear_selection();
    let layout = game.layout(SCREEN);
    let &(min, max, _) = &layout.roster_chips[0];
    let panel = layout
        .panels
        .iter()
        .find(|&&(pmin, pmax)| min.cmpge(pmin).all() && max.cmple(pmax).all())
        .unwrap();
    assert!(panel.0.y <= MARGIN + 1.0, "at the bottom");
    assert!(
        ((panel.0.x + panel.1.x) / 2.0 - SCREEN.x / 2.0).abs() <= 1.0,
        "centered"
    );

    // It never overlaps another panel, with or without a city open.
    let mut game = GameState::city_scenario();
    for open_city in [false, true] {
        if open_city {
            game.select_city();
        }
        let layout = game.layout(SCREEN);
        assert!(!layout.roster_chips.is_empty());
        for (a, &(amin, amax)) in layout.panels.iter().enumerate() {
            for &(bmin, bmax) in &layout.panels[a + 1..] {
                let (a, b) = (
                    Rect {
                        min: amin,
                        max: amax,
                    },
                    Rect {
                        min: bmin,
                        max: bmax,
                    },
                );
                assert!(!a.overlaps(b, 0.0), "{a:?} overlaps {b:?}");
            }
        }
        for &(min, max, _) in &layout.roster_chips {
            assert!(
                layout
                    .panels
                    .iter()
                    .any(|&(pmin, pmax)| { min.cmpge(pmin).all() && max.cmple(pmax).all() }),
                "every chip sits inside the strip's panel"
            );
        }
    }
}

#[test]
fn clear_orders_drops_a_groups_queues() {
    let mut game = GameState::new();
    game.fog_of_war = false;
    let blue: Vec<usize> = (0..game.units.len())
        .filter(|&i| game.is_player_controlled(i))
        .take(2)
        .collect();
    game.set_selection(blue.clone());
    assert!(game.queue_move(Hex::new(0, 1)));
    assert!(blue.iter().any(|&i| game.units[i].has_queue()));
    game.handle_click(
        button_cursor(&game, Target::Unit(UnitAction::ClearOrders)),
        SCREEN,
        ClickMode::Normal,
    );
    for &i in &blue {
        let unit = &game.units[i];
        assert!(!unit.has_queue() && unit.planned_move.is_none());
    }
    let clear = game
        .layout(SCREEN)
        .buttons
        .into_iter()
        .find(|b| b.target == Target::Unit(UnitAction::ClearOrders))
        .unwrap();
    assert_eq!(clear.state, ButtonState::Disabled, "nothing left to clear");
}

#[test]
fn the_settings_menu_opens_centered_over_the_panels_and_its_buttons_work() {
    let playback = Setting::TurnPlayback;
    let down = Target::StepSetting(playback, -1);
    let up = Target::StepSetting(playback, 1);
    // Nothing selected, a unit selected, and a city with its queue open.
    let mut plain = GameState::new();
    plain.clear_selection();
    let unit = GameState::new();
    let mut city = GameState::city_scenario();
    city.select_city();
    city.queue_selected_city_unit(BuildUnit::Melee);
    for mut game in [plain, unit, city] {
        assert!(!game.layout(SCREEN).buttons.iter().any(|b| b.target == up));
        game.settings_open = true;
        for screen in [SCREEN, Vec2::new(1280.0, 720.0)] {
            let layout = game.layout(screen);
            // The menu is placed last, centered; the docked panels still
            // keep clear of each other underneath it.
            let (&menu_rect, docked) = layout.panels.split_last().unwrap();
            let center = (menu_rect.0 + menu_rect.1) / 2.0;
            assert!(
                (center - screen / 2.0).abs().max_element() <= 1.0,
                "{center}"
            );
            for (i, &(a_min, a_max)) in docked.iter().enumerate() {
                for &(b_min, b_max) in &docked[i + 1..] {
                    let apart = a_max.x <= b_min.x
                        || b_max.x <= a_min.x
                        || a_max.y <= b_min.y
                        || b_max.y <= a_min.y;
                    assert!(apart, "panels overlap at {screen}");
                }
            }
            let close = layout
                .buttons
                .iter()
                .find(|b| b.target == Target::CloseSettings)
                .expect("Close shown");
            assert!(contains(menu_rect.0, menu_rect.1, close.min));
            for target in [down, up, Target::Quit] {
                let button = layout.buttons.iter().find(|b| b.target == target).unwrap();
                assert!(contains(menu_rect.0, menu_rect.1, button.min));
                assert!(contains(menu_rect.0, menu_rect.1, button.max));
                // A panel underneath never takes the menu's clicks.
                let middle = (button.min + button.max) / 2.0;
                assert_eq!(layout.button_at(middle).unwrap().target, target);
            }
        }

        // All at once by default: > is spent, < steps down, and then < is.
        let state = |game: &GameState, target| {
            game.layout(SCREEN)
                .buttons
                .iter()
                .find(|b| b.target == target)
                .unwrap()
                .state
        };
        assert!(game.settings.instant_playback);
        assert_eq!(state(&game, up), ButtonState::Disabled);
        game.handle_click(button_cursor(&game, down), SCREEN, ClickMode::Normal);
        assert!(!game.settings.instant_playback);
        assert_eq!(state(&game, down), ButtonState::Disabled);
        game.handle_click(button_cursor(&game, down), SCREEN, ClickMode::Normal);
        assert!(
            !game.settings.instant_playback,
            "a spent button does nothing"
        );
        game.handle_click(button_cursor(&game, up), SCREEN, ClickMode::Normal);
        assert!(game.settings.instant_playback);

        let close = button_cursor(&game, Target::CloseSettings);
        game.handle_click(close, SCREEN, ClickMode::Normal);
        assert!(!game.settings_open);
        assert!(!game.quit_requested());
    }
}

#[test]
fn the_settings_menu_quit_button_asks_the_app_to_quit() {
    let mut game = GameState::new();
    game.clear_selection();
    game.press_escape();
    assert!(!game.quit_requested());
    let quit = button_cursor(&game, Target::Quit);
    game.handle_click(quit, SCREEN, ClickMode::Normal);
    assert!(game.quit_requested());
}

#[test]
fn the_settings_menu_shows_every_setting_and_its_value() {
    let game = GameState::new();
    let text = panel_strings(|panel| *panel = game.settings_panel_content());
    for setting in Setting::ALL {
        let value = setting.value_text(game.settings.get(setting));
        assert_shows(&text, setting.name());
        assert_shows(&text, &value);
    }
    let tooltip = |target| {
        let button = Button {
            target,
            label: String::new(),
            hint: String::new(),
            state: ButtonState::Ready,
            armed: false,
            faded: false,
            min: Vec2::ZERO,
            max: Vec2::ZERO,
        };
        line_strings(game.tooltip_lines(&button).into_iter().map(|(_, l)| l))
    };
    let spent = tooltip(Target::StepSetting(Setting::TurnPlayback, 1));
    assert_shows(&spent, "TURN PLAYBACK");
    assert_shows(&spent, "ALREADY ALL AT ONCE");
    let open = tooltip(Target::StepSetting(Setting::TurnPlayback, -1));
    assert!(!open.iter().any(|line| line.contains("ALREADY")));
}
