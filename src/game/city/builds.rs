//! Builds: the city queue (units and buildings), building sites and
//! confirmation, the Barracks queue, and completing builds.
use super::amount;
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
    pub fn cost(self) -> i32 {
        match self {
            Self::Granary => 48,
            Self::Barracks => 64,
            Self::Mill => 60,
            Self::Workshop => 80,
            Self::CanoeHouse => 64,
            Self::Forge | Self::Stable => 80,
            Self::Watchpost => 64,
            Self::FieldHospital | Self::Cannery => 96,
            Self::WorkCamp => 72,
            Self::Smelter => 96,
            Self::Railhead => 120,
            Self::Harbor => 80,
            Self::CoastalBattery => 96,
        }
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
                "PLACED ON ANY OPEN LAND TILE. WITH THE MANAGER THERE, ITS WORK GROUP TRAINS TROOPS."
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
}

/// What a worker costs, and its key.
pub(in crate::game) const WORKER_COST: i32 = 32;
pub(in crate::game) const WORKER_SHORTCUT: char = '8';

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unit(u) => u.name(),
            Self::Building(b) => b.name(),
            Self::Worker => "WORKER",
        }
    }
    pub fn cost(self) -> i32 {
        match self {
            Self::Unit(u) => u.cost(),
            Self::Building(b) => b.cost(),
            Self::Worker => WORKER_COST,
        }
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
    fn unit_type(self) -> UnitType {
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
    pub fn cost(self) -> i32 {
        match self {
            Self::Melee => 48,
            Self::Ranged => 56,
            Self::Cavalry => 64,
            Self::Siege => 72,
            Self::Armored => 80,
            Self::PatrolGalley => 72,
            Self::LandingCraft => 88,
            Self::BombardShip => 104,
        }
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
        // A city only retains production while it has an active build.
        if self.cities[city].queue.is_empty() {
            self.cities[city].production = 0;
        }
        self.cities[city].queue.push(Build::Unit(build));
        self.notice = format!(
            "BUILDING {} - COST {} PRODUCTION",
            build.name(),
            amount(build.cost())
        );
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
        if self.cities[city].queue.is_empty() {
            self.cities[city].production = 0;
        }
        self.cities[city].queue.push(Build::Worker);
        self.notice = format!(
            "BUILDING A WORKER - COST {} PRODUCTION",
            amount(WORKER_COST)
        );
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
        let c = &mut self.cities[city];
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
        if c.queue.is_empty() {
            c.production = 0;
        }
        c.queue.push(Build::Building(building));
        // Site choice is part of queuing every placeable building, even when
        // other work is ahead of it.
        if building.is_placeable() {
            self.placing_building = Some((city, building));
            self.notice = format!(
                "{} STARTED - CLICK A VALID TILE TO CHOOSE ITS SITE",
                building.name()
            );
            return;
        }
        self.notice = format!(
            "BUILDING {} - COST {} PRODUCTION",
            building.name(),
            amount(building.cost())
        );
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
            c.queue.remove(index);
            if index == 0 {
                c.production = 0;
            }
            self.notice = format!("{} CANCELLED - NO SITE CHOSEN", building.name());
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
        if !self.grid.is_passable(hex) {
            return Some("NEEDS AN OPEN LAND TILE");
        }
        if !self.is_explored(hex) && !self.fog().sees(hex) {
            return Some("NEEDS AN EXPLORED TILE");
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

    fn workshop_discount(&self, team: Team, site: Hex) -> bool {
        self.cities.iter().any(|c| {
            c.team == team
                && c.workshop
                    .is_some_and(|workshop| workshop.distance(site) == 1)
        })
    }

    pub(in crate::game) fn city_build_cost(&self, city: usize, build: Build) -> i32 {
        match build {
            Build::Building(building) if building.is_placeable() => {
                if self.cities[city]
                    .planned_sites
                    .get(&building)
                    .is_some_and(|&site| self.workshop_discount(self.cities[city].team, site))
                {
                    (building.cost() + 1) / 2
                } else {
                    building.cost()
                }
            }
            _ => build.cost(),
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
            || self.cities[city].production < self.city_build_cost(city, Build::Building(building))
        {
            self.notice = format!("{} IS STILL UNDER CONSTRUCTION", building.name());
            return;
        }
        self.cities[city].pending_building = None;
        self.cities[city].queue.remove(0);
        self.cities[city].production = 0;
        self.cities[city].set_placed_site(building, site);
        if building == Building::CoastalBattery {
            self.cities[city].coastal_battery_hp = 150.0;
        }
        self.cities[city].planned_sites.remove(&building);
        self.cities[city].built.push(building);
        self.notice = format!("{} FINALIZED", building.name());
    }

    pub fn queue_selected_barracks_unit(&mut self, build: BuildUnit) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        if self.cities[city].team != PLAYER_TEAM || self.cities[city].barracks.is_none() {
            return;
        }
        if let Some(resource) = build.required_resource()
            && !self.barracks_can_train(city, build)
        {
            self.notice = format!(
                "{} NEEDS {} AT BARRACKS OR AN ADJACENT SUPPORT BUILDING",
                build.name(),
                resource.name()
            );
            return;
        }
        let c = &mut self.cities[city];
        if c.barracks_queue.is_empty() {
            c.barracks_production = 0;
        }
        c.barracks_queue.push(build);
        self.notice = format!(
            "BARRACKS TRAINING {} - NEEDS MANAGER ON BARRACKS",
            build.name()
        );
    }

    pub(in crate::game) fn barracks_can_train(&self, city: usize, build: BuildUnit) -> bool {
        let Some(tile) = self.cities[city].barracks else {
            return false;
        };
        build.required_resource().is_none_or(|resource| {
            self.grid.resource(tile) == Some(resource) || self.barracks_support(city, resource)
        })
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
            self.cities[city].barracks_production = build.cost();
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
            self.cities[city].production = self.city_build_cost(city, build);
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
        let removed = self.cities[city].queue.remove(index);
        if index == 0 {
            self.cities[city].production = 0;
            self.cities[city].pending_building = None;
        }
        if let Build::Building(building) = removed
            && building.is_placeable()
        {
            if self.placing_building == Some((city, building)) {
                self.placing_building = None;
            }
            self.cities[city].planned_sites.remove(&building);
        }
        self.notice = format!("REMOVED {} FROM CITY QUEUE", removed.name());
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
        let removed = self.cities[city].barracks_queue.remove(index);
        if index == 0 {
            self.cities[city].barracks_production = 0;
        }
        self.notice = format!("REMOVED {} FROM BARRACKS QUEUE", removed.name());
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
        for i in 0..self.cities.len() {
            if only.is_some_and(|lane| lane != (i, false)) {
                continue;
            }
            if self.cities[i].queue.is_empty() && self.cities[i].team == Team::Red {
                self.cities[i].queue.push(Build::Unit(BuildUnit::Melee));
            }
            let Some(build) = self.cities[i].queue.first().copied() else {
                continue;
            };
            if self.cities[i].pending_building.is_some()
                || self.cities[i].production < self.city_build_cost(i, build)
            {
                continue;
            }
            if let Build::Building(building) = build {
                match building {
                    Building::Granary => {
                        self.cities[i].production = 0;
                        self.cities[i].queue.remove(0);
                        self.cities[i].built.push(building);
                        self.notice = "GRANARY COMPLETE - +2 FOOD PER TURN".into();
                    }
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
                self.cities[i].production -= build.cost();
                self.cities[i].queue.remove(0);
                self.cities[i].workers += 1;
                log::info!("{:?} city completed a worker", self.cities[i].team);
                if self.cities[i].team == PLAYER_TEAM {
                    self.notice = "WORKER READY - CLICK A TILE TO GIVE IT A JOB".into();
                }
                continue;
            }
            let city = self.cities[i].pos;
            let naval = matches!(build, Build::Unit(unit) if unit.unit_type().is_naval());
            let origin = if naval {
                if !self.city_is_coastal(i) {
                    self.cities[i].production = build.cost();
                    continue;
                }
                let Some(harbor) = self.cities[i].placed_site(Building::Harbor) else {
                    self.cities[i].production = build.cost();
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
                self.cities[i].production = build.cost();
                continue;
            };
            self.cities[i].production -= build.cost();
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
            if self.cities[i].barracks_production < build.cost() {
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
            self.cities[i].barracks_production = 0;
            self.cities[i].barracks_queue.remove(0);
            let upgrade = build
                .required_resource()
                .filter(|&resource| self.barracks_support(i, resource));
            spawn.push((self.cities[i].team, pos, build.unit_type(), upgrade));
        }
        for (team, pos, kind, upgrade) in spawn {
            let id = self.next_unit_id;
            self.next_unit_id += 1;
            let mut unit = Unit::new(id, pos, team, kind);
            unit.training_upgrade = upgrade;
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

    fn is_open_spawn(&self, hex: Hex, spawn: &[(Team, Hex, UnitType, Option<Resource>)]) -> bool {
        self.grid.is_passable(hex)
            && !self.is_occupied(hex)
            && spawn.iter().all(|&(_, pos, _, _)| pos != hex)
    }
}
