//! Cities: first economy experiment. Amounts use quarter units; routes use half-hex costs.
//!
//! This file holds the city types, the tuning constants and the city scenarios'
//! setup; each submodule adds an `impl GameState` block for one concern.
mod builds;
mod citizens;
mod logistics;
#[cfg(test)]
mod tests;
mod view;
mod workers;

use std::collections::HashMap;

use super::hex::{Hex, HexGrid};
use super::mapgen::{generate, start_units};
use super::terrain::{Resource, Tile};
use super::unit::{Team, Unit, UnitType};
use super::{GameState, PLAYER_TEAM};

pub use builds::{Build, BuildUnit, Building};
pub(super) use logistics::{Routes, delivered_share};

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

#[derive(Clone)]
pub(super) struct Site {
    pub team: Team,
    pub food: i32,
    pub production: i32,
    pub label: &'static str,
}

pub(super) fn amount(quarters: i32) -> String {
    format!(
        "{}{:.2}",
        if quarters < 0 { "-" } else { "" },
        quarters.abs() as f32 / 4.0
    )
}

impl GameState {
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
}
