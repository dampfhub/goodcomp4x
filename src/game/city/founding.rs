//! Settlers: founding cities. Workers live with their cities (`workers.rs`).
use super::City;
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
        self.cities.push(City::new(id, unit.team, pos));
        self.settlers.remove(&unit.id);
        self.units.remove(index);
        self.selected = None;
        let city = self.cities.len() - 1;
        self.auto_assign_city(city);
        self.selected_city = Some(city);
        self.notice = "CITY FOUNDED - PRESS 1-4 TO BUILD A UNIT".into();
    }
}
