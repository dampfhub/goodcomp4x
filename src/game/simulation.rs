//! Whole-game simulation tests: both teams planned by the AI, every turn resolved through the
//! same path the game uses, and the board's invariants checked after each turn.
//!
//! Every game is seeded (`GameState::seed_rng`: damage rolls and the F4 world's map), so a
//! failure names its scenario, seed and turn, and replays exactly:
//! `SIM_SEED=<seed> cargo test simulation`. `SIM_SEEDS=<n>` plays seeds `0..n` instead of
//! `DEFAULT_SEEDS`, to hunt for failures.

use std::collections::{HashMap, HashSet};
use std::thread;

use super::city::{Build, CITY_MAX_HP, MAX_CITY_POPULATION};
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

/// Like `play_turn`, but the player's units are ordered through Shift-click queues: each unit
/// without a queue gets three turns of moves toward the nearest enemy and then an attack on
/// its hex, as a player would queue them. The AI still runs the player's cities and workers;
/// units already following a queue keep their orders.
fn play_queued_turn(game: &mut GameState) -> usize {
    game.selected = None;
    game.group.clear();
    let queued: HashMap<u32, (Option<Hex>, Option<Hex>)> = game
        .units
        .iter()
        .filter(|u| u.has_queue())
        .map(|u| (u.id, (u.planned_move, u.planned_attack)))
        .collect();
    game.plan_ai_turn(PLAYER_TEAM);
    for idx in 0..game.units.len() {
        let unit = &mut game.units[idx];
        if unit.team != PLAYER_TEAM {
            continue;
        }
        if let Some(&(planned_move, planned_attack)) = queued.get(&unit.id) {
            unit.planned_move = planned_move;
            unit.planned_attack = planned_attack;
            continue;
        }
        unit.planned_move = None;
        unit.planned_attack = None;
        let pos = unit.pos;
        let Some(target) = game
            .units
            .iter()
            .filter(|u| u.team != PLAYER_TEAM)
            .map(|u| u.pos)
            .min_by_key(|t| (t.distance(pos), t.q, t.r))
        else {
            continue;
        };
        game.selected = Some(idx);
        for _ in 0..3 {
            game.queue_move(target);
        }
        game.queue_attack(target);
    }
    game.selected = None;
    game.resolve_turn();
    game.update(0.0);
    game.units.iter().filter(|u| u.following_queue).count()
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
    for (idx, unit) in game.units.iter().enumerate() {
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
            game.can_enter(unit.pos),
            "{context}: {unit} stands on an impassable or off-map hex"
        );
        occupants.entry(unit.pos).or_default().push(unit.team);
        // A queue is a chain: each queued turn starts where the one before it ends.
        let mut from = unit.planned_pos();
        for (turn, order) in unit.queued.iter().enumerate() {
            assert_eq!(
                order.from,
                from,
                "{context}: {unit}'s queued turn {} doesn't start where the last one ends",
                turn + 2
            );
            from = order.end_pos();
        }
        assert!(
            !unit.has_queue() || game.is_player_controlled(idx),
            "{context}: {unit} follows a queue but isn't the player's"
        );
    }
    for worker in &game.field_workers {
        assert!(
            ids.insert(worker.id),
            "{context}: worker id {} is also another's",
            worker.id
        );
        assert!(
            game.can_enter(worker.pos),
            "{context}: a {:?} worker stands where it can't at {:?}",
            worker.team,
            worker.pos
        );
        assert!(
            game.enemy_of_team_at(worker.pos, worker.team).is_none(),
            "{context}: a {:?} worker shares {:?} with an enemy and wasn't captured",
            worker.team,
            worker.pos
        );
        assert_eq!(
            game.cities[worker.home].team, worker.team,
            "{context}: a worker's home city is another side's"
        );
    }
    for hex in game.structures.keys() {
        assert!(
            game.cities.iter().all(|c| c.pos != *hex),
            "{context}: a structure stands on the city at {hex:?}"
        );
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
    for (i, city) in game.cities.iter().enumerate() {
        // A paid-for unit at the head of the queue is one waiting for an open hex, and the
        // city banks nothing more behind it (#54). A city earning a unit's cost in a turn
        // could have that much left over, so it isn't checked.
        let Some(&Build::Unit(unit)) = city.queue.first() else {
            continue;
        };
        if city.production < unit.cost() || game.income(i).1 >= unit.cost() {
            continue;
        }
        assert_eq!(
            city.production,
            unit.cost(),
            "{context}: city {} banked production behind a finished {}",
            city.id,
            unit.name()
        );
        assert!(
            city.pos
                .neighbors()
                .into_iter()
                .all(|hex| !game.grid.is_passable(hex) || game.is_occupied(hex)),
            "{context}: city {} holds a finished {} beside an open hex",
            city.id,
            unit.name()
        );
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
fn queued_orders_against_the_ai_keep_the_board_consistent() {
    for_every_game(&Scenario::ALL, &seeds(), |scenario, seed| {
        let mut game = start(scenario, seed);
        let name = format!("{} seed {seed}", scenario.name());
        let mut followed = 0;
        for turn in 1..=TURNS {
            followed += play_queued_turn(&mut game);
            let context = format!("{name} queued turn {turn}");
            assert!(!game.is_resolving(), "{context}: the turn did not finish");
            check_invariants(&game, &context);
        }
        // Anti-vacuity: queues were actually carried from turn to turn.
        let has_units = scenario != Scenario::World;
        assert!(
            followed > 0 || !has_units,
            "{name}: no unit ever followed a queued turn"
        );
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
