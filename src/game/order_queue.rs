//! Order queues: Shift-click plans a unit's (or a group's) orders over
//! several turns. Shift-left-click adds the turns it takes to walk to the
//! clicked hex (or, on a hex the plan already moves to, takes that move and
//! the turns after it off); Shift-right-click adds an attack on it. Turn 0 of
//! a plan is the unit's ordinary `planned_move` and `planned_attack`; later
//! turns wait in `Unit::queued`, and each turn's end moves the next one up
//! (`advance_queues`). A unit following a queue doesn't hold up ending the
//! turn, and any other order cancels its queue.

use std::collections::VecDeque;

use super::GameState;
use super::city::turns_icon;
use super::fast_hash::{HashMap, HashSet};
use super::fog::Fog;
use super::hex::Hex;
use super::orders::SHIPS_NOTICE;
use super::turn::{Phase, step_rank};
use super::unit::{Team, TurnOrder, Unit};

impl GameState {
    /// Shift-left-click on a hex: on one of the selection's planned move
    /// destinations (a stop of its queue, or this turn's ghost), takes that
    /// move back off (`unqueue_move`); anywhere else, queues the way there
    /// (`queue_move`). Returns whether any plan changed.
    pub(super) fn queue_or_unqueue_move(&mut self, hex: Hex) -> bool {
        self.unqueue_move(hex) || self.queue_move(hex)
    }

    /// Takes a planned move onto `hex` off the plan of the selected unit (or
    /// group member) heading there, with every turn after it; the turns
    /// before it stay. The turn itself keeps its attack if that's still in
    /// range from where the unit then stands, and is dropped if it's left
    /// with nothing (unless it's this turn). If several moves end on `hex`,
    /// the latest goes: another click takes the one before it. In a group,
    /// the member waits out the turns it lost, so its plan stays as long as
    /// the others'. Returns whether a move was taken off.
    pub(super) fn unqueue_move(&mut self, hex: Hex) -> bool {
        let members = self.selection();
        if self.is_resolving() {
            return false;
        }
        // The latest turn onto `hex`; on a tie, the first in the selection.
        let Some((turn, _, idx)) = members
            .iter()
            .enumerate()
            .flat_map(|(n, &i)| {
                let unit = &self.units[i];
                (0..unit.plan_len())
                    .filter(move |&t| unit.move_on_turn(t) == Some(hex))
                    .map(move |t| (t, std::cmp::Reverse(n), i))
            })
            .max()
        else {
            return false;
        };
        let before = self.units[idx].plan_len();
        let others = members
            .iter()
            .filter(|&&i| i != idx)
            .copied()
            .collect::<Vec<_>>();
        self.cut_plan(idx, turn);
        // Still steered: now toward where the cut plan ends.
        let unit = &mut self.units[idx];
        if !unit.waypoints.is_empty() {
            unit.waypoints = if unit.has_queue() {
                vec![unit.plan_end()]
            } else {
                Vec::new()
            };
        }
        let length = self.plan_length(&others).min(before);
        self.pad_plan(idx, length);
        self.notice = if turn + 1 < before {
            format!("TOOK THE MOVE OFF TURN {} AND ALL AFTER IT", turn + 1)
        } else {
            format!("TOOK THE MOVE OFF TURN {}", turn + 1)
        };
        true
    }

    /// Cuts unit `idx`'s plan back to before turn `turn`'s move: the turns
    /// before stay, the move and every later turn go, and the turn's attack
    /// stays if it's still in range without the move. A later turn left with
    /// nothing is dropped too; a plan left with nothing at all is no plan.
    fn cut_plan(&mut self, idx: usize, turn: usize) {
        if turn == 0 {
            // Half of a swap takes the other half with it.
            self.cancel_swap(idx);
        }
        let unit = &mut self.units[idx];
        unit.queued.truncate(turn);
        if turn == 0 {
            unit.planned_move = None;
            unit.drop_unreachable_attack();
            if unit.planned_attack.is_none() {
                unit.following_queue = false;
            }
            return;
        }
        let range = unit.later_stats().attack_range;
        let order = &mut unit.queued[turn - 1];
        order.move_to = None;
        if order
            .attack
            .is_some_and(|target| order.from.distance(target) > range)
        {
            order.attack = None;
        }
        if order.attack.is_none() {
            unit.queued.pop();
        }
    }

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
        let naval = self.units[members[0]].is_naval();
        if members.iter().any(|&i| self.units[i].is_naval() != naval) {
            self.notice = "QUEUE LAND AND NAVAL UNITS SEPARATELY".into();
            return false;
        }
        let fog = self.fog();
        // A plan built by Shift-clicks alone keeps going where it was sent
        // (`waypoints`); one with an attack queued stays as it was built.
        let steered: Vec<bool> = members
            .iter()
            .map(|&i| self.units[i].plan_len() == 0 || !self.units[i].waypoints.is_empty())
            .collect();
        match self.extend_queue(&members, target, &fog) {
            Extended::Queued {
                first,
                length,
                cut_short,
            } => {
                for (&i, steered) in members.iter().zip(steered) {
                    if steered {
                        self.units[i].waypoints.push(target);
                    }
                }
                // Cut short, the limit is the news (the plan drawn on the map
                // shows how far it goes), and the notice stays short enough
                // for the top bar of a small window.
                let limit = self.settings.max_queued_turns.max(1);
                self.notice = if cut_short {
                    format!("QUEUED UP TO THE {} LIMIT", turns_icon(limit as i32))
                } else {
                    queued_notice(first, length - first)
                };
                true
            }
            Extended::Full(limit) => {
                self.notice = format!("QUEUE FULL - {} LIMIT", turns_icon(limit as i32));
                false
            }
            Extended::NoCloser => {
                self.notice = "CAN'T GET ANY CLOSER THERE".into();
                false
            }
        }
    }

    /// Adds to `members`' plans the turns it takes each to get as close to
    /// `target` as it can (`queue_move`), as the player knows the board,
    /// up to `Settings::max_queued_turns`. Leaves every plan as it was if
    /// nobody moves.
    fn extend_queue(&mut self, members: &[usize], target: Hex, fog: &Fog) -> Extended {
        let team = self.units[members[0]].team;
        let naval = self.units[members[0]].is_naval();
        // Allies with no orders will still be standing where they are.
        let parked: HashSet<Hex> = (0..self.units.len())
            .filter(|&j| {
                !members.contains(&j) && self.units[j].team == team && self.units[j].plan_len() == 0
            })
            .map(|j| self.units[j].pos)
            .collect();
        let walk = self.planned_walk_to(target, team, fog, naval, &parked);
        let before: Vec<Unit> = members.iter().map(|&i| self.units[i].clone()).collect();
        // No member's plan grows past `limit` turns.
        let limit = self.settings.max_queued_turns.max(1);
        let first = before.iter().map(Unit::plan_len).min().unwrap_or(0);
        let longest = self.plan_length(members);
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
            let legs = self.plan_move_turn(&active, target, turn, &walk, fog);
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
            return if first >= limit {
                Extended::Full(limit)
            } else {
                Extended::NoCloser
            };
        }
        // Whether a member whose plan is full could still have got closer,
        // checked before padding (which would count as its turns).
        let cut_short = members.iter().any(|&i| {
            let len = self.units[i].plan_len();
            len >= limit
                && self
                    .plan_move_turn(&[i], target, len, &walk, fog)
                    .iter()
                    .any(|(_, dest)| dest.is_some())
        });
        let length = self.plan_length(members);
        for &i in members {
            self.pad_plan(i, length);
        }
        Extended::Queued {
            first,
            length,
            cut_short,
        }
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
                // Only enemies in sight close hexes: an ally may be moving
                // off (`claimed` and `vacated_too_late` keep out the hexes
                // allies stay on or leave too late), so a column following
                // its leader doesn't stop behind it every turn.
                self.known_reachable_past(
                    start,
                    unit.stats().move_range,
                    team,
                    fog,
                    unit.is_naval(),
                    |hex| fog.sees(hex) && self.enemy_of_team_at(hex, team).is_some(),
                )
            } else {
                self.planned_reachable(
                    start,
                    unit.later_stats().move_range,
                    team,
                    fog,
                    unit.is_naval(),
                )
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
            // Of the hexes as far along the way, the one nearest the target
            // as the crow flies: many ways are equally short on hexes, and
            // this keeps to the straightest, not one direction then another.
            let best = reachable
                .into_iter()
                .filter(|&hex| hex == start || !(claimed.contains(&hex) || vacated_too_late(hex)))
                .min_by_key(|&hex| {
                    (
                        away(hex),
                        hex.distance(start),
                        hex.straight_distance_sq(target),
                        hex.q,
                        hex.r,
                    )
                });
            let dest = best.filter(|&dest| dest != start);
            claimed.insert(dest.unwrap_or(start));
            legs.push((i, dest));
        }
        legs
    }

    /// How many steps each hex is from `target` for a unit of `team` in a
    /// later turn, as far as the player knows (around the terrain, walls and
    /// gates the player has seen, and the `parked` hexes, where allies
    /// stand still; not other units, which will have moved: a hex never seen
    /// counts as open). Hexes with no known way to `target` are left out.
    fn planned_walk_to(
        &self,
        target: Hex,
        team: Team,
        fog: &Fog,
        naval: bool,
        parked: &HashSet<Hex>,
    ) -> HashMap<Hex, i32> {
        let mut steps = HashMap::from_iter([(target, 0)]);
        let mut frontier = VecDeque::from([target]);
        while let Some(hex) = frontier.pop_front() {
            let next = steps[&hex] + 1;
            for neighbor in hex.neighbors() {
                if !steps.contains_key(&neighbor)
                    && !parked.contains(&neighbor)
                    && self.known_passable(neighbor, naval, fog)
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
    /// `Settings::max_queued_turns`. Like a queued move, it's planned on the
    /// board as the player knows it, so a hex out of sight (never seen, even)
    /// can be attacked as a seen one can. Returns whether anything was queued.
    pub(super) fn queue_attack(&mut self, target: Hex) -> bool {
        let members = self.selection();
        // As the player knows it: a hex never seen may be attacked, whatever
        // is really there.
        let fog = self.fog();
        let water = self.explored_by(target, &fog) && self.grid.terrain(target).is_water();
        if members.is_empty()
            || self.is_resolving()
            || !(self.known_passable(target, false, &fog) || water)
        {
            return false;
        }
        if self.known_empty_city_target(target, self.units[members[0]].team, &fog) {
            self.notice = "CITY CENTER CAN ONLY BE CAPTURED FROM ITS INTERIOR".into();
            return false;
        }
        let len = self.plan_length(&members);
        let fill = len > 0 && {
            let able: Vec<usize> = members
                .iter()
                .copied()
                .filter(|&i| self.can_attack_on_turn(i, len - 1, target, &fog))
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
                .filter(|&i| self.can_attack_on_turn(i, turn, target, &fog))
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
            self.notice = format!("QUEUE FULL - {} LIMIT", turns_icon(limit as i32));
            return false;
        }
        if attackers.is_empty() {
            self.notice = if water && members.iter().all(|&i| !self.units[i].attacks_water()) {
                SHIPS_NOTICE.into()
            } else {
                "OUT OF RANGE THERE".into()
            };
            return false;
        }
        for &i in &members {
            // A plan with an attack in it stays as it's built: it isn't
            // planned again toward where it was going.
            self.units[i].waypoints.clear();
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
        unit.wake();
        unit.holding = false;
    }

    fn set_attack_on_turn(&mut self, idx: usize, turn: usize, target: Hex) {
        let unit = &mut self.units[idx];
        match turn {
            0 => unit.planned_attack = Some(target),
            n => unit.queued[n - 1].attack = Some(target),
        }
        unit.following_queue = true;
        unit.wake();
        unit.holding = false;
    }

    /// Whether unit `idx` could attack `target` on turn `turn` of its plan,
    /// from where the plan has it standing then, as far as the player knows.
    fn can_attack_on_turn(&self, idx: usize, turn: usize, target: Hex, fog: &Fog) -> bool {
        let unit = &self.units[idx];
        if !self.known_attack_target_legal(idx, target, turn > 0, fog) {
            return false;
        }
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
    /// far as the player knows: around the terrain, walls and gates it has
    /// seen (a hex never seen counts as open), but not units, which will have
    /// moved by then.
    fn planned_reachable(
        &self,
        start: Hex,
        move_range: i32,
        team: Team,
        fog: &Fog,
        naval: bool,
    ) -> HashSet<Hex> {
        self.reachable_hexes_by(start, move_range, |from, to| {
            self.known_passable(to, naval, fog) && self.known_can_cross(from, to, team, fog)
        })
    }

    /// At the end of a turn, after each unit's `end_turn`: every unit with a
    /// queue takes its next turn's orders. A unit whose queued turn can't be
    /// carried out on the real board (it isn't where the queue expects, it's
    /// in a contested hex, its target is out of range) drops its whole queue
    /// and needs orders again. This runs as the turn resolves, on every
    /// machine of a network game, so it reads no player's fog: what the
    /// player knows is for `replan_queues`, as their next turn's planning
    /// begins.
    pub(super) fn advance_queues(&mut self) {
        for i in 0..self.units.len() {
            let unit = &mut self.units[i];
            if unit.queued.is_empty() {
                // A queue that ran out is done, unless it hasn't got where
                // it was going (its last move was turned back, say): then
                // it's planned again as the next turn's planning begins.
                if unit.waypoints.last().is_some_and(|&end| end != unit.pos) {
                    unit.following_queue = true;
                } else {
                    unit.waypoints.clear();
                }
                continue;
            }
            let next = unit.queued.remove(0);
            unit.planned_move = next.move_to;
            unit.planned_attack = next.attack;
            unit.following_queue = true;
            // Steered, a turned-back step is no reason to stop: the queue is
            // planned again from where the unit stands.
            if unit.pos != next.from && !unit.waypoints.is_empty() {
                unit.planned_move = None;
                unit.planned_attack = None;
                unit.queued.clear();
                continue;
            }
            if let Some(reason) = self.queued_turn_problem(i, next.from) {
                let unit = &self.units[i];
                log::info!("{unit} drops its queued orders: {reason}");
                if self.is_player_controlled(i) {
                    self.notice = format!("{} STOPPED: {reason}", self.unit_role(unit));
                }
                self.units[i].clear_orders();
            }
        }
    }

    /// Why unit `idx` can't carry out the queued turn it just took, on the
    /// real board, if it can't. `from` is where the queue expected it to
    /// start the turn.
    fn queued_turn_problem(&self, idx: usize, from: Hex) -> Option<&'static str> {
        let unit = &self.units[idx];
        if unit.pos != from {
            return Some("IT DIDN'T GET WHERE IT WAS GOING");
        }
        if self.rival_of(idx).is_some() {
            return Some("IT IS FIGHTING FOR ITS HEX");
        }
        if let Some(target) = unit.planned_attack
            && (!unit.can_attack()
                || unit.planned_pos().distance(target) > unit.stats().attack_range)
        {
            return Some("ITS TARGET IS OUT OF RANGE");
        }
        None
    }

    /// As the player's next turn's planning begins (after a network game's
    /// turn-start snapshot, so it's planning like any other and goes in the
    /// side's plan): each of the player's queues is looked over on the board
    /// as the player now knows it. One built by Shift-clicks alone is
    /// planned again from where its units stand toward where it's going
    /// (`waypoints`), along the shortest way the player knows, so a path
    /// set through the fog follows what the fog reveals; a waypoint reached,
    /// or as near as the unit can get, is dropped, and a queue with none
    /// left is done. A queue whose next move runs into an enemy in sight
    /// stops, and its units need orders again. One built with an
    /// attack is kept as it was, unless its next move is now known to be
    /// blocked.
    pub(super) fn replan_queues(&mut self) {
        let fog = self.fog();
        let queued: Vec<usize> = (0..self.units.len())
            .filter(|&i| self.is_player_controlled(i) && self.units[i].following_queue)
            .collect();
        for &i in &queued {
            if let Some(reason) = self.known_queue_problem(i, &fog) {
                self.stop_queue(i, reason);
            }
        }
        // Steered queues, a group (the same waypoints) planned together.
        let mut groups: Vec<(Vec<Hex>, Vec<usize>)> = Vec::new();
        for &i in &queued {
            let unit = &mut self.units[i];
            if !unit.following_queue || unit.waypoints.is_empty() {
                continue;
            }
            while unit.waypoints.first() == Some(&unit.pos) {
                unit.waypoints.remove(0);
            }
            match groups.iter_mut().find(|(way, _)| *way == unit.waypoints) {
                Some((_, members)) => members.push(i),
                None => groups.push((unit.waypoints.clone(), vec![i])),
            }
        }
        for (waypoints, members) in groups {
            for &i in &members {
                let unit = &mut self.units[i];
                unit.planned_move = None;
                unit.planned_attack = None;
                unit.queued.clear();
                unit.following_queue = false;
            }
            let mut left = waypoints.clone();
            for &target in &waypoints {
                match self.extend_queue(&members, target, &fog) {
                    Extended::Queued { .. } => {}
                    // As near as it gets: on to the next.
                    Extended::NoCloser => {
                        left.retain(|&w| w != target);
                    }
                    Extended::Full(_) => break,
                }
            }
            for &i in &members {
                let unit = &mut self.units[i];
                unit.waypoints = if unit.has_queue() {
                    left.clone()
                } else {
                    Vec::new()
                };
            }
        }
    }

    /// Why unit `idx`'s queued turn, just taken, can't go ahead on the
    /// board as the player knows it, if it can't: an enemy in sight where
    /// it's going, or, for a queue kept as built, a way the player now knows
    /// is blocked or an ally standing where it's going.
    fn known_queue_problem(&self, idx: usize, fog: &Fog) -> Option<&'static str> {
        let unit = &self.units[idx];
        let dest = unit.planned_move?;
        if fog.sees(dest) && self.enemy_of_team_at(dest, unit.team).is_some() {
            return Some("AN ENEMY STANDS IN ITS WAY");
        }
        if !unit.waypoints.is_empty() {
            return None;
        }
        let reachable = self.planned_reachable(
            unit.pos,
            unit.stats().move_range,
            unit.team,
            fog,
            unit.is_naval(),
        );
        if !reachable.contains(&dest) {
            return Some("ITS WAY IS BLOCKED");
        }
        let ally_there =
            self.units.iter().enumerate().any(|(j, other)| {
                j != idx && other.team == unit.team && other.planned_pos() == dest
            });
        if ally_there {
            return Some("AN ALLY IS IN ITS WAY");
        }
        None
    }

    /// Drops unit `idx`'s queue and orders for `reason`, telling the player.
    fn stop_queue(&mut self, idx: usize, reason: &str) {
        let unit = &self.units[idx];
        log::info!("{unit} drops its queued orders: {reason}");
        self.notice = format!("{} STOPPED: {reason}", self.unit_role(unit));
        self.units[idx].clear_orders();
    }
}

/// What `extend_queue` made of a Shift-click.
enum Extended {
    /// Turns queued: from turn `first` (0 is this one) to `length`, and
    /// whether the limit cut it short.
    Queued {
        first: usize,
        length: usize,
        cut_short: bool,
    },
    /// Nobody's plan had room: the limit.
    Full(usize),
    /// Nobody could get any closer.
    NoCloser,
}

/// What the top bar says after a Shift-click queued `turns` turns starting
/// on turn `first` (0 is this one).
fn queued_notice(first: usize, turns: usize) -> String {
    let last = first + turns;
    match (first, turns) {
        (0, 1) => "QUEUED FOR THIS TURN - SHIFT-CLICK AGAIN FOR THE NEXT".into(),
        (0, _) => format!("QUEUED FOR THE NEXT {}", turns_icon(turns as i32)),
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
    fn landing_craft_cannot_queue_an_attack() {
        let mut game = open_field(&[UnitType::LandingCraft]);
        game.selected = Some(0);
        assert!(!game.queue_attack(Hex::new(-3, 0)));
        assert!(game.units[0].planned_attack.is_none());
        assert!(game.units[0].queued.is_empty());
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
        assert_eq!(g.notice, "QUEUED FOR THE NEXT \u{E003}6", "no limit hit");
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
        assert_eq!(g.notice, "QUEUED UP TO THE \u{E003}6 LIMIT");
        assert!(!g.queue_move(far), "no more on another click");
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.notice, "QUEUE FULL - \u{E003}6 LIMIT");
        // An attack still fits into the last turn, but a second one would
        // need a turn of its own.
        g.units
            .push(Unit::new(99, Hex::new(3, 0), Team::Red, UnitType::Melee));
        assert!(g.queue_attack(Hex::new(3, 0)));
        assert_eq!(g.units[0].plan_len(), 6);
        assert!(!g.queue_attack(Hex::new(3, 0)));
        assert_eq!(g.units[0].plan_len(), 6);
        assert_eq!(g.notice, "QUEUE FULL - \u{E003}6 LIMIT");

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
        g.set_setting(Setting::MaxQueuedTurns, 3);
        assert_eq!(g.settings.max_queued_turns, 3);
        assert_eq!(g.notice, "QUEUE LIMIT: 3 TURNS");
        g.selected = Some(0);
        let far = Hex::new(4, 0);
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 3);
        assert_eq!(g.notice, "QUEUED UP TO THE \u{E003}3 LIMIT");

        // A higher limit lets the next click go the rest of the way.
        g.set_setting(Setting::MaxQueuedTurns, 25);
        assert_eq!(
            g.settings.max_queued_turns, 20,
            "past the top of its range stops there"
        );
        assert!(g.queue_move(far));
        assert_eq!(g.units[0].plan_len(), 8);
        assert_eq!(g.units[0].plan_end(), far);
        assert!(!g.notice.contains("LIMIT"), "{}", g.notice);

        // At the lowest, only this turn.
        let mut g = open_field(&[UnitType::Melee]);
        g.set_setting(Setting::MaxQueuedTurns, 1);
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
        assert_eq!(g.notice, "QUEUED UP TO THE \u{E003}5 LIMIT");
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
        assert_eq!(g.notice, "QUEUE FULL - \u{E003}5 LIMIT");
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
            // Planned again each turn, the group stays in step.
            let lengths: Vec<usize> = (0..3).map(|i| g.units[i].plan_len()).collect();
            assert!(lengths.windows(2).all(|w| w[0] == w[1]), "{lengths:?}");
        }
        // Planned again each turn, members may take other hexes around the
        // target than first planned, but the group gets as near.
        let near = |hexes: &[Hex]| {
            let mut d: Vec<i32> = hexes.iter().map(|h| h.distance(far)).collect();
            d.sort();
            d
        };
        let reached: Vec<Hex> = (0..3).map(|i| g.units[i].pos).collect();
        assert_eq!(
            near(&reached),
            near(&planned),
            "the group got where it was sent"
        );
    }

    #[test]
    fn a_column_keeps_moving_behind_its_leader() {
        // Two in a line, the second right behind the first: planned again
        // each turn, the second follows into the hex the first leaves
        // rather than waiting behind it.
        let mut g = open_field(&[UnitType::Melee, UnitType::Melee]);
        g.units[0].pos = Hex::new(-3, 0);
        g.units[1].pos = Hex::new(-4, 0);
        g.set_selection(vec![0, 1]);
        assert!(g.queue_move(Hex::new(1, 0)));
        for _ in 0..3 {
            let before = g.units[1].pos;
            play_turn(&mut g);
            assert_ne!(g.units[1].pos, before, "the follower waited");
            assert!(g.units[1].has_queue());
        }
    }

    #[test]
    fn a_queue_goes_around_an_ally_in_its_way() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        assert!(g.queue_move(Hex::new(-1, 0)));
        // An ally arrives on next turn's destination and stays there: the
        // waypoint under it is as near as the unit gets, and it goes on
        // around it.
        g.units
            .push(Unit::new(200, Hex::new(-2, 0), Team::Blue, UnitType::Melee));
        play_turn(&mut g);
        assert_eq!(g.units[0].pos, Hex::new(-3, 0));
        assert!(g.units[0].has_queue());
        assert_eq!(g.units[0].plan_end(), Hex::new(-1, 0));
        assert!(!stops(&g, 0).contains(&Hex::new(-2, 0)));
        assert_eq!(g.units[0].waypoints, vec![Hex::new(-1, 0)]);
    }

    #[test]
    fn a_steered_unit_turned_back_goes_on_from_where_it_stands() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        let target = Hex::new(0, 0);
        assert!(g.queue_move(target));
        // An ally with no orders stops on this turn's destination: the move
        // is turned back, and the queue goes on around it from where the
        // unit stands.
        let dest = g.units[0].planned_move.unwrap();
        g.units
            .push(Unit::new(300, dest, Team::Blue, UnitType::Melee));
        g.units[1].holding = true;
        play_turn(&mut g);
        assert_eq!(g.units[0].pos, Hex::new(-4, 0));
        assert!(g.units[0].has_queue(), "{}", g.notice);
        assert_eq!(g.units[0].plan_end(), target);
        assert!(!stops(&g, 0).contains(&dest));
    }

    #[test]
    fn a_blocked_queue_kept_as_built_is_dropped_and_the_unit_wants_orders() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        assert!(g.queue_move(Hex::new(-3, 0)));
        assert!(g.queue_move(Hex::new(-2, 0)));
        // An attack in the plan keeps it as it was built.
        assert!(g.queue_attack(Hex::new(-1, 0)));
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

    /// `open_field` with the fog of war on and nothing seen yet but what
    /// the units see now.
    fn fogged_field(types: &[UnitType]) -> GameState {
        let mut game = open_field(types);
        game.fog_of_war = true;
        game.memory.clear();
        game.explore();
        game
    }

    #[test]
    fn a_queued_attack_can_target_a_hex_in_the_fog() {
        // Queued moves end at (-2, 0); the melee attacks next door from there.
        let target = Hex::new(-1, 0);
        let queue = |game: &mut GameState| {
            game.selected = Some(0);
            assert!(game.queue_move(Hex::new(-2, 0)));
            assert!(game.queue_attack(target), "{}", game.notice);
            game.units[0].queued.clone()
        };
        let mut seen = open_field(&[UnitType::Melee]);
        let planned = queue(&mut seen);
        assert_eq!(planned.last().unwrap().attack, Some(target));

        // Never seen: the same plan, and it's carried out.
        let mut unseen = fogged_field(&[UnitType::Melee]);
        assert!(!unseen.fog().sees(target) && !unseen.is_explored(target));
        assert_eq!(queue(&mut unseen), planned);
        assert_eq!(unseen.notice, seen.notice);
        // Seen once and out of sight now.
        let mut remembered = fogged_field(&[UnitType::Melee]);
        remembered.units[0].pos = Hex::new(-2, 0);
        remembered.explore();
        remembered.units[0].pos = Hex::new(-4, 0);
        assert!(remembered.is_explored(target) && !remembered.fog().sees(target));
        assert_eq!(queue(&mut remembered), planned);

        // Only in range of where the queue ends, as for a seen hex.
        let mut far = fogged_field(&[UnitType::Melee]);
        far.selected = Some(0);
        assert!(far.queue_move(Hex::new(-2, 0)));
        assert!(!far.queue_attack(Hex::new(0, 0)));
        assert_eq!(far.notice, "OUT OF RANGE THERE");
    }

    #[test]
    fn a_queued_attack_into_the_fog_gives_nothing_away() {
        use crate::game::city::City;
        // A Red city center out of sight, where the melee's queue ends next
        // door: whether anyone stands in it, the attack is queued the same.
        let center = Hex::new(-1, 0);
        let plan = |occupied: bool| {
            let mut g = fogged_field(&[UnitType::Melee]);
            g.cities.push(City::new(7, Team::Red, center));
            if occupied {
                g.units
                    .push(Unit::new(200, center, Team::Red, UnitType::Melee));
            }
            assert!(!g.fog().sees(center));
            g.selected = Some(0);
            assert!(g.queue_move(Hex::new(-2, 0)));
            (g.queue_attack(center), g.notice.clone())
        };
        let (queued, notice) = plan(false);
        assert!(queued, "{notice}");
        assert_eq!(plan(true), (queued, notice));

        // In sight and empty, it's still refused, as the player can see.
        let mut g = open_field(&[UnitType::Melee]);
        g.cities.push(City::new(7, Team::Red, Hex::new(-3, 0)));
        g.selected = Some(0);
        assert!(!g.queue_attack(Hex::new(-3, 0)));
        assert_eq!(
            g.notice,
            "CITY CENTER CAN ONLY BE CAPTURED FROM ITS INTERIOR"
        );
    }

    #[test]
    fn a_queued_attack_on_water_says_who_can_attack_ships() {
        use crate::game::hex::HexGrid;
        use crate::game::terrain::Terrain;
        let lake = Hex::new(-3, 0);
        for (unit_type, queued) in [(UnitType::Melee, false), (UnitType::Ranged, true)] {
            let mut g = open_field(&[unit_type]);
            g.grid = HexGrid::new(6, [(lake, Terrain::Lake)]);
            g.selected = Some(0);
            assert_eq!(g.queue_attack(lake), queued, "{unit_type:?}");
            if !queued {
                assert_eq!(g.notice, SHIPS_NOTICE);
            }
        }
    }

    /// Shift-clicks each of `hexes` on the map, as the player would.
    fn shift_click(game: &mut GameState, hexes: &[Hex]) {
        for &hex in hexes {
            game.handle_map_click(cursor(game, hex), SCREEN, ClickMode::QueueMove);
        }
    }

    #[test]
    fn shift_clicking_a_queued_stop_takes_it_and_every_later_move_off() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        let path = [
            Hex::new(-3, 0),
            Hex::new(-2, 0),
            Hex::new(-1, 0),
            Hex::new(0, 0),
        ];
        shift_click(&mut g, &path);
        assert_eq!(moves(&g, 0), path.map(Some));

        // A stop in the middle: the move there and everything after go.
        shift_click(&mut g, &[path[1]]);
        assert_eq!(moves(&g, 0), [Some(path[0])]);
        assert_eq!(g.notice, "TOOK THE MOVE OFF TURN 2 AND ALL AFTER IT");
        assert!(g.units[0].following_queue, "what's left is still its queue");
        assert!(!g.needs_orders(0));
        assert_eq!(g.selected, Some(0), "selection stays");

        // Not a stop any more, so the same hex queues again, and so does a
        // hex beside the stops.
        shift_click(&mut g, &[path[1], Hex::new(-2, 1)]);
        assert_eq!(
            moves(&g, 0),
            [Some(path[0]), Some(path[1]), Some(Hex::new(-2, 1))]
        );

        // The last stop: only it goes.
        shift_click(&mut g, &[Hex::new(-2, 1)]);
        assert_eq!(moves(&g, 0), [Some(path[0]), Some(path[1])]);
        assert_eq!(g.notice, "TOOK THE MOVE OFF TURN 3");

        // This turn's: nothing is left, and the unit wants orders again.
        shift_click(&mut g, &[path[0]]);
        assert!(!g.units[0].has_orders());
        assert!(g.needs_orders(0));

        // The ghost of a plain move goes the same way.
        g.handle_map_click(cursor(&g, path[0]), SCREEN, ClickMode::Normal);
        assert_eq!(g.units[0].planned_move, Some(path[0]));
        g.selected = Some(0);
        shift_click(&mut g, &[path[0]]);
        assert_eq!(g.units[0].planned_move, None);
    }

    #[test]
    fn a_hex_stopped_on_twice_loses_its_latest_move_first() {
        let mut g = open_field(&[UnitType::Melee]);
        g.selected = Some(0);
        let (a, b) = (Hex::new(-3, 0), Hex::new(-2, 0));
        assert!(g.queue_move(a) && g.queue_move(b) && g.queue_move(a));
        assert_eq!(moves(&g, 0), [Some(a), Some(b), Some(a)]);
        assert!(g.unqueue_move(a));
        assert_eq!(moves(&g, 0), [Some(a), Some(b)]);
        assert!(g.unqueue_move(a));
        assert!(!g.units[0].has_queue());
        assert!(!g.unqueue_move(a), "nothing heads there now");
    }

    #[test]
    fn a_turns_attack_stays_if_still_in_range_without_its_move() {
        let mut g = open_field(&[UnitType::Ranged]);
        g.selected = Some(0);
        let (a, b) = (Hex::new(-3, 0), Hex::new(-2, 0));
        assert!(g.queue_move(a) && g.queue_move(b));
        // Two hexes from `a`, one from `b`: in range either way.
        let near = Hex::new(-1, 0);
        assert!(g.queue_attack(near));
        assert!(g.unqueue_move(b));
        assert_eq!(
            g.units[0].queued,
            [TurnOrder {
                from: a,
                move_to: None,
                attack: Some(near),
            }]
        );

        // Out of range from `a`: the turn goes with its move.
        assert!(g.queue_move(b));
        let far = Hex::new(0, 0);
        assert!(g.queue_attack(far));
        assert!(g.unqueue_move(b));
        assert_eq!(moves(&g, 0), [Some(a), None]);
        assert_eq!(
            g.units[0].attack_on_turn(1),
            Some(near),
            "earlier turns stay"
        );
    }

    #[test]
    fn a_group_member_waits_out_the_moves_taken_off_its_queue() {
        let mut g = open_field(&[UnitType::Melee, UnitType::Melee]);
        g.settings.max_queued_turns = 20;
        g.set_selection(vec![0, 1]);
        assert!(g.queue_move(Hex::new(2, 0)));
        let length = g.units[0].plan_len();
        assert!(length >= 4 && g.units[1].plan_len() == length);
        let (first, second) = (moves(&g, 0), moves(&g, 1));
        // A stop of the first member's that the second never moves onto.
        let turn = (1..length - 1)
            .find(|&t| first[t].is_some_and(|hex| !second.contains(&Some(hex))))
            .expect("a stop of its own");
        shift_click(&mut g, &[first[turn].unwrap()]);

        let cut = moves(&g, 0);
        assert_eq!(cut[..turn], first[..turn], "earlier moves stay");
        assert!(cut[turn..].iter().all(Option::is_none), "{cut:?}");
        assert_eq!(g.units[0].plan_len(), length, "as long as the other's");
        assert_eq!(g.units[0].plan_end(), g.units[0].pos_after(turn));
        assert_eq!(moves(&g, 1), second, "the other member's is untouched");
        assert_eq!(g.group, [0, 1], "the group stays selected");

        // Queuing on continues from where the cut plan ends, for both.
        assert!(g.queue_move(Hex::new(3, 0)));
        assert_eq!(g.units[0].plan_len(), g.units[1].plan_len());
    }

    /// A flat radius-7 map with, if `walled`, a wall of mountains down the
    /// middle (q = 0) but for a gap at its south end; one Blue cavalry at
    /// `start`, selected, with the fog on and nothing seen but what it sees.
    fn field_in_fog(start: Hex, walled: bool) -> GameState {
        use crate::game::hex::HexGrid;
        use crate::game::terrain::Tile;
        let mut game = open_field(&[UnitType::Cavalry]);
        let wall = (-7..=5)
            .filter(|_| walled)
            .map(|r| (Hex::new(0, r), Tile::MOUNTAINS));
        game.grid = HexGrid::new(7, wall);
        game.units[0].pos = start;
        game.fog_of_war = true;
        game.memory.clear();
        game.explore();
        game.selected = Some(0);
        game
    }

    /// The hexes unit `idx`'s plan moves onto, turn by turn.
    fn stops(game: &GameState, idx: usize) -> Vec<Hex> {
        let unit = &game.units[idx];
        (0..unit.plan_len())
            .filter_map(|turn| unit.move_on_turn(turn))
            .collect()
    }

    #[test]
    fn a_queued_path_never_depends_on_what_is_really_in_the_fog() {
        use crate::game::terrain::Terrain;
        for seed in 0..12 {
            let mut game = GameState::solo_world(seed);
            game.fog_of_war = true;
            game.explore();
            let team = game.local_team;
            let movers: Vec<usize> = (0..game.units.len())
                .filter(|&i| game.units[i].team == team && !game.units[i].is_naval())
                .collect();
            let unseen: Vec<Hex> = game
                .grid
                .all_hexes()
                .filter(|&h| !game.is_explored(h))
                .collect();
            for &i in &movers {
                let from = game.units[i].pos;
                let mut targets: Vec<Hex> = unseen
                    .iter()
                    .copied()
                    .filter(|h| h.distance(from) >= 8)
                    .collect();
                targets.sort_by_key(|h| (h.q * 7 + h.r * 13).rem_euclid(97));
                for &target in targets.iter().take(6) {
                    let plan = |mut g: GameState| {
                        g.settings.max_queued_turns = 12;
                        g.selected = Some(i);
                        g.group.clear();
                        g.queue_move(target);
                        (0..g.units[i].plan_len())
                            .map(|t| g.units[i].move_on_turn(t))
                            .collect::<Vec<_>>()
                    };
                    let real = plan(game.clone());
                    for fill in [Terrain::Mountains, Terrain::Ocean, Terrain::Plains] {
                        let mut other = game.clone();
                        for &h in &unseen {
                            other.grid.set_tile(h, fill);
                        }
                        assert_eq!(
                            plan(other),
                            real,
                            "seed {seed}: unit {i} at {from:?} to {target:?} with the fog full of {fill:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_queued_path_through_open_fog_keeps_to_the_straight_line() {
        let (start, target) = (Hex::new(-5, 2), Hex::new(5, -3));
        let mut game = field_in_fog(start, false);
        game.settings.max_queued_turns = 12;
        assert!(game.queue_move(target));
        assert_eq!(
            game.units[0].plan_len() as i32,
            (start.distance(target) + 1) / 2
        );
        // Every stop is within a hex of the line from start to target, not
        // one direction first and then the other.
        let (a, b) = (start.to_world(), target.to_world());
        let step = start.to_world().distance(start.neighbors()[0].to_world());
        for stop in stops(&game, 0) {
            let p = stop.to_world();
            let off = (b - a).perp_dot(p - a).abs() / (b - a).length();
            assert!(off <= step, "{stop:?} is {off} off the line");
        }
    }

    #[test]
    fn a_queued_path_into_the_fog_goes_by_what_the_player_has_seen() {
        let (start, target) = (Hex::new(-5, 0), Hex::new(5, 0));
        let mut game = field_in_fog(start, true);
        assert!(!game.is_explored(Hex::new(0, 0)));
        let fog = game.fog();
        let known = game.planned_walk_to(target, Team::Blue, &fog, false, &HashSet::default());
        assert_eq!(
            known[&start],
            start.distance(target),
            "straight through the wall no one has seen"
        );
        // The Shift-click plans that way too: 10 hexes at 2 a turn.
        assert!(game.queue_move(target));
        assert_eq!(game.units[0].plan_len(), 5);
        // Seen, the wall is in the way.
        game.fog_of_war = false;
        let real =
            game.planned_walk_to(target, Team::Blue, &game.fog(), false, &HashSet::default());
        assert!(real[&start] > start.distance(target));
    }

    #[test]
    fn a_queue_takes_the_shortest_known_way_again_each_turn() {
        let (start, target) = (Hex::new(-5, 0), Hex::new(5, 0));
        let mut game = field_in_fog(start, true);
        game.settings.max_queued_turns = 12;
        assert!(game.queue_move(target));
        let through_the_fog = stops(&game, 0);
        // The fog lifts from the wall (a scout walks by, say): the queue is
        // planned again around it, still to the same hex.
        for r in -7..=5 {
            game.memory.insert(Hex::new(0, r), Default::default());
        }
        game.replan_queues();
        let around = stops(&game, 0);
        assert_ne!(around, through_the_fog);
        assert!(
            around.iter().all(|&h| game.grid.is_passable(h)),
            "{around:?} crosses the wall"
        );
        assert_eq!(game.units[0].plan_end(), target);
        assert!(game.units[0].plan_len() > 5, "the way round is longer");
        assert_eq!(game.units[0].waypoints, vec![target]);
    }

    #[test]
    fn a_queue_keeps_going_where_it_was_sent_past_the_turn_limit() {
        let (start, target) = (Hex::new(-5, 0), Hex::new(5, 0));
        let mut game = field_in_fog(start, false);
        game.settings.max_queued_turns = 2;
        assert!(game.queue_move(target));
        assert_eq!(game.units[0].plan_len(), 2, "cut short at the limit");
        // Each turn it's planned again from where it stands, so it gets there
        // in the 5 turns it takes, and is then done.
        for _ in 0..5 {
            play_turn(&mut game);
        }
        assert_eq!(game.units[0].pos, target);
        assert!(!game.units[0].has_queue());
        assert!(game.units[0].waypoints.is_empty());
        assert!(game.needs_orders(0));
    }

    #[test]
    fn waypoints_are_visited_in_order_and_dropped_once_reached() {
        let mut game = field_in_fog(Hex::new(-5, 0), false);
        let (first, second) = (Hex::new(-3, -2), Hex::new(2, 0));
        assert!(game.queue_move(first));
        assert!(game.queue_move(second));
        assert_eq!(game.units[0].waypoints, vec![first, second]);
        play_turn(&mut game);
        assert_eq!(game.units[0].pos, first);
        assert_eq!(game.units[0].waypoints, vec![second]);
        assert_eq!(game.units[0].plan_end(), second);
    }

    #[test]
    fn a_plan_with_a_queued_attack_is_kept_as_built() {
        let mut game = field_in_fog(Hex::new(-5, 0), false);
        assert!(game.queue_move(Hex::new(-2, 0)));
        assert!(game.queue_attack(Hex::new(-1, 0)));
        assert!(game.units[0].waypoints.is_empty());
        let plan = stops(&game, 0);
        game.replan_queues();
        assert_eq!(stops(&game, 0), plan);
    }

    #[test]
    fn a_turn_resolves_the_same_whatever_the_machine_has_seen() {
        // Blue's queued turn moves onto a Red unit. On Blue's machine it's in
        // sight; on another player's machine (Green's, seeing nothing here)
        // it isn't. Taking the queue's next turn at the turn's end must come
        // out the same on both, or a network game falls out of step.
        let mut blue = field_in_fog(Hex::new(-5, 0), false);
        let (from, dest) = (Hex::new(-5, 0), Hex::new(-4, 0));
        blue.units[0].queued = vec![TurnOrder {
            from,
            move_to: Some(dest),
            attack: None,
        }];
        blue.units[0].following_queue = true;
        blue.units
            .push(Unit::new(300, dest, Team::Red, UnitType::Melee));
        assert!(blue.fog().sees(dest));
        let mut green = blue.clone();
        green.local_team = Team::Green;
        assert!(!green.fog().sees(dest));
        blue.advance_queues();
        green.advance_queues();
        for (a, b) in blue.units.iter().zip(&green.units) {
            assert_eq!(
                (a.planned_move, &a.queued, a.following_queue),
                (b.planned_move, &b.queued, b.following_queue)
            );
        }
        // Blue's own machine then stops it, as its planning begins.
        blue.replan_queues();
        assert!(!blue.units[0].has_queue());
        assert!(blue.notice.contains("ENEMY"), "{}", blue.notice);
    }
}
