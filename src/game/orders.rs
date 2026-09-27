//! Player input and order planning. Orders only take effect when the turn resolves.

use glam::Vec2;

use super::GameState;
use super::hex::Hex;

/// What a left-click on a hex should do, based on the modifier keys held.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickMode {
    /// Select a unit, or queue a move or attack for the selected one.
    Normal,
    /// Move only, chosen from the unit command tray.
    Move,
    /// Attack the clicked hex, occupied or not.
    Attack,
    /// Swap places with the clicked adjacent ally.
    Swap,
}

impl GameState {
    /// M or the Move button: the next map click only moves.
    pub fn choose_move_action(&mut self) {
        self.toggle_ui_click_mode(ClickMode::Move, "MOVE: CLICK A GREEN HEX");
    }

    /// X or the Attack button: the next map click attacks that hex.
    pub fn choose_attack_action(&mut self) {
        self.toggle_ui_click_mode(ClickMode::Attack, "ATTACK: CLICK A TARGET HEX");
    }

    /// The Swap button: the next map click swaps with that adjacent ally.
    pub fn choose_swap_action(&mut self) {
        self.toggle_ui_click_mode(ClickMode::Swap, "SWAP: CLICK AN ADJACENT ALLY");
    }

    /// Arms `mode` for the next map click, or disarms it if it's already armed.
    fn toggle_ui_click_mode(&mut self, mode: ClickMode, notice: &str) {
        if self.is_resolving() || self.selection().is_empty() {
            return;
        }
        if self.ui_click_mode == Some(mode) {
            self.ui_click_mode = None;
        } else {
            self.ui_click_mode = Some(mode);
            self.notice = notice.into();
        }
    }

    /// Left-click: selects one of the player's units, or queues an order for
    /// the selected one. Clicking an already-queued order again cancels it.
    /// An action armed from the command tray applies to this map click only,
    /// unless a modifier key picked `mode` itself. Once the selected unit has
    /// nothing left to plan, selection moves on to the next unit that does.
    /// Ignored while a turn is playing out.
    pub fn handle_click(&mut self, cursor: Vec2, screen_size: Vec2, mode: ClickMode) {
        if self.is_resolving() {
            return;
        }
        if self.click_ui(cursor, screen_size) {
            return;
        }
        self.handle_map_click(cursor, screen_size, mode);
    }

    /// Map clicks after an external UI (such as ImGui) has handled its own hit testing.
    pub fn handle_map_click(&mut self, cursor: Vec2, screen_size: Vec2, mode: ClickMode) {
        if self.is_resolving() {
            return;
        }
        if self.interior_view.is_some() {
            if let Some(tile) = self.hex_at_screen(cursor, screen_size) {
                self.interior_click(tile);
            }
            return;
        }
        let armed = self.ui_click_mode.take();
        let mode = match (mode, armed) {
            (ClickMode::Normal, Some(armed)) => armed,
            _ => mode,
        };
        // Clicking off the map deselects, and leaves the city view.
        let Some(hex) = self.hex_at_screen(cursor, screen_size) else {
            self.set_selection(Vec::new());
            self.leave_city_view();
            return;
        };

        // The preview badge is smaller than the hex. On a worked tile its
        // center picks up the planned building; the rest remains available
        // for manager and citizen clicks.
        if mode == ClickMode::Normal
            && let Some(city) = self.selected_city
            && self.placing_building.is_none()
            && self.moving_manager.is_none()
            && self
                .camera
                .screen_to_world(cursor, screen_size)
                .distance(hex.to_world())
                <= 0.34
            && let Some(building) = [
                super::city::Building::Barracks,
                super::city::Building::Mill,
                super::city::Building::Workshop,
            ]
            .into_iter()
            .find(|building| self.cities[city].planned_sites.get(building) == Some(&hex))
        {
            self.change_selected_building_site(building);
            return;
        }

        if self.city_click(hex) {
            return;
        }

        let ally = self.controlled_unit_at(hex);
        // With a group selected, clicking one of your units picks just it;
        // anything else is an order for the whole group.
        if !self.group.is_empty() {
            match ally {
                Some(ally) if mode == ClickMode::Normal => self.set_selection(vec![ally]),
                _ => self.group_order(hex, mode),
            }
            return;
        }
        let Some(selected) = self.selected else {
            self.selected = ally;
            return;
        };

        match (mode, ally) {
            // Selecting never auto-advances, so a finished unit can be reselected to edit.
            (ClickMode::Normal, Some(ally)) if ally == selected => self.selected = None,
            (ClickMode::Normal, Some(ally)) => self.selected = Some(ally),
            (ClickMode::Swap, None) => {}
            (ClickMode::Attack, _) => {
                self.try_queue_attack(selected, hex);
                self.advance_selection_if_done();
            }
            (ClickMode::Swap, Some(ally)) => {
                self.try_queue_swap(selected, ally);
                self.advance_selection_if_done();
            }
            (ClickMode::Move, None) => {
                self.try_queue_move(selected, hex);
                self.advance_selection_if_done();
            }
            (ClickMode::Move, _) => {}
            (ClickMode::Normal, None) => {
                self.queue_order_at(selected, hex);
                self.advance_selection_if_done();
            }
        }
    }

    /// Space: holds the selected unit (or group) if it still needs orders,
    /// moving on to whatever else does. Once nothing does, ends the turn.
    pub fn hold_or_end_turn(&mut self) {
        if self.is_resolving() {
            return;
        }
        if !self.group.is_empty() {
            self.hold_group();
            return;
        }
        let selected_needs_orders = self.selected.is_some_and(|idx| self.needs_orders(idx));
        if selected_needs_orders || self.pending() != (0, 0) {
            self.hold_selected_unit();
        } else {
            self.end_planning();
        }
    }

    /// The Hold button: the selected unit holds, leaving any move or attack
    /// it hasn't queued unused this turn, and selection moves on.
    pub fn hold_selected_unit(&mut self) {
        if self.is_resolving() {
            return;
        }
        if !self.group.is_empty() {
            self.hold_group();
            return;
        }
        if let Some(idx) = self.selected {
            self.units[idx].holding = true;
        }
        self.select_next_or_end_turn(self.selected);
    }

    /// G or the Guard button: the selected unit stays put and is skipped in
    /// the turn order every turn until it's given an order, or G unguards it.
    pub fn toggle_guard(&mut self) {
        if self.is_resolving() {
            return;
        }
        if !self.group.is_empty() {
            self.toggle_group_guard();
            return;
        }
        let Some(idx) = self.selected else { return };
        let unit = &mut self.units[idx];
        unit.guarding = !unit.guarding;
        if unit.guarding {
            self.select_next_or_end_turn(Some(idx));
        }
    }

    /// How many of the player's units still need orders, and how many of
    /// their cities still need something to build. The turn can't end until
    /// both are zero; assigning citizens never holds it up.
    pub(super) fn pending(&self) -> (usize, usize) {
        let units = (0..self.units.len())
            .filter(|&i| self.is_player_controlled(i) && self.needs_orders(i))
            .count();
        let cities = (0..self.cities.len())
            .filter(|&i| self.city_needs_build(i))
            .count();
        (units, cities)
    }

    /// Tab: selects the next unit that still needs orders, or just the next
    /// unit if they all have them, without holding the current one.
    pub fn select_next_unit(&mut self) {
        if self.is_resolving() {
            return;
        }
        self.leave_city_view();
        let next = self
            .next_unit_needing_orders(self.selected)
            .or_else(|| self.next_player_unit(self.selected));
        self.select_and_focus(next);
    }

    /// Once the selected unit has nothing left to plan, moves selection on to
    /// the next unit that does, leaving city planning open if there are none.
    pub(super) fn advance_selection_if_done(&mut self) {
        if let Some(idx) = self.selected
            && !self.needs_orders(idx)
        {
            self.select_next_or_end_turn(Some(idx));
        }
    }

    /// Selects the next unit after `after` that still needs orders. If none
    /// do, opens a city that needs something to build; failing that, clears
    /// the selection and waits for the player to end the turn.
    pub(super) fn select_next_or_end_turn(&mut self, after: Option<usize>) {
        if let Some(next) = self.next_unit_needing_orders(after) {
            self.select_and_focus(Some(next));
        } else if let Some(city) = (0..self.cities.len()).find(|&i| self.city_needs_build(i)) {
            self.open_city(city);
        } else {
            self.selected = None;
        }
    }

    /// Selects `idx` and glides the camera to it. Used when the game picks the
    /// unit, not when the player clicks one they can already see.
    fn select_and_focus(&mut self, idx: Option<usize>) {
        self.selected = idx;
        self.group.clear();
        self.ui_click_mode = None;
        if let Some(idx) = idx {
            self.selected_city = None;
            self.camera.focus_on(self.units[idx].pos.to_world());
        }
    }

    /// The first of the player's units after `after` (in unit order, wrapping
    /// around) that still needs orders. Starts from the first unit if `after`
    /// is `None`.
    pub(super) fn next_unit_needing_orders(&self, after: Option<usize>) -> Option<usize> {
        self.player_units_after(after)
            .find(|&i| self.needs_orders(i))
    }

    fn next_player_unit(&self, after: Option<usize>) -> Option<usize> {
        self.player_units_after(after).next()
    }

    /// The player's units in unit order, starting just after `after` and
    /// wrapping around, not including `after` itself.
    fn player_units_after(&self, after: Option<usize>) -> impl Iterator<Item = usize> + '_ {
        let count = self.units.len();
        let start = after.map_or(0, |i| i + 1);
        (0..count)
            .map(move |step| (start + step) % count)
            .filter(move |&i| Some(i) != after && self.is_player_controlled(i))
    }

    /// Whether the unit still has something to plan: a move or an attack it
    /// could queue but hasn't. Any hex in range can be attacked, so a unit
    /// that can attack needs orders until it does (or holds, or guards). A
    /// unit locked in a contested hex already has its fight, so it's done.
    pub(super) fn needs_orders(&self, idx: usize) -> bool {
        let unit = &self.units[idx];
        if unit.holding || unit.guarding || self.rival_of(idx).is_some() {
            return false;
        }
        let may_move = unit.planned_move.is_none() && unit.stats().move_range > 0;
        let may_attack =
            unit.planned_attack.is_none() && unit.can_attack() && !self.workers.contains(&unit.id);
        may_move || may_attack
    }

    /// Ctrl-right-click: clears all of the selected unit's (or group's)
    /// orders, including a hold or guard.
    pub fn handle_right_click(&mut self) {
        if self.is_resolving() {
            return;
        }
        if !self.group.is_empty() {
            self.clear_group_orders();
            return;
        }
        if let Some(selected) = self.selected {
            self.cancel_swap(selected);
            self.units[selected].clear_orders();
            self.units[selected].guarding = false;
        }
    }

    /// Context order: right-click moves to an open hex or attacks an enemy.
    /// Ctrl-right-click retains the explicit clear-order behavior. With an
    /// action armed, right-click just disarms it.
    pub fn handle_context_click(&mut self, cursor: Vec2, screen_size: Vec2, clear: bool) {
        if self.is_resolving() {
            return;
        }
        if self.interior_view.is_some() {
            if clear {
                self.clear_selected_interior_orders();
            } else if let Some(tile) = self.hex_at_screen(cursor, screen_size) {
                self.interior_click(tile);
            }
            return;
        }
        if self.ui_click_mode.take().is_some() {
            return;
        }
        if clear {
            self.handle_right_click();
            return;
        }
        if !self.group.is_empty() {
            if let Some(hex) = self.hex_at_screen(cursor, screen_size) {
                self.group_order(hex, ClickMode::Normal);
            }
            return;
        }
        let Some(selected) = self.selected else {
            return;
        };
        let Some(hex) = self.hex_at_screen(cursor, screen_size) else {
            return;
        };
        self.queue_order_at(selected, hex);
        self.advance_selection_if_done();
    }

    /// A plain click or right-click on `hex`: attacks an enemy the player
    /// knows is there, and otherwise moves there. An enemy out of sight isn't
    /// known, so clicking its hex plans a move.
    pub(super) fn queue_order_at(&mut self, idx: usize, hex: Hex) {
        if self.known_enemy_target_at(hex, self.units[idx].team, &self.fog()) {
            self.try_queue_attack(idx, hex);
        } else {
            self.try_queue_move(idx, hex);
        }
    }

    pub(super) fn hex_at_screen(&self, cursor: Vec2, screen_size: Vec2) -> Option<Hex> {
        if screen_size.min_element() <= 0.0 {
            return None;
        }
        let hex = Hex::from_world(self.camera.screen_to_world(cursor, screen_size));
        if self.interior_view.is_some() {
            (hex.distance(Hex::new(0, 0)) <= 2).then_some(hex)
        } else {
            self.grid.contains(hex).then_some(hex)
        }
    }

    /// Toggles a move to `dest`, if there's a path to it within range and no
    /// other friendly unit is already heading there.
    pub(super) fn try_queue_move(&mut self, idx: usize, dest: Hex) {
        let unit = &self.units[idx];
        let reachable = self.known_reachable_hexes(unit.pos, unit.stats().move_range, &self.fog());
        let claimed_by_ally = self
            .units
            .iter()
            .any(|u| u.team == unit.team && u.id != unit.id && u.planned_move == Some(dest));
        if !reachable.contains(&dest) || claimed_by_ally {
            return;
        }

        self.cancel_swap(idx);
        let unit = &mut self.units[idx];
        unit.planned_move = if unit.planned_move == Some(dest) {
            None
        } else {
            Some(dest)
        };
        unit.drop_unreachable_attack();
        // Any order wakes a guarding unit.
        unit.guarding = false;
    }

    /// Toggles an attack on `target`, measured from the unit's planned
    /// position so move-then-attack works. Mountains can't be targeted since
    /// no one can stand there, and a unit locked in a contested hex can only
    /// fight its rival there.
    pub(super) fn try_queue_attack(&mut self, idx: usize, target: Hex) {
        let unable = !self.units[idx].can_attack()
            || self.workers.contains(&self.units[idx].id)
            || self.rival_of(idx).is_some();
        if unable || !self.grid.is_passable(target) {
            return;
        }
        let unit = &mut self.units[idx];
        if unit.planned_pos().distance(target) <= unit.stats().attack_range {
            unit.planned_attack = if unit.planned_attack == Some(target) {
                None
            } else {
                Some(target)
            };
            unit.guarding = false;
        }
    }

    /// Toggles a swap between `idx` and the adjacent ally `ally`: each moves
    /// into the other's hex, when whichever of the two moves first acts.
    /// Replaces any moves either had queued. Both units must be free to move.
    pub(super) fn try_queue_swap(&mut self, idx: usize, ally: usize) {
        let (unit, other) = (&self.units[idx], &self.units[ally]);
        let free_to_move =
            |i: usize| self.units[i].stats().move_range > 0 && self.rival_of(i).is_none();
        let allowed = idx != ally
            && unit.team == other.team
            && unit.pos.distance(other.pos) == 1
            && free_to_move(idx)
            && free_to_move(ally);
        if !allowed {
            return;
        }

        let (unit_pos, ally_pos) = (unit.pos, other.pos);
        let already_swapping = self.swap_partner(idx) == Some(ally);
        self.cancel_swap(idx);
        self.cancel_swap(ally);
        if !already_swapping {
            self.units[idx].planned_move = Some(ally_pos);
            self.units[ally].planned_move = Some(unit_pos);
        }
        for i in [idx, ally] {
            self.units[i].drop_unreachable_attack();
            self.units[i].guarding = false;
        }
    }

    /// Toggles the selected unit's ability for this turn, if it's off cooldown.
    pub fn toggle_selected_ability(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(idx) = self.selected else { return };
        let unit = &mut self.units[idx];
        if unit.ability_cooldown > 0 {
            return;
        }
        unit.ability_queued = !unit.ability_queued;
        self.drop_orders_now_impossible(idx);
        self.advance_selection_if_done();
    }

    /// After a unit's abilities change what it can do, drops any queued move
    /// or attack it can no longer carry out.
    pub(super) fn drop_orders_now_impossible(&mut self, idx: usize) {
        let unit = &self.units[idx];
        let move_range = unit.stats().move_range;
        let move_still_possible = match (unit.planned_move, self.swap_partner(idx)) {
            (None, _) => true,
            (Some(_), Some(_)) => move_range > 0,
            (Some(dest), None) => self
                .known_reachable_hexes(unit.pos, move_range, &self.fog())
                .contains(&dest),
        };
        if !move_still_possible {
            self.cancel_swap(idx);
            self.units[idx].planned_move = None;
        }
        self.units[idx].drop_unreachable_attack();
    }

    /// If `idx` is swapping with an ally, cancels both halves of the swap.
    pub(super) fn cancel_swap(&mut self, idx: usize) {
        if let Some(partner) = self.swap_partner(idx) {
            for i in [idx, partner] {
                self.units[i].planned_move = None;
                self.units[i].drop_unreachable_attack();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::city::Building;

    #[test]
    fn tab_closes_the_barracks_view_and_its_placement_modes() {
        let mut g = GameState::city_scenario();
        g.cities[0].barracks = Some(Hex::new(-1, 0));
        g.open_barracks(0);
        g.moving_manager = Some(0);
        g.placing_building = Some((0, Building::Barracks));
        g.select_next_unit();
        assert_eq!(g.selected_barracks, None);
        assert_eq!(g.selected_city, None);
        assert_eq!(g.moving_manager, None);
        assert_eq!(g.placing_building, None);
        assert!(g.selected.is_some(), "Tab still moves on to a unit");
    }

    #[test]
    fn tab_from_the_city_view_drops_a_building_site_preview() {
        let mut g = GameState::city_scenario();
        g.open_city(0);
        g.queue_selected_city_building(Building::Barracks);
        assert!(g.placing_building.is_some());
        g.select_next_unit();
        assert_eq!(g.selected_city, None);
        assert_eq!(g.placing_building, None);
    }
}
