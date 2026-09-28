//! Turn resolution: queued orders play out step by step, in an order set by
//! each unit type's role. All units in a step act simultaneously.

use std::collections::HashMap;

use super::ability::{Ability, VOLLEY_DAMAGE};
use super::city::{BARRACKS_DEFENSE, Building};
use super::effects::{Effect, Outcome};
use super::hex::Hex;
use super::unit::{Unit, UnitType};
use super::{GameState, combat};

/// Seconds between steps while a turn plays out, and how long the units that
/// just acted stay highlighted. Staggering steps makes the order legible.
const STEP_INTERVAL: f32 = 0.6;
const HIGHLIGHT_DURATION: f32 = 0.4;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Phase {
    Move,
    Attack,
}

/// When each unit type moves and attacks within a turn:
/// - Scouts move first, slipping away before anyone else acts, and poke in
///   alongside the ranged volley.
/// - Cavalry moves before anyone attacks, so it can't be caught mid-charge.
/// - Ranged fires before melee closes in, then repositions (shoot, then move).
/// - Melee moves and fights in the middle, screening for ranged and siege.
/// - Siege is slow: it moves and fires last, and may die before it acts.
const RESOLUTION_ORDER: [(UnitType, Phase); 18] = [
    (UnitType::Scout, Phase::Move),
    (UnitType::Cavalry, Phase::Move),
    (UnitType::Melee, Phase::Move),
    (UnitType::Ranged, Phase::Attack),
    (UnitType::Scout, Phase::Attack),
    (UnitType::Cavalry, Phase::Attack),
    (UnitType::Melee, Phase::Attack),
    (UnitType::Ranged, Phase::Move),
    (UnitType::Siege, Phase::Move),
    (UnitType::Siege, Phase::Attack),
    (UnitType::Armored, Phase::Move),
    (UnitType::Armored, Phase::Attack),
    (UnitType::PatrolGalley, Phase::Move),
    (UnitType::LandingCraft, Phase::Move),
    (UnitType::BombardShip, Phase::Move),
    (UnitType::PatrolGalley, Phase::Attack),
    (UnitType::LandingCraft, Phase::Attack),
    (UnitType::BombardShip, Phase::Attack),
];

/// One step of a turn: a unit type's moves or attacks, or, after all of
/// those, the workers' (`workers.rs`). Workers go last so they're exposed:
/// they walk and work only once every unit has acted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Step {
    Units(UnitType, Phase),
    Workers,
}

/// Where `unit_type` falls (1 = first) among all the steps of `phase`.
pub(super) fn step_rank(unit_type: UnitType, phase: Phase) -> u32 {
    let rank = RESOLUTION_ORDER
        .iter()
        .filter(|&&(_, p)| p == phase)
        .position(|&(t, _)| t == unit_type)
        .expect("every unit type has a move and an attack step");
    rank as u32 + 1
}

impl GameState {
    pub fn is_resolving(&self) -> bool {
        !self.pending_steps.is_empty()
    }

    /// Plans every AI team's turn, then queues every step for `update` to
    /// play out.
    /// Called after the player explicitly ends planning.
    pub(super) fn resolve_turn(&mut self) {
        if self.is_resolving() {
            return;
        }
        self.turn += 1;
        log::info!("=== resolving turn {} ===", self.turn);
        // Indices shift as units die, so nothing stays selected.
        self.selected = None;
        self.group.clear();

        for team in self.ai_teams() {
            self.plan_ai_turn(team);
        }
        self.pending_steps.extend(
            RESOLUTION_ORDER
                .into_iter()
                .map(|(unit_type, phase)| Step::Units(unit_type, phase)),
        );
        self.pending_steps.push_back(Step::Workers);

        // Pause before the first step so the last order placed can be seen.
        self.step_timer = STEP_INTERVAL;
    }

    /// Advances turn playback by `dt` seconds, playing the next step once
    /// `STEP_INTERVAL` has passed. Steps where nobody acts are skipped.
    pub fn update(&mut self, dt: f32) {
        self.explore();
        self.camera.update(dt);
        self.age_effects(dt);
        self.highlight_timer -= dt;
        if self.highlight_timer <= 0.0 {
            self.recent_actors.clear();
        }

        if !self.is_resolving() {
            return;
        }
        self.step_timer -= dt;
        // With instant playback (a player setting, `settings.rs`) every step resolves at
        // once, still in order, so the outcome is the same.
        if self.step_timer > 0.0 && !self.settings.instant_playback {
            return;
        }

        if self.settings.instant_playback {
            self.recent_actors.clear();
        }
        while let Some(step) = self.pending_steps.pop_front() {
            let actors = match step {
                Step::Units(unit_type, phase) => self.resolve_step(unit_type, phase),
                Step::Workers => self.resolve_workers(),
            };
            if !actors.is_empty() {
                self.highlight_timer = HIGHLIGHT_DURATION;
                if self.settings.instant_playback {
                    self.recent_actors.extend(actors);
                    continue;
                }
                self.recent_actors = actors;
                self.step_timer = STEP_INTERVAL;
                break;
            }
        }

        if !self.is_resolving() {
            self.resolve_coastal_batteries();
            self.resolve_transport();
            self.resolve_city_interiors();
            // Before the economy, so a city reward is spent (or capped) like
            // the turn's own income.
            self.resolve_ruins();
            self.resolve_economy();
            for unit in &mut self.units {
                if unit.ability_queued && unit.ability() == Ability::Deploy {
                    let state = if unit.deployed { "packed up" } else { "set up" };
                    log::info!("{unit} has {state}");
                }
                unit.end_turn();
            }
            self.advance_queues();
            for city in &mut self.cities {
                city.workers_resting = false;
            }
            log::info!("=== turn {} resolved ===", self.turn);
            self.select_next_or_end_turn(None);
        }
    }

    /// Cargo remains the same logical unit, with its exterior and interior HP.
    /// It does not occupy a map tile while embarked, and goes down with the ship.
    fn resolve_coastal_batteries(&mut self) {
        let batteries: Vec<_> = self
            .cities
            .iter()
            .filter_map(|city| {
                city.placed_site(Building::CoastalBattery)
                    .filter(|_| city.coastal_battery_hp > 0.0)
                    .map(|site| (site, city.team))
            })
            .collect();
        let mut hits = vec![0.0; self.units.len()];
        for (site, team) in batteries {
            if let Some((index, target)) = self
                .units
                .iter()
                .enumerate()
                .filter(|(_, unit)| {
                    unit.team != team && unit.is_naval() && site.distance(unit.pos) <= 2
                })
                .min_by_key(|(_, unit)| (site.distance(unit.pos), unit.id))
            {
                hits[index] +=
                    combat::roll_damage_against(28.0, target.stats().defense, &mut self.rng);
                self.play(Effect::Shot {
                    from: site.to_world(),
                    to: target.pos.to_world(),
                    outcome: Outcome::Hit,
                });
            }
        }
        for (unit, hit) in self.units.iter_mut().zip(hits) {
            unit.hp = (unit.hp - hit).max(0.0);
        }
        self.units.retain(Unit::is_alive);
        self.discard_interior_copies_of_dead_units();
    }

    fn resolve_transport(&mut self) {
        let unloads: Vec<(u32, Hex)> = self
            .units
            .iter_mut()
            .filter_map(|u| u.planned_unload.take().map(|hex| (u.id, hex)))
            .collect();
        for (ship_id, dest) in unloads {
            let Some(ship) = self.units.iter().position(|u| u.id == ship_id) else {
                continue;
            };
            if self.units[ship].unit_type != UnitType::LandingCraft
                || self.units[ship].pos.distance(dest) != 1
                || !self.grid.is_passable(dest)
                || self.is_occupied(dest)
                || self.cities.iter().any(|city| city.pos == dest)
            {
                continue;
            }
            if !self.units[ship].cargo.is_empty() {
                let mut passenger = self.units[ship].cargo.remove(0);
                passenger.pos = dest;
                passenger.clear_orders();
                self.units.push(passenger);
            }
        }
        let mut boarders: Vec<usize> = self
            .units
            .iter()
            .enumerate()
            .filter_map(|(i, u)| u.planned_board.map(|_| i))
            .collect();
        boarders.sort_unstable_by(|a, b| b.cmp(a));
        for i in boarders {
            let Some(ship_id) = self.units[i].planned_board.take() else {
                continue;
            };
            let Some(ship) = self.units.iter().position(|u| u.id == ship_id) else {
                continue;
            };
            if self.units[ship].unit_type != UnitType::LandingCraft
                || self.units[ship].team != self.units[i].team
                || self.units[ship].pos.distance(self.units[i].pos) != 1
                || self.units[ship].cargo.len() >= 4
            {
                continue;
            }
            let mut passenger = self.units.remove(i);
            passenger.clear_orders();
            let ship = self.units.iter().position(|u| u.id == ship_id).unwrap();
            self.units[ship].cargo.push(passenger);
        }
    }

    /// Resolves one step, returning the ids of the units that acted in it.
    pub(super) fn resolve_step(&mut self, unit_type: UnitType, phase: Phase) -> Vec<u32> {
        let of_type = |i: &usize| self.units[*i].unit_type == unit_type;
        let actors: Vec<usize> = match phase {
            Phase::Move => {
                let mut movers: Vec<usize> = (0..self.units.len())
                    .filter(of_type)
                    .filter(|&i| self.units[i].planned_move.is_some())
                    .collect();
                // A swap partner moves together with its ally, even if its
                // own type moves later.
                for i in movers.clone() {
                    if let Some(partner) = self.swap_partner(i)
                        && !movers.contains(&partner)
                    {
                        movers.push(partner);
                    }
                }
                movers
            }
            // Units in a contested hex fight whether or not they have orders.
            Phase::Attack => (0..self.units.len())
                .filter(of_type)
                .filter(|&i| self.units[i].planned_attack.is_some() || self.rival_of(i).is_some())
                .collect(),
        };
        if actors.is_empty() {
            return Vec::new();
        }

        log::info!("-- {unit_type:?} {phase:?} --");
        let ids = actors.iter().map(|&i| self.units[i].id).collect();
        match phase {
            Phase::Move => self.resolve_moves(&actors),
            Phase::Attack => self.resolve_attacks(&actors),
        }
        ids
    }

    /// Moves `movers` simultaneously:
    /// - Two enemies heading for the same hex both take it, contesting it.
    ///   Any other pile-up on one hex bounces everyone involved.
    /// - Enemies can't swap places by moving through each other; allies can.
    /// - A unit can move into a hex whose occupants are all leaving this step,
    ///   so chains and rotations work, but not one where anyone stays put.
    fn resolve_moves(&mut self, movers: &[usize]) {
        let dests: Vec<Hex> = movers
            .iter()
            .map(|&i| self.units[i].planned_move.unwrap())
            .collect();
        // A wall, or someone else's gate, turns a move back if no way around
        // it is left within the unit's move, e.g. one planned before the
        // player saw the wall.
        let moving: Vec<bool> = movers
            .iter()
            .zip(&dests)
            .map(|(&i, &dest)| {
                let unit = &self.units[i];
                let open = self
                    .reachable_hexes_by(unit.pos, unit.stats().move_range.max(1), |a, b| {
                        if unit.is_naval() {
                            self.grid.contains(b) && self.grid.terrain(b).is_water()
                        } else {
                            self.can_step(a, b, unit.team)
                        }
                    })
                    .contains(&dest)
                    || self.rail_transfer_available(unit.pos, dest, unit.team, None);
                if !open {
                    log::info!(
                        "{unit} move blocked: a wall stands in the way to ({}, {})",
                        dest.q,
                        dest.r
                    );
                }
                open
            })
            .collect();
        let mut moving = moving;

        let mut claims: HashMap<Hex, Vec<usize>> = HashMap::new();
        for (m, &dest) in dests.iter().enumerate().filter(|&(m, _)| moving[m]) {
            claims.entry(dest).or_default().push(m);
        }
        let mut contests = Vec::new();
        for (dest, claimants) in &claims {
            match claimants[..] {
                [_] => {}
                [a, b] if self.units[movers[a]].team != self.units[movers[b]].team => {
                    contests.push((a, b, *dest));
                }
                _ => {
                    let names: Vec<String> = claimants
                        .iter()
                        .map(|&m| self.units[movers[m]].to_string())
                        .collect();
                    log::info!(
                        "{} collide at ({}, {}); none of them moves",
                        names.join(" and "),
                        dest.q,
                        dest.r
                    );
                    claimants.iter().for_each(|&m| moving[m] = false);
                }
            }
        }

        let mut swaps = Vec::new();
        for a in 0..movers.len() {
            for b in a + 1..movers.len() {
                let (unit_a, unit_b) = (&self.units[movers[a]], &self.units[movers[b]]);
                if dests[a] != unit_b.pos || dests[b] != unit_a.pos {
                    continue;
                }
                if unit_a.team == unit_b.team {
                    swaps.push((a, b));
                } else {
                    log::info!("{unit_a} and {unit_b} can't move through each other");
                    moving[a] = false;
                    moving[b] = false;
                }
            }
        }

        // A move is blocked if anyone in the destination hex isn't moving
        // away. Each block can strand another mover, so repeat until nothing
        // changes.
        loop {
            let mut changed = false;
            for m in 0..movers.len() {
                if !moving[m] {
                    continue;
                }
                let staying = self.units_at(dests[m]).find(|&occupant| {
                    !movers
                        .iter()
                        .zip(&moving)
                        .any(|(&i, &ok)| i == occupant && ok)
                });
                if let Some(occupant) = staying {
                    log::info!(
                        "{} move blocked: {} holds the hex",
                        self.units[movers[m]],
                        self.units[occupant]
                    );
                    moving[m] = false;
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for &(a, b) in &swaps {
            if moving[a] && moving[b] {
                log::info!(
                    "{} and {} swap places",
                    self.units[movers[a]],
                    self.units[movers[b]]
                );
            }
        }
        for (m, &i) in movers.iter().enumerate() {
            if moving[m] && self.rival_of(i).is_some() {
                let unit = &self.units[i];
                log::info!(
                    "{unit} withdraws from the contested hex at ({}, {})",
                    unit.pos.q,
                    unit.pos.r
                );
            }
        }
        for &(a, b, dest) in &contests {
            if moving[a] && moving[b] {
                log::info!(
                    "{} and {} clash at ({}, {}); the hex is contested",
                    self.units[movers[a]],
                    self.units[movers[b]],
                    dest.q,
                    dest.r
                );
            }
        }

        // Every mover's order is spent, whether or not it got through.
        for (m, &i) in movers.iter().enumerate() {
            let unit = &mut self.units[i];
            if moving[m] {
                unit.pos = dests[m];
            }
            unit.planned_move = None;
        }
        // Stepping onto an enemy worker captures it.
        self.capture_workers();
    }

    /// Resolves `attackers` simultaneously: every attack sees the board as it
    /// was at the start of the step and damage lands at the end, so a unit
    /// killed this step still gets its own attack off. An attack hits whichever
    /// enemy occupies the target hex now, not necessarily the one there at
    /// planning time. A unit in a contested hex fights its rival instead.
    fn resolve_attacks(&mut self, attackers: &[usize]) {
        let mut engagements: Vec<Engagement> = Vec::new();
        let mut barracks_hits: Vec<(usize, usize, f32)> = Vec::new();
        let mut battery_hits: Vec<(usize, usize, f32)> = Vec::new();
        let mut killed_workers: Vec<u32> = Vec::new();
        // Each attack's animation, played once the step's damage is known.
        let mut shots: Vec<Effect> = Vec::new();
        for &a in attackers {
            if !self.units[a].can_attack() {
                continue;
            }
            let attacker = &self.units[a];
            let from = self.unit_layout(a).0;
            if let Some(rival) = self.rival_of(a) {
                if attacker.planned_attack.is_some() {
                    log::info!(
                        "{attacker} is locked in its contested hex and can't attack elsewhere"
                    );
                }
                engagements.push(Engagement::new(a, rival, 1.0));
                let to = self.unit_layout(rival).0;
                shots.push(Effect::Shot {
                    from,
                    to,
                    outcome: Outcome::Hit,
                });
                continue;
            }

            let target = attacker.planned_attack.unwrap();
            if self.empty_city_target(target, attacker.team) {
                shots.push(Effect::Shot {
                    from,
                    to: target.to_world(),
                    outcome: Outcome::Miss,
                });
                continue;
            }
            if !self.attack_target_legal(a, target, false) {
                continue;
            }
            let to = target.to_world();
            if attacker.pos.distance(target) > attacker.stats().attack_range {
                log::info!("{attacker}'s attack canceled: target out of range after moves");
                shots.push(Effect::Shot {
                    from,
                    to,
                    outcome: Outcome::OutOfRange,
                });
                continue;
            }

            let volley = attacker.ability_queued && attacker.ability() == Ability::Volley;
            let (hexes, scale) = if volley {
                let mut hexes = vec![target];
                hexes.extend(target.neighbors());
                (hexes, VOLLEY_DAMAGE)
            } else {
                (vec![target], 1.0)
            };
            let defenders: Vec<usize> = hexes
                .iter()
                .filter_map(|&hex| self.enemy_of_team_at(hex, attacker.team))
                .collect();
            // A structure is hit only when no enemy unit is.
            let barracks = if defenders.is_empty() {
                self.enemy_barracks_at(target, attacker.team)
            } else {
                None
            };
            if let Some(city) = barracks {
                barracks_hits.push((a, city, scale));
            }
            let battery = if defenders.is_empty() && barracks.is_none() {
                self.enemy_coastal_battery_at(target, attacker.team)
            } else {
                None
            };
            if let Some(city) = battery {
                battery_hits.push((a, city, scale));
            }
            // With nothing else to hit, the attack kills any enemy workers
            // out on its hexes. A unit on a worker's hex shields it.
            let workers_hit: Vec<u32> =
                if defenders.is_empty() && barracks.is_none() && battery.is_none() {
                    hexes
                        .iter()
                        .flat_map(|&hex| self.enemy_workers_at(hex, attacker.team))
                        .map(|w| self.field_workers[w].id)
                        .collect()
                } else {
                    Vec::new()
                };
            let outcome = if !defenders.is_empty()
                || barracks.is_some()
                || battery.is_some()
                || !workers_hit.is_empty()
            {
                Outcome::Hit
            } else {
                log::info!(
                    "{attacker} attacks ({}, {}) but hits nothing",
                    target.q,
                    target.r
                );
                Outcome::Miss
            };
            shots.push(Effect::Shot { from, to, outcome });
            engagements.extend(defenders.into_iter().map(|d| Engagement::new(a, d, scale)));
            killed_workers.extend(workers_hit);
        }

        let mut damage = vec![0.0; self.units.len()];
        let mut barracks_damage = vec![0.0; self.cities.len()];
        for engagement in &engagements {
            let (a, d) = (engagement.attacker, engagement.defender);
            // Two units attacking each other in the same step is a single
            // exchange of blows, not two attacks that each draw retaliation.
            let reverse = engagements
                .iter()
                .find(|e| e.attacker == d && e.defender == a);
            if reverse.is_some() && d < a {
                continue;
            }
            let (attacker, defender) = (&self.units[a], &self.units[d]);
            let (attacker_cover, defender_cover) = (
                self.defense_multiplier(attacker),
                self.defense_multiplier(defender),
            );
            let shore_scale = if !attacker.is_naval() && defender.is_naval() {
                match attacker.unit_type {
                    UnitType::Ranged => 0.4,
                    UnitType::Siege => 0.6,
                    _ => 0.0,
                }
            } else if attacker.unit_type == UnitType::PatrolGalley && !defender.is_naval() {
                0.35
            } else {
                1.0
            };
            let hit = shore_scale
                * engagement.damage_scale
                * combat::roll_damage(attacker, defender, defender_cover, &mut self.rng);
            damage[d] += hit;
            let attacker_note = combat::unit_note(attacker, &self.grid, self.in_fort(attacker));
            let defender_note = combat::unit_note(defender, &self.grid, self.in_fort(defender));
            let verb = if engagement.damage_scale < 1.0 {
                "volleys"
            } else {
                "hits"
            };

            let retaliation_scale = match reverse {
                Some(reverse) => Some(reverse.damage_scale),
                None if combat::draws_retaliation(attacker)
                    && attacker.is_naval() == defender.is_naval()
                    && defender.hp > hit =>
                {
                    Some(1.0)
                }
                None => None,
            };
            if let Some(back_scale) = retaliation_scale {
                let back = back_scale
                    * combat::roll_damage(defender, attacker, attacker_cover, &mut self.rng);
                damage[a] += back;
                let verb = if reverse.is_some() {
                    "trades blows with"
                } else {
                    verb
                };
                log::info!(
                    "{attacker}{attacker_note} {verb} {defender}{defender_note} for {hit:.0}, taking {back:.0} back"
                );
            } else {
                log::info!(
                    "{attacker}{attacker_note} {verb} {defender}{defender_note} for {hit:.0}"
                );
            }
        }

        for (attacker, city, scale) in barracks_hits {
            let attack = self.units[attacker].stats().attack;
            let hit = scale * combat::roll_damage_against(attack, BARRACKS_DEFENSE, &mut self.rng);
            barracks_damage[city] += hit;
        }

        let mut battery_damage = vec![0.0; self.cities.len()];
        for (attacker, city, scale) in battery_hits {
            battery_damage[city] += scale
                * combat::roll_damage_against(
                    self.units[attacker].stats().attack,
                    18.0,
                    &mut self.rng,
                );
        }

        for shot in shots {
            self.play(shot);
        }
        for w in 0..self.field_workers.len() {
            if killed_workers.contains(&self.field_workers[w].id) {
                let at = self.field_workers[w].pos.to_world();
                self.play(Effect::Damage {
                    at,
                    amount: 1.0,
                    fatal: true,
                });
            }
        }
        self.kill_workers(&killed_workers);
        for (i, &taken) in damage.iter().enumerate() {
            if taken > 0.0 {
                let at = self.unit_layout(i).0;
                let fatal = self.units[i].hp <= taken;
                self.play(Effect::Damage {
                    at,
                    amount: taken,
                    fatal,
                });
            }
        }
        for (i, &taken) in barracks_damage.iter().enumerate() {
            if taken > 0.0 {
                let Some(at) = self.cities[i].barracks else {
                    continue;
                };
                let fatal = self.cities[i].barracks_hp <= taken;
                self.play(Effect::Damage {
                    at: at.to_world(),
                    amount: taken,
                    fatal,
                });
                let city = &mut self.cities[i];
                city.barracks_hp = (city.barracks_hp - taken).max(0.0);
                if city.barracks_hp == 0.0 {
                    city.barracks = None;
                    city.barracks_queue.clear();
                    city.barracks_production = 0;
                    // Gone from the map, so the city may build another.
                    city.built.retain(|&b| b != Building::Barracks);
                    if self.selected_barracks == Some(i) {
                        self.selected_barracks = None;
                    }
                }
            }
        }
        for (city, taken) in self.cities.iter_mut().zip(battery_damage) {
            if taken > 0.0 {
                city.coastal_battery_hp = (city.coastal_battery_hp - taken).max(0.0);
                if city.coastal_battery_hp == 0.0 {
                    city.extra_buildings.remove(&Building::CoastalBattery);
                    city.built.retain(|&b| b != Building::CoastalBattery);
                }
            }
        }
        for (unit, &taken) in self.units.iter_mut().zip(&damage) {
            if taken > 0.0 {
                unit.hp = (unit.hp - taken).max(0.0);
                log::info!("  {unit}: {}", combat::hp_status(unit));
            }
        }
        // The attacks have happened, so their planned arrows give way to the
        // animations.
        for &a in attackers {
            self.units[a].planned_attack = None;
        }
        self.units.retain(Unit::is_alive);
        self.discard_interior_copies_of_dead_units();
    }
}

/// One attacker striking one defender within an attack step.
struct Engagement {
    attacker: usize,
    defender: usize,
    /// Multiplier on the damage dealt, below 1 for volley hits.
    damage_scale: f32,
}

impl Engagement {
    fn new(attacker: usize, defender: usize, damage_scale: f32) -> Self {
        Self {
            attacker,
            defender,
            damage_scale,
        }
    }
}

#[cfg(test)]
mod naval_tests {
    use super::*;
    use crate::game::city::{Build, BuildUnit};
    use crate::game::unit::Team;

    #[test]
    fn harbor_produces_a_ship_on_water_and_it_cannot_walk_ashore() {
        let mut g = GameState::naval_scenario();
        g.cities[0].production = BuildUnit::PatrolGalley.cost();
        let before = g.units.len();
        g.complete_builds();
        assert_eq!(g.units.len(), before + 1);
        let ship = g.units.last().unwrap();
        assert_eq!(ship.unit_type, UnitType::PatrolGalley);
        assert!(g.grid.terrain(ship.pos).is_water());
        let reachable = g.known_reachable_for_domain(ship.pos, 3, Team::Blue, &g.fog(), true);
        assert!(reachable.iter().all(|h| g.grid.terrain(*h).is_water()));
        assert!(!reachable.is_empty());
        assert!(
            !g.cities[0]
                .queue
                .contains(&Build::Unit(BuildUnit::PatrolGalley))
        );
    }

    #[test]
    fn resolution_skips_an_invalid_landing_craft_attack() {
        let mut game = GameState::city_scenario();
        game.units.clear();
        game.cities.clear();
        let mut craft = Unit::new(90, Hex::new(0, 0), Team::Blue, UnitType::LandingCraft);
        craft.planned_attack = Some(Hex::new(1, 0));
        game.units.push(craft);
        game.units
            .push(Unit::new(91, Hex::new(1, 0), Team::Red, UnitType::Melee));
        let hp = game.units[1].hp;
        game.resolve_attacks(&[0]);
        assert_eq!(game.units[1].hp, hp);
    }

    #[test]
    fn landing_craft_carries_four_troops_and_lands_one_on_open_shore() {
        let mut g = GameState::naval_scenario();
        g.units.clear();
        let ship_id = 90;
        g.units.push(Unit::new(
            ship_id,
            Hex::new(-1, 0),
            Team::Blue,
            UnitType::LandingCraft,
        ));
        for (id, pos) in [(91, Hex::new(-2, 0)), (92, Hex::new(-2, 1))] {
            let mut troop = Unit::new(id, pos, Team::Blue, UnitType::Melee);
            troop.planned_board = Some(ship_id);
            g.units.push(troop);
        }
        g.resolve_transport();
        assert_eq!(g.units[0].cargo.len(), 2);
        g.units[0].pos = Hex::new(-1, 2);
        for (id, pos) in [(93, Hex::new(-2, 2)), (94, Hex::new(-2, 3))] {
            let mut troop = Unit::new(id, pos, Team::Blue, UnitType::Melee);
            troop.planned_board = Some(ship_id);
            g.units.push(troop);
        }
        g.resolve_transport();
        assert_eq!(g.units.len(), 1);
        assert_eq!(g.units[0].cargo.len(), 4);
        g.units[0].pos = Hex::new(-1, 0);
        let mut fifth = Unit::new(95, Hex::new(-2, 0), Team::Blue, UnitType::Melee);
        fifth.planned_board = Some(ship_id);
        g.units.push(fifth);
        g.resolve_transport();
        assert_eq!(g.units[0].cargo.len(), 4);
        assert_eq!(g.units.len(), 2);
        g.units[0].planned_unload = Some(Hex::new(-2, 1));
        g.resolve_transport();
        assert_eq!(g.units[0].cargo.len(), 3);
        assert_eq!(g.units[2].id, 92);
        assert_eq!(g.units[2].pos, Hex::new(-2, 1));
    }

    #[test]
    fn coastal_battery_hits_ships_and_can_be_destroyed() {
        let mut g = GameState::naval_scenario();
        g.units.clear();
        g.units.push(Unit::new(
            300,
            Hex::new(-1, 0),
            Team::Red,
            UnitType::PatrolGalley,
        ));
        let initial = g.units[0].hp;
        g.resolve_coastal_batteries();
        assert!(g.units[0].hp < initial);
        g.units.clear();
        let site = g.cities[1].placed_site(Building::CoastalBattery).unwrap();
        let mut ship = Unit::new(301, Hex::new(1, 0), Team::Blue, UnitType::BombardShip);
        ship.planned_attack = Some(site);
        g.units.push(ship);
        g.cities[1].coastal_battery_hp = 1.0;
        g.resolve_attacks(&[0]);
        assert_eq!(g.cities[1].placed_site(Building::CoastalBattery), None);
    }
}
