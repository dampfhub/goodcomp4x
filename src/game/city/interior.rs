//! A small tactical board inside each city. Field units on the six neighboring
//! hexes project independent fighters through the corresponding gates.

use crate::game::fast_hash::HashSet;
use crate::game::hex::Hex;
use crate::game::multiplayer::WAITING_NOTICE;
use crate::game::terrain::Resource;
use crate::game::unit::{Team, UnitStats, UnitType, apply_training_upgrade};
use crate::game::{Camera, GameState, combat};

pub(in crate::game) const CORE_HP: f32 = 80.0;
const CORE_DEFENSE: f32 = 18.0;
const CORE_ATTACK: f32 = 12.0;
const CORE_ATTACK_RANGE: i32 = 2;
const CENTER: Hex = Hex::new(0, 0);
/// How far from a captured city's center a unit of another side is pushed
/// off it (`push_off_city_center`): to the nearest free hex within this many
/// hexes, or it is lost.
pub(in crate::game) const PUSH_OFF_RANGE: i32 = 2;

#[derive(Clone)]
pub(in crate::game) struct Interior {
    pub core_hp: f32,
    pub fighters: Vec<InteriorFighter>,
}

impl Default for Interior {
    fn default() -> Self {
        Self {
            core_hp: CORE_HP,
            fighters: Vec::new(),
        }
    }
}

#[derive(Clone)]
pub(in crate::game) struct InteriorFighter {
    pub source_id: u32,
    pub team: Team,
    pub unit_type: UnitType,
    pub training_upgrade: Option<Resource>,
    pub pos: Hex,
    pub hp: f32,
    pub planned_move: Option<Hex>,
    pub planned_attack: Option<Hex>,
}

impl InteriorFighter {
    fn stats(&self) -> UnitStats {
        let mut stats = self.unit_type.stats();
        apply_training_upgrade(&mut stats, self.training_upgrade);
        stats
    }

    pub(in crate::game) fn move_reaches(&self, tile: Hex) -> bool {
        self.planned_move.unwrap_or(self.pos).distance(tile) <= self.stats().move_range.max(1)
    }

    pub(in crate::game) fn attack_reaches(&self, tile: Hex) -> bool {
        self.planned_move.unwrap_or(self.pos).distance(tile) <= self.stats().attack_range
    }

    pub(in crate::game) fn health_fraction(&self) -> f32 {
        self.hp / self.stats().max_hp
    }
}

pub(in crate::game) fn in_bounds(hex: Hex) -> bool {
    hex.distance(CENTER) <= 2
}

impl GameState {
    pub fn is_in_city_interior(&self) -> bool {
        self.interior_view.is_some()
    }

    /// Backspace or the Clear Orders button inside a city: the selected
    /// troop's interior orders go. Not once the turn is out of the player's
    /// hands (playing out, or a network game's plan sent).
    pub fn clear_selected_interior_orders(&mut self) {
        if self.is_resolving() {
            return;
        }
        let (Some(city), Some(source)) = (self.interior_view, self.interior_selected) else {
            return;
        };
        if let Some(fighter) = self.cities[city]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == source && f.team == self.local_team)
        {
            fighter.planned_move = None;
            fighter.planned_attack = None;
            self.notice = "INTERIOR ORDERS CLEARED".into();
        }
    }

    /// V opens the city under the pointer, the selected city, or the closest
    /// city to the selected field unit. The interior is a separate order view.
    pub fn toggle_city_interior(&mut self) {
        if self.is_playing_out() {
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
                        .position(|c| c.pos == hex && (c.team == self.local_team || fog.sees(hex)))
                })
            })
            .or_else(|| {
                self.selected.and_then(|unit| {
                    self.cities
                        .iter()
                        .enumerate()
                        .filter(|(_, city)| city.team == self.local_team || fog.sees(city.pos))
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
        // Looking doesn't change the game: in a network game the troops
        // stand inside from the turn's start (`begin_lockstep_turn`), on
        // every machine at once.
        if !self.is_networked() {
            self.sync_city_interiors();
        }
        self.interior_view = Some(city);
        self.interior_selected = None;
        self.selected_city = None;
        self.selected_barracks = None;
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        // Leave room for the command panel on the left and Debug on the right.
        self.camera = Camera::new(glam::Vec2::new(-1.35, 0.0), 6.0);
        self.notice = if self.waiting_for_peers() {
            WAITING_NOTICE
        } else {
            "CITY INTERIOR: CLICK A BLUE TROOP, THEN A TILE OR ENEMY; ESC RETURNS"
        }
        .into();
    }

    pub(in crate::game) fn close_city_interior(&mut self) {
        let Some(city) = self.interior_view.take() else {
            return;
        };
        if let Some(camera) = self.exterior_camera.take() {
            self.camera = camera;
        }
        self.interior_selected = None;
        self.selected_city = (self.cities[city].team == self.local_team).then_some(city);
        self.hovered_tile = None;
        self.hovered_city = None;
        self.notice = if self.selected_city.is_some() {
            "CITY VIEW: CLICK THE CITY CENTER TO RE-ENTER THE INTERIOR".into()
        } else {
            "EXTERIOR MAP: HOVER THE CITY AND PRESS V TO RE-ENTER".into()
        };
    }

    /// Clicking a tile in the interior view selects a copy or gives it an
    /// independent tactical order. The source field unit is unchanged. With
    /// the plan sent (a network game waiting for the others'), it only
    /// selects.
    pub(in crate::game) fn interior_click(&mut self, tile: Hex) {
        let Some(city) = self.interior_view else {
            return;
        };
        if self.is_playing_out() || !in_bounds(tile) {
            return;
        }
        let interior = &self.cities[city].interior;
        let clicked = interior.fighters.iter().find(|f| f.pos == tile);
        if let Some(fighter) = clicked.filter(|f| f.team == self.local_team) {
            self.interior_selected =
                (self.interior_selected != Some(fighter.source_id)).then_some(fighter.source_id);
            return;
        }
        if self.is_resolving() {
            self.interior_selected = None;
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
        let enemy = clicked.is_some_and(|f| f.team != self.local_team);
        let core = tile == CENTER
            && self.cities[city].team != self.local_team
            && self.cities[city].interior.core_hp > 0.0;
        if enemy || core {
            if fighter.attack_reaches(tile) {
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
            && fighter.move_reaches(tile)
        {
            let fighter = self.cities[city]
                .interior
                .fighters
                .iter_mut()
                .find(|f| f.source_id == source)
                .unwrap();
            fighter.planned_move = (fighter.planned_move != Some(tile)).then_some(tile);
            fighter.planned_attack = None;
            self.notice = if tile == CENTER && self.cities[city].interior.core_hp <= 0.0 {
                "CAPTURE MOVE QUEUED - END TURN TO TAKE THE CITY"
            } else {
                "INTERIOR MOVE QUEUED"
            }
            .into();
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
                        && !unit.is_naval()
                        && !self.settlers.contains(&unit.id)
                        && !unit.is_animal()
                })
                .collect();
            let ids: HashSet<_> = adjacent.iter().map(|unit| unit.id).collect();
            city.interior
                .fighters
                .retain(|f| ids.contains(&f.source_id));
            for unit in adjacent {
                if city
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
                        training_upgrade: unit.training_upgrade,
                        pos,
                        hp: unit.interior_hp,
                        planned_move: None,
                        planned_attack: None,
                    });
                }
            }
            city.interior.fighters.sort_by_key(|f| f.source_id);
        }
    }

    /// Exterior combat may remove a source before the interior phase. Its
    /// tactical copy disappears as part of that same death, not at next entry.
    pub(in crate::game) fn discard_interior_copies_of_dead_units(&mut self) {
        let living: HashSet<_> = self.units.iter().map(|unit| unit.id).collect();
        for city in &mut self.cities {
            city.interior
                .fighters
                .retain(|fighter| living.contains(&fighter.source_id));
        }
        if self
            .interior_selected
            .is_some_and(|source| !living.contains(&source))
        {
            self.interior_selected = None;
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
        let humans = self.humans.clone();
        for fighter in &mut self.cities[city].interior.fighters {
            if humans.contains(&fighter.team) {
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
            let range = fighter.stats().attack_range;
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

    /// Whether the player hears of what happens in `city`'s interior: their
    /// own city, or one in sight.
    fn hears_of_city(&self, city: usize) -> bool {
        let city = &self.cities[city];
        city.team == self.local_team || self.fog().sees(city.pos)
    }

    /// After `city` changes hands, moves every unit not of its new owner
    /// (its old owner's, stepped onto it as it fell) off its center: to the
    /// nearest free hex (`push_off_hex`), or, with none, the unit is lost.
    /// Lowest id first, so each one pushed takes its hex before the next
    /// looks, the same on every machine.
    pub(in crate::game) fn push_off_city_center(&mut self, city: usize) {
        let (center, owner) = (self.cities[city].pos, self.cities[city].team);
        let mut ids: Vec<u32> = self
            .units
            .iter()
            .filter(|unit| unit.pos == center && unit.team != owner)
            .map(|unit| unit.id)
            .collect();
        ids.sort_unstable();
        for id in ids {
            let Some(index) = self.units.iter().position(|unit| unit.id == id) else {
                continue;
            };
            let unit = &self.units[index];
            match self.push_off_hex(center, unit.team, unit.is_naval()) {
                Some(hex) => {
                    let unit = &mut self.units[index];
                    unit.pos = hex;
                    // Displaced: whatever it was doing there is over.
                    unit.wake();
                    unit.clear_orders();
                    log::info!(
                        "{unit} is pushed off city {}'s center to ({}, {})",
                        self.cities[city].id + 1,
                        hex.q,
                        hex.r
                    );
                }
                None => {
                    let unit = self.units.remove(index);
                    self.settlers.remove(&unit.id);
                    self.player_controlled_units.remove(&unit.id);
                    log::info!(
                        "{unit} is lost: no free hex to push it to off city {}'s center",
                        self.cities[city].id + 1
                    );
                }
            }
        }
        self.discard_interior_copies_of_dead_units();
    }

    /// Where a unit of `team` (a ship, if `naval`) pushed off the city
    /// center `center` goes: the nearest hex within `PUSH_OFF_RANGE` it
    /// could stand on (land, or water for a ship), with no unit, no city
    /// center and no other side's worker, ties broken by coordinates. Walls
    /// don't matter: it's pushed, not walking.
    fn push_off_hex(&self, center: Hex, team: Team, naval: bool) -> Option<Hex> {
        let range = PUSH_OFF_RANGE;
        (-range..=range)
            .flat_map(|dq| {
                ((-range).max(-dq - range)..=range.min(-dq + range))
                    .map(move |dr| Hex::new(center.q + dq, center.r + dr))
            })
            .filter(|&hex| hex != center)
            .filter(|&hex| {
                if naval {
                    self.grid.contains(hex) && self.grid.terrain(hex).is_water()
                } else {
                    self.can_enter(hex)
                }
            })
            .filter(|&hex| !self.is_occupied(hex))
            .filter(|&hex| self.cities.iter().all(|city| city.pos != hex))
            .filter(|&hex| {
                self.field_workers
                    .iter()
                    .all(|worker| worker.pos != hex || worker.team == team)
            })
            .min_by_key(|&hex| (center.distance(hex), hex.q, hex.r))
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
            if fighter.pos.distance(target) > fighter.stats().attack_range {
                continue;
            }
            let attack = fighter.stats().attack;
            if let Some((index, defender)) = snapshot
                .iter()
                .enumerate()
                .find(|(_, other)| other.pos == target && other.team != fighter.team)
            {
                let defense = defender.stats().defense;
                damage[index] += combat::damage_against(attack, defense);
            } else if target == CENTER && fighter.team != owner && interior.core_hp > 0.0 {
                core_damage += combat::damage_against(attack, CORE_DEFENSE);
            }
        }
        if interior.core_hp > 0.0
            && let Some((index, fighter)) = snapshot
                .iter()
                .enumerate()
                .filter(|(_, f)| f.team != owner && f.pos.distance(CENTER) <= CORE_ATTACK_RANGE)
                .min_by_key(|(_, f)| f.source_id)
        {
            damage[index] += combat::damage_against(CORE_ATTACK, fighter.stats().defense);
        }
        interior.core_hp = (interior.core_hp - core_damage).max(0.0);
        let breached = core_damage > 0.0 && interior.core_hp <= 0.0;
        let player_inside = snapshot.iter().any(|f| f.team == self.local_team);
        for (fighter, amount) in interior.fighters.iter_mut().zip(damage) {
            fighter.hp = (fighter.hp - amount).max(0.0);
            fighter.planned_move = None;
            fighter.planned_attack = None;
        }
        let casualties: HashSet<_> = interior
            .fighters
            .iter()
            .filter(|fighter| fighter.hp <= 0.0)
            .map(|fighter| fighter.source_id)
            .collect();
        let health_after_battle: Vec<_> = interior
            .fighters
            .iter()
            .map(|fighter| (fighter.source_id, fighter.hp))
            .collect();
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
        if breached && self.hears_of_city(city) {
            let player = self.local_team;
            let id = self.cities[city].id + 1;
            self.notice = if owner == player {
                "YOUR POST BREACHED - KEEP THE ENEMY OFF THE CENTER".into()
            } else if player_inside {
                format!("ENEMY POST BREACHED - MOVE A {player:?} TROOP ONTO THE CENTER TO CAPTURE")
                    .to_uppercase()
            } else {
                format!("{owner:?} CITY {id}'S POST BREACHED").to_uppercase()
            };
        }
        if let Some(team) = conqueror {
            let player = self.local_team;
            let id = self.cities[city].id + 1;
            if team == player {
                self.notice = format!("CITY {id} CAPTURED IN THE INTERIOR");
            } else if self.hears_of_city(city) {
                self.notice = if owner == player {
                    format!("CITY {id} LOST: {team:?} TOOK ITS COMMAND POST")
                } else {
                    format!("{team:?} CAPTURED {owner:?} CITY {id}")
                }
                .to_uppercase();
            }
            let city_ref = &mut self.cities[city];
            city_ref.team = team;
            city_ref.interior.core_hp = CORE_HP;
            city_ref.queue.clear();
            city_ref.barracks_queue.clear();
            city_ref.worker_jobs.clear();
            // Its workers at home serve the new owner, none of them held.
            city_ref.held_workers = 0;
            log::info!("{}: {team:?} captures its command post", city_ref.id + 1);
            for index in 0..self.field_workers.len() {
                if self.field_workers[index].home != city {
                    continue;
                }
                let worker_team = self.field_workers[index].team;
                let worker_pos = self.field_workers[index].pos;
                if let Some(new_home) = self.nearest_city(worker_team, worker_pos) {
                    self.field_workers[index].home = new_home;
                } else {
                    // Without a friendly city to return to, an outlying worker
                    // follows the captured city's new owner. It isn't
                    // recalled (held once home): it takes that city's jobs.
                    let worker = &mut self.field_workers[index];
                    worker.team = team;
                    worker.job = None;
                    worker.work_left = None;
                    worker.home = city;
                    worker.recalled = false;
                }
            }
            self.push_off_city_center(city);
            // A worker changing sides may share its hex with an old-side unit;
            // capture it immediately, as by a move.
            self.capture_workers();
            self.auto_assign_city(city);
        }
        for (source, hp) in health_after_battle {
            if let Some(unit) = self.units.iter_mut().find(|unit| unit.id == source) {
                unit.interior_hp = hp;
            }
            for other_city in &mut self.cities {
                if let Some(copy) = other_city
                    .interior
                    .fighters
                    .iter_mut()
                    .find(|fighter| fighter.source_id == source)
                {
                    copy.hp = hp;
                }
            }
        }
        if !casualties.is_empty() {
            self.units.retain(|unit| !casualties.contains(&unit.id));
            self.discard_interior_copies_of_dead_units();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::unit::Unit;

    /// The siege position with a Red cavalry (id 900, holding and
    /// guarding) on Red's city center as Blue's fighter takes the breached post.
    fn capture_under_a_red_unit(prepare: impl FnOnce(&mut GameState)) -> GameState {
        let mut game = GameState::siege_scenario();
        let center = game.cities[1].pos;
        assert_eq!(game.cities[1].team, Team::Red);
        let mut unit = Unit::new(900, center, Team::Red, UnitType::Cavalry);
        unit.holding = true;
        unit.guarding = true;
        game.units.push(unit);
        prepare(&mut game);
        game.cities[1].interior.core_hp = 0.0;
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
        game
    }

    /// #289: the old owner's unit on the center is pushed to the nearest
    /// free hex, ties broken by coordinates, and its orders end.
    #[test]
    fn capture_pushes_the_old_owners_unit_off_the_center() {
        let center = GameState::siege_scenario().cities[1].pos;
        // Blue troops hold all six gates, so the nearest free hexes are two
        // away; the lowest q, then r, of those is (center.q - 2, center.r).
        let first = Hex::new(center.q - 2, center.r);
        let game = capture_under_a_red_unit(|game| {
            assert!(center.neighbors().iter().all(|&h| game.is_occupied(h)));
            assert!(game.can_enter(first) && !game.is_occupied(first));
        });
        let unit = game.units.iter().find(|u| u.id == 900).unwrap();
        assert_eq!(unit.pos, first);
        assert!(!unit.holding && !unit.guarding && !unit.has_orders());

        // With that hex taken, the next one by coordinates.
        let game = capture_under_a_red_unit(|game| {
            let id = game.next_unit_id;
            game.next_unit_id += 1;
            game.units
                .push(Unit::new(id, first, Team::Red, UnitType::Melee));
        });
        let unit = game.units.iter().find(|u| u.id == 900).unwrap();
        assert_eq!(unit.pos, Hex::new(center.q - 2, center.r + 1));
    }

    /// #289: with no free hex within `PUSH_OFF_RANGE`, the unit is lost.
    #[test]
    fn a_unit_with_nowhere_to_be_pushed_is_lost() {
        let game = capture_under_a_red_unit(|game| {
            let center = game.cities[1].pos;
            let around: Vec<Hex> = game
                .grid
                .all_hexes()
                .filter(|&h| h != center && h.distance(center) <= PUSH_OFF_RANGE)
                .collect();
            for hex in around {
                game.grid
                    .set_tile(hex, crate::game::terrain::Terrain::Mountains);
            }
            game.settlers.insert(900);
        });
        assert!(game.units.iter().all(|u| u.id != 900));
        assert!(!game.settlers.contains(&900));
        let center = game.cities[1].pos;
        assert!(game.units.iter().all(|u| u.pos != center));
    }

    #[test]
    fn wounded_copy_keeps_its_health_after_leaving_and_reentering() {
        let mut game = GameState::siege_scenario();
        // Leave only the ranged attacker in range of the post so its new
        // range-two shot must hurt that fighter.
        game.units.retain(|unit| unit.id != 0);
        game.discard_interior_copies_of_dead_units();
        let exterior_hp = game.units.iter().find(|unit| unit.id == 1).unwrap().hp;
        game.resolve_one_interior(1);
        let wounded = game
            .units
            .iter()
            .find(|unit| unit.id == 1)
            .unwrap()
            .interior_hp;
        assert!(wounded < UnitType::Ranged.stats().max_hp);
        assert_eq!(
            game.units.iter().find(|unit| unit.id == 1).unwrap().hp,
            exterior_hp
        );

        game.units.iter_mut().find(|unit| unit.id == 1).unwrap().pos = Hex::new(2, 0);
        game.sync_city_interiors();
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .all(|f| f.source_id != 1)
        );
        game.units.iter_mut().find(|unit| unit.id == 1).unwrap().pos = Hex::new(3, 1);
        game.sync_city_interiors();
        assert_eq!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .find(|f| f.source_id == 1)
                .unwrap()
                .hp,
            wounded
        );
    }

    #[test]
    fn interior_death_kills_its_exterior_source() {
        let mut game = GameState::siege_scenario();
        game.units.retain(|unit| unit.id != 0);
        game.discard_interior_copies_of_dead_units();
        game.cities[1]
            .interior
            .fighters
            .iter_mut()
            .find(|f| f.source_id == 1)
            .unwrap()
            .hp = 1.0;
        game.resolve_one_interior(1);
        assert!(game.units.iter().all(|unit| unit.id != 1));
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .all(|f| f.source_id != 1)
        );
    }

    #[test]
    fn disbanding_an_exterior_unit_removes_its_interior_copy() {
        let mut game = GameState::siege_scenario();
        game.selected = game.units.iter().position(|unit| unit.id == 0);
        game.disband_selected();
        game.disband_selected();
        assert!(game.units.iter().all(|unit| unit.id != 0));
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .all(|f| f.source_id != 0)
        );
    }

    #[test]
    fn exterior_death_removes_its_interior_copy_in_the_attack_step() {
        let mut game = GameState::siege_scenario();
        game.units.iter_mut().find(|unit| unit.id == 0).unwrap().hp = 1.0;
        let target = game.units.iter().find(|unit| unit.id == 0).unwrap().pos;
        game.units
            .iter_mut()
            .find(|unit| unit.id == 5)
            .unwrap()
            .planned_attack = Some(target);
        game.resolve_step(UnitType::Ranged, crate::game::turn::Phase::Attack);
        assert!(game.units.iter().all(|unit| unit.id != 0));
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .all(|f| f.source_id != 0)
        );
        game.resolve_city_interiors();
        assert!(game.units.iter().all(|unit| unit.id != 0));
        assert!(game.cities.iter().all(|city| {
            city.interior
                .fighters
                .iter()
                .all(|fighter| fighter.source_id != 0)
        }));
    }

    #[test]
    fn exterior_unit_cannot_walk_onto_enemy_city_and_lose_its_interior_copy() {
        let mut game = GameState::siege_scenario();
        let archer = game.units.iter().position(|unit| unit.id == 1).unwrap();
        let city = game.cities[1].pos;
        assert_eq!(game.units[archer].pos.distance(city), 1);
        assert!(!game.can_step(game.units[archer].pos, city, Team::Blue));
        assert!(
            !game
                .known_reachable_hexes(
                    game.units[archer].pos,
                    game.units[archer].stats().move_range,
                    Team::Blue,
                    &game.fog(),
                )
                .contains(&city)
        );

        // Even a stale order from before the city was revealed cannot bypass
        // the check when the move actually resolves.
        game.units[archer].planned_move = Some(city);
        let start = game.units[archer].pos;
        game.resolve_step(UnitType::Ranged, crate::game::turn::Phase::Move);
        assert_eq!(game.units[archer].pos, start);
        game.sync_city_interiors();
        assert!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .any(|fighter| fighter.source_id == 1)
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
    fn nonfatal_exterior_damage_leaves_interior_health_unchanged() {
        let mut game = GameState::siege_scenario();
        let target = game.units.iter().find(|unit| unit.id == 0).unwrap().pos;
        game.units
            .iter_mut()
            .find(|unit| unit.id == 5)
            .unwrap()
            .planned_attack = Some(target);
        game.resolve_step(UnitType::Ranged, crate::game::turn::Phase::Attack);
        let source = game.units.iter().find(|unit| unit.id == 0).unwrap();
        assert!(source.hp < source.max_hp());
        assert_eq!(source.interior_hp, source.max_hp());
        assert_eq!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .find(|fighter| fighter.source_id == 0)
                .unwrap()
                .hp,
            source.max_hp()
        );
    }

    #[test]
    fn capture_requires_occupying_the_breached_command_post() {
        let mut game = GameState::siege_scenario();
        game.field_workers.push(crate::game::workers::FieldWorker {
            id: 10_000,
            team: Team::Red,
            home: 1,
            base: game.cities[1].pos,
            pos: game.cities[1].pos,
            job: None,
            work_left: None,
            recalled: false,
        });
        // Red held its worker at home.
        assert_eq!(game.cities[1].workers, 1);
        game.cities[1].held_workers = 1;
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
        assert_eq!(game.field_workers[0].team, Team::Blue);
        assert_eq!(game.field_workers[0].home, 1);
        // Blue's now: nothing held, and the worker out free to take jobs.
        assert_eq!(game.cities[1].held_workers, 0);
        assert!(!game.field_workers[0].recalled);
    }

    #[test]
    fn city_capture_resolves_a_stranded_worker_on_an_old_side_unit() {
        let mut game = GameState::siege_scenario();
        let worker_pos = game
            .grid
            .all_hexes()
            .find(|&hex| {
                game.grid.is_passable(hex)
                    && hex.distance(game.cities[1].pos) >= 3
                    && game.units.iter().all(|unit| unit.pos != hex)
            })
            .unwrap();
        game.field_workers.push(crate::game::workers::FieldWorker {
            id: 10_001,
            team: Team::Red,
            home: 1,
            base: game.cities[1].pos,
            pos: worker_pos,
            job: None,
            work_left: None,
            recalled: false,
        });
        game.units.push(crate::game::unit::Unit::new(
            10_002,
            worker_pos,
            Team::Red,
            UnitType::Melee,
        ));
        game.cities[1].interior.core_hp = 0.0;
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
        assert!(
            game.field_workers
                .iter()
                .all(|worker| { game.enemy_of_team_at(worker.pos, worker.team).is_none() })
        );
        assert!(game.field_workers.iter().all(|worker| worker.id != 10_001));
    }

    #[test]
    fn post_click_changes_from_attack_to_capture_move_after_breach() {
        let mut game = GameState::siege_scenario();
        game.interior_click(Hex::new(-2, 2));
        game.interior_click(CENTER);
        assert_eq!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .find(|fighter| fighter.source_id == 1)
                .unwrap()
                .planned_attack,
            Some(CENTER)
        );

        game.cities[1].interior.core_hp = 0.0;
        let melee = game.cities[1]
            .interior
            .fighters
            .iter_mut()
            .find(|fighter| fighter.source_id == 0)
            .unwrap();
        melee.pos = Hex::new(-1, 0);
        game.interior_click(Hex::new(-1, 0));
        game.interior_click(CENTER);
        assert_eq!(
            game.cities[1]
                .interior
                .fighters
                .iter()
                .find(|fighter| fighter.source_id == 0)
                .unwrap()
                .planned_move,
            Some(CENTER)
        );
        assert!(game.notice.contains("CAPTURE MOVE QUEUED"));
    }

    #[test]
    fn breaching_post_announces_the_capture_step() {
        let mut game = GameState::siege_scenario();
        game.cities[1].interior.core_hp = 1.0;
        game.cities[1]
            .interior
            .fighters
            .iter_mut()
            .find(|fighter| fighter.source_id == 1)
            .unwrap()
            .planned_attack = Some(CENTER);
        game.resolve_one_interior(1);
        assert_eq!(game.cities[1].interior.core_hp, 0.0);
        assert_eq!(
            game.notice,
            "ENEMY POST BREACHED - MOVE A BLUE TROOP ONTO THE CENTER TO CAPTURE"
        );
    }

    /// The siege of Red's city 2 by Blue, with the player on Green, which
    /// sees only what a scout on `watch` sees: the post breached, then Blue
    /// onto the center. What the player was told of each.
    fn a_siege_watched_by_green(watch: Option<Hex>) -> (String, String) {
        let mut game = GameState::siege_scenario();
        let red = 1;
        assert_eq!(game.cities[red].team, Team::Red);
        game.local_team = Team::Green;
        if let Some(pos) = watch {
            game.units.push(crate::game::unit::Unit::new(
                90,
                pos,
                Team::Green,
                UnitType::Scout,
            ));
        }
        game.notice.clear();
        fn blue(game: &mut GameState, red: usize) -> &mut InteriorFighter {
            game.cities[red]
                .interior
                .fighters
                .iter_mut()
                .find(|fighter| fighter.source_id == 1)
                .unwrap()
        }
        game.cities[red].interior.core_hp = 1.0;
        blue(&mut game, red).planned_attack = Some(CENTER);
        game.resolve_one_interior(red);
        assert_eq!(game.cities[red].interior.core_hp, 0.0);
        let breach = std::mem::take(&mut game.notice);

        game.cities[red]
            .interior
            .fighters
            .retain(|fighter| fighter.team != Team::Red);
        blue(&mut game, red).pos = CENTER;
        game.resolve_one_interior(red);
        assert_eq!(game.cities[red].team, Team::Blue);
        (breach, game.notice)
    }

    #[test]
    fn only_a_city_the_player_owns_or_sees_makes_news() {
        assert_eq!(a_siege_watched_by_green(None), Default::default());
        let city = GameState::siege_scenario().cities[1].pos;
        let (breach, capture) = a_siege_watched_by_green(Some(city.neighbors()[0]));
        assert_eq!(breach, "RED CITY 2'S POST BREACHED");
        assert_eq!(capture, "BLUE CAPTURED RED CITY 2");
    }

    #[test]
    fn the_players_own_city_lost_or_taken_is_worded_by_side() {
        let mut game = GameState::siege_scenario();
        game.cities[1].interior.core_hp = 0.0;
        game.cities[1]
            .interior
            .fighters
            .retain(|fighter| fighter.team != Team::Red);
        game.cities[1].interior.fighters[0].pos = CENTER;
        game.resolve_one_interior(1);
        assert_eq!(game.notice, "CITY 2 CAPTURED IN THE INTERIOR");

        // Played from Red's side, the same capture is a loss.
        let mut game = GameState::siege_scenario();
        game.local_team = Team::Red;
        game.cities[1].interior.core_hp = 0.0;
        game.cities[1]
            .interior
            .fighters
            .retain(|fighter| fighter.team != Team::Red);
        game.cities[1].interior.fighters[0].pos = CENTER;
        game.resolve_one_interior(1);
        assert_eq!(game.notice, "CITY 2 LOST: BLUE TOOK ITS COMMAND POST");
    }

    #[test]
    fn siege_scenario_can_capture_with_a_direct_assault() {
        let results: Vec<_> = (0..16).map(direct_assault_capture_turn).collect();
        assert!(
            results
                .iter()
                .all(|turn| turn.is_some_and(|turn| turn <= 8)),
            "F12 assault capture turns by seed: {results:?}"
        );
    }

    fn direct_assault_capture_turn(seed: u64) -> Option<usize> {
        let mut game = GameState::siege_scenario();
        game.seed_rng(seed);
        assert_eq!(game.cities[1].interior.fighters.len(), 6);

        for turn in 0..12 {
            let snapshot = game.cities[1].interior.fighters.clone();
            let core_breached = game.cities[1].interior.core_hp <= 0.0;
            let occupied: HashSet<_> = snapshot.iter().map(|fighter| fighter.pos).collect();
            let mut claimed = HashSet::default();
            for fighter in &mut game.cities[1].interior.fighters {
                if fighter.team != Team::Blue {
                    continue;
                }
                let range = fighter.stats().attack_range;
                if let Some(enemy) = snapshot
                    .iter()
                    .filter(|other| {
                        other.team == Team::Red && fighter.pos.distance(other.pos) <= range
                    })
                    .min_by_key(|other| (other.hp as i32, other.source_id))
                {
                    fighter.planned_attack = Some(enemy.pos);
                } else if !core_breached && fighter.pos.distance(CENTER) <= range {
                    fighter.planned_attack = Some(CENTER);
                } else if let Some(next) = fighter
                    .pos
                    .neighbors()
                    .into_iter()
                    .filter(|&hex| {
                        in_bounds(hex) && !occupied.contains(&hex) && !claimed.contains(&hex)
                    })
                    .filter(|&hex| hex != CENTER || core_breached)
                    .min_by_key(|hex| (hex.distance(CENTER), hex.q, hex.r))
                {
                    fighter.planned_move = Some(next);
                    claimed.insert(next);
                }
            }
            let field = game.units.clone();
            for unit in &mut game.units {
                if unit.team != Team::Blue {
                    continue;
                }
                unit.planned_attack = field
                    .iter()
                    .filter(|enemy| enemy.team == Team::Red)
                    .filter(|enemy| unit.pos.distance(enemy.pos) <= unit.stats().attack_range)
                    .min_by_key(|enemy| (enemy.hp as i32, enemy.id))
                    .map(|enemy| enemy.pos);
            }
            game.resolve_turn();
            game.update(10.0);
            if game.cities[1].team == Team::Blue {
                return Some(turn + 1);
            }
        }
        None
    }
}
