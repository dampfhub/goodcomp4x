//! Whole ImGui frames (#123): `draw_imgui` run headless (`ImGuiScreen`) over
//! every scenario and kind of selection, which the layout tests beside it,
//! built on hand-made states, never reach.

use super::*;
use crate::game::city::Building;
use crate::game::scenario::Scenario;
use crate::game::workers::JobKind;

/// The kinds of selection `selections` makes, each of which some scenario
/// must have.
const KINDS: [&str; 6] = [
    "unit",
    "group",
    "city",
    "barracks",
    "interior",
    "placing a job",
];

/// `game` with each kind of selection it has (`KINDS`), named: the player's
/// first unit, all of them as a group, and its first city, that city's
/// Barracks and interior, and the city placing a job for its workers (the
/// worker menu).
fn selections(game: &GameState) -> Vec<(&'static str, GameState)> {
    let mut views = Vec::new();
    let mut view = |name, open: &dyn Fn(&mut GameState)| {
        let mut view = game.clone();
        open(&mut view);
        views.push((name, view));
    };
    let units: Vec<usize> = (0..game.units.len())
        .filter(|&i| game.is_player_controlled(i))
        .collect();
    if let Some(&unit) = units.first() {
        view("unit", &|v| v.set_selection(vec![unit]));
    }
    if units.len() > 1 {
        view("group", &|v| v.set_selection(units.clone()));
    }
    let player_cities =
        || (0..game.cities.len()).filter(|&i| game.cities[i].team == game.local_team);
    if let Some(city) = player_cities().next() {
        // No scenario starts with a Barracks: one built next to the city.
        let site = game.cities[city]
            .pos
            .neighbors()
            .into_iter()
            .find(|&hex| game.site_available(city, Building::Barracks, hex));
        if let Some(site) = site {
            view("barracks", &|v| {
                v.cities[city].barracks = Some(site);
                v.open_barracks(city);
            });
        }
        view("city", &|v| v.open_city(city));
        view("interior", &|v| v.open_city_interior(city));
        view("placing a job", &|v| {
            v.open_city(city);
            for kind in [JobKind::Road, JobKind::Improve, JobKind::CutForest] {
                v.arm_worker_job(kind);
                if v.placing_job.is_some() {
                    break;
                }
            }
        });
    }
    views
}

#[test]
fn every_selection_in_every_scenario_draws_its_panel() {
    let mut seen = std::collections::BTreeSet::new();
    for scenario in Scenario::ALL {
        let mut game = GameState::new();
        game.switch_scenario(scenario);
        for (name, mut game) in selections(&game) {
            if name == "placing a job" && game.placing_job.is_none() {
                continue;
            }
            seen.insert(name);
            // A context of its own, so the panel is this selection's.
            let mut screen = ImGuiScreen::new();
            for _ in 0..3 {
                screen.frame(&mut game, None, false);
            }
            let selection = native_state(slot_titles()[SELECTION]);
            assert!(
                selection.is_some_and(|window| window.size.x > 0.0 && window.size.y > 0.0),
                "{scenario:?}, {name}: no Selection window"
            );
        }
    }
    let missing: Vec<_> = KINDS.iter().filter(|kind| !seen.contains(*kind)).collect();
    assert!(missing.is_empty(), "no scenario has {missing:?}");
}
