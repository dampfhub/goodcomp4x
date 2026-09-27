//! Builds: the city queue (units and buildings), building sites and
//! confirmation, the Barracks queue, and completing builds.
use super::amount;
use crate::game::hex::Hex;
use crate::game::terrain::Resource;
use crate::game::unit::{Team, Unit, UnitType};
use crate::game::{GameState, PLAYER_TEAM};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Building {
    Granary,
    Barracks,
    Mill,
    Workshop,
}

impl Building {
    pub fn name(self) -> &'static str {
        match self {
            Self::Granary => "GRANARY",
            Self::Barracks => "BARRACKS",
            Self::Mill => "MILL",
            Self::Workshop => "WORKSHOP",
        }
    }
    pub fn cost(self) -> i32 {
        match self {
            Self::Granary => 48,
            Self::Barracks => 64,
            Self::Mill => 60,
            Self::Workshop => 80,
        }
    }
    pub fn shortcut(self) -> char {
        match self {
            Self::Granary => '4',
            Self::Barracks => '5',
            Self::Mill => '6',
            Self::Workshop => '7',
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
}

impl Build {
    pub fn name(self) -> &'static str {
        match self {
            Self::Unit(u) => u.name(),
            Self::Building(b) => b.name(),
        }
    }
    pub fn cost(self) -> i32 {
        match self {
            Self::Unit(u) => u.cost(),
            Self::Building(b) => b.cost(),
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
}

impl BuildUnit {
    fn unit_type(self) -> UnitType {
        match self {
            Self::Melee => UnitType::Melee,
            Self::Ranged => UnitType::Ranged,
            Self::Cavalry => UnitType::Cavalry,
            Self::Siege => UnitType::Siege,
            Self::Armored => UnitType::Armored,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Melee => "MELEE",
            Self::Ranged => "RANGED",
            Self::Cavalry => "CAVALRY",
            Self::Siege => "SIEGE",
            Self::Armored => "ARMORED",
        }
    }
    pub fn cost(self) -> i32 {
        match self {
            Self::Melee => 48,
            Self::Ranged => 56,
            Self::Cavalry => 64,
            Self::Siege => 72,
            Self::Armored => 80,
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Melee => "TOUGH CLOSE FIGHTER",
            Self::Ranged => "FIRES FROM 2 TILES",
            Self::Cavalry => "FAST FLANKER, NEEDS HORSES",
            Self::Siege => "LONG RANGE, SLOW",
            Self::Armored => "HEAVY IRON INFANTRY",
        }
    }
    pub fn shortcut(self) -> char {
        match self {
            Self::Melee => '1',
            Self::Ranged => '2',
            Self::Siege => '3',
            Self::Cavalry | Self::Armored => '-',
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

    pub fn queue_selected_city_building(&mut self, building: Building) {
        if self.is_resolving() {
            return;
        }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILDING".into();
            return;
        };
        let c = &mut self.cities[city];
        if c.team != PLAYER_TEAM {
            return;
        }
        if c.built.contains(&building)
            || c.pending_building == Some(building)
            || c.queue.contains(&Build::Building(building))
        {
            self.notice = format!("{} ALREADY EXISTS IN THIS CITY", building.name());
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
                "{} STARTED - CLICK AN OPEN TILE TO CHOOSE ITS SITE",
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

    pub(in crate::game) fn site_available(
        &self,
        city: usize,
        building: Building,
        hex: Hex,
    ) -> bool {
        self.grid.is_passable(hex)
            && (self.is_explored(hex) || self.fog().sees(hex))
            && !self.cities.iter().enumerate().any(|(i, c)| {
                c.pos == hex
                    || [Building::Barracks, Building::Mill, Building::Workshop]
                        .into_iter()
                        .any(|kind| c.placed_site(kind) == Some(hex))
                    || c.planned_sites
                        .iter()
                        .any(|(&kind, &site)| site == hex && (i != city || kind != building))
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
        if !self.site_available(city, building, site) {
            self.notice = format!("{} SITE IS NO LONGER AVAILABLE", building.name());
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
        self.cities[city].planned_sites.remove(&building);
        self.cities[city].built.push(building);
        self.notice = format!("{} FINALIZED", building.name());
    }

    pub fn queue_selected_barracks_unit(&mut self, build: BuildUnit) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else {
            return;
        };
        let c = &mut self.cities[city];
        if c.team != PLAYER_TEAM || c.barracks.is_none() {
            return;
        }
        if let Some(resource) = build.required_resource()
            && self.grid.resource(c.barracks.unwrap()) != Some(resource)
        {
            self.notice = format!("{} REQUIRES BARRACKS ON {}", build.name(), resource.name());
            return;
        }
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
        build
            .required_resource()
            .is_none_or(|resource| self.grid.resource(tile) == Some(resource))
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
        self.notice = format!(
            "CHANGE {} SITE - CLICK A NEW OPEN LAND TILE",
            building.name()
        );
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

    pub(super) fn complete_builds(&mut self) {
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
                    Building::Barracks | Building::Mill | Building::Workshop => {
                        self.cities[i].pending_building = Some(building);
                        if !self.cities[i].planned_sites.contains_key(&building) {
                            self.placing_building = Some((i, building));
                        }
                        self.notice =
                            format!("{} COMPLETE - CHOOSE A SITE, THEN CONFIRM", building.name());
                    }
                }
                continue;
            }
            let city = self.cities[i].pos;
            let Some(pos) = city
                .neighbors()
                .into_iter()
                .find(|h| self.grid.is_passable(*h) && !self.is_occupied(*h))
            else {
                continue;
            };
            self.cities[i].production -= build.cost();
            self.cities[i].queue.remove(0);
            let Build::Unit(unit) = build else {
                unreachable!()
            };
            spawn.push((self.cities[i].team, pos, unit.unit_type()));
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
                .find(|h| self.grid.is_passable(*h) && !self.is_occupied(*h))
            else {
                continue;
            };
            self.cities[i].barracks_production = 0;
            self.cities[i].barracks_queue.remove(0);
            spawn.push((self.cities[i].team, pos, build.unit_type()));
        }
        for (team, pos, kind) in spawn {
            let id = self.next_unit_id;
            self.next_unit_id += 1;
            self.units.push(Unit::new(id, pos, team, kind));
            log::info!("{team:?} city completed {kind:?}");
        }
    }
}
