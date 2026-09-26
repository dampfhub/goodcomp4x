//! First economy experiment. Amounts use quarter units; routes use half-hex costs.
use std::collections::{HashMap, HashSet};

use super::hex::{Hex, HexGrid};
use super::terrain::Terrain;
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
pub enum LaborFocus { Food, Production, Balanced }
impl LaborFocus {
    pub fn name(self) -> &'static str { match self { Self::Food => "FOOD", Self::Production => "PRODUCTION", Self::Balanced => "BALANCED" } }
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
    pub pending_building: Option<Building>,
    pub planned_barracks: Option<Hex>,
    /// Barracks production is independent of the city's main queue.
    pub barracks_queue: Vec<BuildUnit>,
    pub barracks_production: i32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Building {
    Granary,
    Barracks,
}

impl Building {
    pub fn name(self) -> &'static str { match self { Self::Granary => "GRANARY", Self::Barracks => "BARRACKS" } }
    pub fn cost(self) -> i32 { match self { Self::Granary => 48, Self::Barracks => 64 } }
    pub fn shortcut(self) -> char { match self { Self::Granary => '5', Self::Barracks => '6' } }
    pub fn description(self) -> &'static str {
        match self {
            Self::Granary => "+2 FOOD PER TURN FOR THIS CITY.",
            Self::Barracks => "PLACE ON A WORKED TILE. A MANAGER THERE DIRECTS ITS WORK GROUP'S PRODUCTION INTO TROOPS.",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Build {
    Unit(BuildUnit),
    Building(Building),
}

impl Build {
    pub fn name(self) -> &'static str { match self { Self::Unit(u) => u.name(), Self::Building(b) => b.name() } }
    pub fn cost(self) -> i32 { match self { Self::Unit(u) => u.cost(), Self::Building(b) => b.cost() } }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BuildUnit {
    Melee,
    Ranged,
    Cavalry,
    Siege,
}

impl BuildUnit {
    fn unit_type(self) -> UnitType {
        match self {
            Self::Melee => UnitType::Melee,
            Self::Ranged => UnitType::Ranged,
            Self::Cavalry => UnitType::Cavalry,
            Self::Siege => UnitType::Siege,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Melee => "MELEE",
            Self::Ranged => "RANGED",
            Self::Cavalry => "CAVALRY",
            Self::Siege => "SIEGE",
        }
    }
    pub fn cost(self) -> i32 {
        match self {
            Self::Melee => 48,
            Self::Ranged => 56,
            Self::Cavalry => 64,
            Self::Siege => 72,
        }
    }
    pub fn description(self) -> &'static str {
        match self {
            Self::Melee => "TOUGH CLOSE FIGHTER",
            Self::Ranged => "FIRES FROM 2 TILES",
            Self::Cavalry => "FAST FLANKER",
            Self::Siege => "LONG RANGE, SLOW",
        }
    }
    pub fn shortcut(self) -> char {
        match self {
            Self::Melee => '1',
            Self::Ranged => '2',
            Self::Cavalry => '3',
            Self::Siege => '4',
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
        let mut terrain: Vec<_> = hills.into_iter().map(|h| (h, Terrain::Hills)).collect();
        terrain.extend([-4, -3, -2, 2, 3, 4].map(|r| (Hex::new(0, r), Terrain::Mountains)));
        self.grid = HexGrid::new(6, terrain);
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
                pending_building: None,
                planned_barracks: None,
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
                (Hex::new(-1, 2), Terrain::Hills),
                (Hex::new(1, -2), Terrain::Hills),
                (Hex::new(0, 3), Terrain::Mountains),
                (Hex::new(0, -3), Terrain::Mountains),
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
            self.units.push(Unit::new(id, pos, team, UnitType::Melee));
            if team == Team::Red {
                self.player_controlled_units.insert(id);
            }
        }
        self.notice = "F FOUND CITY - BOTH STARTING WARRIORS ARE YOURS TO TEST".into();
        self.selected = self.unit_of_team_at(Hex::new(-4, 0), PLAYER_TEAM);
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
            pending_building: None,
            planned_barracks: None,
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
        let Some(i) = self.selected else { return };
        let unit = &self.units[i];
        if !self.workers.contains(&unit.id) {
            self.notice = "ONLY A WORKER IMPROVES TILES".into();
            return;
        }
        let (food, production, label) = if self.grid.terrain(unit.pos) == Terrain::Hills {
            (1, 4, "MINE")
        } else {
            (4, 0, "FARM")
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
        if self.is_resolving() { return; }
        let Some(city) = self.selected_city else {
            self.notice = "OPEN A CITY WITH C BEFORE CHOOSING A BUILDING".into();
            return;
        };
        let c = &mut self.cities[city];
        if c.team != PLAYER_TEAM { return; }
        if c.built.contains(&building) || c.pending_building == Some(building)
            || c.queue.contains(&Build::Building(building)) {
            self.notice = format!("{} ALREADY EXISTS IN THIS CITY", building.name());
            return;
        }
        if c.queue.is_empty() { c.production = 0; }
        c.queue.push(Build::Building(building));
        // Site choice is part of queuing a Barracks, even if other work is
        // ahead of it. This keeps the build from finishing without a site.
        if building == Building::Barracks {
            self.placing_barracks = Some(city);
            self.notice = "BARRACKS STARTED - CLICK ANY OPEN TILE TO CHOOSE ITS SITE".into();
            return;
        }
        self.notice = format!("BUILDING {} - COST {} PRODUCTION", building.name(), amount(building.cost()));
    }

    pub fn set_selected_city_focus(&mut self, focus: LaborFocus) {
        if let Some(city) = self.selected_city {
            self.cities[city].focus = focus;
            self.auto_assign_city(city);
            self.notice = format!("CITY FOCUS: {}", focus.name());
        }
    }

    pub fn confirm_barracks(&mut self) {
        let Some(city) = self.selected_city else { return; };
        let Some(site) = self.cities[city].planned_barracks else {
            self.notice = "CHOOSE A BARRACKS SITE ON THE MAP FIRST".into(); return;
        };
        if self.cities[city].pending_building != Some(Building::Barracks)
            && !self.cities[city].built.contains(&Building::Barracks) {
            self.notice = "BARRACKS IS STILL UNDER CONSTRUCTION".into(); return;
        }
        self.cities[city].pending_building = None;
        self.cities[city].barracks = Some(site);
        self.cities[city].planned_barracks = None;
        if !self.cities[city].built.contains(&Building::Barracks) {
            self.cities[city].built.push(Building::Barracks);
        }
        self.notice = "BARRACKS FINALIZED - MOVE MANAGER ONTO IT TO TRAIN TROOPS".into();
    }

    pub fn queue_selected_barracks_unit(&mut self, build: BuildUnit) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else { return; };
        let c = &mut self.cities[city];
        if c.team != PLAYER_TEAM || c.barracks.is_none() { return; }
        if c.barracks_queue.is_empty() { c.barracks_production = 0; }
        c.barracks_queue.push(build);
        self.notice = format!("BARRACKS TRAINING {} - NEEDS MANAGER ON BARRACKS", build.name());
    }

    /// Reopens placement for a Barracks that is queued or complete but has
    /// not been finalized yet. Completed Barracks are permanent.
    pub fn change_selected_barracks_site(&mut self) {
        let Some(city) = self.selected_city else { return; };
        if self.cities[city].team != PLAYER_TEAM
            || self.cities[city].barracks.is_some()
            || self.cities[city].planned_barracks.is_none()
        {
            return;
        }
        self.placing_barracks = Some(city);
        self.notice = "CHANGE BARRACKS SITE - CLICK A NEW OPEN LAND TILE".into();
    }

    pub fn move_selected_city_queue_item(&mut self, index: usize, up: bool) {
        let Some(city) = self.selected_city else { return; };
        let queue = &mut self.cities[city].queue;
        let other = if up { index.checked_sub(1) } else { index.checked_add(1) };
        if let Some(other) = other.filter(|&other| other < queue.len()) {
            queue.swap(index, other);
            self.notice = "CITY QUEUE REORDERED".into();
        }
    }

    pub fn remove_selected_city_queue_item(&mut self, index: usize) {
        let Some(city) = self.selected_city else { return; };
        if index >= self.cities[city].queue.len() { return; }
        let removed = self.cities[city].queue.remove(index);
        if index == 0 { self.cities[city].production = 0; }
        if removed == Build::Building(Building::Barracks) {
            self.placing_barracks = None;
            self.cities[city].planned_barracks = None;
        }
        self.notice = format!("REMOVED {} FROM CITY QUEUE", removed.name());
    }

    pub fn move_selected_barracks_queue_item(&mut self, index: usize, up: bool) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else { return; };
        let queue = &mut self.cities[city].barracks_queue;
        let other = if up { index.checked_sub(1) } else { index.checked_add(1) };
        if let Some(other) = other.filter(|&other| other < queue.len()) {
            queue.swap(index, other);
            self.notice = "BARRACKS QUEUE REORDERED".into();
        }
    }

    pub fn remove_selected_barracks_queue_item(&mut self, index: usize) {
        let Some(city) = self.selected_barracks.or(self.selected_city) else { return; };
        if index >= self.cities[city].barracks_queue.len() { return; }
        let removed = self.cities[city].barracks_queue.remove(index);
        if index == 0 { self.cities[city].barracks_production = 0; }
        self.notice = format!("REMOVED {} FROM BARRACKS QUEUE", removed.name());
    }

    /// Queue hotkeys operate on the city line currently being produced.
    pub fn remove_selected_city_queue_head(&mut self) { self.remove_selected_city_queue_item(0); }
    pub fn move_selected_city_queue_head(&mut self, up: bool) { self.move_selected_city_queue_item(0, up); }

    pub(super) fn routes(&self, city: usize) -> Routes {
        let city = &self.cities[city];
        let mut result = Routes {
            costs: HashMap::new(),
        };
        if self.enemy_of_team_at(city.pos, city.team).is_some() {
            return result;
        }
        result.costs.insert(city.pos, 0);
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
            for n in hex.neighbors() {
                if !self.grid.is_passable(n)
                    || self.enemy_of_team_at(n, city.team).is_some()
                    || self
                        .cities
                        .iter()
                        .any(|c| c.pos == n && c.team != city.team)
                {
                    continue;
                }
                let step = if self.is_road_hex(n) {
                    1
                } else if self.grid.terrain(n) == Terrain::Hills {
                    3
                } else {
                    2
                };
                let total = cost + step;
                if total <= 8 && result.costs.get(&n).is_none_or(|old| total < *old) {
                    result.costs.insert(n, total);
                }
            }
        }
        result
    }

    pub(super) fn tile_yield(&self, hex: Hex) -> (i32, i32) {
        self.sites.get(&hex).map_or_else(
            || match self.grid.terrain(hex) {
                Terrain::Plains => (2, 1),
                Terrain::Hills => (1, 2),
                Terrain::Mountains => (0, 0),
            },
            |s| (s.food, s.production),
        )
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
        !self.cities.iter().enumerate().any(|(i, c)| i != city && (c.pos == hex || c.worked.contains(&hex)))
            && self.sites.get(&hex).is_none_or(|site| site.team == self.cities[city].team)
            && self.routes(city).costs.contains_key(&hex)
    }

    /// Relocates the manager and carries each worker's axial offset with it.
    /// Workers whose matching tile is unavailable are filled by normal auto-assignment.
    fn move_manager(&mut self, city: usize, new_manager: Hex) {
        let old = self.cities[city].worked.clone();
        let old_manager = old[0];
        let routes = self.routes(city);
        let other_claims: HashSet<Hex> = self.cities.iter().enumerate()
            .filter(|(i, _)| *i != city).flat_map(|(_, c)| c.worked.iter().copied()).collect();
        let mut relocated = vec![new_manager];
        for worker in old.into_iter().skip(1) {
            let target = Hex::new(new_manager.q + worker.q - old_manager.q, new_manager.r + worker.r - old_manager.r);
            let legal = routes.costs.contains_key(&target)
                && !other_claims.contains(&target)
                && !self.cities.iter().any(|c| c.pos == target)
                && self.sites.get(&target).is_none_or(|site| site.team == self.cities[city].team);
            if legal && !relocated.contains(&target) { relocated.push(target); }
        }
        self.cities[city].worked = relocated;
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
        self.reconcile_citizens(city);
        self.notice = "MANAGER MOVED - WORKERS FOLLOWED WHERE POSSIBLE".into();
    }

    pub(super) fn income(&self, city: usize) -> (i32, i32) {
        let routes = self.routes(city);
        let granary_food = if self.cities[city].built.contains(&Building::Granary) { 8 } else { 0 };
        self.cities[city]
            .worked
            .iter()
            .fold((8 + granary_food, 4), |(food, prod), hex| {
                let share = routes.costs.get(hex).map_or(0, |c| delivered_share(*c));
                let (f, p) = self.tile_yield(*hex);
                (food + f * share, prod + p * share)
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
                (*h, f * delivered_share(*cost), p * delivered_share(*cost))
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
                let score = match focus { LaborFocus::Food => f * 5 + p, LaborFocus::Production => p * 5 + f, LaborFocus::Balanced => if needs_food { f * 4 + p } else { p * 4 + f } };
                (-score, h.q, h.r)
            });
            let (hex, f, _) = tiles.remove(0);
            self.cities[city].worked.push(hex);
            food += f;
        }
        self.cities[city].remembered_worked = self.cities[city].worked.clone();
    }

    /// Keeps manual choices where possible, drops tiles cut off by conflict,
    /// and fills new citizen slots after growth.
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
        // Restore player assignments that a hostile unit temporarily cut off.
        let remembered = self.cities[city].remembered_worked.clone();
        for h in remembered {
            if self.cities[city].worked.len() >= self.cities[city].population.min(MAX_CITY_POPULATION) { break; }
            if routes.costs.contains_key(&h) && !other_claims.contains(&h)
                && !self.cities[city].worked.contains(&h)
                && self.may_assign(city, h) {
                self.cities[city].worked.push(h);
            }
        }
        if let Some(manager) = self.cities[city].worked.first().copied() {
            self.cities[city]
                .worked
                .retain(|h| *h == manager || manager.distance(*h) == 1);
        }
        if self.cities[city].remembered_worked.iter().any(|h| !routes.costs.contains_key(h)) {
            return;
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
                (*h, f * delivered_share(*cost), p * delivered_share(*cost))
            })
            .collect();
        while self.cities[city].worked.len() < self.cities[city].population.min(MAX_CITY_POPULATION)
            && !candidates.is_empty()
        {
            if let Some(manager) = self.cities[city].worked.first().copied() {
                candidates.retain(|(hex, _, _)| manager.distance(*hex) == 1);
            }
            candidates.sort_by_key(|(h, f, p)| (-(f * 3 + p), h.q, h.r));
            self.cities[city].worked.push(candidates.remove(0).0);
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
        self.selected_city = Some(i);
        self.selected_barracks = None;
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera.focus_on(self.cities[i].pos.to_world());
        self.notice = if self.city_needs_build(i) {
            format!("CHOOSE WHAT CITY {} BUILDS - 1-4", self.cities[i].id + 1)
        } else {
            "CLICK TILES TO ASSIGN - A AUTO ASSIGN - CLICK A UNIT TO LEAVE".into()
        };
    }

    /// One of the player's cities with nothing queued to build. The turn
    /// waits for these, as it does for units without orders.
    pub(super) fn city_needs_build(&self, i: usize) -> bool {
        let city = &self.cities[i];
        city.team == PLAYER_TEAM && city.queue.is_empty()
    }

    /// Clicking the open city again: back to the units, selecting
    /// the next one that needs orders.
    pub fn close_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        self.leave_city_view();
        self.select_next_unit();
    }

    pub(super) fn leave_city_view(&mut self) {
        self.selected_city = None;
        self.selected_barracks = None;
        self.moving_manager = None;
        self.placing_barracks = None;
        self.inspected_tile = None;
    }

    pub(super) fn open_barracks(&mut self, city: usize) {
        if self.cities[city].barracks.is_none() { return; }
        self.selected_city = None;
        self.selected_barracks = Some(city);
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera.focus_on(self.cities[city].barracks.unwrap().to_world());
        self.notice = "BARRACKS - QUEUE TROOPS OR CLICK CITY TO RETURN".into();
    }

    pub fn open_selected_city_from_barracks(&mut self) {
        if let Some(city) = self.selected_barracks { self.open_city(city); }
    }

    pub(super) fn city_click(&mut self, hex: Hex) -> bool {
        if self.selected_city.is_none() {
            if let Some(i) = self.cities.iter().position(|c| c.barracks == Some(hex) && c.team == PLAYER_TEAM) {
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
        if self.cities[i].barracks == Some(hex) {
            self.open_barracks(i);
            return true;
        }
        if self.placing_barracks == Some(i) {
            if !self.grid.is_passable(hex) || self.cities.iter().any(|c| c.pos == hex) {
                self.notice = "BARRACKS NEEDS AN OPEN LAND TILE".into();
            } else {
                self.cities[i].planned_barracks = Some(hex);
                self.placing_barracks = None;
                self.notice = if self.cities[i].pending_building.is_some() || self.cities[i].built.contains(&Building::Barracks) { "BARRACKS SITE SELECTED - CLICK CONFIRM IN THE CITY TRAY".into() } else { "BARRACKS SITE SELECTED - CONSTRUCTION CONTINUES".into() };
            }
            return true;
        }
        // Selecting one of your units, or clicking the city itself again,
        // leaves the city view.
        if let Some(unit) = self.controlled_unit_at(hex) {
            self.leave_city_view();
            self.selected = Some(unit);
            return true;
        }
        if hex == self.cities[i].pos {
            self.close_city();
            return true;
        }
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
        } else if let Some(manager) = self.cities[i].worked.first().copied()
            && (self.cities[i].worked.len() >= self.cities[i].population.min(MAX_CITY_POPULATION)
                || manager.distance(hex) != 1)
            && self.may_be_manager(i, hex)
        {
            self.move_manager(i, hex);
        } else if !self.may_assign(i, hex) {
            self.notice = "CITY OR CLAIMED SITE - CANNOT ASSIGN".into();
        } else if !self.routes(i).costs.contains_key(&hex) {
            self.notice = "NO OPEN ROUTE WITHIN LOGISTICS BUDGET".into();
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
            if self.is_player_controlled(i) && self.needs_orders(i) { self.units[i].holding = true; }
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
        for (city, (food, production)) in self.cities.iter_mut().zip(income) {
            city.food += food - city.population as i32 * 8;
            let barracks_active = city.barracks.is_some_and(|h| city.worked.first() == Some(&h));
            let worker_production = production - 4;
            if !city.queue.is_empty() {
                // A manager at a Barracks directs the work group there, but
                // does not stop the city itself benefiting from its labor.
                city.production += production;
            } else {
                city.production = 0;
            }
            if !city.barracks_queue.is_empty() && barracks_active {
                city.barracks_production += worker_production;
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
        let mut spawn = Vec::new();
        for i in 0..self.cities.len() {
            if self.cities[i].queue.is_empty() && self.cities[i].team == Team::Red {
                self.cities[i].queue.push(Build::Unit(BuildUnit::Melee));
            }
            let Some(build) = self.cities[i].queue.first().copied() else {
                continue;
            };
            if self.cities[i].production < build.cost() {
                continue;
            }
            if let Build::Building(building) = build {
                self.cities[i].production = 0;
                self.cities[i].queue.remove(0);
                match building {
                    Building::Granary => {
                        self.cities[i].built.push(building);
                        self.notice = "GRANARY COMPLETE - +2 FOOD PER TURN".into();
                    }
                    Building::Barracks => {
                        self.cities[i].pending_building = Some(building);
                        if self.cities[i].planned_barracks.is_none() { self.placing_barracks = Some(i); }
                        self.notice = "BARRACKS COMPLETE - CHOOSE A SITE, THEN CONFIRM".into();
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
            let Build::Unit(unit) = build else { unreachable!() };
            spawn.push((self.cities[i].team, pos, unit.unit_type()));
        }
        for i in 0..self.cities.len() {
            let Some(build) = self.cities[i].barracks_queue.first().copied() else { continue; };
            if self.cities[i].barracks_production < build.cost() { continue; }
            let Some(barracks) = self.cities[i].barracks else { continue; };
            let Some(pos) = barracks.neighbors().into_iter().find(|h| self.grid.is_passable(*h) && !self.is_occupied(*h)) else { continue; };
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
    fn barracks_uses_worker_production_only_with_manager_on_its_tile() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        let manager = Hex::new(-1, 0);
        let worker = Hex::new(-1, 1);
        g.cities[0].worked = vec![manager, worker];
        g.cities[0].barracks = Some(manager);
        g.cities[0].barracks_queue = vec![BuildUnit::Melee];
        g.cities[0].queue = vec![Build::Unit(BuildUnit::Siege)];
        let (_, total_production) = g.income(0);
        g.resolve_economy();
        assert_eq!(g.cities[0].barracks_production, total_production - 4);
        assert_eq!(g.cities[0].production, total_production, "the city and Barracks both receive active group production");

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
        assert!(g.units.iter().filter(|u| u.team == PLAYER_TEAM).all(|u| u.holding));
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
        assert_eq!(g.cities[0].planned_barracks, Some(site));
        assert_eq!(g.placing_barracks, None, "choosing a site exits Barracks placement mode");
        let normal_city_click = Hex::new(-1, 0);
        g.city_click(normal_city_click);
        assert_ne!(g.cities[0].planned_barracks, Some(normal_city_click), "a later city click must not move the planned Barracks");
        g.cities[0].production = Building::Barracks.cost();
        g.complete_builds();
        assert_eq!(g.cities[0].pending_building, Some(Building::Barracks));
        assert!(g.cities[0].barracks.is_none());
        g.confirm_barracks();
        assert_eq!(g.cities[0].barracks, Some(site));
    }

    #[test]
    fn city_queue_completes_in_order_and_can_be_reordered_or_removed() {
        let mut g = GameState::city_scenario();
        g.units.clear();
        g.selected_city = Some(0);
        g.queue_selected_city_unit(BuildUnit::Melee);
        g.queue_selected_city_unit(BuildUnit::Ranged);
        assert_eq!(g.cities[0].queue, vec![Build::Unit(BuildUnit::Melee), Build::Unit(BuildUnit::Ranged)]);
        g.move_selected_city_queue_item(1, true);
        assert_eq!(g.cities[0].queue[0], Build::Unit(BuildUnit::Ranged));
        g.remove_selected_city_queue_item(1);
        assert_eq!(g.cities[0].queue, vec![Build::Unit(BuildUnit::Ranged)]);
        g.cities[0].production = BuildUnit::Ranged.cost();
        g.complete_builds();
        assert!(g.cities[0].queue.is_empty());
        assert!(g.units.iter().any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged));
    }

    #[test]
    fn removing_a_queued_barracks_clears_its_placement_preview() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Barracks);
        g.city_click(Hex::new(-2, 0));
        assert!(g.cities[0].planned_barracks.is_some());
        g.remove_selected_city_queue_item(0);
        assert!(g.cities[0].planned_barracks.is_none());
        assert_eq!(g.placing_barracks, None);
    }

    #[test]
    fn queued_barracks_opens_site_selection_behind_another_build() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.queue_selected_city_unit(BuildUnit::Melee);
        g.queue_selected_city_building(Building::Barracks);
        assert_eq!(g.placing_barracks, Some(0));
        let site = Hex::new(-2, 0);
        g.city_click(site);
        assert_eq!(g.cities[0].planned_barracks, Some(site));
        assert_eq!(g.placing_barracks, None);
    }

    #[test]
    fn planned_barracks_site_can_change_before_final_confirmation() {
        let mut g = GameState::city_scenario();
        g.selected_city = Some(0);
        g.queue_selected_city_building(Building::Barracks);
        let original = Hex::new(-2, 0);
        let revised = Hex::new(-1, 0);
        g.city_click(original);
        g.change_selected_barracks_site();
        assert_eq!(g.placing_barracks, Some(0));
        g.city_click(revised);
        assert_eq!(g.cities[0].planned_barracks, Some(revised));
        assert!(g.cities[0].barracks.is_none());
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
        assert!(g.units.iter().any(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged));
    }
}
