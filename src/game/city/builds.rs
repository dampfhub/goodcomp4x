//! Builds: the city queue (units and buildings), building sites and
//! confirmation, the Barracks queue, and completing builds. What builds
//! cost and how the stockpile pays for them is in `economy.rs`.
use super::MAX_CITY_POPULATION;
use super::barracks::CITY_TRAINING_SLOWDOWN;
use super::economy::{Stock, WORK_PER_TURN, stock_icons, turns_icon};
use crate::game::hex::Hex;
use crate::game::terrain::{Resource, Terrain};
use crate::game::unit::{Team, Unit, UnitType};
use crate::game::{GameState, PLAYER_TEAM};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Building {
    Granary,
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
    pub const ALL: [Self; 15] = [
        Self::Granary,
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
    pub const PLACEABLE: [Self; 14] = [
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
    pub fn name(self) -> &'static str {
        match self {
            Self::Granary => "GRANARY",
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
            Self::Granary => (0, 8, 0),
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
            Self::Granary
            | Self::Barracks
            | Self::Mill
            | Self::CanoeHouse
            | Self::Watchpost
            | Self::WorkCamp => 3,
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
    /// Work it needs, in quarter turns.
    pub fn work(self) -> i32 {
        self.turns() * WORK_PER_TURN
    }
    pub fn shortcut(self) -> char {
        match self {
            Self::Granary => '4',
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
            Self::Granary => "+2 FOOD PER TURN.",
            Self::Barracks => {
                "ON OPEN LAND: TRAINS TROOPS TWICE AS FAST AS THE CITY. ON HORSES OR IRON, ALSO 3 CAVALRY OR ARMORED PER DEPOSIT."
            }
            Self::Mill => "ADJACENT WORKED TILES DELIVER ALL FOOD IF THEY CAN REACH THE CITY.",
            Self::Workshop => "ADJACENT PLACED BUILDINGS CAN BE CONFIRMED AT HALF PRODUCTION.",
            Self::CanoeHouse => "ON A RIVERBANK: ITS CONNECTED RIVER CARRIES GOODS LIKE A ROAD.",
            Self::Forge => "ON OR NEXT TO IRON, BESIDE BARRACKS: TRAINS TOUGHER ARMORED TROOPS.",
            Self::Stable => "ON OR NEXT TO HORSES, BESIDE BARRACKS: TRAINS FASTER CAVALRY.",
            Self::Watchpost => "SEES 4 HEXES, OR 5 FROM HILLS, THROUGH ORDINARY SIGHT LINES.",
            Self::FieldHospital => "HEALS TWO NEARBY FRIENDLY TROOPS EACH TURN, INSIDE AND OUT.",
            Self::Cannery => "COLLECTS FOOD FROM THREE REMOTE IMPROVEMENTS WITHIN 3 HEXES.",
            Self::WorkCamp => "CONNECTED WORKERS START AND END NEARBY JOBS HERE, NOT AT THE CITY.",
            Self::Smelter => "COLLECTS PRODUCTION FROM THREE REMOTE MINES WITHIN 3 HEXES.",
            Self::Railhead => "A CITY-ROAD LINK LETS TROOPS BY THE CITY MOVE HERE IN ONE TURN.",
            Self::Harbor => "ON A COASTAL LAND TILE: TRAINS SHIPS INTO ADJACENT WATER.",
            Self::CoastalBattery => "ON COASTAL LAND: FIRES AT HOSTILE SHIPS WITHIN 2 TILES.",
        }
    }

    pub fn is_placeable(self) -> bool {
        self != Self::Granary
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Build {
    Unit(BuildUnit),
    Building(Building),
    /// A worker for the city's pool (`workers.rs`).
    Worker,
    /// One more citizen, bought with food (`economy::grow_price`).
    Grow,
}

/// The Worker and Grow cards' keys.
pub(in crate::game) const WORKER_SHORTCUT: char = '8';
pub(in crate::game) const GROW_SHORTCUT: char = '9';

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unit(u) => u.name(),
            Self::Building(b) => b.name(),
            Self::Worker => "WORKER",
            Self::Grow => "GROW",
        }
    }
    /// Its price, except a Grow's, which depends on the city
    /// (`GameState::queue_price`).
    pub fn price(self) -> Stock {
        match self {
            Self::Unit(u) => u.price(),
            Self::Building(b) => b.price(),
            Self::Worker => Stock::whole(4, 2, 0),
            Self::Grow => Stock::default(),
        }
    }
    pub fn turns(self) -> i32 {
        match self {
            Self::Unit(u) => u.turns(),
            Self::Building(b) => b.turns(),
            Self::Worker | Self::Grow => 2,
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
            Self::Cavalry => "FAST FLANKER, NEEDS HORSES",
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
        if c.pending_building == Some(building) || c.queue.contains(&Build::Building(building)) {
            // Queued or finished without a site (the view was left before
            // one was chosen): the card resumes choosing it.
            if self.needs_site(city, building) {
                self.placing_building = Some((city, building));
                self.notice = format!("CHOOSE A {} SITE - CLICK A VALID TILE", building.name());
            } else {
                self.notice = format!("{} IS ALREADY QUEUED IN THIS CITY", building.name());
            }
            return;
        }
        if !self.queue_paid(city, Build::Building(building)) {
            return;
        }
        // Site choice is part of queuing every placeable building, even when
        // other work is ahead of it.
        if building.is_placeable() {
            self.placing_building = Some((city, building));
            self.notice = format!(
                "{} PAID - CLICK A VALID TILE TO CHOOSE ITS SITE",
                building.name()
            );
        }
    }

    /// The building whose site the player is choosing, while its city's view
    /// is open. Placement belongs to that view: with no city open, or another
    /// one, nothing follows the cursor and map clicks don't place it.
    pub(in crate::game) fn site_placement(&self) -> Option<(usize, Building)> {
        self.placing_building
            .filter(|&(city, _)| self.selected_city == Some(city))
    }

    /// Stops choosing a building site. A building queued without a site is
    /// taken back out of the queue, since it can't be built without one (a
    /// finished one waits for its site instead). Returns whether a site was
    /// being chosen.
    pub(in crate::game) fn abandon_site_placement(&mut self) -> bool {
        let Some((city, building)) = self.placing_building.take() else {
            return false;
        };
        let c = &mut self.cities[city];
        if c.placed_site(building).is_none()
            && !c.planned_sites.contains_key(&building)
            && c.pending_building != Some(building)
            && let Some(index) = c.queue.iter().position(|&b| b == Build::Building(building))
        {
            self.take_queue_item(city, index);
            self.notice = format!("{} CANCELLED - NO SITE CHOSEN - REFUNDED", building.name());
        }
        true
    }

    /// A placeable building queued in (or finished by) `city` that has no
    /// site yet, so it can't be confirmed until one is chosen.
    pub(in crate::game) fn needs_site(&self, city: usize, building: Building) -> bool {
        let c = &self.cities[city];
        building.is_placeable()
            && c.placed_site(building).is_none()
            && !c.planned_sites.contains_key(&building)
            && (c.pending_building == Some(building)
                || c.queue.contains(&Build::Building(building)))
    }

    /// The first reason a building cannot use this site. Keep this as the
    /// source of truth for previews, placement clicks, and final confirmation.
    pub(in crate::game) fn site_issue(
        &self,
        city: usize,
        building: Building,
        hex: Hex,
    ) -> Option<&'static str> {
        if !self.is_explored(hex) && !self.fog().sees(hex) && self.grid.is_passable(hex) {
            return Some("NEEDS AN EXPLORED TILE");
        }
        self.ai_site_issue(city, building, hex)
    }

    /// Like `site_issue`, but for the AI, which sees the whole map: the
    /// same rules without the player's exploration.
    pub(in crate::game) fn ai_site_issue(
        &self,
        city: usize,
        building: Building,
        hex: Hex,
    ) -> Option<&'static str> {
        if !self.grid.is_passable(hex) {
            return Some("NEEDS AN OPEN LAND TILE");
        }
        if self.cities.iter().enumerate().any(|(i, c)| {
            c.pos == hex
                || Building::PLACEABLE
                    .into_iter()
                    .any(|kind| c.placed_site(kind) == Some(hex))
                || c.planned_sites
                    .iter()
                    .any(|(&kind, &site)| site == hex && (i != city || kind != building))
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

    pub(in crate::game) fn site_available(
        &self,
        city: usize,
        building: Building,
        hex: Hex,
    ) -> bool {
        self.site_issue(city, building, hex).is_none()
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

    fn beside_workshop(&self, team: Team, site: Hex) -> bool {
        self.cities.iter().any(|c| {
            c.team == team
                && c.workshop
                    .is_some_and(|workshop| workshop.distance(site) == 1)
        })
    }

    /// Work `build` needs in `city`'s own queue: its own; half for a
    /// building sited beside one of the side's Workshops; and for a land
    /// troop `CITY_TRAINING_SLOWDOWN` times a Barracks' (`barracks.rs`).
    pub(in crate::game) fn city_build_work(&self, city: usize, build: Build) -> i32 {
        match build {
            Build::Unit(unit) if !unit.unit_type().is_naval() => {
                unit.work() * CITY_TRAINING_SLOWDOWN
            }
            Build::Building(building) if building.is_placeable() => {
                if self.cities[city]
                    .planned_sites
                    .get(&building)
                    .is_some_and(|&site| self.beside_workshop(self.cities[city].team, site))
                {
                    (building.work() + 1) / 2
                } else {
                    building.work()
                }
            }
            _ => build.work(),
        }
    }

    pub fn confirm_building(&mut self, building: Building) {
        if !building.is_placeable() {
            return;
        }
        let Some(city) = self.selected_city else {
            return;
        };
        let Some(&site) = self.cities[city].planned_sites.get(&building) else {
            self.notice = format!("CHOOSE A {} SITE ON THE MAP FIRST", building.name());
            return;
        };
        if let Some(reason) = self.site_issue(city, building, site) {
            self.notice = format!("{} SITE INVALID: {reason}", building.name());
            return;
        }
        if self.cities[city].queue.first() != Some(&Build::Building(building))
            || self.cities[city].progress < self.city_build_work(city, Build::Building(building))
        {
            self.notice = format!("{} IS STILL UNDER CONSTRUCTION", building.name());
            return;
        }
        self.finish_building(city, building, site);
        self.notice = format!("{} FINALIZED", building.name());
    }

    /// Puts the finished head of `city`'s queue, `building`, on `site`.
    fn finish_building(&mut self, city: usize, building: Building, site: Hex) {
        let c = &mut self.cities[city];
        c.pending_building = None;
        c.queue.remove(0);
        c.progress = 0;
        c.set_placed_site(building, site);
        if building == Building::CoastalBattery {
            c.coastal_battery_hp = 150.0;
        }
        c.planned_sites.remove(&building);
        c.built.push(building);
    }

    /// The AI's buildings go up on their planned site as soon as they're
    /// done, where the player confirms theirs: a finished one waiting at
    /// the head of `city`'s queue is placed now (for the player's side when
    /// the AI plans it, as the simulations do).
    pub(in crate::game) fn confirm_ai_building(&mut self, city: usize) {
        if let Some(building) = self.cities[city].pending_building {
            self.place_ai_building(city, building);
        }
    }

    /// Places `building`, finished at the head of `city`'s queue, on its
    /// planned site, or drops it (refunded) if that site went bad.
    fn place_ai_building(&mut self, city: usize, building: Building) {
        match self.cities[city].planned_sites.get(&building).copied() {
            Some(site) if self.ai_site_issue(city, building, site).is_none() => {
                self.finish_building(city, building, site);
            }
            _ => {
                self.take_queue_item(city, 0);
                self.cities[city].planned_sites.remove(&building);
            }
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

    /// Reopens placement for a queued or completed building before confirmation.
    pub fn change_selected_building_site(&mut self, building: Building) {
        let Some(city) = self.selected_city else {
            return;
        };
        if self.cities[city].team != PLAYER_TEAM
            || !building.is_placeable()
            || self.cities[city].placed_site(building).is_some()
            || !self.cities[city].planned_sites.contains_key(&building)
        {
            return;
        }
        self.placing_building = Some((city, building));
        self.notice = format!("CHANGE {} SITE - CLICK A NEW VALID TILE", building.name());
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
            if self.cities[city].pending_building.is_some() {
                return;
            }
            self.cities[city].progress = self.city_build_work(city, build);
            self.complete_builds_for(Some((city, false)));
            self.notice = if let Build::Building(building) = build
                && self.cities[city].pending_building == Some(building)
            {
                format!("DEBUG: {} READY - CONFIRM ITS SITE", building.name())
            } else if self.cities[city].queue.first() == Some(&build) {
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
        let pending = self.cities[city].pending_building.is_some();
        let queue = &mut self.cities[city].queue;
        let other = if up {
            index.checked_sub(1)
        } else {
            index.checked_add(1)
        };
        if let Some(other) = other.filter(|&other| other < queue.len()) {
            if pending && (index == 0 || other == 0) {
                self.notice = "CONFIRM OR REMOVE THE READY BUILDING FIRST".into();
                return;
            }
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
        if let Build::Building(building) = removed
            && building.is_placeable()
        {
            if self.placing_building == Some((city, building)) {
                self.placing_building = None;
            }
            self.cities[city].planned_sites.remove(&building);
        }
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
            if self.cities[i].pending_building.is_some()
                || self.cities[i].progress < self.city_build_work(i, build)
            {
                continue;
            }
            if let Build::Building(building) = build {
                match building {
                    Building::Granary => {
                        self.cities[i].progress = 0;
                        self.cities[i].queue.remove(0);
                        self.cities[i].built.push(building);
                        self.notice = "GRANARY COMPLETE - +2 FOOD PER TURN".into();
                    }
                    // The AI puts a building on its planned site at once.
                    _ if self.cities[i].team != PLAYER_TEAM => self.place_ai_building(i, building),
                    _ => {
                        self.cities[i].pending_building = Some(building);
                        // Placement starts here only in the open city (F9);
                        // a turn's completion leaves it for when the city is
                        // next opened (`open_city`).
                        if self.selected_city == Some(i) && self.needs_site(i, building) {
                            self.placing_building = Some((i, building));
                        }
                        self.notice =
                            format!("{} COMPLETE - CHOOSE A SITE, THEN CONFIRM", building.name());
                    }
                }
                continue;
            }
            if build == Build::Worker {
                self.cities[i].progress -= build.work();
                self.cities[i].queue.remove(0);
                self.cities[i].workers += 1;
                log::info!("{:?} city completed a worker", self.cities[i].team);
                if self.cities[i].team == PLAYER_TEAM {
                    self.notice = "WORKER READY - PRESS W TO GIVE IT A JOB".into();
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
                    self.is_open_naval_spawn(h, &spawn)
                } else {
                    self.is_open_spawn(h, &spawn)
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
                .find(|&h| self.is_open_spawn(h, &spawn))
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
        spawn: &[(Team, Hex, UnitType, Option<Resource>)],
    ) -> bool {
        self.grid.contains(hex)
            && matches!(self.grid.terrain(hex), Terrain::Coast | Terrain::Ocean)
            && !self.is_occupied(hex)
            && spawn.iter().all(|&(_, pos, _, _)| pos != hex)
    }

    /// Whether a finished land unit can appear on `hex`: as a ship's, but on
    /// open land, and not on a worker out on the map (it would share the
    /// hex with an enemy's without capturing it).
    fn is_open_spawn(&self, hex: Hex, spawn: &[(Team, Hex, UnitType, Option<Resource>)]) -> bool {
        self.grid.is_passable(hex)
            && !self.is_occupied(hex)
            && !self.field_workers.iter().any(|worker| worker.pos == hex)
            && spawn.iter().all(|&(_, pos, _, _)| pos != hex)
    }
}
