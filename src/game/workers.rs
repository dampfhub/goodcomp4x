//! Workers: each city keeps a pool of workers at home, safe and off the map,
//! and a queue of jobs the player orders from a tile: roads, tile
//! improvements and structures. In the last step of every turn, after every
//! unit has moved and attacked, cities send idle workers out. Each walks to
//! its job, works it for a few turns, then takes the city's next job or walks
//! home. A worker out on the map is exposed: an enemy unit moving onto its
//! hex captures it for the enemy's nearest city, and an attack on its hex
//! kills it. A unit standing on the same hex shields it from both.
//!
//! Structures: walls and gates stand on the edge between two hexes, walls
//! stopping everyone from crossing it and gates everyone but their owner.
//! Outposts (the owner sees around them) and forts (the owner's units in one
//! defend better) stand on a tile.

use std::collections::{HashMap, VecDeque};

use glam::Vec2;

use super::city::Site;
use super::hex::{Hex, edge};
use super::terrain::Terrain;
use super::unit::{Team, Unit};
use super::{GameState, PLAYER_TEAM};

/// Hexes a worker walks per turn.
pub(super) const WORKER_MOVE: usize = 1;
/// How far from one of its side's cities a worker will go to work, in hexes,
/// unless a road runs by the job (`in_worker_reach`).
pub(super) const WORKER_REACH: i32 = 3;
/// How far an outpost lets its owner see, and a worker out on the map.
pub(super) const OUTPOST_SIGHT: i32 = 2;
pub(super) const WORKER_SIGHT: i32 = 1;
/// Defense multiplier for the owner's units standing in a fort. A
/// placeholder until forts get their real role.
pub(super) const FORT_DEFENSE: f32 = 1.5;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum JobKind {
    Road,
    Improve,
    Wall,
    Gate,
    Outpost,
    Fort,
}

impl JobKind {
    pub const ALL: [JobKind; 6] = [
        JobKind::Road,
        JobKind::Improve,
        JobKind::Wall,
        JobKind::Gate,
        JobKind::Outpost,
        JobKind::Fort,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Road => "ROAD",
            Self::Improve => "IMPROVE",
            Self::Wall => "WALL",
            Self::Gate => "GATE",
            Self::Outpost => "OUTPOST",
            Self::Fort => "FORT",
        }
    }

    /// Turns of work once the worker stands on the tile.
    pub fn turns(self) -> u32 {
        match self {
            Self::Road => 1,
            Self::Improve | Self::Wall => 2,
            Self::Gate | Self::Outpost => 3,
            Self::Fort => 4,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Road => "LOWERS THE COST OF CARRYING GOODS THROUGH THIS TILE.",
            Self::Improve => {
                "A MINE ON HILLS (+2 PRODUCTION), LUMBER MILL IN FOREST OR JUNGLE (+1), FARM ELSEWHERE (+2 FOOD)."
            }
            Self::Wall => "ON A HEX EDGE: NO UNIT OR GOODS CROSS IT, YOURS INCLUDED.",
            Self::Gate => "ON A HEX EDGE: YOUR UNITS AND GOODS CROSS IT; ENEMIES' DON'T.",
            Self::Outpost => "YOU SEE 2 TILES AROUND IT.",
            Self::Fort => "YOUR UNITS IN IT DEFEND 50% BETTER.",
        }
    }

    pub(super) fn structure(self) -> Option<StructureKind> {
        match self {
            Self::Road | Self::Improve => None,
            Self::Wall => Some(StructureKind::Wall),
            Self::Gate => Some(StructureKind::Gate),
            Self::Outpost => Some(StructureKind::Outpost),
            Self::Fort => Some(StructureKind::Fort),
        }
    }

    /// Whether the job goes on the edge between two hexes rather than on a
    /// tile.
    pub fn on_edge(self) -> bool {
        matches!(self, Self::Wall | Self::Gate)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum StructureKind {
    Wall,
    Gate,
    Outpost,
    Fort,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct Structure {
    pub kind: StructureKind,
    pub team: Team,
}

impl Structure {
    /// Whether a unit, worker or goods of `team` may cross the wall or gate.
    pub fn admits(self, team: Team) -> bool {
        match self.kind {
            StructureKind::Wall => false,
            StructureKind::Gate => self.team == team,
            StructureKind::Outpost | StructureKind::Fort => true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) struct WorkerJob {
    /// Where the worker stands to do it.
    pub hex: Hex,
    pub kind: JobKind,
    /// For a wall or gate, the hex across the edge it goes on.
    pub across: Option<Hex>,
}

impl WorkerJob {
    pub fn on_tile(hex: Hex, kind: JobKind) -> Self {
        Self {
            hex,
            kind,
            across: None,
        }
    }

    /// Whether two jobs would build on the same place: the same tile, or
    /// for walls and gates, the same edge.
    fn same_place(self, other: WorkerJob) -> bool {
        match (self.across, other.across) {
            (Some(a), Some(b)) => edge(self.hex, a) == edge(other.hex, b),
            (None, None) => self.hex == other.hex && self.kind == other.kind,
            _ => false,
        }
    }
}

/// The hex edge nearest `point` among `hex`'s six, as the neighbor across it.
pub(super) fn nearest_edge(hex: Hex, point: Vec2) -> Hex {
    let midpoint = |n: Hex| (hex.to_world() + n.to_world()) / 2.0;
    hex.neighbors()
        .into_iter()
        .min_by(|&a, &b| {
            midpoint(a)
                .distance(point)
                .total_cmp(&midpoint(b).distance(point))
        })
        .expect("a hex has six neighbors")
}

/// A worker out on the map.
#[derive(Clone, Debug)]
pub(super) struct FieldWorker {
    pub id: u32,
    pub team: Team,
    /// The city it belongs to and returns to.
    pub home: usize,
    pub pos: Hex,
    /// What it's out to do; `None` while it walks home.
    pub job: Option<WorkerJob>,
    /// Turns of work left, once it stands on the job's tile.
    pub work_left: Option<u32>,
    /// Sent home by the player: it takes no job until it gets there.
    pub recalled: bool,
}

impl GameState {
    /// Whether a unit or worker may stand on `hex`: open ground.
    pub(super) fn can_enter(&self, hex: Hex) -> bool {
        self.grid.is_passable(hex)
    }

    /// Whether `team` may cross the edge between adjacent `from` and `to`:
    /// no wall there, and no gate but its own.
    pub(super) fn can_cross(&self, from: Hex, to: Hex, team: Team) -> bool {
        self.barriers
            .get(&edge(from, to))
            .is_none_or(|barrier| barrier.admits(team))
    }

    /// Whether a unit or worker of `team` can step from `from` onto the
    /// adjacent `to`. Enemy city centers are entered through the interior
    /// battle, not by walking onto the exterior marker.
    pub(super) fn can_step(&self, from: Hex, to: Hex, team: Team) -> bool {
        self.can_enter(to)
            && self
                .cities
                .iter()
                .all(|city| city.pos != to || city.team == team)
            && self.can_cross(from, to, team)
    }

    /// Whether `unit` stands in a fort of its own side.
    pub(super) fn in_fort(&self, unit: &Unit) -> bool {
        self.structures.get(&unit.pos).is_some_and(|structure| {
            structure.kind == StructureKind::Fort && structure.team == unit.team
        })
    }

    /// Terrain and fort bonuses on `unit`'s defense.
    pub(super) fn defense_multiplier(&self, unit: &Unit) -> f32 {
        let fort = if self.in_fort(unit) {
            FORT_DEFENSE
        } else {
            1.0
        };
        self.grid.tile(unit.pos).defense_multiplier() * fort
    }

    /// How many of `city`'s workers are out on the map.
    pub(super) fn workers_out(&self, city: usize) -> usize {
        self.field_workers.iter().filter(|w| w.home == city).count()
    }

    /// `team`'s city nearest `hex`, ties going to the older city.
    pub(super) fn nearest_city(&self, team: Team, hex: Hex) -> Option<usize> {
        (0..self.cities.len())
            .filter(|&i| self.cities[i].team == team)
            .min_by_key(|&i| (self.cities[i].pos.distance(hex), i))
    }

    /// The player's city whose workers would take a job at `hex`: the open
    /// city, or else the nearest one.
    pub(super) fn job_city(&self, hex: Hex) -> Option<usize> {
        self.selected_city
            .filter(|&city| self.cities[city].team == PLAYER_TEAM)
            .or_else(|| self.nearest_city(PLAYER_TEAM, hex))
    }

    /// The improvement a worker would build at `hex`, as the yields it gives
    /// and its label: a mine on hills, a lumber mill in forest or jungle, a
    /// farm elsewhere; nothing on snow.
    fn improvement(&self, hex: Hex) -> Option<(i32, i32, &'static str)> {
        let tile = self.grid.tile(hex);
        let (food, production) = tile.yields();
        if tile.hills {
            Some((food, production + 2, "MINE"))
        } else if tile.feature.is_some() {
            Some((food, production + 1, "LUMBER MILL"))
        } else if tile.terrain == Terrain::Snow {
            None
        } else {
            Some((food + 2, production, "FARM"))
        }
    }

    /// Why `team`'s workers can't do `job`, or `None` if they can.
    pub(super) fn job_problem(&self, team: Team, job: WorkerJob) -> Option<&'static str> {
        let hex = job.hex;
        if !self.grid.contains(hex) || !self.grid.is_passable(hex) {
            return Some("WORKERS CAN'T WORK THIS TERRAIN");
        }
        if !self.in_worker_reach(team, hex) {
            return Some("OUT OF REACH: WORKERS GO 3 TILES FROM A CITY, OR NEXT TO A ROAD");
        }
        if let Some(across) = job.across {
            return if hex.distance(across) != 1 || !self.grid.contains(across) {
                Some("WALLS AND GATES GO BETWEEN TWO TILES ON THE MAP")
            } else if self.barriers.contains_key(&edge(hex, across)) {
                Some("A WALL OR GATE STANDS HERE")
            } else {
                None
            };
        }
        if self.cities.iter().any(|c| c.pos == hex) {
            return Some("A CITY STANDS HERE");
        }
        let building = self
            .cities
            .iter()
            .any(|c| [c.barracks, c.mill, c.workshop].contains(&Some(hex)));
        match job.kind {
            JobKind::Road if self.is_road_hex(hex) => Some("THERE IS A ROAD HERE ALREADY"),
            JobKind::Road => None,
            JobKind::Improve if building => Some("A BUILDING STANDS HERE"),
            JobKind::Improve => match self.sites.get(&hex) {
                Some(site) if site.team != team => Some("THIS TILE BELONGS TO THE ENEMY"),
                Some(_) => Some("THIS TILE IS IMPROVED ALREADY"),
                None if self.improvement(hex).is_none() => Some("NOTHING GROWS ON SNOW"),
                None => None,
            },
            _ if building => Some("A BUILDING STANDS HERE"),
            _ if self.structures.contains_key(&hex) => Some("A STRUCTURE STANDS HERE"),
            _ => None,
        }
    }

    /// Whether `team`'s workers will work at `hex`: within `WORKER_REACH` of
    /// one of its cities, or on or next to a road, so roads carry the reach
    /// out as far as they go.
    pub(super) fn in_worker_reach(&self, team: Team, hex: Hex) -> bool {
        self.cities
            .iter()
            .any(|c| c.team == team && c.pos.distance(hex) <= WORKER_REACH)
            || std::iter::once(hex)
                .chain(hex.neighbors())
                .any(|h| self.roads.contains(&h))
    }

    /// W, or Worker Jobs in the city panel: turns worker mode on or off.
    pub fn toggle_worker_mode(&mut self) {
        let on = !self.worker_mode;
        self.set_worker_mode(on);
    }

    /// Worker mode: the map shows every tile the player's workers can reach,
    /// and a click on any tile, units or not, opens its tile panel to pick a
    /// job. It ends with W, Escape, or selecting a unit or a city.
    pub(super) fn set_worker_mode(&mut self, on: bool) {
        if on == self.worker_mode || self.is_resolving() {
            return;
        }
        if on && !self.cities.iter().any(|c| c.team == PLAYER_TEAM) {
            self.notice = "FOUND A CITY FIRST - ITS WORKERS DO THE WORK".into();
            return;
        }
        if on {
            self.leave_city_view();
        }
        self.selected = None;
        self.group.clear();
        self.inspected_tile = None;
        self.placing_barrier = None;
        self.hovered_edge = None;
        self.ui_click_mode = None;
        self.worker_mode = on;
        self.notice = if on {
            "WORKER JOBS: CLICK A HIGHLIGHTED TILE, THEN PICK A JOB - W WHEN DONE".into()
        } else {
            "DONE WITH WORKER JOBS".into()
        };
    }

    /// A map click in worker mode: opens the clicked tile's panel.
    pub(super) fn worker_mode_click(&mut self, hex: Option<Hex>) {
        self.inspected_tile = hex.filter(|&h| self.grid.contains(h));
        self.notice = match self.inspected_tile {
            Some(h) if !self.in_worker_reach(PLAYER_TEAM, h) => {
                "OUT OF REACH - WORKERS GO 3 TILES FROM A CITY, OR NEXT TO A ROAD".into()
            }
            Some(_) => "PICK A JOB FOR THIS TILE - OR CLICK ANOTHER, W WHEN DONE".into(),
            None => "WORKER JOBS: CLICK A HIGHLIGHTED TILE, THEN PICK A JOB - W WHEN DONE".into(),
        };
    }

    /// Whether `team` already has a job queued or under way in `job`'s place.
    pub(super) fn job_taken(&self, team: Team, job: WorkerJob) -> bool {
        self.cities
            .iter()
            .filter(|c| c.team == team)
            .flat_map(|c| &c.worker_jobs)
            .chain(
                self.field_workers
                    .iter()
                    .filter(|w| w.team == team)
                    .filter_map(|w| w.job.as_ref()),
            )
            .any(|other| other.same_place(job))
    }

    /// Tile panel buttons, or R and I: queues `kind` on the inspected tile
    /// for the city whose workers would do it. A wall or gate instead arms
    /// edge placement: clicks (or a drag) on hex edges queue them there.
    pub fn queue_worker_job(&mut self, kind: JobKind) {
        if self.is_resolving() {
            return;
        }
        if kind.on_edge() {
            if self.nearest_city(PLAYER_TEAM, Hex::new(0, 0)).is_none() {
                self.notice = "FOUND A CITY FIRST - ITS WORKERS DO THE WORK".into();
                return;
            }
            self.placing_barrier = Some(kind);
            self.notice = format!(
                "CLICK OR DRAG ALONG HEX EDGES TO QUEUE {}S - ESC TO STOP",
                kind.name()
            );
            return;
        }
        let Some(hex) = self.inspected_tile else {
            self.notice = "CLICK A TILE FIRST, THEN CHOOSE A WORKER JOB".into();
            return;
        };
        self.queue_worker_job_at(hex, kind);
    }

    /// With a wall or gate armed: the edge under a map point, as the hex the
    /// point is in and the neighbor across the edge.
    pub(super) fn barrier_edge_at(&self, point: Vec2) -> Option<(Hex, Hex)> {
        self.placing_barrier?;
        let hex = Hex::from_world(point);
        self.grid
            .contains(hex)
            .then(|| (hex, nearest_edge(hex, point)))
    }

    /// Queues the armed wall or gate on the edge between `a` and `b`. The
    /// worker builds it from the side nearer its city. Returns whether it
    /// was queued; a drag calls this for every edge it passes, so an edge
    /// already queued is skipped without a notice.
    pub(super) fn queue_barrier_at(&mut self, a: Hex, b: Hex) -> bool {
        let Some(kind) = self.placing_barrier else {
            return false;
        };
        let Some(city) = self.job_city(a) else {
            return false;
        };
        let pos = self.cities[city].pos;
        let open = |h: Hex| self.grid.is_passable(h) && self.is_explored(h);
        let mut sides = [a, b];
        sides.sort_by_key(|h| h.distance(pos));
        let Some(&hex) = sides.iter().find(|&&h| open(h)) else {
            self.notice = format!("A {} NEEDS OPEN, EXPLORED GROUND ON ONE SIDE", kind.name());
            return false;
        };
        let across = if hex == a { b } else { a };
        let job = WorkerJob {
            hex,
            kind,
            across: Some(across),
        };
        if self.job_taken(PLAYER_TEAM, job) {
            return false;
        }
        if let Some(problem) = self.job_problem(PLAYER_TEAM, job) {
            self.notice = problem.into();
            return false;
        }
        self.cities[city].worker_jobs.push(job);
        self.notice = format!(
            "{} QUEUED FOR CITY {} - {} JOBS WAITING - ESC TO STOP",
            kind.name(),
            self.cities[city].id + 1,
            self.cities[city].worker_jobs.len()
        );
        true
    }

    /// Why the player can't queue `kind` at `hex` right now, if they can't.
    /// Walls and gates only need a city: their edge is picked on the map.
    pub(super) fn job_unavailable(&self, hex: Hex, kind: JobKind) -> Option<String> {
        let job = WorkerJob::on_tile(hex, kind);
        if self.job_city(hex).is_none() {
            Some("FOUND A CITY FIRST - ITS WORKERS DO THE WORK".into())
        } else if kind.on_edge() {
            None
        } else if !self.is_explored(hex) {
            Some("WORKERS CAN'T WORK AN UNEXPLORED TILE".into())
        } else if let Some(problem) = self.job_problem(PLAYER_TEAM, job) {
            Some(problem.into())
        } else if self.job_taken(PLAYER_TEAM, job) {
            Some(format!("{} IS QUEUED HERE ALREADY", kind.name()))
        } else {
            // A tile takes one job at a time.
            self.tile_job_at(PLAYER_TEAM, hex)
                .map(|other| format!("{} IS QUEUED HERE - ONE JOB AT A TIME", other.name()))
        }
    }

    /// The job `team` has queued or under way on tile `hex`, if any (walls
    /// and gates, on edges, aside). A tile takes one job at a time.
    pub(super) fn tile_job_at(&self, team: Team, hex: Hex) -> Option<JobKind> {
        self.cities
            .iter()
            .filter(|c| c.team == team)
            .flat_map(|c| &c.worker_jobs)
            .chain(
                self.field_workers
                    .iter()
                    .filter(|w| w.team == team)
                    .filter_map(|w| w.job.as_ref()),
            )
            .find(|job| job.across.is_none() && job.hex == hex)
            .map(|job| job.kind)
    }

    pub(super) fn queue_worker_job_at(&mut self, hex: Hex, kind: JobKind) {
        if let Some(reason) = self.job_unavailable(hex, kind) {
            self.notice = reason;
            return;
        }
        let Some(city) = self.job_city(hex) else {
            return;
        };
        self.cities[city]
            .worker_jobs
            .push(WorkerJob::on_tile(hex, kind));
        let home = self.cities[city].workers;
        self.notice = format!(
            "{} QUEUED FOR CITY {} - {home} WORKER{} AT HOME",
            kind.name(),
            self.cities[city].id + 1,
            if home == 1 { "" } else { "S" }
        );
    }

    /// The X on a worker job in the open city.
    pub fn remove_worker_job(&mut self, index: usize) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            return;
        };
        if index < self.cities[city].worker_jobs.len() {
            let job = self.cities[city].worker_jobs.remove(index);
            self.notice = format!("REMOVED {} FROM THE WORKER JOBS", job.kind.name());
        }
    }

    /// A Recall button: sends one of the player's workers out on the map
    /// straight home, out of danger. Its job goes back to the top of its
    /// city's list, and it takes no new one on the way.
    pub fn recall_worker(&mut self, id: u32) {
        if self.is_resolving() {
            return;
        }
        let Some(w) = self
            .field_workers
            .iter()
            .position(|w| w.id == id && w.team == PLAYER_TEAM)
        else {
            return;
        };
        let worker = self.field_workers[w].clone();
        self.return_job(&worker);
        let worker = &mut self.field_workers[w];
        worker.job = None;
        worker.work_left = None;
        worker.recalled = true;
        self.notice = "WORKER RECALLED - IT HEADS HOME; ITS JOB WAITS ON THE LIST".into();
    }

    /// Takes the first job in `city`'s queue its workers can still do,
    /// dropping any that became impossible.
    fn take_job(&mut self, city: usize) -> Option<WorkerJob> {
        let team = self.cities[city].team;
        while !self.cities[city].worker_jobs.is_empty() {
            let job = self.cities[city].worker_jobs.remove(0);
            match self.job_problem(team, job) {
                None => return Some(job),
                Some(problem) if team == PLAYER_TEAM => {
                    self.notice = format!("{} DROPPED: {problem}", job.kind.name());
                }
                Some(_) => {}
            }
        }
        None
    }

    /// The last step of a turn: workers heading home take newly queued jobs,
    /// cities send idle workers out, and every worker out on the map walks,
    /// works or arrives home. Returns the ids of the workers that did any of
    /// that.
    pub(super) fn resolve_workers(&mut self) -> Vec<u32> {
        for w in 0..self.field_workers.len() {
            if self.field_workers[w].job.is_none() && !self.field_workers[w].recalled {
                let home = self.field_workers[w].home;
                self.field_workers[w].job = self.take_job(home);
            }
        }
        for city in 0..self.cities.len() {
            while self.cities[city].workers > 0 {
                let Some(job) = self.take_job(city) else {
                    break;
                };
                self.cities[city].workers -= 1;
                let id = self.next_unit_id;
                self.next_unit_id += 1;
                self.field_workers.push(FieldWorker {
                    id,
                    team: self.cities[city].team,
                    home: city,
                    pos: self.cities[city].pos,
                    job: Some(job),
                    work_left: None,
                    recalled: false,
                });
            }
        }
        let mut acted = Vec::new();
        let mut home = Vec::new();
        for w in 0..self.field_workers.len() {
            let (did_something, arrived_home) = self.advance_worker(w);
            if did_something {
                acted.push(self.field_workers[w].id);
            }
            if arrived_home {
                home.push(w);
            }
        }
        for w in home.into_iter().rev() {
            let worker = self.field_workers.remove(w);
            self.cities[worker.home].workers += 1;
        }
        acted
    }

    /// One worker's turn. Returns whether it did anything, and whether it
    /// reached its city with nothing left to do.
    fn advance_worker(&mut self, w: usize) -> (bool, bool) {
        let worker = self.field_workers[w].clone();
        if let Some(job) = worker.job
            && worker.pos == job.hex
            && let Some(left) = worker.work_left
        {
            if let Some(problem) = self.job_problem(worker.team, job) {
                if worker.team == PLAYER_TEAM {
                    self.notice = format!("{} ABANDONED: {problem}", job.kind.name());
                }
                self.field_workers[w].job = None;
                self.field_workers[w].work_left = None;
                return (true, false);
            }
            if left > 1 {
                self.field_workers[w].work_left = Some(left - 1);
                return (true, false);
            }
            self.complete_job(worker.team, job);
            let next = self.take_job(worker.home);
            self.field_workers[w].job = next;
            self.field_workers[w].work_left = None;
            return (true, false);
        }

        // Walking: to the job, or home.
        let Some(home) = self.home_of(&worker) else {
            return (false, false);
        };
        let target = worker.job.map_or(self.cities[home].pos, |job| job.hex);
        let Some(path) = self.worker_path(worker.team, worker.pos, target) else {
            if let Some(job) = worker.job {
                if worker.team == PLAYER_TEAM {
                    self.notice = format!(
                        "A WORKER CAN'T REACH ITS {} AT ({}, {}) - IT'S COMING HOME",
                        job.kind.name(),
                        job.hex.q,
                        job.hex.r
                    );
                }
                self.field_workers[w].job = None;
                return (true, false);
            }
            return (false, false);
        };
        let pos = path[WORKER_MOVE.min(path.len() - 1)];
        let moved = pos != worker.pos;
        let worker = &mut self.field_workers[w];
        worker.home = home;
        worker.pos = pos;
        if pos != target {
            return (moved, false);
        }
        match worker.job {
            Some(job) => {
                worker.work_left = Some(job.kind.turns());
                (true, false)
            }
            None => (true, true),
        }
    }

    /// The city a worker heads back to: its own, or if that one was lost,
    /// its side's nearest.
    fn home_of(&self, worker: &FieldWorker) -> Option<usize> {
        if self.cities[worker.home].team == worker.team {
            Some(worker.home)
        } else {
            self.nearest_city(worker.team, worker.pos)
        }
    }

    /// The shortest walk for a worker of `team` from `from` to `to`, both
    /// ends included, around impassable terrain, walls, others' gates, enemy
    /// units and enemy cities. Ties break by hex coordinates.
    fn worker_path(&self, team: Team, from: Hex, to: Hex) -> Option<Vec<Hex>> {
        let open = |at: Hex, hex: Hex| {
            self.can_step(at, hex, team)
                && self.enemy_of_team_at(hex, team).is_none()
                && !self.cities.iter().any(|c| c.pos == hex && c.team != team)
        };
        let mut came_from: HashMap<Hex, Hex> = HashMap::from([(from, from)]);
        let mut queue = VecDeque::from([from]);
        while let Some(hex) = queue.pop_front() {
            if hex == to {
                let mut path = vec![to];
                let mut at = to;
                while at != from {
                    at = came_from[&at];
                    path.push(at);
                }
                path.reverse();
                return Some(path);
            }
            let mut neighbors = hex.neighbors();
            neighbors.sort_by_key(|n| (n.q, n.r));
            for n in neighbors {
                if open(hex, n) && !came_from.contains_key(&n) {
                    came_from.insert(n, hex);
                    queue.push_back(n);
                }
            }
        }
        None
    }

    fn complete_job(&mut self, team: Team, job: WorkerJob) {
        let hex = job.hex;
        match job.kind {
            JobKind::Road => {
                self.roads.insert(hex);
            }
            JobKind::Improve => {
                if let Some((food, production, label)) = self.improvement(hex) {
                    self.sites.insert(
                        hex,
                        Site {
                            team,
                            food,
                            production,
                            label,
                        },
                    );
                }
            }
            kind => {
                if let Some(kind) = kind.structure() {
                    let structure = Structure { kind, team };
                    match job.across {
                        Some(across) => self.barriers.insert(edge(hex, across), structure),
                        None => self.structures.insert(hex, structure),
                    };
                }
            }
        }
        log::info!(
            "{team:?} workers finished a {} at ({}, {})",
            job.kind.name(),
            hex.q,
            hex.r
        );
        if team == PLAYER_TEAM {
            self.notice = format!(
                "WORKERS FINISHED A {} AT ({}, {})",
                job.kind.name(),
                hex.q,
                hex.r
            );
        }
    }

    /// A worker taken off the map with a job in hand leaves that job at the
    /// front of its city's queue, for the next worker to try.
    fn return_job(&mut self, worker: &FieldWorker) {
        if let Some(job) = worker.job
            && self.cities[worker.home].team == worker.team
        {
            self.cities[worker.home].worker_jobs.insert(0, job);
        }
    }

    /// After a move step: every worker sharing a hex with an enemy unit is
    /// captured. It joins the captor's nearest city, or is lost if the captor
    /// has no city.
    pub(super) fn capture_workers(&mut self) {
        let mut w = 0;
        while w < self.field_workers.len() {
            let worker = &self.field_workers[w];
            let Some(captor) = self.enemy_of_team_at(worker.pos, worker.team) else {
                w += 1;
                continue;
            };
            let captor_team = self.units[captor].team;
            let worker = self.field_workers.remove(w);
            self.return_job(&worker);
            log::info!(
                "{} captures a {:?} worker at ({}, {})",
                self.units[captor],
                worker.team,
                worker.pos.q,
                worker.pos.r
            );
            if let Some(city) = self.nearest_city(captor_team, worker.pos) {
                self.cities[city].workers += 1;
            }
            if worker.team == PLAYER_TEAM {
                self.notice = "AN ENEMY CAPTURED ONE OF YOUR WORKERS".into();
            } else if captor_team == PLAYER_TEAM {
                self.notice = "WORKER CAPTURED - IT JOINS YOUR NEAREST CITY".into();
            }
        }
    }

    /// Enemy workers of `team`'s foes on `hex`, by index.
    pub(super) fn enemy_workers_at(&self, hex: Hex, team: Team) -> Vec<usize> {
        (0..self.field_workers.len())
            .filter(|&w| self.field_workers[w].pos == hex && self.field_workers[w].team != team)
            .collect()
    }

    /// Kills the workers with these ids.
    pub(super) fn kill_workers(&mut self, ids: &[u32]) {
        let mut w = 0;
        while w < self.field_workers.len() {
            if !ids.contains(&self.field_workers[w].id) {
                w += 1;
                continue;
            }
            let worker = self.field_workers.remove(w);
            self.return_job(&worker);
            log::info!(
                "a {:?} worker is killed at ({}, {})",
                worker.team,
                worker.pos.q,
                worker.pos.r
            );
            if worker.team == PLAYER_TEAM {
                self.notice = "ONE OF YOUR WORKERS WAS KILLED".into();
            }
        }
    }

    /// The AI's workers: each city with idle workers and nothing queued
    /// improves the tiles it works, then puts roads on them.
    pub(super) fn plan_ai_workers(&mut self, team: Team) {
        for city in 0..self.cities.len() {
            let c = &self.cities[city];
            if c.team != team || c.workers == 0 || !c.worker_jobs.is_empty() {
                continue;
            }
            let worked = c.worked.clone();
            let job = [JobKind::Improve, JobKind::Road]
                .into_iter()
                .find_map(|kind| {
                    worked.iter().find_map(|&hex| {
                        let job = WorkerJob::on_tile(hex, kind);
                        (self.job_problem(team, job).is_none() && !self.job_taken(team, job))
                            .then_some(job)
                    })
                });
            if let Some(job) = job {
                self.cities[city].worker_jobs.push(job);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::city::Build;
    use crate::game::turn::{Phase, Step};
    use crate::game::unit::UnitType;

    /// The Cities scenario with no units, the fog lifted, and Blue's city
    /// (index 0, one worker at home) at (-4, 0).
    fn cities() -> GameState {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        game.fog_of_war = false;
        assert_eq!(game.cities[0].team, PLAYER_TEAM);
        assert_eq!(game.cities[0].workers, 1);
        game
    }

    /// An open, unimproved tile `distance` steps from Blue's city.
    fn bare_tile(game: &GameState, distance: i32) -> Hex {
        game.grid
            .all_hexes()
            .filter(|&h| h.distance(game.cities[0].pos) == distance)
            .filter(|&h| game.grid.is_passable(h) && !game.sites.contains_key(&h))
            .filter(|&h| game.improvement(h).is_some() && !game.roads.contains(&h))
            .min_by_key(|h| (h.q, h.r))
            .expect("a bare tile")
    }

    fn queue(game: &mut GameState, hex: Hex, kind: JobKind) {
        game.inspected_tile = Some(hex);
        game.queue_worker_job(kind);
    }

    #[test]
    fn workers_reach_three_tiles_from_a_city_or_next_to_a_road() {
        let mut game = cities();
        // Blue's city is at (-4, 0); (0, 0) is four away, with no road by it.
        let far = Hex::new(0, 0);
        assert_eq!(game.cities[0].pos.distance(far), 4);
        assert!(!game.in_worker_reach(PLAYER_TEAM, far));
        let reason = game
            .job_unavailable(far, JobKind::Road)
            .expect("out of reach");
        assert!(reason.starts_with("OUT OF REACH"), "{reason}");
        let near = Hex::new(-1, 0);
        assert!(game.in_worker_reach(PLAYER_TEAM, near), "three away");

        // A road next to it brings it into reach, and each road reaches on.
        game.roads.insert(near);
        assert!(game.in_worker_reach(PLAYER_TEAM, far));
        assert!(game.job_unavailable(far, JobKind::Road).is_none());
        assert!(
            !game.in_worker_reach(PLAYER_TEAM, Hex::new(1, 1)),
            "two past the road"
        );
        // Red's reach is its own.
        assert!(!game.in_worker_reach(Team::Red, Hex::new(-4, 4)));
    }

    #[test]
    fn worker_mode_opens_any_tile_and_ends_with_escape_or_a_selection() {
        let mut game = GameState::city_scenario();
        game.select_city();
        assert!(game.selected_city.is_some());
        game.toggle_worker_mode();
        assert!(game.worker_mode);
        assert_eq!(game.selected_city, None, "the city view closes");
        assert_eq!(game.selected, None);

        // A click on one of your units opens its tile, not the unit.
        let unit = game
            .units
            .iter()
            .position(|u| u.team == PLAYER_TEAM)
            .unwrap();
        let hex = game.units[unit].pos;
        game.worker_mode_click(Some(hex));
        assert_eq!(game.inspected_tile, Some(hex));
        assert_eq!(game.selected, None);

        // Escape ends it, tile panel and all.
        game.press_escape();
        assert!(!game.worker_mode);
        assert_eq!(game.inspected_tile, None);
        assert!(!game.settings_open, "Escape closed worker mode, not more");

        // Selecting a unit ends it too; so does opening a city.
        game.toggle_worker_mode();
        game.set_selection(vec![unit]);
        assert!(!game.worker_mode);
        game.toggle_worker_mode();
        game.select_city();
        assert!(!game.worker_mode);

        // No city, no workers.
        let mut game = GameState::new();
        game.toggle_worker_mode();
        assert!(!game.worker_mode);
        assert!(game.notice.contains("FOUND A CITY"), "{}", game.notice);
    }

    #[test]
    fn a_tile_takes_one_job_at_a_time() {
        let mut game = cities();
        let city = game.cities[0].pos;
        let tile = city
            .neighbors()
            .into_iter()
            .find(|&h| {
                game.job_unavailable(h, JobKind::Road).is_none()
                    && game.job_unavailable(h, JobKind::Improve).is_none()
            })
            .expect("a tile that could take either");
        game.queue_worker_job_at(tile, JobKind::Improve);
        assert_eq!(game.tile_job_at(PLAYER_TEAM, tile), Some(JobKind::Improve));
        let reason = game.job_unavailable(tile, JobKind::Road).expect("refused");
        assert_eq!(reason, "IMPROVE IS QUEUED HERE - ONE JOB AT A TIME");
        game.queue_worker_job_at(tile, JobKind::Road);
        assert_eq!(game.cities[0].worker_jobs.len(), 1, "no road queued");
        // A wall on one of its edges is another matter.
        assert!(game.job_unavailable(tile, JobKind::Wall).is_none());
    }

    #[test]
    fn a_job_goes_to_the_nearest_city_once() {
        let mut game = cities();
        let hex = bare_tile(&game, 2);
        queue(&mut game, hex, JobKind::Road);
        let road = WorkerJob::on_tile(hex, JobKind::Road);
        assert_eq!(game.cities[0].worker_jobs, [road]);
        queue(&mut game, hex, JobKind::Road);
        assert_eq!(game.cities[0].worker_jobs.len(), 1, "no duplicate");
        let city = game.cities[0].pos;
        queue(&mut game, city, JobKind::Fort);
        assert_eq!(game.cities[0].worker_jobs.len(), 1, "not on a city");
        assert!(game.notice.contains("CITY"), "{}", game.notice);
    }

    #[test]
    fn a_worker_walks_out_works_and_comes_home() {
        let mut game = cities();
        let hex = bare_tile(&game, 2);
        queue(&mut game, hex, JobKind::Improve);

        // One hex a turn: halfway, then there.
        game.resolve_workers();
        assert_eq!(game.cities[0].workers, 0);
        assert_eq!(game.field_workers.len(), 1);
        assert_eq!(game.field_workers[0].pos.distance(hex), 1);
        game.resolve_workers();
        assert_eq!(game.field_workers[0].pos, hex);
        assert_eq!(game.field_workers[0].work_left, Some(2));
        // Two turns of work.
        game.resolve_workers();
        assert!(!game.sites.contains_key(&hex));
        game.resolve_workers();
        assert_eq!(game.sites[&hex].team, PLAYER_TEAM);
        assert_eq!(game.field_workers[0].job, None);
        // And home, two turns back.
        game.resolve_workers();
        assert_eq!(game.field_workers.len(), 1);
        game.resolve_workers();
        assert!(game.field_workers.is_empty());
        assert_eq!(game.cities[0].workers, 1);
    }

    #[test]
    fn a_worker_takes_the_next_job_before_going_home() {
        let mut game = cities();
        let (first, second) = (bare_tile(&game, 1), bare_tile(&game, 2));
        queue(&mut game, first, JobKind::Road);
        queue(&mut game, second, JobKind::Road);
        for _ in 0..2 {
            game.resolve_workers();
        }
        assert!(game.roads.contains(&first));
        assert_eq!(game.field_workers[0].job.map(|j| j.hex), Some(second));
    }

    #[test]
    fn a_recalled_worker_heads_home_and_leaves_its_job_waiting() {
        let mut game = cities();
        let (first, second) = (bare_tile(&game, 2), bare_tile(&game, 1));
        queue(&mut game, first, JobKind::Fort);
        game.resolve_workers();
        game.resolve_workers();
        assert_eq!(game.field_workers[0].pos, first);
        queue(&mut game, second, JobKind::Road);

        let id = game.field_workers[0].id;
        game.recall_worker(id);
        let kinds: Vec<_> = game.cities[0].worker_jobs.iter().map(|j| j.kind).collect();
        assert_eq!(kinds, [JobKind::Fort, JobKind::Road], "its job back on top");
        // Home in two turns, taking neither job on the way.
        game.resolve_workers();
        assert!(game.field_workers[0].job.is_none());
        game.resolve_workers();
        assert!(game.field_workers.is_empty());
        assert_eq!(game.cities[0].workers, 1);
        assert_eq!(game.cities[0].worker_jobs.len(), 2);
        assert!(!game.structures.contains_key(&first));
    }

    #[test]
    fn workers_act_after_every_unit() {
        let mut game = cities();
        game.settings.instant_playback = false;
        game.resolve_turn();
        assert_eq!(game.pending_steps.back(), Some(&Step::Workers));
        let unit_steps = game.pending_steps.len() - 1;
        assert!(
            game.pending_steps
                .iter()
                .take(unit_steps)
                .all(|step| matches!(step, Step::Units(..)))
        );
    }

    /// A Blue worker at work on a bare tile, with a Red melee beside it.
    fn worker_beside_enemy() -> (GameState, Hex, usize) {
        let mut game = cities();
        let hex = bare_tile(&game, 2);
        game.field_workers.push(FieldWorker {
            id: 100,
            team: Team::Blue,
            home: 0,
            pos: hex,
            job: Some(WorkerJob::on_tile(hex, JobKind::Fort)),
            work_left: Some(3),
            recalled: false,
        });
        let beside = hex
            .neighbors()
            .into_iter()
            .find(|&h| game.grid.is_passable(h) && game.cities.iter().all(|c| c.pos != h))
            .unwrap();
        game.units
            .push(Unit::new(200, beside, Team::Red, UnitType::Melee));
        let red = game.units.len() - 1;
        (game, hex, red)
    }

    #[test]
    fn an_enemy_stepping_onto_a_worker_captures_it() {
        let (mut game, hex, red) = worker_beside_enemy();
        let red_home = game.cities[1].workers;
        game.units[red].planned_move = Some(hex);
        game.resolve_step(UnitType::Melee, Phase::Move);
        assert_eq!(game.units[red].pos, hex);
        assert!(game.field_workers.is_empty());
        assert_eq!(game.cities[1].workers, red_home + 1, "joins Red's city");
        assert_eq!(
            game.cities[0].worker_jobs.first().map(|j| j.hex),
            Some(hex),
            "its job goes back on the list"
        );
    }

    #[test]
    fn an_attack_kills_a_worker_unless_a_unit_shields_it() {
        let (mut game, hex, red) = worker_beside_enemy();
        game.units[red].planned_attack = Some(hex);
        game.resolve_step(UnitType::Melee, Phase::Attack);
        assert!(game.field_workers.is_empty(), "killed");

        let (mut game, hex, red) = worker_beside_enemy();
        game.units
            .push(Unit::new(300, hex, Team::Blue, UnitType::Armored));
        game.units[red].planned_attack = Some(hex);
        game.resolve_step(UnitType::Melee, Phase::Attack);
        assert_eq!(game.field_workers.len(), 1, "the escort took the hit");
    }

    #[test]
    fn walls_stop_everyone_and_gates_stop_only_enemies() {
        let mut game = cities();
        let start = bare_tile(&game, 2);
        let mut around = start
            .neighbors()
            .into_iter()
            .filter(|&h| game.grid.is_passable(h) && game.cities.iter().all(|c| c.pos != h));
        let (wall, gate) = (around.next().unwrap(), around.next().unwrap());
        let team = Team::Blue;
        let wall_kind = StructureKind::Wall;
        let gate_kind = StructureKind::Gate;
        game.barriers.insert(
            edge(start, wall),
            Structure {
                kind: wall_kind,
                team,
            },
        );
        game.barriers.insert(
            edge(start, gate),
            Structure {
                kind: gate_kind,
                team,
            },
        );
        // Both hexes stay open ground; only their edges with `start` close.
        assert!(game.can_enter(wall) && game.can_enter(gate));
        assert!(!game.can_cross(wall, start, Team::Blue));
        let blue = game.reachable_hexes(start, 1, Team::Blue);
        assert!(!blue.contains(&wall) && blue.contains(&gate));
        let red = game.reachable_hexes(start, 1, Team::Red);
        assert!(!red.contains(&wall) && !red.contains(&gate));

        // A move planned into the gate anyway is turned back.
        game.units
            .push(Unit::new(200, start, Team::Red, UnitType::Melee));
        game.units[0].planned_move = Some(gate);
        game.resolve_step(UnitType::Melee, Phase::Move);
        assert_eq!(game.units[0].pos, start);
    }

    #[test]
    fn a_wall_goes_on_an_edge_built_from_the_side_nearer_the_city() {
        let mut game = cities();
        let city = game.cities[0].pos;
        let (near, far) = game
            .grid
            .all_hexes()
            .filter(|&h| h.distance(city) == 2 && game.grid.is_passable(h))
            .find_map(|near| {
                let far = near
                    .neighbors()
                    .into_iter()
                    .find(|&h| h.distance(city) == 3 && game.grid.contains(h))?;
                Some((near, far))
            })
            .unwrap();
        game.inspected_tile = Some(near);
        game.queue_worker_job(JobKind::Wall);
        assert_eq!(game.placing_barrier, Some(JobKind::Wall));
        assert!(game.queue_barrier_at(far, near));
        let job = game.cities[0].worker_jobs[0];
        assert_eq!((job.hex, job.across), (near, Some(far)));
        // The same edge, either way round, or as a gate, isn't queued again.
        assert!(!game.queue_barrier_at(near, far));
        game.placing_barrier = Some(JobKind::Gate);
        assert!(!game.queue_barrier_at(far, near));
        assert_eq!(game.cities[0].worker_jobs.len(), 1);

        for _ in 0..4 {
            game.resolve_workers();
        }
        let wall = game.barriers[&edge(near, far)];
        assert_eq!((wall.kind, wall.team), (StructureKind::Wall, Team::Blue));
        assert!(
            game.structures.is_empty(),
            "nothing on the tiles themselves"
        );
    }

    #[test]
    fn walls_around_a_tile_cut_its_goods_off() {
        let mut game = cities();
        let city = game.cities[0].pos;
        let tile = game.cities[0]
            .worked
            .iter()
            .copied()
            .find(|&h| h.distance(city) == 1)
            .expect("a worked tile next to the city");
        assert!(game.routes(0).costs.contains_key(&tile));
        for n in tile.neighbors() {
            let wall = Structure {
                kind: StructureKind::Wall,
                team: Team::Blue,
            };
            game.barriers.insert(edge(tile, n), wall);
        }
        assert!(!game.routes(0).costs.contains_key(&tile));
    }

    #[test]
    fn an_outpost_lets_its_owner_see_around_it() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        let fog = game.fog();
        let far = game
            .grid
            .all_hexes()
            .filter(|&h| game.grid.is_passable(h) && game.cities.iter().all(|c| c.pos != h))
            .find(|&h| {
                std::iter::once(h)
                    .chain(h.neighbors())
                    .all(|n| !fog.sees(n))
            })
            .expect("a hex out of sight");
        let outpost = Structure {
            kind: StructureKind::Outpost,
            team: PLAYER_TEAM,
        };
        game.structures.insert(far, outpost);
        let fog = game.fog();
        assert!(fog.sees(far));
        let around = far
            .neighbors()
            .into_iter()
            .filter(|&n| game.grid.contains(n));
        assert!(around.clone().all(|n| fog.sees(n)));
    }

    #[test]
    fn a_fort_shields_only_its_owner() {
        let mut game = cities();
        let hex = bare_tile(&game, 2);
        let fort = Structure {
            kind: StructureKind::Fort,
            team: Team::Blue,
        };
        game.structures.insert(hex, fort);
        let ground = game.grid.tile(hex).defense_multiplier();
        let blue = Unit::new(1, hex, Team::Blue, UnitType::Melee);
        let red = Unit::new(2, hex, Team::Red, UnitType::Melee);
        assert_eq!(game.defense_multiplier(&blue), ground * FORT_DEFENSE);
        assert_eq!(game.defense_multiplier(&red), ground);
    }

    #[test]
    fn a_finished_worker_joins_the_city_at_home() {
        let mut game = cities();
        game.selected_city = Some(0);
        game.queue_selected_city_worker();
        assert_eq!(game.cities[0].queue, [Build::Worker]);
        game.debug_complete_current_production();
        assert_eq!(game.cities[0].workers, 2);
        assert!(game.cities[0].queue.is_empty());
    }

    #[test]
    fn a_job_that_became_impossible_is_dropped() {
        let mut game = cities();
        let hex = bare_tile(&game, 2);
        queue(&mut game, hex, JobKind::Improve);
        game.sites.insert(
            hex,
            Site {
                team: Team::Red,
                food: 3,
                production: 0,
                label: "FARM",
            },
        );
        game.resolve_workers();
        assert!(game.field_workers.is_empty(), "nothing left to go out for");
        assert_eq!(game.sites[&hex].team, Team::Red);
    }

    #[test]
    fn the_ai_puts_its_workers_to_work() {
        let mut game = cities();
        let red = 1;
        assert_eq!(game.cities[red].team, Team::Red);
        game.plan_ai_turn(Team::Red);
        assert!(!game.cities[red].worker_jobs.is_empty());
        game.resolve_workers();
        assert!(game.field_workers.iter().any(|w| w.team == Team::Red));
    }

    #[test]
    fn jobs_wait_until_the_turn_has_played_out() {
        let mut game = cities();
        let hex = bare_tile(&game, 2);
        game.pending_steps.push_back(Step::Workers);
        queue(&mut game, hex, JobKind::Road);
        assert!(game.cities[0].worker_jobs.is_empty());
    }
}
