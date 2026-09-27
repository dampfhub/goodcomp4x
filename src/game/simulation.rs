//! Whole-game simulation tests: both teams planned by the AI, every turn resolved through the
//! same path the game uses, and the board's invariants checked after each turn.
//!
//! Every game is seeded (`GameState::seed_rng`: damage rolls and the F4 world's map), so a
//! failure names its scenario, seed and turn, and replays exactly:
//! `SIM_SEED=<seed> cargo test simulation`. `SIM_SEEDS=<n>` plays seeds `0..n` instead of
//! `DEFAULT_SEEDS`, to hunt for failures.

use std::collections::{HashMap, HashSet};
use std::thread;

use super::city::{CITY_MAX_HP, CORE_HP, MAX_CITY_POPULATION};
use super::hex::Hex;
use super::scenario::Scenario;
use super::unit::Team;
use super::{AI_TEAM, GameState, PLAYER_TEAM};

const TURNS: u32 = 40;

/// The seeds every `cargo test` plays, unless `SIM_SEED` or `SIM_SEEDS` is set.
const DEFAULT_SEEDS: [u64; 3] = [1, 2, 3];

/// The seeds to play: `SIM_SEED` alone, seeds `0..SIM_SEEDS`, or `DEFAULT_SEEDS`.
fn seeds() -> Vec<u64> {
    if let Some(seed) = env_number("SIM_SEED") {
        vec![seed]
    } else if let Some(count) = env_number("SIM_SEEDS") {
        (0..count).collect()
    } else {
        DEFAULT_SEEDS.to_vec()
    }
}

fn env_number(name: &str) -> Option<u64> {
    let value = std::env::var(name).ok()?;
    let number = value.trim().parse();
    Some(number.unwrap_or_else(|_| panic!("{name}={value:?} is not a whole number")))
}

/// A fresh game of `scenario` with its RNG seeded, playing every step at once.
fn start(scenario: Scenario, seed: u64) -> GameState {
    let mut game = GameState::new();
    game.instant_playback = true;
    game.seed_rng(seed);
    game.switch_scenario(scenario);
    game
}

/// Plans both teams with the AI and resolves one turn, all steps at once.
fn play_turn(game: &mut GameState) {
    game.selected = None;
    game.group.clear();
    game.plan_ai_turn(PLAYER_TEAM);
    // `resolve_turn` plans the AI team itself.
    game.resolve_turn();
    game.update(0.0);
}

/// Prints how to replay a game if it panics, whether from a failed check or inside the game.
struct ReplayHint {
    scenario: Scenario,
    seed: u64,
}

impl Drop for ReplayHint {
    fn drop(&mut self) {
        if thread::panicking() {
            eprintln!(
                "{} with seed {} failed; replay it with SIM_SEED={} cargo test simulation",
                self.scenario.name(),
                self.seed,
                self.seed
            );
        }
    }
}

/// Runs `play` for every scenario and seed, each game on its own thread (a batch of as many
/// as there are cores at a time) so that more seeds cost little extra time. A panic in any
/// game fails the test once its batch has finished.
fn for_every_game(scenarios: &[Scenario], seeds: &[u64], play: impl Fn(Scenario, u64) + Sync) {
    let games: Vec<(Scenario, u64)> = scenarios
        .iter()
        .flat_map(|&scenario| seeds.iter().map(move |&seed| (scenario, seed)))
        .collect();
    let batch = thread::available_parallelism().map_or(8, |n| n.get());
    for games in games.chunks(batch) {
        thread::scope(|scope| {
            for &(scenario, seed) in games {
                let play = &play;
                thread::Builder::new()
                    .name(format!("{} seed {seed}", scenario.name()))
                    .spawn_scoped(scope, move || {
                        let _hint = ReplayHint { scenario, seed };
                        play(scenario, seed);
                    })
                    .expect("spawn a simulation thread");
            }
        });
    }
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
        assert!(
            (0.0..=CORE_HP).contains(&city.interior.core_hp),
            "{context}: city {} command post has {} HP",
            city.id,
            city.interior.core_hp
        );
        let mut sources = HashSet::new();
        let mut tiles = HashSet::new();
        for fighter in &city.interior.fighters {
            assert!(
                sources.insert(fighter.source_id),
                "{context}: repeated interior source"
            );
            assert!(
                tiles.insert(fighter.pos),
                "{context}: two fighters on one interior tile"
            );
            assert!(
                fighter.pos.distance(Hex::new(0, 0)) <= 2,
                "{context}: fighter outside interior"
            );
            assert!(
                fighter.hp > 0.0 && fighter.hp <= fighter.unit_type.stats().max_hp,
                "{context}: interior fighter has invalid HP"
            );
            assert!(
                game.units.iter().any(|unit| unit.id == fighter.source_id
                    && unit.team == fighter.team
                    && unit.pos.distance(city.pos) == 1),
                "{context}: interior fighter lacks adjacent source"
            );
        }
    }
}

#[test]
fn ai_against_ai_keeps_the_board_consistent_in_every_scenario() {
    for_every_game(&Scenario::ALL, &seeds(), |scenario, seed| {
        let mut game = start(scenario, seed);
        let name = format!("{} seed {seed}", scenario.name());
        check_invariants(&game, &format!("{name} at start"));
        for turn in 1..=TURNS {
            play_turn(&mut game);
            let context = format!("{name} turn {turn}");
            assert!(
                !game.is_resolving(),
                "{context}: the turn did not finish resolving"
            );
            assert_eq!(game.turn, turn, "{context}: turn counter");
            check_invariants(&game, &context);
        }
    });
}

#[test]
fn ai_against_ai_combat_ends_with_fewer_units() {
    // Anti-vacuity: the combat scenario must actually fight, or the invariant test above
    // would pass on a game where nothing happens.
    for_every_game(&[Scenario::Combat], &seeds(), |scenario, seed| {
        let mut game = start(scenario, seed);
        let at_start = game.units.len();
        for _ in 0..TURNS {
            play_turn(&mut game);
        }
        assert!(
            game.units.len() < at_start,
            "seed {seed}: no unit died in {TURNS} turns of AI-vs-AI combat ({at_start} at start)"
        );
        assert!(
            game.units
                .iter()
                .all(|u| u.team == AI_TEAM || u.team == PLAYER_TEAM),
            "only the two teams exist"
        );
    });
}

/// What a replay must reproduce: the map, every unit and every city.
#[derive(PartialEq, Debug)]
struct Fingerprint {
    turn: u32,
    map_seed: Option<u32>,
    units: Vec<(u32, Hex, f32)>,
    cities: Vec<(u32, f32, usize)>,
}

fn fingerprint(scenario: Scenario, seed: u64, turns: u32) -> Fingerprint {
    let mut game = start(scenario, seed);
    for _ in 0..turns {
        play_turn(&mut game);
    }
    Fingerprint {
        turn: game.turn,
        map_seed: game.map_seed,
        units: game.units.iter().map(|u| (u.id, u.pos, u.hp)).collect(),
        cities: game
            .cities
            .iter()
            .map(|c| (c.id, c.hp, c.population))
            .collect(),
    }
}

#[test]
fn the_same_seed_replays_the_same_game() {
    const REPLAY_TURNS: u32 = 20;
    // One seed is enough to catch hidden nondeterminism (such as hash-map iteration order
    // deciding an outcome): each map below has its own random hasher keys.
    for_every_game(&Scenario::ALL, &seeds()[..1], |scenario, seed| {
        assert_eq!(
            fingerprint(scenario, seed, REPLAY_TURNS),
            fingerprint(scenario, seed, REPLAY_TURNS),
            "{} seed {seed} played out differently the second time",
            scenario.name()
        );
    });
    // Anti-vacuity: the seed does drive the rolls. The armies meet on turn 2 and are gone
    // within a few more, so compare right after the first clash.
    assert_ne!(
        fingerprint(Scenario::Combat, 1, 2),
        fingerprint(Scenario::Combat, 2, 2),
        "two seeds played the same combat"
    );
}
