//! Settlers: founding cities. Workers live with their cities (`workers.rs`).
use super::City;
use crate::game::GameState;
use crate::game::hex::Hex;
use crate::game::unit::Team;

/// How close a new city may be to any other, of any side: at least this
/// many hexes apart.
pub(in crate::game) const MIN_CITY_DISTANCE: i32 = 6;

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
        if unit.team != self.local_team || !self.settlers.contains(&unit.id) {
            self.notice = "ONLY A SETTLER CAN FOUND A CITY".into();
            return;
        }
        let (id, team, pos) = (unit.id, unit.team, unit.pos);
        if let Some(why) = self.founding_issue(pos) {
            self.notice = why;
            return;
        }
        self.selected = None;
        let city = self.found_city(id, team, pos);
        self.selected_city = Some(city);
        self.notice = "CITY FOUNDED - CHOOSE WHAT IT BUILDS".into();
    }

    /// Why no city can be founded on `hex`, if none can: only on passable
    /// land, not water, ruins or a den, and at least `MIN_CITY_DISTANCE` hexes
    /// from every other city, whoever's. A settler stands on its hex, so
    /// its side knows the ground; a city it hasn't seen still counts.
    pub(in crate::game) fn founding_issue(&self, hex: Hex) -> Option<String> {
        if !self.grid.contains(hex)
            || !self.grid.is_passable(hex)
            || self.grid.terrain(hex).is_water()
        {
            return Some("A CITY STANDS ON OPEN LAND".into());
        }
        if self.ruin_at(hex).is_some() {
            return Some("NOT ON RUINS: CLAIM THEM FIRST".into());
        }
        if self.den_at(hex).is_some() {
            return Some("NOT ON A DEN: CLEAR IT FIRST".into());
        }
        if self
            .cities
            .iter()
            .any(|c| c.pos.distance(hex) < MIN_CITY_DISTANCE)
        {
            return Some(format!(
                "TOO CLOSE TO ANOTHER CITY - CITIES STAND {MIN_CITY_DISTANCE} HEXES APART"
            ));
        }
        None
    }

    /// Settler `settler` of `team` founds a city on `pos`, where it stands,
    /// and is gone. The city starts at population 1 and assigns its
    /// citizen. Its side's first city comes with a worker, as a starting
    /// city does; any other starts without one. Returns the city's index.
    pub(in crate::game) fn found_city(&mut self, settler: u32, team: Team, pos: Hex) -> usize {
        let first = !self.cities.iter().any(|c| c.team == team);
        if let Some(index) = self.units.iter().position(|u| u.id == settler) {
            self.units.remove(index);
        }
        self.settlers.remove(&settler);
        let id = self.cities.len() as u32;
        self.cities.push(City {
            workers: u32::from(first),
            ..City::new(id, team, pos)
        });
        let city = self.cities.len() - 1;
        self.auto_assign_city(city);
        log::info!("{team:?} founded city {} at {pos:?}", id + 1);
        city
    }
}
