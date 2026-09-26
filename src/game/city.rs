//! First economy experiment. Amounts use quarter units; routes use half-hex costs.
use std::collections::{HashMap, HashSet};

use super::hex::{Hex, HexGrid};
use super::terrain::Terrain;
use super::unit::{Team, Unit, UnitType};
use super::{GameState, PLAYER_TEAM};

/// Camera zoom the city scenarios start at: most of the radius-six map in view.
const SCENARIO_VIEW_HALF_HEIGHT: f32 = 12.0;

pub(super) struct City {
    pub id: u32,
    pub team: Team,
    pub pos: Hex,
    pub population: usize,
    pub food: i32,
    pub production: i32,
    pub worked: Vec<Hex>,
    pub queue: Option<BuildUnit>,
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
    pub fn update_city_hover(&mut self, cursor: Option<glam::Vec2>, size: glam::Vec2) {
        self.hovered_city = cursor
            .filter(|p| {
                p.y >= 42.0 && !(self.selected_city.is_some() && p.x < 660.0 && p.y < 330.0)
            })
            .and_then(|p| {
                let hex = Hex::from_world(self.camera.screen_to_world(p, size));
                self.cities.iter().position(|c| c.pos == hex)
            });
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
                worked: Vec::new(),
                queue: None,
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
        self.notice = "C SELECT CITY - ENTER END PLANNING - F1 COMBAT - F2 CITIES".into();
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
            worked: Vec::new(),
            queue: None,
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
        self.cities[city].queue = Some(build);
        self.notice = format!(
            "BUILDING {} - COST {} PRODUCTION",
            build.name(),
            amount(build.cost())
        );
    }

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
        !self
            .cities
            .iter()
            .any(|c| c.pos == hex || c.worked.contains(&hex))
            && self
                .sites
                .get(&hex)
                .is_none_or(|s| s.team == self.cities[city].team)
    }

    pub(super) fn income(&self, city: usize) -> (i32, i32) {
        let routes = self.routes(city);
        self.cities[city]
            .worked
            .iter()
            .fold((8, 4), |(food, prod), hex| {
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
        while self.cities[city].worked.len() < self.cities[city].population && !tiles.is_empty() {
            let needs_food = food < self.cities[city].population as i32 * 8 + 4;
            tiles.sort_by_key(|(h, f, p)| {
                (-(if needs_food { f * 4 + p } else { p * 4 + f }), h.q, h.r)
            });
            let (hex, f, _) = tiles.remove(0);
            self.cities[city].worked.push(hex);
            food += f;
        }
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
        while self.cities[city].worked.len() < self.cities[city].population
            && !candidates.is_empty()
        {
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

    pub fn select_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        if let Some(i) = self.cities.iter().position(|c| c.team == PLAYER_TEAM) {
            self.selected_city = Some(i);
            self.selected = None;
            self.camera.focus_on(self.cities[i].pos.to_world());
            self.notice = "CLICK TILES TO ASSIGN - A AUTO ASSIGN - ESC UNITS".into();
        }
    }

    /// Escape (or clicking the open city again): back to the units, selecting
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
        self.inspected_tile = None;
    }

    pub(super) fn city_click(&mut self, hex: Hex) -> bool {
        if self.selected_city.is_none() {
            if self
                .cities
                .iter()
                .any(|c| c.pos == hex && c.team == PLAYER_TEAM)
            {
                self.select_city();
                return true;
            }
            return false;
        }
        let i = self.selected_city.unwrap();
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
        if let Some(at) = self.cities[i].worked.iter().position(|h| *h == hex) {
            self.cities[i].worked.remove(at);
            self.notice = "CITIZEN UNASSIGNED".into();
        } else if !self.may_assign(i, hex) {
            self.notice = "CITY OR CLAIMED SITE - CANNOT ASSIGN".into();
        } else if !self.routes(i).costs.contains_key(&hex) {
            self.notice = "NO OPEN ROUTE WITHIN LOGISTICS BUDGET".into();
        } else if self.cities[i].worked.len() >= self.cities[i].population {
            self.notice = "ALL CITIZENS BUSY - RELEASE A WORKED TILE FIRST".into();
        } else {
            self.cities[i].worked.push(hex);
            self.notice = "CITIZEN ASSIGNED".into();
        }
        true
    }

    pub fn end_planning(&mut self) {
        if self.is_resolving() {
            return;
        }
        if let Some(i) =
            (0..self.units.len()).find(|&i| self.is_player_controlled(i) && self.needs_orders(i))
        {
            self.selected_city = None;
            self.selected = Some(i);
            self.camera.focus_on(self.units[i].pos.to_world());
            self.notice = "UNIT NEEDS ORDERS - SPACE TO HOLD".into();
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
            city.production += production;
            let threshold = (10 + 5 * city.population as i32) * 4;
            if city.food >= threshold {
                city.food -= threshold;
                city.population += 1;
            }
            if city.food < 0 {
                city.population = city.population.saturating_sub(1).max(1);
                city.food = 0;
            }
            city.worked.truncate(city.population);
        }
        self.complete_builds();
        for i in 0..self.cities.len() {
            self.reconcile_citizens(i);
        }
        self.notice = "PLANNING - C CITY - SPACE HOLD - ENTER END TURN".into();
    }

    fn complete_builds(&mut self) {
        let mut spawn = Vec::new();
        for i in 0..self.cities.len() {
            if self.cities[i].queue.is_none() && self.cities[i].team == Team::Red {
                self.cities[i].queue = Some(BuildUnit::Melee);
            }
            let Some(build) = self.cities[i].queue else {
                continue;
            };
            if self.cities[i].production < build.cost() {
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
            self.cities[i].queue = None;
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
        g.end_planning();
        g.update(1.0);
        assert_eq!(g.cities[0].production, 6);
        g.update(10.0);
        assert_eq!(g.cities[0].production, 6);
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
        assert!(g.cities[0].queue.is_none());
        let finished = g.units.iter().find(|u| u.team == Team::Blue).unwrap();
        assert_eq!(finished.unit_type, UnitType::Melee);
        assert_eq!(finished.pos.distance(g.cities[0].pos), 1);
    }
}
