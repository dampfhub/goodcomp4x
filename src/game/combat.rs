//! Civ-style combat math: damage grows exponentially with the gap between an
//! attacker's attack and the target's defense (boosted by the terrain it
//! stands on), with ±20% variance.

use rand::RngExt;

use super::ability::Ability;
use super::hex::HexGrid;
use super::unit::Unit;

/// Damage `attacker` deals to `target` in one blow, with `defense_multiplier`
/// the terrain and fort bonus on the target (`GameState::defense_multiplier`).
pub fn roll_damage(
    attacker: &Unit,
    target: &Unit,
    defense_multiplier: f32,
    rng: &mut impl RngExt,
) -> f32 {
    let defense = target.stats().defense * defense_multiplier;
    roll_damage_against(attacker.stats().attack, defense, rng)
}

/// Damage for attacks involving a static structure. Its listed defense already
/// includes its fortification, so terrain does not modify it again.
pub fn roll_damage_against(attack: f32, defense: f32, rng: &mut impl RngExt) -> f32 {
    let base = 30.0 * ((attack - defense) * 0.04).exp();
    (base * rng.random_range(0.8..1.2)).clamp(1.0, 100.0)
}

/// Melee attacks draw retaliation from a surviving defender; ranged ones don't.
pub fn draws_retaliation(attacker: &Unit) -> bool {
    attacker.unit_type.stats().attack_range == 1
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
