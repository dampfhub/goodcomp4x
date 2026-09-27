//! Order queues: Shift-click plans a unit's (or a group's) orders over
//! several turns. Shift-left-click adds the turns it takes to walk to the
//! clicked hex; Shift-right-click adds an attack on it. Turn 0 of a plan is the
//! unit's ordinary `planned_move` and `planned_attack`; later turns wait in
//! `Unit::queued`, and each turn's end moves the next one up
//! (`advance_queues`). A unit following a queue doesn't hold up ending the
//! turn, and any other order cancels its queue.

use std::collections::{HashMap, HashSet, VecDeque};

use super::GameState;
use super::fog::Fog;
use super::hex::Hex;
use super::turn::{Phase, step_rank};
use super::unit::{Team, TurnOrder, Unit};

impl GameState {
    /// Shift-left-click: adds as many turns to the selection's plan as it
    /// takes to get to `target`, as long as no member's plan grows past
    /// `Settings::max_queued_turns` turns in all (this one included). Each
    /// turn every unit moves to the hex it can reach that turn, with no ally
    /// standing on it, that is the shortest walk from `target` (around
    /// terrain and known walls), until nobody can get any closer or every
    /// member's plan is full. A hex farther away is queued as far as the
    /// limit goes. Each member continues from the end of its own plan, so a
    /// unit added to a group with queues starts moving this turn instead of
    /// waiting for the others' queues to run out; what the others already
    /// had queued stays as it was. Afterwards every member's plan has the
    /// same number of turns: those that arrive first, can't get closer or
    /// reach the limit sooner wait at the end. Returns whether anything was
    /// queued.
    pub(super) fn queue_move(&mut self, target: Hex) -> bool {
        let members = self.selection();
        if members.is_empty() || self.is_resolving() {
            return false;
        }
        let fog = self.fog();
        let team = self.units[members[0]].team;
        let walk = self.planned_walk_to(target, team, &fog);
        let before: Vec<Unit> = members.iter().map(|&i| self.units[i].clone()).collect();
        // No member's plan grows past `limit` turns.
        let limit = self.settings.max_queued_turns.max(1);
        let first = before.iter().map(Unit::plan_len).min().unwrap_or(0);
        let longest = self.plan_length(&members);
        let mut moved_any = false;
        let mut turn = first;
        loop {
            // Members whose plans end here take this turn, unless their plans
            // are full; the rest are still busy with what they had queued,
            // and hold their hexes.
            let active: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&i| self.units[i].plan_len() == turn && turn < limit)
                .collect();
            if active.is_empty() {
                if turn >= longest {
                    break;
                }
                turn += 1;
                continue;
            }
            let legs = self.plan_move_turn(&active, target, turn, &walk, &fog);
            let moved = legs.iter().any(|(_, dest)| dest.is_some());
            if !moved && turn >= longest {
                break;
            }
            for (i, dest) in legs {
                self.append_turn(i, dest, None);
            }
            moved_any |= moved;
            turn += 1;
        }
        if !moved_any {
            for (&i, unit) in members.iter().zip(before) {
                self.units[i] = unit;
            }
            self.notice = if first >= limit {
                format!("QUEUE FULL - {limit}-TURN LIMIT")
            } else {
                "CAN'T GET ANY CLOSER THERE".into()
            };
            return false;
        }
        // Whether a member whose plan is full could still have got closer,
        // checked before padding (which would count as its turns).
        let cut_short = members.iter().any(|&i| {
            let len = self.units[i].plan_len();
            len >= limit
                && self
                    .plan_move_turn(&[i], target, len, &walk, &fog)
                    .iter()
                    .any(|(_, dest)| dest.is_some())
        });
        let length = self.plan_length(&members);
        for &i in &members {
            self.pad_plan(i, length);
        }
        // Cut short, the limit is the news (the plan drawn on the map shows
        // how far it goes), and the notice stays short enough for the top
        // bar of a small window.
        self.notice = if cut_short {
            format!("QUEUED UP TO THE {limit}-TURN LIMIT")
        } else {
            queued_notice(first, length - first)
        };
        true
    }

    /// One turn of a queued move toward `target` for each of `active`, on
    /// turn `turn` of their plans: where each moves, or `None` to wait.
    /// Everyone else on the side (including selected units still busy with
    /// earlier turns) keeps the hex its plan has it on then.
    fn plan_move_turn(
        &self,
        active: &[usize],
        target: Hex,
        turn: usize,
        walk: &HashMap<Hex, i32>,
        fog: &Fog,
    ) -> Vec<(usize, Option<Hex>)> {
        let team = self.units[active[0]].team;
        let allies: Vec<usize> = (0..self.units.len())
            .filter(|&i| self.units[i].team == team)
            .collect();
        // Where every other unit on the side stands once that turn is done,
        // and the enemies in sight: a queue stops next to an enemy, and
        // right-click attacks it.
        let mut claimed: HashSet<Hex> = allies
            .iter()
            .filter(|i| !active.contains(i))
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
        // The shortest walk to the target, or past everything the target
        // can't be walked to from, the straight distance.
        let away = |hex: Hex| {
            (
                walk.get(&hex).copied().unwrap_or(i32::MAX),
                hex.distance(target),
            )
        };
        let mut order = active.to_vec();
        order.sort_by_key(|&i| (away(self.units[i].pos_after(turn)), i));
        let mut legs = Vec::new();
        for i in order {
            let unit = &self.units[i];
            let start = unit.pos_after(turn);
            let reachable = if turn == 0 {
                self.known_reachable_hexes(start, unit.stats().move_range, team, fog)
            } else {
                self.planned_reachable(start, unit.later_stats().move_range, team, fog)
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
                .min_by_key(|&hex| (away(hex), hex.distance(start), hex.q, hex.r));
            let dest = best.filter(|&dest| dest != start);
            claimed.insert(dest.unwrap_or(start));
            legs.push((i, dest));
        }
        legs
    }

    /// How many steps each hex is from `target` for a unit of `team` in a
    /// later turn, as far as the player knows (around terrain, walls and
    /// gates, not units). Hexes with no way to `target` are left out.
    fn planned_walk_to(&self, target: Hex, team: Team, fog: &Fog) -> HashMap<Hex, i32> {
        let mut steps = HashMap::from([(target, 0)]);
        let mut frontier = VecDeque::from([target]);
        while let Some(hex) = frontier.pop_front() {
            let next = steps[&hex] + 1;
            for neighbor in hex.neighbors() {
                if !steps.contains_key(&neighbor)
                    && self.can_enter(neighbor)
                    && self.known_can_cross(neighbor, hex, team, fog)
                {
                    steps.insert(neighbor, next);
                    frontier.push_back(neighbor);
                }
            }
        }
        steps
    }

    /// Shift-right-click: adds an attack on `target` to the selection's
    /// plan. It goes into the plan's last turn if nobody who could make it
    /// there attacks anything yet; otherwise into a new turn, which members
    /// out of range spend waiting, unless that would take the plan past
    /// `Settings::max_queued_turns`. Returns whether anything was queued.
    pub(super) fn queue_attack(&mut self, target: Hex) -> bool {
        let members = self.selection();
        if members.is_empty() || self.is_resolving() || !self.grid.is_passable(target) {
            return false;
        }
        if self.empty_city_target(target, self.units[members[0]].team) {
            self.notice = "CITY CENTER CAN ONLY BE CAPTURED FROM ITS INTERIOR".into();
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
        let limit = self.settings.max_queued_turns.max(1);
        if turn >= limit {
            self.notice = format!("QUEUE FULL - {limit}-TURN LIMIT");
            return false;
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
        self.notice = queued_notice(turn, 1);
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
        unit.holding = false;
    }

    fn set_attack_on_turn(&mut self, idx: usize, turn: usize, target: Hex) {
        let unit = &mut self.units[idx];
        match turn {
            0 => unit.planned_attack = Some(target),
            n => unit.queued[n - 1].attack = Some(target),
        }
        unit.following_queue = true;
        unit.guarding = false;
        unit.holding = false;
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

/// What the top bar says after a Shift-click queued `turns` turns starting
/// on turn `first` (0 is this one).
fn queued_notice(first: usize, turns: usize) -> String {
    let last = first + turns;
    match (first, turns) {
        (0, 1) => "QUEUED FOR THIS TURN - SHIFT-CLICK AGAIN FOR THE NEXT".into(),
        (0, _) => format!("QUEUED FOR THE NEXT {turns} TURNS"),
        (_, 1) => format!("QUEUED FOR TURN {last} FROM NOW"),
        _ => format!("QUEUED FOR TURNS {} TO {last} FROM NOW", first + 1),
    }
}

#[cfg(test)]
mod tests {
    use glam::Vec2;

    use super::*;
    use crate::game::hex::edge;
    use crate::game::orders::ClickMode;
    use crate::game::settings::Setting;
    use crate::game::unit::{Unit, UnitType};
    use crate::game::workers::{Structure, StructureKind};

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
        game.settings.instant_playback = true;
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
    fn one_far_shift_click_queues_every_turn_it_takes_to_get_there() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        // Six hexes: as many turns as a queue holds by default.
        let far = Hex::new(2, 0);
        assert!(g.queue_move(far));
        assert_eq!(g.notice, "QUEUED FOR THE NEXT 6 TURNS", "no limit hit");
        let unit = &g.units[0];
        assert_eq!(unit.plan_len(), unit.pos.distance(far) as usize);
        assert_eq!(unit.plan_end(), far);
        assert!(!g.queue_move(far), "already there");
        for _ in 0..6 {
            play_turn(&mut g);
        }
        assert_eq!(g.units[0].pos, far);
    }

    /// Unit `idx`'s move on each turn of its plan (`None` for a wait).
    fn moves(game: &GameState, idx: usize) -> Vec<Option<Hex>> {
        let unit = &game.units[idx];
        let queued = unit.queued.iter().map(|turn| turn.move_to);
        std::iter::once(unit.planned_move).chain(queued).collect()
    }

    #[test]
    fn a_hex_past_the_limit_is_queued_as_far_as_the_limit_goes() {
        let mut g = open_field(&[UnitType::Melee]);
        assert_eq!(g.settings.max_queued_turns, 6, "the default");
        g.selected = Some(0);
        // Eight hexes away: six turns of it, and then the queue is full.
        let far = Hex::new(4, 0);
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.units[0].plan_end(), Hex::new(2, 0), "along the way");
        assert!(moves(&g, 0).iter().all(Option::is_some), "every turn moves");
        assert_eq!(g.notice, "QUEUED UP TO THE 6-TURN LIMIT");
        assert!(!g.queue_move(far), "no more on another click");
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.notice, "QUEUE FULL - 6-TURN LIMIT");
        // An attack still fits into the last turn, but a second one would
        // need a turn of its own.
        g.units
            .push(Unit::new(99, Hex::new(3, 0), Team::Red, UnitType::Melee));
        assert!(g.queue_attack(Hex::new(3, 0)));
        assert_eq!(g.units[0].plan_len(), 6);
        assert!(!g.queue_attack(Hex::new(3, 0)));
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.notice, "QUEUE FULL - 6-TURN LIMIT");

        // Each turn played frees a turn of the queue.
        g.units.pop();
        play_turn(&mut g);
        assert_eq!(g.units[0].plan_len(), 5);
        g.selected = Some(0);
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.units[0].plan_end(), Hex::new(3, 0));
    }

    #[test]
    fn several_shift_clicks_never_queue_past_the_limit() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        // Short hops, one after another, stop at six turns in all.
        for (hop, x) in (-3..=4).enumerate() {
            let queued = g.queue_move(Hex::new(x, 0));
            assert_eq!(queued, hop < 6, "hop {hop}: {}", g.notice);
            assert!(g.units[0].plan_len() <= 6);
        }
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.units[0].plan_end(), Hex::new(2, 0));
    }

    #[test]
    fn the_limit_is_the_queue_limit_setting() {
        let mut g = open_field(&[UnitType::Melee]);
        g.step_setting(Setting::MaxQueuedTurns, -3);
        assert_eq!(g.settings.max_queued_turns, 3);
        assert_eq!(g.notice, "QUEUE LIMIT: 3 TURNS");
        g.selected = Some(0);
        let far = Hex::new(4, 0);
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 3);
        assert_eq!(g.notice, "QUEUED UP TO THE 3-TURN LIMIT");

        // A higher limit lets the next click go the rest of the way.
        g.step_setting(Setting::MaxQueuedTurns, 20);
        assert_eq!(g.settings.max_queued_turns, 20, "the top of its range");
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 8);
        assert_eq!(g.units[0].plan_end(), far);
        assert!(!g.notice.contains("LIMIT"), "{}", g.notice);

        // At the lowest, only this turn.
        let mut g = open_field(&[UnitType::Melee]);
        g.step_setting(Setting::MaxQueuedTurns, -20);
        assert_eq!(g.settings.max_queued_turns, 1);
        g.selected = Some(0);
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 1);
        assert!(!g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 1);
    }

    #[test]
    fn no_group_member_queues_past_the_limit() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Ranged]);
        g.settings.max_queued_turns = 5;
        // The melee has three turns of its own queued.
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-1, 0)));
        assert_eq!(g.units[0].plan_len(), 3);
        let queued = moves(&g, 0);

        // Both are sent farther than five turns take them.
        g.set_selection(vec![0, 1]);
        let far = Hex::new(5, 0);
        assert!(g.queue_move(far));
        assert_eq!(g.notice, "QUEUED UP TO THE 5-TURN LIMIT");
        // The melee keeps its three turns and adds two; the ranged unit,
        // starting now, moves all five.
        for i in 0..2 {
            assert_eq!(g.units[i].plan_len(), 5, "unit {i}");
        }
        let melee = moves(&g, 0);
        assert_eq!(melee[..3], queued[..], "what it had queued stays");
        assert!(melee[3..].iter().all(Option::is_some), "{melee:?}");
        let ranged = moves(&g, 1);
        assert!(ranged.iter().all(Option::is_some), "{ranged:?}");

        assert!(!g.queue_move(far), "both queues are full");
        assert_eq!(g.notice, "QUEUE FULL - 5-TURN LIMIT");
    }

    #[test]
    fn a_far_shift_click_walks_around_a_wall() {
        let mut g = open_field(&[UnitType::Melee]);
        // A wall from (-2, -1) to (-2, 2) along their east edges, so the
        // straight way east is shut and the unit has to go round.
        for r in -1..=2 {
            for across in [Hex::new(-1, r), Hex::new(-1, r - 1)] {
                let structure = Structure {
                    kind: StructureKind::Wall,
                    team: Team::Red,
                };
                g.barriers.insert(edge(Hex::new(-2, r), across), structure);
            }
        }
        g.selected = Some(0);
        let far = Hex::new(0, 0);
        assert!(g.queue_move(far));
        let unit = &g.units[0];
        assert_eq!(unit.plan_end(), far);
        assert!(
            unit.plan_len() > unit.pos.distance(far) as usize,
            "the detour takes longer than the straight line"
        );
        for turn in 0..unit.plan_len() {
            let (from, to) = (unit.pos_after(turn), unit.pos_after(turn + 1));
            assert!(
                g.can_step(from, to, Team::Blue),
                "turn {turn} crosses the wall"
            );
        }
        let turns = unit.plan_len();
        for _ in 0..turns {
            play_turn(&mut g);
        }
        assert_eq!(g.units[0].pos, far);
    }

    #[test]
    fn a_queue_toward_an_enemy_in_sight_stops_next_to_it() {
        let mut g = open_field(&[UnitType::Melee]);
        let enemy = Hex::new(-1, 0);
        g.units
            .push(Unit::new(200, enemy, Team::Red, UnitType::Melee));
        g.selected = Some(0);
        assert!(g.queue_move(enemy));
        assert_eq!(g.units[0].plan_len(), 2);
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
    fn a_stray_click_never_replaces_a_queue() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Melee]);
        g.set_selection(vec![0, 1]);
        assert!(g.queue_move(Hex::new(0, 0)));
        let plans: Vec<usize> = (0..2).map(|i| g.units[i].plan_len()).collect();
        assert!(plans[0] > 1);

        // A click on one hex, then another: both only warn, and the marked
        // hex follows the latest.
        let (a, b) = (Hex::new(-5, 1), Hex::new(-5, 0));
        g.handle_map_click(cursor(&g, a), SCREEN, ClickMode::Normal);
        assert_eq!(g.queue_replace_hex(), Some(a));
        assert_eq!(g.shown_notice(), "CLICK AGAIN TO REPLACE THEIR QUEUES");
        g.handle_map_click(cursor(&g, b), SCREEN, ClickMode::Normal);
        assert_eq!(g.queue_replace_hex(), Some(b));
        // A right-click on the marked hex is a different order: still a warning.
        g.handle_context_click(cursor(&g, b), SCREEN, false, false);
        let unchanged: Vec<usize> = (0..2).map(|i| g.units[i].plan_len()).collect();
        assert_eq!(unchanged, plans, "nothing replaced yet");
        assert!(g.units.iter().all(|u| u.planned_attack.is_none()));

        // Letting go of the group forgets the pending click.
        g.set_selection(vec![0]);
        assert_eq!(g.queue_replace_hex(), None);
        assert!(
            !g.shown_notice().contains("CLICK AGAIN"),
            "the warning goes away"
        );

        // The same click twice replaces the queue.
        g.handle_map_click(cursor(&g, b), SCREEN, ClickMode::Normal);
        assert!(g.units[0].has_queue());
        g.handle_map_click(cursor(&g, b), SCREEN, ClickMode::Normal);
        assert!(!g.units[0].has_queue());
        assert_eq!(g.units[0].planned_move, Some(b));
        assert_eq!(g.queue_replace_hex(), None);
        assert!(!g.shown_notice().contains("CLICK AGAIN"));
    }

    #[test]
    fn any_other_order_cancels_the_queue() {
        type Order = fn(&mut GameState);
        // Map clicks need the same click twice (the first only warns); the
        // buttons and keys act at once.
        let orders: [(&str, Order, bool); 6] = [
            (
                "click a move",
                |g| g.handle_map_click(cursor(g, Hex::new(-5, 0)), SCREEN, ClickMode::Normal),
                true,
            ),
            (
                "right-click an attack",
                |g| g.handle_context_click(cursor(g, Hex::new(-3, 1)), SCREEN, false, false),
                true,
            ),
            ("guard", |g| g.toggle_guard(), false),
            ("ability", |g| g.toggle_selected_ability(), false),
            ("ctrl-right-click", |g| g.handle_right_click(), false),
            (
                "swap",
                |g| g.handle_map_click(cursor(g, Hex::new(-4, 1)), SCREEN, ClickMode::Swap),
                true,
            ),
        ];
        for (name, order, twice) in orders {
            let mut g = open_field(&[UnitType::Melee, UnitType::Melee]);
            g.selected = Some(0);
            assert!(g.queue_move(Hex::new(-3, 0)));
            assert!(g.queue_move(Hex::new(-2, 0)));
            if twice {
                order(&mut g);
                assert!(g.units[0].has_queue(), "one {name} only warns");
            }
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
        // Far enough that the whole way fits in one click.
        g.settings.max_queued_turns = 20;
        // One member already has two turns of its own queued.
        g.selected = Some(1);
        assert!(g.queue_move(Hex::new(-2, 1)));
        assert!(g.queue_move(Hex::new(0, 1)));
        g.set_selection(vec![0, 1, 2]);
        let far = Hex::new(4, 0);
        assert!(g.queue_move(far));
        let lengths: Vec<usize> = (0..3).map(|i| g.units[i].plan_len()).collect();
        assert!(
            lengths.iter().all(|&len| len == lengths[0]),
            "plans differ: {lengths:?}"
        );
        // The others set off at once rather than waiting for the cavalry's
        // two turns, and the group went as far as it could in one click.
        assert!(g.units[0].planned_move.is_some());
        assert!(g.units[2].planned_move.is_some());
        assert!(g.units[0].plan_len() > 5);
        assert!(!g.queue_move(far), "nobody can get any closer");
        assert!((0..3).any(|i| g.units[i].plan_end() == far));
        assert!((0..3).all(|i| g.units[i].plan_end().distance(far) <= 2));

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
    fn a_unit_added_to_a_queued_group_starts_moving_this_turn() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Ranged]);
        // The melee has three turns of its own queued.
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-1, 0)));
        let queued: Vec<Hex> = (1..=3).map(|t| g.units[0].pos_after(t)).collect();
        assert_eq!(g.units[0].plan_len(), 3);

        // The ranged unit joins, and the group is sent farther on.
        g.set_selection(vec![0, 1]);
        assert!(g.queue_move(Hex::new(4, 1)));
        let ranged = &g.units[1];
        assert!(
            ranged.planned_move.is_some(),
            "the new member moves on turn 1, not after the others' queues"
        );
        assert_eq!(
            (1..=3).map(|t| g.units[0].pos_after(t)).collect::<Vec<_>>(),
            queued,
            "what the melee had queued is untouched"
        );
        assert_eq!(g.units[0].plan_len(), g.units[1].plan_len(), "same length");
        // No two members plan to end any turn on the same hex.
        for turn in 1..=g.units[0].plan_len() {
            assert_ne!(g.units[0].pos_after(turn), g.units[1].pos_after(turn));
        }
    }

    #[test]
    fn a_group_marches_its_queue_in_step() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Melee, UnitType::Ranged]);
        g.set_selection(vec![0, 1, 2]);
        let far = Hex::new(3, 1);
        assert!(g.queue_move(far));
        let planned: Vec<Hex> = (0..3).map(|i| g.units[i].plan_end()).collect();
        for _ in 0..g.units[0].plan_len() {
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
