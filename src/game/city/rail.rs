//! A city center is an implicit rail terminal. A remote Railhead connected by
//! an unbroken road corridor receives one-turn moves from the city's ring.
use std::collections::{HashSet, VecDeque};

use super::Building;
use crate::game::GameState;
use crate::game::fog::Fog;
use crate::game::hex::Hex;
use crate::game::unit::Team;

impl GameState {
    pub(in crate::game) fn rail_transfer_available(
        &self,
        from: Hex,
        dest: Hex,
        team: Team,
        known: Option<&Fog>,
    ) -> bool {
        self.cities.iter().enumerate().any(|(city_id, city)| {
            city.team == team
                && city.pos.distance(from) <= 1
                && city.placed_site(Building::Railhead) == Some(dest)
                && self.rail_connected(city_id, known)
        })
    }

    pub(in crate::game) fn rail_connected(&self, city: usize, known: Option<&Fog>) -> bool {
        let owner = &self.cities[city];
        let Some(terminal) = owner.placed_site(Building::Railhead) else {
            return false;
        };
        let team = owner.team;
        let mut visited = HashSet::from([owner.pos]);
        let mut queue = VecDeque::from([owner.pos]);
        while let Some(from) = queue.pop_front() {
            if from == terminal {
                return true;
            }
            for to in from.neighbors() {
                if !self.grid.is_passable(to) || visited.contains(&to) {
                    continue;
                }
                let on_line = to == terminal
                    || known.map_or_else(
                        || self.is_road_hex(to),
                        |fog| {
                            if fog.sees(to) {
                                self.is_road_hex(to)
                            } else {
                                self.remembered(to).is_some_and(|seen| {
                                    seen.road || seen.city.is_some_and(|c| c.team == team)
                                })
                            }
                        },
                    );
                if !on_line {
                    continue;
                }
                let crossable = known.map_or_else(
                    || self.can_cross(from, to, team),
                    |fog| self.known_can_cross(from, to, team, fog),
                );
                let blocked = known.map_or_else(
                    || {
                        self.enemy_of_team_at(to, team).is_some()
                            || self
                                .cities
                                .iter()
                                .any(|city| city.pos == to && city.team != team)
                    },
                    |fog| {
                        if fog.sees(to) {
                            self.enemy_of_team_at(to, team).is_some()
                                || self
                                    .cities
                                    .iter()
                                    .any(|city| city.pos == to && city.team != team)
                        } else {
                            self.remembered(to)
                                .is_some_and(|seen| seen.city.is_some_and(|c| c.team != team))
                        }
                    },
                );
                if crossable && !blocked && visited.insert(to) {
                    queue.push_back(to);
                }
            }
        }
        false
    }
}
