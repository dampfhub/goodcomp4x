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
            Team::Blue => [0.08, 0.60, 0.95, 1.0],
            Team::Red => [0.98, 0.26, 0.12, 1.0],
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UnitType {
    Melee,
    Ranged,
    Cavalry,
    Siege,
    /// Fast and far-sighted, but hardly a fighter: for exploring.
    Scout,
    Armored,
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
    /// once anything reaches it. Scouts give up fighting for speed and sight.
    pub fn stats(self) -> UnitStats {
        let (max_hp, attack, defense, move_range, attack_range) = match self {
            UnitType::Melee => (100.0, 22.0, 20.0, 1, 1),
            UnitType::Ranged => (75.0, 24.0, 10.0, 1, 2),
            UnitType::Cavalry => (100.0, 24.0, 14.0, 2, 1),
            UnitType::Siege => (65.0, 32.0, 6.0, 1, 2),
            UnitType::Scout => (60.0, 8.0, 10.0, 3, 1),
            UnitType::Armored => (140.0, 30.0, 28.0, 1, 1),
        };
        UnitStats {
            max_hp,
            attack,
            defense,
            move_range,
            attack_range,
        }
    }

    /// How many hexes around it the unit sees through the fog of war.
    pub fn sight(self) -> i32 {
        match self {
            UnitType::Scout | UnitType::Cavalry => 3,
            _ => 2,
        }
    }
}

/// One later turn in a unit's order queue (Shift-click, `order_queue.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct TurnOrder {
    /// Where the unit should stand when this turn starts. If it isn't there
    /// (its earlier move was blocked, say), the rest of the queue is dropped.
    pub from: Hex,
    pub move_to: Option<Hex>,
    pub attack: Option<Hex>,
}

impl TurnOrder {
    /// Where the unit stands once this turn's move is done.
    pub fn end_pos(&self) -> Hex {
        self.move_to.unwrap_or(self.from)
    }
}

#[derive(Clone)]
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
    /// Like `holding`, but lasting across turns: the unit stays put and is
    /// skipped in the turn order until it's given an order or unguarded.
    pub guarding: bool,
    /// Scout only: spent last turn on lookout, so it sees farther until the
    /// end of this one.
    pub lookout: bool,
    /// Orders for the turns after this one, in turn order: each turn's end
    /// moves the first of them into `planned_move` and `planned_attack`.
    pub queued: Vec<TurnOrder>,
    /// This turn's orders came from a queue the player built with Shift, so
    /// the unit doesn't hold up ending the turn.
    pub following_queue: bool,
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
            guarding: false,
            lookout: false,
            queued: Vec::new(),
            following_queue: false,
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
                // Standing still, watching.
                Ability::Lookout => stats.move_range = 0,
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

    /// The unit's stats in the turns after this one, for planning its queue:
    /// no ability queued, and deployed if it sets up this turn (or packed up
    /// if it packs up).
    pub fn later_stats(&self) -> UnitStats {
        let mut stats = self.unit_type.stats();
        let deploying = self.ability_queued && self.ability() == Ability::Deploy;
        if self.deployed != deploying {
            stats.move_range = 0;
            stats.attack_range += DEPLOYED_EXTRA_RANGE;
        }
        stats
    }

    /// Whether the unit is following a queue of orders built with Shift.
    pub fn has_queue(&self) -> bool {
        self.following_queue || !self.queued.is_empty()
    }

    /// How many turns the unit has orders for: none, this turn, or this turn
    /// and its queue. A queued turn may be spent waiting.
    pub fn plan_len(&self) -> usize {
        if self.has_queue() {
            1 + self.queued.len()
        } else {
            usize::from(self.planned_move.is_some() || self.planned_attack.is_some())
        }
    }

    /// Where the unit will stand after the first `turns` turns of its plan
    /// (where it stands now for 0; where its plan ends past its last turn).
    pub fn pos_after(&self, turns: usize) -> Hex {
        match turns {
            0 => self.pos,
            1 => self.planned_pos(),
            n => self
                .queued
                .get(n - 2)
                .or(self.queued.last())
                .map_or(self.planned_pos(), TurnOrder::end_pos),
        }
    }

    /// The attack planned for turn `turn` of the unit's plan (0 is this turn).
    pub fn attack_on_turn(&self, turn: usize) -> Option<Hex> {
        match turn {
            0 => self.planned_attack,
            n => self.queued.get(n - 1).and_then(|order| order.attack),
        }
    }

    /// Where the unit's plan leaves it.
    pub fn plan_end(&self) -> Hex {
        self.queued
            .last()
            .map_or(self.planned_pos(), TurnOrder::end_pos)
    }

    /// Drops the turns queued after this one; this turn's orders stay.
    pub fn cancel_queue(&mut self) {
        self.queued.clear();
        self.following_queue = false;
    }

    /// Clears every order: this turn's, the queue, and a hold.
    pub fn clear_orders(&mut self) {
        self.planned_move = None;
        self.planned_attack = None;
        self.ability_queued = false;
        self.holding = false;
        self.cancel_queue();
    }

    /// End-of-turn bookkeeping: puts a used ability on cooldown (or ticks the
    /// cooldown down), completes a siege setup or pack-up, starts or ends a
    /// scout's lookout, and clears this turn's orders. The queue stays, for
    /// `advance_queues` to take the next turn from.
    pub fn end_turn(&mut self) {
        self.lookout = self.ability_queued && self.ability() == Ability::Lookout;
        if self.ability_queued {
            self.ability_cooldown = self.ability().cooldown();
            if self.ability() == Ability::Deploy {
                self.deployed = !self.deployed;
            }
        } else {
            self.ability_cooldown = self.ability_cooldown.saturating_sub(1);
        }
        let queued = std::mem::take(&mut self.queued);
        self.clear_orders();
        self.queued = queued;
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
