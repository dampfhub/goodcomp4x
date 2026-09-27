//! Order queues: Shift-click plans a unit's (or a group's) orders over
//! several turns. Shift-left-click adds a turn that moves toward the clicked
//! hex; Shift-right-click adds an attack on it. Turn 0 of a plan is the
//! unit's ordinary `planned_move` and `planned_attack`; later turns wait in
//! `Unit::queued`, and each turn's end moves the next one up
//! (`advance_queues`). A unit following a queue doesn't hold up ending the
//! turn, and any other order cancels its queue.

use std::collections::HashSet;

use super::GameState;
use super::fog::Fog;
use super::hex::Hex;
use super::turn::{Phase, step_rank};
use super::unit::{Team, TurnOrder};

impl GameState {
    /// Shift-left-click: adds a turn to the selection's plan in which each
    /// unit moves toward `target`, taking the hex nearest it that it can
    /// reach that turn and no ally will stand on. A group's plans always have
    /// the same number of turns: members that can't get closer wait that
    /// turn. Returns whether anything was queued.
    pub(super) fn queue_move(&mut self, target: Hex) -> bool {
        let members = self.selection();
        if members.is_empty() || self.is_resolving() {
            return false;
        }
        let fog = self.fog();
        let turn = self.plan_length(&members);
        let team = self.units[members[0]].team;
        let allies: Vec<usize> = (0..self.units.len())
            .filter(|&i| self.units[i].team == team)
            .collect();
        // Where every other unit on the side stands once that turn is done,
        // and the enemies in sight: a queue stops next to an enemy, and
        // right-click attacks it.
        let mut claimed: HashSet<Hex> = allies
            .iter()
            .filter(|i| !members.contains(i))
            .map(|&i| self.units[i].pos_after(turn + 1))
            .chain(
                self.units
                    .iter()
                    .filter(|u| u.team != team && fog.sees(u.pos))
                    .map(|u| u.pos),
            )
            .collect();

        // Nearest the target choose first, so nobody ahead is cut off: a
        // leg always ends nearer the target than the unit starts, so it
        // never ends where a member choosing later starts.
        let mut order = members.clone();
        order.sort_by_key(|&i| (self.units[i].pos_after(turn).distance(target), i));
        let mut legs = Vec::new();
        for i in order {
            let unit = &self.units[i];
            let start = unit.pos_after(turn);
            let reachable = if turn == 0 {
                self.known_reachable_hexes(start, unit.stats().move_range, team, &fog)
            } else {
                self.planned_reachable(start, unit.later_stats().move_range, team, &fog)
            };
            // An ally that starts the turn on a hex and moves off it in a
            // later step would still be there when this unit arrives.
            let rank = step_rank(unit.unit_type, Phase::Move);
            let vacated_too_late = |hex: Hex| {
                allies.iter().any(|&j| {
                    let ally = &self.units[j];
                    j != i
                        && ally.pos_after(turn) == hex
                        && step_rank(ally.unit_type, Phase::Move) > rank
                })
            };
            // Staying put wins ties, so nobody shuffles sideways for nothing.
            let best = reachable
                .into_iter()
                .filter(|&hex| hex == start || !(claimed.contains(&hex) || vacated_too_late(hex)))
                .min_by_key(|hex| (hex.distance(target), hex.distance(start), hex.q, hex.r));
            let dest = best.filter(|&dest| dest != start);
            claimed.insert(dest.unwrap_or(start));
            legs.push((i, dest));
        }
        if legs.iter().all(|(_, dest)| dest.is_none()) {
            self.notice = "CAN'T GET ANY CLOSER THERE".into();
            return false;
        }
        for (i, dest) in legs {
            self.pad_plan(i, turn);
            self.append_turn(i, dest, None);
        }
        self.notice = queued_notice(turn);
        true
    }

    /// Shift-right-click: adds an attack on `target` to the selection's
    /// plan. It goes into the plan's last turn if nobody who could make it
    /// there attacks anything yet; otherwise into a new turn, which members
    /// out of range spend waiting. Returns whether anything was queued.
    pub(super) fn queue_attack(&mut self, target: Hex) -> bool {
        let members = self.selection();
        if members.is_empty() || self.is_resolving() || !self.grid.is_passable(target) {
            return false;
        }
        let len = self.plan_length(&members);
        let fill = len > 0 && {
            let able: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&i| self.can_attack_on_turn(i, len - 1, target))
                .collect();
            !able.is_empty()
                && able
                    .iter()
                    .all(|&i| self.units[i].attack_on_turn(len - 1).is_none())
        };
        let attackers_on = |turn: usize| -> Vec<usize> {
            members
                .iter()
                .copied()
                .filter(|&i| self.can_attack_on_turn(i, turn, target))
                .collect()
        };
        let mut turn = if fill { len - 1 } else { len };
        let mut attackers = attackers_on(turn);
        // With nothing planned yet and no attack possible this turn (siege
        // setting up, say), the attack waits for the next.
        if attackers.is_empty() && turn == 0 {
            turn = 1;
            attackers = attackers_on(turn);
        }
        if attackers.is_empty() {
            self.notice = "OUT OF RANGE THERE".into();
            return false;
        }
        for &i in &members {
            self.pad_plan(i, if fill { turn + 1 } else { turn });
            let attack = attackers.contains(&i).then_some(target);
            if fill {
                if let Some(attack) = attack {
                    self.set_attack_on_turn(i, turn, attack);
                }
            } else {
                self.append_turn(i, None, attack);
            }
        }
        self.notice = queued_notice(turn);
        true
    }

    /// The longest plan among `members`, in turns: the turn a group's next
    /// queued order goes in.
    fn plan_length(&self, members: &[usize]) -> usize {
        members
            .iter()
            .map(|&i| self.units[i].plan_len())
            .max()
            .unwrap_or(0)
    }

    /// Extends unit `idx`'s plan with turns spent waiting until it has
    /// `turns` of them, so it lines up with the rest of its group.
    fn pad_plan(&mut self, idx: usize, turns: usize) {
        let unit = &mut self.units[idx];
        while unit.plan_len() < turns {
            if unit.plan_len() == 0 {
                unit.following_queue = true;
            } else {
                let from = unit.plan_end();
                unit.queued.push(TurnOrder {
                    from,
                    move_to: None,
                    attack: None,
                });
            }
        }
    }

    /// Adds a turn to the end of unit `idx`'s plan: this turn, if it has no
    /// orders yet.
    fn append_turn(&mut self, idx: usize, move_to: Option<Hex>, attack: Option<Hex>) {
        let unit = &mut self.units[idx];
        if unit.plan_len() == 0 {
            unit.planned_move = move_to;
            unit.planned_attack = attack;
        } else {
            let from = unit.plan_end();
            unit.queued.push(TurnOrder {
                from,
                move_to,
                attack,
            });
        }
        unit.following_queue = true;
        unit.guarding = false;
    }

    fn set_attack_on_turn(&mut self, idx: usize, turn: usize, target: Hex) {
        let unit = &mut self.units[idx];
        match turn {
            0 => unit.planned_attack = Some(target),
            n => unit.queued[n - 1].attack = Some(target),
        }
        unit.following_queue = true;
        unit.guarding = false;
    }

    /// Whether unit `idx` could attack `target` on turn `turn` of its plan,
    /// from where the plan has it standing then.
    fn can_attack_on_turn(&self, idx: usize, turn: usize, target: Hex) -> bool {
        let unit = &self.units[idx];
        let (able, range) = if turn == 0 {
            (
                unit.can_attack() && self.rival_of(idx).is_none(),
                unit.stats().attack_range,
            )
        } else {
            (true, unit.later_stats().attack_range)
        };
        able && unit.pos_after(turn + 1).distance(target) <= range
    }

    /// Hexes a unit of `team` could reach from `start` in a later turn, as
    /// far as the player knows: around terrain, walls and gates, but not
    /// units, which will have moved by then.
    fn planned_reachable(
        &self,
        start: Hex,
        move_range: i32,
        team: Team,
        fog: &Fog,
    ) -> HashSet<Hex> {
        self.reachable_hexes_by(start, move_range, |from, to| {
            self.can_enter(to) && self.known_can_cross(from, to, team, fog)
        })
    }

    /// At the end of a turn, after each unit's `end_turn`: every unit with a
    /// queue takes its next turn's orders. A unit whose queued turn no longer
    /// fits the board (it isn't where the queue expects, it's in a contested
    /// hex, its way or destination is blocked, its target out of range)
    /// drops its whole queue and needs orders again.
    pub(super) fn advance_queues(&mut self) {
        let mut promoted = Vec::new();
        for (i, unit) in self.units.iter_mut().enumerate() {
            if unit.queued.is_empty() {
                continue;
            }
            let next = unit.queued.remove(0);
            unit.planned_move = next.move_to;
            unit.planned_attack = next.attack;
            unit.following_queue = true;
            promoted.push((i, next.from));
        }
        if promoted.is_empty() {
            return;
        }
        let fog = self.fog();
        // Dropping one unit's orders leaves it standing where an ally may
        // have meant to go, so check again until nothing changes.
        loop {
            let mut changed = false;
            for &(i, from) in &promoted {
                if !self.units[i].following_queue {
                    continue;
                }
                if let Some(reason) = self.queued_turn_problem(i, from, &fog) {
                    let unit = &self.units[i];
                    log::info!("{unit} drops its queued orders: {reason}");
                    self.notice = format!("{} STOPPED: {reason}", self.unit_role(unit));
                    self.units[i].clear_orders();
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
    }

    /// Why unit `idx` can't carry out the queued turn it just took, if it
    /// can't. `from` is where the queue expected it to start the turn.
    fn queued_turn_problem(&self, idx: usize, from: Hex, fog: &Fog) -> Option<&'static str> {
        let unit = &self.units[idx];
        if unit.pos != from {
            return Some("IT DIDN'T GET WHERE IT WAS GOING");
        }
        if self.rival_of(idx).is_some() {
            return Some("IT IS FIGHTING FOR ITS HEX");
        }
        if let Some(dest) = unit.planned_move {
            let reachable =
                self.planned_reachable(unit.pos, unit.stats().move_range, unit.team, fog);
            if !reachable.contains(&dest) {
                return Some("ITS WAY IS BLOCKED");
            }
            if fog.sees(dest) && self.enemy_of_team_at(dest, unit.team).is_some() {
                return Some("AN ENEMY STANDS IN ITS WAY");
            }
            let ally_there = self.units.iter().enumerate().any(|(j, other)| {
                j != idx && other.team == unit.team && other.planned_pos() == dest
            });
            if ally_there {
                return Some("AN ALLY IS IN ITS WAY");
            }
        }
        if let Some(target) = unit.planned_attack
            && (!unit.can_attack()
                || unit.planned_pos().distance(target) > unit.stats().attack_range)
        {
            return Some("ITS TARGET IS OUT OF RANGE");
        }
        None
    }
}

/// What the top bar says after a Shift-click queued turn `turn` (0 is this one).
fn queued_notice(turn: usize) -> String {
    match turn {
        0 => "QUEUED FOR THIS TURN - SHIFT-CLICK AGAIN FOR THE NEXT".into(),
        n => format!("QUEUED FOR TURN {} FROM NOW", n + 1),
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;
    use crate::game::orders::ClickMode;
    use crate::game::unit::{Unit, UnitType};

    const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

    /// An open radius-6 map with no fog, no AI units and one Blue unit of
    /// each `types` in a row from (-4, 0), for queuing without interference.
    fn open_field(types: &[UnitType]) -> GameState {
        let mut game = GameState::city_scenario();
        game.fog_of_war = false;
        game.units.clear();
        game.cities.clear();
        game.field_workers.clear();
        game.selected = None;
        game.group.clear();
        for (n, &unit_type) in types.iter().enumerate() {
            let pos = Hex::new(-4, n as i32);
            game.units
                .push(Unit::new(100 + n as u32, pos, Team::Blue, unit_type));
        }
        game
    }

    /// Ends planning and plays the whole turn out at once.
    fn play_turn(game: &mut GameState) {
        game.instant_playback = true;
        game.end_planning();
        game.update(0.0);
        assert!(!game.is_resolving());
    }

    fn cursor(game: &GameState, hex: Hex) -> Vec2 {
        game.camera.world_to_screen(hex.to_world(), SCREEN)
    }

    #[test]
    fn shift_clicks_build_a_queue_the_unit_follows_turn_by_turn() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        let path = [Hex::new(-3, 0), Hex::new(-2, 0), Hex::new(-1, 0)];
        for hex in path {
            g.handle_map_click(cursor(&g, hex), SCREEN, ClickMode::QueueMove);
        }
        let unit = &g.units[0];
        assert_eq!(unit.planned_move, Some(path[0]));
        assert_eq!(unit.queued.len(), 2);
        assert_eq!(unit.plan_end(), path[2]);
        assert_eq!(g.selected, Some(0), "queuing never moves selection on");

        for (turn, &hex) in path.iter().enumerate() {
            play_turn(&mut g);
            assert_eq!(g.units[0].pos, hex, "turn {turn}");
        }
        assert!(!g.units[0].has_queue(), "the queue is used up");
        assert!(g.needs_orders(0), "and then the unit wants orders again");
    }

    #[test]
    fn a_far_shift_click_moves_one_turn_toward_it() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        let far = Hex::new(2, 0);
        assert!(g.queue_move(far));
        assert!(g.queue_move(far));
        let unit = &g.units[0];
        assert_eq!(unit.plan_len(), 2);
        assert_eq!(unit.plan_end().distance(far), unit.pos.distance(far) - 2);
    }

    #[test]
    fn a_queue_toward_an_enemy_in_sight_stops_next_to_it() {
        let mut g = open_field(&[UnitType::Melee]);
        let enemy = Hex::new(-1, 0);
        g.units
            .push(Unit::new(200, enemy, Team::Red, UnitType::Melee));
        g.selected = Some(0);
        assert!(g.queue_move(enemy));
        assert!(g.queue_move(enemy));
        assert!(!g.queue_move(enemy), "already next to it");
        assert_eq!(g.units[0].plan_end().distance(enemy), 1);
        assert!(g.queue_attack(enemy));
        assert_eq!(g.units[0].queued.last().unwrap().attack, Some(enemy));
    }

    #[test]
    fn a_queued_attack_joins_the_move_of_its_turn_or_starts_a_new_one() {
        let mut g = open_field(&[UnitType::Ranged]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        // In range of (-3, 0): fills this turn's attack.
        let target = Hex::new(-1, 0);
        assert!(g.queue_attack(target));
        assert_eq!(g.units[0].planned_attack, Some(target));
        assert!(g.units[0].queued.is_empty());
        // Again: this turn already attacks, so next turn attacks from there.
        assert!(g.queue_attack(target));
        let next = g.units[0].queued[0];
        assert_eq!(next.attack, Some(target));
        assert_eq!(next.move_to, None);
        assert_eq!(next.from, Hex::new(-3, 0));
        // Out of range of where the plan ends: nothing.
        assert!(!g.queue_attack(Hex::new(3, 0)));
        assert_eq!(g.units[0].plan_len(), 2);
    }

    #[test]
    fn right_click_attacks_and_left_click_only_moves() {
        let mut g = open_field(&[UnitType::Melee]);
        g.units
            .push(Unit::new(200, Hex::new(-3, 0), Team::Red, UnitType::Melee));
        g.selected = Some(0);
        let enemy = Hex::new(-3, 0);
        g.handle_map_click(cursor(&g, enemy), SCREEN, ClickMode::Normal);
        assert_eq!(g.units[0].planned_attack, None, "left click doesn't attack");
        assert_eq!(g.units[0].planned_move, None);
        g.handle_context_click(cursor(&g, enemy), SCREEN, false, false);
        assert_eq!(g.units[0].planned_attack, Some(enemy));
        // Right-clicking an empty hex in range attacks it too.
        g.handle_context_click(cursor(&g, Hex::new(-4, 1)), SCREEN, false, false);
        assert_eq!(g.units[0].planned_attack, Some(Hex::new(-4, 1)));
        g.handle_map_click(cursor(&g, Hex::new(-5, 0)), SCREEN, ClickMode::Normal);
        assert_eq!(g.units[0].planned_move, Some(Hex::new(-5, 0)));
    }

    #[test]
    fn shift_right_click_queues_an_attack() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        g.handle_context_click(cursor(&g, Hex::new(-3, 0)), SCREEN, false, true);
        assert_eq!(g.units[0].planned_attack, Some(Hex::new(-3, 0)));
        assert!(g.units[0].following_queue);
        g.handle_context_click(cursor(&g, Hex::new(-3, 0)), SCREEN, false, true);
        assert_eq!(g.units[0].queued.len(), 1);
    }

    #[test]
    fn a_queued_unit_does_not_hold_up_the_turn() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Ranged]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        assert!(
            !g.needs_orders(0),
            "no attack queued, but it follows its queue"
        );
        g.selected = Some(1);
        g.hold_selected_unit();
        assert_eq!(g.pending(), (0, 0));

        // Next turn it follows the queue's second turn and still doesn't wait
        // for orders, while the unit that held does.
        play_turn(&mut g);
        assert_eq!(g.units[0].planned_move, Some(Hex::new(-2, 0)));
        assert!(!g.needs_orders(0));
        assert!(g.needs_orders(1));
        assert_eq!(
            g.selected,
            Some(1),
            "the turn starts on the unit without orders"
        );
    }

    #[test]
    fn any_other_order_cancels_the_queue() {
        type Order = fn(&mut GameState);
        let orders: [(&str, Order); 6] = [
            ("click a move", |g| {
                g.handle_map_click(cursor(g, Hex::new(-5, 0)), SCREEN, ClickMode::Normal)
            }),
            ("right-click an attack", |g| {
                g.handle_context_click(cursor(g, Hex::new(-3, 1)), SCREEN, false, false)
            }),
            ("guard", |g| g.toggle_guard()),
            ("ability", |g| g.toggle_selected_ability()),
            ("ctrl-right-click", |g| g.handle_right_click()),
            ("swap", |g| {
                g.handle_map_click(cursor(g, Hex::new(-4, 1)), SCREEN, ClickMode::Swap)
            }),
        ];
        for (name, order) in orders {
            let mut g = open_field(&[UnitType::Melee, UnitType::Melee]);
            g.selected = Some(0);
            assert!(g.queue_move(Hex::new(-3, 0)));
            assert!(g.queue_move(Hex::new(-2, 0)));
            order(&mut g);
            assert!(!g.units[0].has_queue(), "{name} keeps the queue");
        }

        // Holding isn't an order: the queue stays.
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        g.hold_selected_unit();
        assert_eq!(g.units[0].queued.len(), 1);
    }

    #[test]
    fn a_group_queues_the_same_number_of_turns_for_every_member() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Cavalry, UnitType::Siege]);
        // One member already has two turns of its own queued.
        g.selected = Some(1);
        assert!(g.queue_move(Hex::new(-2, 1)));
        assert!(g.queue_move(Hex::new(0, 1)));
        g.set_selection(vec![0, 1, 2]);
        let far = Hex::new(4, 0);
        for _ in 0..3 {
            assert!(g.queue_move(far));
            let lengths: Vec<usize> = (0..3).map(|i| g.units[i].plan_len()).collect();
            assert!(
                lengths.iter().all(|&len| len == lengths[0]),
                "plans differ: {lengths:?}"
            );
        }
        assert_eq!(g.units[0].plan_len(), 5);
        // The others waited two turns for the cavalry, then set off.
        assert_eq!(g.units[0].planned_move, None);
        assert_eq!(g.units[0].queued[0].move_to, None);
        assert!(g.units[0].queued[1].move_to.is_some());

        // An attack only some can make is still a turn for all of them.
        let end = g.units[2].plan_end();
        let target = Hex::new(end.q + 2, end.r);
        assert!(g.grid.is_passable(target));
        assert!(
            g.units[0].plan_end().distance(target) > 1,
            "out of melee reach"
        );
        assert!(g.queue_attack(target));
        let lengths: Vec<usize> = (0..3).map(|i| g.units[i].plan_len()).collect();
        assert!(lengths.iter().all(|&len| len == lengths[0]), "{lengths:?}");

        // No two members plan to end any turn on the same hex.
        for turn in 1..=g.units[0].plan_len() {
            let ends: HashSet<Hex> = (0..3).map(|i| g.units[i].pos_after(turn)).collect();
            assert_eq!(ends.len(), 3, "turn {turn}");
        }
    }

    #[test]
    fn a_group_marches_its_queue_in_step() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Melee, UnitType::Ranged]);
        g.set_selection(vec![0, 1, 2]);
        let far = Hex::new(3, 1);
        for _ in 0..4 {
            assert!(g.queue_move(far));
        }
        let planned: Vec<Hex> = (0..3).map(|i| g.units[i].plan_end()).collect();
        for _ in 0..4 {
            play_turn(&mut g);
        }
        let reached: Vec<Hex> = (0..3).map(|i| g.units[i].pos).collect();
        assert_eq!(reached, planned, "every member followed its queue");
    }

    #[test]
    fn a_blocked_queue_is_dropped_and_the_unit_wants_orders() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        assert!(g.queue_move(Hex::new(-1, 0)));
        // An ally arrives on next turn's destination and stays there.
        g.units
            .push(Unit::new(200, Hex::new(-2, 0), Team::Blue, UnitType::Melee));
        play_turn(&mut g);
        assert_eq!(g.units[0].pos, Hex::new(-3, 0));
        assert!(!g.units[0].has_queue());
        assert_eq!(g.units[0].planned_move, None);
        assert!(g.needs_orders(0));
        assert!(g.notice.contains("STOPPED"), "{}", g.notice);
    }

    #[test]
    fn an_enemy_seen_on_the_way_stops_the_queue() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        // A Red unit the AI leaves alone, standing on next turn's destination.
        g.units
            .push(Unit::new(200, Hex::new(-2, 0), Team::Red, UnitType::Melee));
        g.player_controlled_units.insert(200);
        play_turn(&mut g);
        assert_eq!(g.units[0].pos, Hex::new(-3, 0));
        assert!(!g.units[0].has_queue());
        assert!(g.notice.contains("ENEMY"), "{}", g.notice);
    }

    #[test]
    fn a_unit_that_misses_its_step_drops_the_rest() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        // Someone stands on this turn's destination, so the move bounces.
        g.units
            .push(Unit::new(200, Hex::new(-3, 0), Team::Red, UnitType::Melee));
        g.fog_of_war = false;
        play_turn(&mut g);
        assert_eq!(g.units[0].pos, Hex::new(-4, 0));
        assert!(!g.units[0].has_queue());
    }

    #[test]
    fn a_queue_survives_a_savestate() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        g.save_state();
        g.units[0].clear_orders();
        g.load_state();
        assert_eq!(g.units[0].queued.len(), 1);
        assert!(g.units[0].following_queue);
    }

    #[test]
    fn a_deploying_siege_plans_its_queue_as_deployed() {
        let mut g = open_field(&[UnitType::Siege]);
        g.selected = Some(0);
        g.toggle_selected_ability();
        assert!(g.units[0].ability_queued);
        // Setting up is all it can do this turn, so selection moved on.
        g.selected = Some(0);
        // Deployed next turn: it can't move, but reaches three hexes.
        assert!(!g.queue_move(Hex::new(-2, 0)));
        let target = Hex::new(-1, 0);
        assert!(g.queue_attack(target), "range 3 once deployed");
        assert_eq!(g.units[0].planned_attack, None, "no attack while deploying");
        assert_eq!(g.units[0].queued[0].attack, Some(target));
        assert!(g.units[0].ability_queued, "queuing keeps the ability");
    }
}
