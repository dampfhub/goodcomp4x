//! A deliberately minimal AI opponent.

use std::collections::{HashMap, VecDeque};

use super::GameState;
use super::city::{Build, BuildUnit, City};
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
            self.cities.push(City {
                queue: vec![Build::Unit(BuildUnit::Melee)],
                ..City::new(id, team, unit.pos)
            });
            self.auto_assign_city(self.cities.len() - 1);
        }
        // A city that has lost every worker trains a new one first.
        for city in 0..self.cities.len() {
            let c = &self.cities[city];
            if c.team == team
                && c.workers == 0
                && self.workers_out(city) == 0
                && !c.queue.contains(&Build::Worker)
            {
                self.cities[city].queue.insert(0, Build::Worker);
            }
        }
        self.plan_ai_workers(team);
        for idx in 0..self.units.len() {
            if self.units[idx].team != team
                || self.rival_of(idx).is_some()
                || self.player_controlled_units.contains(&self.units[idx].id)
                || self.settlers.contains(&self.units[idx].id)
            {
                continue;
            }
            let Some(target) = self.nearest_enemy_pos(idx) else {
                continue;
            };

            let unit = &self.units[idx];
            let stats = unit.stats();
            // An enemy worker is caught by stepping onto it, when that's in
            // reach this turn.
            let worker = self.enemy_of_team_at(target, team).is_none();
            let reachable = self.reachable_hexes(unit.pos, stats.move_range, team);
            let dest = if worker && reachable.contains(&target) {
                target
            } else if unit.pos.distance(target) <= stats.attack_range {
                unit.pos
            } else {
                let to_target = self.walking_distances(target, team);
                let steps_left = |hex: &Hex| to_target.get(hex).copied().unwrap_or(i32::MAX);
                let claimed_by_ally = |hex: &Hex| {
                    self.units
                        .iter()
                        .any(|u| u.team == team && u.planned_move == Some(*hex))
                };
                reachable
                    .into_iter()
                    .filter(|hex| !claimed_by_ally(hex))
                    .min_by_key(|hex| (steps_left(hex), hex.q, hex.r))
                    .unwrap_or(unit.pos)
            };

            let unit = &mut self.units[idx];
            if dest != unit.pos {
                unit.planned_move = Some(dest);
            }
            if dest != target && dest.distance(target) <= stats.attack_range {
                unit.planned_attack = Some(target);
            }
        }
    }

    /// Position of the enemy unit or worker closest on foot, routing around
    /// terrain and walls.
    fn nearest_enemy_pos(&self, idx: usize) -> Option<Hex> {
        let unit = &self.units[idx];
        let from_unit = self.walking_distances(unit.pos, unit.team);
        self.units
            .iter()
            .filter(|other| other.team != unit.team)
            .map(|other| other.pos)
            .chain(
                self.field_workers
                    .iter()
                    .filter(|worker| worker.team != unit.team)
                    .map(|worker| worker.pos),
            )
            .min_by_key(|pos| {
                (
                    from_unit.get(pos).copied().unwrap_or(i32::MAX),
                    pos.q,
                    pos.r,
                )
            })
    }

    /// Steps from `origin` to every hex a unit of `team` can walk to, around
    /// impassable terrain, walls and others' gates but ignoring units, since
    /// those will have moved by the time anyone gets there.
    fn walking_distances(&self, origin: Hex, team: Team) -> HashMap<Hex, i32> {
        let mut distances = HashMap::from([(origin, 0)]);
        let mut queue = VecDeque::from([origin]);
        while let Some(hex) = queue.pop_front() {
            let next_distance = distances[&hex] + 1;
            for neighbor in hex.neighbors() {
                if self.can_step(hex, neighbor, team) && !distances.contains_key(&neighbor) {
                    distances.insert(neighbor, next_distance);
                    queue.push_back(neighbor);
                }
            }
        }
        distances
    }
}
