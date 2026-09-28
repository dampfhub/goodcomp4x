//! Ordering several units at once. Drag a box on the map (or Shift-click
//! units, or Shift-click them in the unit strip) to select a group, and
//! Ctrl-click a member to take it back out. Clicking a hex then sends every member toward it at its own
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
    /// Left-drag: selects the player's units drawn inside the rectangle
    /// between `a` and `b` (window pixels, origin top-left). With `add`
    /// (Shift held), they join the current selection instead of replacing it.
    pub fn select_in_box(&mut self, a: Vec2, b: Vec2, screen_size: Vec2, add: bool) {
        if self.is_resolving() || self.interior_view.is_some() {
            return;
        }
        let (min, max) = (a.min(b), a.max(b));
        let mut members = if add { self.selection() } else { Vec::new() };
        for i in 0..self.units.len() {
            let drawn_at = self
                .camera
                .world_to_screen(self.unit_layout(i).0, screen_size);
            if self.is_player_controlled(i)
                && drawn_at.cmpge(min).all()
                && drawn_at.cmple(max).all()
                && !members.contains(&i)
            {
                members.push(i);
            }
        }
        self.set_selection(members);
    }

    /// Shift-click (on the map or in the unit strip): adds unit `idx` to the
    /// selection. Nothing selected, it's selected on its own.
    pub(super) fn add_to_selection(&mut self, idx: usize) {
        let mut members = self.selection();
        if !members.contains(&idx) {
            members.push(idx);
        }
        self.set_selection(members);
    }

    /// Ctrl-click with several units selected: takes unit `idx` out of the
    /// selection, leaving an ordinary single selection once one is left.
    /// Returns whether it was a member.
    pub(super) fn remove_from_selection(&mut self, idx: usize) -> bool {
        let mut members = self.selection();
        let Some(at) = members.iter().position(|&i| i == idx) else {
            return false;
        };
        members.remove(at);
        self.set_selection(members);
        true
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
        // Land, or water (for ships): anything on the map but mountains.
        let water = self.grid.contains(target) && self.grid.terrain(target).is_water();
        if !(self.grid.is_passable(target) || water) {
            return;
        }
        if let Some(&first) = self.group.first()
            && self.empty_city_target(target, self.units[first].team)
        {
            self.notice = "CITY CENTER CAN ONLY BE CAPTURED FROM ITS INTERIOR".into();
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
                self.attack_target_legal(i, target, false)
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
            unit.holding = false;
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
                .known_reachable_for_domain(
                    start,
                    self.units[i].stats().move_range,
                    self.units[i].team,
                    &fog,
                    self.units[i].is_naval(),
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
                unit.holding = false;
            }
        }
    }

    /// Space or Hold with a group: every member holds, and selection moves on;
    /// if they all already hold, they all stop.
    pub(super) fn hold_group(&mut self) {
        // Every member already holding: they all stop, and stay selected.
        if self.group.iter().all(|&i| self.units[i].holding) {
            for &i in &self.group {
                self.units[i].holding = false;
            }
            return;
        }
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
            self.units[i].holding = false;
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
        game.select_in_box(Vec2::ZERO, SCREEN / Vec2::new(2.0, 1.0), SCREEN, false);
        let mut group = game.group.clone();
        group.sort();
        assert_eq!(
            group,
            blue(&game),
            "only the player's units, all on the left"
        );
        assert_eq!(game.selected, None);

        // Ctrl-clicking members back out leaves an ordinary single selection.
        let units = blue(&game);
        for &i in &units[1..] {
            let cursor = game
                .camera
                .world_to_screen(game.units[i].pos.to_world(), SCREEN);
            game.handle_map_click(cursor, SCREEN, ClickMode::Swap);
        }
        assert!(game.group.is_empty());
        assert_eq!(game.selected, Some(units[0]));

        // Shift-clicking them adds them back, one at a time.
        for &i in &units[1..] {
            let cursor = game
                .camera
                .world_to_screen(game.units[i].pos.to_world(), SCREEN);
            game.handle_map_click(cursor, SCREEN, ClickMode::QueueMove);
        }
        let mut group = game.group.clone();
        group.sort();
        assert_eq!(group, units);
        assert!(
            units.iter().all(|&i| !game.units[i].has_queue()),
            "adding a unit queues nothing"
        );
    }

    #[test]
    fn a_shift_drag_adds_to_the_selection_and_a_plain_one_replaces_it() {
        let mut game = GameState::new();
        let units = blue(&game);
        game.set_selection(vec![units[0]]);
        // An empty corner of the screen adds nobody.
        game.select_in_box(Vec2::ZERO, Vec2::splat(4.0), SCREEN, true);
        assert_eq!(game.selection(), vec![units[0]]);
        let around = |game: &GameState, i: usize| {
            let at = game
                .camera
                .world_to_screen(game.units[i].pos.to_world(), SCREEN);
            (at - Vec2::splat(5.0), at + Vec2::splat(5.0))
        };
        let (a, b) = around(&game, units[1]);
        game.select_in_box(a, b, SCREEN, true);
        assert_eq!(game.selection(), vec![units[0], units[1]]);
        let (a, b) = around(&game, units[2]);
        game.select_in_box(a, b, SCREEN, false);
        assert_eq!(game.selection(), vec![units[2]]);
        // A plain drag over nothing lets go of everything.
        game.select_in_box(Vec2::ZERO, Vec2::splat(4.0), SCREEN, false);
        assert!(game.selection().is_empty());
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
    fn melee_group_cannot_attack_a_ship() {
        let mut game = GameState::naval_scenario();
        game.units.clear();
        game.cities.clear();
        let target = game
            .grid
            .all_hexes()
            .find(|&h| {
                game.grid.terrain(h).is_water()
                    && h.neighbors()
                        .into_iter()
                        .filter(|&n| game.grid.is_passable(n))
                        .count()
                        >= 2
            })
            .unwrap();
        let land: Vec<_> = target
            .neighbors()
            .into_iter()
            .filter(|&h| game.grid.is_passable(h))
            .take(2)
            .collect();
        for (id, pos) in land.into_iter().enumerate() {
            game.units.push(crate::game::unit::Unit::new(
                id as u32,
                pos,
                Team::Blue,
                UnitType::Melee,
            ));
        }
        game.units.push(crate::game::unit::Unit::new(
            9,
            target,
            Team::Red,
            UnitType::PatrolGalley,
        ));
        game.set_selection(vec![0, 1]);
        game.group_order(target, ClickMode::Attack);
        assert!(
            game.units[..2]
                .iter()
                .all(|unit| unit.planned_attack.is_none())
        );
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
