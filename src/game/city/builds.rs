//! Builds: the city queue (units and buildings), building sites and
//! confirmation, the Barracks queue, and completing builds. What builds
//! cost and how the stockpile pays for them is in `economy.rs`.
use super::MAX_CITY_POPULATION;
use super::barracks::CITY_TRAINING_SLOWDOWN;
use super::economy::{Lane, Stock, WORK_PER_TURN, stock_icons, turns_icon};
use super::founding::MIN_CITY_DISTANCE;
use super::supply::build_supply;
use crate::game::GameState;
use crate::game::JobKind;
use crate::game::hex::Hex;
use crate::game::terrain::{Resource, Terrain};
use crate::game::unit::{Team, Unit, UnitType};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
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
    /// The key that places it from an open city, if it has one (`app.rs`).
    pub fn shortcut(self) -> Option<char> {
        match self {
            Self::Barracks => Some('5'),
            Self::Mill => Some('6'),
            Self::Workshop => Some('7'),
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
            | Self::CoastalBattery => None,
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

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Build {
    Unit(BuildUnit),
    /// A worker for the city's pool (`workers.rs`).
    Worker,
    /// One more citizen, bought with food (`economy::grow_price`).
    Grow,
    /// The city spends a turn gathering: free, and `GATHER_YIELD` goes to
    /// the stockpile when it's done. Something a city can always do.
    Gather,
    /// A settler, to found a city (`founding.rs`): dear and slow, and it
    /// takes one of the city's citizens when it's done. Only a city of at
    /// least `SETTLER_MIN_POPULATION` works on it or finishes it; below
    /// that it waits in the queue, keeping its work.
    Settler,
    /// A scout: cheap and quick, and a city queues one at a time.
    Scout,
}

/// The Worker, Grow and Gather cards' keys.
pub(in crate::game) const WORKER_SHORTCUT: char = '8';
pub(in crate::game) const GROW_SHORTCUT: char = '9';
pub(in crate::game) const GATHER_SHORTCUT: char = '0';
/// The Scout and Settler cards' keys.
pub(in crate::game) const SCOUT_SHORTCUT: char = '4';
pub(in crate::game) const SETTLER_SHORTCUT: char = 'S';
/// The citizens a city needs to work on a Settler, which takes one of them
/// when it's done: a city never settles itself below two.
pub(in crate::game) const SETTLER_MIN_POPULATION: usize = 3;
/// What a turn of gathering (`Build::Gather`) brings in, in quarters: 1
/// food, 1 wood and half a metal. Tempo tuning (#239): option B halved it
/// from 2 food, 2 wood, 1 metal. The half metal is kept (Round 6 measured
/// none): a city with no metal tile gets metal only from gathering, and
/// without it the Cities scenario could never afford a Cavalry
/// (`docs/rts-economy.md`, Round 7).
pub(in crate::game) const GATHER_YIELD: Stock = Stock {
    food: 4,
    wood: 4,
    metal: 2,
};

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unit(u) => u.name(),
            Self::Worker => "WORKER",
            Self::Grow => "GROW",
            Self::Gather => "GATHER",
            Self::Settler => "SETTLER",
            Self::Scout => "SCOUT",
        }
    }
    /// Its price, except a Grow's, which depends on the city
    /// (`GameState::queue_price`). A Settler also takes a citizen when it's
    /// done (`complete_builds`).
    pub fn price(self) -> Stock {
        match self {
            Self::Unit(u) => u.price(),
            Self::Worker => Stock::whole(4, 2, 0),
            Self::Settler => Stock::whole(30, 10, 0),
            Self::Scout => Stock::whole(2, 4, 0),
            Self::Grow | Self::Gather => Stock::default(),
        }
    }
    pub fn turns(self) -> i32 {
        match self {
            Self::Unit(u) => u.turns(),
            Self::Worker | Self::Grow | Self::Scout => 2,
            Self::Gather => 1,
            Self::Settler => 6,
        }
    }
    /// Work it needs, in quarter turns.
    pub fn work(self) -> i32 {
        self.turns() * WORK_PER_TURN
    }
    /// The unit it turns out, if it's one: a troop or ship, a settler (a
    /// civilian with the Melee body) or a scout.
    pub(in crate::game) fn unit_type(self) -> Option<UnitType> {
        match self {
            Self::Unit(unit) => Some(unit.unit_type()),
            Self::Settler => Some(UnitType::Melee),
            Self::Scout => Some(UnitType::Scout),
            Self::Worker | Self::Grow | Self::Gather => None,
        }
    }
}

/// Turns a Melee or Ranged takes at a Barracks (twice that in a city
/// center). Tempo tuning (#239): option B raised it from 2
/// (`docs/rts-economy.md`, Round 7).
pub(in crate::game) const LIGHT_TROOP_TURNS: i32 = 3;
/// Turns a Cavalry, Siege or Armored takes at a Barracks (twice that in a
/// city center). Tempo tuning (#239): option B raised it from 3; the
/// Patrol Galley, which shared it, keeps 3 (`docs/rts-economy.md`, Round 7).
pub(in crate::game) const HEAVY_TROOP_TURNS: i32 = 4;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum BuildUnit {
    Melee,
    Pikeman,
    Ranged,
    Cavalry,
    Siege,
    Armored,
    PatrolGalley,
    LandingCraft,
    BombardShip,
}

impl BuildUnit {
    /// Every troop and ship a queue trains.
    pub const ALL: [Self; 9] = [
        Self::Melee,
        Self::Pikeman,
        Self::Ranged,
        Self::Cavalry,
        Self::Siege,
        Self::Armored,
        Self::PatrolGalley,
        Self::LandingCraft,
        Self::BombardShip,
    ];

    pub(in crate::game) fn unit_type(self) -> UnitType {
        match self {
            Self::Melee => UnitType::Melee,
            Self::Pikeman => UnitType::Pikeman,
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
            Self::Pikeman => "PIKEMAN",
            Self::Ranged => "RANGED",
            Self::Cavalry => "CAVALRY",
            Self::Siege => "SIEGE",
            Self::Armored => "ARMORED",
            Self::PatrolGalley => "PATROL GALLEY",
            Self::LandingCraft => "LANDING CRAFT",
            Self::BombardShip => "BOMBARD SHIP",
        }
    }
    /// What queuing it takes from the side's stockpile (`economy.rs`): at
    /// most two kinds of goods, one of them metal (#374), so metal is what
    /// most often limits an army. Foot and horse troops pay food and metal;
    /// bows, engines and ships, built of timber, wood and metal. The amounts
    /// keep the army's tempo near what it was (`docs/rts-economy.md`,
    /// Round 11). Before, Melee was 3/9/0, Ranged 3/11/0, Cavalry 5/6/5,
    /// Siege 2/12/6, Armored 5/3/11, and each ship 1 food more. The Pikeman
    /// (#375) is the one troop paid in food alone: cheap, and weak but for
    /// its bonus against mounted troops.
    pub fn price(self) -> Stock {
        let (food, wood, metal) = match self {
            Self::Melee => (5, 0, 2),
            Self::Pikeman => (6, 0, 0),
            Self::Ranged => (0, 9, 2),
            Self::Cavalry => (6, 0, 5),
            Self::Siege => (0, 12, 6),
            Self::Armored => (6, 0, 10),
            Self::PatrolGalley => (0, 10, 2),
            Self::LandingCraft => (0, 12, 2),
            Self::BombardShip => (0, 12, 6),
        };
        Stock::whole(food, wood, metal)
    }
    /// Turns it takes at the head of the queue; a city center takes
    /// `CITY_TRAINING_SLOWDOWN` times as long for a land troop.
    pub fn turns(self) -> i32 {
        match self {
            Self::Melee | Self::Pikeman | Self::Ranged => LIGHT_TROOP_TURNS,
            Self::Cavalry | Self::Siege | Self::Armored => HEAVY_TROOP_TURNS,
            Self::PatrolGalley => 3,
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
            Self::Pikeman => "CHEAP PIKES: STRONG AGAINST MOUNTED UNITS, WEAK OTHERWISE",
            Self::Ranged => "FIRES FROM 2 TILES",
            Self::Cavalry => "FAST FLANKER",
            Self::Siege => "LONG RANGE, SLOW",
            Self::Armored => "HEAVY IRON INFANTRY",
            Self::PatrolGalley => "FAST COASTAL FIGHTER; STRONG AGAINST SHIPS",
            Self::LandingCraft => "CARRIES UP TO FOUR LAND TROOPS",
            Self::BombardShip => "RANGE 3 SHORE AND SHIP BOMBARDMENT",
        }
    }
    /// The key that queues it in an open city, if it has one (`app.rs`).
    pub fn shortcut(self) -> Option<char> {
        match self {
            Self::Melee => Some('1'),
            Self::Ranged => Some('2'),
            Self::Siege => Some('3'),
            Self::Pikeman
            | Self::Cavalry
            | Self::Armored
            | Self::PatrolGalley
            | Self::LandingCraft
            | Self::BombardShip => None,
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

/// What taking one item off a queue gave back, for its notice.
fn refund_word(paid: bool) -> &'static str {
    if paid {
        "REFUNDED"
    } else {
        "UNPAID, NOTHING TO REFUND"
    }
}

/// What clearing a queue of `count` items, `paid` of them paid for, gave
/// back, for its notice.
fn cleared_words(count: usize, paid: usize) -> String {
    match paid {
        0 => format!("{count} UNPAID, NOTHING TO REFUND"),
        _ if paid == count => format!("{count} REFUNDED"),
        _ => format!("{paid} PAID OF {count} REFUNDED"),
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
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != self.local_team {
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
        if let Some(why) = self.city_build_issue(city, Build::Unit(build)) {
            self.notice = format!("{}: {why}", build.name());
            return;
        }
        self.queue_in_city(city, Build::Unit(build));
    }

    /// Queues `build` in the player's `city`, unpaid (`queue_build`), and
    /// says what it costs, or what it waits for.
    fn queue_in_city(&mut self, city: usize, build: Build) {
        let price = self.queue_price(city, build);
        self.queue_build(city, build);
        let index = self.cities[city].queue.len() - 1;
        let turns = self.city_build_turns(city, build);
        self.notice = self.queued_notice(city, Lane::City, index, price, turns);
    }

    /// The notice for item `index`, just queued in one of `city`'s queues
    /// at `price`: what it costs and takes, paid when work starts, or what
    /// the stockpile is short of if it waits (`waiting_items`).
    fn queued_notice(
        &self,
        city: usize,
        lane: Lane,
        index: usize,
        price: Stock,
        turns: i32,
    ) -> String {
        let name = match lane {
            Lane::City => self.cities[city].queue[index].build.name(),
            Lane::Barracks => self.cities[city].barracks_queue[index].build.name(),
        };
        let forecast = self.forecast(self.cities[city].team);
        let waits = forecast.lane(city, lane).and_then(|lane| {
            self.waiting_items(lane)
                .into_iter()
                .find(|&(waiting, _)| waiting == index)
        });
        if let Some((_, short)) = waits {
            return format!("QUEUED {name} - WAITS FOR {}", stock_icons(short));
        }
        let cost = [stock_icons(price), turns_icon(turns)]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        format!("QUEUED {name} - {cost}")
    }

    /// 9 or the Grow card: one more citizen, bought with food.
    pub fn queue_selected_city_growth(&mut self) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != self.local_team {
            return;
        }
        if !self.can_grow(city) {
            self.notice = format!("A CITY HOLDS AT MOST {MAX_CITY_POPULATION} CITIZENS");
            return;
        }
        self.queue_in_city(city, Build::Grow);
    }

    /// Whether another Grow fits: the city's population plus the Grows
    /// already queued stays under the cap.
    pub(in crate::game) fn can_grow(&self, city: usize) -> bool {
        let c = &self.cities[city];
        let queued = c.queue.iter().filter(|q| q.build == Build::Grow).count();
        c.population + queued < MAX_CITY_POPULATION
    }

    /// 8 or the Worker button: a worker for the open city's pool.
    pub fn queue_selected_city_worker(&mut self) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != self.local_team {
            return;
        }
        self.queue_in_city(city, Build::Worker);
    }

    /// 4 or the Scout card: a scout from the open city's own queue.
    pub fn queue_selected_city_scout(&mut self) {
        self.queue_selected_city_civilian(Build::Scout);
    }

    /// S or the Settler card: a settler from the open city's own queue.
    pub fn queue_selected_city_settler(&mut self) {
        self.queue_selected_city_civilian(Build::Settler);
    }

    /// A scout or a settler from the open city's own queue, if it may queue
    /// one (`city_build_issue`).
    fn queue_selected_city_civilian(&mut self, build: Build) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != self.local_team {
            return;
        }
        if let Some(why) = self.city_build_issue(city, build) {
            self.notice = format!("{}: {why}", build.name());
            return;
        }
        self.queue_in_city(city, build);
    }

    /// Why `city` can't queue `build` now: a Settler needs
    /// `SETTLER_MIN_POPULATION` citizens, a city queues one Scout at a time,
    /// and a troop, ship or Scout needs room in its side's supply
    /// (`supply_lock`).
    pub(in crate::game) fn city_build_issue(&self, city: usize, build: Build) -> Option<String> {
        let c = &self.cities[city];
        match build {
            Build::Settler if c.population < SETTLER_MIN_POPULATION => {
                Some(format!("NEEDS POPULATION {SETTLER_MIN_POPULATION}"))
            }
            Build::Scout if c.queue.iter().any(|q| q.build == Build::Scout) => {
                Some("ONE SCOUT AT A TIME".into())
            }
            _ => self.supply_lock(c.team, build_supply(build)),
        }
    }

    /// Whether item `index` of `city`'s own queue waits for citizens: a
    /// Settler, in a city below `SETTLER_MIN_POPULATION`. The queue works
    /// the next item meanwhile, and the Settler keeps its work.
    pub(in crate::game) fn waits_for_citizens(&self, city: usize, index: usize) -> bool {
        let c = &self.cities[city];
        c.queue[index].build == Build::Settler && c.population < SETTLER_MIN_POPULATION
    }

    pub fn queue_selected_city_building(&mut self, building: Building) {
        if self.is_playing_out() {
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
        if c.team != self.local_team {
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
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILD".into();
            return;
        };
        if self.cities[city].team != self.local_team {
            return;
        }
        self.queue_in_city(city, Build::Gather);
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
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        if self.cities[city].team != self.local_team || self.cities[city].barracks.is_none() {
            return;
        }
        if let Some(reason) = self.barracks_lock(city, build) {
            self.notice = format!("{}: {reason}", build.name());
            return;
        }
        self.queue_barracks(city, build);
        let index = self.cities[city].barracks_queue.len() - 1;
        self.notice = self.queued_notice(city, Lane::Barracks, index, build.price(), build.turns());
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

    /// Completes only the active queue in the currently open structure: the
    /// item it works now (`pick_item`), paid for if it wasn't.
    pub fn debug_complete_current_production(&mut self) {
        if self.refuses_debug() {
            return;
        }
        if self.is_resolving() {
            return;
        }
        let (city, lane) = match (self.selected_barracks, self.selected_city) {
            (Some(city), _) => (city, Lane::Barracks),
            (None, Some(city)) => (city, Lane::City),
            (None, None) => return,
        };
        let team = self.cities[city].team;
        let room = self.supply_room(team);
        let Some((index, price)) = self.pick_item(city, lane, self.stock(team), room) else {
            if self.lane_len(city, lane) > 0 {
                self.notice = "DEBUG: NOTHING QUEUED THE STOCKPILE AND SUPPLY ALLOW".into();
            }
            return;
        };
        let name = match lane {
            Lane::City => self.cities[city].queue[index].build.name(),
            Lane::Barracks => self.cities[city].barracks_queue[index].build.name(),
        };
        let needed = self.lane_item(city, lane, index).2;
        self.work_item(city, lane, index, price, needed);
        let before = self.lane_len(city, lane);
        self.complete_builds_for(Some((city, lane == Lane::Barracks)));
        self.notice = match lane {
            _ if self.lane_len(city, lane) == before => {
                "DEBUG: PRODUCTION READY - NO OPEN SPAWN TILE".into()
            }
            Lane::City => format!("DEBUG: {name} PRODUCTION COMPLETED"),
            Lane::Barracks => format!("DEBUG: {name} TRAINING COMPLETED"),
        };
    }

    pub fn move_selected_city_queue_item(&mut self, index: usize, up: bool) {
        if self.is_playing_out() {
            return;
        }
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

    /// A city queue row's X, or Backspace for the head: takes it off,
    /// refunded if it was paid for. Not while a turn plays out, like
    /// queueing.
    pub fn remove_selected_city_queue_item(&mut self, index: usize) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            return;
        };
        let Some(paid) = self.cities[city].queue.get(index).map(|q| q.paid) else {
            return;
        };
        let removed = self.take_queue_item(city, index);
        self.notice = format!(
            "REMOVED {} FROM CITY QUEUE - {}",
            removed.name(),
            refund_word(paid)
        );
    }

    /// The city queue's Clear button: takes every item off, each through
    /// `take_queue_item` as its X would, so the stockpile ends as it would
    /// after removing them one by one (Grows included: what one refunds
    /// depends only on how many are paid for, not on the order).
    pub fn clear_selected_city_queue(&mut self) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_city else {
            return;
        };
        let count = self.cities[city].queue.len();
        if count == 0 {
            return;
        }
        let paid = self.cities[city].queue.iter().filter(|q| q.paid).count();
        while let Some(last) = self.cities[city].queue.len().checked_sub(1) {
            self.take_queue_item(city, last);
        }
        self.city_queue_scroll = 0;
        self.notice = format!("CLEARED THE CITY QUEUE - {}", cleared_words(count, paid));
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

    /// A Barracks queue row's X: takes it off, refunded if it was paid for.
    /// Not while a turn plays out.
    pub fn remove_selected_barracks_queue_item(&mut self, index: usize) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        let Some(paid) = self.cities[city].barracks_queue.get(index).map(|q| q.paid) else {
            return;
        };
        let removed = self.take_barracks_item(city, index);
        self.notice = format!(
            "REMOVED {} FROM BARRACKS QUEUE - {}",
            removed.name(),
            refund_word(paid)
        );
    }

    /// The Barracks queue's Clear button: takes every troop off, each
    /// through `take_barracks_item` as its X would.
    pub fn clear_selected_barracks_queue(&mut self) {
        if self.is_playing_out() {
            return;
        }
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        let count = self.cities[city].barracks_queue.len();
        if count == 0 {
            return;
        }
        let paid = self.cities[city]
            .barracks_queue
            .iter()
            .filter(|q| q.paid)
            .count();
        while let Some(last) = self.cities[city].barracks_queue.len().checked_sub(1) {
            self.take_barracks_item(city, last);
        }
        self.barracks_queue_scroll = 0;
        self.notice = format!(
            "CLEARED THE BARRACKS QUEUE - {}",
            cleared_words(count, paid)
        );
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
        // Where each settler appears.
        let mut settlers = Vec::new();
        for i in 0..self.cities.len() {
            if only.is_some_and(|lane| lane != (i, false)) {
                continue;
            }
            // The first finished item: the one worked this turn, or one
            // finished before and held for want of an open hex (or, a
            // Settler, of citizens).
            let Some(index) = (0..self.cities[i].queue.len()).find(|&index| {
                let (paid, progress, work) = self.lane_item(i, Lane::City, index);
                paid && progress >= work && !self.waits_for_citizens(i, index)
            }) else {
                continue;
            };
            let build = self.cities[i].queue[index].build;
            if build == Build::Gather {
                let c = &mut self.cities[i];
                c.queue.remove(index);
                let (team, id) = (c.team, c.id);
                *self.stock_mut(team) += GATHER_YIELD;
                if team == self.local_team {
                    self.notice = format!("CITY {} GATHERED {}", id + 1, stock_icons(GATHER_YIELD));
                }
                continue;
            }
            if build == Build::Worker {
                self.cities[i].queue.remove(index);
                self.cities[i].workers += 1;
                log::info!("{:?} city completed a worker", self.cities[i].team);
                if self.cities[i].team == self.local_team {
                    self.notice =
                        "WORKER READY - PLACE ROADS, IMPROVEMENTS AND BUILDINGS FROM THE CITY"
                            .into();
                }
                continue;
            }
            if build == Build::Grow {
                let c = &mut self.cities[i];
                c.queue.remove(index);
                // Starving can't take it over the cap, but a Grow bought
                // before the cap was reached could.
                c.population = (c.population + 1).min(MAX_CITY_POPULATION);
                log::info!("{:?} city {} grew to {}", c.team, c.id + 1, c.population);
                if c.team == self.local_team {
                    self.notice = format!("CITY {} GREW TO {}", c.id + 1, c.population);
                }
                continue;
            }
            // A finished unit waits, done, until a hex opens for it. Work
            // never passes what an item needs, so none is banked for the
            // rest of the queue: a bank would let the rest come out one unit
            // a turn once one did (#54).
            let city = self.cities[i].pos;
            let naval = matches!(build, Build::Unit(unit) if unit.unit_type().is_naval());
            let origin = if naval {
                if !self.city_is_coastal(i) {
                    continue;
                }
                let Some(harbor) = self.cities[i].placed_site(Building::Harbor) else {
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
                continue;
            };
            self.cities[i].queue.remove(index);
            let kind = build.unit_type().expect("every other build is done above");
            spawn.push((self.cities[i].team, pos, kind, None));
            if build == Build::Settler {
                // The settler takes one of the city's citizens with it.
                settlers.push(pos);
                self.remove_citizen(i);
                let c = &self.cities[i];
                log::info!("{:?} city {} completed a settler", c.team, c.id + 1);
                if c.team == self.local_team {
                    self.notice = format!(
                        "CITY {} TRAINED A SETTLER - F FOUNDS A CITY {MIN_CITY_DISTANCE} HEXES FROM ANY OTHER",
                        c.id + 1
                    );
                }
            }
        }
        for i in 0..self.cities.len() {
            if only.is_some_and(|lane| lane != (i, true)) {
                continue;
            }
            let Some(index) = (0..self.cities[i].barracks_queue.len()).find(|&index| {
                let (paid, progress, work) = self.lane_item(i, Lane::Barracks, index);
                paid && progress >= work
            }) else {
                continue;
            };
            let build = self.cities[i].barracks_queue[index].build;
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
            self.cities[i].barracks_queue.remove(index);
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
            if settlers.contains(&pos) {
                self.settlers.insert(id);
            }
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
