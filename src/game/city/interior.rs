//! A small tactical board inside each city. Field units on the six neighboring
//! hexes project independent fighters through the corresponding gates.

use std::collections::HashSet;

use super::CITY_MAX_HP;
use crate::game::hex::Hex;
use crate::game::unit::{Team, UnitType};
use crate::game::{Camera, GameState, PLAYER_TEAM, combat};

pub(in crate::game) const CORE_HP: f32 = 80.0;
const CORE_DEFENSE: f32 = 18.0;
const CORE_ATTACK: f32 = 12.0;
const CENTER: Hex = Hex::new(0, 0);

#[derive(Clone)]
pub(in crate::game) struct Interior {
    pub core_hp: f32,
    pub fighters: Vec<InteriorFighter>,
    /// A defeated copy stays defeated while its field unit holds that gate.
    fallen_sources: HashSet<u32>,
}

impl Default for Interior {
    fn default() -> Self {
        Self {
            core_hp: CORE_HP,
            fighters: Vec::new(),
            fallen_sources: HashSet::new(),
        }
    }
}

#[derive(Clone)]
pub(in crate::game) struct InteriorFighter {
    pub source_id: u32,
    pub team: Team,
    pub unit_type: UnitType,
    pub pos: Hex,
    pub hp: f32,
    pub planned_move: Option<Hex>,
    pub planned_attack: Option<Hex>,
}

pub(super) fn in_bounds(hex: Hex) -> bool {
    hex.distance(CENTER) <= 2
}

impl GameState {
    pub fn is_in_city_interior(&self) -> bool {
        self.interior_view.is_some()
    }

    pub fn clear_selected_interior_orders(&mut self) {
        let (Some(city), Some(source)) = (self.interior_view, self.interior_selected) else {
            return;
        };
        if let Some(fighter) = self.cities[city]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == source && f.team == PLAYER_TEAM)
        {
            fighter.planned_move = None;
            fighter.planned_attack = None;
            self.notice = "INTERIOR ORDERS CLEARED".into();
        }
    }

    /// V opens the city under the pointer, the selected city, or the closest
    /// city to the selected field unit. The interior is a separate order view.
    pub fn toggle_city_interior(&mut self) {
        if self.is_resolving() {
            return;
        }
        if self.interior_view.is_some() {
            self.close_city_interior();
            return;
        }
        let fog = self.fog();
        let city = self
            .selected_city
            .or_else(|| {
                self.hovered_tile.and_then(|hex| {
                    self.cities
                        .iter()
                        .position(|c| c.pos == hex && (c.team == PLAYER_TEAM || fog.sees(hex)))
                })
            })
            .or_else(|| {
                self.selected.and_then(|unit| {
                    self.cities
                        .iter()
                        .enumerate()
                        .filter(|(_, city)| city.team == PLAYER_TEAM || fog.sees(city.pos))
                        .min_by_key(|(_, city)| (city.pos.distance(self.units[unit].pos), city.id))
                        .map(|(i, _)| i)
                })
            });
        if let Some(city) = city {
            self.open_city_interior(city);
        } else {
            self.notice = "HOVER A CITY OR SELECT A UNIT, THEN PRESS V".into();
        }
    }

    pub(in crate::game) fn open_city_interior(&mut self, city: usize) {
        if self.interior_view.is_none() {
            self.exterior_camera = Some(self.camera.clone());
        }
        self.sync_city_interiors();
        self.interior_view = Some(city);
        self.interior_selected = None;
        self.selected_city = None;
        self.selected_barracks = None;
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        // Leave room for the command panel on the left and Debug on the right.
        self.camera = Camera::new(glam::Vec2::new(-1.35, 0.0), 6.0);
        self.notice = "CITY INTERIOR: CLICK A BLUE TROOP, THEN A TILE OR ENEMY; ESC RETURNS".into();
    }

    pub(in crate::game) fn close_city_interior(&mut self) {
        let Some(city) = self.interior_view.take() else {
            return;
        };
        if let Some(camera) = self.exterior_camera.take() {
            self.camera = camera;
        }
        self.interior_selected = None;
        self.selected_city = (self.cities[city].team == PLAYER_TEAM).then_some(city);
        self.hovered_tile = None;
        self.hovered_city = None;
        self.notice = if self.selected_city.is_some() {
            "CITY VIEW: CLICK THE CITY CENTER TO RE-ENTER THE INTERIOR".into()
        } else {
            "EXTERIOR MAP: HOVER THE CITY AND PRESS V TO RE-ENTER".into()
        };
    }

    /// Clicking a tile in the interior view selects a copy or gives it an
    /// independent tactical order. The source field unit is unchanged.
    pub(in crate::game) fn interior_click(&mut self, tile: Hex) {
        let Some(city) = self.interior_view else {
            return;
        };
        if self.is_resolving() || !in_bounds(tile) {
            return;
        }
        let interior = &self.cities[city].interior;
        let clicked = interior.fighters.iter().find(|f| f.pos == tile);
        if let Some(fighter) = clicked.filter(|f| f.team == PLAYER_TEAM) {
            self.interior_selected =
                (self.interior_selected != Some(fighter.source_id)).then_some(fighter.source_id);
            return;
        }
        let Some(source) = self.interior_selected else {
            self.notice = "SELECT A BLUE INTERIOR UNIT FIRST".into();
            return;
        };
        let Some(fighter) = interior.fighters.iter().find(|f| f.source_id == source) else {
            self.interior_selected = None;
            return;
        };
        let from = fighter.planned_move.unwrap_or(fighter.pos);
        let enemy = clicked.is_some_and(|f| f.team != PLAYER_TEAM);
        let core = tile == CENTER
            && self.cities[city].team != PLAYER_TEAM
            && self.cities[city].interior.core_hp > 0.0;
        if enemy || core {
            if from.distance(tile) <= fighter.unit_type.stats().attack_range {
                let fighter = self.cities[city]
                    .interior
                    .fighters
                    .iter_mut()
                    .find(|f| f.source_id == source)
                    .unwrap();
                fighter.planned_attack = (fighter.planned_attack != Some(tile)).then_some(tile);
                self.notice = "INTERIOR ATTACK QUEUED".into();
            } else {
                self.notice = "TARGET OUT OF RANGE: MOVE THE COPY CLOSER FIRST".into();
            }
        } else if clicked.is_none()
            && (tile != CENTER || self.cities[city].interior.core_hp <= 0.0)
            && from.distance(tile) <= fighter.unit_type.stats().move_range.max(1)
        {
            let fighter = self.cities[city]
                .interior
                .fighters
                .iter_mut()
                .find(|f| f.source_id == source)
                .unwrap();
            fighter.planned_move = (fighter.planned_move != Some(tile)).then_some(tile);
            fighter.planned_attack = None;
            self.notice = "INTERIOR MOVE QUEUED".into();
        } else {
            self.notice = "CHOOSE AN OPEN TILE IN MOVE RANGE, OR AN ENEMY IN ATTACK RANGE".into();
        }
    }

    pub(in crate::game) fn sync_city_interiors(&mut self) {
        for city in &mut self.cities {
            let adjacent: Vec<_> = self
                .units
                .iter()
                .filter(|unit| {
                    unit.pos.distance(city.pos) == 1
                        && !self.workers.contains(&unit.id)
                        && !self.settlers.contains(&unit.id)
                })
                .collect();
            let ids: HashSet<_> = adjacent.iter().map(|unit| unit.id).collect();
            city.interior
                .fighters
                .retain(|f| ids.contains(&f.source_id));
            city.interior.fallen_sources.retain(|id| ids.contains(id));
            for unit in adjacent {
                if city.interior.fallen_sources.contains(&unit.id)
                    || city
                        .interior
                        .fighters
                        .iter()
                        .any(|f| f.source_id == unit.id)
                {
                    continue;
                }
                let gate = Hex::new((unit.pos.q - city.pos.q) * 2, (unit.pos.r - city.pos.r) * 2);
                let mut open: Vec<_> = (-2..=2)
                    .flat_map(|q| (-2..=2).map(move |r| Hex::new(q, r)))
                    .filter(|&h| in_bounds(h) && h != CENTER)
                    .filter(|h| !city.interior.fighters.iter().any(|f| f.pos == *h))
                    .collect();
                open.sort_by_key(|h| (h.distance(gate), h.q, h.r));
                if let Some(pos) = open.first().copied() {
                    city.interior.fighters.push(InteriorFighter {
                        source_id: unit.id,
                        team: unit.team,
                        unit_type: unit.unit_type,
                        pos,
                        hp: unit.unit_type.stats().max_hp,
                        planned_move: None,
                        planned_attack: None,
                    });
                }
            }
            city.interior.fighters.sort_by_key(|f| f.source_id);
        }
    }

    /// Interior orders resolve once per global turn, after the field battle.
    /// Moves and damage are simultaneous within the interior layer.
    pub(in crate::game) fn resolve_city_interiors(&mut self) {
        self.sync_city_interiors();
        for city in 0..self.cities.len() {
            self.plan_interior_ai(city);
            self.resolve_one_interior(city);
        }
        if let Some(source) = self.interior_selected
            && !self
                .cities
                .iter()
                .any(|city| city.interior.fighters.iter().any(|f| f.source_id == source))
        {
            self.interior_selected = None;
        }
    }

    fn plan_interior_ai(&mut self, city: usize) {
        let owner = self.cities[city].team;
        let core_breached = self.cities[city].interior.core_hp <= 0.0;
        let snapshot = self.cities[city].interior.fighters.clone();
        for fighter in &mut self.cities[city].interior.fighters {
            if fighter.team == PLAYER_TEAM {
                continue;
            }
            let target = if fighter.team != owner {
                Some(CENTER)
            } else {
                snapshot
                    .iter()
                    .filter(|other| other.team != fighter.team)
                    .min_by_key(|other| (fighter.pos.distance(other.pos), other.source_id))
                    .map(|other| other.pos)
            };
            let Some(target) = target else { continue };
            let range = fighter.unit_type.stats().attack_range;
            if fighter.pos.distance(target) <= range
                && !(target == CENTER && fighter.team != owner && core_breached)
            {
                fighter.planned_attack = Some(target);
                continue;
            }
            let next = fighter
                .pos
                .neighbors()
                .into_iter()
                .filter(|&h| {
                    in_bounds(h) && (h != CENTER || fighter.team == owner || core_breached)
                })
                .filter(|h| !snapshot.iter().any(|other| other.pos == *h))
                .min_by_key(|h| (h.distance(target), h.q, h.r));
            fighter.planned_move = next;
            if let Some(next) = next
                && next.distance(target) <= range
            {
                fighter.planned_attack = Some(target);
            }
        }
    }

    fn resolve_one_interior(&mut self, city: usize) {
        let owner = self.cities[city].team;
        let interior = &mut self.cities[city].interior;
        let snapshot = interior.fighters.clone();
        for fighter in &mut interior.fighters {
            if let Some(dest) = fighter.planned_move
                && in_bounds(dest)
                && (dest != CENTER || fighter.team == owner || interior.core_hp <= 0.0)
                && !snapshot.iter().any(|other| other.pos == dest)
                && snapshot
                    .iter()
                    .filter(|other| other.planned_move == Some(dest))
                    .count()
                    == 1
            {
                fighter.pos = dest;
            }
        }
        let snapshot = interior.fighters.clone();
        let mut damage = vec![0.0; snapshot.len()];
        let mut core_damage = 0.0;
        for fighter in &snapshot {
            let Some(target) = fighter.planned_attack else {
                continue;
            };
            if fighter.pos.distance(target) > fighter.unit_type.stats().attack_range {
                continue;
            }
            let attack = fighter.unit_type.stats().attack;
            if let Some((index, defender)) = snapshot
                .iter()
                .enumerate()
                .find(|(_, other)| other.pos == target && other.team != fighter.team)
            {
                let defense = defender.unit_type.stats().defense;
                damage[index] += combat::roll_damage_against(attack, defense, &mut self.rng);
            } else if target == CENTER && fighter.team != owner && interior.core_hp > 0.0 {
                core_damage += combat::roll_damage_against(attack, CORE_DEFENSE, &mut self.rng);
            }
        }
        if interior.core_hp > 0.0
            && let Some((index, fighter)) = snapshot
                .iter()
                .enumerate()
                .filter(|(_, f)| f.team != owner && f.pos.distance(CENTER) <= 1)
                .min_by_key(|(_, f)| f.source_id)
        {
            damage[index] += combat::roll_damage_against(
                CORE_ATTACK,
                fighter.unit_type.stats().defense,
                &mut self.rng,
            );
        }
        interior.core_hp = (interior.core_hp - core_damage).max(0.0);
        for (fighter, amount) in interior.fighters.iter_mut().zip(damage) {
            fighter.hp = (fighter.hp - amount).max(0.0);
            fighter.planned_move = None;
            fighter.planned_attack = None;
            if fighter.hp <= 0.0 {
                interior.fallen_sources.insert(fighter.source_id);
            }
        }
        interior.fighters.retain(|f| f.hp > 0.0);
        let conqueror = (interior.core_hp <= 0.0)
            .then(|| {
                interior
                    .fighters
                    .iter()
                    .find(|f| f.pos == CENTER && f.team != owner)
            })
            .flatten()
            .map(|f| f.team);
        if let Some(team) = conqueror {
            let city_ref = &mut self.cities[city];
            city_ref.team = team;
            city_ref.interior.core_hp = CORE_HP;
            city_ref.hp = CITY_MAX_HP * 0.5;
            city_ref.queue.clear();
            city_ref.production = 0;
            city_ref.pending_building = None;
            city_ref.planned_sites.clear();
            city_ref.barracks_queue.clear();
            city_ref.barracks_production = 0;
            self.notice = format!("CITY {} CAPTURED IN THE INTERIOR", city_ref.id + 1);
            log::info!("{}: {team:?} captures its command post", city_ref.id + 1);
            self.auto_assign_city(city);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_troops_project_independent_copies_and_fallen_copy_stays_fallen() {
        let mut game = GameState::siege_scenario();
        let unit_hp = game.units.iter().find(|u| u.id == 0).unwrap().hp;
        let copy = game.cities[1]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == 0)
            .unwrap();
        assert_eq!(copy.pos, Hex::new(-2, 0));
        copy.hp = 1.0;
        assert_eq!(game.units.iter().find(|u| u.id == 0).unwrap().hp, unit_hp);
        game.cities[1].interior.fallen_sources.insert(0);
        game.cities[1]
            .interior
            .fighters
            .retain(|f| f.source_id != 0);
        game.sync_city_interiors();
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .all(|f| f.source_id != 0)
        );
        game.units.iter_mut().find(|u| u.id == 0).unwrap().pos = Hex::new(2, 0);
        game.sync_city_interiors();
        game.units.iter_mut().find(|u| u.id == 0).unwrap().pos = Hex::new(3, 0);
        game.sync_city_interiors();
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .any(|f| f.source_id == 0)
        );
    }

    #[test]
    fn interior_orders_do_not_change_the_source_unit() {
        let mut game = GameState::siege_scenario();
        game.interior_click(Hex::new(-2, 0));
        game.interior_click(Hex::new(-1, 0));
        let copy = game.cities[1]
            .interior
            .fighters
            .iter()
            .find(|f| f.source_id == 0)
            .unwrap();
        assert_eq!(copy.planned_move, Some(Hex::new(-1, 0)));
        let source = game.units.iter().find(|u| u.id == 0).unwrap();
        assert_eq!(source.planned_move, None);
        assert_eq!(source.planned_attack, None);
    }

    #[test]
    fn capture_requires_occupying_the_breached_command_post() {
        let mut game = GameState::siege_scenario();
        game.cities[1].interior.core_hp = 0.0;
        game.resolve_one_interior(1);
        assert_eq!(game.cities[1].team, Team::Red);
        let copy = game.cities[1]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == 0)
            .unwrap();
        copy.pos = Hex::new(-1, 0);
        copy.planned_move = Some(CENTER);
        game.resolve_one_interior(1);
        assert_eq!(game.cities[1].team, Team::Blue);
        assert_eq!(game.cities[1].interior.core_hp, CORE_HP);
        assert!(game.cities[1].queue.is_empty());
    }
}
