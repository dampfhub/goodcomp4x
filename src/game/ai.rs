//! A deliberately minimal AI, playing every team but the player's.

use std::collections::{HashMap, HashSet, VecDeque};

use super::city::{Build, BuildUnit, Building, City};
use super::hex::Hex;
use super::unit::{Team, UnitType};
use super::workers::{JobKind, WorkerJob};
use super::{GameState, PLAYER_TEAM};

/// Units (scouts and settlers aside) the AI wants for each of its cities
/// before it spends on growth.
const AI_ARMY_PER_CITY: usize = 2;

impl GameState {
    /// The teams the AI plays: every team but the player's that still has a
    /// unit or a city, in `Team::ALL` order.
    pub(super) fn ai_teams(&self) -> Vec<Team> {
        Team::ALL
            .into_iter()
            .filter(|&team| team != PLAYER_TEAM)
            .filter(|&team| {
                self.units.iter().any(|u| u.team == team)
                    || self.cities.iter().any(|c| c.team == team)
            })
            .collect()
    }

    /// What `team`'s cities and Barracks start, paid from the side's
    /// stockpile (`city/economy.rs`); what the side can't pay for waits a
    /// turn. The Barracks is the military building (`city/barracks.rs`): an
    /// idle one trains Cavalry or Armored when its deposits allow and the
    /// side can pay, else Melee, or Ranged for one in three. A city's own
    /// queue trains a worker first if it has none left; its workers then build
    /// a Barracks (sited by `ai_barracks_site`, paid when placed), and the
    /// queue grows; a city without a Barracks
    /// trains Melee itself, slowly, until the side has `AI_ARMY_PER_CITY`
    /// units per city, falling back on growth, and gathering when it can't pay for anything.
    fn plan_ai_cities(&mut self, team: Team) {
        let soldiers = |game: &GameState, kind: Option<UnitType>| {
            game.units
                .iter()
                .filter(|u| {
                    u.team == team
                        && u.unit_type != UnitType::Scout
                        && !game.settlers.contains(&u.id)
                        && kind.is_none_or(|kind| u.unit_type == kind)
                })
                .count()
        };
        let mut army = soldiers(self, None);
        let cities: Vec<usize> = (0..self.cities.len())
            .filter(|&i| self.cities[i].team == team)
            .collect();
        for &city in &cities {
            self.place_ai_barracks(city);
            if self.cities[city].barracks.is_some() && self.cities[city].barracks_queue.is_empty() {
                let basic = if soldiers(self, Some(UnitType::Ranged)) * 2
                    < soldiers(self, Some(UnitType::Melee))
                {
                    BuildUnit::Ranged
                } else {
                    BuildUnit::Melee
                };
                for build in [BuildUnit::Cavalry, BuildUnit::Armored, basic] {
                    if self.barracks_lock(city, build).is_none()
                        && self.try_queue_barracks(city, build).is_ok()
                    {
                        army += 1;
                        break;
                    }
                }
            }
            let c = &self.cities[city];
            if !c.queue.is_empty() {
                continue;
            }
            let melee = Build::Unit(BuildUnit::Melee);
            let mut choices = if c.workers == 0 && self.workers_out(city) == 0 {
                vec![Build::Worker]
            } else {
                Vec::new()
            };
            if c.barracks.is_none() && army < AI_ARMY_PER_CITY * cities.len() {
                choices.extend([melee, Build::Grow]);
            } else {
                choices.push(Build::Grow);
            }
            // With nothing it can pay for, it gathers.
            choices.push(Build::Gather);
            for build in choices {
                if build == Build::Grow && !self.can_grow(city) {
                    continue;
                }
                if self.try_queue_build(city, build).is_ok() {
                    army += usize::from(build == melee);
                    break;
                }
            }
        }
    }

    /// City `city` of the AI, with a worker and no Barracks built or placed,
    /// places one for its workers to build (`ai_barracks_site`), if its
    /// side can pay.
    fn place_ai_barracks(&mut self, city: usize) {
        let c = &self.cities[city];
        if c.barracks.is_some()
            || self.building_job_queued(city, Building::Barracks)
            || !self.has_workers(city)
        {
            return;
        }
        if let Some(site) = self.ai_barracks_site(city) {
            let job = WorkerJob::on_tile(site, JobKind::Build(Building::Barracks));
            let team = self.cities[city].team;
            if self.job_problem(city, job).is_none() && self.tile_job_at(team, site).is_none() {
                let _ = self.try_queue_job(city, job);
            }
        }
    }

    /// Where city `city` of the AI puts its Barracks: on a Horses or Iron
    /// deposit within 3 hexes, of a kind its side has none of yet if it can,
    /// else on the nearest open, unworked tile within 2; ties by distance,
    /// then hex coordinates.
    fn ai_barracks_site(&self, city: usize) -> Option<Hex> {
        let c = &self.cities[city];
        let team = c.team;
        let open = |hex: Hex| {
            self.grid.contains(hex)
                && self.ai_site_issue(city, Building::Barracks, hex).is_none()
                && self.enemy_of_team_at(hex, team).is_none()
        };
        let mut near: Vec<Hex> = self
            .grid
            .all_hexes()
            .filter(|h| (1..=3).contains(&h.distance(c.pos)) && open(*h))
            .collect();
        near.sort_by_key(|h| (h.distance(c.pos), h.q, h.r));
        let deposit = |hex: &Hex| self.grid.resource(*hex);
        let lacking = |hex: &Hex| {
            deposit(hex).is_some_and(|resource| self.side_deposits(team, resource).is_empty())
        };
        near.iter()
            .find(|h| lacking(h))
            .or_else(|| near.iter().find(|h| deposit(h).is_some()))
            .or_else(|| {
                near.iter().find(|h| {
                    h.distance(c.pos) <= 2 && !self.cities.iter().any(|o| o.worked.contains(h))
                })
            })
            .or_else(|| near.first())
            .copied()
    }

    /// Each unit closes on its nearest enemy, or on ruins nobody on its side
    /// holds (`ruins.rs`) if those are nearer: it attacks if already in
    /// range, otherwise moves as close as it can and attacks if that brings it
    /// into range; ruins it steps onto, and then holds until they're claimed.
    /// Units in a contested hex stay and fight. No coordination beyond not
    /// sending two units to the same hex, and no retreating.
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
            self.cities.push(City::new(id, team, unit.pos));
            self.auto_assign_city(self.cities.len() - 1);
        }
        self.plan_ai_cities(team);
        self.plan_ai_workers(team);
        for idx in 0..self.units.len() {
            if self.units[idx].team != team
                || self.rival_of(idx).is_some()
                || self.player_controlled_units.contains(&self.units[idx].id)
                || self.settlers.contains(&self.units[idx].id)
            {
                continue;
            }
            // A defender already holding a city gate should keep projecting its
            // interior copy while enemy troops contest the surrounding ring.
            let gate_under_attack = self.cities.iter().any(|city| {
                city.team == team
                    && self.units[idx].pos.distance(city.pos) == 1
                    && self
                        .units
                        .iter()
                        .any(|enemy| enemy.team != team && enemy.pos.distance(city.pos) == 1)
            });
            if gate_under_attack {
                let unit = &self.units[idx];
                let attack = self
                    .units
                    .iter()
                    .filter(|enemy| enemy.team != team)
                    .filter(|enemy| unit.pos.distance(enemy.pos) <= unit.stats().attack_range)
                    .min_by_key(|enemy| (unit.pos.distance(enemy.pos), enemy.id))
                    .map(|enemy| enemy.pos);
                self.units[idx].planned_attack = attack;
                continue;
            }
            // Holding ruins until they're claimed: stay, and fight from there.
            if self.ruin_at(self.units[idx].pos).is_some() {
                let unit = &self.units[idx];
                let attack = self
                    .units
                    .iter()
                    .filter(|enemy| enemy.team != team)
                    .filter(|enemy| unit.pos.distance(enemy.pos) <= unit.stats().attack_range)
                    .min_by_key(|enemy| (unit.pos.distance(enemy.pos), enemy.id))
                    .map(|enemy| enemy.pos);
                self.units[idx].planned_attack = attack;
                continue;
            }
            let Some(target) = self.nearest_ai_target(idx) else {
                continue;
            };

            let unit = &self.units[idx];
            let stats = unit.stats();
            // Workers and ruins are captured by moving onto them. Ships keep
            // their water-only movement domain while land units use roads and gates.
            let capture = self.enemy_of_team_at(target, team).is_none();
            let reachable = if unit.is_naval() {
                self.reachable_hexes_by(unit.pos, stats.move_range, |_, to| {
                    self.grid.contains(to)
                        && self.grid.terrain(to).is_water()
                        && !self.is_occupied(to)
                })
            } else {
                self.reachable_hexes(unit.pos, stats.move_range, team)
            };
            let dest = if capture && reachable.contains(&target) {
                target
            } else if !capture && unit.pos.distance(target) <= stats.attack_range {
                unit.pos
            } else {
                let to_target = (!unit.is_naval()).then(|| self.steps_to(target, team, &reachable));
                let steps_left = |hex: &Hex| {
                    to_target.as_ref().map_or_else(
                        || hex.distance(target),
                        |distances| distances.get(hex).copied().unwrap_or(i32::MAX),
                    )
                };
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

            if dest != self.units[idx].pos {
                self.units[idx].planned_move = Some(dest);
            }
            let ruins = self.ruin_at(target).is_some() && !self.is_occupied(target);
            let can_attack = self.units[idx].can_attack();
            let unit = &mut self.units[idx];
            if can_attack && !ruins && dest != target && dest.distance(target) <= stats.attack_range
            {
                unit.planned_attack = Some(target);
            }
        }
    }

    /// Where unit `idx` heads: the enemy unit or worker, or the ruins no ally
    /// holds or is already heading for, closest on foot (routing around
    /// terrain and walls; ties go to the lowest coordinates). With none
    /// reachable, the enemy nearest as the crow flies.
    fn nearest_ai_target(&self, idx: usize) -> Option<Hex> {
        let unit = &self.units[idx];
        let team = unit.team;
        let enemies: Vec<Hex> = self
            .units
            .iter()
            .filter(|other| other.team != team)
            .map(|other| other.pos)
            .chain(
                self.field_workers
                    .iter()
                    .filter(|worker| worker.team != team)
                    .map(|worker| worker.pos),
            )
            .collect();
        let open_ruins: Vec<Hex> = if unit.is_naval() {
            Vec::new()
        } else {
            self.ruins
                .iter()
                .map(|ruin| ruin.pos)
                .filter(|&pos| {
                    !self
                        .units
                        .iter()
                        .any(|u| u.team == team && (u.pos == pos || u.planned_move == Some(pos)))
                })
                .collect()
        };
        if unit.is_naval() {
            return enemies
                .into_iter()
                .min_by_key(|pos| (unit.pos.distance(*pos), pos.q, pos.r));
        }
        let is_target = |hex: &Hex| enemies.contains(hex) || open_ruins.contains(hex);
        // Search outward ring by ring, stopping at the first ring with a target.
        let mut seen = HashSet::from([unit.pos]);
        let mut ring = vec![unit.pos];
        while !ring.is_empty() {
            if let Some(&hex) = ring
                .iter()
                .filter(|hex| is_target(hex))
                .min_by_key(|hex| (hex.q, hex.r))
            {
                return Some(hex);
            }
            let mut next = Vec::new();
            for &hex in &ring {
                for neighbor in hex.neighbors() {
                    if self.can_step(hex, neighbor, team) && seen.insert(neighbor) {
                        next.push(neighbor);
                    }
                }
            }
            ring = next;
        }
        enemies
            .into_iter()
            .min_by_key(|pos| (unit.pos.distance(*pos), pos.q, pos.r))
    }

    /// Steps on foot from each of `wanted` to `target` for a unit of
    /// `team`, around impassable terrain, walls and others' gates but
    /// ignoring units, since those will have moved by the time anyone gets
    /// there. Searches outward from `target` only until every one of
    /// `wanted` is found; any it can't reach are left out.
    fn steps_to(&self, target: Hex, team: Team, wanted: &HashSet<Hex>) -> HashMap<Hex, i32> {
        let mut distances = HashMap::from([(target, 0)]);
        let mut queue = VecDeque::from([target]);
        let mut left = wanted.iter().filter(|hex| **hex != target).count();
        while let Some(hex) = queue.pop_front() {
            if left == 0 {
                break;
            }
            let next_distance = distances[&hex] + 1;
            for neighbor in hex.neighbors() {
                if self.can_step(hex, neighbor, team) && !distances.contains_key(&neighbor) {
                    distances.insert(neighbor, next_distance);
                    queue.push_back(neighbor);
                    if wanted.contains(&neighbor) {
                        left -= 1;
                    }
                }
            }
        }
        distances
    }
}
