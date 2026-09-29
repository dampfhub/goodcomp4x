use super::builder::{ButtonSpec, Row, classic_rows, flat_rows};
use super::text::{end_turn_label, price_hint, quantity, signed_quantity, wrap};
use super::*;

use crate::game::PLAYER_TEAM;
use crate::game::city::{Build, Queued, Stock, stock_icons};
use crate::game::map_icons::{FOOD_ICON, METAL_ICON, TIME_ICON, WOOD_ICON};
use crate::game::orders::ClickMode;
use crate::game::unit::{Team, Unit, UnitType};

const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

#[test]
fn action_toolbar_is_compact_and_every_icon_keeps_its_click_target() {
    let game = GameState::city_scenario();
    let unit = game
        .units
        .iter()
        .position(|u| u.team == PLAYER_TEAM && u.unit_type == UnitType::Melee)
        .unwrap();
    let targets: Vec<_> = game
        .unit_buttons(unit)
        .iter()
        .map(|button| button.target)
        .collect();
    let mut panel = PanelBuilder::default();
    panel.action_toolbar(game.unit_buttons(unit));
    assert!(
        panel.size().x <= 280.0,
        "icon toolbar should fit five columns"
    );
    assert!(
        panel.size().y <= 175.0,
        "icons should use only two short rows"
    );
    let mut layout = Layout::default();
    panel.place_bottom_left(Vec2::ZERO, &mut layout);
    for target in targets {
        let button = layout
            .buttons
            .iter()
            .find(|button| button.target == target)
            .unwrap();
        assert_eq!(
            button.max - button.min,
            Vec2::splat(action_icons::ICON_BUTTON_SIZE)
        );
        assert_eq!(
            layout
                .button_at((button.min + button.max) / 2.0)
                .map(|hit| hit.target),
            Some(target)
        );
    }

    let mut city_panel = PanelBuilder::default();
    game.city_tray(0, &mut city_panel);
    let focus = city_panel
        .rows
        .iter()
        .find_map(|row| match row {
            builder::Row::Buttons(buttons, _)
                if buttons.iter().any(|b| matches!(b.target, Target::Focus(_))) =>
            {
                Some(buttons)
            }
            _ => None,
        })
        .expect("labor focus buttons");
    assert!(builder::icon_row(focus));
}

#[test]
fn hovering_a_button_shows_its_tooltip() {
    let mut game = GameState::city_scenario();
    game.select_city();
    let card = button_cursor(&game, Target::Build(BuildUnit::Siege));
    let plain = game.build_ui(SCREEN, None).len();
    assert!(game.build_ui(SCREEN, Some(card)).len() > plain);
}

#[test]
fn building_catalog_scrolls_with_clickable_cards_inside_the_city_tray() {
    let mut game = GameState::city_scenario();
    game.select_city();
    let first = game.layout(SCREEN);
    let region = first
        .building_scrollbars
        .first()
        .expect("nested building list");
    assert!(region.max_offset > 0);
    let cursor = to_ui((region.min + region.max) / 2.0, SCREEN);
    assert!(
        first
            .buttons
            .iter()
            .any(|button| button.target == Target::Build(BuildUnit::Melee))
    );
    assert!(
        !first
            .buttons
            .iter()
            .any(|button| button.target == Target::Building(Building::Railhead))
    );
    // Past the units, most of the way down the buildings (the works
    // follow them).
    assert!(game.scroll_buildings_at(cursor, SCREEN, -16.0));
    let scrolled = game.layout(SCREEN);
    let card = scrolled
        .buttons
        .iter()
        .find(|button| button.target == Target::Building(Building::Railhead))
        .expect("railhead card shown after scrolling");
    let middle = (card.min + card.max) / 2.0;
    assert_eq!(
        scrolled.button_at(middle).map(|button| button.target),
        Some(Target::Building(Building::Railhead))
    );
    let scrolled_region = scrolled.building_scrollbars.first().unwrap();
    assert!(contains(scrolled_region.min, scrolled_region.max, middle));
    assert!(
        !scrolled
            .buttons
            .iter()
            .any(|button| button.target == Target::Building(Building::Harbor))
    );
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

/// Scroll the shared city production catalogue until a requested card is visible.
fn catalog_cursor(game: &mut GameState, target: Target) -> Vec2 {
    let city = game.selected_city.expect("city view open");
    for offset in 0..64 {
        game.cities[city].building_scroll = offset;
        if game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == target)
        {
            return button_cursor(game, target);
        }
    }
    panic!("production card not shown: {target:?}");
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
fn alert_button_puts_the_selected_unit_on_alert_in_both_presentations() {
    let alert = Target::Unit(UnitAction::Alert);
    // Classic.
    let mut game = GameState::new();
    let first = game.selected.unwrap();
    assert_eq!(game.units[first].unit_type, UnitType::Melee);
    game.handle_click(button_cursor(&game, alert), SCREEN, ClickMode::Normal);
    assert!(game.units[first].alert);
    assert_ne!(game.selected, Some(first), "selection moves on");
    // Reselected, it shows as on.
    game.selected = Some(first);
    let layout = game.layout(SCREEN);
    let button = layout.buttons.iter().find(|b| b.target == alert).unwrap();
    assert_eq!(button.state, ButtonState::Queued);
    assert_eq!(
        layout
            .button_at((button.min + button.max) / 2.0)
            .map(|b| b.target),
        Some(alert)
    );
    // ImGui.
    let mut game = GameState::new();
    let first = game.selected.unwrap();
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, alert);
    assert!(game.units[first].alert);
}

#[test]
fn only_troops_that_can_go_on_alert_show_its_button() {
    let mut game = GameState::city_scenario();
    let targets = |game: &GameState, idx: usize| -> Vec<Target> {
        game.unit_buttons(idx).iter().map(|b| b.target).collect()
    };
    let alert = Target::Unit(UnitAction::Alert);
    let melee = game
        .units
        .iter()
        .position(|u| u.team == PLAYER_TEAM && u.unit_type == UnitType::Melee)
        .unwrap();
    assert!(targets(&game, melee).contains(&alert));
    game.units[melee].unit_type = UnitType::Scout;
    assert!(!targets(&game, melee).contains(&alert));
    // A siege shows it, unavailable until it's set up.
    game.units[melee].unit_type = UnitType::Siege;
    let state = |game: &GameState| {
        game.unit_buttons(melee)
            .into_iter()
            .find(|b| b.target == alert)
            .unwrap()
            .state
    };
    assert_eq!(state(&game), ButtonState::Disabled);
    game.units[melee].deployed = true;
    assert_eq!(state(&game), ButtonState::Ready);
    // A settler, never.
    game.units[melee].unit_type = UnitType::Melee;
    let id = game.units[melee].id;
    game.settlers.insert(id);
    assert!(!targets(&game, melee).contains(&alert));
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
    assert_eq!(
        game.cities[city].queue,
        [Queued::new(Build::Unit(BuildUnit::Siege))]
    );
}

#[test]
fn harbor_reveals_naval_build_cards_in_the_shared_city_tray() {
    let mut game = GameState::naval_scenario();
    game.fund(Team::Blue);
    game.select_city();
    for build in [
        BuildUnit::PatrolGalley,
        BuildUnit::LandingCraft,
        BuildUnit::BombardShip,
    ] {
        let card = catalog_cursor(&mut game, Target::Build(build));
        assert!(game.layout(SCREEN).button_at(to_ui(card, SCREEN)).is_some());
    }
    let landing_craft = catalog_cursor(&mut game, Target::Build(BuildUnit::LandingCraft));
    game.handle_click(landing_craft, SCREEN, ClickMode::Normal);
    let city = game.selected_city.unwrap();
    assert!(
        game.cities[city]
            .queue
            .iter()
            .any(|q| q.build == Build::Unit(BuildUnit::LandingCraft))
    );
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

/// The Cities scenario with city 0 open, the camera settled on it and its
/// side rich.
fn city_view() -> GameState {
    let mut game = GameState::city_scenario();
    // Enough in the stockpile for whatever a test queues.
    game.fund(Team::Blue);
    game.select_city();
    game.update(10.0);
    game
}

#[test]
fn invalid_stable_site_click_reports_horses_instead_of_open_land() {
    let mut game = city_view();
    game.units.clear();
    game.fog_of_war = false;
    let city = game.selected_city.unwrap();
    // Picked first, so the tile is found clear of the panels as they are
    // when it's clicked.
    game.queue_selected_city_building(Building::Stable);
    let site = visible_sites(&game, city, 1)
        .into_iter()
        .chain(
            game.grid
                .all_hexes()
                .filter(|&hex| visible_in_reach(&game, city, hex)),
        )
        .find(|&hex| {
            game.site_available(city, Building::Barracks, hex)
                && game.ai_site_issue(city, Building::Stable, hex)
                    == Some("NEEDS HORSES ON OR NEXT TO THE TILE")
        })
        .expect("visible open tile in reach away from horses");
    game.handle_click(hex_cursor(&game, site), SCREEN, ClickMode::Normal);
    assert_eq!(game.notice, "NEEDS HORSES ON OR NEXT TO THE TILE");
    assert_eq!(game.placing_job, Some(JobKind::Build(Building::Stable)));
    assert!(game.cities[city].worker_jobs.is_empty());
}

/// Whether `hex` is on screen, clear of the panels, and in city `city`'s
/// workers' reach: somewhere a click would place a building.
fn visible_in_reach(game: &GameState, city: usize, hex: Hex) -> bool {
    let cursor = hex_cursor(game, hex);
    (0.0..SCREEN.x).contains(&cursor.x)
        && (0.0..SCREEN.y).contains(&cursor.y)
        && !game.layout(SCREEN).covers(to_ui(cursor, SCREEN))
        && game.in_worker_reach(game.cities[city].team, hex)
}

/// Up to `count` tiles a Barracks could go on where a click would place it.
fn visible_sites(game: &GameState, city: usize, count: usize) -> Vec<Hex> {
    let mut sites: Vec<Hex> = game
        .grid
        .all_hexes()
        .filter(|&hex| {
            game.site_available(city, Building::Barracks, hex) && visible_in_reach(game, city, hex)
        })
        .collect();
    sites.sort_by_key(|h| (h.q, h.r));
    sites.truncate(count);
    sites
}

#[test]
fn a_building_card_then_a_map_click_places_it_for_the_workers() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    let button = catalog_cursor(&mut game, Target::Building(Building::Barracks));
    game.handle_click(button, SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, Some(JobKind::Build(Building::Barracks)));

    // Visible map tiles, so this exercises UI hit testing as well as
    // placing, rather than calling place_job_at directly.
    let sites = visible_sites(&game, city, 2);
    assert_eq!(sites.len(), 2);
    game.handle_click(hex_cursor(&game, sites[0]), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, None, "one of each: done placing");
    let placed = |game: &GameState| {
        game.cities[city]
            .worker_jobs
            .iter()
            .filter(|j| j.kind == JobKind::Build(Building::Barracks))
            .map(|j| j.hex)
            .collect::<Vec<_>>()
    };
    assert_eq!(placed(&game), vec![sites[0]]);
    game.update_hover(Some(hex_cursor(&game, sites[1])), SCREEN, 0.1);
    game.handle_click(hex_cursor(&game, sites[1]), SCREEN, ClickMode::Normal);
    assert_eq!(
        placed(&game),
        vec![sites[0]],
        "a later click doesn't move it"
    );
}

/// The state of `building`'s card in the open city's tray.
fn building_card(game: &mut GameState, building: Building) -> ButtonState {
    catalog_cursor(game, Target::Building(building));
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
fn ending_the_turn_while_placing_a_building_stops_placing_it() {
    // #51: End Turn closed the city but kept placing, so the preview
    // followed the cursor with no city open.
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    // Something to build, so the city isn't left empty-handed.
    game.queue_selected_city_worker();
    let card = catalog_cursor(&mut game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert!(game.placing_job.is_some());
    play_turn(&mut game);
    assert_eq!(game.selected_city, None);
    assert_eq!(game.placing_job, None);
    assert!(!game.building_job_queued(city, Building::Barracks));
}

#[test]
fn a_building_card_shows_placing_then_placed_until_taken_off() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    assert_eq!(
        building_card(&mut game, Building::Barracks),
        ButtonState::Ready
    );
    let card = catalog_cursor(&mut game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(
        building_card(&mut game, Building::Barracks),
        ButtonState::Queued,
        "gold while being placed"
    );

    // The first Escape only stops placing; the city stays open, and
    // nothing was paid.
    let stock = game.stock(PLAYER_TEAM);
    assert!(game.exit_structure_menu());
    assert_eq!(game.selected_city, Some(city));
    assert_eq!(game.placing_job, None);
    assert_eq!(game.stock(PLAYER_TEAM), stock);
    assert_eq!(
        building_card(&mut game, Building::Barracks),
        ButtonState::Ready
    );

    // Placed, the card stays gold until the job is taken off the list.
    game.queue_selected_city_building(Building::Barracks);
    assert!(game.place_job_at(Hex::new(-2, 0), None));
    assert!(game.exit_structure_menu());
    assert_eq!(game.selected_city, None, "nothing to stop: Escape closes");
    game.open_city(city);
    assert_eq!(
        building_card(&mut game, Building::Barracks),
        ButtonState::Queued
    );
    game.remove_worker_job(0);
    assert_eq!(
        building_card(&mut game, Building::Barracks),
        ButtonState::Ready
    );
    // With no worker, nothing can be placed: the card is dimmed.
    game.cities[city].workers = 0;
    assert_eq!(
        building_card(&mut game, Building::Barracks),
        ButtonState::Disabled
    );
}

#[test]
fn opening_another_view_stops_placing() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.queue_selected_city_building(Building::Mill);
    assert_eq!(game.placing_job, Some(JobKind::Build(Building::Mill)));
    game.open_barracks(city);
    assert_eq!(game.placing_job, None);
    assert!(game.cities[city].worker_jobs.is_empty());
}

#[test]
fn mill_and_workshop_cards_are_placed_the_same_way() {
    let mut game = city_view();
    game.units.clear();
    let city = game.selected_city.unwrap();
    for (building, site) in [
        (Building::Mill, Hex::new(-2, 0)),
        (Building::Workshop, Hex::new(-1, 0)),
    ] {
        let card = catalog_cursor(&mut game, Target::Building(building));
        game.handle_click(card, SCREEN, ClickMode::Normal);
        assert_eq!(game.placing_job, Some(JobKind::Build(building)));
        assert!(game.place_job_at(site, None));
        assert!(
            game.cities[city]
                .worker_jobs
                .contains(&crate::game::workers::WorkerJob::on_tile(
                    site,
                    JobKind::Build(building)
                ))
        );
    }
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
    game.cities[city].barracks_queue = vec![Queued::new(BuildUnit::Melee); 12];
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
fn reordered_builds_show_saved_work_in_city_and_barracks_rows() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].queue = vec![
        Queued::worked(Build::Unit(BuildUnit::Melee), 4),
        Queued::new(Build::Unit(BuildUnit::Ranged)),
    ];
    game.reorder_queue(QueueKind::City, 0, 1);
    let mut panel = PanelBuilder::default();
    game.city_queue_panel(city, 2, &mut panel);
    assert!(panel.rows.iter().any(|row| matches!(
        row,
        Row::QueueItem(item) if item.index == 1 && item.label.contains("SAVED")
    )));
    assert_eq!(game.cities[city].queue[1].progress, 4);
    let mut layout = Layout::default();
    panel.place_bottom_left(Vec2::ZERO, &mut layout);
    assert!(
        layout
            .queue_items
            .iter()
            .any(|row| { row.kind == QueueKind::City && row.index == 1 })
    );

    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.cities[city].barracks_queue = vec![
        Queued::worked(BuildUnit::Melee, 4),
        Queued::new(BuildUnit::Ranged),
    ];
    game.reorder_queue(QueueKind::Barracks, 0, 1);
    let mut panel = PanelBuilder::default();
    game.barracks_queue_panel(city, 2, &mut panel);
    assert!(panel.rows.iter().any(|row| matches!(
        row,
        Row::QueueItem(item) if item.index == 1 && item.label.contains("SAVED")
    )));
    assert_eq!(game.cities[city].barracks_queue[1].progress, 4);
    let mut layout = Layout::default();
    panel.place_bottom_left(Vec2::ZERO, &mut layout);
    assert!(
        layout
            .queue_items
            .iter()
            .any(|row| { row.kind == QueueKind::Barracks && row.index == 1 })
    );
}

#[test]
fn queue_rows_drag_to_reorder_and_x_removes_without_dragging() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].queue = [BuildUnit::Melee, BuildUnit::Ranged, BuildUnit::Siege]
        .map(|unit| Queued::new(Build::Unit(unit)))
        .to_vec();
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
    assert_eq!(
        game.cities[city].queue[0].build,
        Build::Unit(BuildUnit::Siege)
    );
    assert!(game.queue_drag.is_none());

    let x = button_cursor(&game, Target::CityQueueRemove(1));
    assert!(!game.start_queue_drag_at(x, SCREEN));
    game.handle_click(x, SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[city].queue.len(), 2);
}

#[test]
fn barracks_queue_rows_use_the_same_drag_and_remove_targets() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.open_barracks(city);
    game.cities[city].barracks_queue = vec![
        Queued::new(BuildUnit::Melee),
        Queued::new(BuildUnit::Ranged),
    ];
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
    assert_eq!(game.cities[city].barracks_queue[0].build, BuildUnit::Ranged);
    let x = button_cursor(&game, Target::BarracksQueueRemove(1));
    assert!(!game.start_queue_drag_at(x, SCREEN));
    game.handle_click(x, SCREEN, ClickMode::Normal);
    assert_eq!(
        game.cities[city].barracks_queue,
        [Queued::new(BuildUnit::Ranged)]
    );
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
    assert_eq!(counts[1], 3);
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
fn city_view_clicks_select_a_unit_only_on_its_token() {
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
    let pos = game.units[unit].pos;
    // Off the token, on its hex: the city keeps the click.
    let off_token = game
        .camera
        .world_to_screen(pos.to_world() + Vec2::new(0.6, 0.0), SCREEN);
    game.handle_click(off_token, SCREEN, ClickMode::Normal);
    assert!(game.selected_city.is_some());
    assert_ne!(game.selected, Some(unit));
    // On the token (drawn in the middle of the hex): the unit.
    game.handle_click(hex_cursor(&game, pos), SCREEN, ClickMode::Normal);
    assert_eq!(game.selected_city, None);
    assert_eq!(game.selected, Some(unit));
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
        button_cursor(&game, Target::OpenSettings),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(game.settings_open);
    game.handle_click(
        button_cursor(&game, Target::ToggleYields),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(!game.show_yields);
    assert!(game.build_vertices().len() < shown, "badges hidden");
    game.toggle_yields();
    assert_eq!(game.build_vertices().len(), shown);
    game.close_settings();

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
        assert!(game.structure_inspect_panel(hex).is_none());
        game.fog_of_war = false;
        assert!(game.structure_inspect_panel(hex).is_some());
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
    assert!(game.structure_inspect_panel(red_pos).is_none());
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
fn every_panel_shows_what_a_city_delivers_in_displayed_units() {
    let mut game = GameState::city_scenario();
    game.fog_of_war = false;
    game.units.clear();
    let income = game.income(0);
    // Stored in quarters: a raw value would read four times too high.
    assert!(income.food > 4 && income.wood > 4);
    let short = price_hint(income);
    assert!(short.contains(&format!("{FOOD_ICON}{}", quantity(income.food))));

    let tray = panel_strings(|panel| game.city_tray(0, panel));
    assert_shows(
        &tray,
        &format!("{WOOD_ICON}{}", signed_quantity(income.wood)),
    );
    assert_shows(
        &tray,
        &format!("{FOOD_ICON}{}", signed_quantity(income.food)),
    );
    let hover = panel_strings(|panel| game.structure_hover_panel(0, false, panel));
    assert_shows(&hover, &format!("DELIVERS {short}"));
    let city_tooltip = line_strings(
        game.tile_tooltip_lines(game.cities[0].pos)
            .into_iter()
            .map(|(_, line)| line),
    );
    assert_shows(&city_tooltip, &format!("DELIVERS {short}"));
}

#[test]
fn the_stockpile_shows_in_the_top_bar_with_its_change_a_turn() {
    let mut game = GameState::city_scenario();
    game.stockpiles[Team::Blue.index()] = Stock::whole(12, 7, 3);
    let income = game.side_income(Team::Blue);
    let food_change = income.food - game.upkeep(Team::Blue);
    let text: String = game
        .stockpile_line()
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert!(text.starts_with(&format!("{FOOD_ICON}12 ")), "{text}");
    assert!(text.contains(&format!("{WOOD_ICON}7 ")), "{text}");
    assert!(text.contains(&format!("{METAL_ICON}3 ")), "{text}");
    assert!(
        text.contains(&signed_quantity(food_change)),
        "net food in {text}"
    );
}

/// The button for `target` in the classic layout.
fn find_button(game: &GameState, target: Target) -> Button {
    let layout = game.layout(SCREEN);
    let button = layout.buttons.iter().find(|b| b.target == target);
    let button = button.unwrap_or_else(|| panic!("{target:?} shown"));
    Button {
        target: button.target,
        label: button.label.clone(),
        hint: button.hint.clone(),
        state: button.state,
        armed: button.armed,
        faded: button.faded,
        min: button.min,
        max: button.max,
    }
}

#[test]
fn build_cards_show_prices_and_queue_what_the_stockpile_cannot_pay_yet() {
    let mut game = GameState::city_scenario();
    game.open_city(0);
    let melee = find_button(&game, Target::Build(BuildUnit::Melee));
    assert_eq!(melee.state, ButtonState::Ready);
    let price = format!("{FOOD_ICON}2 {WOOD_ICON}6");
    assert!(melee.hint.contains(&price), "{}", melee.hint);
    // The city center takes twice a Barracks' turns.
    assert!(
        melee.hint.ends_with(&format!("{TIME_ICON}4")),
        "{}",
        melee.hint
    );
    let grow = find_button(&game, Target::Grow);
    assert!(grow.label.starts_with("GROW TO 3"), "{}", grow.label);

    game.stockpiles[Team::Blue.index()] = Stock::default();
    let tooltip = |game: &GameState, card: &Button| -> String {
        game.tooltip_lines(card)
            .into_iter()
            .flat_map(|(_, line)| line.into_iter().map(|(text, _)| text))
            .collect()
    };
    // A queue's cards can be queued whatever the stockpile holds; the
    // tooltip says what it's short of this turn, and that it'll wait.
    for target in [Target::Build(BuildUnit::Melee), Target::Grow] {
        let card = find_button(&game, target);
        assert_eq!(card.state, ButtonState::Ready, "{target:?}");
        let text = tooltip(&game, &card);
        assert!(
            text.contains("SHORT OF") && text.contains("WAITS"),
            "{target:?}: {text}"
        );
    }
    // A building is still paid when placed: its card is dimmed.
    game.cities[0].building_scroll = 5;
    let card = find_button(&game, Target::Building(Building::Barracks));
    assert_eq!(card.state, ButtonState::Disabled);
    assert!(tooltip(&game, &card).contains("SHORT OF"));
    game.cities[0].building_scroll = 0;

    // Clicked, the card queues its unit, unpaid, and the row says what it
    // waits for.
    game.handle_click(
        button_cursor(&game, Target::Build(BuildUnit::Melee)),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(
        game.cities[0].queue,
        [Queued::new(Build::Unit(BuildUnit::Melee))]
    );
    assert_eq!(game.stock(Team::Blue), Stock::default());
    let short = game
        .expected_stock(Team::Blue)
        .shortfall(BuildUnit::Melee.price());
    let layout = game.layout(SCREEN);
    let row = layout
        .shapes
        .iter()
        .find_map(|shape| match shape {
            Shape::QueueItem { label, waiting, .. } => Some((label.clone(), *waiting)),
            _ => None,
        })
        .expect("the queue's row");
    assert!(row.1, "tinted as waiting");
    assert!(
        row.0.contains(&format!("WAITS {}", stock_icons(short))),
        "{}",
        row.0
    );
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
    // Production first, then units: the turn's order.
    assert_eq!(end_turn_label((3, 1)), "CHOOSE PRODUCTION");
    assert_eq!(end_turn_label((0, 2)), "2 CITIES NEED PRODUCTION");
    assert_eq!(end_turn_label((3, 0)), "3 UNITS NEED ORDERS");
    assert_eq!(end_turn_label((1, 0)), "UNIT NEEDS ORDERS");
    assert_eq!(end_turn_label((0, 0)), "END TURN");

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
/// selected. `open_city_zero` opens the city to place things for its
/// workers.
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

/// Opens Blue's city, where its workers' jobs are placed from, with the
/// camera settled on it.
fn open_city_zero(game: &mut GameState) {
    game.open_city(0);
    game.camera.update(10.0);
}

/// The state of the production card for `target` in the open city.
fn card_state(game: &mut GameState, target: Target) -> ButtonState {
    catalog_cursor(game, target);
    game.layout(SCREEN)
        .buttons
        .iter()
        .find(|b| b.target == target)
        .expect("card shown")
        .state
}

#[test]
fn a_work_card_in_the_city_places_it_on_the_map() {
    let (mut game, hex) = empty_tile_near_blue_city();
    open_city_zero(&mut game);
    for kind in JobKind::ALL {
        catalog_cursor(&mut game, Target::WorkerJob(kind));
    }
    let road = Target::WorkerJob(JobKind::Road);
    let card = catalog_cursor(&mut game, road);
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, Some(JobKind::Road));
    assert_eq!(
        card_state(&mut game, road),
        ButtonState::Queued,
        "the card shows it's picked"
    );
    // (The map click goes straight to the map: a panel may cover the tile.)
    game.handle_map_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[0].worker_jobs.len(), 1);
    assert_eq!(game.cities[0].worker_jobs[0].hex, hex);
    assert_eq!(game.cities[0].worker_jobs[0].kind, JobKind::Road);
    assert_eq!(
        game.placing_job,
        Some(JobKind::Road),
        "still picked, for the next one"
    );
    // Picking it again puts it down.
    let card = catalog_cursor(&mut game, road);
    game.handle_click(card, SCREEN, ClickMode::Normal);
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
        "what workers build is placed from its city"
    );
}

#[test]
fn a_wall_is_placed_by_dragging_along_hex_edges() {
    let (mut game, hex) = empty_tile_near_blue_city();
    open_city_zero(&mut game);
    let wall = Target::WorkerJob(JobKind::Wall);
    let card = catalog_cursor(&mut game, wall);
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, Some(JobKind::Wall));
    assert_eq!(card_state(&mut game, wall), ButtonState::Queued);

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
    let card = catalog_cursor(&mut game, wall);
    assert!(!game.paint_job_at(card, SCREEN, true));

    // Escape stops placing but leaves the city open.
    game.press_escape();
    assert_eq!(game.placing_job, None);
    assert_eq!(game.selected_city, Some(0));
    let n = sides[0];
    assert!(!game.paint_job_at(edge_cursor(&game, n), SCREEN, false));
}

#[test]
fn a_worker_out_can_be_recalled_and_released_from_the_city_panel() {
    let (mut game, hex) = empty_tile_near_blue_city();
    open_city_zero(&mut game);
    game.placing_job = Some(JobKind::Fort);
    assert!(game.place_job_at(hex, None));
    game.placing_job = None;
    // Two hexes out, one a turn.
    game.resolve_workers();
    game.resolve_workers();
    let id = game.field_workers[0].id;
    assert_eq!(game.field_workers[0].pos, hex);

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

    // Home, it's held there with a Release button in its city's panel.
    let release = Target::ReleaseWorker;
    let shown = |game: &GameState| {
        game.layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == release)
    };
    game.resolve_workers();
    assert!(!shown(&game), "still walking home");
    game.resolve_workers();
    assert!(game.field_workers.is_empty());
    assert_eq!(game.cities[0].held_workers, 1);
    let layout = game.layout(SCREEN);
    let button = layout.buttons.iter().find(|b| b.target == release).unwrap();
    assert_eq!(button.label, "HELD AT HOME - RELEASE");
    assert!(
        layout
            .panels
            .iter()
            .any(|&(min, max)| contains(min, max, button.min) && contains(min, max, button.max))
    );
    game.resolve_workers();
    assert!(game.field_workers.is_empty(), "held while its job waits");
    game.handle_click(button_cursor(&game, release), SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[0].held_workers, 0);
    assert!(!shown(&game), "released, nothing left to press");
    game.resolve_workers();
    assert_eq!(game.field_workers[0].job.map(|j| j.hex), Some(hex));
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
fn the_city_panel_lists_its_jobs_and_removes_them() {
    let (mut game, hex) = empty_tile_near_blue_city();
    open_city_zero(&mut game);
    place_road_and_fort(&mut game, hex);
    let before = game.stock(PLAYER_TEAM);
    let remove = Target::WorkerJobRemove(0);
    game.handle_click(button_cursor(&game, remove), SCREEN, ClickMode::Normal);
    let jobs = &game.cities[0].worker_jobs;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, JobKind::Fort);
    assert_eq!(game.stock(PLAYER_TEAM), before + JobKind::Road.price());
    // The panel builds workers too.
    game.queue_selected_city_worker();
    assert_eq!(
        game.cities[0].queue.last(),
        Some(&Queued::new(Build::Worker))
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
    open_city_zero(&mut game);
    place_road_and_fort(&mut game, hex);
    game.reorder_queue(QueueKind::Workers, 1, 0);
    let kinds: Vec<_> = game.cities[0].worker_jobs.iter().map(|j| j.kind).collect();
    assert_eq!(kinds, [JobKind::Fort, JobKind::Road]);
}

#[test]
fn clicking_a_worker_job_or_a_worker_shows_it_on_the_map() {
    let (mut game, hex) = empty_tile_near_blue_city();
    open_city_zero(&mut game);
    place_road_and_fort(&mut game, hex);
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
    game.resolve_workers();
    let worker = game.field_workers[0].clone();
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
    open_city_zero(&mut game);
    game.placing_job = Some(JobKind::Improve);
    assert!(game.place_job_at(hex, None));
    game.placing_job = None;
    let mut panel = PanelBuilder::default();
    game.city_tray(0, &mut panel);
    let labels: Vec<String> = flat_rows(&panel.rows)
        .into_iter()
        .filter_map(|row| match row {
            Row::QueueItem(item) => Some(item.label.clone()),
            _ => None,
        })
        .collect();
    let tile = game.grid.tile(hex).name();
    let build = if game.grid.tile(hex).hills {
        "MINE"
    } else {
        "FARM"
    };
    assert_eq!(labels, [format!("{build} · {tile} · \u{E003}3")]);
    // A job a worker left partway says how much of it is done.
    game.cities[0].worker_jobs[0].done = 2;
    let mut panel = PanelBuilder::default();
    game.city_tray(0, &mut panel);
    let row = flat_rows(&panel.rows)
        .into_iter()
        .find_map(|row| match row {
            Row::QueueItem(item) => Some(item.label.clone()),
            _ => None,
        });
    assert_eq!(row, Some(format!("{build} · {tile} · 2 OF \u{E003}3 DONE")));
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
    // The Cities scenario: Blue's city has nothing queued, and one unit of
    // each military kind. Its worker idle at home waits for nothing.
    let mut game = GameState::city_scenario();
    let city = game
        .cities
        .iter()
        .find(|c| c.team == PLAYER_TEAM)
        .unwrap()
        .id;
    let keys = roster_keys(&game);
    assert_eq!(keys[0], RosterKey::Production(city), "{keys:?}");
    let groups: Vec<UnitType> = keys[1..]
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
    assert_eq!(roster_keys(&game)[1], RosterKey::Group(unit_type, true));

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
    game.cities[open]
        .queue
        .push(Queued::new(Build::Unit(BuildUnit::Melee)));
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

/// The classic settings menu's buttons for `setting`, in order.
fn setting_buttons(game: &GameState, setting: Setting) -> Vec<(i32, ButtonState)> {
    game.layout(SCREEN)
        .buttons
        .iter()
        .filter_map(|b| match b.target {
            Target::SetSetting(s, to) if s == setting => Some((to, b.state)),
            _ => None,
        })
        .collect()
}

#[test]
fn the_settings_menu_opens_centered_over_the_panels_and_its_buttons_work() {
    let off = Target::SetSetting(Setting::TurnPlayback, 0);
    let on = Target::SetSetting(Setting::TurnPlayback, 1);
    // Nothing selected, a unit selected, and a city with its queue open.
    let mut plain = GameState::new();
    plain.clear_selection();
    let unit = GameState::new();
    let mut city = GameState::city_scenario();
    city.select_city();
    city.queue_selected_city_unit(BuildUnit::Melee);
    for mut game in [plain, unit, city] {
        assert!(!game.layout(SCREEN).buttons.iter().any(|b| b.target == on));
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
            assert!(
                menu_rect.0.y >= 0.0 && menu_rect.1.y <= screen.y,
                "the menu fits on a {screen} screen: {menu_rect:?}"
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
            // Every setting's buttons, Close and Quit are inside the menu,
            // and a panel underneath never takes their clicks.
            let menu_buttons: Vec<_> = layout
                .buttons
                .iter()
                .filter(|b| {
                    matches!(
                        b.target,
                        Target::SetSetting(..)
                            | Target::ToggleYields
                            | Target::CloseSettings
                            | Target::Quit
                            | Target::OpenMultiplayer
                    )
                })
                .collect();
            for setting in Setting::ALL {
                assert!(
                    menu_buttons
                        .iter()
                        .any(|b| matches!(b.target, Target::SetSetting(s, _) if s == setting)),
                    "{setting:?} has buttons"
                );
            }
            for target in [Target::OpenMultiplayer, Target::CloseSettings, Target::Quit] {
                assert!(menu_buttons.iter().any(|b| b.target == target));
            }
            // The menu is drawn as a layer over everything else: its panel
            // first, then only its own buttons.
            let (shapes_at, buttons_at) = layout.overlay.expect("the menu is an overlay");
            assert!(matches!(
                layout.shapes[shapes_at],
                Shape::Panel { min, max, .. } if (min, max) == menu_rect
            ));
            assert_eq!(layout.buttons.len() - buttons_at, menu_buttons.len());
            for button in menu_buttons {
                assert!(contains(menu_rect.0, menu_rect.1, button.min));
                assert!(contains(menu_rect.0, menu_rect.1, button.max));
                let middle = (button.min + button.max) / 2.0;
                assert_eq!(layout.button_at(middle).unwrap().target, button.target);
            }
        }

        // Instant playback is on by default: ON is gold, OFF switches it
        // off and turns gold, and clicking it again changes nothing.
        assert!(game.settings.instant_playback);
        let playback = |game: &GameState| setting_buttons(game, Setting::TurnPlayback);
        assert_eq!(
            playback(&game),
            [(0, ButtonState::Ready), (1, ButtonState::Queued)]
        );
        game.handle_click(button_cursor(&game, off), SCREEN, ClickMode::Normal);
        assert!(!game.settings.instant_playback);
        assert_eq!(
            playback(&game),
            [(0, ButtonState::Queued), (1, ButtonState::Ready)]
        );
        game.handle_click(button_cursor(&game, off), SCREEN, ClickMode::Normal);
        assert!(!game.settings.instant_playback);
        game.handle_click(button_cursor(&game, on), SCREEN, ClickMode::Normal);
        assert!(game.settings.instant_playback);

        let close = button_cursor(&game, Target::CloseSettings);
        game.handle_click(close, SCREEN, ClickMode::Normal);
        assert!(!game.settings_open);
        assert!(!game.quit_requested());
    }
}

#[test]
fn the_classic_settings_menu_has_a_button_per_choice_and_steps_the_rest() {
    let mut game = GameState::new();
    game.settings_open = true;
    for setting in Setting::ALL {
        let value = game.settings.get(setting);
        let range = setting.range();
        let buttons = setting_buttons(&game, setting);
        if settings_menu::steps_in_classic(setting) {
            // < and > set the values either side, faded at an end.
            let at = |end: i32| ButtonState::new(false, value == end);
            assert_eq!(
                buttons,
                [
                    (value - 1, at(*range.start())),
                    (value + 1, at(*range.end()))
                ],
                "{setting:?}"
            );
        } else {
            // A button per value, the current one gold.
            let expected: Vec<_> = range
                .map(|to| (to, ButtonState::new(to == value, false)))
                .collect();
            assert_eq!(buttons, expected, "{setting:?}");
        }
    }

    // The queue limit steps with < and >, and stops at the top.
    let limit = Setting::MaxQueuedTurns;
    let up = |game: &GameState| Target::SetSetting(limit, game.settings.get(limit) + 1);
    game.handle_click(button_cursor(&game, up(&game)), SCREEN, ClickMode::Normal);
    assert_eq!(game.settings.max_queued_turns, 7);
    game.set_setting(limit, 20);
    assert_eq!(
        setting_buttons(&game, limit)[1],
        (21, ButtonState::Disabled),
        "spent at the top"
    );
    game.handle_click(button_cursor(&game, up(&game)), SCREEN, ClickMode::Normal);
    assert_eq!(game.settings.max_queued_turns, 20);

    // Fog picks its value by name.
    let grey = Target::SetSetting(Setting::FogStyle, 0);
    game.handle_click(button_cursor(&game, grey), SCREEN, ClickMode::Normal);
    assert!(!game.settings.cloud_fog);
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
fn the_settings_menu_shows_every_setting_under_its_heading() {
    let game = GameState::new();
    let panel = game.settings_panel_content();
    // Each heading once, then its settings at their current values, in
    // `Setting::ALL`'s order.
    let mut heading = None;
    let mut listed = Vec::new();
    for row in &panel.rows {
        match row {
            Row::Heading(text) => heading = Some(text.clone()),
            Row::Setting(setting, value) => {
                assert_eq!(heading.as_deref(), Some(setting.group()), "{setting:?}");
                assert_eq!(*value, game.settings.get(*setting));
                listed.push(*setting);
            }
            _ => {}
        }
    }
    assert_eq!(listed, Setting::ALL);

    // Classic shows the headings and names as text, and the value of
    // anything it steps.
    let text = panel_strings(|p| p.rows = classic_rows(panel.rows.clone()));
    for setting in Setting::ALL {
        assert_shows(&text, setting.group());
        assert_shows(&text, setting.name());
        if settings_menu::steps_in_classic(setting) {
            assert_shows(&text, &setting.value_text(game.settings.get(setting)));
        }
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
    let current = tooltip(Target::SetSetting(Setting::TurnPlayback, 1));
    assert_shows(&current, "INSTANT PLAYBACK");
    assert_shows(&current, "ALREADY ON");
    let other = tooltip(Target::SetSetting(Setting::TurnPlayback, 0));
    assert_shows(&other, "OFF");
    assert!(!other.iter().any(|line| line.contains("ALREADY")));
    let past_the_end = tooltip(Target::SetSetting(Setting::MaxQueuedTurns, 0));
    assert_shows(&past_the_end, "ALREADY 6 TURNS");
}

#[test]
fn the_barracks_panel_shows_each_deposits_cap_and_why_a_troop_is_locked() {
    let mut game = city_view();
    game.units.clear();
    game.cities[0].barracks = Some(Hex::new(-2, 0));
    game.cities[0].built.push(Building::Barracks);
    game.open_barracks(0);
    let tray = panel_strings(|panel| game.barracks_tray(0, panel));
    assert_shows(&tray, "CAVALRY: 3 OF 3 LEFT (1 HORSES DEPOSIT × 3)");
    assert_shows(&tray, "ARMORED: LOCKED - PUT A BARRACKS ON IRON");
    let cavalry = find_button(&game, Target::BarracksBuild(BuildUnit::Cavalry));
    assert_eq!(cavalry.state, ButtonState::Ready);
    let armored = find_button(&game, Target::BarracksBuild(BuildUnit::Armored));
    assert_eq!(armored.state, ButtonState::Disabled);
    let tooltip: String = game
        .tooltip_lines(&armored)
        .into_iter()
        .flat_map(|(_, line)| line.into_iter().map(|(text, _)| text))
        .collect();
    assert!(
        tooltip.contains("NEEDS IRON UNDER THE BARRACKS"),
        "{tooltip}"
    );
    // A Barracks trains at its own pace: a Melee's card shows its 2 turns.
    let melee = find_button(&game, Target::BarracksBuild(BuildUnit::Melee));
    assert!(
        melee.hint.ends_with(&format!("{TIME_ICON}2")),
        "{}",
        melee.hint
    );
}

#[test]
fn the_multiplayer_page_hosts_and_joins_from_the_menu() {
    use super::network_menu::NetField;
    let mut game = GameState::new();
    game.clear_selection();
    game.press_escape();
    let click = |game: &mut GameState, target| {
        game.handle_click(button_cursor(game, target), SCREEN, ClickMode::Normal)
    };
    click(&mut game, Target::OpenMultiplayer);
    // The page takes the settings' place, fits the screen, and every button
    // on it takes its own clicks.
    let targets = [
        Target::NetPlayers(3),
        Target::EditNetField(NetField::Port),
        Target::HostGame,
        Target::EditNetField(NetField::Address),
        Target::EditNetField(NetField::Code),
        Target::JoinGame,
        Target::CloseMultiplayer,
        Target::CloseSettings,
    ];
    for screen in [SCREEN, Vec2::new(1280.0, 720.0)] {
        let layout = game.layout(screen);
        let &(min, max) = layout.panels.last().unwrap();
        assert!(min.cmpge(Vec2::ZERO).all() && max.cmple(screen).all());
        assert!(
            !layout
                .buttons
                .iter()
                .any(|b| matches!(b.target, Target::SetSetting(..)))
        );
        for target in targets {
            let button = layout.buttons.iter().find(|b| b.target == target);
            let button = button.unwrap_or_else(|| panic!("{target:?} shown"));
            assert!(contains(min, max, button.min) && contains(min, max, button.max));
            let middle = (button.min + button.max) / 2.0;
            assert_eq!(layout.button_at(middle).unwrap().target, target);
        }
    }

    // Two to seven players.
    click(&mut game, Target::NetPlayers(3));
    assert_eq!(game.net_menu.players, 3);
    game.activate_target(Target::NetPlayers(99));
    assert_eq!(game.net_menu.players, crate::game::MAX_PLAYERS);
    game.activate_target(Target::NetPlayers(3));

    // Joining wants an address and a code, typed in: a click on a field
    // starts typing, and any other button ends it.
    click(&mut game, Target::JoinGame);
    assert_eq!(game.take_net_request(), None);
    assert!(!game.net_menu.status.is_empty());
    click(&mut game, Target::EditNetField(NetField::Address));
    assert_eq!(game.net_field_editing(), Some(NetField::Address));
    game.type_net_text("192.168.1.20:55741x");
    game.net_field_backspace();
    click(&mut game, Target::EditNetField(NetField::Code));
    assert_eq!(game.net_field_editing(), Some(NetField::Code));
    game.type_net_text("k7m 2qx");
    click(&mut game, Target::JoinGame);
    assert_eq!(game.net_field_editing(), None);
    assert_eq!(
        game.take_net_request(),
        Some(NetRequest::Join {
            address: "192.168.1.20:55741".into(),
            code: "K7M2QX".into(),
        })
    );

    // Hosting wants a port that is one.
    game.set_net_field(NetField::Port, "");
    click(&mut game, Target::EditNetField(NetField::Port));
    game.type_net_text("port 0");
    assert_eq!(game.net_menu.port, "0");
    click(&mut game, Target::HostGame);
    assert_eq!(game.take_net_request(), None);
    game.set_net_field(NetField::Port, "55741");
    click(&mut game, Target::HostGame);
    assert_eq!(
        game.take_net_request(),
        Some(NetRequest::Host {
            port: 55741,
            players: 3
        })
    );

    // What's typed stays through a new game; the menu opens on the
    // settings again next time.
    game.switch_scenario(Scenario::Cities);
    assert_eq!(game.net_menu.address, "192.168.1.20:55741");
    click(&mut game, Target::CloseMultiplayer);
    assert!(game.settings_open && !game.net_menu.open);
    click(&mut game, Target::OpenMultiplayer);
    game.press_escape();
    assert!(!game.settings_open && !game.net_menu.open);
}

#[test]
fn the_multiplayer_page_in_a_network_game_shows_the_code_and_leaves() {
    let mut host = GameState::host_game(3, &crate::game::Settings::default());
    host.settings_open = true;
    host.activate_target(Target::OpenMultiplayer);
    let panel = host.settings_panel_content();
    let text: String = panel
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Text(_, line) => Some(line.iter().map(|(s, _)| s.as_str()).collect::<String>()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n");
    let code = host.join_code().unwrap();
    assert!(
        text.contains(&format!("HOSTING AS BLUE - JOIN CODE {code}")),
        "{text}"
    );
    assert!(text.contains("SEATS OPEN: RED, GREEN"), "{text}");
    assert!(
        !host
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::HostGame)
    );
    host.handle_click(
        button_cursor(&host, Target::LeaveGame),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(host.take_net_request(), Some(NetRequest::Leave));
}

/// The ImGui presentation without a window: a context with a font, and the
/// layout it keeps from frame to frame, as `App` has them.
struct ImGuiScreen {
    context: ::imgui::Context,
    fonts: [::imgui::FontId; 3],
    layout: ImGuiLayoutState,
    /// The window's size.
    size: Vec2,
    /// Last, to drop after the context.
    _one: std::sync::MutexGuard<'static, ()>,
}

impl ImGuiScreen {
    fn new() -> Self {
        Self::with_size(SCREEN)
    }

    /// The same, in a window of `size`.
    fn with_size(size: Vec2) -> Self {
        let one = imgui::one_context_at_a_time();
        let mut context = ::imgui::Context::create();
        context.set_ini_filename(None);
        context.io_mut().display_size = size.to_array();
        context
            .io_mut()
            .config_flags
            .insert(::imgui::ConfigFlags::DOCKING_ENABLE);
        let font = context
            .fonts()
            .add_font(&[::imgui::FontSource::DefaultFontData { config: None }]);
        context.fonts().build_rgba32_texture();
        Self {
            context,
            fonts: [font; 3],
            layout: ImGuiLayoutState::default(),
            size,
            _one: one,
        }
    }

    /// Draws a frame with the mouse at `mouse` (window pixels; `None`: off
    /// the window) and its left button `down`, carrying out what the panels
    /// were asked to do, as `App` does.
    fn frame(&mut self, game: &mut GameState, mouse: Option<Vec2>, down: bool) {
        let io = self.context.io_mut();
        io.delta_time = 1.0 / 60.0;
        io.add_mouse_pos_event(mouse.map_or([f32::MIN; 2], |m| m.to_array()));
        io.add_mouse_button_event(::imgui::MouseButton::Left, down);
        imgui::DRAWN_BUTTONS.with_borrow_mut(Vec::clear);
        let ui = self.context.frame();
        game.draw_imgui(ui, self.size, mouse, &self.fonts, &mut self.layout);
        self.context.render();
    }

    /// A few frames with the mouse away, for the panels to settle where
    /// the dock puts them.
    fn settle(&mut self, game: &mut GameState) {
        for _ in 0..4 {
            self.frame(game, None, false);
        }
    }

    /// The middle of the button ImGui drew for `target` last frame.
    fn button(&self, target: Target) -> Option<Vec2> {
        imgui::DRAWN_BUTTONS.with_borrow(|drawn| {
            drawn
                .iter()
                .find(|(drawn, ..)| *drawn == target)
                .map(|&(_, min, max)| (Vec2::from(min) + Vec2::from(max)) / 2.0)
        })
    }

    /// Moves the mouse onto `target`'s button and clicks it. Panics if
    /// ImGui doesn't show that button.
    fn click(&mut self, game: &mut GameState, target: Target) {
        self.settle(game);
        let at = self
            .button(target)
            .unwrap_or_else(|| panic!("ImGui shows no {target:?} button"));
        self.frame(game, Some(at), false);
        self.frame(game, Some(at), true);
        self.frame(game, Some(at), false);
    }

    /// Whether ImGui keeps a mouse press at `at` from the map (`App` then
    /// doesn't pass it on).
    fn captures_mouse_at(&mut self, game: &mut GameState, at: Vec2) -> bool {
        self.frame(game, Some(at), false);
        self.frame(game, Some(at), false);
        self.context.io().want_capture_mouse
    }
}

/// A map tile near the open city where a click lands on the map, not a
/// panel: `covered` says whether a panel is at a point.
fn open_map_tile(game: &GameState, mut covered: impl FnMut(Vec2) -> bool) -> Hex {
    let city = game.cities[game.selected_city.expect("city view open")].pos;
    let mut tiles: Vec<Hex> = game
        .grid
        .all_hexes()
        .filter(|h| (2..=4).contains(&h.distance(city)))
        .collect();
    tiles.sort_by_key(|h| (h.distance(city), h.q, h.r));
    tiles
        .into_iter()
        .find(|&h| {
            let at = hex_cursor(game, h);
            (0.0..SCREEN.x).contains(&at.x) && (0.0..SCREEN.y).contains(&at.y) && !covered(at)
        })
        .expect("an open tile on screen")
}

/// How many jobs the open city has placed for its workers, and what its
/// side has in its stockpile: what stopping placing mustn't change.
fn placed_and_paid(game: &GameState) -> (usize, Stock) {
    let city = game.selected_city.expect("city view open");
    (
        game.cities[city].worker_jobs.len(),
        game.stock(game.cities[city].team),
    )
}

#[test]
fn placing_shows_a_cancel_button_that_stops_it_with_nothing_placed() {
    let mut game = city_view();
    let before = placed_and_paid(&game);
    let card = catalog_cursor(&mut game, Target::Building(Building::Barracks));
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_job, Some(JobKind::Build(Building::Barracks)));
    // The picked card is gold, and the tray says how to stop.
    let layout = game.layout(SCREEN);
    let armed = layout
        .buttons
        .iter()
        .find(|b| b.target == Target::Building(Building::Barracks))
        .expect("the picked card stays in view");
    assert_eq!(armed.state, ButtonState::Queued);
    let cancel = layout
        .buttons
        .iter()
        .find(|b| b.target == Target::CancelPlacing)
        .expect("a cancel button while placing");
    assert_eq!(cancel.label, "CANCEL PLACING BARRACKS");
    assert_eq!(
        layout
            .button_at((cancel.min + cancel.max) / 2.0)
            .map(|b| b.target),
        Some(Target::CancelPlacing)
    );
    let tray = layout
        .panels
        .iter()
        .find(|panel| {
            contains(panel.0, panel.1, cancel.min) && contains(panel.0, panel.1, cancel.max)
        })
        .expect("inside the city tray");
    assert!(
        contains(tray.0, tray.1, armed.min),
        "the same panel as the card"
    );
    let text = panel_strings(|p| game.city_tray(0, p));
    assert_shows(
        &text,
        "PLACING BARRACKS: CLICK A LIT TILE · RIGHT-CLICK OR ESC TO CANCEL",
    );
    // Map clicks place rather than assign, so that hint makes way.
    assert!(
        !text
            .iter()
            .any(|line| line.contains("CLICK MANAGER TO MOVE"))
    );
    assert!(
        game.notice.contains("RIGHT-CLICK OR ESC TO CANCEL"),
        "{}",
        game.notice
    );
    assert!(!game.notice.contains("BARRACKSS"), "{}", game.notice);
    let tooltip = line_strings(game.tooltip_lines(cancel).into_iter().map(|(_, l)| l)).join(" ");
    assert!(tooltip.contains("NOTHING IS PLACED OR PAID"), "{tooltip}");

    game.handle_click(
        button_cursor(&game, Target::CancelPlacing),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(game.placing_job, None);
    assert_eq!(game.selected_city, Some(0), "the city stays open");
    assert_eq!(placed_and_paid(&game), before);
    assert_eq!(game.notice, "STOPPED PLACING BARRACKS");
    assert!(
        !game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::CancelPlacing),
        "gone once nothing is being placed"
    );
}

#[test]
fn imgui_queues_a_build_the_side_cannot_pay_for_and_shows_it_waiting() {
    let mut game = city_view();
    game.stockpiles[Team::Blue.index()] = Stock::default();
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, Target::Build(BuildUnit::Siege));
    let city = game.selected_city.unwrap();
    assert_eq!(
        game.cities[city].queue,
        [Queued::new(Build::Unit(BuildUnit::Siege))]
    );
    assert!(game.notice.contains("WAITS FOR"), "{}", game.notice);
    // The queue's row is drawn, from the shared content that marks it
    // waiting; the tray says what the city waits for.
    screen.settle(&mut game);
    assert!(
        screen
            .button(Target::QueueItem(QueueKind::City, 0))
            .is_some()
    );
    let mut queue = PanelBuilder::default();
    game.city_queue_panel(city, usize::MAX, &mut queue);
    let waiting: Vec<bool> = queue
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::QueueItem(item) => Some(item.waiting),
            _ => None,
        })
        .collect();
    assert_eq!(waiting, [true]);
    let mut tray = PanelBuilder::default();
    game.city_tray(city, &mut tray);
    let text = line_strings(tray.rows.iter().filter_map(|row| match row {
        Row::Text(_, line) => Some(line.clone()),
        _ => None,
    }))
    .join(" ");
    assert!(text.contains("SIEGE WAITS FOR"), "{text}");
    assert!(text.contains("NOTHING IT CAN PAY FOR"), "{text}");
}

#[test]
fn the_imgui_cancel_button_stops_placing() {
    let mut game = city_view();
    let before = placed_and_paid(&game);
    let mut screen = ImGuiScreen::new();
    for target in [
        Target::WorkerJob(JobKind::Road),
        Target::WorkerJob(JobKind::Wall),
        Target::Building(Building::Mill),
    ] {
        game.activate_target(target);
        assert!(game.placing_job.is_some(), "{target:?}");
        screen.settle(&mut game);
        let at = screen.button(Target::CancelPlacing).expect("shown");
        assert!(
            screen.captures_mouse_at(&mut game, at),
            "a right-click there is ImGui's, not the map's"
        );
        screen.click(&mut game, Target::CancelPlacing);
        assert_eq!(game.placing_job, None, "{target:?}");
        assert_eq!(game.selected_city, Some(0));
        assert_eq!(placed_and_paid(&game), before);
        screen.settle(&mut game);
        assert_eq!(screen.button(Target::CancelPlacing), None);
    }
}

#[test]
fn escape_or_a_right_click_on_the_map_stops_placing_in_both_presentations() {
    let mut game = city_view();
    let before = placed_and_paid(&game);
    let mut screen = ImGuiScreen::new();
    for imgui in [false, true] {
        for kind in [
            JobKind::Road,
            JobKind::Gate,
            JobKind::Build(Building::Barracks),
        ] {
            // Escape: `App` calls `press_escape` in either presentation
            // (unless a text box has the keys, and none is open here).
            game.arm_worker_job(kind);
            assert_eq!(game.placing_job, Some(kind));
            game.press_escape();
            assert_eq!(game.placing_job, None, "{kind:?}");
            assert_eq!(game.selected_city, Some(0), "Escape stops placing first");
            assert_eq!(game.notice, format!("STOPPED PLACING {}", kind.name()));

            // A right-click on the map. ImGui keeps a click over one of its
            // windows (`want_capture_mouse`); classic passes every
            // right-click to `handle_context_click`.
            game.arm_worker_job(kind);
            let tile = if imgui {
                screen.settle(&mut game);
                let mut looking = game.clone();
                let tile = open_map_tile(&game, |at| screen.captures_mouse_at(&mut looking, at));
                let at = hex_cursor(&game, tile);
                assert!(!screen.captures_mouse_at(&mut game, at));
                tile
            } else {
                open_map_tile(&game, |at| game.layout(SCREEN).covers(to_ui(at, SCREEN)))
            };
            assert!(game.notice.starts_with("PLACING"), "{}", game.notice);
            game.handle_context_click(hex_cursor(&game, tile), SCREEN, false, false);
            assert_eq!(game.placing_job, None, "{kind:?}, imgui {imgui}");
            assert_eq!(game.selected_city, Some(0));
            // The status bar says so, as after Escape, rather than still
            // telling the player how to cancel.
            assert_eq!(game.notice, format!("STOPPED PLACING {}", kind.name()));
            assert_eq!(placed_and_paid(&game), before, "{kind:?}");
        }
    }
    // Escape again closes the city.
    game.press_escape();
    assert_eq!(game.selected_city, None);
}

#[test]
fn escape_and_right_click_stop_placing_in_a_network_game() {
    let mut host = GameState::host_game(2, &crate::game::Settings::default());
    let (_, welcome) = host.welcome(&crate::game::NetMessage::Hello {
        version: crate::game::PROTOCOL_VERSION,
    });
    let guest = GameState::join_game(&welcome).expect("joins");
    for mut game in [host, guest] {
        let team = game.local_team;
        let city = game.cities.iter().position(|c| c.team == team).unwrap();
        game.open_city(city);
        game.update(10.0);
        game.cities[city].workers = game.cities[city].workers.max(1);
        game.fund(team);
        let plan = game.team_plan(team);
        game.arm_worker_job(JobKind::Road);
        assert_eq!(game.placing_job, Some(JobKind::Road), "{}", game.notice);
        game.press_escape();
        assert_eq!(game.placing_job, None);
        assert_eq!(game.selected_city, Some(city));
        game.arm_worker_job(JobKind::Road);
        let tile = open_map_tile(&game, |at| game.layout(SCREEN).covers(to_ui(at, SCREEN)));
        game.handle_context_click(hex_cursor(&game, tile), SCREEN, false, false);
        assert_eq!(game.placing_job, None);
        assert_eq!(game.team_plan(team), plan, "nothing planned");
    }
}

/// The classic layout's button for `target`, and the queue panel it's in:
/// the panel (from `layout.panels`) holding the queue's rows.
fn queue_panel_button(layout: &Layout, target: Target) -> (Rect, Rect) {
    let button = layout
        .buttons
        .iter()
        .find(|b| b.target == target)
        .unwrap_or_else(|| panic!("no {target:?} button"));
    let row = layout.queue_items.first().expect("queue rows");
    let &(min, max) = layout
        .panels
        .iter()
        .find(|&&(min, max)| contains(min, max, row.min) && contains(min, max, row.max))
        .expect("the queue panel");
    (
        Rect {
            min: button.min,
            max: button.max,
        },
        Rect { min, max },
    )
}

#[test]
fn each_queue_panel_has_a_clear_button_that_refunds_everything() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    for _ in 0..6 {
        game.queue_selected_city_unit(BuildUnit::Melee);
    }
    game.queue_selected_city_growth();
    game.queue_selected_city_growth();
    // As turns' economies would leave it: the head paid for and under way,
    // and a Grow paid for.
    game.cities[city].queue[0] = Queued::worked(Build::Unit(BuildUnit::Melee), 1);
    game.cities[city].queue[6].paid = true;
    let mut one_by_one = game.clone();
    while !one_by_one.cities[city].queue.is_empty() {
        one_by_one.remove_selected_city_queue_item(0);
    }

    // A long queue scrolls; its Clear button stays on the title's line,
    // clear of the rows, their Xs and the scrollbar, wherever it's scrolled.
    for scroll in [0, 3, 100] {
        game.city_queue_scroll = scroll;
        let layout = game.layout(SCREEN);
        let (clear, panel) = queue_panel_button(&layout, Target::ClearCityQueue);
        assert!(
            contains(panel.min, panel.max, clear.min) && contains(panel.min, panel.max, clear.max)
        );
        let bar = layout.queue_scrollbars.first().expect("the queue scrolls");
        assert!(!clear.overlaps(
            Rect {
                min: bar.track_min,
                max: bar.track_max
            },
            0.0
        ));
        for row in &layout.queue_items {
            assert!(!clear.overlaps(
                Rect {
                    min: row.min,
                    max: row.max
                },
                0.0
            ));
        }
        let middle = (clear.min + clear.max) / 2.0;
        assert_eq!(
            layout.button_at(middle).map(|b| b.target),
            Some(Target::ClearCityQueue)
        );
        assert!(clear.max.y > layout.queue_items[0].max.y, "above the rows");
    }
    // Hover says what it does.
    let tooltip = |game: &GameState, target| {
        let layout = game.layout(SCREEN);
        let button = layout.buttons.iter().find(|b| b.target == target).unwrap();
        line_strings(game.tooltip_lines(button).into_iter().map(|(_, l)| l)).join(" ")
    };
    let text = tooltip(&game, Target::ClearCityQueue);
    assert!(text.contains("EACH REFUNDED"), "{text}");

    game.handle_click(
        button_cursor(&game, Target::ClearCityQueue),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(game.cities[city].queue.is_empty());
    assert_eq!(game.stock(Team::Blue), one_by_one.stock(Team::Blue));
    assert_eq!(game.city_queue_scroll, 0);
    assert!(
        !game
            .layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::ClearCityQueue),
        "no queue, no panel"
    );

    // The Barracks queue, in its own view.
    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.open_barracks(city);
    for build in [BuildUnit::Melee, BuildUnit::Ranged, BuildUnit::Siege] {
        game.queue_selected_barracks_unit(build);
    }
    game.cities[city].barracks_queue[0].paid = true;
    let mut one_by_one = game.clone();
    while !one_by_one.cities[city].barracks_queue.is_empty() {
        one_by_one.remove_selected_barracks_queue_item(0);
    }
    let layout = game.layout(SCREEN);
    let (clear, panel) = queue_panel_button(&layout, Target::ClearBarracksQueue);
    assert!(contains(panel.min, panel.max, clear.min) && contains(panel.min, panel.max, clear.max));
    game.handle_click(
        button_cursor(&game, Target::ClearBarracksQueue),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(game.cities[city].barracks_queue.is_empty());
    assert_eq!(game.stock(Team::Blue), one_by_one.stock(Team::Blue));
}

#[test]
fn the_clear_button_is_dimmed_while_a_turn_plays_out() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.queue_selected_city_unit(BuildUnit::Melee);
    game.resolve_turn();
    assert!(game.is_resolving());
    let layout = game.layout(SCREEN);
    let clear = layout
        .buttons
        .iter()
        .find(|b| b.target == Target::ClearCityQueue)
        .expect("shown");
    assert_eq!(clear.state, ButtonState::Disabled);
    let why = line_strings(game.tooltip_lines(clear).into_iter().map(|(_, l)| l)).join(" ");
    assert!(why.contains("NOT WHILE THE TURN PLAYS OUT"), "{why}");
    game.activate_target(Target::ClearCityQueue);
    assert_eq!(game.cities[city].queue.len(), 1);
}

#[test]
fn the_imgui_clear_buttons_empty_their_queues() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    for _ in 0..4 {
        game.queue_selected_city_unit(BuildUnit::Ranged);
    }
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, Target::ClearCityQueue);
    assert!(game.cities[city].queue.is_empty());

    game.cities[city].barracks = Some(Hex::new(-2, 0));
    game.open_barracks(city);
    game.queue_selected_barracks_unit(BuildUnit::Melee);
    game.queue_selected_barracks_unit(BuildUnit::Melee);
    screen.click(&mut game, Target::ClearBarracksQueue);
    assert!(game.cities[city].barracks_queue.is_empty());
    let stock = game.stock(Team::Blue);
    assert_eq!(stock, Stock::whole(999, 999, 999), "all refunded");
}

/// A guest in a network game who has ended the first turn and waits for
/// the host's plan, with a Barracks by its city (something in its queue)
/// and something queued in the city: every kind of view has something to
/// show.
fn waiting_guest() -> GameState {
    use crate::game::{NetMessage, PROTOCOL_VERSION, Settings};
    let mut host = GameState::host_game(2, &Settings::default());
    let (_, welcome) = host.welcome(&NetMessage::Hello {
        version: PROTOCOL_VERSION,
    });
    let mut guest = GameState::join_game(&welcome).expect("joins");
    guest.update(0.0);
    let team = guest.local_team;
    let city = guest.cities.iter().position(|c| c.team == team).unwrap();
    let worked = guest.cities[city].worked.clone();
    let pos = guest.cities[city].pos;
    let site = pos
        .neighbors()
        .into_iter()
        .find(|&h| guest.grid.is_passable(h) && !guest.is_occupied(h) && !worked.contains(&h))
        .unwrap();
    guest.cities[city].barracks = Some(site);
    guest.cities[city].barracks_queue = vec![Queued::new(BuildUnit::Melee)];
    guest.fund(team);
    guest.open_city(city);
    guest.queue_selected_city_gather();
    guest.queue_selected_city_unit(BuildUnit::Ranged);
    guest.end_planning();
    assert!(guest.waiting_for_peers(), "{}", guest.notice);
    guest
}

/// Every view a waiting player can open: their city, its Barracks and
/// interior, and one of their units.
const WAITING_VIEWS: [&str; 4] = ["city", "barracks", "interior", "unit"];

/// Opens `view` (one of `WAITING_VIEWS`) on the player's first city or
/// unit, from nothing open.
fn open_view(game: &mut GameState, view: &str) {
    let team = game.local_team;
    let city = game.cities.iter().position(|c| c.team == team).unwrap();
    let unit = game.units.iter().position(|u| u.team == team).unwrap();
    game.leave_city_view();
    game.set_selection(Vec::new());
    match view {
        "city" => game.open_city(city),
        "barracks" => game.open_barracks(city),
        "interior" => game.open_city_interior(city),
        _ => game.set_selection(vec![unit]),
    }
}

#[test]
fn waiting_for_the_others_the_classic_panels_show_but_change_nothing() {
    let mut game = waiting_guest();
    let team = game.local_team;
    let plan = game.team_plan(team);
    // The turn still being planned is the one named, not the last.
    let top = game.layout(SCREEN);
    let turn = top
        .shapes
        .iter()
        .any(|s| matches!(s, Shape::Text { line, .. } if line.iter().any(|(t, _)| t == "TURN 1")));
    assert!(turn, "the turn being planned");
    for view in WAITING_VIEWS {
        open_view(&mut game, view);
        let layout = game.layout(SCREEN);
        let changing: Vec<&Button> = layout
            .buttons
            .iter()
            .filter(|b| b.target.changes_plan())
            .collect();
        assert!(!changing.is_empty(), "{view}: something to refuse");
        for button in &changing {
            let target = button.target;
            assert_eq!(button.state, ButtonState::Disabled, "{view}: {target:?}");
            let why = line_strings(game.tooltip_lines(button).into_iter().map(|(_, l)| l));
            assert!(
                why.join(" ").contains(tooltips::PLAN_SENT),
                "{view}: {target:?} says why: {why:?}"
            );
        }
        // Looking stays open.
        let looking = |t: Target| {
            matches!(
                t,
                Target::OpenInterior
                    | Target::OpenBarracks
                    | Target::OpenCity
                    | Target::ToggleYields
            )
        };
        for button in layout.buttons.iter().filter(|b| looking(b.target)) {
            let target = button.target;
            assert_ne!(button.state, ButtonState::Disabled, "{view}: {target:?}");
        }
        // Clicking every button, or dragging a queue row, changes nothing.
        let targets: Vec<Target> = changing.iter().map(|b| b.target).collect();
        for target in targets {
            let at = button_cursor(&game, target);
            game.handle_click(at, SCREEN, ClickMode::Normal);
            assert_eq!(game.team_plan(team), plan, "{view}: {target:?}");
        }
        if let Some(row) = layout.queue_items.first() {
            let body = (row.min + Vec2::new(row.body_max_x, row.max.y)) / 2.0;
            let at = to_ui(body, SCREEN);
            assert!(!game.start_queue_drag_at(at, SCREEN), "{view}: no dragging");
        }
    }
    assert!(game.waiting_for_peers());
}

#[test]
fn waiting_for_the_others_the_imgui_panels_show_but_change_nothing() {
    let mut game = waiting_guest();
    let team = game.local_team;
    let plan = game.team_plan(team);
    let mut screen = ImGuiScreen::new();
    for view in WAITING_VIEWS {
        open_view(&mut game, view);
        screen.settle(&mut game);
        let drawn: Vec<Target> = imgui::DRAWN_BUTTONS.with_borrow(|drawn| {
            drawn
                .iter()
                .map(|&(target, ..)| target)
                .filter(|t| t.changes_plan())
                .collect()
        });
        assert!(!drawn.is_empty(), "{view}: something to refuse");
        for target in drawn {
            // Some of a long list scroll out of sight as others are tried.
            screen.settle(&mut game);
            if screen.button(target).is_none() {
                continue;
            }
            screen.click(&mut game, target);
            assert_eq!(game.team_plan(team), plan, "{view}: {target:?}");
        }
        // Looking stays open: the city's interior button still works. (A
        // click on a card half scrolled out of the catalog can land on the
        // button under it, which may have opened the Barracks.)
        if view == "city" {
            open_view(&mut game, view);
            screen.click(&mut game, Target::OpenInterior);
            assert!(game.interior_view.is_some());
        }
    }
    assert!(game.waiting_for_peers());
}

#[test]
fn the_waiting_button_takes_the_turn_back_in_both_presentations() {
    let mut game = waiting_guest();
    let _sent = game.take_outbox();
    let mut screen = ImGuiScreen::new();
    for imgui in [false, true] {
        // Classic: it names the wait, and offers to take the turn back.
        let button = find_button(&game, Target::EndTurn);
        assert_eq!(button.label, "WAITING FOR THE OTHERS");
        assert_eq!(button.hint, "TAKE BACK");
        assert_eq!(button.state, ButtonState::Ready);
        let tip = line_strings(game.tooltip_lines(&button).into_iter().map(|(_, l)| l));
        assert!(tip[0].starts_with("TAKE BACK END TURN"), "{tip:?}");
        if imgui {
            screen.click(&mut game, Target::EndTurn);
        } else {
            game.handle_click(
                button_cursor(&game, Target::EndTurn),
                SCREEN,
                ClickMode::Normal,
            );
        }
        assert!(!game.waiting_for_peers(), "{}", game.notice);
        assert!(matches!(
            &game.take_outbox()[..],
            [crate::game::NetMessage::Withdraw { turn: 1 }]
        ));
        // The orders can change again: a Melee, and the turn ended again.
        let team = game.local_team;
        let city = game.cities.iter().position(|c| c.team == team).unwrap();
        game.open_city(city);
        let melee = find_button(&game, Target::Build(BuildUnit::Melee));
        assert_ne!(melee.state, ButtonState::Disabled);
        game.activate_target(Target::Build(BuildUnit::Melee));
        game.activate_target(Target::EndTurn);
        assert!(game.waiting_for_peers(), "{}", game.notice);
        assert!(matches!(
            &game.take_outbox()[..],
            [crate::game::NetMessage::Plan(plan)]
                if plan.cities[0].queue.iter().any(|q| q.build == Build::Unit(BuildUnit::Melee))
        ));
    }
}

/// City 0 open with `jobs` roads placed and waiting and `out` workers out
/// on others, its side rich: a tray with a long list of workers and jobs.
fn crowded_city(jobs: usize, out: u32) -> GameState {
    let (mut game, _) = empty_tile_near_blue_city();
    game.fund(Team::Blue);
    open_city_zero(&mut game);
    game.cities[0].workers = out;
    let city = game.cities[0].pos;
    let mut tiles: Vec<Hex> = game
        .grid
        .all_hexes()
        .filter(|h| (1..=4).contains(&h.distance(city)))
        .collect();
    tiles.sort_by_key(|h| (h.distance(city), h.q, h.r));
    game.placing_job = Some(JobKind::Road);
    for hex in tiles {
        if game.cities[0].worker_jobs.len() == out as usize + jobs {
            break;
        }
        if game.job_unavailable(hex, JobKind::Road).is_none() {
            assert!(game.place_job_at(hex, None));
        }
    }
    game.placing_job = None;
    // The workers take the first jobs; the rest wait.
    game.resolve_workers();
    assert_eq!(game.field_workers.len(), out as usize);
    assert_eq!(game.cities[0].worker_jobs.len(), jobs);
    game
}

/// The classic layout's panel holding the button for `target`.
fn panel_with(layout: &Layout, target: Target) -> Option<Rect> {
    let button = layout.buttons.iter().find(|b| b.target == target)?;
    layout
        .panels
        .iter()
        .find(|&&(min, max)| contains(min, max, button.min) && contains(min, max, button.max))
        .map(|&(min, max)| Rect { min, max })
}

/// Whether `target`'s button is in the panel `tray` and a click on its
/// middle lands on it.
fn clickable_in(layout: &Layout, tray: Rect, target: Target) -> bool {
    panel_with(layout, target).is_some_and(|panel| panel.min == tray.min)
        && layout
            .buttons
            .iter()
            .find(|b| b.target == target)
            .is_some_and(|b| {
                layout.button_at((b.min + b.max) / 2.0).map(|b| b.target) == Some(target)
            })
}

/// The worker jobs whose rows the classic layout shows, by index.
fn job_rows(layout: &Layout) -> Vec<usize> {
    layout
        .queue_items
        .iter()
        .filter(|item| item.kind == QueueKind::Workers)
        .map(|item| item.index)
        .collect()
}

/// The classic layout's scroll region for the open city's workers and jobs.
fn worker_list(layout: &Layout) -> Option<&QueueScrollRegion> {
    layout
        .queue_scrollbars
        .iter()
        .find(|s| s.kind == QueueKind::Workers)
}

/// `button_cursor` on a window of size `screen`.
fn button_cursor_at(game: &GameState, target: Target, screen: Vec2) -> Vec2 {
    let layout = game.layout(screen);
    let button = layout
        .buttons
        .iter()
        .find(|b| b.target == target)
        .expect("button shown");
    to_ui((button.min + button.max) / 2.0, screen)
}

#[test]
fn a_crowded_city_tray_stays_docked_with_its_list_scrolling_inside_it() {
    for screen in [SCREEN, Vec2::new(1280.0, 720.0)] {
        for placing in [false, true] {
            let mut game = crowded_city(10, 4);
            if placing {
                game.arm_worker_job(JobKind::Road);
                assert_eq!(game.placing_job, Some(JobKind::Road));
            }
            let at = format!("{screen}, placing {placing}");
            let layout = game.layout(screen);
            let tray = panel_with(&layout, Target::OpenInterior)
                .unwrap_or_else(|| panic!("the city tray docks at {at}"));
            assert!(tray.min.cmpge(Vec2::splat(MARGIN)).all(), "{at}");
            assert!(tray.max.y <= screen.y - TOP_BAR_HEIGHT, "{at}");
            for &(min, max) in &layout.panels {
                let other = Rect { min, max };
                assert!(
                    min == tray.min || !tray.overlaps(other, 0.0),
                    "{at}: {other:?} over the tray"
                );
            }
            // Its buttons, catalogue included, are inside it and clickable.
            for target in [
                Target::Build(BuildUnit::Melee),
                Target::Focus(LaborFocus::Balanced),
                Target::OpenInterior,
            ] {
                assert!(clickable_in(&layout, tray, target), "{target:?} at {at}");
            }
            assert_eq!(
                clickable_in(&layout, tray, Target::CancelPlacing),
                placing,
                "{at}"
            );
            let catalog = layout.building_scrollbars.first().expect("the catalogue");
            assert!(contains(tray.min, tray.max, catalog.min), "{at}");
            assert!(contains(tray.min, tray.max, catalog.max), "{at}");

            // The workers and jobs scroll in a window of their own, with
            // rows that click and drag as the full list's do.
            let list = worker_list(&layout).unwrap_or_else(|| panic!("a scrollbar at {at}"));
            assert!(contains(tray.min, tray.max, list.panel_min), "{at}");
            assert!(contains(tray.min, tray.max, list.panel_max), "{at}");
            let worker = game.field_workers[0].id;
            assert!(
                clickable_in(&layout, tray, Target::RecallWorker(worker)),
                "{at}"
            );
            let shown = job_rows(&layout);
            assert!(shown.len() < 10, "{at}: {shown:?}");

            // Scrolled to the end: the last job, and its X removes it.
            let wheel = to_ui((list.panel_min + list.panel_max) / 2.0, screen);
            assert!(game.scroll_queue_at(wheel, screen, -100.0));
            let layout = game.layout(screen);
            assert_eq!(job_rows(&layout).last(), Some(&9), "{at}");
            assert!(
                !layout
                    .buttons
                    .iter()
                    .any(|b| b.target == Target::RecallWorker(worker))
            );
            assert!(
                clickable_in(&layout, tray, Target::WorkerJobRemove(9)),
                "{at}"
            );
            let row = layout
                .queue_items
                .iter()
                .find(|item| item.kind == QueueKind::Workers && item.index == 9)
                .unwrap();
            assert!(contains(list.panel_min, list.panel_max, row.min), "{at}");
            assert!(row.max.x < list.track_min.x, "{at}: clear of the scrollbar");
            let remove = button_cursor_at(&game, Target::WorkerJobRemove(9), screen);
            game.handle_click(remove, screen, ClickMode::Normal);
            assert_eq!(game.cities[0].worker_jobs.len(), 9, "{at}");

            // Its scrollbar drags back to the top.
            let layout = game.layout(screen);
            let list = worker_list(&layout).unwrap();
            let top = to_ui(Vec2::new(list.track_min.x + 1.0, list.track_max.y), screen);
            assert!(game.drag_queue_scrollbar_at(top, screen, false));
            assert_eq!(game.cities[0].worker_scroll, 0, "{at}");
            let layout = game.layout(screen);
            assert!(
                clickable_in(&layout, tray, Target::RecallWorker(worker)),
                "{at}"
            );
        }
    }
}

#[test]
fn a_tray_with_room_shows_its_whole_list_and_catalogue() {
    let game = crowded_city(2, 1);
    let layout = game.layout(SCREEN);
    assert_eq!(job_rows(&layout), [0, 1]);
    assert!(worker_list(&layout).is_none(), "nothing to scroll");
    let catalog = layout.building_scrollbars.first().unwrap();
    let cards = layout
        .buttons
        .iter()
        .filter(|b| contains(catalog.min, catalog.max, b.min))
        .count();
    // Its heading, then cards.
    assert_eq!(cards, BUILDING_LIST_VISIBLE - 1);
}

#[test]
fn the_wheel_steps_back_from_the_end_of_a_list_left_scrolled_past_it() {
    let mut game = crowded_city(10, 4);
    // As if rows were taken off after scrolling to the end.
    game.cities[0].worker_scroll = 100;
    let layout = game.layout(SCREEN);
    let list = worker_list(&layout).unwrap();
    let last = list.max_offset;
    assert_eq!(job_rows(&layout).last(), Some(&9));
    let wheel = to_ui((list.panel_min + list.panel_max) / 2.0, SCREEN);
    assert!(game.scroll_queue_at(wheel, SCREEN, 1.0));
    assert_eq!(game.cities[0].worker_scroll, last - 1);
}

#[test]
fn a_crowded_imgui_city_panel_scrolls_to_every_row() {
    let on_screen = |at: Vec2, size: Vec2| at.cmpge(Vec2::ZERO).all() && at.cmple(size).all();
    for size in [SCREEN, Vec2::new(1280.0, 720.0)] {
        let mut game = crowded_city(10, 4);
        game.arm_worker_job(JobKind::Road);
        let mut screen = ImGuiScreen::with_size(size);
        screen.settle(&mut game);
        // The top of the panel shows: how to stop placing, and the cards.
        for target in [Target::CancelPlacing, Target::Build(BuildUnit::Melee)] {
            let at = screen.button(target).expect("drawn");
            assert!(on_screen(at, size), "{target:?} at {at} in {size}");
        }
        // Its window scrolls (the wheel over it, clear of the catalogue)
        // down to the last job, whose X removes it.
        let over = screen.button(Target::OpenInterior).unwrap();
        for _ in 0..5 {
            screen.context.io_mut().add_mouse_wheel_event([0.0, -5.0]);
            screen.frame(&mut game, Some(over), false);
        }
        screen.settle(&mut game);
        let last = screen.button(Target::WorkerJobRemove(9)).unwrap();
        assert!(on_screen(last, size), "{last} in {size}");
        screen.click(&mut game, Target::WorkerJobRemove(9));
        assert_eq!(game.cities[0].worker_jobs.len(), 9, "{size}");
    }
}

#[test]
fn waiting_for_the_others_the_city_workers_list_changes_nothing() {
    use crate::game::workers::{FieldWorker, WorkerJob};
    let mut game = waiting_guest();
    let team = game.local_team;
    let city = game.cities.iter().position(|c| c.team == team).unwrap();
    let pos = game.cities[city].pos;
    // A worker held at home, one out on a road, and a road waiting: the
    // city tray's scrolling list of workers and jobs has every kind of row.
    let [out, waiting] = [pos.neighbors()[0], pos.neighbors()[3]];
    game.cities[city].held_workers = 1;
    game.cities[city].workers = game.cities[city].workers.max(1);
    game.field_workers.push(FieldWorker {
        id: 9_000,
        team,
        home: city,
        base: pos,
        pos: out,
        job: Some(WorkerJob::on_tile(out, JobKind::Road)),
        work_left: Some(2),
        recalled: false,
    });
    game.cities[city]
        .worker_jobs
        .push(WorkerJob::on_tile(waiting, JobKind::Road));
    let plan = game.team_plan(team);
    game.open_city(city);
    let changing = [
        Target::ReleaseWorker,
        Target::RecallWorker(9_000),
        Target::WorkerJobRemove(0),
    ];
    // Classic: shown, disabled, and a click does nothing; the camera can
    // still go to the worker.
    for target in changing {
        // The list may scroll: the job's row is its last.
        game.cities[city].worker_scroll = if target == Target::WorkerJobRemove(0) {
            99
        } else {
            0
        };
        assert_eq!(
            find_button(&game, target).state,
            ButtonState::Disabled,
            "{target:?}"
        );
        game.handle_click(button_cursor(&game, target), SCREEN, ClickMode::Normal);
        assert_eq!(game.team_plan(team), plan, "{target:?}");
    }
    game.cities[city].worker_scroll = 0;
    let show = find_button(&game, Target::ShowWorker(9_000));
    assert_ne!(show.state, ButtonState::Disabled);
    // ImGui: the same.
    let mut screen = ImGuiScreen::new();
    for target in changing {
        screen.click(&mut game, target);
        assert_eq!(game.team_plan(team), plan, "{target:?}");
    }
    assert!(game.waiting_for_peers());
}

#[test]
fn a_frozen_plan_locks_a_waiting_queue_row_like_any_other() {
    let mut game = city_view();
    game.stockpiles[Team::Blue.index()] = Stock::default();
    let city = game.selected_city.unwrap();
    game.queue_build(city, Build::Unit(BuildUnit::Siege));
    game.queue_build(city, Build::Gather);
    let mut panel = PanelBuilder::default();
    game.city_queue_panel(city, usize::MAX, &mut panel);
    // Waiting for the others' plans, nothing in the queue can change.
    panel.freeze_plan();
    let rows: Vec<(bool, bool)> = panel
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::QueueItem(item) => Some((item.waiting, item.locked)),
            _ => None,
        })
        .collect();
    assert_eq!(rows, [(true, true), (false, true)]);
}

#[test]
fn scout_and_settler_cards_queue_from_the_city_and_say_why_when_they_cannot() {
    let mut game = GameState::city_scenario();
    game.open_city(0);
    let tooltip = |game: &GameState, card: &Button| -> String {
        line_strings(game.tooltip_lines(card).into_iter().map(|(_, l)| l)).join(" ")
    };
    // A city of 2 is too small for a Settler: the card is dimmed and
    // says so, and its tooltip gives the rule.
    catalog_cursor(&mut game, Target::BuildSettler);
    let settler = find_button(&game, Target::BuildSettler);
    assert_eq!(settler.state, ButtonState::Disabled);
    assert_eq!(settler.hint, "NEEDS POPULATION 3");
    let text = tooltip(&game, &settler);
    assert!(text.contains("NEEDS POPULATION 3"), "{text}");
    assert!(text.contains("TAKES A CITIZEN"), "{text}");

    // The Scout card queues a scout, and then says it's one at a time.
    let card = catalog_cursor(&mut game, Target::BuildScout);
    let scout = find_button(&game, Target::BuildScout);
    assert_eq!(scout.state, ButtonState::Ready);
    assert!(
        scout.hint.contains(&format!("{FOOD_ICON}2 {WOOD_ICON}4")),
        "{}",
        scout.hint
    );
    assert!(
        scout.hint.ends_with(&format!("{TIME_ICON}2")),
        "{}",
        scout.hint
    );
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(game.cities[0].queue, [Queued::new(Build::Scout)]);
    let scout = find_button(&game, Target::BuildScout);
    assert_eq!(
        (scout.state, scout.hint.as_str()),
        (ButtonState::Disabled, "ONE SCOUT AT A TIME")
    );

    // Grown to 3, the Settler card queues one: 6 turns, city or not.
    game.cities[0].population = 3;
    let card = catalog_cursor(&mut game, Target::BuildSettler);
    let settler = find_button(&game, Target::BuildSettler);
    assert_eq!(settler.state, ButtonState::Ready);
    assert!(
        settler
            .hint
            .contains(&format!("{FOOD_ICON}30 {WOOD_ICON}10")),
        "{}",
        settler.hint
    );
    assert!(
        settler.hint.ends_with(&format!("{TIME_ICON}6")),
        "{}",
        settler.hint
    );
    game.handle_click(card, SCREEN, ClickMode::Normal);
    assert_eq!(
        game.cities[0].queue,
        [Queued::new(Build::Scout), Queued::new(Build::Settler)]
    );
}

#[test]
fn imgui_queues_a_scout_and_a_settler_from_their_cards() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].population = 3;
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, Target::BuildScout);
    screen.click(&mut game, Target::BuildSettler);
    assert_eq!(
        game.cities[city].queue,
        [Queued::new(Build::Scout), Queued::new(Build::Settler)]
    );
}

#[test]
fn a_settler_waiting_for_citizens_says_so_in_its_row_and_the_tray() {
    let mut game = city_view();
    let city = game.selected_city.unwrap();
    game.cities[city].population = 2;
    game.cities[city].queue = vec![Queued::new(Build::Settler)];
    let mut queue = PanelBuilder::default();
    game.city_queue_panel(city, usize::MAX, &mut queue);
    let rows: Vec<(String, bool)> = queue
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::QueueItem(item) => Some((item.label.clone(), item.waiting)),
            _ => None,
        })
        .collect();
    assert_eq!(rows.len(), 1);
    assert!(rows[0].0.contains("WAITS FOR POP 3"), "{}", rows[0].0);
    assert!(rows[0].1, "tinted as waiting");
    let mut tray = PanelBuilder::default();
    game.city_tray(city, &mut tray);
    let text = line_strings(tray.rows.iter().filter_map(|row| match row {
        Row::Text(_, line) => Some(line.clone()),
        _ => None,
    }))
    .join(" ");
    assert!(text.contains("SETTLER WAITS FOR POPULATION 3"), "{text}");
}
