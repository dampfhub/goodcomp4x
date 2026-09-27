//! Settlers and workers: founding cities, building roads and improving tiles.
use std::collections::HashMap;

use super::{BARRACKS_MAX_HP, CITY_MAX_HP, City, LaborFocus, Site};
use crate::game::terrain::Terrain;
use crate::game::{GameState, PLAYER_TEAM};

impl GameState {
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
}
