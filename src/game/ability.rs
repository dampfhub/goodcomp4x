//! Each unit type's special ability, queued like an order for the turn.

use super::unit::UnitType;

pub const SHIELD_WALL_DEFENSE: f32 = 1.5;
pub const VOLLEY_DAMAGE: f32 = 0.6;
pub const CHARGE_ATTACK: f32 = 1.5;
pub const CHARGE_EXTRA_MOVE: i32 = 1;
pub const DEPLOYED_EXTRA_RANGE: i32 = 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Ability {
    /// Melee braces: +50% defense for the turn, but it can't move.
    ShieldWall,
    /// Ranged: the attack also hits enemies next to its target, but every
    /// hit deals 60% damage.
    Volley,
    /// Cavalry: +1 move and +50% attack for the turn.
    Charge,
    /// Siege spends a whole turn setting up (or packing up) instead of moving
    /// or attacking. While set up it has +1 range but can't move.
    Deploy,
}

impl Ability {
    pub fn of(unit_type: UnitType) -> Self {
        match unit_type {
            UnitType::Melee => Ability::ShieldWall,
            UnitType::Ranged => Ability::Volley,
            UnitType::Cavalry => Ability::Charge,
            UnitType::Siege => Ability::Deploy,
            UnitType::Horse => Ability::Charge,
            UnitType::Armored => Ability::ShieldWall,
        }
    }

    /// Turns the ability is unavailable after being used.
    pub fn cooldown(self) -> u32 {
        match self {
            Ability::ShieldWall => 1,
            Ability::Volley | Ability::Charge => 2,
            Ability::Deploy => 0,
        }
    }
}
