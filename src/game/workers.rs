//! Workers: each city keeps a pool of workers at home, safe and off the map,
//! and a queue of jobs: everything the city puts on the map, placed from its
//! production list and paid from the stockpile when placed. Roads, tile
//! improvements, structures and the city's buildings with a site are all
//! built this way. In the last step of every turn, after every
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

use super::GameState;
use super::city::{Building, Site, Stock, stock_icons};
use super::hex::{Hex, edge};
use super::terrain::Terrain;
use super::unit::{Team, Unit};

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

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum JobKind {
    Road,
    Improve,
    Wall,
    Gate,
    Outpost,
    Fort,
    /// One of the city's buildings with a site (`Building::is_placeable`).
    Build(Building),
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
            Self::Build(building) => building.name(),
        }
    }

    /// What placing it takes from the side's stockpile (`city/economy.rs`),
    /// given back if it's taken off the list or dropped before it's done.
    pub fn price(self) -> Stock {
        match self {
            Self::Road => Stock::whole(0, 2, 0),
            Self::Improve => Stock::whole(0, 4, 0),
            Self::Wall => Stock::whole(0, 3, 0),
            Self::Gate => Stock::whole(0, 3, 2),
            Self::Outpost => Stock::whole(0, 6, 0),
            Self::Fort => Stock::whole(0, 8, 4),
            Self::Build(building) => building.price(),
        }
    }

    /// Turns of work once the worker stands on the tile.
    pub fn turns(self) -> u32 {
        match self {
            Self::Road | Self::Wall => 2,
            Self::Improve => 3,
            Self::Gate | Self::Outpost => 3,
            Self::Fort => 4,
            Self::Build(building) => building.turns() as u32,
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Road => "CHEAPER DELIVERY, AND WORKERS REACH ALONG IT.",
            Self::Improve => "MINE ON HILLS, LUMBER MILL IN FOREST, ELSE A FARM.",
            Self::Wall => "ON AN EDGE: NOBODY CROSSES.",
            Self::Gate => "ON AN EDGE: ONLY YOUR SIDE CROSSES.",
            Self::Outpost => "SEES 2 HEXES AROUND IT.",
            Self::Fort => "+50% DEFENSE FOR YOUR UNITS IN IT.",
            Self::Build(building) => building.description(),
        }
    }

    pub(super) fn structure(self) -> Option<StructureKind> {
        match self {
            Self::Road | Self::Improve | Self::Build(_) => None,
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct WorkerJob {
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
    /// The city center or connected Work Camp this assignment uses as a base.
    pub base: Hex,
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

    /// The player's city whose workers build what's placed on the map: the
    /// open city. Everything a worker builds is placed from its city.
    pub(super) fn job_city(&self) -> Option<usize> {
        self.selected_city
            .filter(|&city| self.cities[city].team == self.local_team)
    }

    /// What `job` builds, as the player reads it: an improvement by its kind
    /// (FARM, MINE or LUMBER MILL), anything else by the job's name.
    pub(super) fn job_name(&self, job: WorkerJob) -> &'static str {
        match job.kind {
            JobKind::Improve => self
                .improvement(job.hex)
                .map_or(job.kind.name(), |(_, _, label)| label),
            kind => kind.name(),
        }
    }

    /// A job as the city panel names it: what it builds, and the tile it's
    /// on, like "FARM · GRASSLAND" or "MINE · PLAINS HILLS".
    pub(super) fn job_title(&self, job: WorkerJob) -> String {
        format!(
            "{} · {}",
            self.job_name(job),
            self.grid.tile(job.hex).name()
        )
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

    /// Why city `city`'s workers can't do `job`, or `None` if they can.
    pub(super) fn job_problem(&self, city: usize, job: WorkerJob) -> Option<&'static str> {
        let team = self.cities[city].team;
        let hex = job.hex;
        if !self.grid.contains(hex) || !self.grid.is_passable(hex) {
            return Some("WORKERS CAN'T WORK THIS TERRAIN");
        }
        if !self.in_worker_reach(team, hex) {
            return Some(
                "OUT OF REACH: WORKERS GO 3 TILES FROM A CITY OR WORK CAMP, OR NEXT TO A ROAD",
            );
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
        if let JobKind::Build(building) = job.kind {
            if self.cities[city].built.contains(&building) {
                return Some("THIS CITY HAS ONE ALREADY");
            }
            if self.structures.contains_key(&hex) {
                return Some("A STRUCTURE STANDS HERE");
            }
            return self.ai_site_issue(city, building, hex);
        }
        let building = self.cities.iter().any(|c| {
            crate::game::city::Building::PLACEABLE
                .into_iter()
                .any(|b| c.placed_site(b) == Some(hex))
        });
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

    /// Where `team`'s workers go out from: its cities, and each Work Camp
    /// connected to its city (workers start nearby jobs from one,
    /// `work_base_for`).
    pub(super) fn worker_bases(&self, team: Team) -> Vec<Hex> {
        let mut bases = Vec::new();
        for (i, city) in self.cities.iter().enumerate() {
            if city.team != team {
                continue;
            }
            bases.push(city.pos);
            if let Some(camp) = city.placed_site(crate::game::city::Building::WorkCamp)
                && self.routes(i).costs.contains_key(&camp)
            {
                bases.push(camp);
            }
        }
        bases
    }

    /// Whether `team`'s workers will work at `hex`: within `WORKER_REACH` of
    /// one of its cities or connected Work Camps (`worker_bases`), or on or
    /// next to a road, so roads carry the reach out as far as they go.
    pub(super) fn in_worker_reach(&self, team: Team, hex: Hex) -> bool {
        self.in_reach_of(&self.worker_bases(team), hex)
    }

    /// `in_worker_reach` with the bases worked out once, for checking many
    /// hexes (the lit tiles while placing).
    pub(super) fn in_reach_of(&self, bases: &[Hex], hex: Hex) -> bool {
        bases.iter().any(|base| base.distance(hex) <= WORKER_REACH)
            || std::iter::once(hex)
                .chain(hex.neighbors())
                .any(|h| self.roads.contains(&h))
    }

    /// The city whose worker jobs a queue row or its X acts on: the open one.
    pub(super) fn worker_list_city(&self) -> Option<usize> {
        self.selected_city
    }

    /// Player-facing reach uses the last observed roads. Turn resolution
    /// continues to use `in_worker_reach` and the real board.
    pub(super) fn known_worker_reach(&self, hex: Hex) -> bool {
        let fog = self.fog();
        let bases: Vec<_> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(_, city)| city.team == self.local_team)
            .flat_map(|(i, city)| {
                let camp = city
                    .placed_site(crate::game::city::Building::WorkCamp)
                    .filter(|&h| self.known_routes(i, &fog).costs.contains_key(&h));
                std::iter::once(city.pos).chain(camp)
            })
            .collect();
        bases.iter().any(|base| base.distance(hex) <= WORKER_REACH)
            || std::iter::once(hex).chain(hex.neighbors()).any(|h| {
                if fog.sees(h) {
                    self.roads.contains(&h)
                } else {
                    self.remembered(h).is_some_and(|seen| seen.road)
                }
            })
    }

    /// Player-facing job check for city `home`: unseen mutable objects come
    /// from memory, never from the live enemy board.
    fn known_job_problem(&self, home: usize, job: WorkerJob) -> Option<&'static str> {
        let hex = job.hex;
        let fog = self.fog();
        if !self.grid.contains(hex) || !self.grid.is_passable(hex) {
            return Some("WORKERS CAN'T WORK THIS TERRAIN");
        }
        if !self.known_worker_reach(hex) {
            return Some(
                "OUT OF REACH: WORKERS GO 3 TILES FROM A CITY OR WORK CAMP, OR NEXT TO A ROAD",
            );
        }
        if let Some(across) = job.across {
            return if hex.distance(across) != 1 || !self.grid.contains(across) {
                Some("WALLS AND GATES GO BETWEEN TWO TILES ON THE MAP")
            } else if self.known_barrier(hex, across, &fog).is_some() {
                Some("A WALL OR GATE STANDS HERE")
            } else {
                None
            };
        }
        let visible = fog.sees(hex);
        let remembered = self.remembered(hex);
        let city = if visible {
            self.cities.iter().any(|c| c.pos == hex)
        } else {
            remembered.is_some_and(|seen| seen.city.is_some())
                || self
                    .cities
                    .iter()
                    .any(|c| c.team == self.local_team && c.pos == hex)
        };
        if city {
            return Some("A CITY STANDS HERE");
        }
        let building = self
            .cities
            .iter()
            .filter(|c| visible || c.team == self.local_team)
            .any(|c| {
                crate::game::city::Building::PLACEABLE
                    .into_iter()
                    .any(|b| c.placed_site(b) == Some(hex))
            });
        let road = if visible {
            self.roads.contains(&hex)
        } else {
            remembered.is_some_and(|seen| seen.road)
        };
        let site = if visible {
            self.sites.get(&hex).map(|site| site.team)
        } else {
            remembered.and_then(|seen| seen.site.map(|(_, team)| team))
        };
        let structure = if visible {
            self.structures.contains_key(&hex)
        } else {
            remembered.is_some_and(|seen| seen.structure.is_some())
        };
        if let JobKind::Build(placed) = job.kind {
            if self.cities[home].built.contains(&placed) {
                return Some("THIS CITY HAS ONE ALREADY");
            }
            if building {
                return Some("SITE IS ALREADY CLAIMED BY A CITY OR BUILDING");
            }
            if structure {
                return Some("A STRUCTURE STANDS HERE");
            }
            // The site's own rules, but not a claim the player can't see:
            // that one drops the job (refunded) when a worker gets there.
            return self
                .ai_site_issue(home, placed, hex)
                .filter(|&issue| issue != "SITE IS ALREADY CLAIMED BY A CITY OR BUILDING");
        }
        match job.kind {
            JobKind::Road if road => Some("THERE IS A ROAD HERE ALREADY"),
            JobKind::Road => None,
            JobKind::Improve if building => Some("A BUILDING STANDS HERE"),
            JobKind::Improve => match site {
                Some(owner) if owner != self.local_team => Some("THIS TILE BELONGS TO THE ENEMY"),
                Some(_) => Some("THIS TILE IS IMPROVED ALREADY"),
                None if self.improvement(hex).is_none() => Some("NOTHING GROWS ON SNOW"),
                None => None,
            },
            _ if building => Some("A BUILDING STANDS HERE"),
            _ if structure => Some("A STRUCTURE STANDS HERE"),
            _ => None,
        }
    }

    /// Whether city `city` has a worker, at home or out, to build what it
    /// places.
    pub(super) fn has_workers(&self, city: usize) -> bool {
        self.cities[city].workers > 0 || self.workers_out(city) > 0
    }

    /// Whether `building` is among city `city`'s worker jobs, waiting or
    /// under way.
    pub(super) fn building_job_queued(&self, city: usize, building: Building) -> bool {
        let kind = JobKind::Build(building);
        self.cities[city].worker_jobs.iter().any(|j| j.kind == kind)
            || self
                .field_workers
                .iter()
                .any(|w| w.home == city && w.job.is_some_and(|j| j.kind == kind))
    }

    /// Pays for `job` from city `city`'s side and adds it to the city's
    /// worker jobs, or returns what the side is short.
    pub(super) fn try_queue_job(&mut self, city: usize, job: WorkerJob) -> Result<(), Stock> {
        let price = job.kind.price();
        let team = self.cities[city].team;
        if !self.stock(team).covers(price) {
            return Err(self.stock(team).shortfall(price));
        }
        *self.stock_mut(team) -= price;
        self.cities[city].worker_jobs.push(job);
        Ok(())
    }

    /// Gives `team` back what `job` cost: a job taken off the list, or
    /// dropped before it was done.
    fn refund_job(&mut self, team: Team, job: WorkerJob) {
        *self.stock_mut(team) += job.kind.price();
    }

    /// Turns of work `job` takes once its worker stands on it: its kind's,
    /// halved for a building beside one of its side's Workshops.
    pub(super) fn job_turns(&self, team: Team, job: WorkerJob) -> u32 {
        match job.kind {
            JobKind::Build(_) if self.beside_workshop(team, job.hex) => {
                job.kind.turns().div_ceil(2)
            }
            kind => kind.turns(),
        }
    }

    /// Why the open city `city` can't place `kind` anywhere right now: no
    /// worker to build it, a building it has or has placed already, or the
    /// price.
    pub(super) fn job_kind_unavailable(&self, city: usize, kind: JobKind) -> Option<String> {
        let c = &self.cities[city];
        if let JobKind::Build(building) = kind {
            if c.built.contains(&building) {
                return Some(format!("{} ALREADY EXISTS IN THIS CITY", building.name()));
            }
            if self.building_job_queued(city, building) {
                return Some(format!(
                    "{} IS ALREADY PLACED FOR THIS CITY",
                    building.name()
                ));
            }
            if matches!(building, Building::Harbor | Building::CoastalBattery)
                && !self.city_is_coastal(city)
            {
                return Some("ONLY COASTAL CITIES CAN BUILD NAVAL BUILDINGS".into());
            }
        }
        if !self.has_workers(city) {
            return Some("TRAIN A WORKER FIRST - WORKERS BUILD WHAT THE CITY PLACES".into());
        }
        let stock = self.stock(c.team);
        (!stock.covers(kind.price())).then(|| {
            format!(
                "{} - SHORT OF {}",
                kind.name(),
                stock_icons(stock.shortfall(kind.price()))
            )
        })
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

    /// A card in the open city's production list, or R and I: arms `kind`
    /// for placing on the map, or disarms it if it's the one armed already.
    pub fn arm_worker_job(&mut self, kind: JobKind) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.job_city() else {
            self.notice = "OPEN A CITY FIRST - ITS WORKERS BUILD WHAT IT PLACES".into();
            return;
        };
        if self.placing_job == Some(kind) {
            self.placing_job = None;
            self.hovered_job = None;
            self.notice = format!("STOPPED PLACING {}", kind.name());
            return;
        }
        if let Some(reason) = self.job_kind_unavailable(city, kind) {
            self.notice = reason;
            return;
        }
        self.placing_job = Some(kind);
        self.notice = if kind.on_edge() {
            format!(
                "CLICK OR DRAG ALONG HEX EDGES TO PLACE {}S - ESC TO STOP",
                kind.name()
            )
        } else {
            format!(
                "CLICK OR DRAG OVER LIT TILES TO PLACE {}S - ESC TO STOP",
                kind.name()
            )
        };
    }

    /// With a job armed: where a map point would place it, as the tile the
    /// point is in and, for a wall or gate, the neighbor across the edge
    /// nearest the point.
    pub(super) fn job_target_at(&self, point: Vec2) -> Option<(Hex, Option<Hex>)> {
        let kind = self.placing_job?;
        let hex = Hex::from_world(point);
        if !self.grid.contains(hex) {
            return None;
        }
        Some((hex, kind.on_edge().then(|| nearest_edge(hex, point))))
    }

    /// Places the armed job at `hex` (across the edge to `across`, for a
    /// wall or gate). Returns whether it was queued. A drag calls this for
    /// every tile or edge it passes, so a place that already has this job
    /// is skipped without a notice.
    pub(super) fn place_job_at(&mut self, hex: Hex, across: Option<Hex>) -> bool {
        let Some(kind) = self.placing_job.filter(|_| !self.is_resolving()) else {
            return false;
        };
        if let Some(across) = across {
            return self.queue_barrier_at(hex, across);
        }
        if self.job_taken(self.local_team, WorkerJob::on_tile(hex, kind)) {
            return false;
        }
        if let Some(reason) = self.job_unavailable(hex, kind) {
            self.notice = reason;
            return false;
        }
        self.queue_worker_job_at(hex, kind);
        true
    }

    /// Queues the armed wall or gate on the edge between `a` and `b`. The
    /// worker builds it from the side nearer its city. Returns whether it
    /// was queued; a drag calls this for every edge it passes, so an edge
    /// already queued is skipped without a notice.
    pub(super) fn queue_barrier_at(&mut self, a: Hex, b: Hex) -> bool {
        let Some(kind) = self.placing_job else {
            return false;
        };
        let Some(city) = self.job_city() else {
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
        if self.job_taken(self.local_team, job) {
            return false;
        }
        if let Some(problem) = self.known_job_problem(city, job) {
            self.notice = problem.into();
            return false;
        }
        if let Err(short) = self.try_queue_job(city, job) {
            self.notice = format!("{} - SHORT OF {}", kind.name(), stock_icons(short));
            return false;
        }
        self.notice = format!(
            "{} QUEUED FOR CITY {} - {} JOBS WAITING - ESC TO STOP",
            kind.name(),
            self.cities[city].id + 1,
            self.cities[city].worker_jobs.len()
        );
        true
    }

    /// With a job armed, places it at the tile or hex edge under `cursor`
    /// (window pixels). Called on the press and for every cursor move while
    /// the button is held, so a drag places it on each tile or edge it
    /// passes. Returns whether the press belongs to placing: false over the
    /// classic UI (`check_ui`) or with nothing armed.
    pub fn paint_job_at(&mut self, cursor: Vec2, screen_size: Vec2, check_ui: bool) -> bool {
        if self.placing_job.is_none() || self.is_resolving() {
            return false;
        }
        if check_ui && self.ui_covers(cursor, screen_size) {
            return false;
        }
        let point = self.camera.screen_to_world(cursor, screen_size);
        if let Some((hex, across)) = self.job_target_at(point) {
            self.place_job_at(hex, across);
        }
        true
    }

    /// Why the player can't queue `kind` at `hex` right now, if they can't.
    /// Walls and gates only need their city: their edge is picked on the
    /// map.
    pub(super) fn job_unavailable(&self, hex: Hex, kind: JobKind) -> Option<String> {
        let job = WorkerJob::on_tile(hex, kind);
        let Some(city) = self.job_city() else {
            return Some("OPEN A CITY FIRST - ITS WORKERS BUILD WHAT IT PLACES".into());
        };
        if let Some(reason) = self.job_kind_unavailable(city, kind) {
            Some(reason)
        } else if kind.on_edge() {
            None
        } else if !self.is_explored(hex) {
            Some("WORKERS CAN'T WORK AN UNEXPLORED TILE".into())
        } else if let Some(problem) = self.known_job_problem(city, job) {
            Some(problem.into())
        } else if self.job_taken(self.local_team, job) {
            Some(format!("{} IS QUEUED HERE ALREADY", kind.name()))
        } else {
            // A tile takes one job at a time.
            self.tile_job_at(self.local_team, hex)
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
        let Some(city) = self.job_city() else {
            return;
        };
        if let Err(short) = self.try_queue_job(city, WorkerJob::on_tile(hex, kind)) {
            self.notice = format!("{} - SHORT OF {}", kind.name(), stock_icons(short));
            return;
        }
        // A city has one of each building: placed, it's done placing.
        if matches!(kind, JobKind::Build(_)) {
            self.placing_job = None;
            self.hovered_job = None;
        }
        let home = self.cities[city].workers;
        self.notice = format!(
            "{} PLACED FOR CITY {} - {home} WORKER{} AT HOME",
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
        let Some(city) = self.worker_list_city() else {
            return;
        };
        if index < self.cities[city].worker_jobs.len() {
            let job = self.cities[city].worker_jobs.remove(index);
            self.refund_job(self.cities[city].team, job);
            self.notice = format!(
                "REMOVED {} FROM THE WORKER JOBS - REFUNDED",
                job.kind.name()
            );
        }
    }

    /// A worker's row in the city panel: the camera goes to the worker.
    pub fn show_worker(&mut self, id: u32) {
        if let Some(worker) = self
            .field_workers
            .iter()
            .find(|w| w.id == id && w.team == self.local_team)
        {
            self.camera.focus_on(worker.pos.to_world());
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
            .position(|w| w.id == id && w.team == self.local_team)
        else {
            return;
        };
        let worker = self.field_workers[w].clone();
        self.return_job(&worker);
        let worker = &mut self.field_workers[w];
        worker.job = None;
        worker.work_left = None;
        worker.recalled = true;
        worker.base = self.cities[worker.home].pos;
        self.notice = "WORKER RECALLED - IT HEADS HOME; ITS JOB WAITS ON THE LIST".into();
    }

    /// Takes the first job in `city`'s queue its workers can still do,
    /// dropping any that became impossible.
    fn take_job(&mut self, city: usize) -> Option<WorkerJob> {
        let team = self.cities[city].team;
        while !self.cities[city].worker_jobs.is_empty() {
            let job = self.cities[city].worker_jobs.remove(0);
            let Some(problem) = self.job_problem(city, job) else {
                return Some(job);
            };
            self.refund_job(team, job);
            if team == self.local_team {
                self.notice = format!("{} DROPPED, REFUNDED: {problem}", job.kind.name());
            }
        }
        None
    }

    fn work_base_for(&self, city: usize, job: WorkerJob) -> Hex {
        let center = self.cities[city].pos;
        self.cities[city]
            .placed_site(crate::game::city::Building::WorkCamp)
            .filter(|&camp| {
                camp.distance(job.hex) <= 3 && self.routes(city).costs.contains_key(&camp)
            })
            .unwrap_or(center)
    }

    fn return_base_for(&self, worker: &FieldWorker, home: usize) -> Hex {
        self.cities[home]
            .placed_site(crate::game::city::Building::WorkCamp)
            .filter(|&camp| worker.base == camp && self.routes(home).costs.contains_key(&camp))
            .unwrap_or(self.cities[home].pos)
    }

    /// The last step of a turn: workers heading home take newly queued jobs,
    /// cities send idle workers out, and every worker out on the map walks,
    /// works or arrives home. Returns the ids of the workers that did any of
    /// that.
    pub(super) fn resolve_workers(&mut self) -> Vec<u32> {
        for w in 0..self.field_workers.len() {
            if self.field_workers[w].job.is_none() && !self.field_workers[w].recalled {
                let home = self.field_workers[w].home;
                if let Some(job) = self.take_job(home) {
                    self.field_workers[w].base = self.work_base_for(home, job);
                    self.field_workers[w].job = Some(job);
                }
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
                let base = self.work_base_for(city, job);
                self.field_workers.push(FieldWorker {
                    id,
                    team: self.cities[city].team,
                    home: city,
                    base,
                    pos: base,
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
            let problem = match self.home_of(&worker) {
                Some(home) => self.job_problem(home, job),
                None => Some("ITS SIDE HAS NO CITY LEFT"),
            };
            if let Some(problem) = problem {
                self.refund_job(worker.team, job);
                if worker.team == self.local_team {
                    self.notice = format!("{} ABANDONED, REFUNDED: {problem}", job.kind.name());
                }
                self.field_workers[w].job = None;
                self.field_workers[w].work_left = None;
                return (true, false);
            }
            if left > 1 {
                self.field_workers[w].work_left = Some(left - 1);
                return (true, false);
            }
            let Some(home) = self.home_of(&worker) else {
                return (false, false);
            };
            self.complete_job(home, job);
            let next = self.take_job(worker.home);
            if let Some(next_job) = next {
                self.field_workers[w].base = self.work_base_for(worker.home, next_job);
            }
            self.field_workers[w].job = next;
            self.field_workers[w].work_left = None;
            return (true, false);
        }

        // Walking: to the job, or home.
        let Some(home) = self.home_of(&worker) else {
            return (false, false);
        };
        let target = worker
            .job
            .map_or_else(|| self.return_base_for(&worker, home), |job| job.hex);
        let Some(path) = self.worker_path(worker.team, worker.pos, target) else {
            if let Some(job) = worker.job {
                self.refund_job(worker.team, job);
                if worker.team == self.local_team {
                    self.notice = format!(
                        "A WORKER CAN'T REACH ITS {} - IT'S COMING HOME, REFUNDED",
                        self.job_title(job)
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
                let team = worker.team;
                self.field_workers[w].work_left = Some(self.job_turns(team, job));
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

    fn complete_job(&mut self, city: usize, job: WorkerJob) {
        let team = self.cities[city].team;
        let hex = job.hex;
        match job.kind {
            JobKind::Build(building) => {
                let c = &mut self.cities[city];
                c.set_placed_site(building, hex);
                if building == Building::CoastalBattery {
                    c.coastal_battery_hp = 150.0;
                }
                c.built.push(building);
            }
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
        if team == self.local_team {
            self.notice = format!("WORKERS FINISHED: {}", self.job_title(job));
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
            if worker.team == self.local_team {
                self.notice = "AN ENEMY CAPTURED ONE OF YOUR WORKERS".into();
            } else if captor_team == self.local_team {
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
            if worker.team == self.local_team {
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
            let stock = self.stock(team);
            let worked = c.worked.clone();
            let job = [JobKind::Improve, JobKind::Road]
                .into_iter()
                .find_map(|kind| {
                    worked.iter().find_map(|&hex| {
                        let job = WorkerJob::on_tile(hex, kind);
                        (stock.covers(kind.price())
                            && self.job_problem(city, job).is_none()
                            && !self.job_taken(team, job))
                        .then_some(job)
                    })
                });
            if let Some(job) = job {
                let _ = self.try_queue_job(city, job);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PLAYER_TEAM;
    use crate::game::city::Build;
    use crate::game::turn::{Phase, Step};
    use crate::game::unit::UnitType;

    /// The Cities scenario with no units, the fog lifted, and Blue's city
    /// (index 0, one worker at home) at (-4, 0), open.
    fn cities() -> GameState {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.selected = None;
        // Jobs are placed from the open city.
        game.selected_city = Some(0);
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
        game.placing_job = Some(kind);
        game.place_job_at(hex, None);
        game.placing_job = None;
    }

    #[test]
    fn hidden_enemy_road_and_farm_do_not_change_player_job_planning() {
        use crate::game::city::Site;
        use crate::game::unit::Unit;
        let (mut game, _, far) = crate::game::fog::tests::remembered_route_hex();
        let road = far
            .neighbors()
            .into_iter()
            .find(|&h| game.grid.is_passable(h) && game.cities.iter().all(|c| c.pos != h))
            .unwrap();
        game.roads.insert(road);
        game.units
            .push(Unit::new(900, road, PLAYER_TEAM, UnitType::Scout));
        game.explore();
        game.units.clear();
        assert!(!game.fog().sees(far));
        assert!(game.known_worker_reach(far));
        let before = game.job_unavailable(far, JobKind::Improve);
        let reach_before: Vec<_> = game
            .grid
            .all_hexes()
            .map(|h| (h, game.known_worker_reach(h)))
            .collect();
        game.roads.insert(far);
        game.sites.insert(
            far,
            Site {
                team: Team::Red,
                food: 9,
                production: 0,
                label: "FARM",
            },
        );
        assert_eq!(game.job_unavailable(far, JobKind::Improve), before);
        let reach_after: Vec<_> = game
            .grid
            .all_hexes()
            .map(|h| (h, game.known_worker_reach(h)))
            .collect();
        assert_eq!(reach_after, reach_before);
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
    fn jobs_are_placed_from_the_open_city_until_escape_or_leaving_it() {
        let mut game = GameState::city_scenario();
        game.explore();
        // No city open: nothing to place from.
        game.arm_worker_job(JobKind::Road);
        assert_eq!(game.placing_job, None);
        assert!(game.notice.contains("OPEN A CITY FIRST"), "{}", game.notice);

        // R picks roads in the open city; Escape puts them down, keeping
        // the city open.
        game.open_city(0);
        game.arm_worker_job(JobKind::Road);
        assert_eq!(game.placing_job, Some(JobKind::Road));
        game.press_escape();
        assert_eq!(game.placing_job, None);
        assert_eq!(game.selected_city, Some(0));
        // Leaving the city stops placing too.
        game.arm_worker_job(JobKind::Road);
        game.leave_city_view();
        assert_eq!(game.placing_job, None);

        // A city with no worker places nothing.
        game.open_city(0);
        game.cities[0].workers = 0;
        game.arm_worker_job(JobKind::Road);
        assert_eq!(game.placing_job, None);
        assert_eq!(
            game.notice,
            "TRAIN A WORKER FIRST - WORKERS BUILD WHAT THE CITY PLACES"
        );
    }

    #[test]
    fn a_job_is_paid_when_placed_and_refunded_if_taken_off() {
        let mut game = cities();
        let tile = bare_tile(&game, 1);
        let before = game.stock(PLAYER_TEAM);
        queue(&mut game, tile, JobKind::Road);
        assert_eq!(game.stock(PLAYER_TEAM), before - JobKind::Road.price());
        game.remove_worker_job(0);
        assert_eq!(game.stock(PLAYER_TEAM), before);
        // Short of wood: nothing placed, and the notice says so.
        *game.stock_mut(PLAYER_TEAM) = Stock::default();
        queue(&mut game, tile, JobKind::Road);
        assert!(game.cities[0].worker_jobs.is_empty());
        assert!(
            game.notice.starts_with("ROAD - SHORT OF"),
            "{}",
            game.notice
        );
    }

    #[test]
    fn a_job_can_not_go_on_an_unexplored_tile() {
        let mut game = cities();
        let unseen = Hex::new(-2, 3);
        game.memory.remove(&unseen);
        game.fog_of_war = true;
        assert!(!game.is_explored(unseen));
        game.placing_job = Some(JobKind::Road);
        assert!(!game.place_job_at(unseen, None));
        assert_eq!(game.notice, "WORKERS CAN'T WORK AN UNEXPLORED TILE");
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
    fn new_placed_buildings_keep_worker_structures_off_their_tile() {
        let mut game = cities();
        let tile = bare_tile(&game, 2);
        game.cities[0]
            .extra_buildings
            .insert(crate::game::city::Building::FieldHospital, tile);
        queue(&mut game, tile, JobKind::Improve);
        assert!(game.cities[0].worker_jobs.is_empty());
        assert!(game.notice.contains("BUILDING"));
        queue(&mut game, tile, JobKind::Fort);
        assert!(game.cities[0].worker_jobs.is_empty());
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
        assert_eq!(game.field_workers[0].work_left, Some(3));
        // Three turns of work.
        for _ in 0..2 {
            game.resolve_workers();
            assert!(!game.sites.contains_key(&hex));
        }
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
    fn a_connected_work_camp_extends_the_reach() {
        let mut game = cities();
        // Five from Blue's city at (-4, 0), with no road by it.
        let far = Hex::new(1, 0);
        assert!(!game.in_worker_reach(PLAYER_TEAM, far));
        let camp = Hex::new(-1, 0);
        game.cities[0]
            .extra_buildings
            .insert(crate::game::city::Building::WorkCamp, camp);
        assert!(game.routes(0).costs.contains_key(&camp), "connected");
        assert_eq!(game.worker_bases(PLAYER_TEAM), [game.cities[0].pos, camp]);
        assert!(game.in_worker_reach(PLAYER_TEAM, far), "two from the camp");
        assert!(game.job_unavailable(far, JobKind::Road).is_none());
        assert!(
            !game.in_worker_reach(PLAYER_TEAM, Hex::new(2, -4)),
            "four from it"
        );
    }

    #[test]
    fn connected_work_camp_is_a_fast_base_for_nearby_jobs() {
        let mut game = cities();
        game.roads.clear();
        let camp = Hex::new(-3, 1);
        let job = Hex::new(-2, 1);
        game.cities[0]
            .extra_buildings
            .insert(crate::game::city::Building::WorkCamp, camp);
        assert!(game.routes(0).costs.contains_key(&camp));
        queue(&mut game, job, JobKind::Road);
        game.resolve_workers();
        assert_eq!(game.field_workers[0].base, camp);
        assert_eq!(game.field_workers[0].pos, job);
        // Two turns of work.
        game.resolve_workers();
        assert!(!game.roads.contains(&job));
        game.resolve_workers();
        assert!(game.roads.contains(&job));
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
        // A turn to get there, two of work.
        for _ in 0..3 {
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
            base: game.cities[0].pos,
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
        game.arm_worker_job(JobKind::Wall);
        assert_eq!(game.placing_job, Some(JobKind::Wall));
        assert!(game.queue_barrier_at(far, near));
        let job = game.cities[0].worker_jobs[0];
        assert_eq!((job.hex, job.across), (near, Some(far)));
        // The same edge, either way round, or as a gate, isn't queued again.
        assert!(!game.queue_barrier_at(near, far));
        game.placing_job = Some(JobKind::Gate);
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
