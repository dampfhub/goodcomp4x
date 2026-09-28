//! Cities: first economy experiment. Amounts use quarter units; routes use half-hex costs.
//!
//! This file holds the city types, the tuning constants and the city scenarios'
//! setup; each submodule adds an `impl GameState` block for one concern.
mod barracks;
mod builds;
mod citizens;
mod economy;
mod founding;
mod interior;
mod logistics;
mod rail;
#[cfg(test)]
mod tests;
mod view;

use super::hex::{Hex, HexGrid};
use super::mapgen::{generate, start_units};
use super::ruins::{Ruin, RuinReward};
use super::settings::Settings;
use super::terrain::{Resource, Tile};
use super::unit::{Team, Unit, UnitType};
use super::workers::WorkerJob;
use super::{GameState, PLAYER_TEAM};
use crate::game::fast_hash::HashMap;

pub(in crate::game) use barracks::{CITY_TRAINING_SLOWDOWN, UNITS_PER_DEPOSIT};
pub use builds::{Build, BuildUnit, Building};
pub(in crate::game) use builds::{GATHER_SHORTCUT, GATHER_YIELD, GROW_SHORTCUT, WORKER_SHORTCUT};
pub use economy::Stock;
pub(in crate::game) use economy::{
    FOOD_PER_CITIZEN, STARTING_STOCK, grow_price, resource_icon, stock_icons, stock_words,
    turns_icon,
};
pub(super) use interior::CORE_HP;
pub(super) use interior::Interior;
pub(in crate::game) use interior::in_bounds as in_interior;
pub(super) use logistics::{Routes, delivered_share};

/// Camera zoom the city scenarios start at: most of the radius-six map in view.
const SCENARIO_VIEW_HALF_HEIGHT: f32 = 12.0;
/// One manager and up to six nearby workers.
pub(super) const MAX_CITY_POPULATION: usize = 7;
pub(super) const BARRACKS_MAX_HP: f32 = 220.0;
pub(super) const BARRACKS_DEFENSE: f32 = 25.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum LaborFocus {
    Food,
    Wood,
    Metal,
    Balanced,
}
impl LaborFocus {
    pub const ALL: [Self; 4] = [Self::Food, Self::Wood, Self::Metal, Self::Balanced];

    pub fn name(self) -> &'static str {
        match self {
            Self::Food => "FOOD",
            Self::Wood => "WOOD",
            Self::Metal => "METAL",
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
    /// Work done on the head of the queue, in quarter turns
    /// (`WORK_PER_TURN` a turn, `economy.rs`).
    pub progress: i32,
    pub barracks_hp: f32,
    pub coastal_battery_hp: f32,
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
    /// New placed buildings share one site map instead of adding a city field each.
    pub extra_buildings: HashMap<Building, Hex>,
    /// Scroll position in the city tray's building list.
    pub building_scroll: usize,
    /// Scroll position in the city tray's list of workers and their jobs.
    pub worker_scroll: usize,
    /// The Barracks' own queue, independent of the city's main queue; it
    /// advances only while the manager stands on the Barracks.
    pub barracks_queue: Vec<BuildUnit>,
    pub barracks_progress: i32,
    /// A separate tactical board. Adjacent field troops project copies here.
    pub interior: Interior,
    /// Workers at home, safe and off the map (`workers.rs`).
    pub workers: u32,
    /// Of `workers`, those the player recalled: they stay home, sent out to
    /// no job, until released (`workers.rs`). Never more than `workers`.
    pub held_workers: u32,
    /// What the city placed on the map, waiting for a worker, first to go
    /// first (`workers.rs`).
    pub worker_jobs: Vec<WorkerJob>,
}

impl City {
    /// A newly founded city: one citizen and one worker.
    pub fn new(id: u32, team: Team, pos: Hex) -> Self {
        Self {
            id,
            team,
            pos,
            population: 1,
            progress: 0,
            barracks_hp: BARRACKS_MAX_HP,
            coastal_battery_hp: 150.0,
            worked: Vec::new(),
            remembered_worked: Vec::new(),
            focus: LaborFocus::Balanced,
            queue: Vec::new(),
            built: Vec::new(),
            barracks: None,
            mill: None,
            workshop: None,
            extra_buildings: HashMap::default(),
            building_scroll: 0,
            worker_scroll: 0,
            barracks_queue: Vec::new(),
            barracks_progress: 0,
            interior: Interior::default(),
            workers: 1,
            held_workers: 0,
            worker_jobs: Vec::new(),
        }
    }

    pub fn placed_site(&self, building: Building) -> Option<Hex> {
        match building {
            Building::Barracks => self.barracks,
            Building::Mill => self.mill,
            Building::Workshop => self.workshop,
            _ => self.extra_buildings.get(&building).copied(),
        }
    }

    pub(in crate::game) fn set_placed_site(&mut self, building: Building, site: Hex) {
        match building {
            Building::Barracks => {
                self.barracks = Some(site);
                self.barracks_hp = BARRACKS_MAX_HP;
            }
            Building::Mill => self.mill = Some(site),
            Building::Workshop => self.workshop = Some(site),
            _ => {
                self.extra_buildings.insert(building, site);
            }
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

/// Quarters as whole units, with decimals only when they aren't whole:
/// "3", "2.25".
pub(super) fn amount(quarters: i32) -> String {
    if quarters % 4 == 0 {
        (quarters / 4).to_string()
    } else {
        format!("{:.2}", quarters as f32 / 4.0)
    }
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
                population: 2,
                ..City::new(id, team, pos)
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

    /// The world (F4): a map for the player and `Settings::world_ai` AI
    /// sides, each on its own start (the player on the first) with a scout
    /// and either its city or a settler to found it (`world_start_city`).
    /// The map's ruins each get a reward, in turn by the seed.
    pub(super) fn setup_world(&mut self, seed: u32, settings: &Settings) {
        let ai = settings.world_ai_for(seed).min(Team::ALL.len() - 1);
        let map = generate(seed, 1 + ai);
        self.grid = map.grid;
        self.map_seed = Some(seed);
        self.camera.half_height = SCENARIO_VIEW_HALF_HEIGHT;
        self.ruins = map
            .ruins
            .iter()
            .enumerate()
            .map(|(i, &pos)| {
                let rewards = RuinReward::ALL;
                Ruin::new(pos, rewards[(seed as usize + i) % rewards.len()])
            })
            .collect();
        for (&team, &start) in Team::ALL.iter().zip(&map.starts) {
            // The start comes with hills for the scout. Only the blank
            // fallback map lacks them. A city starts with a worker at home.
            let open: Vec<Hex> = start
                .neighbors()
                .into_iter()
                .filter(|h| self.grid.is_passable(*h))
                .collect();
            let (_, scout) = start_units(&self.grid, start).unwrap_or((open[0], open[1]));
            if settings.world_start_city {
                // Every queue starts empty: the AI picks and pays for its
                // builds as it plans (`plan_ai_cities`).
                let id = self.cities.len() as u32;
                self.cities.push(City::new(id, team, start));
                self.auto_assign_city(self.cities.len() - 1);
            } else {
                let id = self.next_unit_id;
                self.next_unit_id += 1;
                self.units.push(Unit::new(id, start, team, UnitType::Melee));
                self.settlers.insert(id);
            }
            let id = self.next_unit_id;
            self.next_unit_id += 1;
            self.units.push(Unit::new(id, scout, team, UnitType::Scout));
        }
        let founding = if settings.world_start_city {
            "C OPENS YOUR CITY"
        } else {
            "F FOUNDS A CITY"
        };
        self.notice = format!("WORLD SEED {seed} - {ai} AI - {founding} - F4 FOR A NEW MAP");
    }
}
