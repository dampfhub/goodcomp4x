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
pub(in crate::game) use economy::{
    FOOD_PER_CITIZEN, Lane, STARTING_STOCK, grow_price, resource_icon, stock_icons, stock_words,
    turns_icon,
};
pub use economy::{Queued, Stock};
pub(super) use interior::CORE_HP;
pub(super) use interior::Interior;
pub(in crate::game) use interior::in_bounds as in_interior;
pub(super) use logistics::{Routes, delivered_share};

/// Camera zoom the city scenarios start at: most of the radius-six map in view.
const SCENARIO_VIEW_HALF_HEIGHT: f32 = 12.0;
/// Workers each manager has at most, on the tiles around it.
pub(super) const WORKERS_PER_MANAGER: usize = 6;
/// Managers a city has at most, each starting a cluster of itself and up
/// to `WORKERS_PER_MANAGER` workers.
pub(super) const MAX_MANAGERS: usize = 4;
/// A cluster's citizens: its manager and its workers.
pub(super) const CLUSTER_SIZE: usize = 1 + WORKERS_PER_MANAGER;
/// Four clusters of a manager and six workers.
pub(super) const MAX_CITY_POPULATION: usize = MAX_MANAGERS * CLUSTER_SIZE;
pub(super) const BARRACKS_MAX_HP: f32 = 220.0;
pub(super) const BARRACKS_DEFENSE: f32 = 25.0;

/// A good a city's citizens bring in, as its priority order ranks them.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Good {
    Food,
    Wood,
    Metal,
}

impl Good {
    pub const ALL: [Self; 3] = [Self::Food, Self::Wood, Self::Metal];

    pub fn name(self) -> &'static str {
        match self {
            Self::Food => "FOOD",
            Self::Wood => "WOOD",
            Self::Metal => "METAL",
        }
    }

    /// This good's amount in a tile's `(food, wood, metal)`.
    fn of(self, (food, wood, metal): (i32, i32, i32)) -> i32 {
        match self {
            Self::Food => food,
            Self::Wood => wood,
            Self::Metal => metal,
        }
    }
}

/// A city's priority order: food, wood and metal, first to last, each
/// once. Auto-assign weighs a tile's delivered goods by their places
/// (`WEIGHTS`), with food first until the city is fed (`score`).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub struct Priorities(pub [Good; 3]);

impl Default for Priorities {
    fn default() -> Self {
        Self(Good::ALL)
    }
}

impl Priorities {
    /// What the first, second and third good count for: the first
    /// dominates, and ties go down the list.
    pub const WEIGHTS: [i32; 3] = [9, 3, 1];

    /// Whether this is an order of the three goods, each once (a network
    /// plan's may not be).
    pub fn is_order(self) -> bool {
        Good::ALL.iter().all(|good| self.0.contains(good))
    }

    /// Where `good` stands, 0 first.
    pub fn rank(self, good: Good) -> usize {
        self.0.iter().position(|&g| g == good).unwrap_or(0)
    }

    /// The same order with `good` moved to the front, the others keeping
    /// theirs.
    pub fn with_first(self, good: Good) -> Self {
        self.moved(self.rank(good), 0)
    }

    /// The same order with the good at `from` taken out and put back at
    /// `to` (a chip dragged onto another).
    pub fn moved(self, from: usize, to: usize) -> Self {
        let mut goods = self.0;
        if from < goods.len() && to < goods.len() {
            if from < to {
                goods[from..=to].rotate_left(1);
            } else {
                goods[to..=from].rotate_right(1);
            }
        }
        Self(goods)
    }

    /// A tile's worth to auto-assign: its delivered `(food, wood, metal)`,
    /// each times its place's weight. Until the city is `fed` (the food
    /// floor: its tiles' food covers its upkeep and one more), food counts
    /// as first whatever the order, the others keeping theirs.
    pub fn score(self, goods: (i32, i32, i32), fed: bool) -> i32 {
        let order = if fed {
            self
        } else {
            self.with_first(Good::Food)
        };
        order
            .0
            .iter()
            .zip(Self::WEIGHTS)
            .map(|(good, weight)| good.of(goods) * weight)
            .sum()
    }

    /// "FOOD > WOOD > METAL".
    pub fn text(self) -> String {
        self.0.map(Good::name).join(" > ")
    }
}

/// A manager and the workers it runs, each on a tile beside it. A city's
/// citizens work in up to `MAX_MANAGERS` of these; a tile belongs to one
/// city and one cluster.
#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Cluster {
    /// On land, and never beside another of its city's managers.
    pub manager: Hex,
    /// Up to `WORKERS_PER_MANAGER`, each beside the manager.
    pub workers: Vec<Hex>,
}

impl Cluster {
    pub fn new(manager: Hex) -> Self {
        Self {
            manager,
            workers: Vec::new(),
        }
    }

    /// Its tiles: the manager's, then its workers'.
    pub fn tiles(&self) -> impl Iterator<Item = Hex> + '_ {
        std::iter::once(self.manager).chain(self.workers.iter().copied())
    }

    /// Its citizens: the manager and its workers.
    pub fn citizens(&self) -> usize {
        1 + self.workers.len()
    }
}

/// The managers a city of `population` may have (`City::managers_allowed`).
pub(super) fn managers_for(population: usize) -> usize {
    population.min(MAX_CITY_POPULATION).div_ceil(CLUSTER_SIZE)
}

/// How cluster `cluster` of `clusters` marks its manager, on the map and in
/// the city tray: M, or with several clusters M1 to M4.
pub(in crate::game) fn manager_label(cluster: usize, clusters: usize) -> String {
    if clusters > 1 {
        format!("M{}", cluster + 1)
    } else {
        "M".into()
    }
}

/// Every tile `clusters` work, cluster by cluster.
pub(super) fn cluster_tiles(clusters: &[Cluster]) -> impl Iterator<Item = Hex> + '_ {
    clusters.iter().flat_map(Cluster::tiles)
}

#[derive(Clone)]
pub(super) struct City {
    pub id: u32,
    pub team: Team,
    pub pos: Hex,
    pub population: usize,
    pub barracks_hp: f32,
    pub coastal_battery_hp: f32,
    /// The tiles its citizens work, as up to `MAX_MANAGERS` clusters (a
    /// manager and its workers), first to last (`citizens.rs`).
    pub clusters: Vec<Cluster>,
    /// The clusters as last assigned (auto-assign, a click, a manager
    /// moved): tiles a blocked route or a claim displaced come back from
    /// here when they're free again, unless the player changes the
    /// assignment (`reconcile_citizens`).
    pub remembered: Vec<Cluster>,
    /// What auto-assign favors for its citizens (`Priorities`).
    pub priorities: Priorities,
    /// Units, workers, Grows and Gathers, each with whether it's paid and
    /// the work done on it (`Queued`). Each turn the city works the first
    /// item that's paid for or that the stockpile can pay for
    /// (`economy.rs`).
    pub queue: Vec<Queued<Build>>,
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
    /// The Barracks' own queue, independent of the city's main queue and
    /// worked the same way.
    pub barracks_queue: Vec<Queued<BuildUnit>>,
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
            barracks_hp: BARRACKS_MAX_HP,
            coastal_battery_hp: 150.0,
            clusters: Vec::new(),
            remembered: Vec::new(),
            priorities: Priorities::default(),
            queue: Vec::new(),
            built: Vec::new(),
            barracks: None,
            mill: None,
            workshop: None,
            extra_buildings: HashMap::default(),
            building_scroll: 0,
            worker_scroll: 0,
            barracks_queue: Vec::new(),
            interior: Interior::default(),
            workers: 1,
            held_workers: 0,
            worker_jobs: Vec::new(),
        }
    }

    /// Every tile its citizens work, cluster by cluster, each manager
    /// before its workers.
    pub fn worked(&self) -> impl Iterator<Item = Hex> + '_ {
        cluster_tiles(&self.clusters)
    }

    /// Whether one of its citizens works `hex`.
    pub fn works(&self, hex: Hex) -> bool {
        self.worked().any(|h| h == hex)
    }

    /// How many of its citizens work a tile.
    pub fn working(&self) -> usize {
        self.clusters.iter().map(Cluster::citizens).sum()
    }

    /// The cluster `hex` is worked in, if one is.
    pub fn cluster_of(&self, hex: Hex) -> Option<usize> {
        self.clusters
            .iter()
            .position(|c| c.tiles().any(|h| h == hex))
    }

    /// How many of its citizens can work: all of them, up to the cap.
    pub fn capacity(&self) -> usize {
        self.population.min(MAX_CITY_POPULATION)
    }

    /// How many managers it may have: one for each `CLUSTER_SIZE`
    /// citizens or part of that (citizens 1, 8, 15 and 22 start a cluster).
    pub fn managers_allowed(&self) -> usize {
        managers_for(self.population)
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
                // Every queue starts empty: the AI picks its builds as it
                // plans (`plan_ai_cities`).
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

/// Tests: one cluster of `tiles`, the first its manager.
#[cfg(test)]
pub(super) fn one_cluster(tiles: &[Hex]) -> Vec<Cluster> {
    tiles
        .split_first()
        .map(|(&manager, workers)| {
            vec![Cluster {
                manager,
                workers: workers.to_vec(),
            }]
        })
        .unwrap_or_default()
}
