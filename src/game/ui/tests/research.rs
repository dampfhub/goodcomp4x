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

/// The research view with the last tech queued, and with it what it needs.
fn research_queued() -> GameState {
    use crate::game::research::Tech;
    let mut game = research_view();
    game.activate_target(Target::Research(Tech::all().last().unwrap()));
    game
}

#[test]
fn the_research_view_takes_the_screen_below_the_top_bar_in_both_presentations() {
    use crate::game::research::Tech;
    let techs: Vec<Tech> = Tech::all().collect();
    let game = research_queued();
    let queued = game.research(Team::Blue).queue.len();
    for screen in [SCREEN, Vec2::new(1280.0, 720.0)] {
        // Classic: a ground over everything below the top bar, the tree's
        // panel from the left and the queue's at the right, as tall as the
        // room, and nothing else.
        let layout = game.layout(screen);
        let below = screen.y - TOP_BAR_HEIGHT;
        for corner in [Vec2::splat(1.0), Vec2::new(screen.x - 1.0, below - 1.0)] {
            assert!(layout.covers(corner), "{corner} at {screen}");
        }
        let tree = panel_with(&layout, Target::Research(techs[0])).expect("the tree");
        let queue = panel_with(&layout, Target::ResearchQueueRemove(0)).expect("the queue");
        assert_eq!(tree.min, Vec2::splat(MARGIN), "{screen}");
        assert_eq!(queue.max.x, screen.x - MARGIN, "{screen}");
        assert!(tree.max.x + GAP <= queue.min.x, "{screen}");
        assert_eq!((tree.min.y, tree.max.y), (queue.min.y, queue.max.y));
        assert!(tree.max.y > below - 3.0 * MARGIN, "as tall as the room");
        assert!(layout.roster_chips.is_empty(), "no turn strip");
        for button in &layout.buttons {
            let inside = |panel: Rect| contains(panel.min, panel.max, button.min);
            assert!(
                inside(tree) || inside(queue) || button.min.y >= below,
                "{:?} shows at {screen}",
                button.target
            );
        }
        for &tech in &techs {
            assert!(
                clickable_in(&layout, tree, Target::Research(tech)),
                "{tech:?} at {screen}"
            );
        }
        let rows = layout
            .queue_items
            .iter()
            .filter(|i| i.kind == QueueKind::Research)
            .count();
        assert_eq!(rows, queued, "{screen}");

        // ImGui: one window over everything below the status bar, the
        // tree's columns side by side and the queue at their right, and
        // nothing else drawn.
        let mut game = game.clone();
        let mut imgui = ImGuiScreen::styled(screen);
        imgui.settle(&mut game);
        let (at, size, _) = imgui::native_geometry(imgui::RESEARCH_WINDOW).unwrap();
        assert_eq!(at, Vec2::new(0.0, imgui::STATUS_HEIGHT));
        assert_eq!(size, Vec2::new(screen.x, screen.y - imgui::STATUS_HEIGHT));
        let windows: std::collections::BTreeSet<String> = imgui::DRAWN_MARKS
            .with_borrow(|marks| marks.iter().map(|mark| mark.window.clone()).collect());
        let expected = ["Status", imgui::RESEARCH_WINDOW].map(String::from);
        assert_eq!(windows, expected.into(), "{screen}");
        let at = |target: Target| screen_button(&imgui, target);
        assert!(at(Target::Research(techs[0])).x < at(Target::Research(techs[3])).x);
        assert!(at(Target::Research(techs[3])).x < at(Target::Research(techs[7])).x);
        let remove = at(Target::ResearchQueueRemove(0));
        for &tech in &techs {
            let card = imgui.button(Target::Research(tech));
            assert!(card.is_none_or(|card| card.x < remove.x), "{tech:?}");
        }
        let crowded = crowded_marks(imgui::RESEARCH_WINDOW);
        assert!(crowded.is_empty(), "{screen}: {crowded:#?}");
        assert!(imgui.captures_mouse_at(&mut game, screen / 2.0), "no map");
    }
}

#[test]
fn the_research_view_queues_reorders_and_clears_in_both_presentations() {
    use crate::game::research::Tech;
    let techs: Vec<Tech> = Tech::all().collect();
    // Two techs that need nothing, so either may go first.
    let (first, second) = (techs[0], techs[1]);
    let order = |game: &GameState| -> Vec<Tech> {
        game.research(Team::Blue)
            .queue
            .iter()
            .map(|q| q.build)
            .collect()
    };
    // Classic: a card's click queues it, a row dragged onto another
    // reorders, its X takes it off and Clear empties the queue.
    let mut game = research_view();
    for tech in [first, second] {
        game.handle_click(
            button_cursor(&game, Target::Research(tech)),
            SCREEN,
            ClickMode::Normal,
        );
    }
    assert_eq!(order(&game), [first, second]);
    assert!(find_button(&game, Target::Research(first)).queued);
    let layout = game.layout(SCREEN);
    let row = |index| {
        let row = layout
            .queue_items
            .iter()
            .find(|row| row.kind == QueueKind::Research && row.index == index)
            .unwrap();
        let middle = Vec2::new(
            (row.min.x + row.body_max_x) / 2.0,
            (row.min.y + row.max.y) / 2.0,
        );
        to_ui(middle, SCREEN)
    };
    assert!(game.start_queue_drag_at(row(1), SCREEN));
    game.finish_queue_drag_at(row(0), SCREEN);
    assert_eq!(order(&game), [second, first], "{}", game.notice);
    game.handle_click(
        button_cursor(&game, Target::ResearchQueueRemove(0)),
        SCREEN,
        ClickMode::Normal,
    );
    assert_eq!(order(&game), [first], "{}", game.notice);
    game.handle_click(
        button_cursor(&game, Target::ClearResearchQueue),
        SCREEN,
        ClickMode::Normal,
    );
    assert!(order(&game).is_empty(), "{}", game.notice);

    // ImGui: the same.
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, Target::Research(first));
    screen.click(&mut game, Target::Research(second));
    assert_eq!(order(&game), [first, second]);
    screen.settle(&mut game);
    let row = |i| Target::QueueItem(QueueKind::Research, i);
    let rows = [0, 1].map(|i| screen_button(&screen, row(i)));
    screen.drag(&mut game, rows[1], rows[0]);
    assert_eq!(order(&game), [second, first], "{}", game.notice);
    screen.click(&mut game, Target::ResearchQueueRemove(0));
    assert_eq!(order(&game), [first], "{}", game.notice);
    screen.click(&mut game, Target::ClearResearchQueue);
    assert!(order(&game).is_empty(), "{}", game.notice);
    // Empty, the queue says so.
    let empty = text!("research_queue_empty");
    let says = |shape: &Shape| match shape {
        Shape::Text { line, .. } => line.iter().any(|(text, _)| text == empty),
        _ => false,
    };
    assert!(game.layout(SCREEN).shapes.iter().any(says));

    // A researched tech's card is off, and says so.
    game.research[0].done.insert(first);
    let card = find_button(&game, Target::Research(first));
    assert_eq!(card.state(), ButtonState::Disabled);
    assert_eq!(
        card.unavailable.as_deref(),
        Some(text!("research_card_done"))
    );
    // Escape and Space close it, as does T; opening a city closes it, and
    // so does the settings menu.
    game.press_escape();
    assert!(!game.research_view());
    game.toggle_research();
    assert!(game.exit_structure_menu());
    assert!(!game.research_view());
    game.toggle_research();
    game.toggle_research();
    assert!(!game.research_view());
    game.toggle_research();
    game.open_city(0);
    game.update(0.0);
    game.leave_city_view();
    assert!(
        !game.research_view(),
        "it doesn't come back behind the city"
    );
    game.toggle_research();
    game.activate_target(Target::OpenSettings);
    game.update(0.0);
    game.press_escape();
    assert!(!game.research_view(), "nor behind the menu");
}

/// In ImGui with the game's fonts, the least window wide enough for the
/// tree's columns and the queue.
const WHOLE_TREE: f32 = 1100.0;

/// The research view's text and icons that run into each other in ImGui,
/// or out of what they're drawn in, as `crowded_marks` finds them; but for
/// those its tree's pane cuts at its edge, when it `scrolls` across.
fn crowded_research(scrolls: bool) -> Vec<String> {
    crowded_marks(imgui::RESEARCH_WINDOW)
        .into_iter()
        .filter(|crowded| !scrolls || !crowded.contains(" runs out of "))
        .collect()
}

#[test]
fn imgui_research_view_keeps_its_text_and_icons_apart_at_every_width() {
    let mut game = research_queued();
    let height = 720.0;
    let least = MIN_WINDOW_SIZE[0] as u32;
    let mut screen = ImGuiScreen::styled(Vec2::new(least as f32, height));
    for width in (least..=1600).step_by(40) {
        let width = width as f32;
        screen.size = Vec2::new(width, height);
        screen.context.io_mut().display_size = [width, height];
        screen.settle(&mut game);
        let crowded = crowded_research(width < WHOLE_TREE);
        assert!(crowded.is_empty(), "at {width}: {crowded:#?}");
        for target in [Target::ClearResearchQueue, Target::ResearchQueueRemove(0)] {
            assert!(screen.button(target).is_some(), "{target:?} at {width}");
        }
    }
}

/// #422: the research view hides every other panel, and leaves the
/// layout as it was, so all are back in their places as it closes: a
/// panel docked at the game window's edge, one in a box, the turn strip.
#[test]
fn closing_the_research_view_puts_every_panel_back_where_it_was() {
    let mut game = city_view();
    let mut screen = ImGuiScreen::new();
    screen.capture_selection(&mut game);
    game.leave_city_view();
    screen.dock_to_window_edge(&mut game, "Debug", "left");
    screen.settle(&mut game);
    let edge = screen.layout.window_edge("Debug");
    assert!(edge.is_some(), "Debug docked at the edge");
    let shown = || -> Vec<(String, Vec2, Vec2)> {
        let windows: std::collections::BTreeSet<String> = imgui::DRAWN_MARKS
            .with_borrow(|marks| marks.iter().map(|mark| mark.window.clone()).collect());
        windows
            .into_iter()
            .map(|title| {
                let (at, size, _) = imgui::native_geometry(&title).unwrap();
                (title, at, size)
            })
            .collect()
    };
    let before = shown();
    let titles: Vec<&str> = before.iter().map(|(title, ..)| title.as_str()).collect();
    for title in ["Debug", "Units"] {
        assert!(titles.contains(&title), "{title} in {titles:?}");
    }
    assert!(
        titles.iter().any(|title| title.contains("Pinned")),
        "the captured panel too: {titles:?}"
    );

    // Open (T), only it and the status bar show.
    game.toggle_research();
    screen.settle(&mut game);
    let open: Vec<String> = shown().into_iter().map(|(title, ..)| title).collect();
    assert_eq!(open, [imgui::RESEARCH_WINDOW, "Status"]);
    // Closed (Escape), each is where it was, as big.
    game.press_escape();
    screen.settle(&mut game);
    assert_eq!(shown(), before);
    assert_eq!(screen.layout.window_edge("Debug"), edge);
    assert_eq!(screen.layout.captured(), 1);

    // Opened by the city's Research button and closed by Space, the same.
    game.open_city(0);
    screen.settle(&mut game);
    game.activate_target(Target::OpenResearch);
    assert!(game.research_view());
    screen.settle(&mut game);
    let open: Vec<String> = shown().into_iter().map(|(title, ..)| title).collect();
    assert_eq!(open, [imgui::RESEARCH_WINDOW, "Status"]);
    assert!(game.exit_structure_menu());
    screen.settle(&mut game);
    assert_eq!(shown(), before);
    assert_eq!(screen.layout.window_edge("Debug"), edge);
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
    let target = Target::Building(Building::Mill);
    catalog_cursor(&mut game, target);
    let card = find_button(&game, target);
    assert_eq!(card.state(), ButtonState::Disabled);
    assert_eq!(card.hint, needs_mill);
    let tip = line_strings(game.tooltip_lines(&card).into_iter().map(|(_, l)| l));
    assert_eq!(tip.last(), Some(&needs_mill), "{tip:?}");
    // The Barracks' card too, and ImGui draws it off, so a click there
    // queues nothing.
    game.open_barracks(0);
    let mut screen = ImGuiScreen::new();
    screen.click(&mut game, Target::BarracksBuild(BuildUnit::Ranged));
    assert!(game.cities[0].barracks_queue.is_empty());
    drop(screen);
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
    // crossing a card; in more room, the columns further apart and
    // centered in it.
    let metrics = tree.classic_metrics();
    let least = tree.width(&metrics);
    for width in [least, least * 1.4, least * 3.0] {
        let placed = tree.place(width, &metrics);
        assert!(!placed.stacked);
        let rect = |i: usize| Rect {
            min: placed.cards[i].0,
            max: placed.cards[i].1,
        };
        for i in 0..tree.cards.len() {
            for j in i + 1..tree.cards.len() {
                assert!(!rect(i).overlaps(rect(j), 0.0), "{i} and {j} in {width}");
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
        let left = placed.cards.iter().map(|c| c.0.x).fold(f32::MAX, f32::min);
        let right = placed.cards.iter().map(|c| c.1.x).fold(0.0, f32::max);
        assert!((left - (width - right)).abs() <= 1.0, "centered in {width}");
        assert!(right - left <= width + 0.5);
        if width > least {
            assert!(right - left > least, "spread in {width}");
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

    // ImGui: the tree's columns side by side, its text and icons apart, a
    // card's tooltip, and a card's click queues it.
    let mut game = research_in_progress();
    let mut screen = ImGuiScreen::styled(SCREEN);
    screen.settle(&mut game);
    let at = |tech: Tech| screen_button(&screen, Target::Research(tech));
    assert!(at(techs[0]).x < at(techs[3]).x);
    assert!(at(techs[3]).x < at(techs[7]).x);
    let crowded = crowded_marks(imgui::RESEARCH_WINDOW);
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
    // In the least window the game allows, the columns stay side by side,
    // the tree's pane scrolling across to the last; its text and icons
    // still apart.
    drop(screen);
    let least = Vec2::from(MIN_WINDOW_SIZE);
    let mut screen = ImGuiScreen::styled(least);
    screen.settle(&mut game);
    let at = |screen: &ImGuiScreen, tech: Tech| screen.button(Target::Research(tech));
    assert!(at(&screen, techs[0]).unwrap().x < at(&screen, techs[3]).unwrap().x);
    let crowded = crowded_research(true);
    assert!(crowded.is_empty(), "{crowded:#?}");
    let card = at(&screen, techs[0]).unwrap();
    for _ in 0..3 {
        screen.context.io_mut().add_mouse_wheel_event([-5.0, 0.0]);
        screen.frame(&mut game, Some(card), false);
    }
    screen.frame(&mut game, None, false);
    let moved = at(&screen, techs[0]);
    assert!(
        moved.is_none_or(|moved| moved.x < card.x),
        "scrolled across"
    );
}

/// The middle of the button ImGui drew for `target` last frame.
fn screen_button(screen: &ImGuiScreen, target: Target) -> Vec2 {
    screen
        .button(target)
        .unwrap_or_else(|| panic!("ImGui shows no {target:?} button"))
}
