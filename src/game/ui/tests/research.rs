//! The research view's tests: its tech tree, its queue, the locks it lifts
//! and the top bar's research, in both presentations.

use super::*;

/// The research view (T) on the Cities map, nothing researched.
fn research_view() -> GameState {
    let mut game = GameState::city_scenario();
    game.research[0] = Default::default();
    game.toggle_research();
    assert!(game.research_view());
    game
}

#[test]
fn the_research_view_docks_its_tree_and_queues_a_tech_in_both_presentations() {
    use crate::game::research::Tech;
    let techs: Vec<Tech> = Tech::all().collect();
    for screen in [SCREEN, Vec2::new(1280.0, 720.0)] {
        let mut game = research_view();
        game.activate_target(Target::Research(techs[techs.len() - 1]));
        let layout = game.layout(screen);
        let tray = panel_with(&layout, Target::Research(techs[0]))
            .unwrap_or_else(|| panic!("the tree docks at {screen}"));
        assert!(tray.min.cmpge(Vec2::splat(MARGIN)).all(), "{screen}");
        assert!(tray.max.y <= screen.y - TOP_BAR_HEIGHT, "{screen}");
        for &(min, max) in &layout.panels {
            let other = Rect { min, max };
            assert!(min == tray.min || !tray.overlaps(other, 0.0), "{screen}");
        }
        // Every tech's card is in it and takes its click; the queue panel
        // holds a row for each tech queued.
        for &tech in &techs {
            assert!(
                clickable_in(&layout, tray, Target::Research(tech)),
                "{tech:?} at {screen}"
            );
        }
        let rows = layout
            .queue_items
            .iter()
            .filter(|i| i.kind == QueueKind::Research)
            .count();
        assert!(rows >= 1, "{rows} rows at {screen}");
    }
    // Classic: a card queues its tech; the row's X takes it off.
    let mut game = research_view();
    let first = techs[0];
    game.handle_click(
        button_cursor(&game, Target::Research(first)),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(game.research(Team::Blue).queue, [Queued::new(first)]);
    let card = find_button(&game, Target::Research(first));
    assert!(card.queued);
    game.handle_click(
        button_cursor(&game, Target::ResearchQueueRemove(0)),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(
        game.research(Team::Blue).queue.is_empty(),
        "{}",
        game.notice
    );
    // ImGui: the same card, and the queue's Clear.
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, Target::Research(first));
    assert_eq!(game.research(Team::Blue).queue, [Queued::new(first)]);
    // The queue panel has just shown: let it settle where the dock puts it.
    screen.settle(&mut game);
    screen.click(&mut game, Target::ClearResearchQueue);
    assert!(
        game.research(Team::Blue).queue.is_empty(),
        "{}",
        game.notice
    );
    // A researched tech's card is off, and says so.
    game.research[0].done.insert(first);
    let card = find_button(&game, Target::Research(first));
    assert_eq!(card.state(), ButtonState::Disabled);
    assert_eq!(
        card.unavailable.as_deref(),
        Some(text!("research_card_done"))
    );
    // Escape closes it; T opens it again, and opening a city closes it.
    game.press_escape();
    assert!(!game.research_view());
    game.toggle_research();
    assert!(game.research_view());
    game.open_city(0);
    game.update(0.0);
    game.leave_city_view();
    assert!(
        !game.research_view(),
        "it doesn't come back behind the city"
    );
}

#[test]
fn a_locked_card_names_the_tech_that_unlocks_it_in_both_presentations() {
    use crate::game::research::{Tech, Unlock};
    let mut game = GameState::city_scenario();
    game.units.clear();
    game.cities[0].barracks = Some(Hex::new(-2, 0));
    game.cities[0].built.push(Building::Barracks);
    game.cities[0].workers = 1;
    game.research[0] = Default::default();
    game.open_city(0);
    let ranged = Tech::unlocking(Unlock::Unit(BuildUnit::Ranged)).unwrap();
    let needs = text!("research_lock", tech = ranged.name());
    let mill = Tech::unlocking(Unlock::Building(Building::Mill)).unwrap();
    let needs_mill = text!("research_lock", tech = mill.name());
    for (target, needs) in [
        (Target::Build(BuildUnit::Ranged), &needs),
        (Target::Building(Building::Mill), &needs_mill),
    ] {
        catalog_cursor(&mut game, target);
        let card = find_button(&game, target);
        assert_eq!(card.state(), ButtonState::Disabled, "{target:?}");
        assert_eq!(&card.hint, needs, "{target:?}");
        let tip = line_strings(game.tooltip_lines(&card).into_iter().map(|(_, l)| l));
        assert_eq!(tip.last(), Some(needs), "{target:?}: {tip:?}");
    }
    // ImGui draws it off, so a click there queues nothing.
    game.cities[0].building_scroll = 0;
    let mut screen = ImGuiScreen::new();
    let before = game.cities[0].queue.clone();
    screen.click(&mut game, Target::Build(BuildUnit::Ranged));
    assert_eq!(game.cities[0].queue, before);
    // The Barracks' card too.
    game.open_barracks(0);
    let card = find_button(&game, Target::BarracksBuild(BuildUnit::Ranged));
    assert_eq!(card.hint, needs);
    assert_eq!(card.unavailable.as_deref(), Some(needs.as_str()));
    // Researched, it's there.
    game.research[0].done.insert(ranged);
    let card = find_button(&game, Target::BarracksBuild(BuildUnit::Ranged));
    assert_eq!(card.state(), ButtonState::Ready);
}

#[test]
fn the_top_bar_names_the_research_and_its_turns() {
    use crate::game::research::Tech;
    let mut game = research_view();
    let status =
        |game: &GameState| -> String { game.status_line().into_iter().map(|(t, _)| t).collect() };
    assert!(
        status(&game).ends_with(text!("status_research_none")),
        "{}",
        status(&game)
    );
    let tech = Tech::all().next().unwrap();
    game.activate_target(Target::Research(tech));
    let expected = format!("{} {}", tech.name(), turns_icon(tech.turns()));
    assert!(status(&game).ends_with(&expected), "{}", status(&game));
    *game.stock_mut(Team::Blue) = Stock::default();
    game.cities.iter_mut().for_each(|c| c.clusters.clear());
    assert!(
        status(&game).ends_with(&text!("status_research_waits", tech = tech.name())),
        "{}",
        status(&game)
    );
}

/// The research view with a tech of each standing: TECH I-A researched,
/// II-B researched now (paid, part done), and III-C queued with what it
/// needs (I-B, II-C), the rest available or locked.
fn research_in_progress() -> GameState {
    use crate::game::research::Tech;
    let techs: Vec<Tech> = Tech::all().collect();
    let mut game = research_view();
    game.research[0].done.insert(techs[0]);
    game.activate_target(Target::Research(techs[4]));
    game.activate_target(Target::Research(techs[9]));
    let head = &mut game.research[0].queue[0];
    head.paid = true;
    head.progress = 5;
    game
}

#[test]
fn the_tech_tree_puts_each_tier_in_a_column_with_lines_to_what_needs_it() {
    use super::tech_tree::{LinkState, TechState};
    use crate::game::research::Tech;
    let game = research_in_progress();
    let tree = game.tech_tree();
    let tier = |tech: Tech| tree.slots[usize::from(tech.0)].0;
    let state = |i: usize| tree.cards[i].1.state;
    // Each card says where it stands.
    assert_eq!(state(0), TechState::Researched);
    assert_eq!(state(4), TechState::Researching);
    assert_eq!(tree.cards[4].1.progress, Some(5.0 / 16.0));
    assert_eq!(state(1), TechState::Queued);
    assert_eq!(
        tree.cards[1].1.tag,
        Some(text!("research_card_queued", place = 2))
    );
    assert_eq!(state(2), TechState::Available);
    assert_eq!(state(6), TechState::Locked, "II-D needs I-C");
    assert_eq!(tree.cards[0].1.detail, text!("research_card_done"));
    // Its unlocks, with their names.
    let names: Vec<&str> = tree.cards[4].1.unlocks.iter().map(|u| u.name).collect();
    assert_eq!(names, ["SIEGE", "WORKSHOP", "SMELTER"]);
    // A line from each tech to each that needs it, green from a
    // researched tech, gold between two queued.
    for tech in Tech::all() {
        for need in tech.requires() {
            let (from, to) = (usize::from(need.0), usize::from(tech.0));
            let link = tree.links.iter().find(|l| l.from == from && l.to == to);
            assert!(link.is_some(), "{need:?} to {tech:?}");
            assert!(tier(need) < tier(tech));
        }
    }
    let link = |from: usize, to: usize| {
        tree.links
            .iter()
            .find(|l| l.from == from && l.to == to)
            .unwrap()
            .state
    };
    assert_eq!(link(0, 4), LinkState::Done);
    assert_eq!(link(5, 9), LinkState::Planned);
    assert_eq!(link(2, 6), LinkState::Open);

    // Classic: side by side, tier by tier, nothing overlapping, and no line
    // crossing a card.
    let metrics = tree.classic_metrics();
    let placed = tree.place(tree.width(&metrics), &metrics);
    assert!(!placed.stacked);
    let rect = |i: usize| Rect {
        min: placed.cards[i].0,
        max: placed.cards[i].1,
    };
    for i in 0..tree.cards.len() {
        for j in i + 1..tree.cards.len() {
            assert!(!rect(i).overlaps(rect(j), 0.0), "{i} and {j}");
            if tree.slots[i].0 < tree.slots[j].0 {
                assert!(rect(i).max.x < rect(j).min.x, "{i} left of {j}");
            }
        }
    }
    for &(min, max, _) in &placed.wires {
        let wire = Rect {
            min: min + 1.5,
            max: max - 1.5,
        };
        for i in 0..tree.cards.len() {
            assert!(!wire.overlaps(rect(i), 0.0), "a line crosses card {i}");
        }
    }
    // Too narrow for the columns: the tiers stack, still apart, in the width.
    let width = metrics.card_width * 1.5;
    let narrow = tree.place(width, &metrics);
    assert!(narrow.stacked && narrow.wires.is_empty());
    assert!(narrow.size.x <= width);
    for (i, &(a_min, a_max)) in narrow.cards.iter().enumerate() {
        for &(b_min, b_max) in &narrow.cards[i + 1..] {
            let (a, b) = (
                Rect {
                    min: a_min,
                    max: a_max,
                },
                Rect {
                    min: b_min,
                    max: b_max,
                },
            );
            assert!(!a.overlaps(b, 0.0));
        }
    }
}

#[test]
fn the_tech_tree_shows_in_columns_and_queues_a_click_in_both_presentations() {
    use crate::game::research::Tech;
    let techs: Vec<Tech> = Tech::all().collect();
    // Classic: each card is a button that draws itself, the columns left
    // to right by tier, and at 1280x720 the cards drop their unlocks'
    // names to fit.
    let game = research_in_progress();
    for (screen, names) in [(SCREEN, true), (Vec2::new(1280.0, 720.0), false)] {
        let layout = game.layout(screen);
        let card = |tech: Tech| {
            layout
                .buttons
                .iter()
                .find(|b| b.target == Target::Research(tech))
                .unwrap()
        };
        assert!(card(techs[0]).max.x < card(techs[3]).min.x, "{screen}");
        assert!(card(techs[3]).max.x < card(techs[7]).min.x, "{screen}");
        let drawn = card(techs[4]).card.as_ref().expect("a tech card");
        assert_eq!(drawn.names, names, "{screen}");
        assert!(
            layout
                .shapes
                .iter()
                .any(|shape| matches!(shape, Shape::Fill { .. })),
            "the lines at {screen}"
        );
    }
    // A click on a locked card queues it and what it needs.
    let mut game = research_view();
    game.handle_click(
        button_cursor(&game, Target::Research(techs[3])),
        SCREEN,
        ClickMode::Normal,
    );
    let queued: Vec<Tech> = game
        .research(Team::Blue)
        .queue
        .iter()
        .map(|q| q.build)
        .collect();
    assert_eq!(queued, [techs[0], techs[3]]);

    // ImGui: the tree's columns side by side in Selection, its text and
    // icons apart, a card's tooltip, and a card's click queues it.
    let mut game = research_in_progress();
    let mut screen = ImGuiScreen::styled(SCREEN);
    screen.settle(&mut game);
    let at = |tech: Tech| screen_button(&screen, Target::Research(tech));
    assert!(at(techs[0]).x < at(techs[3]).x);
    assert!(at(techs[3]).x < at(techs[7]).x);
    let crowded = crowded_marks("Selection");
    assert!(crowded.is_empty(), "{crowded:#?}");
    let card = screen_button(&screen, Target::Research(techs[2]));
    screen.frame(&mut game, Some(card), false);
    screen.frame(&mut game, Some(card), false);
    let shown = imgui::SHOWN_TOOLTIPS.with_borrow(|shown| shown.last().cloned());
    assert!(
        shown.is_some_and(|lines| lines.iter().any(|l| l.contains("WORK CAMP"))),
        "the card's tooltip"
    );
    screen.click(&mut game, Target::Research(techs[2]));
    assert_eq!(game.research_place(Team::Blue, techs[2]), Some(4));
    // Narrow, the tiers stack: a column's cards one over the other.
    screen
        .layout
        .resize_panel("Selection", Vec2::new(320.0, SCREEN.y - 120.0));
    screen.settle(&mut game);
    let at = |tech: Tech| screen_button(&screen, Target::Research(tech));
    assert_eq!(at(techs[0]).x, at(techs[3]).x);
    assert!(at(techs[0]).y < at(techs[3]).y);
}

/// The middle of the button ImGui drew for `target` last frame.
fn screen_button(screen: &ImGuiScreen, target: Target) -> Vec2 {
    screen
        .button(target)
        .unwrap_or_else(|| panic!("ImGui shows no {target:?} button"))
}
