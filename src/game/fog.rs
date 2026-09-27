//! Fog of war for the player's side, in three layers:
//! - in sight: hexes the player's units, cities and barracks see now, shown
//!   as they are;
//! - remembered: hexes seen before but out of sight now, shown under a grey
//!   veil as they were when last seen (`Sighting`): enemy units, cities,
//!   barracks, improvements and roads;
//! - unexplored: never seen, blank.
//!
//! A debug setting (F10) turns the fog off. The AI ignores it.

use std::collections::{HashMap, HashSet};

use super::city::{BARRACKS_MAX_HP, Building, CITY_MAX_HP};
use super::draw::UnitLook;
use super::hex::Hex;
use super::terrain::Terrain;
use super::unit::{Team, Unit};
use super::{GameState, PLAYER_TEAM};

/// How far a city sees, and a barracks.
const CITY_SIGHT: i32 = 3;
const BARRACKS_SIGHT: i32 = 1;
/// Extra sight for a unit standing on hills.
const HILLS_SIGHT: i32 = 1;
/// Extra sight for a scout after a turn on lookout.
pub(super) const LOOKOUT_SIGHT: i32 = 2;
/// How far a line of sight is shifted to settle which side of an edge it
/// passes (see `Hex::line_between`).
const SIGHT_NUDGE: f32 = 1e-4;

/// What the player can see this frame.
pub(super) struct Fog {
    /// Hexes in sight, or `None` with the fog turned off.
    visible: Option<HashSet<Hex>>,
}

impl Fog {
    /// Whether `hex` is in sight (always, with the fog off).
    pub fn sees(&self, hex: Hex) -> bool {
        self.visible.as_ref().is_none_or(|v| v.contains(&hex))
    }

    /// Whether to show `unit`: the player's own always, others in sight.
    pub fn shows(&self, unit: &Unit) -> bool {
        unit.team == PLAYER_TEAM || self.sees(unit.pos)
    }
}

/// A hex as the player last saw it. Only what can change is kept; the
/// ground itself never does.
#[derive(Clone, Default)]
pub(super) struct Sighting {
    /// Other sides' units there, each with how it's drawn. The player's
    /// own are always shown where they really are.
    pub units: Vec<(Unit, UnitLook)>,
    pub city: Option<SeenBuilding>,
    pub barracks: Option<SeenBuilding>,
    /// An improvement's label and owner.
    pub site: Option<(&'static str, Team)>,
    pub road: bool,
}

#[derive(Clone, Copy)]
pub(super) struct SeenBuilding {
    pub team: Team,
    /// The city's number, from 0.
    pub id: u32,
    /// Share of full health, 0 to 1.
    pub health: f32,
    /// For a city: its population, and whether it has a granary.
    pub population: usize,
    pub granary: bool,
}

impl GameState {
    pub(super) fn fog(&self) -> Fog {
        Fog {
            visible: self.fog_of_war.then(|| self.visible_hexes()),
        }
    }

    /// Whether the player has ever seen `hex` (always, with the fog off).
    pub(super) fn is_explored(&self, hex: Hex) -> bool {
        !self.fog_of_war || self.memory.contains_key(&hex)
    }

    /// How `hex` looked when last seen, if it ever was.
    pub(super) fn remembered(&self, hex: Hex) -> Option<&Sighting> {
        self.memory.get(&hex)
    }

    /// How far `unit` sees: its type's sight, more from hills or after a
    /// turn on lookout.
    pub(super) fn sight(&self, unit: &Unit) -> i32 {
        let hills = if self.grid.tile(unit.pos).hills {
            HILLS_SIGHT
        } else {
            0
        };
        let lookout = if unit.lookout { LOOKOUT_SIGHT } else { 0 };
        unit.unit_type.sight() + hills + lookout
    }

    /// Whether `from` can see `to`: no mountain stands on the line between
    /// them. A mountain itself can be seen, just not past. Where the line
    /// runs along the edge between two hexes, either side being clear will do.
    fn in_line_of_sight(&self, from: Hex, to: Hex) -> bool {
        [SIGHT_NUDGE, -SIGHT_NUDGE].into_iter().any(|nudge| {
            from.line_between(to, nudge)
                .all(|h| self.grid.terrain(h) != Terrain::Mountains)
        })
    }

    /// Every hex the player's units, cities and barracks can see now.
    fn visible_hexes(&self) -> HashSet<Hex> {
        let mut seen = HashSet::new();
        let mut look = |from: Hex, range: i32| {
            for dq in -range..=range {
                for dr in (-range).max(-dq - range)..=range.min(-dq + range) {
                    let hex = Hex::new(from.q + dq, from.r + dr);
                    if self.grid.contains(hex) && self.in_line_of_sight(from, hex) {
                        seen.insert(hex);
                    }
                }
            }
        };
        for (idx, unit) in self.units.iter().enumerate() {
            if self.is_player_controlled(idx) {
                look(unit.pos, self.sight(unit));
            }
        }
        for city in self.cities.iter().filter(|c| c.team == PLAYER_TEAM) {
            look(city.pos, CITY_SIGHT);
            if let Some(barracks) = city.barracks {
                look(barracks, BARRACKS_SIGHT);
            }
        }
        seen
    }

    /// `hex` as it is right now, for the memory.
    fn sighting(&self, hex: Hex) -> Sighting {
        let units = self
            .units_at(hex)
            .filter(|&i| !self.is_player_controlled(i))
            .map(|i| (self.units[i].clone(), self.unit_look(&self.units[i])))
            .collect();
        let city = self
            .cities
            .iter()
            .find(|c| c.pos == hex)
            .map(|c| SeenBuilding {
                team: c.team,
                id: c.id,
                health: c.hp / CITY_MAX_HP,
                population: c.population,
                granary: c.built.contains(&Building::Granary),
            });
        let barracks = self
            .cities
            .iter()
            .find(|c| c.barracks == Some(hex))
            .map(|c| SeenBuilding {
                team: c.team,
                id: c.id,
                health: c.barracks_hp / BARRACKS_MAX_HP,
                population: 0,
                granary: false,
            });
        Sighting {
            units,
            city,
            barracks,
            site: self.sites.get(&hex).map(|s| (s.label, s.team)),
            road: self.roads.contains(&hex),
        }
    }

    /// Records everything in sight as the player's latest memory of it.
    /// Called every frame.
    pub(super) fn explore(&mut self) {
        if !self.fog_of_war {
            return;
        }
        for hex in self.visible_hexes() {
            let sighting = self.sighting(hex);
            self.memory.insert(hex, sighting);
        }
    }

    /// F10: turns the fog of war on or off.
    pub fn toggle_fog(&mut self) {
        self.fog_of_war = !self.fog_of_war;
        self.explore();
        self.notice = if self.fog_of_war {
            "FOG OF WAR ON - F10 TO LIFT IT".into()
        } else {
            "FOG OF WAR OFF - F10 TO BRING IT BACK".into()
        };
    }
}

/// The player's memory of the map: every hex ever seen, as last seen.
pub(super) type Memory = HashMap<Hex, Sighting>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::unit::{Team, UnitType};

    #[test]
    fn units_see_around_them_and_explored_hexes_stay_explored() {
        let mut game = GameState::world_scenario(3);
        game.explore();
        let scout = game
            .units
            .iter()
            .find(|u| u.team == Team::Blue && u.unit_type == UnitType::Scout)
            .unwrap();
        let (start, sight) = (scout.pos, game.sight(scout));
        assert!(sight >= 3, "scouts see farther");
        let far = Hex::new(start.q + sight, start.r);
        let fog = game.fog();
        assert!(fog.sees(start) && game.is_explored(start));
        assert!(fog.sees(far) || !game.grid.contains(far));
        let distant = game
            .grid
            .all_hexes()
            .find(|h| {
                game.units
                    .iter()
                    .all(|u| u.team != Team::Blue || u.pos.distance(*h) > 8)
            })
            .unwrap();
        assert!(!fog.sees(distant) && !game.is_explored(distant));

        // Every Blue unit leaves; what they saw stays explored but unseen.
        for unit in game.units.iter_mut().filter(|u| u.team == Team::Blue) {
            unit.pos = distant;
        }
        game.explore();
        assert!(game.is_explored(start));
        assert!(!game.fog().sees(start));

        game.toggle_fog();
        assert!(game.fog().sees(start) && game.is_explored(Hex::new(0, 0)));
    }

    #[test]
    fn enemies_out_of_sight_are_hidden() {
        let game = GameState::world_scenario(3);
        let fog = game.fog();
        let red = game.units.iter().find(|u| u.team == Team::Red).unwrap();
        assert!(!fog.shows(red), "Red starts far out of sight");
        let blue = game.units.iter().find(|u| u.team == Team::Blue).unwrap();
        assert!(fog.shows(blue));
    }

    #[test]
    fn remembered_hexes_keep_what_was_last_seen() {
        let mut game = GameState::world_scenario(3);
        let settler = game.units[0].pos;
        let near = settler
            .neighbors()
            .into_iter()
            .find(|h| game.grid.is_passable(*h) && !game.is_occupied(*h))
            .unwrap();
        // A Red scout wanders into sight next to Blue's settler.
        let red = game
            .units
            .iter()
            .position(|u| u.team == Team::Red && u.unit_type == UnitType::Scout)
            .unwrap();
        game.units[red].pos = near;
        game.explore();
        assert_eq!(game.remembered(near).unwrap().units.len(), 1);

        // Every Blue unit leaves, and the scout moves on unseen.
        let far = game.units.iter().find(|u| u.team == Team::Red).unwrap().pos;
        for unit in game.units.iter_mut().filter(|u| u.team == Team::Blue) {
            unit.pos = far;
        }
        game.units[red].pos = Hex::new(far.q, far.r + 1);
        game.explore();
        assert!(!game.fog().sees(near));
        let seen = game.remembered(near).unwrap();
        assert_eq!(seen.units.len(), 1, "still remembered where it was");
        assert_eq!(seen.units[0].0.team, Team::Red);

        // Coming back into sight updates the memory.
        for unit in game.units.iter_mut().filter(|u| u.team == Team::Blue) {
            unit.pos = settler;
        }
        game.explore();
        assert!(game.remembered(near).unwrap().units.is_empty());
    }

    #[test]
    fn the_players_own_units_are_never_remembered() {
        let mut game = GameState::world_scenario(3);
        game.explore();
        let blue = game.units.iter().find(|u| u.team == Team::Blue).unwrap();
        assert!(game.remembered(blue.pos).unwrap().units.is_empty());
    }

    #[test]
    fn mountains_block_sight_beyond_them() {
        use crate::game::hex::HexGrid;
        use crate::game::terrain::Tile;
        let mut game = GameState::new();
        game.units.clear();
        game.grid = HexGrid::new(4, [(Hex::new(1, 0), Tile::MOUNTAINS)]);
        game.units
            .push(Unit::new(0, Hex::new(0, 0), Team::Blue, UnitType::Scout));
        let fog = game.fog();
        assert!(fog.sees(Hex::new(1, 0)), "the mountain itself shows");
        assert!(!fog.sees(Hex::new(2, 0)) && !fog.sees(Hex::new(3, 0)));
        // A line along the mountain's edge still passes on its clear side.
        assert!(fog.sees(Hex::new(2, -1)));
        assert!(fog.sees(Hex::new(-3, 0)), "other directions are open");
    }

    #[test]
    fn every_world_starts_its_scouts_on_hills() {
        for seed in 0..6 {
            let game = GameState::world_scenario(seed);
            for unit in &game.units {
                let hills = game.grid.tile(unit.pos).hills;
                let scout = unit.unit_type == UnitType::Scout;
                assert_eq!(hills, scout, "seed {seed}: {unit}");
            }
        }
    }
}
