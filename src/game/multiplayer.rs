//! Multiplayer in lockstep (`docs/multiplayer.md`). A host and up to six
//! guests play a generated world, each on their own side, and the AI plays
//! the rest. Every machine runs the whole game. Turns are simultaneous, so a
//! player's planning stays on their own machine until they end it: then their
//! side's *plan* (`TeamPlan`: its units' orders, its cities' queues and
//! citizens, its placed jobs, its stockpile) goes to the host. Once the host
//! has every human side's plan it sends them all to every guest, and each
//! machine applies them, in side order, to the game as it stood when the
//! turn's planning began (`turn_start`) and resolves the turn. The same plans on the same game resolve the same way
//! (the simulation is deterministic), and a checksum of the result, compared
//! after every turn, catches it if they ever don't.
//!
//! This module is the protocol and the game side of it; `src/net` moves
//! the messages, encrypted.

use std::hash::{DefaultHasher, Hash, Hasher};

use serde::{Deserialize, Serialize};

use super::GameState;
use super::camera::Camera;
use super::city::{
    Build, BuildUnit, Building, City, LaborFocus, MAX_CITY_POPULATION, Stock, grow_price,
    in_interior,
};
use super::hex::Hex;
use super::settings::Settings;
use super::terrain::Resource;
use super::unit::{Team, TurnOrder};
use super::workers::WorkerJob;

/// Bumped whenever a message or a plan changes shape, or the rules a turn
/// plays out by, so mismatched builds refuse each other instead of
/// desyncing.
pub const PROTOCOL_VERSION: u32 = 6;
/// The most of anything a plan may list (units, a queue, worked tiles...):
/// far past what play produces, and a bound on what a hostile peer can make
/// this machine process.
const MAX_PLAN_LIST: usize = 256;
/// Letters a join code is made of: no 0/O or 1/I to confuse.
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;
/// The most of a refusal's reason a guest shows.
const MAX_REASON: usize = 120;

/// The port a host listens on unless told another.
pub const DEFAULT_PORT: u16 = 7777;

/// The side the host plays; guests take the sides after it, in `Team::ALL`
/// order, as they join.
pub const HOST_SEAT: Team = Team::Blue;
/// The most players a game takes: every side a world can have.
pub const MAX_PLAYERS: usize = Team::ALL.len();

/// What goes over the wire (`src/net` encodes, seals and frames it).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Message {
    /// Guest to host, first thing over the encrypted channel (which only
    /// opens with the host's join code, `src/net/secure.rs`).
    Hello { version: u32 },
    /// Host to guest: the game to build, the same on every machine: the
    /// world (its map seed, and how many AI sides it has and whether they
    /// start with a city), the human sides and the guest's seat among them.
    Welcome {
        version: u32,
        seat: Team,
        humans: Vec<Team>,
        map_seed: u32,
        world_ai: usize,
        world_start_city: bool,
        rng_seed: u64,
        production_speedup: bool,
        lifetime_special_cap: bool,
    },
    /// Guest to host: its side's plan for the turn.
    Plan(TeamPlan),
    /// Host to guests: every human side's plan for the turn; resolve it.
    Resolve(Vec<TeamPlan>),
    /// Host to guests: this side's player left; the AI plays it from the
    /// next turn on. Sent before the turn's `Resolve`, so every machine
    /// changes hands at the same point.
    SeatLeft(Team),
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

/// What a side has paid for and not yet got: every item in its cities'
/// queues and Barracks' queues and every job it has placed, at the price it
/// paid (a Grow's depends on how many are queued ahead of it in its city).
fn committed<'a>(
    cities: impl Iterator<Item = (usize, &'a [Build], &'a [BuildUnit], &'a [WorkerJob])>,
    field_jobs: impl Iterator<Item = WorkerJob>,
) -> Stock {
    let mut total = Stock::default();
    for (population, queue, barracks, jobs) in cities {
        let mut grows = 0;
        for build in queue {
            total += match build {
                Build::Grow => {
                    grows += 1;
                    grow_price(population + grows - 1)
                }
                build => build.price(),
            };
        }
        for unit in barracks {
            total += unit.price();
        }
        for job in jobs {
            total += job.kind.price();
        }
    }
    for job in field_jobs {
        total += job.kind.price();
    }
    total
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
    /// Host: the human sides with a player (its own, and each guest's).
    seated: Vec<Team>,
    /// Host: the world's settings, for guests to build the same one.
    world: (usize, bool),
    /// Host: the code a guest must give to join (`host_game`): the key to
    /// the encrypted channel.
    join_code: String,
    /// Host: its own checksums of the last few turns, to compare each
    /// guest's with.
    checksums: Vec<(u32, u64)>,
    /// A turn whose checksums didn't match, once one hasn't.
    pub desync: Option<u32>,
    /// Guest: why the host dropped it, if it said.
    dropped: Option<String>,
    /// Messages for `src/net` to send: from the host, to every guest; from
    /// a guest, to the host.
    outbox: Vec<Message>,
}

impl Lockstep {
    fn new(role: Role) -> Self {
        Self {
            role,
            turn_start: None,
            submitted: false,
            plans: Vec::new(),
            seated: Vec::new(),
            world: (0, false),
            join_code: String::new(),
            checksums: Vec::new(),
            desync: None,
            dropped: None,
            outbox: Vec::new(),
        }
    }
}

/// The world a game's settings and player count make: its AI sides (never
/// fewer than the players need) and how every side starts.
fn world_settings(settings: &Settings, map_seed: u32, players: usize) -> Settings {
    Settings {
        world_ai: settings
            .world_ai_for(map_seed)
            .max(players.saturating_sub(1))
            .min(MAX_PLAYERS - 1),
        world_start_city: settings.world_start_city,
        ..Settings::default()
    }
}

impl GameState {
    /// A game to host: a new world for `players` people (2 to
    /// `MAX_PLAYERS`), the host on Blue and each guest on the next side as
    /// they join, with AI on the rest (as many as `settings` ask for, and
    /// no fewer than the players need).
    pub fn host_game(players: usize, settings: &Settings) -> GameState {
        let players = players.clamp(2, MAX_PLAYERS);
        let map_seed = rand::random();
        let world = world_settings(settings, map_seed, players);
        let mut game = GameState::world_scenario_with(map_seed, &world);
        let humans = Team::ALL[..players].to_vec();
        game.seat_players(Role::Host, HOST_SEAT, humans, rand::random());
        let code: String = (0..CODE_LENGTH)
            .map(|_| CODE_LETTERS[rand::random_range(0..CODE_LETTERS.len())] as char)
            .collect();
        game.notice = format!("HOSTING - JOIN CODE {code}");
        if let Some(lockstep) = game.lockstep.as_mut() {
            lockstep.join_code = code;
            lockstep.seated = vec![HOST_SEAT];
            lockstep.world = (world.world_ai, world.world_start_city);
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

    /// Host: the human sides still waiting for a player, in seating order.
    pub fn open_seats(&self) -> Vec<Team> {
        let Some(lockstep) = self.lockstep.as_deref().filter(|l| l.role == Role::Host) else {
            return Vec::new();
        };
        self.humans
            .iter()
            .copied()
            .filter(|t| !lockstep.seated.contains(t))
            .collect()
    }

    /// The host's reply to a guest's `Hello`: the next open seat and the
    /// game to build, or why not.
    pub fn welcome(&mut self, hello: &Message) -> (Option<Team>, Message) {
        let refuse = |why: &str| (None, Message::Refused(why.into()));
        let Message::Hello { version } = hello else {
            return refuse("EXPECTED A HELLO");
        };
        if *version != PROTOCOL_VERSION {
            return refuse(&format!(
                "VERSION MISMATCH: HOST {PROTOCOL_VERSION}, GUEST {version}"
            ));
        }
        let Some(&seat) = self.open_seats().first() else {
            return refuse("THE GAME IS FULL");
        };
        // Joining is only before the first turn: a later guest couldn't
        // rebuild the game from its seed.
        if self.turn > 0 {
            return refuse("THE GAME HAS STARTED");
        }
        let Some(lockstep) = self.lockstep.as_mut() else {
            return refuse("NOT HOSTING");
        };
        lockstep.seated.push(seat);
        let (world_ai, world_start_city) = lockstep.world;
        let start = lockstep.turn_start.as_ref().expect("planning");
        let welcome = Message::Welcome {
            version: PROTOCOL_VERSION,
            seat,
            humans: self.humans.clone(),
            map_seed: start.map_seed.expect("a world"),
            world_ai,
            world_start_city,
            rng_seed: self.rng_seed,
            production_speedup: start.production_speedup,
            lifetime_special_cap: start.lifetime_special_cap,
        };
        let open = self.open_seats().len();
        self.notice = if open == 0 {
            format!("{seat:?} JOINED - EVERYONE'S HERE").to_uppercase()
        } else {
            format!("{seat:?} JOINED - {open} MORE TO COME").to_uppercase()
        };
        (Some(seat), welcome)
    }

    /// The guest's game, built from the host's `Welcome` the same way the
    /// host built its own.
    pub fn join_game(welcome: &Message) -> Result<GameState, String> {
        match welcome {
            Message::Welcome {
                version,
                seat,
                humans,
                map_seed,
                world_ai,
                world_start_city,
                rng_seed,
                production_speedup,
                lifetime_special_cap,
            } => {
                if *version != PROTOCOL_VERSION {
                    return Err(format!(
                        "VERSION MISMATCH: HOST {version}, GUEST {PROTOCOL_VERSION}"
                    ));
                }
                // The human sides: the host's first, each once, the guest's
                // among them, all sides the world has.
                let mut unique = humans.clone();
                unique.sort();
                unique.dedup();
                if humans.len() < 2
                    || humans.len() > MAX_PLAYERS
                    || unique.len() != humans.len()
                    || humans[0] != HOST_SEAT
                    || *seat == HOST_SEAT
                    || !humans.contains(seat)
                    || *world_ai + 1 < humans.len()
                    || *world_ai >= MAX_PLAYERS
                {
                    return Err("THE HOST OFFERED A GAME THAT DOESN'T ADD UP".into());
                }
                let world = Settings {
                    world_ai: *world_ai,
                    world_start_city: *world_start_city,
                    ..Settings::default()
                };
                let mut game = GameState::world_scenario_with(*map_seed, &world);
                game.production_speedup = *production_speedup;
                game.lifetime_special_cap = *lifetime_special_cap;
                game.seat_players(Role::Guest, *seat, humans.clone(), *rng_seed);
                game.notice = format!("JOINED AS {seat:?}").to_uppercase();
                Ok(game)
            }
            Message::Refused(reason) => Err(reason.clone()),
            other => Err(format!("UNEXPECTED MESSAGE: {other:?}")),
        }
    }

    /// Makes this a networked game: `humans` played by people, this one's
    /// player on `seat`, the RNG seeded the same on every machine, and this
    /// turn's planning begun, looking at this side's home.
    fn seat_players(&mut self, role: Role, seat: Team, humans: Vec<Team>, rng_seed: u64) {
        self.humans = humans;
        self.local_team = seat;
        // What this side has seen is its own: none of another side's view.
        self.memory.clear();
        self.reseed(rng_seed);
        self.lockstep = Some(Box::new(Lockstep::new(role)));
        self.selected = None;
        self.selected_city = None;
        self.begin_lockstep_turn();
        let home = self
            .cities
            .iter()
            .find(|c| c.team == seat)
            .map(|c| c.pos)
            .or_else(|| self.units.iter().find(|u| u.team == seat).map(|u| u.pos));
        if let Some(home) = home {
            self.camera = Camera::new(home.to_world(), self.camera.half_height);
        }
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

    /// Whether the first turn has resolved: after it, nobody can join.
    pub fn has_started(&self) -> bool {
        self.turn > 0
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
        let open = self.open_seats().len();
        match self.lockstep.as_deref() {
            Some(l) if l.role == Role::Host && open > 0 => {
                format!("JOIN CODE {} - {open} TO COME", l.join_code)
            }
            Some(l) if l.submitted => match l.role {
                // The host knows whose plans are in.
                Role::Host => {
                    let waiting: Vec<String> = self
                        .humans
                        .iter()
                        .filter(|t| !l.plans.iter().any(|p| p.team == **t))
                        .map(|t| format!("{t:?}").to_uppercase())
                        .collect();
                    format!("WAITING FOR {}", waiting.join(", "))
                }
                Role::Guest => "WAITING FOR THE OTHERS".into(),
            },
            _ => "RESOLVING".into(),
        }
    }

    /// The host turned away too many players and stopped listening.
    pub fn stop_listening(&mut self) {
        self.notice = "TOO MANY FAILED JOINS - NO LONGER LISTENING".into();
    }

    /// A guest: the host is gone, and the game with it (or it dropped this
    /// guest, and said why).
    pub fn peer_lost(&mut self) {
        let dropped = self.lockstep.as_ref().and_then(|l| l.dropped.clone());
        self.notice = match dropped {
            Some(why) => format!("THE HOST DROPPED YOU: {why}"),
            None => format!("{HOST_SEAT:?} (THE HOST) LEFT - THE GAME CAN'T GO ON").to_uppercase(),
        };
    }

    /// Host: `team`'s player is gone. Before the game starts, their seat
    /// opens again for someone else; after, the AI plays their side from the
    /// next turn on, on every machine (`Message::SeatLeft`), and the turn
    /// no longer waits for them.
    pub fn seat_left(&mut self, team: Team) {
        let Some(lockstep) = self.lockstep.as_mut() else {
            return;
        };
        if lockstep.role != Role::Host || team == HOST_SEAT {
            return;
        }
        // The first turn hasn't begun to play out: nothing of theirs is
        // in the game yet, so someone else can take the seat.
        let started = self.turn > 0 || lockstep.turn_start.is_none();
        lockstep.seated.retain(|&t| t != team);
        lockstep.plans.retain(|p| p.team != team);
        if !started {
            self.notice = format!("{team:?} LEFT - WAITING FOR A PLAYER").to_uppercase();
            return;
        }
        lockstep.outbox.push(Message::SeatLeft(team));
        self.humans.retain(|&t| t != team);
        self.notice = format!("{team:?} LEFT - THE AI PLAYS THEM NOW").to_uppercase();
        self.resolve_when_ready();
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
        // Troops beside a city stand in its interior from the start of the
        // turn, the same on every machine, so a player can order them
        // there without the game changing on their machine alone.
        self.sync_city_interiors();
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
        let open = self.open_seats().len();
        let Some(lockstep) = self.lockstep.as_mut() else {
            return;
        };
        if open > 0 {
            self.notice = format!("WAITING FOR {open} MORE TO JOIN");
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
        self.notice = "WAITING FOR THE OTHERS".into();
        self.resolve_when_ready();
    }

    /// Handles a message from the player on `from` (a guest, on the host;
    /// the host, on a guest). Everything that arrives is checked before it
    /// touches the game; an `Err` says why the sender should be dropped (a
    /// malformed or hostile message).
    pub fn receive(&mut self, from: Team, message: Message) -> Result<(), String> {
        let Some(role) = self.lockstep.as_ref().map(|l| l.role) else {
            return Ok(());
        };
        match (role, message) {
            (Role::Host, Message::Plan(plan)) => {
                if plan.team != from || !self.is_human(from) {
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
            (Role::Guest, Message::SeatLeft(team)) => {
                if team == self.local_team || team == HOST_SEAT || !self.is_human(team) {
                    return Err(format!("{team:?} CAN'T LEAVE"));
                }
                self.humans.retain(|&t| t != team);
                self.notice = format!("{team:?} LEFT - THE AI PLAYS THEM NOW").to_uppercase();
            }
            (Role::Guest, Message::Refused(why)) => {
                // The host is dropping this guest; the link closes next.
                // Shown as it comes, so only what the fonts draw, and not
                // much of it.
                let why: String = why
                    .chars()
                    .filter(|c| c.is_ascii_graphic() || *c == ' ')
                    .take(MAX_REASON)
                    .collect();
                log::warn!("the host dropped us: {why}");
                self.lockstep.as_mut().expect("networked").dropped = Some(why);
                self.peer_lost();
            }
            (Role::Host, Message::Checksum { turn, value }) => {
                let lockstep = self.lockstep.as_mut().expect("networked");
                if let Some(&(_, own)) = lockstep.checksums.iter().find(|&&(t, _)| t == turn)
                    && own != value
                    && lockstep.desync.is_none()
                {
                    lockstep.desync = Some(turn);
                    log::error!("desync after turn {turn}: host {own:x}, {from:?} {value:x}");
                    self.notice = format!("DESYNC WITH {from:?} AFTER TURN {turn}").to_uppercase();
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
        // std's set, with random keys, not `fast_hash`: these are a remote
        // player's keys, not yet checked.
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
            if unit.ability_queued && !body.ability_queued && body.ability_cooldown > 0 {
                return bad(format!("UNIT {}'S ABILITY ISN'T READY", unit.id));
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
            let at = format!("CITY AT ({}, {})", city.pos.q, city.pos.r);
            let before = start.cities.iter().find(|c| c.pos == city.pos);
            let population = before.map_or(1, |c| c.population);
            // Planning keeps a build's progress, or clears it by taking the
            // build off: it never adds any.
            let (progress, barracks_progress) =
                before.map_or((0, 0), |c| (c.progress, c.barracks_progress));
            if ![0, progress].contains(&city.progress)
                || ![0, barracks_progress].contains(&city.barracks_progress)
            {
                return bad(format!("{at}: PROGRESS IT DIDN'T MAKE"));
            }
            // Its queue holds what a city trains: no Cavalry or Armored,
            // ships only with a Harbor, no growing past the cap.
            let harbor = before.is_some_and(|c| c.placed_site(Building::Harbor).is_some());
            let trainable = |build: &Build| match build {
                Build::Unit(unit) => {
                    unit.required_resource().is_none() && (!unit.unit_type().is_naval() || harbor)
                }
                _ => true,
            };
            let grows = city.queue.iter().filter(|&&b| b == Build::Grow).count();
            if !city.queue.iter().all(trainable) || population + grows > MAX_CITY_POPULATION {
                return bad(format!("{at}: A BUILD IT CAN'T MAKE"));
            }
            let barracks = before.is_some_and(|c| c.barracks.is_some());
            if !city.barracks_queue.is_empty()
                && (!barracks || city.barracks_queue.iter().any(|u| u.unit_type().is_naval()))
            {
                return bad(format!("{at}: A BARRACKS BUILD IT CAN'T MAKE"));
            }
            // Its citizens work tiles in its reach (or ones they already
            // worked), never a city or a building, no more than it has.
            let routes = start.routes_from(team, city.pos);
            let workable = |h: &Hex| {
                (routes.costs.contains_key(h) || before.is_some_and(|c| c.worked.contains(h)))
                    && !start.closed_to_citizens(*h)
            };
            if city.worked.len() > population.min(MAX_CITY_POPULATION)
                || city.remembered_worked.len() > MAX_CITY_POPULATION
                || !city.worked.iter().all(workable)
            {
                return bad(format!("{at}: TILES IT CAN'T WORK"));
            }
        }
        // Cavalry and Armored: no more queued than its deposits allow, or
        // at least no more than it had queued.
        for resource in [Resource::Horses, Resource::Iron] {
            let queued = |queues: &mut dyn Iterator<Item = &BuildUnit>| {
                queues
                    .filter(|u| u.required_resource() == Some(resource))
                    .count()
            };
            let before = queued(
                &mut start
                    .cities
                    .iter()
                    .filter(|c| c.team == team)
                    .flat_map(|c| &c.barracks_queue),
            );
            let now = queued(&mut plan.cities.iter().flat_map(|c| &c.barracks_queue));
            let used = start.special_used(team, resource) - before + now;
            if now > before && used > start.special_cap(team, resource) {
                return bad(format!(
                    "MORE {} TROOPS THAN ITS DEPOSITS ALLOW",
                    resource.name()
                ));
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
            // Planning only recalls a worker: its job and work go, and it
            // heads for its city. It can't be given work, or un-recalled.
            let body = start
                .field_workers
                .iter()
                .find(|w| w.id == worker.id)
                .expect("own worker");
            let home = start.cities.get(body.home).map(|c| c.pos);
            if (worker.job.is_some() && worker.job != body.job)
                || (worker.work_left.is_some() && worker.work_left != body.work_left)
                || (body.recalled && !worker.recalled)
                || (worker.base != body.base && Some(worker.base) != home)
            {
                return bad(format!("WORKER {}: WORK IT WASN'T GIVEN", worker.id));
            }
        }
        // Work put into a job stays with it through planning (a recalled
        // worker's job goes back on its city's list), or goes with the job
        // taken off: planning never adds any, or copies it onto a second job.
        let mut worked_jobs: Vec<WorkerJob> = start
            .cities
            .iter()
            .filter(|c| c.team == team)
            .flat_map(|c| c.worker_jobs.iter().copied())
            .chain(
                start
                    .field_workers
                    .iter()
                    .filter(|w| w.team == team)
                    .filter_map(|w| w.job),
            )
            .filter(|j| j.done > 0)
            .collect();
        let planned_jobs = plan.workers.iter().filter_map(|w| w.job).chain(
            plan.cities
                .iter()
                .flat_map(|c| c.worker_jobs.iter().copied()),
        );
        for job in planned_jobs.filter(|j| j.done > 0) {
            let Some(i) = worked_jobs
                .iter()
                .position(|w| w.fresh() == job.fresh() && w.done >= job.done)
            else {
                return bad(format!(
                    "JOB AT ({}, {}): WORK IT DIDN'T DO",
                    job.hex.q, job.hex.r
                ));
            };
            worked_jobs.swap_remove(i);
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
        // Nothing comes free. Every price is paid when queued and refunded
        // when taken off, so what the side holds plus everything it has
        // queued and placed is worth exactly what it held and had queued
        // when the turn began.
        let population = |pos: Hex| {
            start
                .cities
                .iter()
                .find(|c| c.pos == pos)
                .map_or(1, |c| c.population)
        };
        let before = start.stock(team)
            + committed(
                start.cities.iter().filter(|c| c.team == team).map(|c| {
                    (
                        c.population,
                        &c.queue[..],
                        &c.barracks_queue[..],
                        &c.worker_jobs[..],
                    )
                }),
                start
                    .field_workers
                    .iter()
                    .filter(|w| w.team == team)
                    .filter_map(|w| w.job),
            );
        let after = plan.stock
            + committed(
                plan.cities.iter().map(|c| {
                    (
                        population(c.pos),
                        &c.queue[..],
                        &c.barracks_queue[..],
                        &c.worker_jobs[..],
                    )
                }),
                plan.workers.iter().filter_map(|w| w.job),
            );
        let stock = plan.stock;
        if stock.food < 0 || stock.wood < 0 || stock.metal < 0 || after != before {
            return bad("SPENDING THAT DOESN'T ADD UP".into());
        }
        Ok(())
    }

    /// Host: once every human side's plan is in, sends them all and resolves.
    fn resolve_when_ready(&mut self) {
        if !self.open_seats().is_empty() {
            return;
        }
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
        std::mem::swap(&mut self.net_menu, &mut old.net_menu);
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
                Role::Host => {
                    lockstep.checksums.push((turn, value));
                    // A guest's checksum arrives within a turn or two.
                    lockstep.checksums.retain(|&(t, _)| t + 8 > turn);
                }
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
            for j in &c.worker_jobs {
                (j.hex, j.done).hash(&mut h);
            }
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
            (w.id, w.team, w.pos, w.work_left, w.job.map(|j| j.done)).hash(&mut h);
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

    /// The seat the first guest gets.
    const GUEST_SEAT: Team = Team::Red;

    /// A small world's settings: one AI side, cities to start.
    fn small() -> Settings {
        Settings {
            world_ai: 1,
            ..Settings::default()
        }
    }

    /// A host and a guest, joined, on a two-side world: two machines' games
    /// in one process.
    fn pair() -> (GameState, GameState) {
        let mut host = GameState::host_game(2, &small());
        let (seat, welcome) = host.welcome(&hello(&host));
        assert_eq!(seat, Some(GUEST_SEAT));
        let guest = GameState::join_game(&welcome).expect("joins");
        (host, guest)
    }

    /// A guest's hello.
    fn hello(_host: &GameState) -> Message {
        Message::Hello {
            version: PROTOCOL_VERSION,
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
                guest
                    .receive(HOST_SEAT, roundtrip(&m))
                    .expect("a sound message");
            }
            for m in to_host {
                host.receive(GUEST_SEAT, roundtrip(&m))
                    .expect("a sound message");
            }
        }
    }

    /// `m` through the wire encoding and back, as `src/net` sends it.
    fn roundtrip(m: &Message) -> Message {
        postcard::from_bytes(&postcard::to_allocvec(m).unwrap()).unwrap()
    }

    /// Plays out a turn that's resolving.
    fn play_out(game: &mut GameState) {
        while game.is_resolving() && !game.waiting_for_peers() {
            game.update(1.0);
        }
    }

    #[test]
    fn a_guest_joins_on_red_with_the_hosts_world() {
        let (host, guest) = pair();
        assert_eq!(host.scenario, super::super::Scenario::World);
        assert_eq!(host.map_seed, guest.map_seed);
        assert_eq!(host.local_team, HOST_SEAT);
        assert_eq!(guest.local_team, GUEST_SEAT);
        assert_eq!(host.humans, guest.humans);
        assert!(host.ai_teams().is_empty() && guest.ai_teams().is_empty());
        assert_eq!(host.checksum(), guest.checksum(), "the same game");
        assert_eq!(host.rng_seed, guest.rng_seed);
        // A second guest, or a different version, is refused.
        let mut host = host;
        let again = host.welcome(&hello(&host));
        assert!(matches!(again, (None, Message::Refused(_))));
        let mut fresh = GameState::host_game(2, &small());
        let (seat, old) = fresh.welcome(&Message::Hello { version: 0 });
        assert_eq!(seat, None);
        assert!(GameState::join_game(&old).is_err());
        // A join code: six letters and digits, none of them easily confused.
        let code = fresh.join_code().unwrap().to_string();
        assert_eq!(code.len(), CODE_LENGTH);
        assert!(code.bytes().all(|b| CODE_LETTERS.contains(&b)), "{code}");
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
        assert_eq!(host.notice, "DESYNC WITH RED AFTER TURN 1");
    }

    #[test]
    fn a_plan_keeps_a_jobs_work_but_never_adds_any() {
        use super::super::JobKind;
        use super::super::workers::FieldWorker;
        let (mut host, mut guest) = pair();
        // A guest worker two turns into a fort by its city, on both machines.
        let city = guest
            .cities
            .iter()
            .position(|c| c.team == GUEST_SEAT)
            .unwrap();
        let pos = guest.cities[city].pos;
        let hex = pos
            .neighbors()
            .into_iter()
            .find(|&h| guest.grid.is_passable(h))
            .unwrap();
        let worker = FieldWorker {
            id: 9_000,
            team: GUEST_SEAT,
            home: city,
            base: pos,
            pos: hex,
            job: Some(WorkerJob {
                done: 2,
                ..WorkerJob::on_tile(hex, JobKind::Fort)
            }),
            work_left: Some(2),
            recalled: false,
        };
        guest.field_workers.push(worker.clone());
        let start = host.lockstep.as_mut().unwrap().turn_start.as_mut().unwrap();
        start.field_workers.push(worker);
        let refused = |host: &mut GameState, plan: TeamPlan| {
            host.receive(GUEST_SEAT, Message::Plan(plan))
                .expect_err("refused")
        };
        // Recalled, its job goes back on the list with the work kept.
        guest.recall_worker(9_000);
        let recalled = guest.team_plan(GUEST_SEAT);
        let c = recalled.cities.iter().position(|c| c.pos == pos).unwrap();
        let listed = recalled.cities.iter().flat_map(|c| &c.worker_jobs);
        assert_eq!(listed.map(|j| j.done).collect::<Vec<_>>(), [2]);
        assert_eq!(host.check_plan(&recalled), Ok(()));
        // More work than was done.
        let mut plan = recalled.clone();
        plan.cities[c].worker_jobs[0].done = 3;
        let why = refused(&mut host, plan);
        assert!(why.contains("WORK IT DIDN'T DO"), "{why}");
        // The work copied: the worker keeps the job, and it's listed too.
        let mut plan = recalled.clone();
        let w = plan.workers.iter_mut().find(|w| w.id == 9_000).unwrap();
        w.job = Some(WorkerJob {
            done: 2,
            ..WorkerJob::on_tile(hex, JobKind::Fort)
        });
        w.work_left = Some(2);
        w.recalled = false;
        let why = refused(&mut host, plan);
        assert!(why.contains("WORK IT DIDN'T DO"), "{why}");
        // Work on a job nobody worked.
        let mut plan = recalled;
        let job = &mut plan.cities[c].worker_jobs[0];
        job.hex = pos.neighbors()[3];
        let why = refused(&mut host, plan);
        assert!(why.contains("WORK IT DIDN'T DO"), "{why}");
    }

    #[test]
    fn a_hostile_plan_is_refused_before_it_touches_the_game() {
        let (mut host, guest) = pair();
        let good = guest.team_plan(GUEST_SEAT);
        let before = host.checksum();
        let refused = |host: &mut GameState, plan: TeamPlan| {
            host.receive(GUEST_SEAT, Message::Plan(plan))
                .expect_err("refused")
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
        // Riches from nowhere, and builds for free.
        let mut plan = good.clone();
        plan.stock = Stock::whole(1_000_000, 0, 0);
        refused(&mut host, plan);
        let mut plan = good.clone();
        plan.cities[0].queue.push(Build::Unit(BuildUnit::Siege));
        refused(&mut host, plan);
        // Progress it didn't make, a troop a city can't train, and tiles
        // out of its reach.
        let mut plan = good.clone();
        plan.cities[0].queue = vec![Build::Gather];
        plan.cities[0].progress = 999;
        refused(&mut host, plan);
        let mut plan = good.clone();
        plan.cities[0].queue.push(Build::Unit(BuildUnit::Cavalry));
        plan.stock -= BuildUnit::Cavalry.price();
        refused(&mut host, plan);
        let mut plan = good.clone();
        let far = host
            .grid
            .all_hexes()
            .find(|&h| h.distance(plan.cities[0].pos) > 6)
            .unwrap();
        plan.cities[0].worked.push(far);
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
        host.receive(GUEST_SEAT, Message::Plan(good))
            .expect("sound");
    }

    #[test]
    fn a_guest_refuses_a_hosts_turn_without_every_side() {
        let (mut host, mut guest) = pair();
        let plan = host.team_plan(HOST_SEAT);
        let before = guest.checksum();
        assert!(
            guest
                .receive(HOST_SEAT, Message::Resolve(vec![plan]))
                .is_err()
        );
        assert_eq!(guest.checksum(), before);
        assert!(
            guest
                .receive(HOST_SEAT, Message::Plan(guest.team_plan(GUEST_SEAT)))
                .is_err()
        );
        let _ = host.take_outbox();
    }

    /// Plans a hostile peer could send that pass every check never crash
    /// either machine: thousands of random orders, builds, tiles and
    /// jobs, each applied and resolved.
    #[test]
    fn no_plan_that_passes_the_checks_crashes_the_game() {
        use rand::{RngExt, SeedableRng};
        let (host, guest) = pair();
        let good = guest.team_plan(GUEST_SEAT);
        let start = host.lockstep.as_ref().unwrap().turn_start.clone().unwrap();
        let hexes: Vec<Hex> = start.grid.all_hexes().collect();
        let builds = [
            Build::Unit(BuildUnit::Melee),
            Build::Unit(BuildUnit::Ranged),
            Build::Unit(BuildUnit::Siege),
            Build::Unit(BuildUnit::PatrolGalley),
            Build::Worker,
            Build::Grow,
            Build::Gather,
        ];
        let kinds = super::super::JobKind::ALL;
        let mut rng = rand::rngs::StdRng::seed_from_u64(7);
        let (mut accepted, mut tried) = (0, 0);
        while tried < 3000 {
            tried += 1;
            let mut plan = good.clone();
            let near = |from: Hex, reach: i32, rng: &mut rand::rngs::StdRng| {
                let close: Vec<Hex> = hexes
                    .iter()
                    .copied()
                    .filter(|h| h.distance(from) <= reach)
                    .collect();
                close[rng.random_range(0..close.len())]
            };
            for unit in &mut plan.units {
                let pos = start.units.iter().find(|u| u.id == unit.id).unwrap().pos;
                if rng.random_bool(0.5) {
                    unit.planned_move = Some(near(pos, 2, &mut rng));
                }
                if rng.random_bool(0.4) {
                    unit.planned_attack = Some(near(unit.planned_move.unwrap_or(pos), 3, &mut rng));
                }
                if rng.random_bool(0.2) {
                    unit.planned_unload = Some(near(pos, 2, &mut rng));
                }
                if rng.random_bool(0.2) {
                    let own: Vec<u32> = good.units.iter().map(|u| u.id).collect();
                    unit.planned_board = Some(own[rng.random_range(0..own.len())]);
                }
                if rng.random_bool(0.3) {
                    let mut from = unit.planned_move.unwrap_or(pos);
                    for _ in 0..rng.random_range(1..4) {
                        let move_to = near(from, 2, &mut rng);
                        unit.queued.push(TurnOrder {
                            from,
                            move_to: Some(move_to),
                            attack: rng.random_bool(0.5).then(|| near(move_to, 2, &mut rng)),
                        });
                        from = move_to;
                    }
                }
                unit.holding = rng.random_bool(0.2);
                unit.guarding = rng.random_bool(0.2);
                unit.following_queue = rng.random_bool(0.3);
                unit.ability_queued = rng.random_bool(0.3);
            }
            for city in &mut plan.cities {
                city.queue = (0..rng.random_range(0..4))
                    .map(|_| builds[rng.random_range(0..builds.len())])
                    .collect();
                city.worked.reverse();
                city.worker_jobs = (0..rng.random_range(0..4))
                    .map(|_| {
                        let hex = near(city.pos, 4, &mut rng);
                        let kind = kinds[rng.random_range(0..kinds.len())];
                        WorkerJob {
                            hex,
                            kind,
                            across: kind
                                .on_edge()
                                .then(|| hex.neighbors()[rng.random_range(0..6)]),
                            done: 0,
                        }
                    })
                    .collect();
            }
            // Keep the books balanced, so the plan gets through when the
            // side can pay for it.
            let population = |pos: Hex| {
                start
                    .cities
                    .iter()
                    .find(|c| c.pos == pos)
                    .map_or(1, |c| c.population)
            };
            let spent = committed(
                plan.cities.iter().map(|c| {
                    (
                        population(c.pos),
                        &c.queue[..],
                        &c.barracks_queue[..],
                        &c.worker_jobs[..],
                    )
                }),
                plan.workers.iter().filter_map(|w| w.job),
            );
            let had = good.stock
                + committed(
                    good.cities.iter().map(|c| {
                        (
                            population(c.pos),
                            &c.queue[..],
                            &c.barracks_queue[..],
                            &c.worker_jobs[..],
                        )
                    }),
                    good.workers.iter().filter_map(|w| w.job),
                );
            plan.stock = had - spent;
            if host.check_plan(&plan).is_err() {
                continue;
            }
            accepted += 1;
            let mut game = (*start).clone();
            game.apply_plan(&plan);
            game.apply_plan(&host.team_plan(HOST_SEAT));
            game.resolve_turn();
            while game.is_resolving() {
                game.update(1.0);
            }
        }
        assert!(
            accepted > 100,
            "only {accepted} of {tried} plans got through"
        );
    }

    /// A host and `guests` guests on a world with AI sides besides.
    fn table(guests: usize) -> (GameState, Vec<GameState>) {
        let settings = Settings {
            world_ai: guests + 2,
            ..Settings::default()
        };
        let mut host = GameState::host_game(guests + 1, &settings);
        let joined = (0..guests)
            .map(|_| {
                let (seat, welcome) = host.welcome(&hello(&host));
                assert!(seat.is_some(), "{welcome:?}");
                GameState::join_game(&welcome).expect("joins")
            })
            .collect();
        (host, joined)
    }

    /// Delivers the host's messages to every guest and theirs to it, until
    /// nobody has any.
    fn exchange_all(host: &mut GameState, guests: &mut [GameState]) {
        loop {
            let to_guests = host.take_outbox();
            let mut quiet = to_guests.is_empty();
            for guest in guests.iter_mut() {
                for m in &to_guests {
                    guest
                        .receive(HOST_SEAT, roundtrip(m))
                        .expect("a sound message");
                }
                let from = guest.local_team;
                for m in guest.take_outbox() {
                    quiet = false;
                    host.receive(from, roundtrip(&m)).expect("a sound message");
                }
            }
            if quiet {
                return;
            }
        }
    }

    /// Every game ends its turn and plays it out, the messages flowing.
    fn play_turn(host: &mut GameState, guests: &mut [GameState]) {
        for game in std::iter::once(&mut *host).chain(guests.iter_mut()) {
            game.submit_plan();
        }
        exchange_all(host, guests);
        play_out(host);
        for guest in guests.iter_mut() {
            play_out(guest);
        }
        exchange_all(host, guests);
    }

    #[test]
    fn three_players_share_a_world_with_the_ai_on_the_rest() {
        let (mut host, mut guests) = table(2);
        let seats: Vec<Team> = guests.iter().map(|g| g.local_team).collect();
        assert_eq!(seats, [Team::Red, Team::Green], "in side order");
        assert_eq!(host.humans, [Team::Blue, Team::Red, Team::Green]);
        assert!(!host.ai_teams().is_empty(), "the AI plays the rest");
        assert_eq!(host.ai_teams(), guests[0].ai_teams());
        // A fourth is turned away: the seats are full.
        assert!(matches!(
            host.welcome(&hello(&host)),
            (None, Message::Refused(_))
        ));
        for guest in &guests {
            assert_eq!(guest.checksum(), host.checksum());
        }
        for _ in 0..3 {
            play_turn(&mut host, &mut guests);
            for guest in &guests {
                assert_eq!(guest.checksum(), host.checksum(), "turn {}", host.turn);
            }
        }
        assert_eq!(host.turn, 3);
        assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
        // Nobody joins a game under way.
        assert!(host.has_started());
    }

    #[test]
    fn the_host_waits_for_every_seat_before_the_first_turn() {
        let settings = Settings {
            world_ai: 3,
            ..Settings::default()
        };
        let mut host = GameState::host_game(3, &settings);
        let (_, welcome) = host.welcome(&hello(&host));
        let _red = GameState::join_game(&welcome).unwrap();
        assert_eq!(host.open_seats(), [Team::Green]);
        host.submit_plan();
        assert!(!host.waiting_for_peers(), "Green hasn't joined");
        assert_eq!(
            host.resolving_label(),
            format!("JOIN CODE {} - 1 TO COME", host.join_code().unwrap())
        );
        // A guest who leaves before the start frees their seat.
        host.seat_left(Team::Red);
        assert_eq!(host.open_seats(), [Team::Red, Team::Green]);
        assert_eq!(
            host.humans,
            [Team::Blue, Team::Red, Team::Green],
            "still seats for people"
        );
    }

    #[test]
    fn a_guest_who_leaves_mid_game_hands_their_side_to_the_ai() {
        let (mut host, mut guests) = table(2);
        play_turn(&mut host, &mut guests);
        // Green's player goes; Red is waiting on the turn.
        let green = guests.pop().unwrap();
        guests[0].submit_plan();
        host.submit_plan();
        exchange_all(&mut host, &mut guests);
        assert!(host.waiting_for_peers(), "waiting for Green");
        host.seat_left(green.local_team);
        // The turn goes on without Green, on both machines left.
        exchange_all(&mut host, &mut guests);
        play_out(&mut host);
        play_out(&mut guests[0]);
        exchange_all(&mut host, &mut guests);
        assert_eq!(host.turn, 2);
        assert_eq!(guests[0].turn, 2);
        assert!(!host.is_human(Team::Green) && !guests[0].is_human(Team::Green));
        assert!(host.ai_teams().contains(&Team::Green));
        assert_eq!(host.checksum(), guests[0].checksum());
        // And the next turn too.
        play_turn(&mut host, &mut guests);
        assert_eq!(host.checksum(), guests[0].checksum());
        assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
    }

    #[test]
    fn a_guest_refuses_a_seat_change_it_couldnt_have_had() {
        let (_, mut guests) = table(2);
        let red = &mut guests[0];
        assert!(
            red.receive(HOST_SEAT, Message::SeatLeft(Team::Red))
                .is_err(),
            "itself"
        );
        assert!(
            red.receive(HOST_SEAT, Message::SeatLeft(Team::Blue))
                .is_err(),
            "the host"
        );
        let ai = red.ai_teams()[0];
        assert!(
            red.receive(HOST_SEAT, Message::SeatLeft(ai)).is_err(),
            "an AI side"
        );
    }

    #[test]
    fn a_welcome_that_doesnt_add_up_is_refused() {
        let mut host = GameState::host_game(2, &small());
        let (_, welcome) = host.welcome(&hello(&host));
        let Message::Welcome { humans, .. } = &welcome else {
            panic!("{welcome:?}")
        };
        let tweak = |f: &dyn Fn(&mut Message)| {
            let mut bad = welcome.clone();
            f(&mut bad);
            GameState::join_game(&bad).is_err()
        };
        assert!(tweak(&|m| if let Message::Welcome { seat, .. } = m {
            *seat = HOST_SEAT;
        }));
        assert!(tweak(&|m| if let Message::Welcome { humans, .. } = m {
            humans.push(Team::Red);
        }));
        assert!(tweak(&|m| if let Message::Welcome { world_ai, .. } = m {
            *world_ai = 99;
        }));
        assert!(tweak(&|m| if let Message::Welcome { world_ai, .. } = m {
            *world_ai = 0;
        }));
        assert_eq!(humans.len(), 2);
    }

    #[test]
    fn looking_inside_a_city_and_ordering_a_troop_there_is_a_sound_plan() {
        let (mut host, mut guests) = table(2);
        // A Red troop beside a city stands in its interior from the turn's
        // start, on every machine.
        let (city, fighter) = {
            let red = &guests[0];
            red.cities
                .iter()
                .enumerate()
                .find_map(|(i, c)| {
                    let unit = red.units.iter().find(|u| {
                        u.team == Team::Red
                            && u.pos.distance(c.pos) == 1
                            && !u.is_naval()
                            && !red.settlers.contains(&u.id)
                    })?;
                    Some((i, unit.id))
                })
                .expect("a Red troop beside a city")
        };
        for game in std::iter::once(&host).chain(&guests) {
            let inside = &game.cities[city].interior.fighters;
            assert!(inside.iter().any(|f| f.source_id == fighter));
        }
        let before = host.checksum();
        host.open_city_interior(city);
        host.close_city_interior();
        assert_eq!(host.checksum(), before, "looking changes nothing");
        // Red looks inside and orders its troop to an open tile.
        let red = &mut guests[0];
        red.open_city_interior(city);
        red.interior_click(
            red.cities[city]
                .interior
                .fighters
                .iter()
                .find(|f| f.source_id == fighter)
                .unwrap()
                .pos,
        );
        let open = (-2..=2)
            .flat_map(|q| (-2..=2).map(move |r| Hex::new(q, r)))
            .filter(|&h| in_interior(h))
            .find(|&h| {
                h != Hex::new(0, 0)
                    && !red.cities[city]
                        .interior
                        .fighters
                        .iter()
                        .any(|f| f.pos == h)
                    && red.cities[city]
                        .interior
                        .fighters
                        .iter()
                        .find(|f| f.source_id == fighter)
                        .is_some_and(|f| f.pos.distance(h) == 1)
            })
            .expect("an open tile");
        red.interior_click(open);
        red.close_city_interior();
        assert!(
            red.team_plan(Team::Red)
                .fighters
                .iter()
                .any(|f| f.planned_move == Some(open))
        );
        // The host takes the plan, and every machine plays the same turn.
        play_turn(&mut host, &mut guests);
        for guest in &guests {
            assert_eq!(guest.checksum(), host.checksum());
        }
        assert_eq!(host.turn, 1);
    }

    #[test]
    fn a_seat_left_on_the_first_turn_waits_for_a_new_player() {
        let (mut host, mut guests) = table(2);
        // Green ends the first turn, then goes before it plays out.
        guests[1].submit_plan();
        exchange_all(&mut host, &mut guests);
        let _green = guests.pop();
        host.seat_left(Team::Green);
        assert_eq!(host.open_seats(), [Team::Green]);
        assert!(
            host.humans.contains(&Team::Green),
            "still a seat for a person"
        );
        guests[0].submit_plan();
        host.submit_plan();
        exchange_all(&mut host, &mut guests);
        assert!(
            !host.waiting_for_peers(),
            "the host can't end the turn with a seat open"
        );
        assert_eq!(host.turn, 0, "the turn waits for Green's seat");
        // Someone new takes it, and the turn plays out with their plan.
        let (seat, welcome) = host.welcome(&hello(&host));
        assert_eq!(seat, Some(Team::Green));
        guests.push(GameState::join_game(&welcome).unwrap());
        guests[1].submit_plan();
        host.submit_plan();
        exchange_all(&mut host, &mut guests);
        play_out(&mut host);
        for guest in &mut guests {
            play_out(guest);
        }
        exchange_all(&mut host, &mut guests);
        assert_eq!(host.turn, 1);
        for guest in &guests {
            assert_eq!(guest.checksum(), host.checksum());
        }
    }

    #[test]
    fn a_dropped_guest_says_why() {
        let (_, mut guest) = pair();
        guest
            .receive(HOST_SEAT, Message::Refused("RED'S PLAN: UNIT 3".into()))
            .unwrap();
        guest.peer_lost();
        assert_eq!(guest.notice, "THE HOST DROPPED YOU: RED'S PLAN: UNIT 3");
        // Only so much of a reason, and only what the fonts draw.
        let long = format!("\u{202e}\n{}", "X".repeat(500));
        guest.receive(HOST_SEAT, Message::Refused(long)).unwrap();
        assert_eq!(
            guest.notice,
            format!("THE HOST DROPPED YOU: {}", "X".repeat(MAX_REASON))
        );
    }

    #[test]
    #[ignore = "slow (20 s): cargo test plans_the_ai_makes -- --ignored"]
    fn plans_the_ai_makes_for_a_player_pass_the_checks() {
        // The AI plays every seat, as a player could: the host must take
        // every plan, and every machine must play the same game. Players
        // look inside their cities too, which must change nothing.
        for _ in 0..4 {
            let (mut host, mut guests) = table(2);
            for _ in 0..40 {
                for game in std::iter::once(&mut host).chain(guests.iter_mut()) {
                    let team = game.local_team;
                    for city in 0..game.cities.len() {
                        if game.cities[city].team == team {
                            game.open_city_interior(city);
                            game.close_city_interior();
                        }
                    }
                    game.plan_ai_turn(team);
                    // The AI gives workers already out on the map new jobs,
                    // which a player can't (`check_plan`): undo those.
                    for w in &mut game.field_workers {
                        if w.team == team {
                            let start =
                                game.lockstep.as_ref().unwrap().turn_start.as_ref().unwrap();
                            let before = start.field_workers.iter().find(|b| b.id == w.id);
                            if let Some(b) = before {
                                w.job = b.job;
                                w.work_left = b.work_left;
                                w.base = b.base;
                                w.recalled = b.recalled;
                            }
                        }
                    }
                }
                play_turn(&mut host, &mut guests);
                for guest in &guests {
                    assert_eq!(guest.checksum(), host.checksum(), "turn {}", host.turn);
                }
            }
        }
    }

    #[test]
    fn a_plan_survives_the_wire() {
        let (host, _) = pair();
        let plan = host.team_plan(HOST_SEAT);
        assert_eq!(roundtrip(&Message::Plan(plan.clone())), Message::Plan(plan));
    }
}
