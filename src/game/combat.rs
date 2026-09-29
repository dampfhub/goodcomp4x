//! Civ-style combat math: damage grows exponentially with the gap between an
//! attacker's attack and the target's defense (boosted by the terrain it
//! stands on). There is no random spread: the same fight always deals the
//! same damage, so the attack preview (`draw.rs`) calls these same functions.

use super::ability::Ability;
use super::hex::HexGrid;
use super::unit::{Unit, UnitType};

/// Damage `attacker` deals to `target` in one blow, with `defense_multiplier`
/// the terrain and fort bonus on the target (`GameState::defense_multiplier`).
pub fn damage(attacker: &Unit, target: &Unit, defense_multiplier: f32) -> f32 {
    let defense = target.stats().defense * defense_multiplier;
    damage_against(attacker.stats().attack, defense)
}

/// Damage an attack of `attack` deals in one blow against `defense`: the one
/// formula every fight uses, units and structures alike. A structure's listed
/// defense already includes its fortification, so terrain does not modify it
/// again.
pub fn damage_against(attack: f32, defense: f32) -> f32 {
    (30.0 * ((attack - defense) * 0.04).exp()).clamp(1.0, 100.0)
}

/// The share of `attacker`'s damage that reaches `defender` across the
/// waterline: land melee can't reach ships at all, and a galley's guns do
/// little to troops ashore. 1 for a fight on one side of it.
pub fn shore_scale(attacker: &Unit, defender: &Unit) -> f32 {
    if !attacker.is_naval() && defender.is_naval() {
        match attacker.unit_type {
            UnitType::Ranged => 0.4,
            UnitType::Siege => 0.6,
            _ => 0.0,
        }
    } else if attacker.unit_type == UnitType::PatrolGalley && !defender.is_naval() {
        0.35
    } else {
        1.0
    }
}

/// Melee attacks draw retaliation from a surviving defender; ranged ones don't.
pub fn draws_retaliation(attacker: &Unit) -> bool {
    attacker.unit_type.stats().attack_range == 1
}

/// Whether `defender`, hit by `attacker` for `hit`, strikes back: a melee
/// attack it survives, with both on the same side of the waterline.
pub fn retaliates(attacker: &Unit, defender: &Unit, hit: f32) -> bool {
    draws_retaliation(attacker) && attacker.is_naval() == defender.is_naval() && defender.hp > hit
}

/// Combat-relevant conditions on `unit`, like " (on hills, shield wall)", for
/// combat log lines. Empty when there are none.
pub fn unit_note(unit: &Unit, grid: &HexGrid, in_fort: bool) -> String {
    let mut notes = Vec::new();
    if in_fort {
        notes.push("in a fort");
    }
    if grid.tile(unit.pos).feature.is_some() {
        notes.push("under cover");
    }
    if grid.tile(unit.pos).hills {
        notes.push("on hills");
    }
    if unit.deployed {
        notes.push("deployed");
    }
    if unit.ability_queued {
        match unit.ability() {
            Ability::ShieldWall => notes.push("shield wall"),
            Ability::Charge => notes.push("charging"),
            Ability::Volley | Ability::Deploy | Ability::Lookout => {}
        }
    }
    if notes.is_empty() {
        String::new()
    } else {
        format!(" ({})", notes.join(", "))
    }
}

pub fn hp_status(unit: &Unit) -> String {
    if unit.is_alive() {
        format!("{:.0}/{:.0} hp", unit.hp, unit.max_hp())
    } else {
        "destroyed".to_string()
    }
}
