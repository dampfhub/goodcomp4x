//! Multiplayer in lockstep (`docs/multiplayer.md`). Every machine runs the
//! whole game. Turns are simultaneous, so a player's planning stays on their
//! own machine until they end it: then their side's *plan* (`TeamPlan`: its
//! units' orders, its cities' queues and citizens, its placed jobs, its
//! stockpile) goes to the host. Once the host has every human side's plan it
//! sends them all to everyone, and each machine applies them, in side order,
//! to the game as it stood when the turn's planning began (`turn_start`) and
//! resolves the turn. The same plans on the same game resolve the same way
//! (the simulation is deterministic), and a checksum of the result, compared
//! after every turn, catches it if they ever don't.
//!
//! This module is the protocol and the game side of it; `src/net.rs` moves
//! the messages.

use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::city::{Build, BuildUnit, City, LaborFocus, Stock, grow_price, in_interior};
use super::hex::Hex;
use super::unit::{Team, TurnOrder};
use super::workers::WorkerJob;
use super::{GameState, Scenario};

/// Bumped whenever a message or a plan changes shape, so mismatched builds
/// refuse each other instead of desyncing.
pub const PROTOCOL_VERSION: u32 = 2;
/// The most of anything a plan may list (units, a queue, worked tiles...):
/// far past what play produces, and a bound on what a hostile peer can make
/// this machine process.
const MAX_PLAN_LIST: usize = 256;
/// Letters a join code is made of: no 0/O or 1/I to confuse.
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;

/// The side the host plays, and the one the joining player gets.
pub const HOST_SEAT: Team = Team::Blue;
pub const GUEST_SEAT: Team = Team::Red;

/// What goes over the wire (`src/net.rs` frames and encodes it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Message {
    /// Guest to host, on connecting, with the join code the host shows.
    Hello { version: u32, code: String },
    /// Host to guest: the game to build, the same on both machines.
    Welcome {
        version: u32,
        seat: Team,
        scenario: Scenario,
        rng_seed: u64,
        production_speedup: bool,
        lifetime_special_cap: bool,
    },
    /// Guest to host: its side's plan for the turn.
    Plan(TeamPlan),
    /// Host to guest: every human side's plan for the turn; resolve it.
    Resolve(Vec<TeamPlan>),
    /// Guest to host: the game's checksum after resolving `turn`.
    Checksum { turn: u32, value: u64 },
    /// Host to guest: refused (a different protocol version, or a full game).
    Refused(String),
}

/// Which end of the connection this game is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Host,
    Guest,
}

/// One side's plan for a turn: everything its player's planning can change,
/// as it stood when they ended planning.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TeamPlan {
    /// The turn it's for: the one about to resolve.
    pub turn: u32,
    pub team: Team,
    pub stock: Stock,
    /// Its units that remain (one disbanded, or a settler that founded a
    /// city, isn't here), with their orders.
    pub units: Vec<UnitPlan>,
    pub cities: Vec<CityPlan>,
    pub workers: Vec<WorkerPlan>,
    /// Orders to its troops inside city interiors.
    pub fighters: Vec<FighterPlan>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UnitPlan {
    pub id: u32,
    pub planned_move: Option<Hex>,
    pub planned_attack: Option<Hex>,
    pub ability_queued: bool,
    pub holding: bool,
    pub guarding: bool,
    pub queued: Vec<TurnOrder>,
    pub following_queue: bool,
    pub planned_board: Option<u32>,
    pub planned_unload: Option<Hex>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CityPlan {
    /// A city is known by its position, which never changes.
    pub pos: Hex,
    /// The settler that founded it this turn, for a city new since the turn
    /// began.
    pub founded_by: Option<u32>,
    pub queue: Vec<Build>,
    pub progress: i32,
    pub barracks_queue: Vec<BuildUnit>,
    pub barracks_progress: i32,
    pub worked: Vec<Hex>,
    pub remembered_worked: Vec<Hex>,
    pub focus: LaborFocus,
    pub worker_jobs: Vec<WorkerJob>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorkerPlan {
    pub id: u32,
    pub job: Option<WorkerJob>,
    pub work_left: Option<u32>,
    pub recalled: bool,
    pub base: Hex,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FighterPlan {
    /// The city whose interior it's in, by position.
    pub city: Hex,
    pub source_id: u32,
    pub planned_move: Option<Hex>,
    pub planned_attack: Option<Hex>,
}

/// A networked game's lockstep state (`GameState::lockstep`).
#[derive(Clone)]
pub(super) struct Lockstep {
    pub role: Role,
    /// The game as this turn's planning began: every side's plan is applied
    /// to a copy of it. `None` only while a turn resolves.
    turn_start: Option<Box<GameState>>,
    /// Whether this side's player has ended planning this turn.
    submitted: bool,
    /// Host: the plans in for this turn.
    plans: Vec<TeamPlan>,
    /// Host: whether the guest has joined.
    pub peer_joined: bool,
    /// Host: the code a guest must give to join (`host_game`).
    join_code: String,
    /// Host: its own checksums, by turn, until the guest's arrive.
    checksums: Vec<(u32, u64)>,
    /// A turn whose checksums didn't match, once one hasn't.
    pub desync: Option<u32>,
    /// Messages for `src/net.rs` to send.
    outbox: Vec<Message>,
}

impl Lockstep {
    fn new(role: Role) -> Self {
        Self {
            role,
            turn_start: None,
            submitted: false,
            plans: Vec::new(),
            peer_joined: false,
            join_code: String::new(),
            checksums: Vec::new(),
            desync: None,
            outbox: Vec::new(),
        }
    }
}

impl GameState {
    /// A game to host: the Cities scenario, the host playing Blue and the
    /// guest who joins playing Red.
    pub fn host_game() -> GameState {
        let mut game = GameState::city_scenario();
        let seed = rand::random();
        game.seat_players(Role::Host, HOST_SEAT, seed);
        let code: String = (0..CODE_LENGTH)
            .map(|_| CODE_LETTERS[rand::random_range(0..CODE_LETTERS.len())] as char)
            .collect();
        game.notice = format!("HOSTING - JOIN CODE {code}");
        if let Some(lockstep) = game.lockstep.as_mut() {
            lockstep.join_code = code;
        }
        game
    }

    /// The code a guest must give to join this hosted game.
    pub fn join_code(&self) -> Option<&str> {
        self.lockstep
            .as_ref()
            .filter(|l| l.role == Role::Host)
            .map(|l| l.join_code.as_str())
    }

    /// The host's reply to a guest's `Hello`: the game to build, or why not.
    pub fn welcome(&mut self, hello: &Message) -> Message {
        let Message::Hello { version, code } = hello else {
            return Message::Refused("EXPECTED A HELLO".into());
        };
        if *version != PROTOCOL_VERSION {
            return Message::Refused(format!(
                "VERSION MISMATCH: HOST {PROTOCOL_VERSION}, GUEST {version}"
            ));
        }
        let Some(lockstep) = self.lockstep.as_mut() else {
            return Message::Refused("NOT HOSTING".into());
        };
        if lockstep.peer_joined {
            return Message::Refused("THE GAME IS FULL".into());
        }
        if !code.eq_ignore_ascii_case(&lockstep.join_code) {
            return Message::Refused("WRONG JOIN CODE".into());
        }
        lockstep.peer_joined = true;
        let turn_start = lockstep.turn_start.as_ref().expect("planning");
        self.notice = format!("{GUEST_SEAT:?} JOINED").to_uppercase();
        Message::Welcome {
            version: PROTOCOL_VERSION,
            seat: GUEST_SEAT,
            scenario: turn_start.scenario,
            rng_seed: self.rng_seed,
            production_speedup: turn_start.production_speedup,
            lifetime_special_cap: turn_start.lifetime_special_cap,
        }
    }

    /// The guest's game, built from the host's `Welcome` the same way the
    /// host built its own.
    pub fn join_game(welcome: &Message) -> Result<GameState, String> {
        match welcome {
            Message::Welcome {
                version,
                seat,
                scenario,
                rng_seed,
                production_speedup,
                lifetime_special_cap,
            } => {
                if *version != PROTOCOL_VERSION {
                    return Err(format!(
                        "VERSION MISMATCH: HOST {version}, GUEST {PROTOCOL_VERSION}"
                    ));
                }
                if *scenario != Scenario::Cities {
                    return Err(format!("CAN'T JOIN A {} GAME YET", scenario.name()));
                }
                if *seat != GUEST_SEAT {
                    return Err(format!("THE HOST OFFERED {seat:?}, NOT {GUEST_SEAT:?}"));
                }
                let mut game = GameState::city_scenario();
                game.production_speedup = *production_speedup;
                game.lifetime_special_cap = *lifetime_special_cap;
                game.seat_players(Role::Guest, *seat, *rng_seed);
                game.notice = format!("JOINED AS {seat:?}").to_uppercase();
                Ok(game)
            }
            Message::Refused(reason) => Err(reason.clone()),
            other => Err(format!("UNEXPECTED MESSAGE: {other:?}")),
        }
    }

    /// Makes this a networked game: both sides human, this one's player on
    /// `seat`, the RNG seeded the same on both machines, and this turn's
    /// planning begun.
    fn seat_players(&mut self, role: Role, seat: Team, rng_seed: u64) {
        self.humans = vec![HOST_SEAT, GUEST_SEAT];
        self.local_team = seat;
        // What this side has seen is its own: none of another side's view.
        self.memory.clear();
        self.reseed(rng_seed);
        self.lockstep = Some(Box::new(Lockstep::new(role)));
        self.selected = None;
        self.selected_city = None;
        self.begin_lockstep_turn();
        self.select_next_or_end_turn(None);
    }

    /// The turns resolved so far.
    #[cfg(test)]
    pub fn turn(&self) -> u32 {
        self.turn
    }

    /// The latest notice (the top bar's message).
    #[cfg(test)]
    pub fn notice(&self) -> &str {
        &self.notice
    }

    /// Whether this is a networked game.
    pub fn is_networked(&self) -> bool {
        self.lockstep.is_some()
    }

    /// While this side has ended planning and waits for the others' plans:
    /// input waits too (`is_resolving`).
    pub(super) fn waiting_for_peers(&self) -> bool {
        self.lockstep.as_ref().is_some_and(|l| l.submitted)
    }

    /// What the End Turn button says while the turn isn't this side's to
    /// play: whose plan it waits for, or that it's resolving.
    pub(super) fn resolving_label(&self) -> String {
        match self.lockstep.as_deref() {
            Some(l) if l.role == Role::Host && !l.peer_joined => "WAITING TO JOIN".into(),
            Some(l) if l.submitted => {
                let other = self.humans.iter().find(|&&t| t != self.local_team);
                format!("WAITING FOR {:?}", other.copied().unwrap_or(GUEST_SEAT)).to_uppercase()
            }
            _ => "RESOLVING".into(),
        }
    }

    /// The host turned away too many players and stopped listening.
    pub fn stop_listening(&mut self) {
        self.notice = "TOO MANY FAILED JOINS - NO LONGER LISTENING".into();
    }

    /// The connection to the other player is gone: the game can't go on,
    /// and says so.
    pub fn peer_lost(&mut self) {
        let other = self.humans.iter().find(|&&t| t != self.local_team).copied();
        self.notice = format!(
            "{:?} LEFT - THE GAME CAN'T GO ON",
            other.unwrap_or(GUEST_SEAT)
        )
        .to_uppercase();
    }

    /// Takes the messages waiting to be sent.
    pub fn take_outbox(&mut self) -> Vec<Message> {
        self.lockstep
            .as_mut()
            .map_or_else(Vec::new, |l| std::mem::take(&mut l.outbox))
    }

    /// Remembers the game as this turn's planning begins, for the plans to
    /// be applied to.
    fn begin_lockstep_turn(&mut self) {
        let mut start = self.clone();
        start.lockstep = None;
        start.savestate = None;
        start.effects.clear();
        if let Some(lockstep) = self.lockstep.as_mut() {
            lockstep.turn_start = Some(Box::new(start));
            lockstep.submitted = false;
            lockstep.plans.clear();
        }
    }

    /// End Turn in a networked game (`end_planning`, once nothing needs
    /// orders): this side's plan goes to the host (or, on the host, waits
    /// for the guest's), and input waits for the turn.
    pub(super) fn submit_plan(&mut self) {
        let plan = self.team_plan(self.local_team);
        let other = self.humans.iter().find(|&&t| t != self.local_team).copied();
        let Some(lockstep) = self.lockstep.as_mut() else {
            return;
        };
        if lockstep.role == Role::Host && !lockstep.peer_joined {
            self.notice = "WAITING FOR A PLAYER TO JOIN".into();
            return;
        }
        lockstep.submitted = true;
        match lockstep.role {
            Role::Host => lockstep.plans.push(plan),
            Role::Guest => lockstep.outbox.push(Message::Plan(plan)),
        }
        self.selected_city = None;
        self.selected = None;
        self.group.clear();
        self.notice = format!("WAITING FOR {:?}", other.unwrap_or(GUEST_SEAT)).to_uppercase();
        self.resolve_when_ready();
    }

    /// Handles a message from the other end. Everything that arrives is
    /// checked before it touches the game; an `Err` says why the peer
    /// should be dropped (a malformed or hostile message).
    pub fn receive(&mut self, message: Message) -> Result<(), String> {
        let Some(role) = self.lockstep.as_ref().map(|l| l.role) else {
            return Ok(());
        };
        match (role, message) {
            (Role::Host, Message::Plan(plan)) => {
                if plan.team != GUEST_SEAT {
                    return Err(format!("A PLAN FOR {:?}, NOT ITS OWN SIDE", plan.team));
                }
                self.check_plan(&plan)?;
                let lockstep = self.lockstep.as_mut().expect("networked");
                lockstep.plans.retain(|p| p.team != plan.team);
                lockstep.plans.push(plan);
                self.resolve_when_ready();
            }
            (Role::Guest, Message::Resolve(plans)) => {
                // One plan for each human side, each one sound.
                let mut teams: Vec<Team> = plans.iter().map(|p| p.team).collect();
                teams.sort();
                let mut humans = self.humans.clone();
                humans.sort();
                if teams != humans {
                    return Err(format!("A TURN'S PLANS FOR {teams:?}"));
                }
                for plan in &plans {
                    self.check_plan(plan)?;
                }
                self.resolve_with_plans(plans);
            }
            (Role::Host, Message::Checksum { turn, value }) => {
                let lockstep = self.lockstep.as_mut().expect("networked");
                if let Some(at) = lockstep.checksums.iter().position(|&(t, _)| t == turn) {
                    let (_, own) = lockstep.checksums.remove(at);
                    if own != value && lockstep.desync.is_none() {
                        lockstep.desync = Some(turn);
                        log::error!("desync after turn {turn}: host {own:x}, guest {value:x}");
                        self.notice = format!("DESYNC AFTER TURN {turn}");
                    }
                }
            }
            (_, other) => return Err(format!("UNEXPECTED MESSAGE: {other:?}")),
        }
        Ok(())
    }

    /// Why `plan` can't be applied to this turn's game, if it can't: it's
    /// checked against the game as the turn began, which is what it'll be
    /// applied to. Everything it names must exist and be its side's, every
    /// hex must be on the map (or in a city's interior), its lists must be
    /// short, and its stockpile no more than the side had plus what it
    /// could have taken back off its queues. It catches a hostile or broken
    /// peer before anything reaches the game; it doesn't catch every cheat.
    fn check_plan(&self, plan: &TeamPlan) -> Result<(), String> {
        let bad = |why: String| Err(format!("{:?}'S PLAN: {why}", plan.team).to_uppercase());
        let Some(start) = self.lockstep.as_ref().and_then(|l| l.turn_start.as_deref()) else {
            return bad("NO TURN IS BEING PLANNED".into());
        };
        let team = plan.team;
        if plan.turn != start.turn + 1 {
            return bad(format!("TURN {}, NOT {}", plan.turn, start.turn + 1));
        }
        if !start.is_human(team) {
            return bad("NOT A PLAYER'S SIDE".into());
        }
        let on_map = |hex: Hex| start.grid.contains(hex);
        let hexes_on_map = |hexes: &[Hex]| hexes.iter().all(|&h| on_map(h));
        let short = |len: usize| len <= MAX_PLAN_LIST;
        if ![
            plan.units.len(),
            plan.cities.len(),
            plan.workers.len(),
            plan.fighters.len(),
        ]
        .into_iter()
        .all(short)
        {
            return bad("TOO LONG".into());
        }
        // Units: its own, each once, with orders on the map.
        let own_unit = |id: u32| start.units.iter().any(|u| u.id == id && u.team == team);
        let mut seen = std::collections::HashSet::new();
        for unit in &plan.units {
            if !own_unit(unit.id) || !seen.insert(unit.id) {
                return bad(format!("UNIT {}", unit.id));
            }
            let orders = unit
                .queued
                .iter()
                .flat_map(|o| [Some(o.from), o.move_to, o.attack])
                .chain([unit.planned_move, unit.planned_attack, unit.planned_unload])
                .flatten()
                .collect::<Vec<_>>();
            if !short(unit.queued.len()) || !hexes_on_map(&orders) {
                return bad(format!("UNIT {}'S ORDERS", unit.id));
            }
            if unit.planned_board.is_some_and(|ship| !own_unit(ship)) {
                return bad(format!("UNIT {} BOARDS A SHIP NOT ITS OWN", unit.id));
            }
            // No move or attack past the unit's reach: a turn's move within
            // its range, an attack within its range of where it ends up. One
            // hex to spare for what an ability adds (Charge, Deploy); the
            // player plans on the board as they see it, so this is a bound,
            // not a pathfinding check.
            let body = start
                .units
                .iter()
                .find(|u| u.id == unit.id)
                .expect("own unit");
            let stats = body.stats();
            let (moves, reach) = (stats.move_range + 1, stats.attack_range + 1);
            let end = unit.planned_move.unwrap_or(body.pos);
            let steps_ok = end.distance(body.pos) <= moves
                && unit.planned_attack.is_none_or(|a| a.distance(end) <= reach)
                && unit.queued.iter().all(|o| {
                    o.end_pos().distance(o.from) <= moves
                        && o.attack.is_none_or(|a| a.distance(o.end_pos()) <= reach)
                });
            if !steps_ok {
                return bad(format!("UNIT {}'S ORDERS REACH TOO FAR", unit.id));
            }
        }
        // Cities: its own, or one a settler of its founded where it stood.
        let mut seen = std::collections::HashSet::new();
        for city in &plan.cities {
            if !on_map(city.pos) || !seen.insert((city.pos.q, city.pos.r)) {
                return bad("A CITY OFF THE MAP, OR TWICE".into());
            }
            let owned = start
                .cities
                .iter()
                .any(|c| c.pos == city.pos && c.team == team);
            let founded = !start.cities.iter().any(|c| c.pos == city.pos)
                && city.founded_by.is_some_and(|settler| {
                    start.settlers.contains(&settler)
                        && !plan.units.iter().any(|u| u.id == settler)
                        && start
                            .units
                            .iter()
                            .any(|u| u.id == settler && u.team == team && u.pos == city.pos)
                });
            if !owned && !founded {
                return bad(format!(
                    "A CITY AT ({}, {}) NOT ITS OWN",
                    city.pos.q, city.pos.r
                ));
            }
            let jobs_on_map = city
                .worker_jobs
                .iter()
                .all(|j| on_map(j.hex) && j.across.is_none_or(on_map));
            if ![
                city.queue.len(),
                city.barracks_queue.len(),
                city.worked.len(),
                city.remembered_worked.len(),
                city.worker_jobs.len(),
            ]
            .into_iter()
            .all(short)
                || !hexes_on_map(&city.worked)
                || !hexes_on_map(&city.remembered_worked)
                || !jobs_on_map
            {
                return bad(format!("CITY AT ({}, {})", city.pos.q, city.pos.r));
            }
        }
        // Workers out on the map: its own, with jobs on the map.
        for worker in &plan.workers {
            let own = start
                .field_workers
                .iter()
                .any(|w| w.id == worker.id && w.team == team);
            let job_on_map = worker
                .job
                .is_none_or(|j| on_map(j.hex) && j.across.is_none_or(on_map));
            if !own || !on_map(worker.base) || !job_on_map {
                return bad(format!("WORKER {}", worker.id));
            }
        }
        // Troops in a city's interior: its own, ordered within it.
        for fighter in &plan.fighters {
            let own = start.cities.iter().any(|c| {
                c.pos == fighter.city
                    && c.interior
                        .fighters
                        .iter()
                        .any(|f| f.source_id == fighter.source_id && f.team == team)
            });
            let inside = [fighter.planned_move, fighter.planned_attack]
                .into_iter()
                .flatten()
                .all(in_interior);
            if !own || !inside {
                return bad(format!("TROOP {} INSIDE A CITY", fighter.source_id));
            }
        }
        // The stockpile: never below nothing, nor above what the side had
        // plus every refund its queues could give.
        let mut most = start.stock(team);
        for city in start.cities.iter().filter(|c| c.team == team) {
            for (index, build) in city.queue.iter().enumerate() {
                most += match build {
                    Build::Grow => grow_price(city.population + index),
                    build => build.price(),
                };
            }
            for unit in &city.barracks_queue {
                most += unit.price();
            }
            for job in &city.worker_jobs {
                most += job.kind.price();
            }
        }
        for job in start
            .field_workers
            .iter()
            .filter(|w| w.team == team)
            .filter_map(|w| w.job)
        {
            most += job.kind.price();
        }
        let stock = plan.stock;
        if stock.food < 0
            || stock.wood < 0
            || stock.metal < 0
            || stock.food > most.food
            || stock.wood > most.wood
            || stock.metal > most.metal
        {
            return bad("A STOCKPILE IT DIDN'T HAVE".into());
        }
        Ok(())
    }

    /// Host: once every human side's plan is in, sends them all and resolves.
    fn resolve_when_ready(&mut self) {
        let humans = self.humans.clone();
        let Some(lockstep) = self.lockstep.as_mut() else {
            return;
        };
        if lockstep.role != Role::Host
            || !humans
                .iter()
                .all(|t| lockstep.plans.iter().any(|p| p.team == *t))
        {
            return;
        }
        let plans = std::mem::take(&mut lockstep.plans);
        lockstep.outbox.push(Message::Resolve(plans.clone()));
        self.resolve_with_plans(plans);
    }

    /// Applies every side's plan, in side order, to the game as this turn's
    /// planning began, keeping this machine's view (camera, fog memory,
    /// settings), and resolves the turn.
    fn resolve_with_plans(&mut self, mut plans: Vec<TeamPlan>) {
        let Some(mut lockstep) = self.lockstep.take() else {
            return;
        };
        let Some(start) = lockstep.turn_start.take() else {
            self.lockstep = Some(lockstep);
            return;
        };
        let mut next = *start;
        plans.sort_by_key(|p| p.team);
        for plan in &plans {
            next.apply_plan(plan);
        }
        next.keep_view_of(self);
        lockstep.submitted = false;
        next.lockstep = Some(lockstep);
        *self = next;
        // As `end_planning` does for a single-player turn.
        for i in 0..self.cities.len() {
            if !self.is_human(self.cities[i].team) {
                self.auto_assign_city(i);
            }
        }
        self.notice = "RESOLVING ORDERS".into();
        self.resolve_turn();
    }

    /// Carries this machine's view of the game (not the game itself) over
    /// from `old`: the camera, fog memory, settings and panels.
    fn keep_view_of(&mut self, old: &mut GameState) {
        std::mem::swap(&mut self.camera, &mut old.camera);
        std::mem::swap(&mut self.exterior_camera, &mut old.exterior_camera);
        std::mem::swap(&mut self.settings, &mut old.settings);
        std::mem::swap(&mut self.memory, &mut old.memory);
        self.settings_open = old.settings_open;
        self.show_yields = old.show_yields;
        self.show_details = old.show_details;
        self.cloud_time = old.cloud_time;
        self.quit_requested = old.quit_requested;
        self.fog_of_war = old.fog_of_war;
        self.local_team = old.local_team;
        self.humans = old.humans.clone();
        self.rng_seed = old.rng_seed;
    }

    /// After a networked turn resolves: its checksum goes to the host (or,
    /// on the host, waits for the guest's), and the next turn's planning
    /// begins.
    pub(super) fn finish_lockstep_turn(&mut self) {
        if self.lockstep.is_none() {
            return;
        }
        let value = self.checksum();
        let turn = self.turn;
        if let Some(lockstep) = self.lockstep.as_mut() {
            match lockstep.role {
                Role::Host => lockstep.checksums.push((turn, value)),
                Role::Guest => lockstep.outbox.push(Message::Checksum { turn, value }),
            }
        }
        self.begin_lockstep_turn();
    }

    /// `team`'s plan: what its player's planning set, as it stands now.
    pub(super) fn team_plan(&self, team: Team) -> TeamPlan {
        let start = self.lockstep.as_ref().and_then(|l| l.turn_start.as_deref());
        let units = self
            .units
            .iter()
            .filter(|u| u.team == team)
            .map(|u| UnitPlan {
                id: u.id,
                planned_move: u.planned_move,
                planned_attack: u.planned_attack,
                ability_queued: u.ability_queued,
                holding: u.holding,
                guarding: u.guarding,
                queued: u.queued.clone(),
                following_queue: u.following_queue,
                planned_board: u.planned_board,
                planned_unload: u.planned_unload,
            })
            .collect();
        let cities = self
            .cities
            .iter()
            .filter(|c| c.team == team)
            .map(|c| {
                // A city new this turn was founded by the settler that stood
                // on its tile when the turn began.
                let founded_by = start
                    .filter(|s| !s.cities.iter().any(|o| o.pos == c.pos))
                    .and_then(|s| {
                        s.units
                            .iter()
                            .find(|u| {
                                u.team == team && u.pos == c.pos && s.settlers.contains(&u.id)
                            })
                            .map(|u| u.id)
                    });
                CityPlan {
                    pos: c.pos,
                    founded_by,
                    queue: c.queue.clone(),
                    progress: c.progress,
                    barracks_queue: c.barracks_queue.clone(),
                    barracks_progress: c.barracks_progress,
                    worked: c.worked.clone(),
                    remembered_worked: c.remembered_worked.clone(),
                    focus: c.focus,
                    worker_jobs: c.worker_jobs.clone(),
                }
            })
            .collect();
        let workers = self
            .field_workers
            .iter()
            .filter(|w| w.team == team)
            .map(|w| WorkerPlan {
                id: w.id,
                job: w.job,
                work_left: w.work_left,
                recalled: w.recalled,
                base: w.base,
            })
            .collect();
        let fighters = self
            .cities
            .iter()
            .flat_map(|c| {
                c.interior
                    .fighters
                    .iter()
                    .filter(|f| f.team == team)
                    .map(|f| FighterPlan {
                        city: c.pos,
                        source_id: f.source_id,
                        planned_move: f.planned_move,
                        planned_attack: f.planned_attack,
                    })
            })
            .collect();
        TeamPlan {
            turn: self.turn + 1,
            team,
            stock: self.stock(team),
            units,
            cities,
            workers,
            fighters,
        }
    }

    /// Sets what `plan`'s side planned on this game, which must be the one
    /// its planning began from.
    pub(super) fn apply_plan(&mut self, plan: &TeamPlan) {
        let team = plan.team;
        *self.stock_mut(team) = plan.stock;
        // Its units not in the plan were disbanded, or founded a city.
        let kept = |id: u32| plan.units.iter().any(|u| u.id == id);
        let gone: Vec<u32> = self
            .units
            .iter()
            .filter(|u| u.team == team && !kept(u.id))
            .map(|u| u.id)
            .collect();
        self.units.retain(|u| !gone.contains(&u.id));
        for id in &gone {
            self.settlers.remove(id);
            self.player_controlled_units.remove(id);
        }
        if !gone.is_empty() {
            self.discard_interior_copies_of_dead_units();
        }
        for unit_plan in &plan.units {
            let Some(unit) = self
                .units
                .iter_mut()
                .find(|u| u.id == unit_plan.id && u.team == team)
            else {
                continue;
            };
            unit.planned_move = unit_plan.planned_move;
            unit.planned_attack = unit_plan.planned_attack;
            unit.ability_queued = unit_plan.ability_queued;
            unit.holding = unit_plan.holding;
            unit.guarding = unit_plan.guarding;
            unit.queued = unit_plan.queued.clone();
            unit.following_queue = unit_plan.following_queue;
            unit.planned_board = unit_plan.planned_board;
            unit.planned_unload = unit_plan.planned_unload;
        }
        for city_plan in &plan.cities {
            let index = match self.cities.iter().position(|c| c.pos == city_plan.pos) {
                Some(index) => index,
                None if city_plan.founded_by.is_some() => {
                    let id = self.cities.len() as u32;
                    self.cities.push(City::new(id, team, city_plan.pos));
                    self.cities.len() - 1
                }
                None => continue,
            };
            let city = &mut self.cities[index];
            if city.team != team {
                continue;
            }
            city.queue = city_plan.queue.clone();
            city.progress = city_plan.progress;
            city.barracks_queue = city_plan.barracks_queue.clone();
            city.barracks_progress = city_plan.barracks_progress;
            city.worked = city_plan.worked.clone();
            city.remembered_worked = city_plan.remembered_worked.clone();
            city.focus = city_plan.focus;
            city.worker_jobs = city_plan.worker_jobs.clone();
        }
        for worker_plan in &plan.workers {
            let Some(worker) = self
                .field_workers
                .iter_mut()
                .find(|w| w.id == worker_plan.id && w.team == team)
            else {
                continue;
            };
            worker.job = worker_plan.job;
            worker.work_left = worker_plan.work_left;
            worker.recalled = worker_plan.recalled;
            worker.base = worker_plan.base;
        }
        for fighter_plan in &plan.fighters {
            let Some(city) = self.cities.iter_mut().find(|c| c.pos == fighter_plan.city) else {
                continue;
            };
            if let Some(fighter) = city
                .interior
                .fighters
                .iter_mut()
                .find(|f| f.team == team && f.source_id == fighter_plan.source_id)
            {
                fighter.planned_move = fighter_plan.planned_move;
                fighter.planned_attack = fighter_plan.planned_attack;
            }
        }
    }

    /// A fingerprint of the game's state, the same on every machine that
    /// resolved the same turns the same way: units, cities, the stockpiles,
    /// workers and what's built on the map. Views (camera, fog memory,
    /// panels) aren't in it.
    pub fn checksum(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.turn.hash(&mut h);
        let mut units: Vec<_> = self.units.iter().collect();
        units.sort_by_key(|u| u.id);
        for u in units {
            (u.id, u.team, u.pos, u.hp.to_bits(), u.interior_hp.to_bits()).hash(&mut h);
            (u.ability_cooldown, u.deployed, u.cargo.len()).hash(&mut h);
        }
        for c in &self.cities {
            (c.id, c.team, c.pos, c.population, c.progress, c.workers).hash(&mut h);
            (c.barracks, c.barracks_hp.to_bits(), c.barracks_progress).hash(&mut h);
            c.worked.hash(&mut h);
            c.built.len().hash(&mut h);
            c.queue.len().hash(&mut h);
            c.interior.core_hp.to_bits().hash(&mut h);
            for f in &c.interior.fighters {
                (f.source_id, f.team, f.pos, f.hp.to_bits()).hash(&mut h);
            }
        }
        for stock in &self.stockpiles {
            (stock.food, stock.wood, stock.metal).hash(&mut h);
        }
        let mut workers: Vec<_> = self.field_workers.iter().collect();
        workers.sort_by_key(|w| w.id);
        for w in workers {
            (w.id, w.team, w.pos, w.work_left).hash(&mut h);
        }
        let mut roads: Vec<_> = self.roads.iter().collect();
        roads.sort_by_key(|h| (h.q, h.r));
        roads.hash(&mut h);
        let mut sites: Vec<_> = self
            .sites
            .iter()
            .map(|(hex, s)| (*hex, s.team, s.label))
            .collect();
        sites.sort_by_key(|(hex, ..)| (hex.q, hex.r));
        sites.hash(&mut h);
        let mut structures: Vec<_> = self
            .structures
            .iter()
            .map(|(hex, s)| (*hex, s.team, s.kind as u8))
            .collect();
        structures.sort_by_key(|(hex, ..)| (hex.q, hex.r));
        structures.hash(&mut h);
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A host and a guest, joined: two machines' games in one process.
    fn pair() -> (GameState, GameState) {
        let mut host = GameState::host_game();
        let welcome = host.welcome(&hello(&host));
        let guest = GameState::join_game(&welcome).expect("joins");
        (host, guest)
    }

    /// A guest's hello to `host`, with its join code.
    fn hello(host: &GameState) -> Message {
        Message::Hello {
            version: PROTOCOL_VERSION,
            code: host.join_code().unwrap().to_string(),
        }
    }

    /// Delivers every message waiting on each side to the other, until
    /// neither has any.
    fn exchange(host: &mut GameState, guest: &mut GameState) {
        loop {
            let to_guest = host.take_outbox();
            let to_host = guest.take_outbox();
            if to_guest.is_empty() && to_host.is_empty() {
                return;
            }
            for m in to_guest {
                guest.receive(roundtrip(&m)).expect("a sound message");
            }
            for m in to_host {
                host.receive(roundtrip(&m)).expect("a sound message");
            }
        }
    }

    /// `m` through the wire encoding and back, as `src/net.rs` sends it.
    fn roundtrip(m: &Message) -> Message {
        bincode::deserialize(&bincode::serialize(m).unwrap()).unwrap()
    }

    /// Plays out a turn that's resolving.
    fn play_out(game: &mut GameState) {
        while game.is_resolving() && !game.waiting_for_peers() {
            game.update(1.0);
        }
    }

    #[test]
    fn a_guest_joins_on_red_with_the_hosts_game() {
        let (host, guest) = pair();
        assert_eq!(host.local_team, HOST_SEAT);
        assert_eq!(guest.local_team, GUEST_SEAT);
        assert_eq!(host.humans, guest.humans);
        assert!(host.ai_teams().is_empty() && guest.ai_teams().is_empty());
        assert_eq!(host.checksum(), guest.checksum(), "the same game");
        assert_eq!(host.rng_seed, guest.rng_seed);
        // A second guest, or a different version, is refused.
        let mut host = host;
        let again = host.welcome(&hello(&host));
        assert!(matches!(again, Message::Refused(_)));
        let mut fresh = GameState::host_game();
        let old = fresh.welcome(&Message::Hello {
            version: 0,
            code: fresh.join_code().unwrap().to_string(),
        });
        assert!(GameState::join_game(&old).is_err());
        // Without the host's join code, nobody joins.
        let mut fresh = GameState::host_game();
        let wrong = fresh.welcome(&Message::Hello {
            version: PROTOCOL_VERSION,
            code: "NOPE".into(),
        });
        assert_eq!(wrong, Message::Refused("WRONG JOIN CODE".into()));
        let code = fresh.join_code().unwrap().to_lowercase();
        let right = fresh.welcome(&Message::Hello {
            version: PROTOCOL_VERSION,
            code,
        });
        assert!(matches!(right, Message::Welcome { .. }), "any case");
    }

    #[test]
    fn both_plans_resolve_the_same_turn_on_both_machines() {
        let (mut host, mut guest) = pair();
        // Each side orders something of its own: a unit's move, and a
        // build in its city.
        for game in [&mut host, &mut guest] {
            let team = game.local_team;
            let unit = game.units.iter().position(|u| u.team == team).unwrap();
            let target = game.units[unit]
                .pos
                .neighbors()
                .into_iter()
                .find(|&h| game.grid.is_passable(h) && !game.is_occupied(h))
                .unwrap();
            game.units[unit].planned_move = Some(target);
            let city = game.cities.iter().position(|c| c.team == team).unwrap();
            game.open_city(city);
            game.queue_selected_city_unit(BuildUnit::Melee);
            game.leave_city_view();
        }
        let host_plan = host.team_plan(HOST_SEAT);
        let guest_plan = guest.team_plan(GUEST_SEAT);
        // The guest ends its turn first: the host waits for nothing more
        // than its own player.
        guest.submit_plan();
        assert!(guest.waiting_for_peers() && guest.is_resolving());
        exchange(&mut host, &mut guest);
        assert!(!host.is_resolving(), "the host hasn't ended its turn");
        host.submit_plan();
        exchange(&mut host, &mut guest);
        play_out(&mut host);
        play_out(&mut guest);
        exchange(&mut host, &mut guest);
        assert_eq!(host.turn, 1);
        assert_eq!(guest.turn, 1);
        assert_eq!(host.checksum(), guest.checksum());
        assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
        // Each side's orders played out on both machines.
        for plan in [&host_plan, &guest_plan] {
            let city = plan.cities[0].pos;
            for game in [&host, &guest] {
                let queued = &game.cities.iter().find(|c| c.pos == city).unwrap().queue;
                assert_eq!(queued.len(), 1, "{:?}'s build", plan.team);
            }
        }
        // A new turn's planning began on both.
        assert!(!host.waiting_for_peers() && !guest.waiting_for_peers());
    }

    #[test]
    fn a_disbanded_unit_is_gone_on_both_machines() {
        let (mut host, mut guest) = pair();
        let unit = guest
            .units
            .iter()
            .position(|u| u.team == GUEST_SEAT)
            .unwrap();
        let id = guest.units[unit].id;
        guest.selected = Some(unit);
        guest.disband_selected();
        guest.selected = guest.units.iter().position(|u| u.id == id);
        guest.disband_selected();
        assert!(guest.units.iter().all(|u| u.id != id));
        guest.submit_plan();
        host.submit_plan();
        exchange(&mut host, &mut guest);
        play_out(&mut host);
        play_out(&mut guest);
        exchange(&mut host, &mut guest);
        assert!(host.units.iter().all(|u| u.id != id));
        assert_eq!(host.checksum(), guest.checksum());
    }

    #[test]
    fn a_mismatched_checksum_is_reported_as_a_desync() {
        let (mut host, mut guest) = pair();
        guest.submit_plan();
        host.submit_plan();
        exchange(&mut host, &mut guest);
        play_out(&mut host);
        // The guest's game drifts before it resolves.
        guest.stockpiles[GUEST_SEAT.index()] = Stock::whole(1, 2, 3);
        play_out(&mut guest);
        exchange(&mut host, &mut guest);
        assert_eq!(host.lockstep.as_ref().unwrap().desync, Some(1));
        assert_eq!(host.notice, "DESYNC AFTER TURN 1");
    }

    #[test]
    fn a_hostile_plan_is_refused_before_it_touches_the_game() {
        let (mut host, guest) = pair();
        let good = guest.team_plan(GUEST_SEAT);
        let before = host.checksum();
        let refused = |host: &mut GameState, plan: TeamPlan| {
            host.receive(Message::Plan(plan)).expect_err("refused")
        };
        // Orders for the host's units.
        let mut plan = good.clone();
        let blue = host.units.iter().find(|u| u.team == HOST_SEAT).unwrap();
        plan.units[0].id = blue.id;
        refused(&mut host, plan);
        // A plan for the host's side.
        let mut plan = good.clone();
        plan.team = HOST_SEAT;
        refused(&mut host, plan);
        // A move off the map, and one across it.
        let mut plan = good.clone();
        plan.units[0].planned_move = Some(Hex::new(9_999, -9_999));
        refused(&mut host, plan);
        let mut plan = good.clone();
        let far = host
            .grid
            .all_hexes()
            .max_by_key(|h| {
                h.distance(
                    guest
                        .units
                        .iter()
                        .find(|u| u.id == good.units[0].id)
                        .unwrap()
                        .pos,
                )
            })
            .unwrap();
        plan.units[0].planned_move = Some(far);
        refused(&mut host, plan);
        // A worker job off the map.
        let mut plan = good.clone();
        plan.cities[0].worker_jobs.push(WorkerJob::on_tile(
            Hex::new(500, 0),
            super::super::JobKind::Road,
        ));
        refused(&mut host, plan);
        // A city it doesn't own, "founded" with no settler.
        let mut plan = good.clone();
        plan.cities[0].pos = host
            .cities
            .iter()
            .find(|c| c.team == HOST_SEAT)
            .unwrap()
            .pos;
        refused(&mut host, plan);
        // Riches from nowhere.
        let mut plan = good.clone();
        plan.stock = Stock::whole(1_000_000, 0, 0);
        refused(&mut host, plan);
        // A queue far longer than play makes.
        let mut plan = good.clone();
        plan.cities[0].queue = vec![Build::Gather; MAX_PLAN_LIST + 1];
        refused(&mut host, plan);
        // The wrong turn.
        let mut plan = good.clone();
        plan.turn = 7;
        refused(&mut host, plan);
        // Nothing reached the game, and a sound plan still does.
        assert_eq!(host.checksum(), before);
        assert!(host.lockstep.as_ref().unwrap().plans.is_empty());
        host.receive(Message::Plan(good)).expect("sound");
    }

    #[test]
    fn a_guest_refuses_a_hosts_turn_without_every_side() {
        let (mut host, mut guest) = pair();
        let plan = host.team_plan(HOST_SEAT);
        let before = guest.checksum();
        assert!(guest.receive(Message::Resolve(vec![plan])).is_err());
        assert_eq!(guest.checksum(), before);
        assert!(
            guest
                .receive(Message::Plan(guest.team_plan(GUEST_SEAT)))
                .is_err()
        );
        let _ = host.take_outbox();
    }

    #[test]
    fn a_plan_survives_the_wire() {
        let (host, _) = pair();
        let plan = host.team_plan(HOST_SEAT);
        assert_eq!(roundtrip(&Message::Plan(plan.clone())), Message::Plan(plan));
    }
}
