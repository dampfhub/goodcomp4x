//! Whole-game simulation tests: both teams planned by the AI, every turn resolved through the
//! same path the game uses, and the board's invariants checked after each turn. Damage rolls
//! use the thread RNG, so a failure message names the scenario and turn to reproduce from.

use std::collections::{HashMap, HashSet};

use super::city::{CITY_MAX_HP, MAX_CITY_POPULATION};
use super::scenario::Scenario;
use super::unit::Team;
use super::{AI_TEAM, GameState, PLAYER_TEAM};

const TURNS: u32 = 40;

/// Plans both teams with the AI and resolves one turn, all steps at once.
fn play_turn(game: &mut GameState) {
    game.selected = None;
    game.group.clear();
    game.plan_ai_turn(PLAYER_TEAM);
    // `resolve_turn` plans the AI team itself.
    game.resolve_turn();
    game.update(0.0);
}

fn check_invariants(game: &GameState, context: &str) {
    let mut ids = HashSet::new();
    let mut occupants: HashMap<_, Vec<Team>> = HashMap::new();
    for unit in &game.units {
        assert!(
            ids.insert(unit.id),
            "{context}: unit id {} appears twice",
            unit.id
        );
        assert!(
            unit.hp > 0.0 && unit.hp <= unit.max_hp(),
            "{context}: {unit} has {} of {} HP",
            unit.hp,
            unit.max_hp()
        );
        assert!(
            game.grid.is_passable(unit.pos),
            "{context}: {unit} stands on an impassable or off-map hex"
        );
        occupants.entry(unit.pos).or_default().push(unit.team);
    }
    for (hex, teams) in occupants {
        // One unit per hex, except a contest: exactly two enemies.
        let contested = teams.len() == 2 && teams[0] != teams[1];
        assert!(
            teams.len() == 1 || contested,
            "{context}: {hex:?} holds {teams:?}"
        );
    }
    for city in &game.cities {
        assert!(
            (0.0..=CITY_MAX_HP).contains(&city.hp),
            "{context}: city {} has {} HP",
            city.id,
            city.hp
        );
        assert!(
            (1..=MAX_CITY_POPULATION).contains(&city.population),
            "{context}: city {} has population {}",
            city.id,
            city.population
        );
        assert!(
            city.worked.len() <= city.population,
            "{context}: city {} works {} tiles with population {}",
            city.id,
            city.worked.len(),
            city.population
        );
    }
}

#[test]
fn ai_against_ai_keeps_the_board_consistent_in_every_scenario() {
    for scenario in Scenario::ALL {
        let mut game = GameState::new();
        game.switch_scenario(scenario);
        game.instant_playback = true;
        check_invariants(&game, &format!("{} at start", scenario.name()));
        for turn in 1..=TURNS {
            play_turn(&mut game);
            let context = format!("{} turn {turn}", scenario.name());
            assert!(
                !game.is_resolving(),
                "{context}: the turn did not finish resolving"
            );
            assert_eq!(game.turn, turn, "{context}: turn counter");
            check_invariants(&game, &context);
        }
    }
}

#[test]
fn ai_against_ai_combat_ends_with_fewer_units() {
    // Anti-vacuity: the combat scenario must actually fight, or the invariant test above
    // would pass on a game where nothing happens.
    let mut game = GameState::new();
    game.instant_playback = true;
    let start = game.units.len();
    for _ in 0..TURNS {
        play_turn(&mut game);
    }
    assert!(
        game.units.len() < start,
        "no unit died in {TURNS} turns of AI-vs-AI combat ({start} units at start)"
    );
    assert!(
        game.units
            .iter()
            .all(|u| u.team == AI_TEAM || u.team == PLAYER_TEAM),
        "only the two teams exist"
    );
}
