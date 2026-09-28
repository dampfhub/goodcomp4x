//! Ruins: one-use tiles on contested ground (`mapgen.rs` places them about
//! as far from the two nearest starts), there to give every side an early
//! reason to fight. A side claims one by holding it with a military unit
//! (anything but a settler, scouts included) at the end of
//! `RUIN_HOLD_TURNS` turns, and gets its reward at once; the ruins are then
//! gone. The count pauses while the hex is contested or empty, and starts
//! over when another side takes it. The rewards are a first pass.

use super::GameState;
use super::city::{Stock, stock_words, turns_icon};
use super::hex::Hex;
use super::unit::{Team, Unit, UnitType};

/// Turn ends a side must hold ruins through to claim them.
pub const RUIN_HOLD_TURNS: u32 = 3;
/// What the Supplies reward adds to the claimant's stockpile.
const SUPPLIES: Stock = Stock::whole(0, 4, 2);
/// What the Harvest reward adds to the claimant's stockpile.
const HARVEST: Stock = Stock::whole(8, 0, 0);

/// What claiming ruins gives.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RuinReward {
    /// A cavalry unit joins the claimant beside the ruins.
    Recruits,
    /// Wood and metal for the claimant's stockpile.
    Supplies,
    /// Food for the claimant's stockpile.
    Harvest,
}

impl RuinReward {
    pub const ALL: [RuinReward; 3] = [
        RuinReward::Recruits,
        RuinReward::Supplies,
        RuinReward::Harvest,
    ];

    /// What it gives, for the tile tooltip.
    pub fn description(self) -> String {
        match self {
            RuinReward::Recruits => "A CAVALRY UNIT".into(),
            RuinReward::Supplies => format!("+{} FOR THE STOCKPILE", stock_words(SUPPLIES)),
            RuinReward::Harvest => format!("+{} FOR THE STOCKPILE", stock_words(HARVEST)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Ruin {
    pub pos: Hex,
    pub reward: RuinReward,
    /// The side whose count is running, if any has held the ruins yet.
    pub holder: Option<Team>,
    /// Turn ends `holder` has held the ruins through.
    pub held: u32,
}

impl Ruin {
    pub fn new(pos: Hex, reward: RuinReward) -> Self {
        Self {
            pos,
            reward,
            holder: None,
            held: 0,
        }
    }
}

impl GameState {
    /// The ruins on `hex`, if any are left.
    pub(super) fn ruin_at(&self, hex: Hex) -> Option<&Ruin> {
        self.ruins.iter().find(|r| r.pos == hex)
    }

    /// The tile tooltip's lines about ruins on `hex`: what they give, and
    /// who has held them how long. Out of sight (`remembered`), only that
    /// they were there when last seen.
    pub(super) fn ruin_notes(&self, hex: Hex, remembered: bool) -> Vec<String> {
        if remembered {
            let seen = self.memory.get(&hex).is_some_and(|seen| seen.ruin);
            return if seen {
                vec!["RUINS, AS LAST SEEN".into()]
            } else {
                Vec::new()
            };
        }
        let Some(ruin) = self.ruin_at(hex) else {
            return Vec::new();
        };
        let mut notes = vec![format!(
            "RUINS: HOLD {} WITH A MILITARY UNIT FOR {}",
            turns_icon(RUIN_HOLD_TURNS as i32),
            ruin.reward.description()
        )];
        if let Some(team) = ruin.holder {
            notes.push(
                format!("{team:?} HAS HELD THEM {}/{RUIN_HOLD_TURNS}", ruin.held).to_uppercase(),
            );
        }
        notes
    }

    /// The side holding `hex` with military units, alone: `None` if it's
    /// empty, contested, or held only by a settler.
    fn sole_holder(&self, hex: Hex) -> Option<Option<Team>> {
        let mut teams = self
            .units
            .iter()
            .filter(|u| u.pos == hex && !self.settlers.contains(&u.id))
            .map(|u| u.team);
        let first = teams.next();
        match first {
            None => Some(None),
            Some(team) if teams.all(|t| t == team) => Some(Some(team)),
            // Contested.
            Some(_) => None,
        }
    }

    /// At the end of a turn: counts a turn for every ruins held by one side
    /// alone, starting over for a side that has just taken them from another,
    /// and hands out the reward for any held long enough. Contested or empty
    /// ruins keep their count.
    pub(super) fn resolve_ruins(&mut self) {
        let mut claimed = Vec::new();
        for i in 0..self.ruins.len() {
            let Some(Some(team)) = self.sole_holder(self.ruins[i].pos) else {
                continue;
            };
            let ruin = &mut self.ruins[i];
            if ruin.holder == Some(team) {
                ruin.held += 1;
            } else {
                ruin.holder = Some(team);
                ruin.held = 1;
            }
            log::info!(
                "{team:?} holds the ruins at ({}, {}): {}/{RUIN_HOLD_TURNS}",
                ruin.pos.q,
                ruin.pos.r,
                ruin.held
            );
            if ruin.held >= RUIN_HOLD_TURNS {
                claimed.push(i);
            }
        }
        for i in claimed.into_iter().rev() {
            let ruin = self.ruins.remove(i);
            self.claim_ruin(&ruin);
        }
    }

    /// Gives `ruin`'s holder its reward. Goods go to the side's stockpile;
    /// a side without a city gets the Recruits instead, and Recruits with no
    /// room beside the ruins give nothing.
    fn claim_ruin(&mut self, ruin: &Ruin) {
        let Some(team) = ruin.holder else { return };
        let reward = if self.cities.iter().any(|c| c.team == team) {
            ruin.reward
        } else {
            RuinReward::Recruits
        };
        let what = match reward {
            RuinReward::Supplies | RuinReward::Harvest => {
                let goods = if reward == RuinReward::Supplies {
                    SUPPLIES
                } else {
                    HARVEST
                };
                *self.stock_mut(team) += goods;
                format!("+{} FOR THE STOCKPILE", stock_words(goods))
            }
            RuinReward::Recruits => {
                let spot = ruin
                    .pos
                    .neighbors()
                    .into_iter()
                    .chain([ruin.pos])
                    .find(|&h| {
                        self.grid.is_passable(h)
                            && !self.is_occupied(h)
                            && self.spawn_clear_of_enemy_civilians(h, team)
                    });
                match spot {
                    Some(spot) => {
                        let id = self.next_unit_id;
                        self.next_unit_id += 1;
                        self.units
                            .push(Unit::new(id, spot, team, UnitType::Cavalry));
                        "A CAVALRY UNIT JOINS YOU".into()
                    }
                    None => "BUT THERE WAS NO ROOM FOR RECRUITS".into(),
                }
            }
        };
        log::info!(
            "{team:?} claims the ruins at ({}, {}): {what}",
            ruin.pos.q,
            ruin.pos.r
        );
        if team == self.local_team {
            self.notice = format!("RUINS CLAIMED - {what}");
        } else if self.fog().sees(ruin.pos) {
            self.notice = format!("{team:?} CLAIMED THE RUINS").to_uppercase();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::city::City;

    /// The combat scenario with no units, one ruin at the center and a city
    /// for each side far from it.
    fn ruins_game(reward: RuinReward) -> GameState {
        let mut game = GameState::new();
        game.units.clear();
        game.selected = None;
        game.ruins = vec![Ruin::new(Hex::new(0, 0), reward)];
        game
    }

    fn put(game: &mut GameState, id: u32, hex: Hex, team: Team, unit_type: UnitType) {
        game.units.push(Unit::new(id, hex, team, unit_type));
    }

    fn end_turns(game: &mut GameState, turns: u32) {
        for _ in 0..turns {
            game.resolve_ruins();
        }
    }

    #[test]
    fn recruits_skip_an_enemy_city_and_field_worker() {
        let mut game = ruins_game(RuinReward::Recruits);
        let ruin = game.ruins[0].pos;
        let open: Vec<_> = ruin
            .neighbors()
            .into_iter()
            .filter(|&hex| game.grid.is_passable(hex))
            .collect();
        assert!(open.len() >= 3);
        game.cities.push(City::new(0, Team::Red, open[0]));
        game.field_workers.push(crate::game::workers::FieldWorker {
            id: 1000,
            team: Team::Red,
            home: 0,
            base: open[0],
            pos: open[1],
            job: None,
            work_left: None,
            recalled: false,
        });
        game.ruins[0].holder = Some(Team::Blue);
        let reward = game.ruins[0].clone();
        game.claim_ruin(&reward);
        let recruit = game
            .units
            .iter()
            .find(|unit| unit.team == Team::Blue)
            .unwrap();
        assert_ne!(recruit.pos, open[0]);
        assert_ne!(recruit.pos, open[1]);
    }

    #[test]
    fn holding_ruins_for_three_turns_claims_them() {
        let mut game = ruins_game(RuinReward::Recruits);
        put(&mut game, 1, Hex::new(0, 0), Team::Blue, UnitType::Scout);
        end_turns(&mut game, RUIN_HOLD_TURNS - 1);
        assert_eq!(game.ruins[0].held, RUIN_HOLD_TURNS - 1);
        assert_eq!(game.units.len(), 1);
        end_turns(&mut game, 1);
        assert!(game.ruins.is_empty(), "claimed and gone");
        assert_eq!(game.units.len(), 2, "a cavalry unit joined");
        assert_eq!(game.units[1].unit_type, UnitType::Cavalry);
        assert_eq!(game.units[1].team, Team::Blue);
        assert!(game.notice.starts_with("RUINS CLAIMED"), "{}", game.notice);
    }

    #[test]
    fn a_contested_or_empty_ruin_pauses_and_a_new_holder_starts_over() {
        let mut game = ruins_game(RuinReward::Recruits);
        let ruin = Hex::new(0, 0);
        put(&mut game, 1, ruin, Team::Blue, UnitType::Melee);
        end_turns(&mut game, 2);
        assert_eq!(game.ruins[0].held, 2);

        // Contested: the count pauses.
        put(&mut game, 2, ruin, Team::Red, UnitType::Melee);
        end_turns(&mut game, 3);
        assert_eq!(game.ruins[0].holder, Some(Team::Blue));
        assert_eq!(game.ruins[0].held, 2);

        // Red wins the hex: its count starts over.
        game.units.retain(|u| u.team == Team::Red);
        end_turns(&mut game, 1);
        assert_eq!(game.ruins[0].holder, Some(Team::Red));
        assert_eq!(game.ruins[0].held, 1);

        // Left empty, the count holds.
        game.units.clear();
        end_turns(&mut game, 5);
        assert_eq!(game.ruins[0].held, 1);
        assert_eq!(game.ruins.len(), 1);
    }

    #[test]
    fn a_settler_does_not_hold_ruins() {
        let mut game = ruins_game(RuinReward::Recruits);
        put(&mut game, 1, Hex::new(0, 0), Team::Blue, UnitType::Melee);
        game.settlers.insert(1);
        end_turns(&mut game, RUIN_HOLD_TURNS + 1);
        assert_eq!(game.ruins[0].held, 0);
    }

    #[test]
    fn goods_rewards_go_to_the_claimants_stockpile() {
        for (reward, goods) in [
            (RuinReward::Supplies, SUPPLIES),
            (RuinReward::Harvest, HARVEST),
        ] {
            let mut game = ruins_game(reward);
            game.cities.push(City::new(0, Team::Blue, Hex::new(-3, 0)));
            put(&mut game, 1, Hex::new(0, 0), Team::Blue, UnitType::Melee);
            let (blue, red) = (game.stock(Team::Blue), game.stock(Team::Red));
            end_turns(&mut game, RUIN_HOLD_TURNS);
            assert!(game.ruins.is_empty());
            assert_eq!(game.stock(Team::Blue), blue + goods, "{reward:?}");
            assert_eq!(game.stock(Team::Red), red, "only the claimant gains");
        }
    }

    #[test]
    fn the_tooltip_tells_what_ruins_give_and_who_holds_them() {
        let mut game = ruins_game(RuinReward::Harvest);
        let ruin = Hex::new(0, 0);
        let notes = game.ruin_notes(ruin, false);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("HOLD \u{E003}3 "), "{notes:?}");
        assert!(notes[0].contains("FOOD"), "{notes:?}");
        put(&mut game, 1, ruin, Team::Red, UnitType::Melee);
        end_turns(&mut game, 1);
        let notes = game.ruin_notes(ruin, false);
        assert_eq!(notes[1], "RED HAS HELD THEM 1/3");
        assert!(game.ruin_notes(Hex::new(1, 0), false).is_empty());
        // Out of sight, only what was seen: nothing, before anyone looked.
        assert!(game.ruin_notes(ruin, true).is_empty());
    }

    #[test]
    fn a_side_without_a_city_gets_recruits() {
        let mut game = ruins_game(RuinReward::Supplies);
        put(&mut game, 1, Hex::new(0, 0), Team::Red, UnitType::Scout);
        end_turns(&mut game, RUIN_HOLD_TURNS);
        assert!(game.ruins.is_empty());
        assert!(
            game.units
                .iter()
                .any(|u| u.team == Team::Red && u.unit_type == UnitType::Cavalry)
        );
    }
}
