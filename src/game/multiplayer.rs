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
    Build, BuildUnit, Building, City, MAX_CITY_POPULATION, Priorities, Queued, Stock, grow_price,
    in_interior,
};
use super::hex::Hex;
use super::settings::Settings;
use super::terrain::Resource;
use super::unit::{Team, TurnOrder};
use super::workers::WorkerJob;

/// Bumped whenever a message or a plan changes shape, or the rules a turn
/// plays out by, or the map a seed generates (every machine builds the world
/// from its seed, `mapgen.rs`), so mismatched builds refuse each other
/// instead of desyncing.
pub const PROTOCOL_VERSION: u32 = 16;
/// The most of anything a plan may list (units, a queue, worked tiles...):
/// far past what play produces, and a bound on what a hostile peer can make
/// this machine process.
const MAX_PLAN_LIST: usize = 256;
/// Letters a join code is made of: no 0/O or 1/I to confuse.
const CODE_LETTERS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
const CODE_LENGTH: usize = 6;
/// The most of a refusal's reason a guest shows.
const MAX_REASON: usize = 120;

/// What the top bar says once this side's plan is sent, while it waits for
/// the others'.
pub(super) const WAITING_NOTICE: &str = "ORDERS SENT - THE WAITING BUTTON TAKES THEM BACK";

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
    /// Guest to host: its player took back ending `turn` (the one being
    /// planned), to change its orders and send a new `Plan`. The host drops
    /// the plan it has from that side and waits for the new one, unless it
    /// has already resolved the turn: then the withdrawal (and the plan
    /// sent after it) came too late and changes nothing, and the turn plays
    /// out with the plan the host had.
    Withdraw { turn: u32 },
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
    /// On alert (`Unit::alert`): only for its own troops that can be.
    pub alert: bool,
    pub queued: Vec<TurnOrder>,
    pub following_queue: bool,
    pub waypoints: Vec<Hex>,
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
    /// Each item with whether it's paid for and its work (`Queued`).
    pub queue: Vec<Queued<Build>>,
    pub barracks_queue: Vec<Queued<BuildUnit>>,
    pub worked: Vec<Hex>,
    pub remembered_worked: Vec<Hex>,
    pub priorities: Priorities,
    pub worker_jobs: Vec<WorkerJob>,
    /// Of its workers at home, those held there (recalled, not yet
    /// released).
    pub held_workers: u32,
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

/// Whether a planned queue, `now`, could come of a city's queue as the turn
/// began, `before`, by planning alone: every unpaid item has no work, and
/// every paid item is one of `before`'s paid items, each at most once, with
/// the same build and work. Planning adds, removes and reorders items; only
/// the turn's economy pays for them or works on them.
fn keeps_what_was_paid<B: PartialEq>(before: &[Queued<B>], now: &[Queued<B>]) -> bool {
    let mut paid: Vec<&Queued<B>> = before.iter().filter(|q| q.paid).collect();
    now.iter().all(|item| {
        if !item.paid {
            return item.progress == 0;
        }
        match paid.iter().position(|was| *was == item) {
            Some(i) => {
                paid.swap_remove(i);
                true
            }
            None => false,
        }
    })
}

/// A city's queues and jobs as `committed` reads them: its population, its
/// queue, its Barracks' queue and its placed jobs.
type CityBooks<'a> = (
    usize,
    &'a [Queued<Build>],
    &'a [Queued<BuildUnit>],
    &'a [WorkerJob],
);

/// What a side has paid for and not yet got: every paid item in its cities'
/// queues and Barracks' queues and every job it has placed, at the price it
/// paid. Its city's paid Grows were priced a citizen apart, from its
/// population up (`GameState::item_price`). Unpaid items cost nothing yet.
fn committed<'a>(
    cities: impl Iterator<Item = CityBooks<'a>>,
    field_jobs: impl Iterator<Item = WorkerJob>,
) -> Stock {
    let mut total = Stock::default();
    for (population, queue, barracks, jobs) in cities {
        let mut grows = 0;
        for item in queue.iter().filter(|q| q.paid) {
            total += match item.build {
                Build::Grow => {
                    grows += 1;
                    grow_price(population + grows - 1)
                }
                build => build.price(),
            };
        }
        for item in barracks.iter().filter(|q| q.paid) {
            total += item.build.price();
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
    /// Host: the plans in for this turn, one a side at most.
    plans: Vec<TeamPlan>,
    /// Host: the turn it last resolved, and for each guest that hasn't yet
    /// sent its checksum for it, whether the latest plan it sent for it
    /// stands (not taken back). A guest that takes its turn back just as
    /// the host resolves it doesn't know it's too late until the `Resolve`
    /// arrives, so its `Withdraw`, and a `Plan` it sends again, may still
    /// come, alternately; they came too late and change nothing (`late`).
    late: (u32, Vec<(Team, bool)>),
    /// Guest: the plans it has sent for this turn (one, or more if its
    /// player took the turn back): the host's `Resolve` must carry one of
    /// them for its side.
    sent: Vec<TeamPlan>,
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
            late: (0, Vec::new()),
            sent: Vec::new(),
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

    /// Host: whether `team`'s plan for this turn is in.
    #[cfg(test)]
    pub fn has_plan_from(&self, team: Team) -> bool {
        let plans = self.lockstep.as_ref().map(|l| &l.plans[..]);
        plans.is_some_and(|plans| plans.iter().any(|p| p.team == team))
    }

    /// How many of `unit` `team`'s cities have queued.
    #[cfg(test)]
    pub fn units_queued(&self, team: Team, unit: BuildUnit) -> usize {
        let queued = self.cities.iter().filter(|c| c.team == team);
        let builds = queued.flat_map(|c| &c.queue);
        builds.filter(|q| q.build == Build::Unit(unit)).count()
    }

    /// Whether the first turn has resolved: after it, nobody can join.
    pub fn has_started(&self) -> bool {
        self.turn > 0
    }

    /// Whether this is a networked game.
    pub fn is_networked(&self) -> bool {
        self.lockstep.is_some()
    }

    /// While this side has ended planning and waits for the others' plans.
    /// Its plan is sent, so nothing may change it (`is_resolving` refuses
    /// every order), but the player may still look around: select units and
    /// cities and open their views, which waits only for `is_playing_out`.
    pub(super) fn waiting_for_peers(&self) -> bool {
        self.lockstep.as_ref().is_some_and(|l| l.submitted)
    }

    /// While a turn plays out: every input waits, looking around included.
    /// Anything that changes the plan waits for `is_resolving` instead, which
    /// also holds while a network game waits for the others' plans.
    pub fn is_playing_out(&self) -> bool {
        !self.pending_steps.is_empty()
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

    /// Whether messages from the other machines may be handed to `receive`
    /// now. A host takes none while a turn plays out: a guest that played
    /// it out sooner may already send its plan for the next, which can
    /// only be checked against the next turn's start, so it waits (`src/net`
    /// leaves it in the queue) until the host's planning begins. A guest
    /// always takes the host's: the next `Resolve` can't come before the
    /// guest's own plan.
    pub fn takes_messages(&self) -> bool {
        self.lockstep
            .as_ref()
            .is_none_or(|l| l.role == Role::Guest || l.turn_start.is_some())
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
        lockstep.late.1.retain(|&(t, _)| t != team);
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
            lockstep.sent.clear();
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
            Role::Guest => {
                lockstep.sent.push(plan.clone());
                lockstep.outbox.push(Message::Plan(plan));
            }
        }
        // The player may go on looking (the selection and open views stay),
        // but nothing armed to change the plan outlives it.
        self.ui_click_mode = None;
        self.placing_job = None;
        self.hovered_job = None;
        self.moving_manager = None;
        self.queue_drag = None;
        self.queue_replace_armed = None;
        self.disband_armed = None;
        self.notice = WAITING_NOTICE.into();
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
                if plan.team != from || from == HOST_SEAT || !self.is_human(from) {
                    return Err(format!("A PLAN FOR {:?}, NOT ITS OWN SIDE", plan.team));
                }
                if self.came_too_late(from, plan.turn, true)? {
                    return Ok(());
                }
                let lockstep = self.lockstep.as_ref().expect("networked");
                if lockstep.plans.iter().any(|p| p.team == from) {
                    return Err(format!("A SECOND PLAN FOR TURN {}", plan.turn));
                }
                self.check_plan(&plan)?;
                let lockstep = self.lockstep.as_mut().expect("networked");
                lockstep.plans.push(plan);
                self.resolve_when_ready();
            }
            (Role::Host, Message::Withdraw { turn }) => {
                if from == HOST_SEAT || !self.is_human(from) {
                    return Err(format!("{from:?} HAS NO TURN TO TAKE BACK"));
                }
                if self.came_too_late(from, turn, false)? {
                    return Ok(());
                }
                if self.planning_turn() != Some(turn) {
                    return Err(format!("TOOK BACK TURN {turn}"));
                }
                let lockstep = self.lockstep.as_mut().expect("networked");
                let Some(at) = lockstep.plans.iter().position(|p| p.team == from) else {
                    return Err(format!("TOOK BACK TURN {turn} WITHOUT ENDING IT"));
                };
                lockstep.plans.remove(at);
                self.notice = format!("{from:?} IS CHANGING THEIR ORDERS").to_uppercase();
            }
            (Role::Guest, Message::Resolve(plans)) => {
                // One plan for each human side, each one sound, and this
                // side's one it sent.
                let mut teams: Vec<Team> = plans.iter().map(|p| p.team).collect();
                teams.sort();
                let mut humans = self.humans.clone();
                humans.sort();
                if teams != humans {
                    return Err(format!("A TURN'S PLANS FOR {teams:?}"));
                }
                let lockstep = self.lockstep.as_ref().expect("networked");
                let own = plans.iter().find(|p| p.team == self.local_team);
                if !own.is_some_and(|p| lockstep.sent.contains(p)) {
                    return Err("A TURN WITH ORDERS WE NEVER SENT".into());
                }
                // Taken back, but too late: the host had them all, with
                // orders this side has since changed (or is changing).
                let too_late = !lockstep.submitted || lockstep.sent.last() != own;
                for plan in &plans {
                    self.check_plan(plan)?;
                }
                self.resolve_with_plans(plans);
                if too_late {
                    self.notice = "TOO LATE TO CHANGE - THE TURN PLAYS OUT AS SENT".into();
                }
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
                // It has played the turn out: nothing more comes for it.
                if lockstep.late.0 == turn {
                    lockstep.late.1.retain(|&(t, _)| t != from);
                }
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

    /// The turn whose planning is under way (the one the plans are for),
    /// unless one is playing out.
    fn planning_turn(&self) -> Option<u32> {
        let start = self.lockstep.as_ref()?.turn_start.as_deref()?;
        Some(start.turn + 1)
    }

    /// Host: whether a `Plan` (`plan`) or `Withdraw` from guest `from` for
    /// `turn` is for the turn it has just resolved, sent before the
    /// `Resolve` reached them: taken back too late, it changes nothing.
    /// Such messages come only in the order a player makes them, a
    /// withdrawal first (the turn resolved with their plan in), and stop
    /// once their checksum for the turn arrives; anything else is refused.
    fn came_too_late(&mut self, from: Team, turn: u32, plan: bool) -> Result<bool, String> {
        let lockstep = self.lockstep.as_mut().expect("networked");
        let (resolved, guests) = &mut lockstep.late;
        let Some((_, standing)) = guests.iter_mut().find(|(t, _)| *t == from) else {
            return Ok(false);
        };
        if turn != *resolved {
            return Ok(false);
        }
        if *standing == plan {
            return Err(if plan {
                format!("A SECOND PLAN FOR TURN {turn}")
            } else {
                format!("TOOK BACK TURN {turn} WITHOUT ENDING IT")
            });
        }
        *standing = plan;
        Ok(true)
    }

    /// Take back End Turn, while this side's plan waits for the others':
    /// the player can change their orders and end the turn again. On a
    /// guest the host is told (`Withdraw`), and if it already had every
    /// plan the turn plays out with the one sent; the host drops its own.
    pub fn take_back_turn(&mut self) {
        let (team, turn) = (self.local_team, self.turn + 1);
        let Some(lockstep) = self.lockstep.as_mut().filter(|l| l.submitted) else {
            return;
        };
        lockstep.submitted = false;
        match lockstep.role {
            Role::Host => lockstep.plans.retain(|p| p.team != team),
            Role::Guest => lockstep.outbox.push(Message::Withdraw { turn }),
        }
        self.notice = "TURN TAKEN BACK - CHANGE YOUR ORDERS, THEN END IT AGAIN".into();
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
                .chain(unit.waypoints.iter().copied())
                .collect::<Vec<_>>();
            if !short(unit.queued.len()) || !short(unit.waypoints.len()) || !hexes_on_map(&orders) {
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
            // On alert only a troop that can be (`can_go_on_alert`), with
            // the plan's ability (a siege setting up may): never a settler,
            // a scout or a ship.
            if unit.alert {
                let mut planned = body.clone();
                planned.ability_queued = unit.ability_queued;
                if !planned.alert_capable() || start.settlers.contains(&unit.id) {
                    return bad(format!("UNIT {} CAN'T GO ON ALERT", unit.id));
                }
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
            if !city.priorities.is_order() {
                return bad(format!(
                    "{at}: A PRIORITY ORDER THAT ISN'T FOOD, WOOD AND METAL"
                ));
            }
            let before = start.cities.iter().find(|c| c.pos == city.pos);
            let population = before.map_or(1, |c| c.population);
            // Planning pays for nothing and works on nothing: that happens as
            // the turn plays out (`work_queues`). What it queues is unpaid,
            // with no work, and every paid item it keeps is one this city
            // had paid for, with the work it had. It never adds work or
            // marks an item paid.
            let (queue, barracks_queue) = before.map_or((&[][..], &[][..]), |c| {
                (&c.queue[..], &c.barracks_queue[..])
            });
            if !keeps_what_was_paid(queue, &city.queue)
                || !keeps_what_was_paid(barracks_queue, &city.barracks_queue)
            {
                return bad(format!("{at}: PAYMENT OR PROGRESS IT DIDN'T MAKE"));
            }
            // Planning releases held workers; only coming home recalled
            // holds one, as the turn plays out.
            if city.held_workers > before.map_or(0, |c| c.held_workers) {
                return bad(format!("{at}: WORKERS IT DIDN'T HOLD"));
            }
            // Its queue holds what a city trains: no Cavalry or Armored,
            // ships only with a Harbor, no growing past the cap.
            let harbor = before.is_some_and(|c| c.placed_site(Building::Harbor).is_some());
            let trainable = |item: &Queued<Build>| match item.build {
                Build::Unit(unit) => {
                    unit.required_resource().is_none() && (!unit.unit_type().is_naval() || harbor)
                }
                _ => true,
            };
            let grows = city.queue.iter().filter(|q| q.build == Build::Grow).count();
            if !city.queue.iter().all(trainable) || population + grows > MAX_CITY_POPULATION {
                return bad(format!("{at}: A BUILD IT CAN'T MAKE"));
            }
            let barracks = before.is_some_and(|c| c.barracks.is_some());
            if !city.barracks_queue.is_empty()
                && (!barracks
                    || city
                        .barracks_queue
                        .iter()
                        .any(|q| q.build.unit_type().is_naval()))
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
            let queued = |queues: &mut dyn Iterator<Item = &Queued<BuildUnit>>| {
                queues
                    .filter(|q| q.build.required_resource() == Some(resource))
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
        // Nothing comes free. Planning pays for no queued item (the check
        // above), placing a job pays its price, and taking a paid item or a
        // job off refunds what was paid, so what the side holds plus every
        // paid item and placed job is worth exactly what it held and had
        // paid for when the turn began. Unpaid items count for nothing.
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
        // Every guest's plan stands in it, whatever they send before the
        // `Resolve` reaches them (`came_too_late`).
        let guests = humans.iter().filter(|&&t| t != HOST_SEAT);
        lockstep.late = (self.turn + 1, guests.map(|&t| (t, true)).collect());
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
        // What was open while waiting belongs to the game being replaced;
        // closing the interior gives the map's camera back.
        self.leave_city_view();
        self.selected = None;
        self.group.clear();
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
                alert: u.alert,
                queued: u.queued.clone(),
                waypoints: u.waypoints.clone(),
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
                    barracks_queue: c.barracks_queue.clone(),
                    worked: c.worked.clone(),
                    remembered_worked: c.remembered_worked.clone(),
                    priorities: c.priorities,
                    worker_jobs: c.worker_jobs.clone(),
                    held_workers: c.held_workers,
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
            unit.alert = unit_plan.alert;
            unit.queued = unit_plan.queued.clone();
            unit.waypoints = unit_plan.waypoints.clone();
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
            city.barracks_queue = city_plan.barracks_queue.clone();
            city.worked = city_plan.worked.clone();
            city.remembered_worked = city_plan.remembered_worked.clone();
            city.priorities = city_plan.priorities;
            city.worker_jobs = city_plan.worker_jobs.clone();
            city.held_workers = city_plan.held_workers;
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
    /// workers, what's built on the map and how much of it each side's own
    /// memory holds (`side_fog`, which the AI plans on). Views (camera, the
    /// player's fog memory, panels) aren't in it.
    pub fn checksum(&self) -> u64 {
        let mut h = DefaultHasher::new();
        self.turn.hash(&mut h);
        let mut units: Vec<_> = self.units.iter().collect();
        units.sort_by_key(|u| u.id);
        for u in units {
            (u.id, u.team, u.pos, u.hp.to_bits(), u.interior_hp.to_bits()).hash(&mut h);
            (u.ability_cooldown, u.deployed, u.cargo.len()).hash(&mut h);
            u.alert.hash(&mut h);
        }
        for c in &self.cities {
            (c.id, c.team, c.pos, c.population, c.workers).hash(&mut h);
            c.held_workers.hash(&mut h);
            (c.barracks, c.barracks_hp.to_bits()).hash(&mut h);
            for q in &c.queue {
                (q.paid, q.progress).hash(&mut h);
            }
            for q in &c.barracks_queue {
                (q.paid, q.progress).hash(&mut h);
            }
            c.worked.hash(&mut h);
            c.priorities.hash(&mut h);
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
        for memory in &self.side_memory {
            memory.len().hash(&mut h);
        }
        h.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::city::Good;

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

    /// The window a test clicks in.
    const SCREEN: glam::Vec2 = glam::Vec2::new(1600.0, 900.0);

    /// A left click on `hex` on the map, as `App` passes it on.
    fn click(game: &mut GameState, hex: Hex, mode: super::super::ClickMode) {
        let camera = &game.camera;
        let offset = (hex.to_world() - camera.center) / camera.half_height;
        let ndc = glam::Vec2::new(offset.x * SCREEN.y / SCREEN.x, -offset.y);
        let cursor = (ndc + 1.0) / 2.0 * SCREEN;
        game.handle_map_click(cursor, SCREEN, mode);
    }

    #[test]
    fn waiting_for_the_others_a_player_looks_around_but_changes_nothing() {
        use super::super::ClickMode;
        let (mut host, mut guest) = pair();
        let team = GUEST_SEAT;
        let city = guest.cities.iter().position(|c| c.team == team).unwrap();
        let city_pos = guest.cities[city].pos;
        // A Barracks by its city (not on a tile a citizen works), on both
        // machines, as an earlier turn would leave it.
        let worked = guest.cities[city].worked.clone();
        let barracks = city_pos
            .neighbors()
            .into_iter()
            .find(|&h| guest.grid.is_passable(h) && !guest.is_occupied(h) && !worked.contains(&h))
            .unwrap();
        for game in [&mut host, &mut guest] {
            game.cities[city].barracks = Some(barracks);
            game.finish_lockstep_turn();
            let _ = game.take_outbox();
        }
        // What Red sees, as a frame would show it.
        guest.update(0.0);
        // The guest plans: a unit's move and a build, then ends its turn.
        let units: Vec<usize> = (0..guest.units.len())
            .filter(|&i| {
                let u = &guest.units[i];
                u.team == team && u.pos != city_pos && u.pos != barracks
            })
            .collect();
        assert!(!units.is_empty(), "a unit of Red's out on the map");
        let unit = units[0];
        let pos = guest.units[unit].pos;
        let open = |game: &GameState, around: Hex| {
            around
                .neighbors()
                .into_iter()
                .find(|&h| {
                    game.grid.is_passable(h)
                        && !game.is_occupied(h)
                        && game
                            .cities
                            .iter()
                            .all(|c| c.pos != h && c.barracks != Some(h))
                })
                .unwrap()
        };
        guest.units[unit].planned_move = Some(open(&guest, pos));
        guest.open_city(city);
        guest.queue_selected_city_unit(BuildUnit::Melee);
        guest.end_planning();
        assert!(guest.waiting_for_peers(), "{}", guest.notice);
        let sent = guest.take_outbox();
        let [Message::Plan(sent_plan)] = &sent[..] else {
            panic!("{sent:?}")
        };
        let plan = guest.team_plan(team);
        assert_eq!(&plan, sent_plan);
        let checksum = guest.checksum();
        let unchanged = |game: &GameState, what: &str| {
            assert_eq!(game.team_plan(team), plan, "{what} changed the plan");
            assert_eq!(game.checksum(), checksum, "{what} changed the game");
            assert!(game.waiting_for_peers(), "{what}");
        };

        // A unit, selected by clicking it; a click elsewhere only lets go.
        let away = open(&guest, pos);
        guest.press_escape();
        click(&mut guest, pos, ClickMode::Normal);
        assert_eq!(guest.selected, Some(unit), "{}", guest.notice);
        click(&mut guest, away, ClickMode::Normal);
        assert_eq!(guest.selected, None);
        unchanged(&guest, "a click on an open hex");
        click(&mut guest, pos, ClickMode::Normal);
        // Every order for it is refused.
        guest.choose_move_action();
        assert_eq!(guest.ui_click_mode, None);
        click(&mut guest, away, ClickMode::Move);
        guest.selected = Some(unit);
        guest.toggle_selected_ability();
        guest.hold_selected_unit();
        guest.toggle_guard();
        guest.toggle_alert();
        guest.disband_selected();
        guest.disband_selected();
        guest.found_city_selected();
        guest.handle_right_click();
        guest.hold_or_end_turn();
        unchanged(&guest, "a unit's orders");
        // Tab picks a unit, and a box picks every one in it.
        guest.set_selection(Vec::new());
        guest.select_next_unit();
        assert!(guest.selected.is_some());
        guest.camera.focus_on(pos.to_world());
        guest.camera.update(10.0);
        guest.select_in_box(glam::Vec2::ZERO, SCREEN, SCREEN, false);
        assert!(guest.selection().contains(&unit));
        unchanged(&guest, "selecting");

        // Its city opens (C), and nothing in it changes.
        guest.select_city();
        assert_eq!(guest.selected_city, Some(city));
        guest.queue_selected_city_unit(BuildUnit::Ranged);
        guest.queue_selected_city_gather();
        guest.queue_selected_city_growth();
        guest.queue_selected_city_worker();
        guest.queue_selected_city_building(Building::Mill);
        guest.move_selected_city_queue_item(0, false);
        guest.remove_selected_city_queue_item(0);
        guest.clear_selected_city_queue();
        guest.auto_assign_selected_city();
        guest.prioritize_selected_city(Good::Metal);
        guest.set_selected_city_priorities(Priorities([Good::Wood, Good::Food, Good::Metal]));
        guest.arm_worker_job(super::super::JobKind::Road);
        assert_eq!(guest.placing_job, None);
        guest.release_worker();
        // Clicks on its tiles don't move citizens (or its manager, picked
        // up from its tile).
        let worked = guest.cities[city].worked.clone();
        for &hex in worked.iter().chain(&city_pos.neighbors()) {
            click(&mut guest, hex, ClickMode::Normal);
            guest.open_city(city);
        }
        unchanged(&guest, "the city view");
        assert_eq!(guest.selected_city, Some(city));

        // The interior: a troop there can be picked, not ordered.
        guest.open_city(city);
        click(&mut guest, city_pos, ClickMode::Normal);
        assert_eq!(guest.interior_view, Some(city), "{}", guest.notice);
        let fighters: Vec<(u32, Hex)> = guest.cities[city]
            .interior
            .fighters
            .iter()
            .filter(|f| f.team == team)
            .map(|f| (f.source_id, f.pos))
            .collect();
        if let Some(&(id, at)) = fighters.first() {
            guest.interior_click(at);
            assert_eq!(guest.interior_selected, Some(id));
            for hex in at.neighbors() {
                guest.interior_click(hex);
                guest.interior_selected = Some(id);
            }
            guest.clear_selected_interior_orders();
        }
        guest.press_escape();
        assert_eq!(guest.interior_view, None);
        unchanged(&guest, "the interior");

        // The Barracks, by clicking it.
        guest.press_escape();
        click(&mut guest, barracks, ClickMode::Normal);
        assert_eq!(guest.selected_barracks, Some(city), "{}", guest.notice);
        guest.queue_selected_barracks_unit(BuildUnit::Melee);
        guest.clear_selected_barracks_queue();
        unchanged(&guest, "the Barracks view");

        // The host gets the plan as sent, and both machines play the same
        // turn, the guest looking inside its city as it arrives.
        guest.open_city(city);
        let map_camera = (guest.camera.center, guest.camera.half_height);
        guest.toggle_city_interior();
        assert!(guest.interior_view.is_some());
        for message in sent {
            host.receive(GUEST_SEAT, roundtrip(&message)).unwrap();
        }
        assert_eq!(host.lockstep.as_ref().unwrap().plans, [plan]);
        host.submit_plan();
        exchange(&mut host, &mut guest);
        assert_eq!(guest.interior_view, None, "views close as the turn plays");
        assert!(guest.exterior_camera.is_none());
        assert_eq!(
            (guest.camera.center, guest.camera.half_height),
            map_camera,
            "the map's camera back"
        );
        play_out(&mut host);
        play_out(&mut guest);
        exchange(&mut host, &mut guest);
        assert_eq!(host.turn, 1);
        assert_eq!(host.checksum(), guest.checksum());
        let queued = &guest.cities[city].queue;
        assert_eq!(queued.len(), 1, "the one build sent");
    }

    /// `team`'s first city, by index.
    fn city_of(game: &GameState, team: Team) -> usize {
        game.cities.iter().position(|c| c.team == team).unwrap()
    }

    /// Ends `game`'s turn with `build` queued in its first city.
    fn end_turn_building(game: &mut GameState, build: Option<BuildUnit>) {
        let city = city_of(game, game.local_team);
        game.open_city(city);
        match build {
            Some(unit) => game.queue_selected_city_unit(unit),
            None => game.queue_selected_city_gather(),
        }
        game.end_planning();
        // Waiting for the others, or resolving if it was the last.
        assert!(game.is_resolving(), "{}", game.notice);
    }

    #[test]
    fn a_guest_takes_back_its_turn_and_its_new_orders_play_out_everywhere() {
        let (mut host, mut guests) = table(2);
        let red = Team::Red;
        // Red ends its turn, gathering.
        end_turn_building(&mut guests[0], None);
        exchange_all(&mut host, &mut guests);
        assert!(host.has_plan_from(red));
        // It takes the turn back: the host lets go of its plan and waits.
        guests[0].take_back_turn();
        assert!(!guests[0].waiting_for_peers() && !guests[0].is_resolving());
        exchange_all(&mut host, &mut guests);
        assert!(!host.has_plan_from(red));
        assert_eq!(host.notice, "RED IS CHANGING THEIR ORDERS");
        // Its player queues a Melee too and ends the turn again.
        end_turn_building(&mut guests[0], Some(BuildUnit::Melee));
        exchange_all(&mut host, &mut guests);
        assert!(host.has_plan_from(red));
        // Green and the host end theirs, and the turn plays out with Red's
        // new orders on every machine.
        guests[1].submit_plan();
        host.submit_plan();
        exchange_all(&mut host, &mut guests);
        play_out(&mut host);
        for guest in &mut guests {
            play_out(guest);
        }
        exchange_all(&mut host, &mut guests);
        for game in std::iter::once(&host).chain(&guests) {
            assert_eq!(game.turn, 1);
            assert_eq!(game.checksum(), host.checksum());
            assert_eq!(
                game.units_queued(red, BuildUnit::Melee),
                1,
                "the new orders"
            );
        }
        assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
    }

    #[test]
    fn the_host_takes_back_its_own_turn() {
        let (mut host, mut guest) = pair();
        end_turn_building(&mut host, None);
        assert!(host.has_plan_from(HOST_SEAT));
        host.take_back_turn();
        assert!(!host.waiting_for_peers() && !host.has_plan_from(HOST_SEAT));
        // The guest's plan alone doesn't resolve the turn now.
        end_turn_building(&mut guest, None);
        exchange(&mut host, &mut guest);
        assert!(!host.is_playing_out(), "waiting for the host's plan");
        end_turn_building(&mut host, Some(BuildUnit::Melee));
        exchange(&mut host, &mut guest);
        play_out(&mut host);
        play_out(&mut guest);
        exchange(&mut host, &mut guest);
        assert_eq!(host.turn, 1);
        assert_eq!(host.checksum(), guest.checksum());
        assert_eq!(guest.units_queued(HOST_SEAT, BuildUnit::Melee), 1);
    }

    #[test]
    fn a_take_back_after_the_host_resolved_changes_nothing() {
        let (mut host, mut guest) = pair();
        end_turn_building(&mut guest, None);
        for m in guest.take_outbox() {
            host.receive(GUEST_SEAT, roundtrip(&m)).unwrap();
        }
        // The host ends its turn: it has every plan, and resolves.
        host.submit_plan();
        assert!(host.is_playing_out());
        // Red, before the `Resolve` reaches it, takes its turn back and ends
        // it again with a Melee queued.
        guest.take_back_turn();
        end_turn_building(&mut guest, Some(BuildUnit::Melee));
        let late = guest.take_outbox();
        assert!(matches!(
            &late[..],
            [Message::Withdraw { turn: 1 }, Message::Plan(_)]
        ));
        // They wait while the host plays the turn out (`takes_messages`),
        // then come too late to change anything, but aren't hostile.
        assert!(!host.takes_messages());
        play_out(&mut host);
        assert!(host.takes_messages());
        let resolved = host.checksum();
        for m in late {
            host.receive(GUEST_SEAT, roundtrip(&m))
                .expect("too late, not hostile");
        }
        assert_eq!(host.checksum(), resolved);
        assert!(!host.has_plan_from(GUEST_SEAT), "nothing for the next turn");
        // Red plays the turn out as the host resolved it, with its first
        // orders, and says so.
        exchange(&mut host, &mut guest);
        assert_eq!(
            guest.notice,
            "TOO LATE TO CHANGE - THE TURN PLAYS OUT AS SENT"
        );
        play_out(&mut guest);
        exchange(&mut host, &mut guest);
        assert_eq!(host.turn, 1);
        assert_eq!(host.checksum(), guest.checksum());
        assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
        for game in [&host, &guest] {
            assert_eq!(game.units_queued(GUEST_SEAT, BuildUnit::Melee), 0);
        }
        // Its checksum is in: nothing more comes for that turn.
        let why = host
            .receive(GUEST_SEAT, Message::Withdraw { turn: 1 })
            .expect_err("refused");
        assert!(why.contains("TOOK BACK TURN 1"), "{why}");
    }

    #[test]
    fn a_take_back_or_plan_out_of_turn_is_refused() {
        let (host, guest) = pair();
        let plan = guest.team_plan(GUEST_SEAT);
        let withdraw = |turn| Message::Withdraw { turn };
        let refused =
            |host: &mut GameState, m: Message| host.receive(GUEST_SEAT, m).expect_err("refused");
        let with_plan = || {
            let mut host = host.clone();
            host.receive(GUEST_SEAT, Message::Plan(plan.clone()))
                .unwrap();
            host
        };
        // Nothing to take back.
        let why = refused(&mut host.clone(), withdraw(1));
        assert!(why.contains("WITHOUT ENDING IT"), "{why}");
        // A second plan without taking the first back: two plans from one
        // side never stand at once.
        let why = refused(&mut with_plan(), Message::Plan(plan.clone()));
        assert!(why.contains("A SECOND PLAN"), "{why}");
        // Taking back another turn, or the same one twice.
        refused(&mut with_plan(), withdraw(0));
        refused(&mut with_plan(), withdraw(2));
        let mut again = with_plan();
        again.receive(GUEST_SEAT, withdraw(1)).unwrap();
        assert!(!again.has_plan_from(GUEST_SEAT));
        refused(&mut again, withdraw(1));
        // Taken back, a new plan comes in as the first did.
        let mut again = with_plan();
        again.receive(GUEST_SEAT, withdraw(1)).unwrap();
        again
            .receive(GUEST_SEAT, Message::Plan(plan.clone()))
            .unwrap();
        assert_eq!(
            again.lockstep.as_ref().unwrap().plans,
            std::slice::from_ref(&plan)
        );
        // A host's message only goes one way.
        let mut red = guest.clone();
        assert!(red.receive(HOST_SEAT, withdraw(1)).is_err());
        // After the resolve, too late: a plan sent again must follow a
        // take-back, and a take-back a plan.
        let mut resolved = with_plan();
        resolved.submit_plan();
        play_out(&mut resolved);
        let why = refused(&mut resolved.clone(), Message::Plan(plan.clone()));
        assert!(why.contains("A SECOND PLAN"), "{why}");
        let mut late = resolved.clone();
        late.receive(GUEST_SEAT, withdraw(1)).unwrap();
        refused(&mut late, withdraw(1));
        let mut late = resolved.clone();
        late.receive(GUEST_SEAT, withdraw(1)).unwrap();
        late.receive(GUEST_SEAT, Message::Plan(plan.clone()))
            .unwrap();
        refused(&mut late, Message::Plan(plan.clone()));
        // And a take-back for the turn now being planned, with nothing sent.
        refused(&mut resolved, withdraw(2));
    }

    #[test]
    fn a_guest_refuses_a_turn_with_orders_it_never_sent() {
        let (mut host, mut guest) = pair();
        end_turn_building(&mut guest, None);
        let sent = guest.team_plan(GUEST_SEAT);
        let own = host.team_plan(HOST_SEAT);
        // Its orders changed on the way, though they'd pass the checks.
        let mut changed = sent.clone();
        changed.units[0].holding = !changed.units[0].holding;
        assert_eq!(host.check_plan(&changed), Ok(()));
        let before = guest.checksum();
        let why = guest
            .receive(HOST_SEAT, Message::Resolve(vec![own.clone(), changed]))
            .expect_err("refused");
        assert!(why.contains("NEVER SENT"), "{why}");
        assert_eq!(guest.checksum(), before);
        assert!(guest.waiting_for_peers());
        // Its own go through.
        guest
            .receive(HOST_SEAT, Message::Resolve(vec![own, sent]))
            .unwrap();
        assert!(guest.is_playing_out());
        let _ = host.take_outbox();
    }

    /// However a host and a guest interleave ending turns, taking them back
    /// and their messages, every honest message is taken and every machine
    /// plays the same turn; and whatever a hostile guest sends among them,
    /// the host never holds two plans from one side, nor panics.
    #[test]
    fn no_order_of_ending_and_taking_back_turns_confuses_the_host() {
        use rand::{RngExt, SeedableRng};
        let (host, guest) = pair();
        let unit = guest
            .units
            .iter()
            .position(|u| u.team == GUEST_SEAT)
            .unwrap();
        let near: Vec<Option<Hex>> = std::iter::once(None)
            .chain(guest.units[unit].pos.neighbors().map(Some))
            .collect();
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        for trial in 0..300 {
            let hostile = trial % 2 == 1;
            let (mut host, mut guest) = (host.clone(), guest.clone());
            // What's on its way: to the host, and to the guest.
            let (mut up, mut down) = (Vec::new(), Vec::new());
            let mut refused = false;
            let mut log = Vec::new();
            for _ in 0..rng.random_range(4..16) {
                let event = rng.random_range(0..8);
                log.push(event);
                match event {
                    0 if !guest.is_resolving() => {
                        guest.units[unit].planned_move = near[rng.random_range(0..near.len())];
                        guest.submit_plan();
                    }
                    1 => guest.take_back_turn(),
                    2 if !host.is_resolving() => host.submit_plan(),
                    3 => host.take_back_turn(),
                    4 | 5 if host.takes_messages() && !up.is_empty() => {
                        let m: Message = up.remove(0);
                        match host.receive(GUEST_SEAT, roundtrip(&m)) {
                            // After a forged message, the honest ones may not fit.
                            Err(_) if hostile => refused = true,
                            Err(why) => panic!("trial {trial}: an honest {m:?} refused: {why}"),
                            Ok(()) => {}
                        }
                    }
                    6 if !down.is_empty() => {
                        let m: Message = down.remove(0);
                        let taken = guest.receive(HOST_SEAT, roundtrip(&m));
                        assert!(hostile || taken.is_ok(), "trial {trial}: {taken:?}");
                    }
                    7 if hostile => {
                        let forged = match rng.random_range(0..3) {
                            0 => Message::Withdraw {
                                turn: rng.random_range(0..3),
                            },
                            1 => Message::Plan(TeamPlan {
                                turn: rng.random_range(0..3),
                                ..guest.team_plan(GUEST_SEAT)
                            }),
                            _ => Message::Checksum {
                                turn: rng.random_range(0..3),
                                value: 0,
                            },
                        };
                        if host.receive(GUEST_SEAT, forged).is_err() {
                            refused = true;
                        }
                    }
                    _ => {}
                }
                up.extend(guest.take_outbox());
                down.extend(host.take_outbox());
                let plans = &host.lockstep.as_ref().unwrap().plans;
                let mut teams: Vec<Team> = plans.iter().map(|p| p.team).collect();
                teams.dedup();
                assert_eq!(teams.len(), plans.len(), "trial {trial}: one plan a side");
                if refused {
                    // The host drops a guest it refuses a message from.
                    break;
                }
                if host.is_playing_out() {
                    play_out(&mut host);
                }
            }
            if hostile {
                continue;
            }
            // Whatever's left arrives, and whoever hasn't ended the turn
            // does, until it has played out everywhere.
            for _ in 0..8 {
                let played = |g: &GameState| g.turn == 1 && !g.is_playing_out();
                if played(&host) && played(&guest) {
                    break;
                }
                if !guest.is_resolving() && guest.turn == 0 {
                    guest.submit_plan();
                }
                if !host.is_resolving() && host.turn == 0 {
                    host.submit_plan();
                }
                up.extend(guest.take_outbox());
                down.extend(host.take_outbox());
                for m in up.drain(..) {
                    // As `src/net` hands them over: once the host is planning.
                    play_out(&mut host);
                    if let Err(why) = host.receive(GUEST_SEAT, roundtrip(&m)) {
                        panic!("trial {trial}: an honest {m:?} refused: {why}");
                    }
                }
                down.extend(host.take_outbox());
                for m in down.drain(..) {
                    guest.receive(HOST_SEAT, roundtrip(&m)).unwrap();
                }
                play_out(&mut guest);
                play_out(&mut host);
            }
            up.extend(guest.take_outbox());
            for m in up.drain(..) {
                host.receive(GUEST_SEAT, roundtrip(&m)).unwrap();
            }
            assert_eq!((host.turn, guest.turn), (1, 1), "trial {trial}");
            assert_eq!(host.checksum(), guest.checksum(), "trial {trial}: {log:?}");
            assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
        }
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

    /// The guest's city with queues part paid for and under way, as earlier
    /// turns' economies leave them, the same on both machines as this
    /// turn's planning begins; and the city's index.
    fn paid_queues(host: &mut GameState, guest: &mut GameState) -> usize {
        let city = guest
            .cities
            .iter()
            .position(|c| c.team == GUEST_SEAT)
            .unwrap();
        // Beside the city, on no tile its citizens work.
        let c = &guest.cities[city];
        let barracks = c
            .pos
            .neighbors()
            .into_iter()
            .find(|h| guest.grid.is_passable(*h) && !c.worked.contains(h))
            .unwrap();
        let start = host.lockstep.as_mut().unwrap().turn_start.as_mut().unwrap();
        for c in [&mut guest.cities[city], &mut start.cities[city]] {
            c.barracks = Some(barracks);
            c.queue = vec![
                Queued::worked(Build::Unit(BuildUnit::Melee), 4),
                Queued::prepaid(Build::Grow),
                Queued::new(Build::Worker),
                Queued::prepaid(Build::Grow),
                Queued::new(Build::Unit(BuildUnit::Siege)),
            ];
            c.barracks_queue = vec![
                Queued::new(BuildUnit::Ranged),
                Queued::worked(BuildUnit::Melee, 2),
            ];
        }
        city
    }

    #[test]
    fn planning_keeps_what_was_paid_and_pays_for_nothing() {
        let (mut host, mut guest) = pair();
        let city = paid_queues(&mut host, &mut guest);
        let good = guest.team_plan(GUEST_SEAT);
        assert_eq!(host.check_plan(&good), Ok(()));
        let c = good
            .cities
            .iter()
            .position(|c| c.pos == guest.cities[city].pos)
            .unwrap();
        let refused = |host: &GameState, plan: &TeamPlan| {
            let why = host.check_plan(plan).expect_err("refused");
            assert!(
                why.contains("PAYMENT OR PROGRESS") || why.contains("SPENDING"),
                "{why}"
            );
        };
        // Paying as it queues: the Siege marked paid, with or without its
        // price off the stockpile.
        let mut plan = good.clone();
        plan.cities[c].queue[4].paid = true;
        refused(&host, &plan);
        plan.stock -= BuildUnit::Siege.price();
        refused(&host, &plan);
        // Work it didn't do, on a paid item or an unpaid one.
        let mut plan = good.clone();
        plan.cities[c].queue[0].progress += 4;
        refused(&host, &plan);
        let mut plan = good.clone();
        plan.cities[c].barracks_queue[0].progress = 1;
        refused(&host, &plan);
        // A paid item kept but unpaid, keeping its work.
        let mut plan = good.clone();
        plan.cities[c].queue[0].paid = false;
        refused(&host, &plan);
        // A paid item copied.
        let mut plan = good.clone();
        let copy = plan.cities[c].queue[1];
        plan.cities[c].queue.push(copy);
        refused(&host, &plan);
        // A refund for an unpaid item taken off.
        let mut plan = good.clone();
        plan.cities[c].queue.remove(2);
        plan.stock += Build::Worker.price();
        refused(&host, &plan);

        // Taken off paid, the Melee refunds its price, and a paid Grow the
        // dearer Grow's: the checks take both, and nothing more.
        guest.open_city(city);
        guest.remove_selected_city_queue_item(0);
        guest.remove_selected_city_queue_item(0);
        let plan = guest.team_plan(GUEST_SEAT);
        assert_eq!(host.check_plan(&plan), Ok(()));
        let population = guest.cities[city].population;
        assert_eq!(
            plan.stock,
            good.stock + BuildUnit::Melee.price() + grow_price(population + 1)
        );
        let mut greedy = plan.clone();
        greedy.stock.food += 1;
        refused(&host, &greedy);
    }

    /// Whatever the player does to its queues, the plan passes; and any
    /// tampering with what's paid, the work done or the stockpile on top of
    /// that is refused.
    #[test]
    fn no_plan_pays_early_or_gets_a_build_for_free() {
        use rand::{RngExt, SeedableRng};
        let (mut host, mut guest) = pair();
        let city = paid_queues(&mut host, &mut guest);
        guest.fund(GUEST_SEAT);
        host.lockstep
            .as_mut()
            .unwrap()
            .turn_start
            .as_mut()
            .unwrap()
            .fund(GUEST_SEAT);
        let mut rng = rand::rngs::StdRng::seed_from_u64(11);
        for round in 0..400 {
            let mut game = guest.clone();
            game.open_city(city);
            for _ in 0..rng.random_range(0..8) {
                let len = game.cities[city].queue.len();
                let troops = game.cities[city].barracks_queue.len();
                match rng.random_range(0..9) {
                    0 => game.queue_selected_city_unit(BuildUnit::Melee),
                    1 => game.queue_selected_city_growth(),
                    2 => game.queue_selected_city_worker(),
                    3 => game.queue_selected_city_gather(),
                    4 => game.queue_selected_barracks_unit(BuildUnit::Siege),
                    5 if len > 0 => game.remove_selected_city_queue_item(rng.random_range(0..len)),
                    6 if troops > 0 => {
                        game.remove_selected_barracks_queue_item(rng.random_range(0..troops))
                    }
                    7 if len > 1 => {
                        game.move_selected_city_queue_item(rng.random_range(1..len), true)
                    }
                    8 => game.clear_selected_city_queue(),
                    _ => {}
                }
            }
            let plan = game.team_plan(GUEST_SEAT);
            assert_eq!(host.check_plan(&plan), Ok(()), "round {round}");
            let c = plan
                .cities
                .iter()
                .position(|c| c.pos == game.cities[city].pos)
                .unwrap();
            let mut tampered = plan.clone();
            let queue = &mut tampered.cities[c].queue;
            let unpaid: Vec<usize> = (0..queue.len()).filter(|&i| !queue[i].paid).collect();
            let paid: Vec<usize> = (0..queue.len()).filter(|&i| queue[i].paid).collect();
            match rng.random_range(0..5) {
                0 if !unpaid.is_empty() => {
                    let i = unpaid[rng.random_range(0..unpaid.len())];
                    queue[i].paid = true;
                    if rng.random_bool(0.5) {
                        let price = queue[i].build.price();
                        tampered.stock -= price;
                    }
                }
                1 if !queue.is_empty() => {
                    let i = rng.random_range(0..queue.len());
                    queue[i].progress += rng.random_range(1..8);
                }
                2 if !paid.is_empty() => {
                    let copy = queue[paid[rng.random_range(0..paid.len())]];
                    queue.push(copy);
                }
                3 if !paid.is_empty() => {
                    // Unpaid, but no refund for it.
                    let i = paid[rng.random_range(0..paid.len())];
                    queue[i].paid = false;
                    queue[i].progress = 0;
                }
                _ => {
                    let mut coin = Stock::default();
                    match rng.random_range(0..3) {
                        0 => coin.food = 1,
                        1 => coin.wood = 1,
                        _ => coin.metal = 1,
                    }
                    tampered.stock += coin;
                }
            }
            assert!(
                host.check_plan(&tampered).is_err(),
                "round {round}: {tampered:?}"
            );
        }
    }

    #[test]
    fn a_plan_releases_held_workers_but_never_holds_more() {
        let (mut host, mut guest) = pair();
        let city = guest
            .cities
            .iter()
            .position(|c| c.team == GUEST_SEAT)
            .unwrap();
        // The guest's worker came home recalled: held, on both machines.
        assert_eq!(guest.cities[city].workers, 1);
        guest.cities[city].held_workers = 1;
        let start = host.lockstep.as_mut().unwrap().turn_start.as_mut().unwrap();
        start.cities[city].held_workers = 1;
        let held = guest.team_plan(GUEST_SEAT);
        assert_eq!(host.check_plan(&held), Ok(()));
        guest.selected_city = Some(city);
        guest.release_worker();
        let released = guest.team_plan(GUEST_SEAT);
        assert_eq!(host.check_plan(&released), Ok(()));
        // Holding one more than came home.
        let mut plan = held;
        let c = plan
            .cities
            .iter()
            .position(|c| c.held_workers == 1)
            .unwrap();
        plan.cities[c].held_workers = 2;
        let why = host
            .receive(GUEST_SEAT, Message::Plan(plan))
            .expect_err("refused");
        assert!(why.contains("WORKERS IT DIDN'T HOLD"), "{why}");
        // The release carries to the host.
        host.receive(GUEST_SEAT, Message::Plan(released.clone()))
            .unwrap();
        let mut applied = host.lockstep.as_ref().unwrap().turn_start.clone().unwrap();
        applied.apply_plan(&released);
        assert_eq!(applied.cities[city].held_workers, 0);
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
        // A build marked paid that it never paid for, and one paid for as
        // it's queued: only the turn's economy pays, as work starts.
        let mut plan = good.clone();
        plan.cities[0]
            .queue
            .push(Queued::prepaid(Build::Unit(BuildUnit::Siege)));
        refused(&mut host, plan.clone());
        plan.stock -= BuildUnit::Siege.price();
        refused(&mut host, plan);
        // Progress it didn't make, a troop a city can't train, and tiles
        // out of its reach.
        let mut plan = good.clone();
        plan.cities[0].queue = vec![Queued {
            progress: 999,
            ..Queued::new(Build::Gather)
        }];
        refused(&mut host, plan.clone());
        plan.cities[0].queue[0].paid = true;
        refused(&mut host, plan);
        let mut plan = good.clone();
        plan.cities[0]
            .queue
            .push(Queued::new(Build::Unit(BuildUnit::Cavalry)));
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
        plan.cities[0].queue = vec![Queued::new(Build::Gather); MAX_PLAN_LIST + 1];
        refused(&mut host, plan);
        // A priority order that isn't each good once.
        let mut plan = good.clone();
        plan.cities[0].priorities = Priorities([Good::Wood, Good::Wood, Good::Metal]);
        let why = refused(&mut host, plan);
        assert!(why.contains("PRIORITY ORDER"), "{why}");
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
                unit.alert = rng.random_bool(0.1);
                unit.following_queue = rng.random_bool(0.3);
                unit.ability_queued = rng.random_bool(0.3);
            }
            for city in &mut plan.cities {
                city.queue = (0..rng.random_range(0..4))
                    .map(|_| Queued {
                        build: builds[rng.random_range(0..builds.len())],
                        paid: rng.random_bool(0.1),
                        progress: if rng.random_bool(0.1) { 4 } else { 0 },
                    })
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
    fn an_alert_goes_in_the_plan_only_for_its_own_troops_that_can() {
        use super::super::unit::{Unit, UnitType};
        let (mut host, mut guest) = pair();
        // A melee troop of the guest's, on both machines, from the turn's
        // start.
        let scout = guest
            .units
            .iter()
            .position(|u| u.team == GUEST_SEAT && u.unit_type == UnitType::Scout)
            .unwrap();
        let open = guest.units[scout]
            .pos
            .neighbors()
            .into_iter()
            .find(|&h| {
                guest.grid.is_passable(h)
                    && !guest.is_occupied(h)
                    && guest.cities.iter().all(|c| c.pos != h)
            })
            .unwrap();
        let id = guest.next_unit_id;
        for game in [&mut host, &mut guest] {
            game.units
                .push(Unit::new(id, open, GUEST_SEAT, UnitType::Melee));
            game.next_unit_id += 1;
            game.begin_lockstep_turn();
        }
        let melee = guest.units.len() - 1;
        guest.set_selection(vec![melee]);
        guest.toggle_alert();
        assert!(guest.units[melee].alert);
        let plan = guest.team_plan(GUEST_SEAT);
        assert!(plan.units.iter().any(|u| u.id == id && u.alert));
        assert_eq!(
            roundtrip(&Message::Plan(plan.clone())),
            Message::Plan(plan.clone())
        );
        assert_eq!(host.check_plan(&plan), Ok(()));
        let mut applied = host.clone();
        applied.apply_plan(&plan);
        assert!(applied.units.iter().any(|u| u.id == id && u.alert));

        let alert_on = |plan: &TeamPlan, unit: u32| {
            let mut plan = plan.clone();
            plan.units.iter_mut().find(|u| u.id == unit).unwrap().alert = true;
            plan
        };
        // Not a scout.
        let why = host
            .check_plan(&alert_on(&plan, guest.units[scout].id))
            .unwrap_err();
        assert!(why.contains("CAN'T GO ON ALERT"), "{why}");
        // Not a unit of another side's.
        let mut foreign = plan.clone();
        let mut stolen = host.team_plan(HOST_SEAT).units[0].clone();
        stolen.alert = true;
        foreign.units.push(stolen);
        assert!(host.check_plan(&foreign).is_err());
        // Not a settler, though it's a melee body.
        let start = host.lockstep.as_mut().unwrap().turn_start.as_mut().unwrap();
        start.settlers.insert(id);
        let why = host.check_plan(&plan).unwrap_err();
        assert!(why.contains("CAN'T GO ON ALERT"), "{why}");
    }

    #[test]
    fn a_plan_survives_the_wire() {
        let (host, _) = pair();
        let plan = host.team_plan(HOST_SEAT);
        assert_eq!(roundtrip(&Message::Plan(plan.clone())), Message::Plan(plan));
    }

    #[test]
    fn a_citys_priority_order_crosses_the_wire_and_applies() {
        let (mut host, mut guest) = pair();
        let city = guest
            .cities
            .iter()
            .position(|c| c.team == GUEST_SEAT)
            .unwrap();
        guest.selected_city = Some(city);
        let order = Priorities([Good::Metal, Good::Food, Good::Wood]);
        guest.set_selected_city_priorities(order);
        let plan = guest.team_plan(GUEST_SEAT);
        let arrived = roundtrip(&Message::Plan(plan.clone()));
        assert_eq!(arrived, Message::Plan(plan.clone()));
        host.receive(GUEST_SEAT, arrived).expect("sound");
        let mut applied = host.lockstep.as_ref().unwrap().turn_start.clone().unwrap();
        applied.apply_plan(&plan);
        assert_eq!(applied.cities[city].priorities, order);
        assert_eq!(applied.cities[city].worked, guest.cities[city].worked);
    }

    #[test]
    fn queues_planned_again_each_turn_keep_both_machines_in_step() {
        let (mut host, mut guest) = pair();
        // Each side Shift-queues a unit of its own to a hex far off in the
        // fog; each turn its machine plans the queue again from what it
        // knows, and that goes in its plan.
        let mut queued = Vec::new();
        for game in [&mut host, &mut guest] {
            let team = game.local_team;
            let unit = game
                .units
                .iter()
                .position(|u| u.team == team && !game.settlers.contains(&u.id))
                .unwrap();
            let from = game.units[unit].pos;
            let target = game
                .grid
                .all_hexes()
                .filter(|&h| {
                    game.grid.is_passable(h) && !game.is_explored(h) && from.distance(h) >= 6
                })
                .min_by_key(|&h| (from.distance(h), h.q, h.r))
                .expect("somewhere far in the fog");
            game.selected = Some(unit);
            assert!(game.queue_move(target));
            assert_eq!(game.units[unit].waypoints, vec![target]);
            queued.push((game.units[unit].id, from, target));
        }
        for _ in 0..4 {
            guest.submit_plan();
            exchange(&mut host, &mut guest);
            host.submit_plan();
            exchange(&mut host, &mut guest);
            play_out(&mut host);
            play_out(&mut guest);
            exchange(&mut host, &mut guest);
            assert_eq!(host.checksum(), guest.checksum());
            assert_eq!(host.lockstep.as_ref().unwrap().desync, None);
            assert_eq!(guest.lockstep.as_ref().unwrap().dropped, None);
        }
        // Both queues went on toward where they were sent, on both machines.
        for (id, from, target) in queued {
            for game in [&host, &guest] {
                let unit = game.units.iter().find(|u| u.id == id).unwrap();
                assert!(
                    unit.pos.distance(target) < from.distance(target),
                    "{id} got no nearer {target:?}"
                );
            }
        }
    }
}
