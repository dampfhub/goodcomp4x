//! A deliberately minimal AI opponent.

use std::collections::{HashMap, VecDeque};

use super::GameState;
use super::hex::Hex;
use super::unit::Team;

impl GameState {
    /// Each unit closes on its nearest enemy: it attacks if already in range,
    /// otherwise moves as close as it can and attacks if that brings it into
    /// range. Units in a contested hex stay and fight. No coordination beyond
    /// not sending two units to the same hex, and no retreating.
    pub(super) fn plan_ai_turn(&mut self, team: Team) {
        // The computer founds its first city immediately, before combat orders.
        if self.cities.iter().all(|c| c.team != team)
            && let Some(i) = self
                .units
                .iter()
                .position(|u| u.team == team && self.settlers.contains(&u.id))
        {
            let unit = self.units.remove(i);
            self.settlers.remove(&unit.id);
            let id = self.cities.len() as u32;
            self.cities.push(super::city::City {
                id,
                team,
                pos: unit.pos,
                population: 1,
                food: 0,
                production: 0,
                hp: super::city::CITY_MAX_HP,
                barracks_hp: super::city::BARRACKS_MAX_HP,
                worked: Vec::new(),
                remembered_worked: Vec::new(),
                focus: super::city::LaborFocus::Balanced,
                queue: vec![super::city::Build::Unit(super::city::BuildUnit::Melee)],
                built: Vec::new(),
                barracks: None,
                mill: None,
                workshop: None,
                pending_building: None,
                planned_sites: std::collections::HashMap::new(),
                barracks_queue: Vec::new(),
                barracks_production: 0,
            });
            self.auto_assign_city(self.cities.len() - 1);
        }
        for idx in 0..self.units.len() {
            if self.units[idx].team != team
                || self.rival_of(idx).is_some()
                || self.player_controlled_units.contains(&self.units[idx].id)
                || self.settlers.contains(&self.units[idx].id)
                || self.workers.contains(&self.units[idx].id)
            {
                continue;
            }
            let Some(target) = self.nearest_enemy_pos(idx) else {
                continue;
            };

            let unit = &self.units[idx];
            let stats = unit.stats();
            let dest = if unit.pos.distance(target) <= stats.attack_range {
                unit.pos
            } else {
                let to_target = self.walking_distances(target);
                let steps_left = |hex: &Hex| to_target.get(hex).copied().unwrap_or(i32::MAX);
                let claimed_by_ally = |hex: &Hex| {
                    self.units
                        .iter()
                        .any(|u| u.team == team && u.planned_move == Some(*hex))
                };
                self.reachable_hexes(unit.pos, stats.move_range)
                    .into_iter()
                    .filter(|hex| !claimed_by_ally(hex))
                    .min_by_key(|hex| (steps_left(hex), hex.q, hex.r))
                    .unwrap_or(unit.pos)
            };

            let unit = &mut self.units[idx];
            if dest != unit.pos {
                unit.planned_move = Some(dest);
            }
            if dest.distance(target) <= stats.attack_range {
                unit.planned_attack = Some(target);
            }
        }
    }

    /// Position of the enemy closest on foot, routing around terrain.
    fn nearest_enemy_pos(&self, idx: usize) -> Option<Hex> {
        let unit = &self.units[idx];
        let from_unit = self.walking_distances(unit.pos);
        self.units
            .iter()
            .filter(|other| other.team != unit.team)
            .map(|other| other.pos)
            .min_by_key(|pos| from_unit.get(pos).copied().unwrap_or(i32::MAX))
    }

    /// Steps from `origin` to every hex it can walk to, around impassable
    /// terrain but ignoring units, since those will have moved by the time
    /// anyone gets there.
    fn walking_distances(&self, origin: Hex) -> HashMap<Hex, i32> {
        let mut distances = HashMap::from([(origin, 0)]);
        let mut queue = VecDeque::from([origin]);
        while let Some(hex) = queue.pop_front() {
            let next_distance = distances[&hex] + 1;
            for neighbor in hex.neighbors() {
                if self.grid.is_passable(neighbor) && !distances.contains_key(&neighbor) {
                    distances.insert(neighbor, next_distance);
                    queue.push_back(neighbor);
                }
            }
        }
        distances
    }
}
