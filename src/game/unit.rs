use std::fmt;

use super::ability::{
    Ability, CHARGE_ATTACK, CHARGE_EXTRA_MOVE, DEPLOYED_EXTRA_RANGE, SHIELD_WALL_DEFENSE,
};
use super::hex::Hex;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Team {
    Blue,
    Red,
}

impl Team {
    pub fn color(self) -> [f32; 4] {
        match self {
            Team::Blue => [0.30, 0.55, 0.95, 1.0],
            Team::Red => [0.92, 0.32, 0.28, 1.0],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitType {
    Melee,
    Ranged,
    Cavalry,
    Siege,
}

#[derive(Clone, Copy, Debug)]
pub struct UnitStats {
    pub max_hp: f32,
    pub attack: f32,
    pub defense: f32,
    pub move_range: i32,
    pub attack_range: i32,
}

impl UnitType {
    /// Melee is the balanced baseline; ranged trades toughness for reach,
    /// cavalry trades defense for mobility, and siege hits hardest but folds
    /// once anything reaches it.
    pub fn stats(self) -> UnitStats {
        let (max_hp, attack, defense, move_range, attack_range) = match self {
            UnitType::Melee => (100.0, 22.0, 20.0, 1, 1),
            UnitType::Ranged => (75.0, 24.0, 10.0, 1, 2),
            UnitType::Cavalry => (100.0, 24.0, 14.0, 2, 1),
            UnitType::Siege => (65.0, 32.0, 6.0, 1, 2),
        };
        UnitStats {
            max_hp,
            attack,
            defense,
            move_range,
            attack_range,
        }
    }

    pub fn letter(self) -> char {
        match self {
            UnitType::Melee => 'M',
            UnitType::Ranged => 'R',
            UnitType::Cavalry => 'C',
            UnitType::Siege => 'S',
        }
    }
}

pub struct Unit {
    pub id: u32,
    pub pos: Hex,
    pub team: Team,
    pub unit_type: UnitType,
    pub hp: f32,
    /// Orders queued for this turn. The attack targets a hex rather than a
    /// unit: whichever enemy stands there when it resolves gets hit.
    pub planned_move: Option<Hex>,
    pub planned_attack: Option<Hex>,
    /// Whether the unit's ability is queued for this turn.
    pub ability_queued: bool,
    /// Turns left before the ability can be used again.
    pub ability_cooldown: u32,
    /// Siege only: set up, with extra range but unable to move.
    pub deployed: bool,
    /// The player chose to hold this unit, leaving any move or attack it
    /// hasn't queued unused this turn.
    pub holding: bool,
}

impl Unit {
    pub fn new(id: u32, pos: Hex, team: Team, unit_type: UnitType) -> Self {
        Self {
            id,
            pos,
            team,
            unit_type,
            hp: unit_type.stats().max_hp,
            planned_move: None,
            planned_attack: None,
            ability_queued: false,
            ability_cooldown: 0,
            deployed: false,
            holding: false,
        }
    }

    pub fn ability(&self) -> Ability {
        Ability::of(self.unit_type)
    }

    /// The unit's stats with its queued ability and siege deployment applied.
    pub fn stats(&self) -> UnitStats {
        let mut stats = self.unit_type.stats();
        if self.ability_queued {
            match self.ability() {
                Ability::ShieldWall => {
                    stats.defense *= SHIELD_WALL_DEFENSE;
                    stats.move_range = 0;
                }
                Ability::Charge => {
                    stats.attack *= CHARGE_ATTACK;
                    stats.move_range += CHARGE_EXTRA_MOVE;
                }
                // Busy setting up or packing up.
                Ability::Deploy => stats.move_range = 0,
                Ability::Volley => {}
            }
        }
        if self.deployed {
            stats.move_range = 0;
            stats.attack_range += DEPLOYED_EXTRA_RANGE;
        }
        stats
    }

    /// Siege spends the turn it sets up or packs up unable to attack.
    pub fn can_attack(&self) -> bool {
        !(self.ability_queued && self.ability() == Ability::Deploy)
    }

    /// Where the unit will be once its queued move (if any) resolves.
    pub fn planned_pos(&self) -> Hex {
        self.planned_move.unwrap_or(self.pos)
    }

    /// Drops the queued attack if the unit can no longer make it, e.g. after
    /// the move that brought it into range was changed.
    pub fn drop_unreachable_attack(&mut self) {
        if let Some(target) = self.planned_attack
            && (!self.can_attack()
                || self.planned_pos().distance(target) > self.stats().attack_range)
        {
            self.planned_attack = None;
        }
    }

    pub fn clear_orders(&mut self) {
        self.planned_move = None;
        self.planned_attack = None;
        self.ability_queued = false;
        self.holding = false;
    }

    /// End-of-turn bookkeeping: puts a used ability on cooldown (or ticks the
    /// cooldown down), completes a siege setup or pack-up, and clears orders.
    pub fn end_turn(&mut self) {
        if self.ability_queued {
            self.ability_cooldown = self.ability().cooldown();
            if self.ability() == Ability::Deploy {
                self.deployed = !self.deployed;
            }
        } else {
            self.ability_cooldown = self.ability_cooldown.saturating_sub(1);
        }
        self.clear_orders();
    }

    pub fn max_hp(&self) -> f32 {
        self.unit_type.stats().max_hp
    }

    pub fn is_alive(&self) -> bool {
        self.hp > 0.0
    }
}

impl fmt::Display for Unit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?} {:?} #{}", self.team, self.unit_type, self.id)
    }
}
