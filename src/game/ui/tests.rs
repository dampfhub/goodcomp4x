use super::builder::{ButtonSpec, Row};
use super::text::{end_turn_label, signed_quantity, wrap};
use super::*;

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
fn clicking_the_open_city_keeps_city_management_open() {
    let mut game = city_view();
    let city = game.cities[game.selected_city.unwrap()].pos;
    game.handle_click(hex_cursor(&game, city), SCREEN, ClickMode::Normal);
    assert_eq!(game.selected_city, Some(0));
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
fn tooltip_and_city_panel_show_an_unseen_hex_as_last_seen() {
    let (mut game, city, far) = crate::game::fog::tests::remembered_route_hex();
    game.inspected_tile = Some(far);
    let read = |game: &GameState| {
        let tooltip = line_strings(game.tile_tooltip_lines(far).into_iter().map(|(_, l)| l));
        let tray = panel_strings(|panel| game.city_tray(city, panel));
        (tooltip, tray)
    };
    let before = read(&game);
    assert!(
        before.0.iter().any(|s| s.contains("REACHES CITY")),
        "{:?}",
        before.0
    );
    assert!(
        before.1.iter().any(|s| s.starts_with("SELECTED TILE")),
        "{:?}",
        before.1
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
    assert_eq!(end_turn_label((3, 1)), "3 UNITS NEED ORDERS");
    assert_eq!(end_turn_label((1, 1)), "UNIT NEEDS ORDERS");
    assert_eq!(end_turn_label((0, 1)), "CHOOSE PRODUCTION");
    assert_eq!(end_turn_label((0, 2)), "2 CITIES NEED PRODUCTION");
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
fn clicking_a_tile_with_nothing_selected_offers_worker_jobs() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.handle_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert_eq!(game.inspected_tile, Some(hex));
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
    game.handle_click(
        button_cursor(&game, Target::WorkerJob(JobKind::Road)),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(game.cities[0].worker_jobs.len(), 1);
    assert_eq!(game.cities[0].worker_jobs[0].hex, hex);
    // Now queued, the button says so.
    let layout = game.layout(SCREEN);
    let road = layout
        .buttons
        .iter()
        .find(|b| b.target == Target::WorkerJob(JobKind::Road))
        .unwrap();
    assert_eq!(road.state, ButtonState::Queued);
}

#[test]
fn a_wall_is_placed_by_dragging_along_hex_edges() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.handle_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    let wall = Target::WorkerJob(JobKind::Wall);
    game.handle_click(button_cursor(&game, wall), SCREEN, ClickMode::Normal);
    assert_eq!(game.placing_barrier, Some(JobKind::Wall));
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
        assert!(game.paint_barrier_at(edge_cursor(&game, n), SCREEN, false));
        // Passing the same edge again adds nothing.
        game.paint_barrier_at(edge_cursor(&game, n), SCREEN, false);
    }
    let jobs = &game.cities[0].worker_jobs;
    assert_eq!(jobs.len(), 3);
    for (job, n) in jobs.iter().zip(&sides) {
        assert_eq!(job.kind, JobKind::Wall);
        let placed = crate::game::hex::edge(job.hex, job.across.unwrap());
        assert_eq!(placed, crate::game::hex::edge(hex, *n));
    }

    // A press on the panel itself doesn't place anything through it.
    assert!(!game.paint_barrier_at(button_cursor(&game, wall), SCREEN, true));

    // Escape stops placing but leaves the tile panel, then closes it.
    assert!(game.clear_selection());
    assert_eq!(game.placing_barrier, None);
    assert_eq!(game.inspected_tile, Some(hex));
    let n = sides[0];
    assert!(!game.paint_barrier_at(edge_cursor(&game, n), SCREEN, false));
}

#[test]
fn a_worker_out_can_be_recalled_from_its_tile_or_its_city() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.inspected_tile = Some(hex);
    game.queue_worker_job(JobKind::Fort);
    // Two hexes out, one a turn.
    game.resolve_workers();
    game.resolve_workers();
    let id = game.field_workers[0].id;
    assert_eq!(game.field_workers[0].pos, hex);

    // From the city panel.
    game.select_city();
    let recall = Target::RecallWorker(id);
    assert!(
        game.layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == recall)
    );
    game.leave_city_view();

    // And from the tile it stands on.
    game.handle_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert_eq!(game.inspected_tile, Some(hex));
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
fn escape_closes_the_tile_panel() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.handle_click(hex_cursor(&game, hex), SCREEN, ClickMode::Normal);
    assert!(game.clear_selection());
    assert_eq!(game.inspected_tile, None);
    assert!(!game.clear_selection(), "nothing left to close");
}

#[test]
fn the_city_lists_its_worker_jobs_and_removes_them() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.inspected_tile = Some(hex);
    game.queue_worker_job(JobKind::Road);
    game.queue_worker_job(JobKind::Fort);
    game.select_city();
    let remove = Target::WorkerJobRemove(0);
    assert!(
        game.layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == remove)
    );
    game.handle_click(button_cursor(&game, remove), SCREEN, ClickMode::Normal);
    let jobs = &game.cities[0].worker_jobs;
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].kind, JobKind::Fort);

    game.queue_selected_city_worker();
    assert_eq!(game.cities[0].queue.last(), Some(&Build::Worker));
    assert!(
        game.layout(SCREEN)
            .buttons
            .iter()
            .any(|b| b.target == Target::BuildWorker)
    );
}

#[test]
fn worker_jobs_reorder_by_dragging() {
    let (mut game, hex) = empty_tile_near_blue_city();
    game.inspected_tile = Some(hex);
    game.queue_worker_job(JobKind::Road);
    game.queue_worker_job(JobKind::Fort);
    game.select_city();
    game.reorder_queue(QueueKind::Workers, 1, 0);
    let kinds: Vec<_> = game.cities[0].worker_jobs.iter().map(|j| j.kind).collect();
    assert_eq!(kinds, [JobKind::Fort, JobKind::Road]);
}
