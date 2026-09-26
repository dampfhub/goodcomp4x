//! Turn resolution: queued orders play out step by step, in an order set by
//! each unit type's role. All units in a step act simultaneously.

use std::collections::HashMap;

use super::ability::{Ability, VOLLEY_DAMAGE};
use super::city::{BARRACKS_DEFENSE, CITY_ATTACK, CITY_ATTACK_RANGE, CITY_DEFENSE};
use super::effects::{Effect, Outcome};
use super::hex::Hex;
use super::unit::{Unit, UnitType};
use super::{AI_TEAM, GameState, combat};

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
const RESOLUTION_ORDER: [(UnitType, Phase); 12] = [
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
];

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

    /// Plans the AI's turn, then queues every step for `update` to play out.
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

        self.plan_ai_turn(AI_TEAM);
        self.pending_steps.extend(RESOLUTION_ORDER);

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
        // With instant playback (a debug setting) every step resolves at
        // once, still in order, so the outcome is the same.
        if self.step_timer > 0.0 && !self.instant_playback {
            return;
        }

        if self.instant_playback {
            self.recent_actors.clear();
        }
        while let Some((unit_type, phase)) = self.pending_steps.pop_front() {
            let actors = self.resolve_step(unit_type, phase);
            if !actors.is_empty() {
                self.highlight_timer = HIGHLIGHT_DURATION;
                if self.instant_playback {
                    self.recent_actors.extend(actors);
                    continue;
                }
                self.recent_actors = actors;
                self.step_timer = STEP_INTERVAL;
                break;
            }
        }

        if !self.is_resolving() {
            self.resolve_economy();
            for unit in &mut self.units {
                if unit.ability_queued && unit.ability() == Ability::Deploy {
                    let state = if unit.deployed { "packed up" } else { "set up" };
                    log::info!("{unit} has {state}");
                }
                unit.end_turn();
            }
            log::info!("=== turn {} resolved ===", self.turn);
            self.select_next_or_end_turn(None);
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
                .filter(|&i| !self.workers.contains(&self.units[i].id))
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
        let mut moving = vec![true; movers.len()];

        let mut claims: HashMap<Hex, Vec<usize>> = HashMap::new();
        for (m, &dest) in dests.iter().enumerate() {
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
    }

    /// Resolves `attackers` simultaneously: every attack sees the board as it
    /// was at the start of the step and damage lands at the end, so a unit
    /// killed this step still gets its own attack off. An attack hits whichever
    /// enemy occupies the target hex now, not necessarily the one there at
    /// planning time. A unit in a contested hex fights its rival instead.
    fn resolve_attacks(&mut self, attackers: &[usize]) {
        let mut engagements: Vec<Engagement> = Vec::new();
        let mut structure_hits: Vec<(usize, usize, bool, f32)> = Vec::new();
        // Each attack's animation, played once the step's damage is known.
        let mut shots: Vec<Effect> = Vec::new();
        for &a in attackers {
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
                .into_iter()
                .filter_map(|hex| self.enemy_of_team_at(hex, attacker.team))
                .collect();
            if defenders.is_empty() {
                if let Some(city) = self.enemy_city_at(target, attacker.team) {
                    structure_hits.push((a, city, false, scale));
                } else if let Some(city) = self.enemy_barracks_at(target, attacker.team) {
                    structure_hits.push((a, city, true, scale));
                }
            }
            if defenders.is_empty() && structure_hits.last().is_none_or(|hit| hit.0 != a) {
                log::info!(
                    "{attacker} attacks ({}, {}) but hits nothing",
                    target.q,
                    target.r
                );
            }
            let outcome = if defenders.is_empty() {
                Outcome::Miss
            } else {
                Outcome::Hit
            };
            shots.push(Effect::Shot { from, to, outcome });
            engagements.extend(defenders.into_iter().map(|d| Engagement::new(a, d, scale)));
        }

        let mut damage = vec![0.0; self.units.len()];
        let mut city_damage = vec![0.0; self.cities.len()];
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
            let hit = engagement.damage_scale
                * combat::roll_damage(attacker, defender, &self.grid, &mut self.rng);
            damage[d] += hit;
            let attacker_note = combat::unit_note(attacker, &self.grid);
            let defender_note = combat::unit_note(defender, &self.grid);
            let verb = if engagement.damage_scale < 1.0 {
                "volleys"
            } else {
                "hits"
            };

            let retaliation_scale = match reverse {
                Some(reverse) => Some(reverse.damage_scale),
                None if combat::draws_retaliation(attacker) && defender.hp > hit => Some(1.0),
                None => None,
            };
            if let Some(back_scale) = retaliation_scale {
                let back =
                    back_scale * combat::roll_damage(defender, attacker, &self.grid, &mut self.rng);
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

        for (attacker, city, barracks, scale) in structure_hits {
            let attack = self.units[attacker].stats().attack;
            let defense = if barracks {
                BARRACKS_DEFENSE
            } else {
                CITY_DEFENSE
            };
            let hit = scale * combat::roll_damage_against(attack, defense, &mut self.rng);
            if barracks {
                barracks_damage[city] += hit;
            } else {
                city_damage[city] += hit;
                // A city returns fire at every unit attacking from its range.
                let unit = &self.units[attacker];
                if self.cities[city].pos.distance(unit.pos) <= CITY_ATTACK_RANGE {
                    let defense =
                        unit.stats().defense * self.grid.tile(unit.pos).defense_multiplier();
                    damage[attacker] +=
                        combat::roll_damage_against(CITY_ATTACK, defense, &mut self.rng);
                }
            }
        }

        for shot in shots {
            self.play(shot);
        }
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
        for (i, &taken) in city_damage.iter().enumerate() {
            if taken > 0.0 {
                let (at, fatal) = {
                    let city = &self.cities[i];
                    (city.pos.to_world(), city.hp <= taken)
                };
                self.play(Effect::Damage {
                    at,
                    amount: taken,
                    fatal,
                });
                self.cities[i].hp = (self.cities[i].hp - taken).max(0.0);
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
