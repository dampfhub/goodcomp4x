//! Ordering several units at once. Alt-drag a box (or Alt-click units) to
//! select a group. Clicking a hex then sends every member toward it at its own
//! speed, each taking the free hex nearest the target that it can reach; the
//! nearest members choose first. Right-clicking a hex has every member in
//! range attack it. Members that can't get any closer, or reach, stay as they
//! are. Shift-clicks queue orders for later turns (`order_queue.rs`).

use std::collections::HashSet;

use glam::Vec2;

use super::GameState;
use super::hex::Hex;
use super::orders::ClickMode;

impl GameState {
    /// Alt-drag: selects the player's units drawn inside the rectangle
    /// between `a` and `b` (window pixels, origin top-left).
    pub fn select_in_box(&mut self, a: Vec2, b: Vec2, screen_size: Vec2) {
        if self.is_resolving() {
            return;
        }
        let (min, max) = (a.min(b), a.max(b));
        let inside = (0..self.units.len())
            .filter(|&i| self.is_player_controlled(i))
            .filter(|&i| {
                let drawn_at = self
                    .camera
                    .world_to_screen(self.unit_layout(i).0, screen_size);
                drawn_at.cmpge(min).all() && drawn_at.cmple(max).all()
            })
            .collect();
        self.set_selection(inside);
    }

    /// Alt-click: adds the player's unit under the cursor to the selection,
    /// or takes it out if it's already in.
    pub fn toggle_in_selection(&mut self, cursor: Vec2, screen_size: Vec2) {
        if self.is_resolving() {
            return;
        }
        let Some(unit) = self
            .hex_at_screen(cursor, screen_size)
            .and_then(|hex| self.controlled_unit_at(hex))
        else {
            return;
        };
        let mut members = self.selection();
        match members.iter().position(|&i| i == unit) {
            Some(at) => {
                members.remove(at);
            }
            None => members.push(unit),
        }
        self.set_selection(members);
    }

    /// Every selected unit: the group, or else the one selected unit.
    pub(super) fn selection(&self) -> Vec<usize> {
        if self.group.is_empty() {
            self.selected.into_iter().collect()
        } else {
            self.group.clone()
        }
    }

    /// Selects `units`: nothing, one unit as usual, or several as a group.
    pub(super) fn set_selection(&mut self, units: Vec<usize>) {
        self.ui_click_mode = None;
        if !units.is_empty() {
            self.leave_city_view();
        }
        if units.len() > 1 {
            self.selected = None;
            self.group = units;
        } else {
            self.selected = units.first().copied();
            self.group.clear();
        }
    }

    /// A map click with a group selected: left-click (or an armed Move)
    /// moves toward the hex, right-click (or an armed Attack) attacks it, and
    /// Shift adds either to every member's queue. Swapping is for single
    /// units.
    pub(super) fn group_order(&mut self, hex: Hex, mode: ClickMode) {
        match mode {
            ClickMode::Attack => self.group_attack(hex),
            ClickMode::Move | ClickMode::Normal => self.group_move(hex),
            ClickMode::QueueMove => {
                self.queue_move(hex);
            }
            ClickMode::QueueAttack => {
                self.queue_attack(hex);
            }
            ClickMode::Swap => {}
        }
    }

    /// Every member that can reach `target` from where it's heading attacks
    /// it. Clicking a target all of them already attack calls it off. Being
    /// a new order, it replaces every member's queue.
    fn group_attack(&mut self, target: Hex) {
        if !self.grid.is_passable(target) {
            return;
        }
        for &i in &self.group {
            self.units[i].cancel_queue();
        }
        let able: Vec<usize> = self
            .group
            .iter()
            .copied()
            .filter(|&i| {
                let unit = &self.units[i];
                unit.can_attack()
                    && self.rival_of(i).is_none()
                    && unit.planned_pos().distance(target) <= unit.stats().attack_range
            })
            .collect();
        let already = !able.is_empty()
            && able
                .iter()
                .all(|&i| self.units[i].planned_attack == Some(target));
        for i in able {
            let unit = &mut self.units[i];
            unit.planned_attack = (!already).then_some(target);
            unit.guarding = false;
        }
    }

    /// Sends every member toward `target`, replacing their queued moves: each
    /// takes the reachable hex nearest the target that nobody else on its
    /// side is heading for, nearest members first. One that can't get any
    /// closer stays put.
    fn group_move(&mut self, target: Hex) {
        let mut members = self.group.clone();
        for &i in &members {
            self.cancel_swap(i);
            self.units[i].planned_move = None;
            self.units[i].cancel_queue();
        }
        let team = self.units[members[0]].team;
        let mut claimed: HashSet<Hex> = self
            .units
            .iter()
            .filter(|u| u.team == team)
            .filter_map(|u| u.planned_move)
            .collect();

        members.sort_by_key(|&i| (self.units[i].pos.distance(target), i));
        let fog = self.fog();
        for i in members {
            let start = self.units[i].pos;
            // Staying put wins ties, so nobody shuffles sideways for nothing.
            let best = self
                .known_reachable_hexes(
                    start,
                    self.units[i].stats().move_range,
                    self.units[i].team,
                    &fog,
                )
                .into_iter()
                .filter(|hex| *hex == start || !claimed.contains(hex))
                .min_by_key(|hex| (hex.distance(target), hex.distance(start), hex.q, hex.r));
            if let Some(dest) = best.filter(|&dest| dest != start) {
                claimed.insert(dest);
                let unit = &mut self.units[i];
                unit.planned_move = Some(dest);
                unit.drop_unreachable_attack();
                unit.guarding = false;
            }
        }
    }

    /// Space or Hold with a group: every member holds, and selection moves on.
    pub(super) fn hold_group(&mut self) {
        for &i in &self.group {
            self.units[i].holding = true;
        }
        self.group.clear();
        self.select_next_or_end_turn(None);
    }

    /// G or Guard with a group: every member guards, or if they all already
    /// are, they all stop.
    pub(super) fn toggle_group_guard(&mut self) {
        let all_guarding = self.group.iter().all(|&i| self.units[i].guarding);
        for &i in &self.group {
            self.units[i].guarding = !all_guarding;
            self.units[i].cancel_queue();
        }
        if !all_guarding {
            self.group.clear();
            self.select_next_or_end_turn(None);
        }
    }

    /// Ctrl-right-click with a group: clears every member's orders.
    pub(super) fn clear_group_orders(&mut self) {
        for i in self.group.clone() {
            self.cancel_swap(i);
            self.units[i].clear_orders();
            self.units[i].guarding = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::unit::{Team, UnitType};

    const SCREEN: Vec2 = Vec2::new(1600.0, 900.0);

    fn blue(game: &GameState) -> Vec<usize> {
        (0..game.units.len())
            .filter(|&i| game.units[i].team == Team::Blue)
            .collect()
    }

    #[test]
    fn a_box_around_the_army_selects_it_as_a_group() {
        let mut game = GameState::new();
        game.select_in_box(Vec2::ZERO, SCREEN / Vec2::new(2.0, 1.0), SCREEN);
        let mut group = game.group.clone();
        group.sort();
        assert_eq!(
            group,
            blue(&game),
            "only the player's units, all on the left"
        );
        assert_eq!(game.selected, None);

        // Alt-clicking members back out leaves an ordinary single selection.
        let units = blue(&game);
        for &i in &units[1..] {
            let cursor = game
                .camera
                .world_to_screen(game.units[i].pos.to_world(), SCREEN);
            game.toggle_in_selection(cursor, SCREEN);
        }
        assert!(game.group.is_empty());
        assert_eq!(game.selected, Some(units[0]));
    }

    #[test]
    fn a_group_converges_on_the_target_without_sharing_hexes() {
        let mut game = GameState::new();
        let units = blue(&game);
        game.set_selection(units.clone());
        let target = Hex::new(0, 1);
        let before: Vec<i32> = units
            .iter()
            .map(|&i| game.units[i].pos.distance(target))
            .collect();

        game.group_order(target, ClickMode::Normal);
        let destinations: Vec<Hex> = units
            .iter()
            .filter_map(|&i| game.units[i].planned_move)
            .collect();
        let distinct: HashSet<Hex> = destinations.iter().copied().collect();
        assert_eq!(distinct.len(), destinations.len(), "no two share a hex");
        for (&i, before) in units.iter().zip(before) {
            let after = game.units[i].planned_pos().distance(target);
            assert!(after <= before, "nobody ends up farther away");
        }
        assert!(!destinations.is_empty());
    }

    #[test]
    fn only_members_in_range_join_a_group_attack() {
        let mut game = GameState::new();
        let melee = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == UnitType::Melee)
            .unwrap();
        let ranged = game
            .units
            .iter()
            .position(|u| u.team == Team::Blue && u.unit_type == UnitType::Ranged)
            .unwrap();
        game.set_selection(vec![melee, ranged]);
        // Two hexes from both: only the ranged unit reaches.
        let target = Hex::new(0, 0);
        assert_eq!(game.units[melee].pos.distance(target), 2);
        assert_eq!(game.units[ranged].pos.distance(target), 2);

        game.group_order(target, ClickMode::Attack);
        assert_eq!(game.units[ranged].planned_attack, Some(target));
        assert_eq!(game.units[melee].planned_attack, None);

        // Again calls it off.
        game.group_order(target, ClickMode::Attack);
        assert_eq!(game.units[ranged].planned_attack, None);
    }
}
