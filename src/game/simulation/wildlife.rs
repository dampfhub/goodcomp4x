//! `animal_report`: not a check but a report to read. It plays AI-vs-AI worlds (the F4
//! scenario, 4 to 6 AI sides picked by the map's seed, every side the AI's) and measures the
//! animals (`animals.rs`): how many there are on the map turn by turn, what the sides lose to
//! them (scouts, settlers, troops and workers), early and later, and how many the sides kill
//! and how many dens they clear.
//!
//! Run it with `cargo test --release animal_report -- --ignored --nocapture`. Its knobs are
//! environment variables:
//!
//! - `SIM_SEED=<n>` / `SIM_SEEDS=<n>`: one seed, or seeds `0..n`, as for the other simulations
//!   (default: `DEFAULT_SEEDS`).
//! - `REPORT_TURNS=<n>`: turns per game (default `REPORT_TURNS`, 40, the simulations' length).
//! - `REPORT_ANIMALS=<n>`: the Animals setting (`Settings::world_animals`): 1 for few (the
//!   default), 2 for many.
//!
//! A loss is the animals' when it happens in an animal's attack step (only animals strike
//! then, and what they strike strikes back only at them), or in another attack step to a unit
//! that stood next to an animal as the step began with no unit of another side within 3 hexes
//! (it died of an animal's blow back), or to a worker in the workers' step with an animal
//! next to it.

use std::sync::Mutex;
use std::thread;

use super::super::GameState;
use super::super::PLAYER_TEAM;
use super::super::fast_hash::HashSet;
use super::super::hex::Hex;
use super::super::scenario::Scenario;
use super::super::turn::{Phase, Step};
use super::super::unit::{Team, UnitType};
use super::{env_number, seeds, start_with};

/// Turns per game unless `REPORT_TURNS` says otherwise.
const REPORT_TURNS: u32 = 40;
/// Turns the animal counts are shown for (those within the game's length).
const ROWS: [u32; 8] = [1, 5, 10, 15, 20, 30, 40, 60];
/// Losses up to this turn are early ones.
const EARLY: u32 = 20;

/// What a side can lose to an animal.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Loss {
    Scout,
    Settler,
    Troop,
    Worker,
}

impl Loss {
    const ALL: [Loss; 4] = [Loss::Scout, Loss::Settler, Loss::Troop, Loss::Worker];

    fn name(self) -> &'static str {
        match self {
            Loss::Scout => "scouts",
            Loss::Settler => "settlers",
            Loss::Troop => "troops",
            Loss::Worker => "workers",
        }
    }
}

/// One game's numbers.
#[derive(Default)]
struct Game {
    sides: usize,
    dens_at_start: usize,
    /// Animals alive at the end of each turn of `ROWS` (0 before it).
    alive: Vec<(u32, usize)>,
    /// The most alive at once.
    most_alive: usize,
    /// Losses to animals: (turn, what).
    losses: Vec<(u32, Loss)>,
    killed: usize,
    /// Cities, and troops (scouts and settlers aside), of every side at the end.
    cities: usize,
    troops: usize,
    cleared: usize,
}

/// What stood where as a step began.
struct Before {
    /// Every unit of a side: (id, team, what it would count as, where).
    units: Vec<(u32, Team, Loss, Hex)>,
    workers: Vec<(u32, Hex)>,
    animals: Vec<(u32, Hex)>,
}

fn before(game: &GameState) -> Before {
    let units = game
        .units
        .iter()
        .filter(|u| u.team.is_side())
        .map(|u| {
            let loss = if game.settlers.contains(&u.id) {
                Loss::Settler
            } else if u.unit_type == UnitType::Scout {
                Loss::Scout
            } else {
                Loss::Troop
            };
            (u.id, u.team, loss, u.pos)
        })
        .collect();
    let workers = game.field_workers.iter().map(|w| (w.id, w.pos)).collect();
    let animals = game
        .units
        .iter()
        .filter(|u| u.is_animal())
        .map(|u| (u.id, u.pos))
        .collect();
    Before {
        units,
        workers,
        animals,
    }
}

/// Every unit id on the board or aboard a ship.
fn unit_ids(game: &GameState) -> HashSet<u32> {
    game.units
        .iter()
        .flat_map(|u| std::iter::once(u.id).chain(u.cargo.iter().map(|c| c.id)))
        .collect()
}

/// The losses to animals in the step `step` that took the board from `was` to `game`.
fn losses(game: &GameState, was: &Before, step: Step) -> Vec<Loss> {
    let ids = unit_ids(game);
    let workers: HashSet<u32> = game.field_workers.iter().map(|w| w.id).collect();
    let near_animal = |hex: Hex| was.animals.iter().any(|&(_, at)| at.distance(hex) <= 1);
    let mut out = Vec::new();
    match step {
        Step::Units(kind, Phase::Attack) => {
            for &(id, team, loss, pos) in &was.units {
                if ids.contains(&id) {
                    continue;
                }
                let rival_near = was
                    .units
                    .iter()
                    .any(|&(_, t, _, at)| t != team && at.distance(pos) <= 3);
                if kind.is_animal() || (near_animal(pos) && !rival_near) {
                    out.push(loss);
                }
            }
            if kind.is_animal() {
                out.extend(
                    was.workers
                        .iter()
                        .filter(|(id, _)| !workers.contains(id))
                        .map(|_| Loss::Worker),
                );
            }
        }
        Step::Workers => out.extend(
            was.workers
                .iter()
                .filter(|&&(id, pos)| !workers.contains(&id) && near_animal(pos))
                .map(|_| Loss::Worker),
        ),
        Step::Units(_, Phase::Move) => {}
    }
    out
}

fn play(seed: u64, animals: usize, turns: u32) -> Game {
    let mut game = start_with(Scenario::World, seed, |s| s.world_animals = animals);
    let mut out = Game {
        sides: Team::ALL
            .into_iter()
            .filter(|&t| game.cities.iter().any(|c| c.team == t))
            .count(),
        dens_at_start: game.dens.len(),
        ..Game::default()
    };
    let animal_ids = |game: &GameState| -> Vec<u32> {
        game.units
            .iter()
            .filter(|u| u.is_animal())
            .map(|u| u.id)
            .collect()
    };
    let mut ever: HashSet<u32> = animal_ids(&game).into_iter().collect();
    for turn in 1..=turns {
        game.selected = None;
        game.group.clear();
        game.plan_ai_turn(PLAYER_TEAM);
        game.resolve_turn();
        // Each step by hand, the workers' last through `update`, which also ends the turn.
        let steps: Vec<Step> = game.pending_steps.drain(..).collect();
        for step in steps {
            let was = before(&game);
            match step {
                Step::Units(kind, phase) => {
                    game.resolve_step(kind, phase);
                }
                Step::Workers => {
                    game.pending_steps.push_back(step);
                    game.update(0.0);
                }
            }
            out.losses
                .extend(losses(&game, &was, step).into_iter().map(|l| (turn, l)));
        }
        assert!(
            !game.is_resolving(),
            "seed {seed} turn {turn} still resolving"
        );
        let alive = animal_ids(&game);
        ever.extend(alive.iter().copied());
        out.most_alive = out.most_alive.max(alive.len());
        if ROWS.contains(&turn) {
            out.alive.push((turn, alive.len()));
        }
    }
    let alive: HashSet<u32> = animal_ids(&game).into_iter().collect();
    out.killed = ever.iter().filter(|id| !alive.contains(id)).count();
    out.cleared = out.dens_at_start - game.dens.len();
    out.cities = game.cities.len();
    out.troops = game
        .units
        .iter()
        .filter(|u| {
            u.team.is_side() && u.unit_type != UnitType::Scout && !game.settlers.contains(&u.id)
        })
        .count();
    out
}

#[test]
#[ignore = "a report to read, not a check: run with --release -- --ignored --nocapture"]
fn animal_report() {
    let animals = env_number("REPORT_ANIMALS").map_or(1, |n| n as usize);
    let turns = env_number("REPORT_TURNS").map_or(REPORT_TURNS, |n| n as u32);
    let seeds = seeds();
    let games = Mutex::new(Vec::new());
    let batch = thread::available_parallelism().map_or(8, |n| n.get());
    for chunk in seeds.chunks(batch) {
        thread::scope(|scope| {
            for &seed in chunk {
                let games = &games;
                scope.spawn(move || {
                    let game = play(seed, animals, turns);
                    games.lock().unwrap().push((seed, game));
                });
            }
        });
    }
    let mut games = games.into_inner().unwrap();
    games.sort_by_key(|(seed, _)| *seed);
    let n = games.len() as f32;
    let mean = |f: &dyn Fn(&Game) -> f32| games.iter().map(|(_, g)| f(g)).sum::<f32>() / n;
    println!(
        "animal report: {} worlds (seeds {:?}), Animals {animals}, {turns} turns",
        games.len(),
        games.iter().map(|(s, _)| *s).collect::<Vec<_>>()
    );
    println!(
        "  per world: {:.1} sides, {:.1} dens at start",
        mean(&|g| g.sides as f32),
        mean(&|g| g.dens_at_start as f32)
    );
    print!("  animals alive, mean per world at turn:");
    for &row in ROWS.iter().filter(|&&r| r <= turns) {
        let at = |g: &Game| {
            g.alive
                .iter()
                .find(|(t, _)| *t == row)
                .map_or(0.0, |(_, a)| *a as f32)
        };
        print!("  {row}: {:.1}", mean(&at));
    }
    println!();
    println!(
        "  most alive at once, mean per world: {:.1}",
        mean(&|g| g.most_alive as f32)
    );
    for (label, range) in [
        (format!("turns 1-{EARLY}"), 1..=EARLY),
        (format!("turns {}-{turns}", EARLY + 1), EARLY + 1..=turns),
    ] {
        print!("  lost to animals, {label}, total (mean per world):");
        for loss in Loss::ALL {
            let count = games
                .iter()
                .flat_map(|(_, g)| &g.losses)
                .filter(|(t, l)| range.contains(t) && *l == loss)
                .count();
            print!("  {} {count} ({:.2})", loss.name(), count as f32 / n);
        }
        println!();
    }
    println!(
        "  animals killed by sides: {} ({:.1} per world); dens cleared: {} ({:.1} per world)",
        games.iter().map(|(_, g)| g.killed).sum::<usize>(),
        mean(&|g| g.killed as f32),
        games.iter().map(|(_, g)| g.cleared).sum::<usize>(),
        mean(&|g| g.cleared as f32),
    );
    println!(
        "  at the end, mean per world: {:.1} cities, {:.1} troops",
        mean(&|g| g.cities as f32),
        mean(&|g| g.troops as f32),
    );
}
