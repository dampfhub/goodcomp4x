use super::*;
use crate::game::PLAYER_TEAM;

#[test]
fn hover_text_keeps_the_same_imgui_button_id() {
    let _one = one_context_at_a_time();
    let mut context = ::imgui::Context::create();
    context.io_mut().display_size = [640.0, 480.0];
    context.fonts().build_rgba32_texture();
    let ui = context.frame();
    ui.window("ID test").build(|| {
        assert_eq!(
            ui.new_id_str("MOVE###Unit(Move)"),
            ui.new_id_str("MOVE\nM###Unit(Move)"),
        );
        assert_ne!(
            ui.new_id_str("MOVE##Unit(Move)"),
            ui.new_id_str("MOVE\nM##Unit(Move)"),
        );
    });
}

#[test]
fn default_debug_placement_flows_into_views_until_customized() {
    let mut layout = ImGuiLayoutState::default();
    let mut default = WindowGeometry {
        floating_pos: Vec2::new(900.0, 80.0),
        floating_size: Vec2::new(300.0, 350.0),
        ..Default::default()
    };
    layout
        .debug_view_geometry
        .insert(ViewScope::Default, default);
    assert_eq!(
        layout.debug_geometry_for_view(ViewScope::City).floating_pos,
        default.floating_pos
    );
    default.floating_pos = Vec2::new(700.0, 110.0);
    layout
        .debug_view_geometry
        .insert(ViewScope::Default, default);
    assert_eq!(
        layout
            .debug_geometry_for_view(ViewScope::Troop)
            .floating_pos,
        default.floating_pos
    );
    let mut city = default;
    city.floating_pos = Vec2::new(100.0, 400.0);
    layout.debug_view_geometry.insert(ViewScope::City, city);
    layout.debug_view_overrides.insert(ViewScope::City);
    default.floating_pos = Vec2::new(600.0, 120.0);
    layout
        .debug_view_geometry
        .insert(ViewScope::Default, default);
    assert_eq!(
        layout.debug_geometry_for_view(ViewScope::City).floating_pos,
        city.floating_pos
    );
    assert_eq!(
        layout
            .debug_geometry_for_view(ViewScope::Troop)
            .floating_pos,
        default.floating_pos
    );
    layout.debug_view_boxes.insert(ViewScope::City, 42);
    layout
        .debug_context_relations
        .insert(SelectionKind::City, beside());
    layout.clear_view_debug_override(ViewScope::City);
    assert_eq!(
        layout.debug_geometry_for_view(ViewScope::City).floating_pos,
        default.floating_pos
    );
    assert!(!layout.debug_view_boxes.contains_key(&ViewScope::City));
    assert!(
        !layout
            .debug_context_relations
            .contains_key(&SelectionKind::City)
    );
}

/// Debug docked to the right of Selection, half and half.
fn beside() -> QueueDockRelation {
    QueueDockRelation {
        direction: ::imgui::sys::ImGuiDir_Right,
        fraction: 0.5,
        anchor_group: false,
    }
}

#[test]
fn a_view_reset_forgets_the_debug_docking_of_every_selection_shown_in_it() {
    // #109: Ctrl+Shift+R in City / Building left an open interior's
    // Debug docking behind.
    let kinds = [
        SelectionKind::Interior,
        SelectionKind::City,
        SelectionKind::Barracks,
        SelectionKind::Unit,
        SelectionKind::Group,
    ];
    let docked = || {
        let mut layout = ImGuiLayoutState::default();
        for kind in kinds {
            layout.debug_context_relations.insert(kind, beside());
        }
        layout
    };
    for view in [ViewScope::Default, ViewScope::City, ViewScope::Troop] {
        let mut layout = docked();
        layout.clear_view_debug_override(view);
        for kind in kinds {
            assert_eq!(
                layout.debug_context_relations.contains_key(&kind),
                kind.view() != view,
                "{kind:?} after resetting {view:?}"
            );
        }
    }
    // Each is the kind of what the game shows, in the view the game
    // is in: an open interior is City / Building's.
    let mut game = GameState::city_scenario();
    let city = game
        .cities
        .iter()
        .position(|city| city.team == PLAYER_TEAM)
        .unwrap();
    game.open_city_interior(city);
    let (kind, context) = game.selection_kind().unwrap();
    assert_eq!(kind, SelectionKind::Interior);
    assert_eq!(context, format!("interior-{city}"));
    assert_eq!(game.layout_view(), ViewScope::City);
    let mut layout = docked();
    layout.clear_view_debug_override(game.layout_view());
    assert!(!layout.debug_context_relations.contains_key(&kind));
    game.close_city_interior();
    game.clear_selection();
    game.set_selection(vec![
        game.units
            .iter()
            .position(|unit| unit.team == PLAYER_TEAM)
            .unwrap(),
    ]);
    let (kind, _) = game.selection_kind().unwrap();
    assert_eq!(kind, SelectionKind::Unit);
    assert_eq!(game.layout_view(), kind.view());
    game.clear_selection();
    assert_eq!(game.selection_kind(), None);
    assert_eq!(game.layout_view(), ViewScope::Default);
}

#[test]
fn restoring_a_debug_docking_a_reset_forgot_stops_instead_of_panicking() {
    // A reset can forget the relation Debug is being docked back by
    // while that's under way.
    let _one = one_context_at_a_time();
    let mut context = ::imgui::Context::create();
    context
        .io_mut()
        .config_flags
        .insert(::imgui::ConfigFlags::DOCKING_ENABLE);
    context.io_mut().display_size = [1280.0, 720.0];
    context.fonts().build_rgba32_texture();
    let mut layout = ImGuiLayoutState::default();
    layout
        .debug_context_relations
        .insert(SelectionKind::Interior, beside());
    let ui = context.frame();
    // Both windows there, apart.
    ui.window(slot_titles()[SELECTION])
        .position([20.0, 100.0], ::imgui::Condition::Always)
        .build(|| {});
    ui.window(slot_titles()[DEBUG])
        .position([900.0, 100.0], ::imgui::Condition::Always)
        .build(|| {});
    // Interior opens: two tries to dock Debug beside Selection again.
    layout.maintain_debug_scope(Some(SelectionKind::Interior));
    assert_eq!(layout.debug_restore_attempts, 2);
    layout.clear_view_debug_override(ViewScope::City);
    layout.maintain_debug_scope(Some(SelectionKind::Interior));
    assert_eq!(layout.debug_restore_attempts, 0, "nothing to restore");
    context.render();
}

#[test]
fn reset_shortcut_is_only_available_while_editing_a_contextual_view() {
    let mut layout = ImGuiLayoutState::default();
    assert!(!layout.request_reset_active_view());
    layout.active_view = ViewScope::City;
    layout.editing_outer = true;
    assert!(!layout.request_reset_active_view());
    layout.editing_outer = false;
    assert!(layout.request_reset_active_view());
    assert_eq!(layout.pending_view_reset, Some(ViewScope::City));
}

#[test]
fn boxes_follow_their_view_or_the_shared_outer_layer() {
    let mut layout = ImGuiLayoutState::default();
    let viewport = Vec2::new(1280.0, 800.0);
    layout.active_view = ViewScope::City;
    layout.create_box(viewport);
    layout.editing_outer = true;
    layout.create_box(viewport);
    layout.active_view = ViewScope::Troop;
    layout.editing_outer = false;
    layout.create_box(viewport);
    assert_eq!(
        layout
            .outer_boxes
            .iter()
            .filter(|b| layout.box_visible(b))
            .count(),
        2
    );
    layout.active_view = ViewScope::Default;
    assert_eq!(
        layout
            .outer_boxes
            .iter()
            .filter(|b| layout.box_visible(b))
            .count(),
        1
    );
    layout.active_view = ViewScope::City;
    assert_eq!(
        layout
            .outer_boxes
            .iter()
            .filter(|b| layout.box_visible(b))
            .count(),
        2
    );
}

#[test]
fn outer_boxes_keep_distinct_space_without_selection() {
    let mut layout = ImGuiLayoutState::default();
    let viewport = Vec2::new(1280.0, 800.0);
    layout.editing_outer = true;
    layout.create_box(viewport);
    layout.create_box(viewport);
    assert_eq!(layout.outer_boxes.len(), 2);
    assert_ne!(layout.outer_boxes[0].id, layout.outer_boxes[1].id);
    let rect = |outer: &OuterBox| Rect {
        min: Vec2::new(
            outer.geometry.pos.x,
            viewport.y - outer.geometry.pos.y - outer.geometry.size.y,
        ),
        max: Vec2::new(
            outer.geometry.pos.x + outer.geometry.size.x,
            viewport.y - outer.geometry.pos.y,
        ),
    };
    assert!(!rect(&layout.outer_boxes[0]).overlaps(rect(&layout.outer_boxes[1]), 0.0));
}

#[test]
fn captured_unit_action_targets_its_stable_unit_id() {
    let mut game = GameState::new();
    let index = game
        .units
        .iter()
        .position(|unit| unit.team == PLAYER_TEAM)
        .unwrap();
    let pin = PinnedPanel {
        kind: PinnedKind::Unit,
        id: game.units[index].id,
    };
    game.selected = None;
    game.activate_pinned(
        pin,
        &PinnedGroups::default(),
        Target::Unit(UnitAction::Hold),
    );
    assert!(game.units[index].holding);
}

#[test]
fn pinned_city_action_uses_its_own_city_after_selection_closes() {
    let mut game = GameState::city_scenario();
    game.select_city();
    let pin = game.selected_pin().unwrap();
    game.leave_city_view();
    assert!(game.selected_city.is_none());
    game.activate_pinned(
        pin,
        &PinnedGroups::default(),
        Target::Build(BuildUnit::Melee),
    );
    assert_eq!(game.selected_city, game.pinned_city_index(pin));
    assert!(!game.cities[game.selected_city.unwrap()].queue.is_empty());
}

/// The player's units, by index.
fn player_units(game: &GameState) -> Vec<usize> {
    (0..game.units.len())
        .filter(|&i| game.units[i].team == PLAYER_TEAM)
        .collect()
}

fn unit_pin(game: &GameState, unit: usize) -> PinnedPanel {
    PinnedPanel {
        kind: PinnedKind::Unit,
        id: game.units[unit].id,
    }
}

#[test]
fn a_captured_units_tooltips_describe_that_unit() {
    let mut game = GameState::new();
    let units = player_units(&game);
    let (selected, pinned) = (units[0], units[1]);
    game.units[pinned].ability_cooldown = 2;
    let pin = unit_pin(&game, pinned);
    let focus = game.pin_focus(pin, &PinnedGroups::default()).unwrap();
    // The Ability button of `unit`'s tray, and its tooltip for `subject`.
    let tooltip = |game: &GameState, unit: usize, subject| {
        let ability = Target::Unit(UnitAction::Ability);
        let buttons = game.unit_buttons(unit);
        let spec = buttons.iter().find(|b| b.target == ability).unwrap();
        game.subject_tooltip_lines(ability, "", spec.unavailable.as_deref(), subject)
            .into_iter()
            .flat_map(|(_, line)| line.into_iter().map(|(text, _)| text))
            .collect::<String>()
    };
    // Nothing selected: the panel's unit still has a tooltip.
    let subject = GameState::pin_subject(&focus);
    assert!(tooltip(&game, pinned, subject).contains("READY IN"));
    // Another unit selected: still the panel's unit's, not the selection's.
    game.set_selection(vec![selected]);
    assert!(tooltip(&game, pinned, subject).contains("READY IN"));
    let selection = game.selection_subject();
    assert!(!tooltip(&game, selected, selection).contains("READY IN"));
}

#[test]
fn a_captured_unit_is_selected_as_the_map_selects_it() {
    let mut game = GameState::city_scenario();
    let unit = player_units(&game)[0];
    let pin = unit_pin(&game, unit);
    game.select_city();
    game.placing_job = Some(JobKind::Road);
    game.activate_pinned(
        pin,
        &PinnedGroups::default(),
        Target::Unit(UnitAction::Move),
    );
    assert_eq!(game.selected, Some(unit));
    assert!(game.group.is_empty());
    assert_eq!(game.selected_city, None, "the city view closes");
    assert_eq!(game.placing_job, None, "placing belongs to the city");
    assert_eq!(game.ui_click_mode, Some(ClickMode::Move));
}

#[test]
fn a_captured_units_armed_button_toggles_off_while_it_is_selected() {
    let mut game = GameState::new();
    let unit = player_units(&game)[0];
    let pin = unit_pin(&game, unit);
    let groups = PinnedGroups::default();
    game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
    assert_eq!(game.ui_click_mode, Some(ClickMode::Move));
    game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
    assert_eq!(game.ui_click_mode, None, "the second click disarms it");
}

#[test]
fn a_captured_group_down_to_one_member_selects_it_as_one_unit() {
    let mut game = GameState::new();
    let units = player_units(&game);
    let ids: Vec<u32> = units[..2].iter().map(|&i| game.units[i].id).collect();
    let pin = PinnedPanel {
        kind: PinnedKind::Group,
        id: 7,
    };
    let mut groups = PinnedGroups::default();
    groups.insert(7, ids.clone());
    game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
    assert_eq!(game.group, units[..2], "both members");
    assert_eq!(game.selected, None);
    // One member dies: the other is selected alone, as `set_selection`
    // keeps it.
    game.units.remove(units[0]);
    let left = game.units.iter().position(|u| u.id == ids[1]).unwrap();
    game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
    assert_eq!(game.selected, Some(left));
    assert!(game.group.is_empty());
}

#[test]
fn a_captured_city_already_open_is_not_opened_again() {
    let mut game = GameState::city_scenario();
    game.fund(crate::game::Team::Blue);
    game.select_city();
    let pin = game.selected_pin().unwrap();
    game.camera.center += Vec2::new(3.0, 0.0);
    let camera = game.camera.center;
    game.notice = "SOMETHING TO KEEP".into();
    game.activate_pinned(
        pin,
        &PinnedGroups::default(),
        Target::Build(BuildUnit::Melee),
    );
    assert_eq!(game.camera.center, camera, "no glide back to the city");
    assert!(!game.notice.starts_with("CHOOSE WHAT"), "{}", game.notice);
    // From a unit's selection it opens the city, as the map does.
    let unit = player_units(&game)[0];
    game.set_selection(vec![unit]);
    game.activate_pinned(pin, &PinnedGroups::default(), Target::ToggleYields);
    assert_eq!(game.selected_city, game.pinned_city_index(pin));
    assert_eq!(game.selected, None);
}

#[test]
fn captured_panels_do_nothing_while_a_turn_plays_out() {
    let mut game = GameState::new();
    let units = player_units(&game);
    game.set_selection(vec![units[0]]);
    game.resolve_turn();
    assert!(game.is_playing_out());
    let pin = unit_pin(&game, units[1]);
    game.activate_pinned(
        pin,
        &PinnedGroups::default(),
        Target::Unit(UnitAction::Hold),
    );
    assert!(!game.units[units[1]].holding);
}

#[test]
fn captured_panels_clear_whenever_the_game_changes() {
    let mut game = GameState::city_scenario();
    let mut layout = ImGuiLayoutState::default();
    assert!(!layout.game_changed(game.generation), "the first frame");
    assert!(!layout.game_changed(game.generation), "the same game");
    // F1-F4 and F12 as the debug panel's buttons do: `switch_scenario`.
    game.switch_scenario(Scenario::Cities);
    assert!(layout.game_changed(game.generation));
    assert!(!layout.game_changed(game.generation));
    // F6 changes nothing; F7 loads another game, even the same one.
    game.save_state();
    assert!(!layout.game_changed(game.generation));
    game.load_state();
    assert!(layout.game_changed(game.generation));
    // A network game (`App` keeps the menus of the one it replaces).
    let mut joined = GameState::city_scenario();
    joined.keep_menus_of(&game);
    assert!(layout.game_changed(joined.generation));
}

#[test]
fn pinned_panel_geometry_reserves_room_for_floating_panels() {
    let mut layout = ImGuiLayoutState::default();
    let pin = PinnedPanel {
        kind: PinnedKind::City,
        id: 1,
    };
    layout.pinned_geometry.insert(
        pin,
        WindowGeometry {
            pos: Vec2::new(14.0, 66.0),
            size: Vec2::new(370.0, 450.0),
            ..Default::default()
        },
    );
    let viewport = Vec2::new(1280.0, 800.0);
    let mut sizes = [
        Some(Vec2::new(370.0, 450.0)),
        None,
        Some(Vec2::new(330.0, 300.0)),
        None,
        None,
        None,
    ];
    let positions = layout.plan(viewport, &mut sizes);
    let selection = positions[SELECTION].unwrap();
    let selected = Rect {
        min: Vec2::new(
            selection.x,
            viewport.y - selection.y - sizes[SELECTION].unwrap().y,
        ),
        max: Vec2::new(
            selection.x + sizes[SELECTION].unwrap().x,
            viewport.y - selection.y,
        ),
    };
    let pinned = Rect {
        min: Vec2::new(14.0, 800.0 - 66.0 - 450.0),
        max: Vec2::new(384.0, 800.0 - 66.0),
    };
    assert!(!selected.overlaps(pinned, 0.0));
}

mod layout;
