//! Builds: the city queue (units and buildings), building sites and
//! confirmation, the Barracks queue, and completing builds. What builds
//! cost and how the stockpile pays for them is in `economy.rs`.
use super::MAX_CITY_POPULATION;
use super::barracks::CITY_TRAINING_SLOWDOWN;
use super::economy::{Stock, WORK_PER_TURN, stock_icons, turns_icon};
use crate::game::JobKind;
use crate::game::hex::Hex;
use crate::game::terrain::{Resource, Terrain};
use crate::game::unit::{Team, Unit, UnitType};
use crate::game::{GameState, PLAYER_TEAM};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Building {
    Barracks,
    Mill,
    Workshop,
    CanoeHouse,
    Forge,
    Stable,
    Watchpost,
    FieldHospital,
    Cannery,
    WorkCamp,
    Smelter,
    Railhead,
    Harbor,
    CoastalBattery,
}

impl Building {
    pub const ALL: [Self; 14] = [
        Self::Barracks,
        Self::Mill,
        Self::Workshop,
        Self::CanoeHouse,
        Self::Forge,
        Self::Stable,
        Self::Watchpost,
        Self::FieldHospital,
        Self::Cannery,
        Self::WorkCamp,
        Self::Smelter,
        Self::Railhead,
        Self::Harbor,
        Self::CoastalBattery,
    ];
    /// Every building stands on a site on the map, which the city's
    /// workers build it on (`workers.rs`).
    pub const PLACEABLE: [Self; 14] = Self::ALL;
    pub fn name(self) -> &'static str {
        match self {
            Self::Barracks => "BARRACKS",
            Self::Mill => "MILL",
            Self::Workshop => "WORKSHOP",
            Self::CanoeHouse => "CANOE HOUSE",
            Self::Forge => "FORGE",
            Self::Stable => "STABLE",
            Self::Watchpost => "WATCHPOST",
            Self::FieldHospital => "FIELD HOSPITAL",
            Self::Cannery => "CANNERY",
            Self::WorkCamp => "WORK CAMP",
            Self::Smelter => "SMELTER",
            Self::Railhead => "RAILHEAD",
            Self::Harbor => "HARBOR",
            Self::CoastalBattery => "COASTAL BATTERY",
        }
    }
    /// What queuing it takes from the side's stockpile (`economy.rs`).
    pub fn price(self) -> Stock {
        let (food, wood, metal) = match self {
            Self::Barracks => (0, 10, 0),
            Self::Mill | Self::CanoeHouse | Self::Watchpost => (0, 10, 0),
            Self::Workshop => (0, 10, 4),
            Self::Forge => (0, 6, 8),
            Self::Stable => (2, 12, 0),
            Self::FieldHospital => (4, 10, 4),
            Self::Cannery => (0, 12, 4),
            Self::WorkCamp => (2, 10, 2),
            Self::Smelter => (0, 8, 8),
            Self::Railhead => (0, 12, 12),
            Self::Harbor => (0, 14, 0),
            Self::CoastalBattery => (0, 8, 10),
        };
        Stock::whole(food, wood, metal)
    }
    /// Turns it takes at the head of the queue.
    pub fn turns(self) -> i32 {
        match self {
            Self::Barracks | Self::Mill | Self::CanoeHouse | Self::Watchpost | Self::WorkCamp => 3,
            Self::Workshop
            | Self::Forge
            | Self::Stable
            | Self::FieldHospital
            | Self::Cannery
            | Self::Smelter
            | Self::Harbor
            | Self::CoastalBattery => 4,
            Self::Railhead => 5,
        }
    }
    pub fn shortcut(self) -> char {
        match self {
            Self::Barracks => '5',
            Self::Mill => '6',
            Self::Workshop => '7',
            Self::CanoeHouse
            | Self::Forge
            | Self::Stable
            | Self::Watchpost
            | Self::FieldHospital
            | Self::Cannery
            | Self::WorkCamp
            | Self::Smelter
            | Self::Railhead
            | Self::Harbor
            | Self::CoastalBattery => ' ',
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Barracks => {
                "TRAINS TROOPS TWICE AS FAST. ON OPEN LAND; ON HORSES OR IRON, ALSO CAVALRY OR ARMORED."
            }
            Self::Mill => "ADJACENT WORKED TILES DELIVER ALL THEIR FOOD.",
            Self::Workshop => "ADJACENT BUILDINGS TAKE HALF THE TURNS.",
            Self::CanoeHouse => "ON A RIVERBANK: THE RIVER CARRIES GOODS LIKE A ROAD.",
            Self::Forge => "NEXT TO IRON AND A BARRACKS: TOUGHER ARMORED.",
            Self::Stable => "NEXT TO HORSES AND A BARRACKS: FASTER CAVALRY.",
            Self::Watchpost => "SEES 4 HEXES, 5 FROM HILLS.",
            Self::FieldHospital => "HEALS 2 NEARBY TROOPS A TURN.",
            Self::Cannery => "COLLECTS FOOD FROM 3 IMPROVEMENTS WITHIN 3 HEXES.",
            Self::WorkCamp => "NEARBY JOBS START FROM HERE.",
            Self::Smelter => "COLLECTS METAL FROM 3 MINES WITHIN 3 HEXES.",
            Self::Railhead => "TROOPS BY THE CITY REACH IT IN ONE TURN, ALONG A ROAD.",
            Self::Harbor => "ON THE COAST: TRAINS SHIPS.",
            Self::CoastalBattery => "ON THE COAST: FIRES AT SHIPS WITHIN 2.",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Build {
    Unit(BuildUnit),
    /// A worker for the city's pool (`workers.rs`).
    Worker,
    /// One more citizen, bought with food (`economy::grow_price`).
    Grow,
    /// The city spends a turn gathering: free, and `GATHER_YIELD` goes to
    /// the stockpile when it's done. Something a city can always do.
    Gather,
}

/// The Worker, Grow and Gather cards' keys.
pub(in crate::game) const WORKER_SHORTCUT: char = '8';
pub(in crate::game) const GROW_SHORTCUT: char = '9';
pub(in crate::game) const GATHER_SHORTCUT: char = '0';
/// What a turn of gathering (`Build::Gather`) brings in.
pub(in crate::game) const GATHER_YIELD: Stock = Stock::whole(2, 2, 1);

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unit(u) => u.name(),
            Self::Worker => "WORKER",
            Self::Grow => "GROW",
            Self::Gather => "GATHER",
        }
    }
    /// Its price, except a Grow's, which depends on the city
    /// (`GameState::queue_price`).
    pub fn price(self) -> Stock {
        match self {
            Self::Unit(u) => u.price(),
            Self::Worker => Stock::whole(4, 2, 0),
            Self::Grow | Self::Gather => Stock::default(),
        }
    }
    pub fn turns(self) -> i32 {
        match self {
            Self::Unit(u) => u.turns(),
            Self::Worker | Self::Grow => 2,
            Self::Gather => 1,
        }
    }
    /// Work it needs, in quarter turns.
    pub fn work(self) -> i32 {
        self.turns() * WORK_PER_TURN
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildUnit {
    Melee,
    Ranged,
    Cavalry,
    Siege,
    Armored,
    PatrolGalley,
    LandingCraft,
    BombardShip,
}

impl BuildUnit {
    pub(in crate::game) fn unit_type(self) -> UnitType {
        match self {
            Self::Melee => UnitType::Melee,
            Self::Ranged => UnitType::Ranged,
            Self::Cavalry => UnitType::Cavalry,
            Self::Siege => UnitType::Siege,
            Self::Armored => UnitType::Armored,
            Self::PatrolGalley => UnitType::PatrolGalley,
            Self::LandingCraft => UnitType::LandingCraft,
            Self::BombardShip => UnitType::BombardShip,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Melee => "MELEE",
            Self::Ranged => "RANGED",
            Self::Cavalry => "CAVALRY",
            Self::Siege => "SIEGE",
            Self::Armored => "ARMORED",
            Self::PatrolGalley => "PATROL GALLEY",
            Self::LandingCraft => "LANDING CRAFT",
            Self::BombardShip => "BOMBARD SHIP",
        }
    }
    /// What queuing it takes from the side's stockpile (`economy.rs`).
    pub fn price(self) -> Stock {
        let (food, wood, metal) = match self {
            Self::Melee => (2, 6, 0),
            Self::Ranged => (2, 7, 0),
            Self::Cavalry => (3, 4, 3),
            Self::Siege => (1, 8, 4),
            Self::Armored => (3, 2, 7),
            Self::PatrolGalley => (1, 10, 2),
            Self::LandingCraft => (1, 12, 2),
            Self::BombardShip => (1, 12, 6),
        };
        Stock::whole(food, wood, metal)
    }
    /// Turns it takes at the head of the queue.
    pub fn turns(self) -> i32 {
        match self {
            Self::Melee | Self::Ranged => 2,
            Self::Cavalry | Self::Siege | Self::Armored | Self::PatrolGalley => 3,
            Self::LandingCraft | Self::BombardShip => 4,
        }
    }
    /// Work it needs, in quarter turns.
    pub fn work(self) -> i32 {
        self.turns() * WORK_PER_TURN
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Melee => "TOUGH CLOSE FIGHTER",
            Self::Ranged => "FIRES FROM 2 TILES",
            Self::Cavalry => "FAST FLANKER",
            Self::Siege => "LONG RANGE, SLOW",
            Self::Armored => "HEAVY IRON INFANTRY",
            Self::PatrolGalley => "FAST COASTAL FIGHTER; STRONG AGAINST SHIPS",
            Self::LandingCraft => "CARRIES UP TO FOUR LAND TROOPS",
            Self::BombardShip => "RANGE 3 SHORE AND SHIP BOMBARDMENT",
        }
    }
    pub fn shortcut(self) -> char {
        match self {
            Self::Melee => '1',
            Self::Ranged => '2',
            Self::Siege => '3',
            Self::Cavalry
            | Self::Armored
            | Self::PatrolGalley
            | Self::LandingCraft
            | Self::BombardShip => '-',
        }
    }

    pub fn required_resource(self) -> Option<Resource> {
        match self {
            Self::Cavalry => Some(Resource::Horses),
            Self::Armored => Some(Resource::Iron),
            _ => None,
        }
    }
}

impl GameState {
    /// A city's center must touch sea water to support naval construction.
    pub(in crate::game) fn city_is_coastal(&self, city: usize) -> bool {
        self.cities[city].pos.neighbors().into_iter().any(|hex| {
            self.grid.contains(hex)
                && matches!(self.grid.terrain(hex), Terrain::Coast | Terrain::Ocean)
        })
    }

    pub fn queue_selected_city_unit(&mut self, build: BuildUnit) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != PLAYER_TEAM {
            return;
        }
        if build.unit_type().is_naval() && !self.city_is_coastal(city) {
            self.notice = "ONLY COASTAL CITIES CAN BUILD SHIPS".into();
            return;
        }
        if build.unit_type().is_naval() && self.cities[city].placed_site(Building::Harbor).is_none()
        {
            self.notice = "BUILD A HARBOR BEFORE TRAINING SHIPS".into();
            return;
        }
        if let Some(resource) = build.required_resource() {
            self.notice = format!(
                "{} TRAINS AT A BARRACKS ON {}",
                build.name(),
                resource.name()
            );
            return;
        }
        self.queue_paid(city, Build::Unit(build));
    }

    /// Pays for `build` and queues it in the player's `city`, or says what
    /// the side is short. Returns whether it was queued.
    fn queue_paid(&mut self, city: usize, build: Build) -> bool {
        let price = self.queue_price(city, build);
        match self.try_queue_build(city, build) {
            Ok(()) => {
                self.notice = format!(
                    "QUEUED {} - PAID {} - {}",
                    build.name(),
                    stock_icons(price),
                    turns_icon(self.city_build_turns(city, build))
                );
                true
            }
            Err(short) => {
                self.notice = format!("{} - SHORT OF {}", build.name(), stock_icons(short));
                false
            }
        }
    }

    /// 9 or the Grow card: one more citizen, bought with food.
    pub fn queue_selected_city_growth(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != PLAYER_TEAM {
            return;
        }
        if !self.can_grow(city) {
            self.notice = format!("A CITY HOLDS AT MOST {MAX_CITY_POPULATION} CITIZENS");
            return;
        }
        self.queue_paid(city, Build::Grow);
    }

    /// Whether another Grow fits: the city's population plus the Grows
    /// already queued stays under the cap.
    pub(in crate::game) fn can_grow(&self, city: usize) -> bool {
        let c = &self.cities[city];
        let queued = c.queue.iter().filter(|&&b| b == Build::Grow).count();
        c.population + queued < MAX_CITY_POPULATION
    }

    /// 8 or the Worker button: a worker for the open city's pool.
    pub fn queue_selected_city_worker(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != PLAYER_TEAM {
            return;
        }
        self.queue_paid(city, Build::Worker);
    }

    pub fn queue_selected_city_building(&mut self, building: Building) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILDING".into();
            return;
        };
        if matches!(building, Building::Harbor | Building::CoastalBattery)
            && !self.city_is_coastal(city)
        {
            self.notice = "ONLY COASTAL CITIES CAN BUILD NAVAL BUILDINGS".into();
            return;
        }
        let c = &self.cities[city];
        if c.team != PLAYER_TEAM {
            return;
        }
        if c.built.contains(&building) {
            self.notice = format!("{} ALREADY EXISTS IN THIS CITY", building.name());
            return;
        }
        // Placed on the map, and the city's workers build it there
        // (`workers.rs`).
        self.arm_worker_job(JobKind::Build(building));
    }

    /// 0 or the Gather card: the open city spends a turn gathering.
    pub fn queue_selected_city_gather(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != PLAYER_TEAM {
            return;
        }
        self.queue_paid(city, Build::Gather);
    }

    /// The first reason `building` can't stand on `hex` for city `city`:
    /// the rules of its site (workers' reach and exploration aside, which
    /// `job_problem` and `job_unavailable` check).
    pub(in crate::game) fn ai_site_issue(
        &self,
        city: usize,
        building: Building,
        hex: Hex,
    ) -> Option<&'static str> {
        if !self.grid.is_passable(hex) {
            return Some("NEEDS AN OPEN LAND TILE");
        }
        if self.cities.iter().any(|c| {
            c.pos == hex
                || Building::PLACEABLE
                    .into_iter()
                    .any(|kind| c.placed_site(kind) == Some(hex))
        }) {
            return Some("SITE IS ALREADY CLAIMED BY A CITY OR BUILDING");
        }
        if matches!(building, Building::Harbor | Building::CoastalBattery)
            && !self.city_is_coastal(city)
        {
            return Some("NEEDS A CITY CENTER ON THE COAST");
        }
        match building {
            Building::CanoeHouse
                if !hex
                    .neighbors()
                    .into_iter()
                    .any(|n| self.grid.has_river(hex, n)) =>
            {
                Some("NEEDS A RIVERBANK TILE")
            }
            Building::Forge if !self.resource_near(hex, Resource::Iron) => {
                Some("NEEDS IRON ON OR NEXT TO THE TILE")
            }
            Building::Stable if !self.resource_near(hex, Resource::Horses) => {
                Some("NEEDS HORSES ON OR NEXT TO THE TILE")
            }
            Building::Harbor | Building::CoastalBattery
                if !hex.neighbors().into_iter().any(|n| {
                    self.grid.contains(n)
                        && matches!(self.grid.terrain(n), Terrain::Coast | Terrain::Ocean)
                }) =>
            {
                Some("NEEDS A TILE NEXT TO COAST OR OCEAN")
            }
            Building::Smelter
                if !self.grid.tile(hex).hills
                    && !hex.neighbors().into_iter().any(|n| {
                        self.grid.contains(n)
                            && (self.grid.tile(n).hills
                                || self.grid.resource(n) == Some(Resource::Iron))
                    }) =>
            {
                Some("NEEDS HILLS OR IRON ON OR NEXT TO THE TILE")
            }
            _ => None,
        }
    }

    /// Tests: whether `building` may stand on `hex` for city `city`, by its
    /// site's rules alone (`ai_site_issue`).
    #[cfg(test)]
    pub(in crate::game) fn site_available(
        &self,
        city: usize,
        building: Building,
        hex: Hex,
    ) -> bool {
        self.ai_site_issue(city, building, hex).is_none()
    }

    pub(in crate::game) fn resource_near(&self, hex: Hex, resource: Resource) -> bool {
        self.grid.resource(hex) == Some(resource)
            || hex
                .neighbors()
                .into_iter()
                .any(|n| self.grid.resource(n) == Some(resource))
    }

    fn barracks_support(&self, city: usize, resource: Resource) -> bool {
        let c = &self.cities[city];
        let (kind, barracks) = match resource {
            Resource::Iron => (Building::Forge, c.barracks),
            Resource::Horses => (Building::Stable, c.barracks),
        };
        barracks.is_some_and(|b| {
            c.placed_site(kind)
                .is_some_and(|site| b.distance(site) == 1 && self.resource_near(site, resource))
        })
    }

    pub(in crate::game) fn beside_workshop(&self, team: Team, site: Hex) -> bool {
        self.cities.iter().any(|c| {
            c.team == team
                && c.workshop
                    .is_some_and(|workshop| workshop.distance(site) == 1)
        })
    }

    /// Work `build` needs in `city`'s own queue: its own, and for a land
    /// troop `CITY_TRAINING_SLOWDOWN` times a Barracks' (`barracks.rs`).
    pub(in crate::game) fn city_build_work(&self, _city: usize, build: Build) -> i32 {
        match build {
            Build::Unit(unit) if !unit.unit_type().is_naval() => {
                unit.work() * CITY_TRAINING_SLOWDOWN
            }
            _ => build.work(),
        }
    }

    /// Turns `build` takes in `city`'s own queue (`city_build_work`).
    pub(in crate::game) fn city_build_turns(&self, city: usize, build: Build) -> i32 {
        (self.city_build_work(city, build) + WORK_PER_TURN - 1) / WORK_PER_TURN
    }

    pub fn queue_selected_barracks_unit(&mut self, build: BuildUnit) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        if self.cities[city].team != PLAYER_TEAM || self.cities[city].barracks.is_none() {
            return;
        }
        if let Some(reason) = self.barracks_lock(city, build) {
            self.notice = format!("{}: {reason}", build.name());
            return;
        }
        self.notice = match self.try_queue_barracks(city, build) {
            Ok(()) => format!(
                "BARRACKS TRAINING {} - PAID {} - {}",
                build.name(),
                stock_icons(build.price()),
                turns_icon(build.turns())
            ),
            Err(short) => format!("{} - SHORT OF {}", build.name(), stock_icons(short)),
        };
    }

    /// Whether city `city`'s Barracks can train `build` at all: it has one,
    /// and a deposit for a troop that needs Horses or Iron (`barracks.rs`),
    /// whatever the cap.
    #[cfg(test)]
    pub(in crate::game) fn barracks_can_train(&self, city: usize, build: BuildUnit) -> bool {
        self.cities[city].barracks.is_some()
            && build
                .required_resource()
                .is_none_or(|resource| !self.barracks_deposits(city, resource).is_empty())
    }

    /// Completes only the active queue in the currently open structure.
    pub fn debug_complete_current_production(&mut self) {
        if self.is_resolving() {
            return;
        }
        if let Some(city) = self.selected_barracks {
            let Some(build) = self.cities[city].barracks_queue.first().copied() else {
                return;
            };
            self.cities[city].barracks_progress = build.work();
            self.complete_builds_for(Some((city, true)));
            self.notice = if self.cities[city].barracks_queue.first() == Some(&build) {
                "DEBUG: PRODUCTION READY - NO OPEN SPAWN TILE".into()
            } else {
                format!("DEBUG: {} TRAINING COMPLETED", build.name())
            };
        } else if let Some(city) = self.selected_city {
            let Some(build) = self.cities[city].queue.first().copied() else {
                return;
            };
            self.cities[city].progress = self.city_build_work(city, build);
            self.complete_builds_for(Some((city, false)));
            self.notice = if self.cities[city].queue.first() == Some(&build) {
                "DEBUG: PRODUCTION READY - NO OPEN SPAWN TILE".into()
            } else {
                format!("DEBUG: {} PRODUCTION COMPLETED", build.name())
            };
        }
    }

    pub fn move_selected_city_queue_item(&mut self, index: usize, up: bool) {
        let Some(city) = self.selected_city else {
            return;
        };
        let queue = &mut self.cities[city].queue;
        let other = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        };
        if let Some(other) = other.filter(|&other| other < queue.len()) {
            queue.swap(index, other);
            self.notice = "CITY QUEUE REORDERED".into();
        }
    }

    pub fn remove_selected_city_queue_item(&mut self, index: usize) {
        let Some(city) = self.selected_city else {
            return;
        };
        if index >= self.cities[city].queue.len() {
            return;
        }
        let removed = self.take_queue_item(city, index);
        self.notice = format!("REMOVED {} FROM CITY QUEUE - REFUNDED", removed.name());
    }

    #[cfg(test)]
    pub fn move_selected_barracks_queue_item(&mut self, index: usize, up: bool) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        let queue = &mut self.cities[city].barracks_queue;
        let other = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        };
        if let Some(other) = other.filter(|&other| other < queue.len()) {
            queue.swap(index, other);
            self.notice = "BARRACKS QUEUE REORDERED".into();
        }
    }

    pub fn remove_selected_barracks_queue_item(&mut self, index: usize) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        if index >= self.cities[city].barracks_queue.len() {
            return;
        }
        let removed = self.take_barracks_item(city, index);
        self.notice = format!("REMOVED {} FROM BARRACKS QUEUE - REFUNDED", removed.name());
    }

    /// Queue hotkeys operate on the city line currently being produced.
    pub fn remove_selected_city_queue_head(&mut self) {
        self.remove_selected_city_queue_item(0);
    }
    pub fn move_selected_city_queue_head(&mut self, up: bool) {
        self.move_selected_city_queue_item(0, up);
    }

    pub(in crate::game) fn complete_builds(&mut self) {
        self.complete_builds_for(None);
    }

    /// `only` limits debug completion to one city's selected production lane.
    fn complete_builds_for(&mut self, only: Option<(usize, bool)>) {
        let mut spawn = Vec::new();
        // Where each Barracks troop that drew on a deposit appears, and which.
        let mut drawn = Vec::new();
        for i in 0..self.cities.len() {
            if only.is_some_and(|lane| lane != (i, false)) {
                continue;
            }
            let Some(build) = self.cities[i].queue.first().copied() else {
                continue;
            };
            if self.cities[i].progress < self.city_build_work(i, build) {
                continue;
            }
            if build == Build::Gather {
                let c = &mut self.cities[i];
                c.progress -= build.work();
                c.queue.remove(0);
                let (team, id) = (c.team, c.id);
                *self.stock_mut(team) += GATHER_YIELD;
                if team == PLAYER_TEAM {
                    self.notice = format!("CITY {} GATHERED {}", id + 1, stock_icons(GATHER_YIELD));
                }
                continue;
            }
            if build == Build::Worker {
                self.cities[i].progress -= build.work();
                self.cities[i].queue.remove(0);
                self.cities[i].workers += 1;
                log::info!("{:?} city completed a worker", self.cities[i].team);
                if self.cities[i].team == PLAYER_TEAM {
                    self.notice =
                        "WORKER READY - PLACE ROADS, IMPROVEMENTS AND BUILDINGS FROM THE CITY"
                            .into();
                }
                continue;
            }
            if build == Build::Grow {
                let c = &mut self.cities[i];
                c.progress -= build.work();
                c.queue.remove(0);
                // Starving can't take it over the cap, but a Grow bought
                // before the cap was reached could.
                c.population = (c.population + 1).min(MAX_CITY_POPULATION);
                log::info!("{:?} city {} grew to {}", c.team, c.id + 1, c.population);
                if c.team == PLAYER_TEAM {
                    self.notice = format!("CITY {} GREW TO {}", c.id + 1, c.population);
                }
                continue;
            }
            let city = self.cities[i].pos;
            let naval = matches!(build, Build::Unit(unit) if unit.unit_type().is_naval());
            let origin = if naval {
                if !self.city_is_coastal(i) {
                    self.cities[i].progress = self.city_build_work(i, build);
                    continue;
                }
                let Some(harbor) = self.cities[i].placed_site(Building::Harbor) else {
                    self.cities[i].progress = self.city_build_work(i, build);
                    continue;
                };
                harbor
            } else {
                city
            };
            let Some(pos) = origin.neighbors().into_iter().find(|&h| {
                if naval {
                    self.is_open_naval_spawn(h, self.cities[i].team, &spawn)
                } else {
                    self.is_open_spawn(h, self.cities[i].team, &spawn)
                }
            }) else {
                // The city holds the finished unit until a hex opens, and
                // banks nothing more meanwhile: a bank would let the rest of
                // the queue come out one unit a turn once one did (#54).
                self.cities[i].progress = self.city_build_work(i, build);
                continue;
            };
            self.cities[i].progress -= self.city_build_work(i, build);
            self.cities[i].queue.remove(0);
            let Build::Unit(unit) = build else {
                unreachable!()
            };
            spawn.push((self.cities[i].team, pos, unit.unit_type(), None));
        }
        for i in 0..self.cities.len() {
            if only.is_some_and(|lane| lane != (i, true)) {
                continue;
            }
            let Some(build) = self.cities[i].barracks_queue.first().copied() else {
                continue;
            };
            if self.cities[i].barracks_progress < build.work() {
                continue;
            }
            let Some(barracks) = self.cities[i].barracks else {
                continue;
            };
            let Some(pos) = barracks
                .neighbors()
                .into_iter()
                .find(|&h| self.is_open_spawn(h, self.cities[i].team, &spawn))
            else {
                continue;
            };
            self.cities[i].barracks_progress = 0;
            self.cities[i].barracks_queue.remove(0);
            let upgrade = build
                .required_resource()
                .filter(|&resource| self.barracks_support(i, resource));
            spawn.push((self.cities[i].team, pos, build.unit_type(), upgrade));
            if let Some(resource) = build.required_resource() {
                let team = self.cities[i].team;
                self.special_trained[team.index()][resource.index()] += 1;
                drawn.push((pos, resource));
            }
        }
        for (team, pos, kind, upgrade) in spawn {
            let id = self.next_unit_id;
            self.next_unit_id += 1;
            let mut unit = Unit::new(id, pos, team, kind);
            unit.training_upgrade = upgrade;
            unit.drawn_from = drawn
                .iter()
                .find(|&&(at, _)| at == pos)
                .map(|&(_, resource)| resource);
            unit.hp = unit.max_hp();
            unit.interior_hp = unit.max_hp();
            self.units.push(unit);
            log::info!("{team:?} city completed {kind:?}");
        }
    }

    /// Whether a finished unit can appear on `hex`: passable, with no unit on
    /// it and none already finishing there this turn (a barracks beside its
    /// city shares hexes with it).
    fn is_open_naval_spawn(
        &self,
        hex: Hex,
        team: Team,
        spawn: &[(Team, Hex, UnitType, Option<Resource>)],
    ) -> bool {
        self.grid.contains(hex)
            && matches!(self.grid.terrain(hex), Terrain::Coast | Terrain::Ocean)
            && !self.is_occupied(hex)
            && self.spawn_clear_of_enemy_civilians(hex, team)
            && spawn.iter().all(|&(_, pos, _, _)| pos != hex)
    }

    /// Land spawns avoid enemy centers and all field workers.
    fn is_open_spawn(
        &self,
        hex: Hex,
        team: Team,
        spawn: &[(Team, Hex, UnitType, Option<Resource>)],
    ) -> bool {
        self.grid.is_passable(hex)
            && !self.is_occupied(hex)
            && self.spawn_clear_of_enemy_civilians(hex, team)
            && !self.field_workers.iter().any(|worker| worker.pos == hex)
            && spawn.iter().all(|&(_, pos, _, _)| pos != hex)
    }
}

#[cfg(test)]
mod spawn_tests {
    use super::*;
    use crate::game::workers::FieldWorker;

    #[test]
    fn spawns_reject_enemy_city_centers_and_uncaptured_workers() {
        let mut game = GameState::city_scenario();
        let red = game
            .cities
            .iter()
            .position(|c| c.team == Team::Red)
            .unwrap();
        let blue = game
            .cities
            .iter()
            .position(|c| c.team == Team::Blue)
            .unwrap();
        let red_center = game.cities[red].pos;
        game.units.retain(|unit| unit.pos != red_center);
        assert!(!game.is_open_spawn(red_center, Team::Blue, &[]));

        let worker_pos = game.cities[blue]
            .pos
            .neighbors()
            .into_iter()
            .find(|&hex| game.grid.is_passable(hex) && !game.is_occupied(hex))
            .unwrap();
        game.field_workers.push(FieldWorker {
            id: 1000,
            team: Team::Red,
            home: red,
            base: red_center,
            pos: worker_pos,
            job: None,
            work_left: None,
            recalled: false,
        });
        assert!(!game.is_open_spawn(worker_pos, Team::Blue, &[]));
        assert!(!game.is_open_spawn(worker_pos, Team::Red, &[]));
    }
}
