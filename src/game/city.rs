//! First economy experiment. Amounts use quarter units; routes use half-hex costs.
use std::collections::{HashMap, HashSet};

use super::hex::{Hex, HexGrid};
use super::mapgen::{generate, start_units};
use super::terrain::{Resource, Terrain, Tile};
use super::unit::{Team, Unit, UnitType};
use super::{GameState, PLAYER_TEAM};

/// Camera zoom the city scenarios start at: most of the radius-six map in view.
const SCENARIO_VIEW_HALF_HEIGHT: f32 = 12.0;
/// One manager and up to six nearby workers.
pub(super) const MAX_CITY_POPULATION: usize = 7;
pub(super) const CITY_MAX_HP: f32 = 320.0;
pub(super) const BARRACKS_MAX_HP: f32 = 220.0;
pub(super) const CITY_DEFENSE: f32 = 30.0;
pub(super) const BARRACKS_DEFENSE: f32 = 25.0;
pub(super) const CITY_ATTACK: f32 = 26.0;
pub(super) const CITY_ATTACK_RANGE: i32 = 2;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LaborFocus {
    Food,
    Production,
    Balanced,
}
impl LaborFocus {
    pub fn name(self) -> &'static str {
        match self {
            Self::Food => "FOOD",
            Self::Production => "PRODUCTION",
            Self::Balanced => "BALANCED",
        }
    }
}

#[derive(Clone)]
pub(super) struct City {
    pub id: u32,
    pub team: Team,
    pub pos: Hex,
    pub population: usize,
    pub food: i32,
    pub production: i32,
    pub hp: f32,
    pub barracks_hp: f32,
    pub worked: Vec<Hex>,
    /// Manual tiles displaced by a blocked logistics route. They return when
    /// available unless the player changes the assignment.
    pub remembered_worked: Vec<Hex>,
    pub focus: LaborFocus,
    /// The city works the first item, then immediately continues with the
    /// following items. Buildings and units deliberately share this queue.
    pub queue: Vec<Build>,
    pub built: Vec<Building>,
    pub barracks: Option<Hex>,
    pub mill: Option<Hex>,
    pub workshop: Option<Hex>,
    pub pending_building: Option<Building>,
    pub planned_sites: HashMap<Building, Hex>,
    /// Barracks production is independent of the city's main queue.
    pub barracks_queue: Vec<BuildUnit>,
    pub barracks_production: i32,
}

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

impl City {
    pub fn placed_site(&self, building: Building) -> Option<Hex> {
        match building {
            Building::Granary => None,
            Building::Barracks => self.barracks,
            Building::Mill => self.mill,
            Building::Workshop => self.workshop,
        }
    }

    fn set_placed_site(&mut self, building: Building, site: Hex) {
        match building {
            Building::Granary => unreachable!(),
            Building::Barracks => {
                self.barracks = Some(site);
                self.barracks_hp = BARRACKS_MAX_HP;
            }
            Building::Mill => self.mill = Some(site),
            Building::Workshop => self.workshop = Some(site),
        }
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

#[derive(Clone)]
pub(super) struct Site {
    pub team: Team,
    pub food: i32,
    pub production: i32,
    pub label: &'static str,
}

pub(super) struct Routes {
    pub costs: HashMap<Hex, i32>,
}

pub(super) fn delivered_share(cost: i32) -> i32 {
    match cost {
        0..=2 => 4,
        3..=4 => 3,
        5..=6 => 2,
        7..=8 => 1,
        _ => 0,
    }
}

pub(super) fn amount(quarters: i32) -> String {
    format!(
        "{}{:.2}",
        if quarters < 0 { "-" } else { "" },
        quarters.abs() as f32 / 4.0
    )
}

impl GameState {
    pub(super) fn is_road_hex(&self, hex: Hex) -> bool {
        self.roads.contains(&hex) || self.cities.iter().any(|city| city.pos == hex)
    }
    /// Y or the Yields button: shows or hides tile yields around the open city.
    pub fn toggle_yields(&mut self) {
        self.show_yields = !self.show_yields;
    }

    /// The open city, if its tile yields are being shown.
    pub(super) fn yields_city(&self) -> Option<usize> {
        self.selected_city.filter(|_| self.show_yields)
    }

    /// What `hex` produces when worked: a city center's own yield, a site's,
    /// or its terrain's.
    pub(super) fn raw_yield(&self, hex: Hex) -> (i32, i32) {
        if self.cities.iter().any(|c| c.pos == hex) {
            (2, 1)
        } else {
            self.tile_yield(hex)
        }
    }

    pub(super) fn setup_cities(&mut self) {
        let hills = [Hex::new(-2, 2), Hex::new(2, -2), Hex::new(0, 0)];
        let mut terrain: Vec<_> = hills.into_iter().map(|h| (h, Tile::HILLS)).collect();
        terrain.extend([-4, -3, -2, 2, 3, 4].map(|r| (Hex::new(0, r), Tile::MOUNTAINS)));
        self.grid = HexGrid::with_resources(
            6,
            terrain,
            [
                (Hex::new(-2, 0), Resource::Horses),
                (Hex::new(-2, 1), Resource::Iron),
                (Hex::new(2, 0), Resource::Horses),
                (Hex::new(2, -1), Resource::Iron),
            ],
        );
        self.camera.half_height = SCENARIO_VIEW_HALF_HEIGHT;
        for (id, team, sign) in [(0, Team::Blue, -1), (1, Team::Red, 1)] {
            let pos = Hex::new(sign * 4, 0);
            self.cities.push(City {
                id,
                team,
                pos,
                population: 2,
                food: 32,
                production: 0,
                hp: CITY_MAX_HP,
                barracks_hp: BARRACKS_MAX_HP,
                worked: Vec::new(),
                remembered_worked: Vec::new(),
                focus: LaborFocus::Balanced,
                queue: Vec::new(),
                built: Vec::new(),
                barracks: None,
                mill: None,
                workshop: None,
                pending_building: None,
                planned_sites: HashMap::new(),
                barracks_queue: Vec::new(),
                barracks_production: 0,
            });
            for (q, r, food, production, label) in [
                (4, -1, 4, 0, "FARM"),
                (3, 0, 4, 0, "FARM"),
                (2, -2, 0, 4, "MINE"),
                (4, 1, 3, 1, "PASTURE"),
            ] {
                self.sites.insert(
                    Hex::new(sign * q, sign * r),
                    Site {
                        team,
                        food,
                        production,
                        label,
                    },
                );
            }
            for (q, r) in [(4, 0), (3, 0), (3, -1), (3, -2), (2, -2)] {
                self.roads.insert(Hex::new(sign * q, sign * r));
            }
            let worker_id = self.next_unit_id;
            self.next_unit_id += 1;
            self.units.push(Unit::new(
                worker_id,
                Hex::new(sign * 4, sign * 2),
                team,
                UnitType::Melee,
            ));
            self.workers.insert(worker_id);
        }
        for i in 0..self.cities.len() {
            self.auto_assign_city(i);
        }
        self.notice = "C CITY - SPACE HOLD OR END TURN - F1 COMBAT - F2 CITIES".into();
    }

    pub(super) fn setup_frontier(&mut self) {
        self.grid = HexGrid::new(
            6,
            [
                (Hex::new(-1, 2), Tile::HILLS),
                (Hex::new(1, -2), Tile::HILLS),
                (Hex::new(0, 3), Tile::MOUNTAINS),
                (Hex::new(0, -3), Tile::MOUNTAINS),
            ],
        );
        self.camera.half_height = SCENARIO_VIEW_HALF_HEIGHT;
        for (team, pos) in [(Team::Blue, Hex::new(-4, 0)), (Team::Red, Hex::new(4, 0))] {
            let id = self.next_unit_id;
            self.next_unit_id += 1;
            self.units.push(Unit::new(id, pos, team, UnitType::Melee));
            self.settlers.insert(id);
            let worker_id = self.next_unit_id;
            self.next_unit_id += 1;
            let worker_pos = Hex::new(pos.q, pos.r + if team == Team::Blue { 1 } else { -1 });
            self.units
                .push(Unit::new(worker_id, worker_pos, team, UnitType::Melee));
            self.workers.insert(worker_id);
        }
        for (team, pos) in [(Team::Blue, Hex::new(-3, -1)), (Team::Red, Hex::new(3, 1))] {
            let id = self.next_unit_id;
            self.next_unit_id += 1;
            self.units.push(Unit::new(id, pos, team, UnitType::Scout));
            if team == Team::Red {
                self.player_controlled_units.insert(id);
            }
        }
        self.notice = "F FOUND CITY - BOTH STARTING SCOUTS ARE YOURS TO TEST".into();
        self.selected = self.unit_of_team_at(Hex::new(-4, 0), PLAYER_TEAM);
    }

    pub(super) fn setup_world(&mut self, seed: u32) {
        let map = generate(seed);
        self.grid = map.grid;
        self.map_seed = Some(seed);
        self.camera.half_height = SCENARIO_VIEW_HALF_HEIGHT;
        for (team, start) in [Team::Blue, Team::Red].into_iter().zip(map.starts) {
            // Starts come with a flat hex for the worker and hills for the
            // scout, so neither side begins seeing more than the other. Only
            // the blank fallback map lacks them.
            let open: Vec<Hex> = start
                .neighbors()
                .into_iter()
                .filter(|h| self.grid.is_passable(*h))
                .collect();
            let (worker, scout) = start_units(&self.grid, start).unwrap_or((open[0], open[1]));
            for (pos, role, unit_type) in [
                (start, "settler", UnitType::Melee),
                (worker, "worker", UnitType::Melee),
                (scout, "scout", UnitType::Scout),
            ] {
                let id = self.next_unit_id;
                self.next_unit_id += 1;
                self.units.push(Unit::new(id, pos, team, unit_type));
                match role {
                    "settler" => self.settlers.insert(id),
                    "worker" => self.workers.insert(id),
                    _ => false,
                };
            }
        }
        self.notice = format!("WORLD SEED {seed} - F FOUNDS A CITY - F4 FOR A NEW MAP");
    }

    pub fn found_city_selected(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(index) = self.selected else {
            self.notice = "SELECT YOUR SETTLER FIRST".into();
            return;
        };
        let unit = &self.units[index];
        if unit.team != PLAYER_TEAM || !self.settlers.contains(&unit.id) {
            self.notice = "ONLY A SETTLER CAN FOUND A CITY".into();
            return;
        }
        let pos = unit.pos;
        if self.cities.iter().any(|c| c.pos.distance(pos) < 3) {
            self.notice = "TOO CLOSE TO ANOTHER CITY".into();
            return;
        }
        let id = self.cities.len() as u32;
        self.cities.push(City {
            id,
            team: unit.team,
            pos,
            population: 1,
            food: 0,
            production: 0,
            hp: CITY_MAX_HP,
            barracks_hp: BARRACKS_MAX_HP,
            worked: Vec::new(),
            remembered_worked: Vec::new(),
            focus: LaborFocus::Balanced,
            queue: Vec::new(),
            built: Vec::new(),
            barracks: None,
            mill: None,
            workshop: None,
            pending_building: None,
            planned_sites: HashMap::new(),
            barracks_queue: Vec::new(),
            barracks_production: 0,
        });
        self.settlers.remove(&unit.id);
        self.units.remove(index);
        self.selected = None;
        let city = self.cities.len() - 1;
        self.auto_assign_city(city);
        self.selected_city = Some(city);
        self.notice = "CITY FOUNDED - PRESS 1-4 TO BUILD A UNIT".into();
    }

    pub fn build_worker_road_selected(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(i) = self.selected else { return };
        let unit = &self.units[i];
        if !self.workers.contains(&unit.id) {
            self.notice = "ONLY A WORKER BUILDS ROADS".into();
            return;
        }
        self.roads.insert(unit.pos);
        self.notice = "DIRT ROAD BUILT - IT LOWERS LOGISTICS COST".into();
    }

    pub fn improve_worker_tile_selected(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(i) = self.selected else { return };
        let unit = &self.units[i];
        if !self.workers.contains(&unit.id) {
            self.notice = "ONLY A WORKER IMPROVES TILES".into();
            return;
        }
        if self
            .sites
            .get(&unit.pos)
            .is_some_and(|site| site.team != unit.team)
        {
            self.notice = "THIS TILE BELONGS TO THE ENEMY - NO IMPROVEMENT POSSIBLE".into();
            return;
        }
        // Improvements add to what the tile already gives: a mine two
        // production on hills, a lumber mill one in forest or jungle, a farm
        // two food elsewhere.
        let tile = self.grid.tile(unit.pos);
        let (food, production) = tile.yields();
        let (food, production, label) = if tile.hills {
            (food, production + 2, "MINE")
        } else if tile.feature.is_some() {
            (food, production + 1, "LUMBER MILL")
        } else if tile.terrain == Terrain::Snow {
            self.notice = "NOTHING GROWS ON SNOW - NO IMPROVEMENT POSSIBLE".into();
            return;
        } else {
            (food + 2, production, "FARM")
        };
        self.sites.insert(
            unit.pos,
            Site {
                team: unit.team,
                food,
                production,
                label,
            },
        );
        self.notice = format!("{} BUILT", label);
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

    pub fn set_selected_city_focus(&mut self, focus: LaborFocus) {
        if let Some(city) = self.selected_city {
            self.cities[city].focus = focus;
            self.auto_assign_city(city);
            self.notice = format!("CITY FOCUS: {}", focus.name());
        }
    }

    pub(super) fn site_available(&self, city: usize, building: Building, hex: Hex) -> bool {
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

    pub(super) fn city_build_cost(&self, city: usize, build: Build) -> i32 {
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

    pub(super) fn mill_food_share(&self, city: usize, hex: Hex, cost: i32) -> i32 {
        if self.cities.iter().any(|c| {
            c.team == self.cities[city].team && c.mill.is_some_and(|mill| mill.distance(hex) == 1)
        }) {
            4
        } else {
            delivered_share(cost)
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

    pub(super) fn barracks_can_train(&self, city: usize, build: BuildUnit) -> bool {
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

    pub(super) fn routes(&self, city: usize) -> Routes {
        let city = &self.cities[city];
        self.routes_from(city.team, city.pos)
    }

    /// Delivery network to a city center or a placed building. Each endpoint
    /// has its own falloff, so a worker can deliver differently to each.
    pub(super) fn routes_from(&self, team: Team, origin: Hex) -> Routes {
        self.routes_from_by(
            origin,
            |hex| {
                self.enemy_of_team_at(hex, team).is_some()
                    || self.cities.iter().any(|c| c.pos == hex && c.team != team)
            },
            |hex| self.is_road_hex(hex),
        )
    }

    /// Like `routes_from`, with `blocked` deciding which hexes goods can't
    /// cross and `road` which carry them cheaply: the real board for the
    /// economy, or what the player knows of it for what's shown to them
    /// (`known_routes`).
    pub(super) fn routes_from_by(
        &self,
        origin: Hex,
        blocked: impl Fn(Hex) -> bool,
        road: impl Fn(Hex) -> bool,
    ) -> Routes {
        let mut result = Routes {
            costs: HashMap::new(),
        };
        if blocked(origin) {
            return result;
        }
        result.costs.insert(origin, 0);
        let mut visited = HashSet::new();
        loop {
            let next = result
                .costs
                .iter()
                .filter(|(h, _)| !visited.contains(*h))
                .min_by_key(|(h, c)| (**c, h.q, h.r))
                .map(|(h, c)| (*h, *c));
            let Some((hex, cost)) = next else { break };
            visited.insert(hex);
            // Goods come in from water tiles but never travel across them.
            if self.grid.terrain(hex).is_water() {
                continue;
            }
            for n in hex.neighbors() {
                if !self.grid.contains(n) || !self.grid.terrain(n).is_workable() || blocked(n) {
                    continue;
                }
                let step = if road(n) {
                    1
                } else {
                    self.grid.tile(n).route_cost()
                };
                let total = cost + step;
                if total <= 8 && result.costs.get(&n).is_none_or(|old| total < *old) {
                    result.costs.insert(n, total);
                }
            }
        }
        result
    }

    pub(super) fn barracks_income(&self, city: usize) -> i32 {
        let c = &self.cities[city];
        let Some(barracks) = c.barracks.filter(|tile| c.worked.first() == Some(tile)) else {
            return 0;
        };
        let routes = self.routes_from(c.team, barracks);
        c.worked
            .iter()
            .map(|tile| {
                let (_, production) = self.tile_yield(*tile);
                production
                    * routes
                        .costs
                        .get(tile)
                        .map_or(0, |cost| delivered_share(*cost))
            })
            .sum()
    }

    /// What a worked tile produces: its site's yield or its terrain's, plus
    /// one food for fresh water (a river or lake beside it).
    pub(super) fn tile_yield(&self, hex: Hex) -> (i32, i32) {
        let (food, production) = self
            .sites
            .get(&hex)
            .map_or_else(|| self.grid.tile(hex).yields(), |s| (s.food, s.production));
        (food + i32::from(self.grid.has_fresh_water(hex)), production)
    }

    fn may_assign(&self, city: usize, hex: Hex) -> bool {
        let manager_can_reach = self.cities[city]
            .worked
            .first()
            .is_none_or(|manager| manager.distance(hex) == 1);
        manager_can_reach
            && !self
                .cities
                .iter()
                .any(|c| c.pos == hex || c.worked.contains(&hex))
            && self
                .sites
                .get(&hex)
                .is_none_or(|s| s.team == self.cities[city].team)
    }

    fn may_be_manager(&self, city: usize, hex: Hex) -> bool {
        !self
            .cities
            .iter()
            .enumerate()
            .any(|(i, c)| i != city && (c.pos == hex || c.worked.contains(&hex)))
            && self
                .sites
                .get(&hex)
                .is_none_or(|site| site.team == self.cities[city].team)
            && !self.grid.terrain(hex).is_water()
            && self.routes(city).costs.contains_key(&hex)
    }

    /// Whether `hex` can take the city's next citizen as far as the manager
    /// goes: the first citizen, the manager, has to be on land.
    fn may_manage_or_work(&self, city: usize, hex: Hex) -> bool {
        !self.cities[city].worked.is_empty() || !self.grid.terrain(hex).is_water()
    }

    /// Relocates the manager and carries each worker's axial offset with it.
    /// Workers whose matching tile is unavailable are filled by normal auto-assignment.
    fn move_manager(&mut self, city: usize, new_manager: Hex) {
        let old = self.cities[city].worked.clone();
        let old_manager = old[0];
        let routes = self.routes(city);
        let other_claims: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != city)
            .flat_map(|(_, c)| c.worked.iter().copied())
            .collect();
        let mut relocated = vec![new_manager];
        for worker in old.into_iter().skip(1) {
            let target = Hex::new(
                new_manager.q + worker.q - old_manager.q,
                new_manager.r + worker.r - old_manager.r,
            );
            let legal = routes.costs.contains_key(&target)
                && !other_claims.contains(&target)
                && !self.cities.iter().any(|c| c.pos == target)
                && self
                    .sites
                    .get(&target)
                    .is_none_or(|site| site.team == self.cities[city].team);
            if legal && !relocated.contains(&target) {
                relocated.push(target);
            }
        }
        self.cities[city].worked = relocated;
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
        self.reconcile_citizens(city);
        self.notice = "MANAGER MOVED - WORKERS FOLLOWED WHERE POSSIBLE".into();
    }

    pub(super) fn income(&self, city: usize) -> (i32, i32) {
        let routes = self.routes(city);
        let granary_food = if self.cities[city].built.contains(&Building::Granary) {
            8
        } else {
            0
        };
        self.cities[city]
            .worked
            .iter()
            .fold((8 + granary_food, 4), |(food, prod), hex| {
                let (food_share, production_share) =
                    routes.costs.get(hex).map_or((0, 0), |&cost| {
                        (
                            self.mill_food_share(city, *hex, cost),
                            delivered_share(cost),
                        )
                    });
                let (f, p) = self.tile_yield(*hex);
                (food + f * food_share, prod + p * production_share)
            })
    }

    pub(super) fn auto_assign_city(&mut self, city: usize) {
        self.cities[city].worked.clear();
        let routes = self.routes(city);
        let mut tiles: Vec<_> = routes
            .costs
            .iter()
            .filter(|(h, _)| self.may_assign(city, **h))
            .map(|(h, cost)| {
                let (f, p) = self.tile_yield(*h);
                (
                    *h,
                    f * self.mill_food_share(city, *h, *cost),
                    p * delivered_share(*cost),
                )
            })
            .collect();
        let mut food = 8;
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !tiles.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                tiles.retain(|(hex, _, _)| manager.distance(*hex) == 1);
            }
            let needs_food = food < self.cities[city].population as i32 * 8 + 4;
            let focus = self.cities[city].focus;
            tiles.sort_by_key(|(h, f, p)| {
                let score = match focus {
                    LaborFocus::Food => f * 5 + p,
                    LaborFocus::Production => p * 5 + f,
                    LaborFocus::Balanced => {
                        if needs_food {
                            f * 4 + p
                        } else {
                            p * 4 + f
                        }
                    }
                };
                (-score, h.q, h.r)
            });
            // The first citizen is the manager, who must stand on land.
            let Some(pick) = tiles
                .iter()
                .position(|(h, _, _)| self.may_manage_or_work(city, *h))
            else {
                break;
            };
            let (hex, f, _) = tiles.remove(pick);
            self.cities[city].worked.push(hex);
            food += f;
        }
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
    }

    /// Keeps manual choices where possible, substitutes around temporarily
    /// blocked tiles, then restores those choices once they are available.
    fn reconcile_citizens(&mut self, city: usize) {
        let routes = self.routes(city);
        let other_claims: HashSet<Hex> = self
            .cities
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != city)
            .flat_map(|(_, c)| c.worked.iter().copied())
            .collect();
        self.cities[city]
            .worked
            .retain(|h| routes.costs.contains_key(h) && !other_claims.contains(h));
        // The first remembered tile is the manager. Restore it to the first
        // slot before restoring workers, so an automatically promoted worker
        // never becomes a permanent manager after the original tile clears.
        let remembered = self.cities[city].remembered_worked.clone();
        let capacity = self.cities[city].population.min(MAX_CITY_POPULATION);
        if let Some(manager) = remembered.first().copied()
            && routes.costs.contains_key(&manager)
            && !other_claims.contains(&manager)
        {
            let worked = &mut self.cities[city].worked;
            if !worked.contains(&manager) && worked.len() >= capacity {
                if let Some(fallback) = worked.iter().rposition(|h| !remembered.contains(h)) {
                    worked.remove(fallback);
                } else {
                    worked.pop();
                }
            }
            worked.retain(|h| *h != manager);
            worked.insert(0, manager);
        }

        // Desired assignments take priority over temporary replacements. A
        // manual change updates `remembered_worked`, so this only restores
        // tiles that the player has not since replaced.
        for h in remembered.iter().copied().skip(1) {
            if routes.costs.contains_key(&h)
                && !other_claims.contains(&h)
                && !self.cities[city].worked.contains(&h)
                && self.may_assign(city, h)
            {
                if self.cities[city].worked.len() >= capacity {
                    if let Some(fallback) = self.cities[city]
                        .worked
                        .iter()
                        .rposition(|tile| !remembered.contains(tile))
                    {
                        self.cities[city].worked.remove(fallback);
                    } else {
                        continue;
                    }
                }
                self.cities[city].worked.push(h);
            }
        }
        if let Some(manager) = self.cities[city].worked.first().copied() {
            self.cities[city]
                .worked
                .retain(|h| *h == manager || manager.distance(*h) == 1);
        }
        let mut candidates: Vec<_> = routes
            .costs
            .iter()
            .filter(|(h, _)| !self.cities[city].worked.contains(h) && !other_claims.contains(h))
            .filter(|(h, _)| {
                self.sites
                    .get(h)
                    .is_none_or(|s| s.team == self.cities[city].team)
            })
            .map(|(h, cost)| {
                let (f, p) = self.tile_yield(*h);
                (
                    *h,
                    f * self.mill_food_share(city, *h, *cost),
                    p * delivered_share(*cost),
                )
            })
            .collect();
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !candidates.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                candidates.retain(|(hex, _, _)| manager.distance(*hex) == 1);
            }
            candidates.sort_by_key(|(h, f, p)| (-(f * 3 + p), h.q, h.r));
            let Some(pick) = candidates
                .iter()
                .position(|(h, _, _)| self.may_manage_or_work(city, *h))
            else {
                break;
            };
            self.cities[city].worked.push(candidates.remove(pick).0);
        }
    }

    pub(super) fn growth_status(&self, city: usize) -> (i32, i32, String) {
        let c = &self.cities[city];
        let threshold = (10 + 5 * c.population as i32) * 4;
        let net = self.income(city).0 - c.population as i32 * 8;
        let turns = if net > 0 {
            ((threshold - c.food).max(0) + net - 1) / net
        } else {
            0
        };
        let label = if net > 0 {
            format!("GROWTH IN {turns} TURNS")
        } else {
            "NO GROWTH: NEED FOOD".into()
        };
        ((c.food * 100 / threshold).clamp(0, 100), threshold, label)
    }

    pub fn auto_assign_selected_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        if let Some(city) = self.selected_city {
            self.auto_assign_city(city);
            self.notice = "CITIZENS AUTO ASSIGNED".into();
        }
    }

    /// C: opens a city that needs something to build, or else the first of
    /// the player's cities.
    pub fn select_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        let city = (0..self.cities.len())
            .find(|&i| self.city_needs_build(i))
            .or_else(|| self.cities.iter().position(|c| c.team == PLAYER_TEAM));
        if let Some(i) = city {
            self.open_city(i);
        }
    }

    /// Opens city `i`'s view and glides the camera to it.
    pub(super) fn open_city(&mut self, i: usize) {
        if self.selected_city != Some(i) {
            self.city_queue_scroll = 0;
        }
        self.selected_city = Some(i);
        self.selected_barracks = None;
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera.focus_on(self.cities[i].pos.to_world());
        self.notice = if self.city_needs_build(i) {
            format!("CHOOSE WHAT CITY {} BUILDS - 1-4", self.cities[i].id + 1)
        } else {
            "CLICK TILES TO ASSIGN - A AUTO ASSIGN - ESC OR SPACE TO EXIT".into()
        };
    }

    /// One of the player's cities with nothing queued to build. The turn
    /// waits for these, as it does for units without orders.
    pub(super) fn city_needs_build(&self, i: usize) -> bool {
        let city = &self.cities[i];
        city.team == PLAYER_TEAM && city.queue.is_empty()
    }

    pub(super) fn leave_city_view(&mut self) {
        self.queue_drag = None;
        self.selected_city = None;
        self.selected_barracks = None;
        self.moving_manager = None;
        self.placing_building = None;
        self.inspected_tile = None;
    }

    /// Escape and Space dismiss city or building management without issuing a
    /// unit order or starting the quit hold.
    pub fn exit_structure_menu(&mut self) -> bool {
        if self.selected_city.is_none() && self.selected_barracks.is_none() {
            return false;
        }
        self.leave_city_view();
        self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
        true
    }

    pub(super) fn open_barracks(&mut self, city: usize) {
        if self.cities[city].barracks.is_none() {
            return;
        }
        if self.selected_barracks != Some(city) {
            self.barracks_queue_scroll = 0;
        }
        self.selected_city = None;
        self.selected_barracks = Some(city);
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera
            .focus_on(self.cities[city].barracks.unwrap().to_world());
        self.notice = "BARRACKS - QUEUE TROOPS OR CLICK CITY TO RETURN".into();
    }

    pub fn open_selected_city_from_barracks(&mut self) {
        if let Some(city) = self.selected_barracks {
            self.open_city(city);
        }
    }

    pub(super) fn city_click(&mut self, hex: Hex) -> bool {
        // A structure menu owns all map clicks until its explicit exit action.
        // This keeps city assignment, Barracks management, and unit selection
        // on one consistent interaction model.
        if self.selected_barracks.is_some() {
            self.inspected_tile = Some(hex);
            self.notice = "BARRACKS MENU - PRESS ESC OR SPACE TO EXIT".into();
            return true;
        }
        if self.selected_city.is_none() {
            if let Some(i) = self
                .cities
                .iter()
                .position(|c| c.barracks == Some(hex) && c.team == PLAYER_TEAM)
            {
                self.open_barracks(i);
                return true;
            }
            if let Some(i) = self
                .cities
                .iter()
                .position(|c| c.pos == hex && c.team == PLAYER_TEAM)
            {
                self.open_city(i);
                return true;
            }
            return false;
        }
        let i = self.selected_city.unwrap();
        if let Some((city, building)) = self.placing_building
            && city == i
        {
            if !self.site_available(i, building, hex) {
                self.notice = format!("{} NEEDS AN OPEN LAND TILE", building.name());
            } else {
                self.cities[i].planned_sites.insert(building, hex);
                self.placing_building = None;
                if self.cities[i].pending_building == Some(building)
                    && self.cities[i].production
                        < self.city_build_cost(i, Build::Building(building))
                {
                    self.cities[i].pending_building = None;
                }
                self.notice = if self.cities[i].pending_building == Some(building)
                    || (self.cities[i].queue.first() == Some(&Build::Building(building))
                        && self.cities[i].production
                            >= self.city_build_cost(i, Build::Building(building)))
                {
                    format!(
                        "{} SITE SELECTED - CLICK CONFIRM IN THE CITY TRAY",
                        building.name()
                    )
                } else {
                    format!("{} SITE SELECTED - CONSTRUCTION CONTINUES", building.name())
                };
            }
            return true;
        }
        // City management owns map clicks. Dismiss it with Escape or Space
        // before selecting units, so workers may be assigned onto a unit's
        // tile without the unit stealing the click.
        self.inspected_tile = Some(hex);
        if self.moving_manager == Some(i) {
            if self.cities[i].worked.first() == Some(&hex) {
                self.moving_manager = None;
                self.notice = "MANAGER MOVE CANCELLED".into();
            } else if self.may_be_manager(i, hex) {
                self.moving_manager = None;
                self.move_manager(i, hex);
            } else {
                self.notice = "MANAGER NEEDS A REACHABLE, UNCLAIMED TILE".into();
            }
            return true;
        }
        if let Some(at) = self.cities[i].worked.iter().position(|h| *h == hex) {
            if at == 0 {
                self.moving_manager = Some(i);
                self.notice = "MANAGER PICKED UP - CLICK A DESTINATION".into();
                return true;
            }
            self.cities[i].worked.remove(at);
            self.cities[i].remembered_worked.retain(|h| *h != hex);
            self.notice = "CITIZEN UNASSIGNED".into();
        } else if !self.may_assign(i, hex) {
            self.notice = "CLICK A WORKED TILE TO MOVE OR RELEASE A CITIZEN; CLICK AN OPEN ADJACENT TILE TO ASSIGN".into();
        } else if !self.routes(i).costs.contains_key(&hex) {
            self.notice = "NO OPEN ROUTE WITHIN LOGISTICS BUDGET".into();
        } else if !self.may_manage_or_work(i, hex) {
            self.notice = "THE FIRST CITIZEN MANAGES THE OTHERS AND MUST WORK LAND".into();
        } else if self.cities[i].worked.len() >= self.cities[i].population.min(MAX_CITY_POPULATION)
        {
            self.notice = "ALL CITIZENS BUSY - RELEASE A WORKED TILE FIRST".into();
        } else {
            self.cities[i].worked.push(hex);
            self.cities[i].remembered_worked = self.cities[i].worked.clone();
            self.notice = "CITIZEN ASSIGNED".into();
        }
        true
    }

    /// Space (once nothing needs orders) or the End Turn button: resolves the
    /// turn, unless a unit still needs orders or a city needs something to
    /// build, in which case that's selected instead.
    pub fn end_planning(&mut self) {
        if self.is_resolving() {
            return;
        }
        for i in 0..self.units.len() {
            if self.is_player_controlled(i) && self.needs_orders(i) {
                self.units[i].holding = true;
            }
        }
        if let Some(i) = (0..self.cities.len()).find(|&i| self.city_needs_build(i)) {
            self.open_city(i);
            return;
        }
        for i in 0..self.cities.len() {
            if self.cities[i].team != PLAYER_TEAM {
                self.auto_assign_city(i);
            }
        }
        self.selected_city = None;
        self.notice = "RESOLVING ORDERS".into();
        self.resolve_turn();
    }

    pub(super) fn resolve_economy(&mut self) {
        let income: Vec<_> = (0..self.cities.len()).map(|i| self.income(i)).collect();
        let barracks_income: Vec<_> = (0..self.cities.len())
            .map(|i| self.barracks_income(i))
            .collect();
        for ((city, (food, production)), barracks_production) in
            self.cities.iter_mut().zip(income).zip(barracks_income)
        {
            city.food += food - city.population as i32 * 8;
            if !city.queue.is_empty() {
                // A manager at a Barracks directs the work group there, but
                // does not stop the city itself benefiting from its labor.
                city.production += production;
            } else {
                city.production = 0;
            }
            if !city.barracks_queue.is_empty() && barracks_production > 0 {
                city.barracks_production += barracks_production;
            } else if city.barracks_queue.is_empty() {
                city.barracks_production = 0;
            }
            let threshold = (10 + 5 * city.population as i32) * 4;
            if city.food >= threshold && city.population < MAX_CITY_POPULATION {
                city.food -= threshold;
                city.population += 1;
            }
            if city.food < 0 {
                city.population = city.population.saturating_sub(1).max(1);
                city.food = 0;
            }
            city.worked
                .truncate(city.population.min(MAX_CITY_POPULATION));
        }
        self.complete_builds();
        for i in 0..self.cities.len() {
            self.reconcile_citizens(i);
        }
        self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
    }

    fn complete_builds(&mut self) {
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roads_improve_delivery_and_enemy_occupation_blocks_the_site() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.roads.clear();
        let tile = Hex::new(-1, 0);
        assert_eq!(g.routes(0).costs[&tile], 6);
        for q in -3..=-1 {
            g.roads.insert(Hex::new(q, 0));
        }
        assert_eq!(g.routes(0).costs[&tile], 3);
        g.units.push(super::super::unit::Unit::new(
            100,
            tile,
            Team::Red,
            super::super::unit::UnitType::Melee,
        ));
        assert!(!g.routes(0).costs.contains_key(&tile));
    }
    #[test]
    fn economy_ticks_once_with_no_units_and_preserves_quarters() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.cities[0].worked.clear();
        let tile = Hex::new(-1, 0);
        g.roads.clear();
        g.cities[0].worked.push(tile);
        assert_eq!(g.income(0), (12, 6));
        // The turn waits until the city has something to build; siege costs
        // more than one turn's production, so none is spent.
        g.end_planning();
        assert!(!g.is_resolving());
        g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
        g.end_planning();
        g.update(1.0);
        assert_eq!(g.cities[0].production, 6);
        g.update(10.0);
        assert_eq!(g.cities[0].production, 6);
    }

    #[test]
    fn active_build_accumulates_production_and_population_is_capped_at_seven() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.cities[0].worked = vec![Hex::new(-1, 0)];
        g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
        g.resolve_economy();
        assert!(g.cities[0].production > 0);

        g.cities[0].population = MAX_CITY_POPULATION;
        g.cities[0].food = 10_000;
        g.resolve_economy();
        assert_eq!(g.cities[0].population, MAX_CITY_POPULATION);
    }

    /// A land tile with no road, city or unit that a worker can improve.
    fn open_tile(g: &GameState) -> Hex {
        g.grid
            .all_hexes()
            .find(|&h| {
                g.grid.is_passable(h)
                    && g.grid.tile(h).terrain != Terrain::Snow
                    && !g.roads.contains(&h)
                    && g.cities.iter().all(|c| c.pos != h)
                    && g.units.iter().all(|u| u.pos != h)
            })
            .unwrap()
    }

    /// Selects the player's worker, moved to `at`.
    fn select_worker_at(g: &mut GameState, at: Hex) -> usize {
        let i = g
            .units
            .iter()
            .position(|u| u.team == PLAYER_TEAM && g.workers.contains(&u.id))
            .unwrap();
        g.units[i].pos = at;
        g.selected = Some(i);
        i
    }

    #[test]
    fn worker_actions_wait_for_the_turn_to_finish_playing() {
        use super::super::turn::Phase;
        let mut g = GameState::city_scenario();
        let at = open_tile(&g);
        g.sites.remove(&at);
        select_worker_at(&mut g, at);
        g.pending_steps.push_back((UnitType::Melee, Phase::Move));
        assert!(g.is_resolving());
        g.build_worker_road_selected();
        g.improve_worker_tile_selected();
        assert!(!g.roads.contains(&at));
        assert!(!g.sites.contains_key(&at));

        g.pending_steps.clear();
        g.build_worker_road_selected();
        g.improve_worker_tile_selected();
        assert!(g.roads.contains(&at));
        assert_eq!(g.sites[&at].team, PLAYER_TEAM);
    }

    #[test]
    fn a_worker_cannot_improve_over_an_enemy_site() {
        let mut g = GameState::city_scenario();
        let at = open_tile(&g);
        select_worker_at(&mut g, at);
        g.sites.insert(
            at,
            Site {
                team: Team::Red,
                food: 3,
                production: 0,
                label: "FARM",
            },
        );
        g.improve_worker_tile_selected();
        assert_eq!(g.sites[&at].team, Team::Red);
        assert_eq!(g.sites[&at].food, 3);
    }

    #[test]
    fn workers_after_the_manager_must_be_adjacent_to_it() {
        let mut g = GameState::city_scenario();
        g.cities[0].worked.clear();
        let manager = Hex::new(-1, 0);
        let nearby = Hex::new(-1, 1);
        let distant = Hex::new(-3, 0);
        assert!(g.may_assign(0, manager));
        g.cities[0].worked.push(manager);
        assert!(g.may_assign(0, nearby));
        assert!(!g.may_assign(0, distant));
    }

    #[test]
    fn blocked_worked_tile_is_restored_after_the_unit_leaves() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        let manager = Hex::new(-2, 0);
        let blocked_worker = Hex::new(-1, 0);
        g.cities[0].worked = vec![manager, blocked_worker];
        g.cities[0].remembered_worked = g.cities[0].worked.clone();
        g.units
            .push(Unit::new(777, blocked_worker, Team::Red, UnitType::Melee));

        g.reconcile_citizens(0);
        assert!(!g.cities[0].worked.contains(&blocked_worker));
        assert!(g.cities[0].remembered_worked.contains(&blocked_worker));

        g.units.clear();
        g.reconcile_citizens(0);
        assert!(g.cities[0].worked.contains(&blocked_worker));
    }
    #[test]
    fn growth_starvation_and_assignments_obey_population() {
        let mut g = GameState::city_scenario();
        g.cities[0].food = 100;
        g.resolve_economy();
        assert_eq!(g.cities[0].population, 3);
        g.cities[0].food = -100;
        g.resolve_economy();
        assert_eq!(
            g.cities[0].population, 2,
            "starvation loses at most one population per turn"
        );
        g.auto_assign_city(0);
        assert_eq!(g.cities[0].worked.len(), 2);
        assert!(!g.may_assign(1, g.cities[0].worked[0]));
    }

    #[test]
    fn frontier_settler_founds_city_and_city_spends_production_on_unit() {
        let mut g = GameState::frontier_scenario();
        g.found_city_selected();
        assert_eq!(g.cities.len(), 1);
        assert_eq!(g.settlers.len(), 1, "the opposing settler remains");
        g.queue_selected_city_unit(BuildUnit::Melee);
        g.cities[0].production = BuildUnit::Melee.cost();
        g.complete_builds();
        assert!(g.cities[0].queue.is_empty());
        let finished = g.units.iter().find(|u| u.team == Team::Blue).unwrap();
        assert_eq!(finished.unit_type, UnitType::Melee);
        assert_eq!(finished.pos.distance(g.cities[0].pos), 1);
    }

    #[test]
    fn granary_is_unique_and_adds_two_food_per_turn() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.cities[0].worked.clear();
        assert_eq!(g.income(0).0, 8);
        g.cities[0].queue = vec![Build::Building(Building::Granary)];
        g.cities[0].production = Building::Granary.cost();
        g.complete_builds();
        assert!(g.cities[0].built.contains(&Building::Granary));
        assert_eq!(g.income(0).0, 16);
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Granary);
        assert!(g.cities[0].queue.is_empty());
    }

    #[test]
    fn barracks_has_its_own_delivery_falloff_when_manager_is_on_site() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        let manager = Hex::new(-1, 0);
        let worker = Hex::new(-1, 1);
        g.cities[0].worked = vec![manager, worker];
        g.cities[0].barracks = Some(manager);
        g.cities[0].barracks_queue = vec![BuildUnit::Melee];
        g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
        let (_, total_production) = g.income(0);
        let barracks_production = g.barracks_income(0);
        g.resolve_economy();
        assert_eq!(g.cities[0].barracks_production, barracks_production);
        assert_eq!(
            g.cities[0].production, total_production,
            "the city and Barracks both receive active group production"
        );

        g.cities[0].worked.swap(0, 1);
        let stored = g.cities[0].barracks_production;
        g.resolve_economy();
        assert_eq!(g.cities[0].barracks_production, stored);
    }

    #[test]
    fn end_turn_holds_unfinished_player_units() {
        let mut g = GameState::new();
        assert!(g.next_unit_needing_orders(None).is_some());
        g.end_planning();
        assert!(
            g.units
                .iter()
                .filter(|u| u.team == PLAYER_TEAM)
                .all(|u| u.holding)
        );
        assert!(g.is_resolving());
    }

    #[test]
    fn barracks_site_can_be_chosen_before_completion_and_needs_confirmation() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Barracks);
        let site = Hex::new(-2, 0);
        assert!(g.grid.is_passable(site));
        g.city_click(site);
        assert_eq!(
            g.cities[0].planned_sites.get(&Building::Barracks).copied(),
            Some(site)
        );
        assert_eq!(
            g.placing_building, None,
            "choosing a site exits Barracks placement mode"
        );
        let normal_city_click = Hex::new(-1, 0);
        g.city_click(normal_city_click);
        assert_ne!(
            g.cities[0].planned_sites.get(&Building::Barracks).copied(),
            Some(normal_city_click),
            "a later city click must not move the planned Barracks"
        );
        g.cities[0].production = Building::Barracks.cost();
        g.complete_builds();
        assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
        assert!(g.cities[0].barracks.is_none());
        g.confirm_building(Building::Barracks);
        assert_eq!(g.cities[0].barracks, Some(site));
    }

    #[test]
    fn a_barracks_may_stand_on_an_unworked_tile_as_its_card_says() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        let unworked = g
            .grid
            .all_hexes()
            .find(|&h| {
                !g.cities.iter().any(|c| c.worked.contains(&h))
                    && g.site_available(0, Building::Barracks, h)
            })
            .expect("an open, unworked land tile");
        g.queue_selected_city_building(Building::Barracks);
        g.city_click(unworked);
        assert_eq!(
            g.cities[0].planned_sites.get(&Building::Barracks),
            Some(&unworked)
        );
        let card = Building::Barracks.description();
        assert!(!card.contains("WORKED TILE"), "{card}");
        assert!(card.contains("OPEN LAND"), "{card}");
    }

    #[test]
    fn a_destroyed_barracks_can_be_rebuilt() {
        use super::super::turn::Phase;
        let mut g = GameState::city_scenario();
        g.units.clear();
        let old_site = Hex::new(-2, 0);
        g.cities[0].barracks = Some(old_site);
        g.cities[0].built.push(Building::Barracks);
        g.cities[0].barracks_hp = 1.0;
        g.cities[0].barracks_queue = vec![BuildUnit::Melee];
        g.open_barracks(0);
        g.units.push(Unit::new(
            901,
            old_site.neighbors()[0],
            Team::Red,
            UnitType::Ranged,
        ));
        g.units[0].planned_attack = Some(old_site);
        g.resolve_step(UnitType::Ranged, Phase::Attack);
        assert_eq!(g.cities[0].barracks, None);
        assert!(!g.cities[0].built.contains(&Building::Barracks));
        assert!(g.cities[0].barracks_queue.is_empty());
        assert_eq!(g.selected_barracks, None, "its view closes with it");

        g.units.clear();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Barracks);
        assert_eq!(
            g.cities[0].queue.last(),
            Some(&Build::Building(Building::Barracks))
        );
        let site = Hex::new(-1, 0);
        g.city_click(site);
        g.cities[0]
            .queue
            .retain(|&b| b == Build::Building(Building::Barracks));
        g.cities[0].production = Building::Barracks.cost();
        g.complete_builds();
        g.confirm_building(Building::Barracks);
        assert_eq!(g.cities[0].barracks, Some(site));
        assert_eq!(g.cities[0].barracks_hp, BARRACKS_MAX_HP);
    }

    #[test]
    fn city_queue_completes_in_order_and_can_be_reordered_or_removed() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        g.queue_selected_city_unit(BuildUnit::Melee);
        g.queue_selected_city_unit(BuildUnit::Ranged);
        assert_eq!(
            g.cities[0].queue,
            vec![
                Build::Unit(BuildUnit::Melee),
                Build::Unit(BuildUnit::Ranged)
            ]
        );
        g.move_selected_city_queue_item(1, true);
        assert_eq!(g.cities[0].queue[0], Build::Unit(BuildUnit::Ranged));
        g.remove_selected_city_queue_item(1);
        assert_eq!(g.cities[0].queue, vec![Build::Unit(BuildUnit::Ranged)]);
        g.cities[0].production = BuildUnit::Ranged.cost();
        g.complete_builds();
        assert!(g.cities[0].queue.is_empty());
        assert!(
            g.units
                .iter()
                .any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged)
        );
    }

    #[test]
    fn removing_a_queued_barracks_clears_its_placement_preview() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Barracks);
        g.city_click(Hex::new(-2, 0));
        assert!(
            g.cities[0]
                .planned_sites
                .get(&Building::Barracks)
                .copied()
                .is_some()
        );
        g.remove_selected_city_queue_item(0);
        assert!(
            g.cities[0]
                .planned_sites
                .get(&Building::Barracks)
                .copied()
                .is_none()
        );
        assert_eq!(g.placing_building, None);
    }

    #[test]
    fn queued_barracks_opens_site_selection_behind_another_build() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.queue_selected_city_unit(BuildUnit::Melee);
        g.queue_selected_city_building(Building::Barracks);
        assert_eq!(g.placing_building, Some((0, Building::Barracks)));
        let site = Hex::new(-2, 0);
        g.city_click(site);
        assert_eq!(
            g.cities[0].planned_sites.get(&Building::Barracks).copied(),
            Some(site)
        );
        assert_eq!(g.placing_building, None);
    }

    #[test]
    fn planned_barracks_site_can_change_before_final_confirmation() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Barracks);
        let original = Hex::new(-2, 0);
        let revised = Hex::new(-1, 0);
        g.city_click(original);
        g.change_selected_building_site(Building::Barracks);
        assert_eq!(g.placing_building, Some((0, Building::Barracks)));
        g.city_click(revised);
        assert_eq!(
            g.cities[0].planned_sites.get(&Building::Barracks).copied(),
            Some(revised)
        );
        assert!(g.cities[0].barracks.is_none());
    }

    #[test]
    fn mill_restores_food_delivery_only_within_city_reach() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.roads.clear();
        let worked = Hex::new(-1, 0);
        assert_eq!(g.routes(0).costs.get(&worked), Some(&6));
        g.cities[0].worked = vec![worked];
        let food_before = g.income(0).0;
        let food_yield = g.tile_yield(worked).0;
        g.cities[0].mill = Some(Hex::new(-2, 0));
        assert_eq!(g.income(0).0, food_before + food_yield * 2);

        let out_of_reach = g
            .grid
            .all_hexes()
            .filter(|&h| g.grid.is_passable(h) && !g.routes(0).costs.contains_key(&h))
            .find(|&h| h.neighbors().into_iter().any(|n| g.grid.is_passable(n)))
            .expect("an open tile beyond the logistics cutoff");
        let mill_site = out_of_reach
            .neighbors()
            .into_iter()
            .find(|&h| g.grid.is_passable(h))
            .unwrap();
        g.cities[0].worked = vec![out_of_reach];
        g.cities[0].mill = Some(mill_site);
        assert_eq!(
            g.income(0).0,
            8,
            "a mill cannot bypass the hard route cutoff"
        );
    }

    #[test]
    fn workshop_allows_early_confirmation_of_adjacent_buildings() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        let workshop = Hex::new(-2, 0);
        let barracks = Hex::new(-1, 0);
        assert!(g.site_available(0, Building::Workshop, workshop));
        assert!(g.site_available(0, Building::Barracks, barracks));
        g.cities[0].workshop = Some(workshop);
        g.queue_selected_city_building(Building::Barracks);
        g.city_click(barracks);
        assert_eq!(
            g.city_build_cost(0, Build::Building(Building::Barracks)),
            Building::Barracks.cost() / 2
        );
        g.cities[0].production = Building::Barracks.cost() / 2 - 1;
        g.confirm_building(Building::Barracks);
        assert_eq!(g.cities[0].barracks, None);
        g.cities[0].production += 1;
        g.confirm_building(Building::Barracks);
        assert_eq!(g.cities[0].barracks, Some(barracks));
        assert!(g.cities[0].queue.is_empty());
        assert_eq!(g.cities[0].production, 0);
    }

    #[test]
    fn moving_a_half_built_site_away_from_workshop_resumes_construction() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        let workshop = Hex::new(-2, 0);
        let adjacent = Hex::new(-1, 0);
        let distant = g
            .grid
            .all_hexes()
            .find(|&hex| hex.distance(workshop) > 1 && g.site_available(0, Building::Barracks, hex))
            .expect("open site outside workshop reach");
        g.cities[0].workshop = Some(workshop);
        g.queue_selected_city_building(Building::Barracks);
        g.city_click(adjacent);
        let half = Building::Barracks.cost() / 2;
        g.cities[0].production = half;
        g.complete_builds();
        assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
        assert_eq!(
            g.cities[0].queue.first(),
            Some(&Build::Building(Building::Barracks))
        );
        g.change_selected_building_site(Building::Barracks);
        g.city_click(distant);
        assert_eq!(g.cities[0].pending_building, None);
        assert_eq!(g.cities[0].production, half);
        assert_eq!(
            g.city_build_cost(0, Build::Building(Building::Barracks)),
            Building::Barracks.cost()
        );
        g.confirm_building(Building::Barracks);
        assert_eq!(g.cities[0].barracks, None);
        assert_eq!(
            g.cities[0].queue.first(),
            Some(&Build::Building(Building::Barracks))
        );
        g.cities[0].production = Building::Barracks.cost();
        g.complete_builds();
        assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
        g.confirm_building(Building::Barracks);
        assert_eq!(g.cities[0].barracks, Some(distant));
        assert!(g.cities[0].queue.is_empty());
    }

    #[test]
    fn placed_buildings_cannot_share_a_site() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        let site = Hex::new(-2, 0);
        g.cities[0].mill = Some(site);
        g.queue_selected_city_building(Building::Workshop);
        g.city_click(site);
        assert!(!g.cities[0].planned_sites.contains_key(&Building::Workshop));
        assert_eq!(g.placing_building, Some((0, Building::Workshop)));
    }

    #[test]
    fn debug_completion_only_finishes_the_selected_production_lane() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Mill);
        g.city_click(Hex::new(-2, 0));
        g.cities[1].queue = vec![Build::Building(Building::Granary)];
        g.cities[1].production = Building::Granary.cost();
        g.debug_complete_current_production();
        assert_eq!(g.cities[0].pending_building, Some(Building::Mill));
        assert_eq!(g.cities[1].queue, vec![Build::Building(Building::Granary)]);
        assert!(!g.cities[1].built.contains(&Building::Granary));

        g.cities[0].barracks = Some(Hex::new(-1, 0));
        g.selected_city = None;
        g.selected_barracks = Some(0);
        g.cities[0].barracks_queue = vec![BuildUnit::Melee];
        g.debug_complete_current_production();
        assert!(g.cities[0].barracks_queue.is_empty());
        assert!(
            g.units
                .iter()
                .any(|u| u.team == PLAYER_TEAM && u.unit_type == UnitType::Melee)
        );
        assert_eq!(g.cities[0].pending_building, Some(Building::Mill));
    }

    #[test]
    fn barracks_queue_is_independent_and_completes_in_order() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        let site = Hex::new(-1, 0);
        g.cities[0].barracks = Some(site);
        g.cities[0].worked = vec![site];
        g.queue_selected_barracks_unit(BuildUnit::Melee);
        g.queue_selected_barracks_unit(BuildUnit::Ranged);
        g.move_selected_barracks_queue_item(1, true);
        assert_eq!(g.cities[0].barracks_queue[0], BuildUnit::Ranged);
        g.cities[0].barracks_production = BuildUnit::Ranged.cost();
        g.complete_builds();
        assert_eq!(g.cities[0].barracks_queue, vec![BuildUnit::Melee]);
        assert!(
            g.units
                .iter()
                .any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged)
        );
    }

    #[test]
    fn resource_units_require_a_barracks_on_the_matching_resource() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.cities[0].barracks = Some(Hex::new(-2, 0));
        assert!(g.barracks_can_train(0, BuildUnit::Cavalry));
        assert!(!g.barracks_can_train(0, BuildUnit::Armored));
        g.queue_selected_barracks_unit(BuildUnit::Cavalry);
        assert_eq!(g.cities[0].barracks_queue, vec![BuildUnit::Cavalry]);
        g.cities[0].barracks = Some(Hex::new(-2, 1));
        assert!(g.barracks_can_train(0, BuildUnit::Armored));
        assert!(!g.barracks_can_train(0, BuildUnit::Cavalry));
    }

    #[test]
    fn structure_menu_can_be_dismissed_without_selecting_a_unit() {
        let mut g = GameState::city_scenario();
        g.open_city(0);
        assert!(g.exit_structure_menu());
        assert_eq!(g.selected_city, None);
        assert!(!g.exit_structure_menu());
    }

    #[test]
    fn city_menu_keeps_barracks_clicks_in_manager_assignment_context() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        let manager = Hex::new(-2, 0);
        let barracks = Hex::new(-1, 0);
        g.cities[0].worked = vec![manager];
        g.cities[0].barracks = Some(barracks);
        g.open_city(0);

        g.city_click(manager);
        assert_eq!(g.moving_manager, Some(0));
        g.city_click(barracks);

        assert_eq!(g.cities[0].worked.first(), Some(&barracks));
        assert_eq!(g.selected_city, Some(0));
        assert_eq!(g.selected_barracks, None);
    }
}
