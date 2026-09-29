//! `economy_report`: not a check but a report to read. It plays AI-vs-AI games of the Cities
//! and World scenarios and measures, turn by turn, how the stockpile economy runs
//! (`docs/rts-economy.md`, Round 6): what each side trains, by type and by turn; how its
//! cities grow and when each growth step lands; what piles up in the stockpile and what runs
//! short; when sides first meet and first fight, and how many units die a turn after; how long
//! each build takes in practice (queued, paid, worked, out); where the resources go; and how far
//! apart the sides end up.
//!
//! Run it with `cargo test --release economy_report -- --ignored --nocapture`. Its knobs are
//! environment variables:
//!
//! - `SIM_SEED=<n>` / `SIM_SEEDS=<n>`: one seed, or seeds `0..n`, as for the other simulations
//!   (default: `DEFAULT_SEEDS`).
//! - `REPORT_TURNS=<n>`: turns per game (default `REPORT_TURNS`, 60).
//! - `REPORT_GAMES=<list>`: which games, comma-separated (default `DEFAULT_GAMES`): `cities`
//!   (the F2 scenario, two sides), `world` (the F4 world with 4 to 6 AI sides, picked by the
//!   map's seed, as a new game picks them) or `world<n>` (the world with `n` AI sides, 1 to 6).
//!   Every side is the AI's, the player's included, so a `world<n>` game has `n + 1` sides.
//! - `REPORT_JSON=<path>`: also writes the numbers to that file as JSON, for charts.
//! - `REPORT_ARMY_FIRST=1`: after the AI plans, a city queue that would only gather trains a
//!   Melee instead when its side can pay for one (`spend_on_troops`): a side spending on troops
//!   what the AI banks, for how fast an army can come.
//! - `REPORT_GAME_LINES=1`: also prints each game, a line per side every 5 turns (the
//!   report of the earlier rounds).
//! - `SIM_SPEEDUP=1` and `SIM_LIFETIME_CAP=1`: production speeding builds, and the lifetime
//!   Cavalry and Armored cap, as for the other simulations.
//!
//! Resources are shown in whole units (the game keeps quarters). "Army" is every unit but
//! scouts and settlers; "trained" counts the troops the queues turned out (ruins' Cavalry
//! recruits are counted apart). Per-side numbers are over every side of every game, the
//! eliminated ones included (as zeros).

use std::fmt::Write as _;
use std::thread;

use super::super::GameState;
use super::super::city::{
    Build, BuildUnit, CLUSTER_SIZE, Lane, MAX_CITY_POPULATION, MAX_MANAGERS, Stock,
};
use super::super::fast_hash::{HashMap, HashSet};
use super::super::hex::Hex;
use super::super::unit::{Team, Unit, UnitType};
use super::super::workers::JobKind;
use super::super::{PLAYER_TEAM, scenario::Scenario};
use super::{env_number, seeds, start_with};

/// Turns per game unless `REPORT_TURNS` says otherwise.
const REPORT_TURNS: u32 = 60;
/// The games played unless `REPORT_GAMES` says otherwise.
const DEFAULT_GAMES: &str = "cities,world1,world4,world5,world6";
/// Turns the tables show a row for (those within the game's length).
const ROWS: [u32; 10] = [1, 5, 10, 15, 20, 30, 40, 50, 60, 80];
/// A side has met another once a unit or city of each stands this close.
const CONTACT_RANGE: i32 = 3;
/// The army sizes whose first turn is an event.
const ARMY_STEPS: [usize; 4] = [3, 5, 10, 15];
/// Kinds of unit a side gains, for "trained by type".
const KINDS: [&str; 7] = [
    "melee", "ranged", "cavalry", "siege", "armored", "ship", "recruit",
];
/// Where resources go: what paying for things spends them on.
const SPENDING: [&str; 5] = ["troops", "growth", "workers", "buildings", "works"];

/// One kind of game: a scenario and, for the world, how many AI sides.
#[derive(Clone, Copy)]
struct GameKind {
    scenario: Scenario,
    /// `Settings::world_ai`: `None` keeps the default, 4 to 6 by the map's seed.
    world_ai: Option<usize>,
}

impl GameKind {
    fn parse(name: &str) -> Self {
        let name = name.trim().to_ascii_lowercase();
        match name.as_str() {
            "cities" => Self {
                scenario: Scenario::Cities,
                world_ai: None,
            },
            "world" => Self {
                scenario: Scenario::World,
                world_ai: None,
            },
            _ => {
                let ai = name
                    .strip_prefix("world")
                    .and_then(|n| n.parse().ok())
                    .filter(|n| (1..Team::ALL.len()).contains(n))
                    .unwrap_or_else(|| {
                        panic!("REPORT_GAMES: {name:?} is not cities, world or world1-world6")
                    });
                Self {
                    scenario: Scenario::World,
                    world_ai: Some(ai),
                }
            }
        }
    }

    fn name(self) -> String {
        match (self.scenario, self.world_ai) {
            (Scenario::Cities, _) => "cities".into(),
            (_, None) => "world".into(),
            (_, Some(ai)) => format!("world{ai}"),
        }
    }

    fn start(self, seed: u64) -> GameState {
        start_with(self.scenario, seed, |settings| {
            if let Some(ai) = self.world_ai {
                settings.world_ai = ai;
            }
        })
    }
}

/// One side's state at the end of a turn.
#[derive(Clone, Copy, Default)]
struct SideTurn {
    /// Food, wood and metal in the stockpile.
    stock: [f64; 3],
    /// What its cities deliver a turn, before the citizens eat.
    income: [f64; 3],
    /// Food its citizens eat a turn.
    upkeep: f64,
    pop: f64,
    cities: f64,
    /// Units alive, scouts and settlers aside.
    army: f64,
    /// Troops its queues turned out so far.
    trained: f64,
    /// Units gained so far by `KINDS`.
    gained: [f64; KINDS.len()],
    /// Workers at home and out.
    workers: f64,
    /// Its units that died this turn.
    deaths: f64,
    /// What it paid for so far, by `SPENDING`, in food, wood and metal.
    spent: [[f64; 3]; SPENDING.len()],
    /// What Gather brought in so far.
    gathered: [f64; 3],
}

/// When things first happened to a side, by turn.
#[derive(Clone, Default)]
struct SideEvents {
    first_troop: Option<u32>,
    barracks_placed: Option<u32>,
    barracks_built: Option<u32>,
    /// The turn its first city reached each population (index), if it did.
    capital_pop: [Option<u32>; MAX_CITY_POPULATION + 1],
    /// The turn its army first reached each of `ARMY_STEPS`.
    army: [Option<u32>; ARMY_STEPS.len()],
    first_contact: Option<u32>,
    /// It attacked, or one of its units was hurt or killed.
    first_fight: Option<u32>,
    /// The same, with a troop of its (not a scout) the attacker or the one hurt.
    troop_fight: Option<u32>,
    first_loss: Option<u32>,
    second_city: Option<u32>,
    eliminated: Option<u32>,
}

/// What each of a side's queues did in the turns of a game.
#[derive(Clone, Default)]
struct LaneUse {
    /// City queue: turns working each build, by name.
    working: HashMap<&'static str, u32>,
    /// City queue: turns gathering at the population cap (nothing else to buy) and below it.
    gather_at_cap: u32,
    gather_poor: u32,
    /// Turns the head item waited unpaid.
    waiting: u32,
    empty: u32,
    /// Barracks: turns training, and turns idle with the resources a Melee lacked.
    training: u32,
    idle: u32,
    idle_short: [u32; 3],
    /// Turns each queue (city, Barracks) held a finished unit for want of an open hex beside it.
    held: [u32; 2],
}

struct SideLog {
    team: Team,
    turns: Vec<SideTurn>,
    events: SideEvents,
    lanes: LaneUse,
}

/// One item's way through a queue, by turn: queued, paid (work started), all its work done,
/// and out (a unit waits for an open hex).
struct BuildRecord {
    name: String,
    price: Stock,
    queued: u32,
    paid: u32,
    done: u32,
    left: u32,
}

/// A worker job's way: placed (and paid), then done.
struct JobRecord {
    name: &'static str,
    placed: u32,
    done: u32,
}

struct GameLog {
    sides: Vec<SideLog>,
    first_contact: Option<u32>,
    first_fight: Option<u32>,
    troop_fight: Option<u32>,
    /// Units that died each turn, all sides.
    deaths: Vec<f64>,
    captures: u32,
    builds: Vec<BuildRecord>,
    jobs: Vec<JobRecord>,
    /// Unpaid items the AI took off because they waited.
    abandoned: u32,
}

/// A queue's first item being followed.
struct Tracked {
    build: Build,
    team: Team,
    price: Stock,
    queued: u32,
    paid: Option<u32>,
    done: Option<u32>,
}

type JobKey = (Team, Hex, Option<Hex>, JobKind);
/// One number of a side's turn, to tabulate.
type Measure<'a> = &'a dyn Fn(&SideTurn) -> f64;
/// Named percentages, in the order to show them.
type Shares = Vec<(String, f64)>;

/// Watches one game as it is played: after the AI plans each turn and after the turn resolves.
struct Observer {
    log: GameLog,
    /// Every unit alive at the last look (passengers too): its side, health and whether it is
    /// a troop (not a scout or settler).
    alive: HashMap<u32, (Team, f32, f32, bool)>,
    settlers: HashSet<u32>,
    /// Each side's first city, by index.
    capitals: HashMap<Team, usize>,
    city_teams: Vec<Team>,
    /// Each queue's first item, by city and lane (0 the city's, 1 the Barracks').
    heads: HashMap<(usize, usize), Tracked>,
    jobs: HashMap<JobKey, u32>,
}

fn whole(stock: Stock) -> [f64; 3] {
    [stock.food, stock.wood, stock.metal].map(|q| f64::from(q) / 4.0)
}

fn spending(build: Build) -> Option<usize> {
    match build {
        Build::Unit(_) => Some(0),
        Build::Grow => Some(1),
        Build::Worker => Some(2),
        Build::Gather => None,
    }
}

fn kind(unit: &Unit) -> usize {
    match unit.unit_type {
        UnitType::Melee => 0,
        UnitType::Ranged => 1,
        UnitType::Cavalry if unit.drawn_from.is_none() => 6,
        UnitType::Cavalry => 2,
        UnitType::Siege => 3,
        UnitType::Armored => 4,
        _ => 5,
    }
}

fn is_army(game: &GameState, unit: &Unit) -> bool {
    unit.unit_type != UnitType::Scout && !game.settlers.contains(&unit.id)
}

fn first(event: &mut Option<u32>, turn: u32) {
    event.get_or_insert(turn);
}

fn lane_of(index: usize) -> Lane {
    if index == 0 {
        Lane::City
    } else {
        Lane::Barracks
    }
}

impl Observer {
    fn new(game: &GameState) -> Self {
        let teams: Vec<Team> = Team::ALL
            .into_iter()
            .filter(|&t| {
                game.units.iter().any(|u| u.team == t) || game.cities.iter().any(|c| c.team == t)
            })
            .collect();
        let mut observer = Self {
            log: GameLog {
                sides: teams
                    .iter()
                    .map(|&team| SideLog {
                        team,
                        turns: Vec::new(),
                        events: SideEvents::default(),
                        lanes: LaneUse::default(),
                    })
                    .collect(),
                first_contact: None,
                first_fight: None,
                troop_fight: None,
                deaths: Vec::new(),
                captures: 0,
                builds: Vec::new(),
                jobs: Vec::new(),
                abandoned: 0,
            },
            alive: HashMap::default(),
            settlers: game.settlers.clone(),
            capitals: HashMap::default(),
            city_teams: Vec::new(),
            heads: HashMap::default(),
            jobs: HashMap::default(),
        };
        observer.alive = Self::units(game);
        observer.city_teams = game.cities.iter().map(|c| c.team).collect();
        for (i, city) in game.cities.iter().enumerate() {
            observer.capitals.entry(city.team).or_insert(i);
        }
        observer.jobs = Self::current_jobs(game)
            .into_iter()
            .map(|key| (key, 0))
            .collect();
        observer
    }

    fn side(&mut self, team: Team) -> Option<&mut SideLog> {
        self.log.sides.iter_mut().find(|s| s.team == team)
    }

    fn units(game: &GameState) -> HashMap<u32, (Team, f32, f32, bool)> {
        game.units
            .iter()
            .flat_map(|u| std::iter::once(u).chain(&u.cargo))
            .map(|u| (u.id, (u.team, u.hp, u.interior_hp, is_army(game, u))))
            .collect()
    }

    fn current_jobs(game: &GameState) -> Vec<JobKey> {
        let listed = game.cities.iter().flat_map(|c| {
            c.worker_jobs
                .iter()
                .map(move |j| (c.team, j.hex, j.across, j.kind))
        });
        let out = game
            .field_workers
            .iter()
            .filter_map(|w| w.job.map(|j| (w.team, j.hex, j.across, j.kind)));
        listed.chain(out).collect()
    }

    /// `team` fought this turn; `troops` if a troop of its (not a scout) was in it.
    fn fought(&mut self, team: Team, troops: bool, turn: u32) {
        first(&mut self.log.first_fight, turn);
        if troops {
            first(&mut self.log.troop_fight, turn);
        }
        if let Some(side) = self.side(team) {
            first(&mut side.events.first_fight, turn);
            if troops {
                first(&mut side.events.troop_fight, turn);
            }
        }
    }

    fn spend(&mut self, team: Team, category: usize, price: Stock) {
        if let Some(side) = self.side(team) {
            let spent = &mut side.turns.last_mut().expect("a turn recorded").spent[category];
            for (total, amount) in spent.iter_mut().zip(whole(price)) {
                *total += amount;
            }
        }
    }

    /// After every side planned its turn, before it resolves: attacks planned, what the AI
    /// queued, took off or placed for its workers, and each idle Barracks.
    fn after_plan(&mut self, game: &GameState) {
        let turn = game.turn;
        // This turn's numbers start from the last turn's; the end of the turn fills them in.
        for side in &mut self.log.sides {
            let last = side.turns.last().copied().unwrap_or_default();
            side.turns.push(SideTurn {
                deaths: 0.0,
                ..last
            });
        }
        let mut fights = Vec::new();
        for unit in &game.units {
            let Some(target) = unit.planned_attack else {
                continue;
            };
            let enemy = game
                .units
                .iter()
                .find(|e| e.pos == target && e.team != unit.team)
                .map(|e| (e.team, is_army(game, e)))
                .or_else(|| {
                    game.field_workers
                        .iter()
                        .find(|w| w.pos == target && w.team != unit.team)
                        .map(|w| (w.team, false))
                });
            if let Some((enemy, enemy_troop)) = enemy {
                fights.extend([(unit.team, is_army(game, unit)), (enemy, enemy_troop)]);
            }
        }
        for (team, troops) in fights {
            self.fought(team, troops, turn);
        }
        for city in 0..game.cities.len() {
            for lane in 0..2 {
                let head = (game.lane_len(city, lane_of(lane)) > 0).then(|| {
                    let (paid, _, _) = game.lane_item(city, lane_of(lane), 0);
                    (self.build_at(game, city, lane), paid)
                });
                if let Some(tracked) = self.heads.get(&(city, lane))
                    && head.is_none_or(|(build, paid)| {
                        build != tracked.build || (tracked.paid.is_some() && !paid)
                    })
                {
                    if tracked.paid.is_none() {
                        self.log.abandoned += 1;
                    }
                    self.heads.remove(&(city, lane));
                }
                if let Some((build, paid)) = head {
                    let price = if paid {
                        Stock::default()
                    } else {
                        game.item_price(city, lane_of(lane), 0)
                    };
                    let team = game.cities[city].team;
                    let tracked = self.heads.entry((city, lane)).or_insert(Tracked {
                        build,
                        team,
                        price,
                        queued: turn,
                        paid: None,
                        done: None,
                    });
                    if !paid {
                        tracked.price = price;
                    }
                }
            }
        }
        // An idle Barracks: nothing queued after planning. What a Melee lacked of what the
        // side has left to spend this turn says what held it back.
        for team in Team::ALL {
            let spare = game.forecast(team).spare;
            let short = spare.shortfall(BuildUnit::Melee.price());
            let idle = game
                .cities
                .iter()
                .filter(|c| c.team == team && c.barracks.is_some())
                .map(|c| c.barracks_queue.is_empty())
                .collect::<Vec<_>>();
            let Some(side) = self.side(team) else {
                continue;
            };
            for is_idle in idle {
                if !is_idle {
                    side.lanes.training += 1;
                    continue;
                }
                side.lanes.idle += 1;
                for (count, lacking) in side.lanes.idle_short.iter_mut().zip(whole(short)) {
                    *count += u32::from(lacking > 0.0);
                }
            }
        }
        for key in Self::current_jobs(game) {
            if let std::collections::hash_map::Entry::Vacant(placed) = self.jobs.entry(key) {
                placed.insert(turn);
                let category = if matches!(key.3, JobKind::Build(_)) {
                    3
                } else {
                    4
                };
                self.spend(key.0, category, key.3.price());
                if key.3 == JobKind::Build(super::super::city::Building::Barracks)
                    && let Some(side) = self.side(key.0)
                {
                    first(&mut side.events.barracks_placed, turn);
                }
            }
        }
    }

    /// The build of the first item of one of `city`'s queues.
    fn build_at(&self, game: &GameState, city: usize, lane: usize) -> Build {
        let c = &game.cities[city];
        if lane == 0 {
            c.queue[0].build
        } else {
            Build::Unit(c.barracks_queue[0].build)
        }
    }

    /// After the turn resolved: units gained, lost and hurt, cities taken and grown, what the
    /// queues paid for and finished, jobs done, contact, and each side's numbers.
    fn after_turn(&mut self, game: &GameState) {
        let turn = game.turn;
        let now = Self::units(game);
        let mut deaths = 0.0;
        for (id, &(team, hp, interior, troop)) in &now {
            if let Some(&(_, before, before_interior, _)) = self.alive.get(id) {
                if hp < before - 1e-3 || interior < before_interior - 1e-3 {
                    self.fought(team, troop, turn);
                }
                continue;
            }
            if self.settlers.contains(id) {
                continue;
            }
            let Some(unit) = game.units.iter().find(|u| u.id == *id) else {
                continue;
            };
            let kind = kind(unit);
            let Some(side) = self.side(team) else {
                continue;
            };
            let current = side.turns.last_mut().expect("a turn recorded");
            current.gained[kind] += 1.0;
            if kind != 6 && is_army(game, unit) {
                current.trained += 1.0;
                first(&mut side.events.first_troop, turn);
            }
        }
        let lost: Vec<(Team, bool)> = self
            .alive
            .iter()
            .filter(|(id, _)| !now.contains_key(id) && !self.settlers.contains(id))
            .map(|(_, &(team, _, _, troop))| (team, troop))
            .collect();
        for (team, troop) in lost {
            deaths += 1.0;
            self.fought(team, troop, turn);
            if let Some(side) = self.side(team) {
                side.turns.last_mut().expect("a turn recorded").deaths += 1.0;
                first(&mut side.events.first_loss, turn);
            }
        }
        self.log.deaths.push(deaths);
        self.alive = now;

        for (i, city) in game.cities.iter().enumerate() {
            match self.city_teams.get(i) {
                Some(&before) if before != city.team => {
                    self.log.captures += 1;
                    self.heads.remove(&(i, 0));
                    self.heads.remove(&(i, 1));
                }
                None => {
                    self.capitals.entry(city.team).or_insert(i);
                }
                _ => {}
            }
        }
        self.city_teams = game.cities.iter().map(|c| c.team).collect();

        for city in 0..game.cities.len() {
            for lane in 0..2 {
                self.follow_queue(game, city, lane, turn);
            }
        }

        let current: HashSet<JobKey> = Self::current_jobs(game).into_iter().collect();
        let finished: Vec<(JobKey, u32)> = self
            .jobs
            .iter()
            .filter(|(key, _)| !current.contains(key))
            .map(|(&key, &placed)| (key, placed))
            .collect();
        for (key, placed) in finished {
            self.jobs.remove(&key);
            self.log.jobs.push(JobRecord {
                name: key.3.name(),
                placed,
                done: turn,
            });
        }

        self.contact(game, turn);

        for side in &mut self.log.sides {
            let team = side.team;
            let cities: Vec<usize> = (0..game.cities.len())
                .filter(|&i| game.cities[i].team == team)
                .collect();
            let army = game
                .units
                .iter()
                .filter(|u| u.team == team && is_army(game, u))
                .count();
            let now = side.turns.last_mut().expect("a turn recorded");
            now.stock = whole(game.stock(team));
            now.income = whole(game.side_income(team));
            now.upkeep = f64::from(game.upkeep(team)) / 4.0;
            now.pop = cities
                .iter()
                .map(|&i| game.cities[i].population)
                .sum::<usize>() as f64;
            now.cities = cities.len() as f64;
            now.army = army as f64;
            now.workers = cities
                .iter()
                .map(|&i| game.cities[i].workers as usize + game.workers_out(i))
                .sum::<usize>() as f64;
            let events = &mut side.events;
            if cities.iter().any(|&i| game.cities[i].barracks.is_some()) {
                first(&mut events.barracks_built, turn);
            }
            if cities.len() >= 2 {
                first(&mut events.second_city, turn);
            }
            if let Some(&capital) = self.capitals.get(&team)
                && game.cities[capital].team == team
            {
                let pop = game.cities[capital].population;
                for reached in &mut events.capital_pop[..=pop.min(MAX_CITY_POPULATION)] {
                    first(reached, turn);
                }
            }
            for (step, reached) in ARMY_STEPS.iter().zip(&mut events.army) {
                if army >= *step {
                    first(reached, turn);
                }
            }
            if cities.is_empty() && !game.units.iter().any(|u| u.team == team) {
                first(&mut events.eliminated, turn);
            }
        }
    }

    /// One queue's first item after the turn: paid, its work done, or out of the queue.
    fn follow_queue(&mut self, game: &GameState, city: usize, lane: usize, turn: u32) {
        let team = game.cities[city].team;
        let head = (game.lane_len(city, lane_of(lane)) > 0).then(|| {
            let (paid, progress, work) = game.lane_item(city, lane_of(lane), 0);
            (self.build_at(game, city, lane), paid, progress >= work)
        });
        // What the queue did this turn: the item it followed, and whether that is paid now.
        let mut worked = None;
        if let Some(tracked) = self.heads.remove(&(city, lane)) {
            match head {
                Some((build, paid, done)) if build == tracked.build => {
                    worked = Some((build, paid));
                    let mut tracked = tracked;
                    if paid && tracked.paid.is_none() {
                        tracked.paid = Some(turn);
                        if let Some(category) = spending(build) {
                            self.spend(tracked.team, category, tracked.price);
                        }
                    }
                    if done && tracked.done.is_none() {
                        tracked.done = Some(turn);
                    }
                    self.heads.insert((city, lane), tracked);
                }
                _ => {
                    // Out of the queue this turn: finished.
                    worked = Some((tracked.build, true));
                    if tracked.paid.is_none()
                        && let Some(category) = spending(tracked.build)
                    {
                        self.spend(tracked.team, category, tracked.price);
                    }
                    if tracked.build == Build::Gather
                        && let Some(side) = self.side(tracked.team)
                    {
                        let now = side.turns.last_mut().expect("a turn recorded");
                        for (total, amount) in now
                            .gathered
                            .iter_mut()
                            .zip(whole(super::super::city::GATHER_YIELD))
                        {
                            *total += amount;
                        }
                    }
                    let lane_name = if lane == 0 { "city" } else { "barracks" };
                    self.log.builds.push(BuildRecord {
                        name: format!("{} ({lane_name})", tracked.build.name()),
                        price: tracked.price,
                        queued: tracked.queued,
                        paid: tracked.paid.unwrap_or(turn),
                        done: tracked.done.unwrap_or(turn),
                        left: turn,
                    });
                }
            }
        }
        // A new head this late was queued behind the one that finished.
        if let Some((build, paid, done)) = head
            && !self.heads.contains_key(&(city, lane))
        {
            self.heads.insert(
                (city, lane),
                Tracked {
                    build,
                    team,
                    price: game.item_price(city, lane_of(lane), 0),
                    queued: turn,
                    paid: paid.then_some(turn),
                    done: done.then_some(turn),
                },
            );
        }
        if let Some((_, _, true)) = head
            && let Some(side) = self.side(team)
        {
            side.lanes.held[lane] += 1;
        }
        if lane == 0 {
            let at_cap = !game.can_grow(city);
            let Some(side) = self.side(team) else {
                return;
            };
            let lanes = &mut side.lanes;
            match worked {
                None => lanes.empty += 1,
                Some((_, false)) => lanes.waiting += 1,
                Some((Build::Gather, true)) if at_cap => lanes.gather_at_cap += 1,
                Some((Build::Gather, true)) => lanes.gather_poor += 1,
                Some((build, true)) => *lanes.working.entry(build.name()).or_default() += 1,
            }
        }
    }

    /// Sides whose units or cities stand within `CONTACT_RANGE` of each other have met.
    fn contact(&mut self, game: &GameState, turn: u32) {
        let presence: Vec<(Team, Hex)> = game
            .units
            .iter()
            .map(|u| (u.team, u.pos))
            .chain(game.cities.iter().map(|c| (c.team, c.pos)))
            .collect();
        let mut met = HashSet::default();
        for (i, &(a, at)) in presence.iter().enumerate() {
            for &(b, bt) in &presence[i + 1..] {
                if a != b && at.distance(bt) <= CONTACT_RANGE {
                    met.insert(a);
                    met.insert(b);
                }
            }
        }
        if !met.is_empty() {
            first(&mut self.log.first_contact, turn);
        }
        for team in met {
            if let Some(side) = self.side(team) {
                first(&mut side.events.first_contact, turn);
            }
        }
    }
}

/// `REPORT_ARMY_FIRST=1`: after the AI plans, each city queue that would only gather trains a
/// Melee instead, if its side can pay for one this turn (`forecast`'s spare, counting what its
/// other queues start): a side that spends on troops what the AI banks.
fn spend_on_troops(game: &mut GameState) {
    let melee = Build::Unit(BuildUnit::Melee);
    for city in 0..game.cities.len() {
        let queue = &game.cities[city].queue;
        if queue.len() != 1 || queue[0].paid || queue[0].build != Build::Gather {
            continue;
        }
        let team = game.cities[city].team;
        if game.forecast(team).spare.covers(melee.price()) {
            game.take_queue_item(city, 0);
            game.queue_build(city, melee);
        }
    }
}

/// Plays one game of `kind` from `seed` for `turns` turns, as `play_turn` does, watching it.
fn play(kind: GameKind, seed: u64, turns: u32, game_lines: bool) -> (GameLog, String) {
    let army_first = env_number("REPORT_ARMY_FIRST").is_some_and(|n| n > 0);
    let mut game = kind.start(seed);
    let mut observer = Observer::new(&game);
    let mut lines = String::new();
    if game_lines {
        let _ = writeln!(lines, "{} seed {seed}", kind.name());
    }
    for turn in 1..=turns {
        game.selected = None;
        game.group.clear();
        game.plan_ai_turn(PLAYER_TEAM);
        // Plans every other side, then the turn plays out in `update`.
        game.resolve_turn();
        if army_first {
            spend_on_troops(&mut game);
        }
        observer.after_plan(&game);
        game.update(0.0);
        assert!(!game.is_resolving(), "the turn finished resolving");
        observer.after_turn(&game);
        if game_lines && turn % 5 == 0 {
            for side in &observer.log.sides {
                let now = side.turns.last().expect("a turn recorded");
                let _ = writeln!(
                    lines,
                    "  turn {turn:2} {:?}: food {:3} wood {:3} metal {:3} | pop {:2} in {} cities | army {:2} | trained {:2}",
                    side.team,
                    now.stock[0],
                    now.stock[1],
                    now.stock[2],
                    now.pop,
                    now.cities,
                    now.army,
                    now.trained,
                );
            }
        }
    }
    (observer.log, lines)
}

/// A number for JSON: up to three decimals, and `null` for none.
fn number(value: f64) -> String {
    if !value.is_finite() {
        return "null".into();
    }
    let text = format!("{value:.3}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    if text.is_empty() || text == "-" || text == "-0" {
        "0".into()
    } else {
        text.into()
    }
}

/// Nearest-rank percentile `p` (0 to 100) of `values`, which it sorts.
fn percentile(values: &mut [f64], p: f64) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    values.sort_by(f64::total_cmp);
    let rank = ((p / 100.0) * (values.len() - 1) as f64).round() as usize;
    values[rank]
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        f64::NAN
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Median, 10th and 90th percentile of the turns an event happened on, and the share of
/// sides (or games) it never happened to.
struct EventStats {
    median: f64,
    p10: f64,
    p90: f64,
    never: f64,
}

fn event_stats(events: &[Option<u32>]) -> EventStats {
    let mut turns: Vec<f64> = events.iter().flatten().map(|&t| f64::from(t)).collect();
    let never = events.iter().filter(|e| e.is_none()).count() as f64 / events.len().max(1) as f64;
    EventStats {
        median: percentile(&mut turns, 50.0),
        p10: percentile(&mut turns, 10.0),
        p90: percentile(&mut turns, 90.0),
        never,
    }
}

/// Everything played of one kind of game.
struct KindReport {
    kind: GameKind,
    games: Vec<(u64, GameLog)>,
    turns: u32,
}

impl KindReport {
    fn sides(&self) -> impl Iterator<Item = &SideLog> {
        self.games.iter().flat_map(|(_, g)| &g.sides)
    }

    /// `value` of every side at the end of `turn` (1-based).
    fn at(&self, turn: u32, value: impl Fn(&SideTurn) -> f64) -> Vec<f64> {
        self.sides()
            .map(|s| value(&s.turns[turn as usize - 1]))
            .collect()
    }

    fn rows(&self) -> Vec<u32> {
        ROWS.into_iter().filter(|&t| t <= self.turns).collect()
    }

    fn events(&self) -> Vec<(String, Vec<Option<u32>>)> {
        let mut events = Vec::new();
        let mut side_event = |name: String, event: &dyn Fn(&SideEvents) -> Option<u32>| {
            events.push((name, self.sides().map(|s| event(&s.events)).collect()));
        };
        side_event("first troop".into(), &|e| e.first_troop);
        side_event("Barracks placed".into(), &|e| e.barracks_placed);
        side_event("Barracks built".into(), &|e| e.barracks_built);
        // Each step of the first cluster, then each cluster's last citizen.
        for pop in (2..=CLUSTER_SIZE).chain((2..=MAX_MANAGERS).map(|n| n * CLUSTER_SIZE)) {
            side_event(format!("first city at pop {pop}"), &|e| e.capital_pop[pop]);
        }
        for (i, step) in ARMY_STEPS.iter().enumerate() {
            side_event(format!("army of {step}"), &|e| e.army[i]);
        }
        side_event("contact (side)".into(), &|e| e.first_contact);
        side_event("fight (side)".into(), &|e| e.first_fight);
        side_event("troop fight (side)".into(), &|e| e.troop_fight);
        side_event("first loss".into(), &|e| e.first_loss);
        side_event("second city".into(), &|e| e.second_city);
        side_event("eliminated".into(), &|e| e.eliminated);
        events.push((
            "contact (game)".into(),
            self.games.iter().map(|(_, g)| g.first_contact).collect(),
        ));
        events.push((
            "fight (game)".into(),
            self.games.iter().map(|(_, g)| g.first_fight).collect(),
        ));
        events.push((
            "troop fight (game)".into(),
            self.games.iter().map(|(_, g)| g.troop_fight).collect(),
        ));
        events
    }

    /// Units that died a turn, per side, from each game's first fight between troops on.
    fn deaths_after_fight(&self) -> Vec<f64> {
        self.games
            .iter()
            .filter_map(|(_, g)| {
                let from = g.troop_fight? as usize;
                let deaths = &g.deaths[from - 1..];
                Some(deaths.iter().sum::<f64>() / deaths.len() as f64 / g.sides.len() as f64)
            })
            .collect()
    }

    /// Per build: how many finished, its price, and the mean turns waiting for the price,
    /// working and waiting for an open hex.
    fn builds(&self) -> Vec<(String, usize, Stock, [f64; 3])> {
        let mut by_name: Vec<(String, Vec<&BuildRecord>)> = Vec::new();
        for record in self.games.iter().flat_map(|(_, g)| &g.builds) {
            match by_name.iter_mut().find(|(name, _)| *name == record.name) {
                Some((_, records)) => records.push(record),
                None => by_name.push((record.name.clone(), vec![record])),
            }
        }
        by_name.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
        by_name
            .into_iter()
            .map(|(name, records)| {
                let turns = |f: &dyn Fn(&BuildRecord) -> u32| {
                    mean(&records.iter().map(|r| f64::from(f(r))).collect::<Vec<_>>())
                };
                let price = records
                    .iter()
                    .map(|r| r.price)
                    .max_by_key(|p| (p.food, p.wood, p.metal))
                    .unwrap_or_default();
                (
                    name,
                    records.len(),
                    price,
                    [
                        turns(&|r| r.paid - r.queued),
                        turns(&|r| r.done - r.paid + 1),
                        turns(&|r| r.left - r.done),
                    ],
                )
            })
            .collect()
    }

    fn jobs(&self) -> Vec<(&'static str, usize, f64)> {
        let mut by_name: Vec<(&'static str, Vec<f64>)> = Vec::new();
        for job in self.games.iter().flat_map(|(_, g)| &g.jobs) {
            let turns = f64::from(job.done - job.placed);
            match by_name.iter_mut().find(|(name, _)| *name == job.name) {
                Some((_, all)) => all.push(turns),
                None => by_name.push((job.name, vec![turns])),
            }
        }
        by_name.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
        by_name
            .into_iter()
            .map(|(name, all)| (name, all.len(), mean(&all)))
            .collect()
    }

    /// Per game at `turn`: the gap between the side that has the most of `value` and the one
    /// with the least, and the most as a multiple of the game's mean.
    fn spread(&self, turn: u32, value: impl Fn(&SideTurn) -> f64) -> (f64, f64) {
        let mut gaps = Vec::new();
        let mut leads = Vec::new();
        for (_, game) in &self.games {
            let values: Vec<f64> = game
                .sides
                .iter()
                .map(|s| value(&s.turns[turn as usize - 1]))
                .collect();
            let max = values.iter().copied().fold(f64::MIN, f64::max);
            let min = values.iter().copied().fold(f64::MAX, f64::min);
            gaps.push(max - min);
            let average = mean(&values);
            if average > 0.0 {
                leads.push(max / average);
            }
        }
        (mean(&gaps), mean(&leads))
    }

    /// What the queues did, as shares of their turns: the city queue's by what it worked (and
    /// gathering at the cap or below it, waiting, empty, holding a finished unit), and the
    /// Barracks' (training, idle, what a Melee lacked when idle, holding a finished unit).
    fn queue_use(&self) -> (Shares, Shares) {
        let lanes: Vec<&LaneUse> = self.sides().map(|s| &s.lanes).collect();
        let sum = |f: &dyn Fn(&LaneUse) -> u32| f64::from(lanes.iter().map(|l| f(l)).sum::<u32>());
        let city_turns = sum(&|l| {
            l.working.values().sum::<u32>() + l.gather_at_cap + l.gather_poor + l.waiting + l.empty
        })
        .max(1.0);
        let mut working: Vec<(&str, u32)> = Vec::new();
        for lane in &lanes {
            for (&name, &n) in &lane.working {
                match working.iter_mut().find(|(w, _)| *w == name) {
                    Some((_, total)) => *total += n,
                    None => working.push((name, n)),
                }
            }
        }
        working.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        let mut city: Vec<(String, f64)> = working
            .into_iter()
            .map(|(name, n)| (name.to_string(), 100.0 * f64::from(n) / city_turns))
            .collect();
        for (name, n) in [
            ("GATHER at the cap", sum(&|l| l.gather_at_cap)),
            ("GATHER below the cap", sum(&|l| l.gather_poor)),
            ("waiting", sum(&|l| l.waiting)),
            ("empty", sum(&|l| l.empty)),
            ("holding a finished unit", sum(&|l| l.held[0])),
        ] {
            city.push((name.into(), 100.0 * n / city_turns));
        }
        let barracks_turns = sum(&|l| l.training + l.idle).max(1.0);
        let idle = sum(&|l| l.idle).max(1.0);
        let barracks = vec![
            (
                "training".into(),
                100.0 * sum(&|l| l.training) / barracks_turns,
            ),
            ("idle".into(), 100.0 * sum(&|l| l.idle) / barracks_turns),
            (
                "idle, a Melee lacked food".into(),
                100.0 * sum(&|l| l.idle_short[0]) / idle,
            ),
            (
                "idle, a Melee lacked wood".into(),
                100.0 * sum(&|l| l.idle_short[1]) / idle,
            ),
            (
                "idle, a Melee lacked metal".into(),
                100.0 * sum(&|l| l.idle_short[2]) / idle,
            ),
            (
                "holding a finished unit".into(),
                100.0 * sum(&|l| l.held[1]) / barracks_turns,
            ),
        ];
        (city, barracks)
    }

    fn print(&self) {
        let sides = self.sides().count();
        let seeds: Vec<u64> = self.games.iter().map(|(s, _)| *s).collect();
        println!(
            "\n=== {}: {} games (seeds {:?}), {:.1} sides a game, {} turns ===",
            self.kind.name(),
            self.games.len(),
            seeds,
            sides as f64 / self.games.len() as f64,
            self.turns
        );
        println!("Per side, mean [10th-90th percentile]:");
        println!(
            "turn |   pop       | army        | trained     | food        | wood        | metal       | income f/w/m     | upkeep | workers"
        );
        for turn in self.rows() {
            let cell = |value: &dyn Fn(&SideTurn) -> f64| {
                let mut values = self.at(turn, value);
                format!(
                    "{:5.1} [{:2.0}-{:3.0}]",
                    mean(&values),
                    percentile(&mut values, 10.0),
                    percentile(&mut values, 90.0)
                )
            };
            let m = |value: &dyn Fn(&SideTurn) -> f64| mean(&self.at(turn, value));
            println!(
                "{turn:4} | {} | {} | {} | {} | {} | {} | {:4.1}/{:4.1}/{:4.1} | {:6.1} | {:4.1}",
                cell(&|s| s.pop),
                cell(&|s| s.army),
                cell(&|s| s.trained),
                cell(&|s| s.stock[0]),
                cell(&|s| s.stock[1]),
                cell(&|s| s.stock[2]),
                m(&|s| s.income[0]),
                m(&|s| s.income[1]),
                m(&|s| s.income[2]),
                m(&|s| s.upkeep),
                m(&|s| s.workers),
            );
        }

        println!("\nGained so far, by type (mean per side):");
        println!("turn | {}", KINDS.map(|k| format!("{k:>7}")).join(" "));
        for turn in self.rows() {
            let counts: Vec<String> = (0..KINDS.len())
                .map(|k| format!("{:7.2}", mean(&self.at(turn, |s| s.gained[k]))))
                .collect();
            println!("{turn:4} | {}", counts.join(" "));
        }

        println!("\nEvents, by turn: median [10th-90th], and the share that never happened:");
        for (name, events) in self.events() {
            let stats = event_stats(&events);
            println!(
                "  {name:<20} {:5.1} [{:4.0}-{:4.0}]  never {:3.0}%",
                stats.median,
                stats.p10,
                stats.p90,
                stats.never * 100.0
            );
        }
        let mut deaths = self.deaths_after_fight();
        println!(
            "  deaths a turn per side after the first troop fight: mean {:.2}, median {:.2}; cities captured: {} in {} games",
            mean(&deaths),
            percentile(&mut deaths, 50.0),
            self.games.iter().map(|(_, g)| g.captures).sum::<u32>(),
            self.games.len()
        );

        println!("\nSpent so far (food/wood/metal, mean per side), and Gather's yield:");
        for turn in self.rows() {
            let parts: Vec<String> = (0..SPENDING.len())
                .map(|c| {
                    let f = |r: usize| mean(&self.at(turn, |s| s.spent[c][r]));
                    format!("{} {:.0}/{:.0}/{:.0}", SPENDING[c], f(0), f(1), f(2))
                })
                .collect();
            let gathered = |r: usize| mean(&self.at(turn, |s| s.gathered[r]));
            println!(
                "{turn:4} | {} | gathered {:.0}/{:.0}/{:.0}",
                parts.join(" | "),
                gathered(0),
                gathered(1),
                gathered(2)
            );
        }

        let (city, barracks) = self.queue_use();
        let shares = |parts: &[(String, f64)]| {
            parts
                .iter()
                .map(|(name, share)| format!("{name} {share:.0}%"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        println!(
            "
City queue, share of city-turns: {}",
            shares(&city)
        );
        println!(
            "Barracks, share of Barracks-turns (what a Melee lacked as a share of idle turns): {}",
            shares(&barracks)
        );

        println!(
            "\nBuilds finished: count, price f/w/m, mean turns waiting for the price, working, waiting for an open hex:"
        );
        for (name, count, price, [wait, work, spawn]) in self.builds() {
            let [f, w, m] = whole(price);
            println!(
                "  {name:<22} {count:5}  {f:4.0}/{w:3.0}/{m:3.0}  wait {wait:4.2}  work {work:4.2}  out {spawn:4.2}"
            );
        }
        println!(
            "  (unpaid items the AI took off after waiting: {})",
            self.games.iter().map(|(_, g)| g.abandoned).sum::<u32>()
        );
        println!("Worker jobs done: count, mean turns from placed (and paid) to done:");
        for (name, count, turns) in self.jobs() {
            println!("  {name:<22} {count:5}  {turns:5.2}");
        }

        println!(
            "\nSpread between sides, per game (mean): gap between most and least, and the most as a multiple of the mean:"
        );
        for turn in self.rows().into_iter().filter(|&t| t >= 10) {
            let (trained_gap, trained_lead) = self.spread(turn, |s| s.trained);
            let (army_gap, army_lead) = self.spread(turn, |s| s.army);
            let (pop_gap, pop_lead) = self.spread(turn, |s| s.pop);
            println!(
                "{turn:4} | trained {trained_gap:4.1} (x{trained_lead:.2}) | army {army_gap:4.1} (x{army_lead:.2}) | pop {pop_gap:4.1} (x{pop_lead:.2})"
            );
        }
        println!("{}", self.headline());
    }

    /// One line to compare runs by (a knob changed, say).
    fn headline(&self) -> String {
        let at = |turn: u32, value: &dyn Fn(&SideTurn) -> f64| {
            if turn <= self.turns {
                format!("{:.1}", mean(&self.at(turn, value)))
            } else {
                "-".into()
            }
        };
        let events = self.events();
        let median = |name: &str| {
            events
                .iter()
                .find(|(n, _)| n == name)
                .map_or(f64::NAN, |(_, e)| event_stats(e).median)
        };
        format!(
            "HEADLINE {}: army@10/20/40/60 {}/{}/{}/{} | trained@20/40/60 {}/{}/{} | pop@10/20/30 {}/{}/{} | pop7 t{:.0} | first troop t{:.0} | stock@40 f/w/m {}/{}/{} | troop fight t{:.0} | deaths/turn/side {:.2}",
            self.kind.name(),
            at(10, &|s| s.army),
            at(20, &|s| s.army),
            at(40, &|s| s.army),
            at(60, &|s| s.army),
            at(20, &|s| s.trained),
            at(40, &|s| s.trained),
            at(60, &|s| s.trained),
            at(10, &|s| s.pop),
            at(20, &|s| s.pop),
            at(30, &|s| s.pop),
            median("first city at pop 7"),
            median("first troop"),
            at(40, &|s| s.stock[0]),
            at(40, &|s| s.stock[1]),
            at(40, &|s| s.stock[2]),
            median("troop fight (game)"),
            mean(&self.deaths_after_fight()),
        )
    }

    /// The numbers as a JSON object, for charts.
    fn json(&self) -> String {
        let numbers = |values: &[f64]| {
            let parts: Vec<String> = values.iter().map(|&v| number(v)).collect();
            format!("[{}]", parts.join(","))
        };
        let turns: Vec<u32> = (1..=self.turns).collect();
        let series = |value: &dyn Fn(&SideTurn) -> f64| {
            let (mut means, mut p10, mut p90) = (Vec::new(), Vec::new(), Vec::new());
            for &turn in &turns {
                let mut values = self.at(turn, value);
                means.push(mean(&values));
                p10.push(percentile(&mut values, 10.0));
                p90.push(percentile(&mut values, 90.0));
            }
            format!(
                "{{\"mean\":{},\"p10\":{},\"p90\":{}}}",
                numbers(&means),
                numbers(&p10),
                numbers(&p90)
            )
        };
        let mut out = String::new();
        let _ = write!(
            out,
            "{{\"name\":\"{}\",\"games\":{},\"seeds\":{},\"sides_per_game\":{},\"turns\":{},",
            self.kind.name(),
            self.games.len(),
            numbers(
                &self
                    .games
                    .iter()
                    .map(|(s, _)| *s as f64)
                    .collect::<Vec<_>>()
            ),
            numbers(
                &self
                    .games
                    .iter()
                    .map(|(_, g)| g.sides.len() as f64)
                    .collect::<Vec<_>>()
            ),
            self.turns
        );
        let per_turn: [(&str, Measure); 14] = [
            ("pop", &|s| s.pop),
            ("cities", &|s| s.cities),
            ("army", &|s| s.army),
            ("trained", &|s| s.trained),
            ("workers", &|s| s.workers),
            ("food", &|s| s.stock[0]),
            ("wood", &|s| s.stock[1]),
            ("metal", &|s| s.stock[2]),
            ("income_food", &|s| s.income[0]),
            ("income_wood", &|s| s.income[1]),
            ("income_metal", &|s| s.income[2]),
            ("upkeep", &|s| s.upkeep),
            ("deaths", &|s| s.deaths),
            ("gathered_total", &|s| s.gathered.iter().sum()),
        ];
        out.push_str("\"per_side\":{");
        let parts: Vec<String> = per_turn
            .iter()
            .map(|(name, value)| format!("\"{name}\":{}", series(*value)))
            .collect();
        out.push_str(&parts.join(","));
        out.push_str("},\"gained_by_type\":{");
        let parts: Vec<String> = KINDS
            .iter()
            .enumerate()
            .map(|(k, name)| {
                let means: Vec<f64> = turns
                    .iter()
                    .map(|&t| mean(&self.at(t, |s| s.gained[k])))
                    .collect();
                format!("\"{name}\":{}", numbers(&means))
            })
            .collect();
        out.push_str(&parts.join(","));
        out.push_str("},\"spent\":{");
        let parts: Vec<String> = SPENDING
            .iter()
            .enumerate()
            .map(|(c, name)| {
                let per_resource: Vec<String> = ["food", "wood", "metal"]
                    .iter()
                    .enumerate()
                    .map(|(r, resource)| {
                        let means: Vec<f64> = turns
                            .iter()
                            .map(|&t| mean(&self.at(t, |s| s.spent[c][r])))
                            .collect();
                        format!("\"{resource}\":{}", numbers(&means))
                    })
                    .collect();
                format!("\"{name}\":{{{}}}", per_resource.join(","))
            })
            .collect();
        out.push_str(&parts.join(","));
        out.push_str("},\"deaths_per_turn_all_sides\":");
        let deaths: Vec<f64> = (0..self.turns as usize)
            .map(|t| {
                mean(
                    &self
                        .games
                        .iter()
                        .map(|(_, g)| g.deaths[t])
                        .collect::<Vec<_>>(),
                )
            })
            .collect();
        out.push_str(&numbers(&deaths));
        out.push_str(",\"events\":{");
        let parts: Vec<String> = self
            .events()
            .iter()
            .map(|(name, events)| {
                let stats = event_stats(events);
                let turns: Vec<f64> = events
                    .iter()
                    .map(|e| e.map_or(f64::NAN, f64::from))
                    .collect();
                format!(
                    "\"{name}\":{{\"median\":{},\"p10\":{},\"p90\":{},\"never\":{},\"turns\":{}}}",
                    number(stats.median),
                    number(stats.p10),
                    number(stats.p90),
                    number(stats.never),
                    numbers(&turns)
                )
            })
            .collect();
        out.push_str(&parts.join(","));
        let mut deaths = self.deaths_after_fight();
        let _ = write!(
            out,
            "}},\"deaths_per_turn_per_side_after_troop_fight\":{{\"mean\":{},\"median\":{}}},\"captures\":{},",
            number(mean(&deaths)),
            number(percentile(&mut deaths, 50.0)),
            self.games.iter().map(|(_, g)| g.captures).sum::<u32>()
        );
        out.push_str("\"builds\":[");
        let parts: Vec<String> = self
            .builds()
            .iter()
            .map(|(name, count, price, [wait, work, spawn])| {
                format!(
                    "{{\"name\":\"{name}\",\"count\":{count},\"price\":{},\"wait\":{},\"work\":{},\"out\":{}}}",
                    numbers(&whole(*price)),
                    number(*wait),
                    number(*work),
                    number(*spawn),
                )
            })
            .collect();
        out.push_str(&parts.join(","));
        let (city, barracks) = self.queue_use();
        let shares = |parts: &[(String, f64)]| {
            parts
                .iter()
                .map(|(name, share)| format!("\"{name}\":{}", number(*share)))
                .collect::<Vec<_>>()
                .join(",")
        };
        let _ = write!(
            out,
            "],\"queue_use_percent\":{{\"city\":{{{}}},\"barracks\":{{{}}}}}",
            shares(&city),
            shares(&barracks)
        );
        out.push_str(",\"spread\":{");
        let spread_turns: Vec<u32> = (1..=self.turns).collect();
        let spreads: [(&str, Measure); 3] = [
            ("trained", &|s| s.trained),
            ("army", &|s| s.army),
            ("pop", &|s| s.pop),
        ];
        let parts: Vec<String> = spreads
            .iter()
            .map(|(name, value)| {
                let (gaps, leads): (Vec<f64>, Vec<f64>) =
                    spread_turns.iter().map(|&t| self.spread(t, value)).unzip();
                format!(
                    "\"{name}\":{{\"gap\":{},\"lead\":{}}}",
                    numbers(&gaps),
                    numbers(&leads)
                )
            })
            .collect();
        out.push_str(&parts.join(","));
        out.push_str("}}");
        out
    }
}

#[test]
#[ignore = "a report to read, not a check"]
fn economy_report() {
    let turns = env_number("REPORT_TURNS").map_or(REPORT_TURNS, |n| n as u32);
    assert!(turns >= 1, "REPORT_TURNS is at least 1");
    let kinds: Vec<GameKind> = std::env::var("REPORT_GAMES")
        .unwrap_or_else(|_| DEFAULT_GAMES.into())
        .split(',')
        .map(GameKind::parse)
        .collect();
    let game_lines = env_number("REPORT_GAME_LINES").is_some_and(|n| n > 0);
    let seeds = seeds();
    println!(
        "economy_report: {turns} turns, seeds {seeds:?}, games {}{}{}{}",
        kinds.iter().map(|k| k.name()).collect::<Vec<_>>().join(","),
        if env_number("SIM_SPEEDUP").is_some_and(|n| n > 0) {
            ", production speeds builds"
        } else {
            ""
        },
        if env_number("SIM_LIFETIME_CAP").is_some_and(|n| n > 0) {
            ", lifetime cap"
        } else {
            ""
        },
        if env_number("REPORT_ARMY_FIRST").is_some_and(|n| n > 0) {
            ", army first"
        } else {
            ""
        },
    );
    // Every game on its own thread, a batch of as many as there are cores at a time.
    let games: Vec<(usize, u64)> = (0..kinds.len())
        .flat_map(|k| seeds.iter().map(move |&seed| (k, seed)))
        .collect();
    let batch = thread::available_parallelism().map_or(8, |n| n.get());
    let mut played: Vec<(usize, u64, GameLog, String)> = Vec::new();
    for chunk in games.chunks(batch) {
        let results: Vec<_> = thread::scope(|scope| {
            let handles: Vec<_> = chunk
                .iter()
                .map(|&(k, seed)| {
                    let kind = kinds[k];
                    scope.spawn(move || {
                        let (log, lines) = play(kind, seed, turns, game_lines);
                        (k, seed, log, lines)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("a game played"))
                .collect()
        });
        played.extend(results);
    }
    let mut reports: Vec<KindReport> = kinds
        .iter()
        .map(|&kind| KindReport {
            kind,
            games: Vec::new(),
            turns,
        })
        .collect();
    for (k, seed, log, lines) in played {
        print!("{lines}");
        reports[k].games.push((seed, log));
    }
    for report in &reports {
        report.print();
    }
    println!();
    for report in &reports {
        println!("{}", report.headline());
    }
    if let Ok(path) = std::env::var("REPORT_JSON") {
        let json = format!(
            "{{\"turns\":{turns},\"speedup\":{},\"lifetime_cap\":{},\"army_first\":{},\"kinds\":[{}]}}\n",
            env_number("SIM_SPEEDUP").is_some_and(|n| n > 0),
            env_number("SIM_LIFETIME_CAP").is_some_and(|n| n > 0),
            env_number("REPORT_ARMY_FIRST").is_some_and(|n| n > 0),
            reports
                .iter()
                .map(KindReport::json)
                .collect::<Vec<_>>()
                .join(",")
        );
        std::fs::write(&path, json).unwrap_or_else(|e| panic!("REPORT_JSON: writing {path}: {e}"));
        println!("wrote {path}");
    }
}
