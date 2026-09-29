//! Player input and order planning. Orders only take effect when the turn resolves.

use glam::Vec2;

use super::GameState;
use super::fog::Fog;
use super::hex::Hex;

/// Why a land troop that isn't ranged or siege can't attack a water hex.
pub(super) const SHIPS_NOTICE: &str = "ONLY RANGED AND SIEGE LAND TROOPS CAN ATTACK SHIPS";

/// Why a unit can't go on alert.
pub(super) const ALERT_NOTICE: &str =
    "ONLY MELEE, CAVALRY, ARMORED, RANGED AND SET-UP SIEGE CAN GO ON ALERT";

/// A click that would replace the selection's multi-turn queue, remembered
/// until it's repeated: which hex, whether it was an attack, for which
/// units, on which turn.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(super) struct QueueReplace {
    hex: Hex,
    attack: bool,
    units: Vec<u32>,
    turn: u32,
}

/// What a click on a hex should do, based on the button and the modifier
/// keys held: left-click moves, right-click attacks, Shift adds to the
/// order queue (`order_queue.rs`) instead.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ClickMode {
    /// Left-click: select a unit, or move the selected one.
    Normal,
    /// Move only, chosen from the unit command tray.
    Move,
    /// Attack the clicked hex, occupied or not: right-click, or armed from
    /// the tray.
    Attack,
    /// Ctrl-left-click: swap places with the clicked adjacent ally, or with
    /// several units selected, take the clicked one out of the selection.
    Swap,
    /// Shift-left-click: add the turns moving to the hex to the queue; on one
    /// of the player's units, add it to the selection; on a move the
    /// selection already plans (a queued stop, a ghost), take it off.
    QueueMove,
    /// Shift-right-click: add an attack on the hex to the queue.
    QueueAttack,
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

    /// Left-click: selects one of the player's units, or moves the selected
    /// one (Shift: adds to its queue instead). Clicking an already-queued
    /// order again cancels it.
    /// An action armed from the command tray applies to this map click only,
    /// unless a modifier key picked `mode` itself. Once the selected unit has
    /// nothing left to plan, selection moves on to the next unit that does.
    /// Ignored while a turn is playing out; while a network game waits for
    /// the others' plans, it only selects (`handle_map_click`).
    pub fn handle_click(&mut self, cursor: Vec2, screen_size: Vec2, mode: ClickMode) {
        if self.is_playing_out() {
            return;
        }
        if self.click_ui(cursor, screen_size, mode) {
            return;
        }
        self.handle_map_click(cursor, screen_size, mode);
    }

    /// Map clicks after an external UI (such as ImGui) has handled its own hit testing.
    pub fn handle_map_click(&mut self, cursor: Vec2, screen_size: Vec2, mode: ClickMode) {
        if self.is_playing_out() {
            return;
        }
        if self.interior_view.is_some() {
            if let Some(tile) = self.hex_at_screen(cursor, screen_size) {
                self.interior_click(tile);
            }
            return;
        }
        if self.paint_job_at(cursor, screen_size, false) {
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

        let point = self.camera.screen_to_world(cursor, screen_size);
        if self.city_click_at(hex, point) {
            return;
        }
        // The plan is sent (a network game waiting for the others'): a
        // click only picks what to look at.
        if self.is_resolving() {
            self.select_only(hex, mode);
            return;
        }

        // A selected land troop boards a friendly landing craft; a selected
        // craft lands its first passenger on adjacent open ground. Both orders
        // resolve after combat, so a ship sunk this turn cannot unload.
        if mode == ClickMode::Normal
            && let Some(selected) = self.selected
        {
            if let Some(craft) = self.controlled_unit_at(hex)
                && self.units[craft].unit_type == super::unit::UnitType::LandingCraft
                && !self.units[selected].is_naval()
                && self.units[selected].team == self.units[craft].team
                && self.units[selected].pos.distance(hex) == 1
            {
                self.try_board(selected, craft);
                return;
            }
            if self.units[selected].unit_type == super::unit::UnitType::LandingCraft
                && !self.grid.terrain(hex).is_water()
                && self.grid.is_passable(hex)
                && self.units[selected].pos.distance(hex) == 1
            {
                self.try_land(selected, hex);
                return;
            }
        }
        let ally = self.controlled_unit_at(hex);
        // Shift-clicking one of your units adds it to the selection, and
        // Ctrl-clicking a member of a group takes it out (with one unit
        // selected, Ctrl-click still swaps).
        if let Some(ally) = ally {
            if mode == ClickMode::QueueMove {
                self.add_to_selection(ally);
                return;
            }
            if mode == ClickMode::Swap && !self.group.is_empty() && self.remove_from_selection(ally)
            {
                return;
            }
        }
        // A plain order that would replace a multi-turn queue needs the same
        // click twice, so looking at a plan and clicking away can't ruin it.
        let replaces_queue = match (mode, ally) {
            (ClickMode::Normal, None) | (ClickMode::Move, None) | (ClickMode::Attack, _) => true,
            (ClickMode::Swap, Some(_)) => self.group.is_empty(),
            _ => false,
        };
        if replaces_queue && !self.confirm_queue_replace(hex, mode == ClickMode::Attack) {
            return;
        }
        // With a group selected, a plain click on one of your units picks
        // just it; anything else is an order for the whole group.
        if !self.group.is_empty() {
            match ally {
                Some(ally) if mode == ClickMode::Normal => self.set_selection(vec![ally]),
                _ => self.group_order(hex, mode),
            }
            return;
        }
        let Some(selected) = self.selected else {
            // With nothing selected, a click selects your unit there, if any.
            // Workers are given jobs from their city's production list.
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
            // Queuing never moves selection on: the player keeps adding
            // turns until they let go of the unit.
            (ClickMode::QueueMove, _) => {
                self.queue_or_unqueue_move(hex);
            }
            (ClickMode::QueueAttack, _) => {
                self.queue_attack(hex);
            }
        }
    }

    /// A map click on `hex` that changes nothing but the selection, while
    /// the plan can't change: one of the player's units there is selected
    /// (Shift adds it, Ctrl takes it out of a group); anywhere else lets go.
    fn select_only(&mut self, hex: Hex, mode: ClickMode) {
        match (mode, self.controlled_unit_at(hex)) {
            (ClickMode::QueueMove, Some(ally)) => self.add_to_selection(ally),
            (ClickMode::Swap, Some(ally)) if !self.group.is_empty() => {
                self.remove_from_selection(ally);
            }
            (_, ally) => self.set_selection(ally.into_iter().collect()),
        }
    }

    /// Whether a plain order on `hex` (an attack if `attack`) may go ahead
    /// for the selection. If a selected unit follows a queue reaching past
    /// this turn, which the order would replace, the first such click only
    /// warns and marks the hex; the same click again goes through. Any other
    /// click leaves the queue alone.
    pub(super) fn confirm_queue_replace(&mut self, hex: Hex, attack: bool) -> bool {
        let members = self.selection();
        let queued = members.iter().any(|&i| self.units[i].plans_later_turns());
        let pending = QueueReplace {
            hex,
            attack,
            units: members.iter().map(|&i| self.units[i].id).collect(),
            turn: self.turn,
        };
        if !queued || self.queue_replace_armed.as_ref() == Some(&pending) {
            self.queue_replace_armed = None;
            return true;
        }
        self.queue_replace_armed = Some(pending);
        // The warning shows through `shown_notice`, only while it applies.
        false
    }

    /// What the top bar says: the warning that a click is waiting to be
    /// repeated to replace a queue, while one is, or else the latest notice.
    pub(super) fn shown_notice(&self) -> &str {
        match self.queue_replace_hex() {
            Some(_) if self.group.is_empty() => "CLICK AGAIN TO REPLACE ITS QUEUE",
            Some(_) => "CLICK AGAIN TO REPLACE THEIR QUEUES",
            None => &self.notice,
        }
    }

    /// The hex a click is waiting to be repeated on to replace the selection's
    /// queue, while it still applies (same selection, same turn).
    pub(super) fn queue_replace_hex(&self) -> Option<Hex> {
        let pending = self.queue_replace_armed.as_ref()?;
        let units: Vec<u32> = self.selection().iter().map(|&i| self.units[i].id).collect();
        (pending.units == units && pending.turn == self.turn).then_some(pending.hex)
    }

    /// Escape, once no structure menu is open: lets go of the selected unit
    /// or group. Returns whether there was
    /// anything to let go of.
    pub fn clear_selection(&mut self) -> bool {
        // Something being placed for a city's workers stops first.
        if self.stop_placing() {
            return true;
        }
        let had = self.selected.is_some() || !self.group.is_empty();
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        had
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
        let selected_holding = self.selected.is_some_and(|idx| self.units[idx].holding);
        if selected_needs_orders || selected_holding || self.pending() != (0, 0) {
            self.hold_selected_unit();
        } else {
            self.end_planning();
        }
    }

    /// The Hold button: the selected unit holds, leaving any move or attack
    /// it hasn't queued unused this turn, and selection moves on. On a unit
    /// already holding, it stops holding instead and stays selected, back in
    /// the turn order.
    pub fn hold_selected_unit(&mut self) {
        if self.is_resolving() {
            return;
        }
        if !self.group.is_empty() {
            self.hold_group();
            return;
        }
        if let Some(idx) = self.selected
            && self.units[idx].holding
        {
            self.units[idx].holding = false;
            self.notice = "NO LONGER HOLDING - GIVE IT ORDERS".into();
            return;
        }
        if let Some(idx) = self.selected {
            self.units[idx].holding = true;
        }
        self.select_next_needing_attention(self.selected);
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
        unit.cancel_queue();
        if unit.guarding {
            // Guard and Alert are two stances; one ends the other.
            unit.alert = false;
            self.select_next_needing_attention(Some(idx));
        }
    }

    /// E or the Alert button: the selected unit (or group) goes on alert. It
    /// drops this turn's move and attack and its queue, stays put and is
    /// skipped in the turn order every turn, and in its attack step attacks
    /// an enemy in range (`alert_target`), until it's given another order,
    /// or E takes it off alert. Only troops that can (`can_go_on_alert`).
    pub fn toggle_alert(&mut self) {
        if self.is_resolving() {
            return;
        }
        if !self.group.is_empty() {
            self.toggle_group_alert();
            return;
        }
        let Some(idx) = self.selected.filter(|&i| self.is_player_controlled(i)) else {
            return;
        };
        if self.units[idx].alert {
            self.units[idx].alert = false;
            return;
        }
        if !self.can_go_on_alert(idx) {
            self.notice = ALERT_NOTICE.into();
            return;
        }
        self.go_on_alert(idx);
        self.select_next_needing_attention(Some(idx));
    }

    /// Whether unit `idx` can go on alert: a troop that fights on land
    /// (`Unit::alert_capable`: siege only set up), not a settler.
    pub(super) fn can_go_on_alert(&self, idx: usize) -> bool {
        let unit = &self.units[idx];
        unit.alert_capable() && !self.settlers.contains(&unit.id)
    }

    /// Puts unit `idx` on alert, dropping whatever else it had planned but
    /// its ability (a siege setting up keeps setting up).
    pub(super) fn go_on_alert(&mut self, idx: usize) {
        self.cancel_swap(idx);
        let unit = &mut self.units[idx];
        unit.take_new_order();
        unit.planned_move = None;
        unit.planned_attack = None;
        unit.cancel_queue();
        unit.alert = true;
    }

    /// Delete or the Disband button: removes the selected unit for good. The
    /// first press only arms it (the button asks to confirm); a second press
    /// on the same unit disbands it, and selection moves on. A landing craft
    /// goes with its passengers, which the first press warns of.
    pub fn disband_selected(&mut self) {
        if self.is_resolving() {
            return;
        }
        let Some(idx) = self.selected.filter(|&i| self.is_player_controlled(i)) else {
            return;
        };
        let id = self.units[idx].id;
        if self.disband_armed != Some(id) {
            self.disband_armed = Some(id);
            self.notice = match self.units[idx].cargo.len() {
                0 => "PRESS DISBAND (OR DELETE) AGAIN TO REMOVE THIS UNIT".into(),
                1 => {
                    "PRESS DISBAND (OR DELETE) AGAIN TO REMOVE THIS CRAFT AND ITS PASSENGER".into()
                }
                n => format!(
                    "PRESS DISBAND (OR DELETE) AGAIN TO REMOVE THIS CRAFT AND ITS {n} PASSENGERS"
                ),
            };
            return;
        }
        self.disband_armed = None;
        let unit = self.units.remove(idx);
        self.discard_interior_copies_of_dead_units();
        for gone in std::iter::once(&unit).chain(&unit.cargo) {
            self.settlers.remove(&gone.id);
            self.player_controlled_units.remove(&gone.id);
        }
        for passenger in &unit.cargo {
            log::info!("{passenger} disbanded with its landing craft");
        }
        log::info!("{unit} disbanded");
        self.notice = format!("{} DISBANDED", self.unit_role(&unit));
        // Indices after it shifted down, so nothing else stays selected.
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.select_next_needing_attention(idx.checked_sub(1));
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
        if self.is_playing_out() {
            return;
        }
        self.leave_city_view();
        let next = self
            .next_unit_needing_orders(self.selected)
            .or_else(|| self.next_player_unit(self.selected));
        self.select_and_focus(next);
    }

    /// Once the selected unit has nothing left to plan, moves on to whatever
    /// needs seeing to next (`select_next_needing_attention`).
    pub(super) fn advance_selection_if_done(&mut self) {
        if let Some(idx) = self.selected
            && !self.needs_orders(idx)
        {
            self.select_next_needing_attention(Some(idx));
        }
    }

    /// Moves on to what needs seeing to next, in the turn strip's order: a
    /// city with nothing to build, opened; or else the next unit after
    /// `after` that still needs orders (`next_unit_needing_orders`). With
    /// neither, lets go of the selection (and any action armed for it) and
    /// waits for the player to end the turn; it never ends the turn itself.
    pub(super) fn select_next_needing_attention(&mut self, after: Option<usize>) {
        // In the turn strip's order: production first, then the units.
        if let Some(city) = (0..self.cities.len()).find(|&i| self.city_needs_build(i)) {
            self.open_city(city);
        } else if let Some(next) = self.next_unit_needing_orders(after) {
            self.select_and_focus(Some(next));
        } else {
            self.set_selection(Vec::new());
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
    /// around) that still needs orders, a settler if any still does. Starts
    /// from the first unit if `after` is `None`.
    pub(super) fn next_unit_needing_orders(&self, after: Option<usize>) -> Option<usize> {
        let waiting: Vec<usize> = self
            .player_units_after(after)
            .filter(|&i| self.needs_orders(i))
            .collect();
        // Civilians (settlers) come first, as in the turn strip.
        waiting
            .iter()
            .copied()
            .find(|&i| self.settlers.contains(&self.units[i].id))
            .or(waiting.first().copied())
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
    /// that can attack needs orders until it does (or holds, guards, is on
    /// alert, follows a queue built with Shift, boards a craft or, a craft,
    /// lands a passenger). A unit locked in a contested hex already has its
    /// fight, so it's done.
    pub(super) fn needs_orders(&self, idx: usize) -> bool {
        let unit = &self.units[idx];
        if unit.holding
            || unit.guarding
            || unit.alert
            || unit.has_queue()
            || unit.planned_board.is_some()
            || unit.planned_unload.is_some()
            || self.rival_of(idx).is_some()
        {
            return false;
        }
        let may_move = unit.planned_move.is_none() && unit.stats().move_range > 0;
        let may_attack = unit.planned_attack.is_none() && unit.can_attack();
        may_move || may_attack
    }

    /// Ctrl-right-click: clears all of the selected unit's (or group's)
    /// orders, including a hold, guard or alert.
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
            self.units[selected].wake();
        }
    }

    /// Right-click: the selected unit (or group) attacks the hex, occupied or
    /// not; with Shift (`queue`), the attack is added to its order queue.
    /// Ctrl-right-click (`clear`) clears its orders instead. With an action
    /// armed, or walls or gates being placed, right-click just stops that.
    pub fn handle_context_click(
        &mut self,
        cursor: Vec2,
        screen_size: Vec2,
        clear: bool,
        queue: bool,
    ) {
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
        if self.stop_placing() {
            return;
        }
        if clear {
            self.handle_right_click();
            return;
        }
        let Some(hex) = self.hex_at_screen(cursor, screen_size) else {
            return;
        };
        let mode = if queue {
            ClickMode::QueueAttack
        } else {
            ClickMode::Attack
        };
        if !queue && !self.confirm_queue_replace(hex, true) {
            return;
        }
        if !self.group.is_empty() {
            self.group_order(hex, mode);
        } else if let Some(selected) = self.selected {
            if queue {
                self.queue_attack(hex);
            } else {
                self.try_queue_attack(selected, hex);
                self.advance_selection_if_done();
            }
        }
    }

    /// A plain left click on `hex` with a unit selected: moves there. An
    /// enemy the player knows is there isn't a move, so they're reminded
    /// that right-click attacks; one out of sight isn't known, so clicking
    /// its hex plans a move.
    pub(super) fn queue_order_at(&mut self, idx: usize, hex: Hex) {
        if self.known_enemy_target_at(hex, self.units[idx].team, &self.fog()) {
            self.notice = "RIGHT-CLICK TO ATTACK".into();
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
        let reachable = self.known_reachable_for_domain(
            unit.pos,
            unit.stats().move_range,
            unit.team,
            &self.fog(),
            unit.is_naval(),
        );
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
        // Any order wakes a guarding (or alert) unit, and replaces a queue.
        unit.take_new_order();
        unit.cancel_queue();
    }

    /// Shared exterior attack rule for the AI's and resolving orders, on the
    /// real board. `later` ignores this turn's deploy and contest lock.
    pub(super) fn attack_target_legal(&self, idx: usize, target: Hex, later: bool) -> bool {
        self.attack_rule(idx, target, later)
            && !self.empty_city_target(target, self.units[idx].team)
    }

    /// `attack_target_legal` as the player knows the board, for the orders
    /// the player plans (direct, group and queued): a city center out of
    /// sight may have someone standing in it, so it's never refused as
    /// empty (`known_empty_city_target`), and whether it's accepted gives
    /// nothing away. If it's empty when the attack comes, it misses.
    pub(super) fn known_attack_target_legal(
        &self,
        idx: usize,
        target: Hex,
        later: bool,
        fog: &Fog,
    ) -> bool {
        self.attack_rule(idx, target, later)
            && !self.known_empty_city_target(target, self.units[idx].team, fog)
    }

    /// Everything in the attack rule but the empty city center: the unit can
    /// attack (this turn, unless `later`), and the target is open ground, or
    /// water for a ship or a ranged or siege land troop.
    fn attack_rule(&self, idx: usize, target: Hex, later: bool) -> bool {
        let unit = &self.units[idx];
        if unit.unit_type == super::unit::UnitType::LandingCraft
            || (!later && (!unit.can_attack() || self.rival_of(idx).is_some()))
            || !(self.grid.is_passable(target)
                || (self.grid.contains(target) && self.grid.terrain(target).is_water()))
        {
            return false;
        }
        !self.grid.terrain(target).is_water() || unit.attacks_water()
    }

    /// Toggles an attack on `target`, measured from the unit's planned
    /// position so move-then-attack works. Mountains can't be targeted since
    /// no one can stand there, and a unit locked in a contested hex can only
    /// fight its rival there.
    pub(super) fn try_queue_attack(&mut self, idx: usize, target: Hex) {
        if self.grid.contains(target)
            && self.grid.terrain(target).is_water()
            && !self.units[idx].attacks_water()
        {
            self.notice = SHIPS_NOTICE.into();
            return;
        }
        let fog = self.fog();
        if self.known_empty_city_target(target, self.units[idx].team, &fog) {
            self.notice = "CITY CENTER CAN ONLY BE CAPTURED FROM ITS INTERIOR".into();
            return;
        }
        if !self.known_attack_target_legal(idx, target, false, &fog) {
            return;
        }
        let unit = &mut self.units[idx];
        if unit.planned_pos().distance(target) <= unit.stats().attack_range {
            unit.planned_attack = if unit.planned_attack == Some(target) {
                None
            } else {
                Some(target)
            };
            unit.take_new_order();
            unit.cancel_queue();
        }
    }

    /// Plans land troop `idx` boarding the adjacent friendly landing craft
    /// `craft` after combat, if the craft has room: four passengers,
    /// counting those already planning to board it. Like any other order,
    /// boarding replaces the troop's move (both halves of a swap), attack,
    /// queue, hold, guard and alert.
    pub(super) fn try_board(&mut self, idx: usize, craft: usize) {
        let (troop_id, craft_id) = (self.units[idx].id, self.units[craft].id);
        let reserved = self
            .units
            .iter()
            .filter(|u| u.id != troop_id && u.planned_board == Some(craft_id))
            .count();
        if self.units[craft].cargo.len() + reserved >= 4 {
            self.notice = "LANDING CRAFT FULL (4 TROOPS)".into();
            return;
        }
        self.cancel_swap(idx);
        let unit = &mut self.units[idx];
        unit.take_new_order();
        unit.planned_move = None;
        unit.planned_attack = None;
        unit.cancel_queue();
        unit.planned_board = Some(craft_id);
        self.notice = "BOARDING LANDING CRAFT AFTER COMBAT".into();
    }

    /// Plans landing craft `craft` landing its first passenger on the
    /// adjacent open land `hex` after combat. The craft must stay where it
    /// is to land it, so landing replaces its move (both halves of a swap),
    /// queue, hold and guard, as any other order does.
    pub(super) fn try_land(&mut self, craft: usize, hex: Hex) {
        if self.units[craft].cargo.is_empty()
            || self.is_occupied(hex)
            || self.cities.iter().any(|city| city.pos == hex)
        {
            return;
        }
        self.cancel_swap(craft);
        let unit = &mut self.units[craft];
        unit.take_new_order();
        unit.planned_move = None;
        unit.planned_attack = None;
        unit.cancel_queue();
        unit.planned_unload = Some(hex);
        self.notice = "LANDING FIRST PASSENGER AFTER COMBAT".into();
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
            self.units[i].take_new_order();
            self.units[i].cancel_queue();
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
        // The queue was planned with the unit's old stats.
        unit.cancel_queue();
        // Like any other order, it ends an alert (a siege packing up
        // couldn't keep one). Set up first, then go on alert.
        unit.alert = false;
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
                .known_reachable_hexes(unit.pos, move_range, unit.team, &self.fog())
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
    use crate::game::JobKind;
    use crate::game::city::Building;
    use crate::game::unit::{Team, Unit, UnitType};

    #[test]
    fn a_held_unit_can_be_unheld_or_given_orders_again() {
        let mut g = GameState::new();
        g.fog_of_war = false;
        let first = g.selected.expect("a unit selected");
        g.hold_selected_unit();
        assert!(g.units[first].holding);
        assert_ne!(g.selected, Some(first), "holding moves on");

        // Hold again on the held unit puts it back in the turn order.
        g.selected = Some(first);
        g.hold_selected_unit();
        assert!(!g.units[first].holding);
        assert_eq!(g.selected, Some(first), "it stays selected");
        assert!(g.needs_orders(first));

        // Space does the same.
        g.hold_selected_unit();
        g.selected = Some(first);
        g.hold_or_end_turn();
        assert!(!g.units[first].holding);
        assert_eq!(g.selected, Some(first));

        // A held unit given a move stops holding, so it stays selected for
        // its attack instead of selection jumping to the next unit.
        g.hold_selected_unit();
        g.selected = Some(first);
        let pos = g.units[first].pos;
        let reachable =
            g.known_reachable_hexes(pos, g.units[first].stats().move_range, Team::Blue, &g.fog());
        let dest = reachable
            .into_iter()
            .filter(|&h| h != pos && !g.is_occupied(h))
            .min_by_key(|h| (h.q, h.r))
            .expect("somewhere to move");
        g.queue_order_at(first, dest);
        g.advance_selection_if_done();
        assert!(!g.units[first].holding);
        assert_eq!(g.units[first].planned_move, Some(dest));
        assert_eq!(g.selected, Some(first), "still needs its attack");
        let target = dest
            .neighbors()
            .into_iter()
            .find(|&h| h != pos && g.grid.is_passable(h))
            .unwrap();
        g.try_queue_attack(first, target);
        assert_eq!(g.units[first].planned_attack, Some(target));
    }

    #[test]
    fn disbanding_takes_two_presses_and_moves_selection_on() {
        let mut g = GameState::new();
        let first = g
            .selected
            .expect("the combat map starts with a unit selected");
        let id = g.units[first].id;
        let count = g.units.len();

        g.disband_selected();
        assert_eq!(g.units.len(), count, "the first press only asks");
        assert_eq!(g.disband_armed, Some(id));

        // Arming one unit doesn't carry over to another.
        let other = (0..g.units.len())
            .find(|&i| i != first && g.is_player_controlled(i))
            .unwrap();
        g.selected = Some(other);
        g.disband_selected();
        assert_eq!(g.units.len(), count);
        assert_eq!(g.disband_armed, Some(g.units[other].id));

        g.selected = Some(first);
        g.disband_selected();
        g.disband_selected();
        assert_eq!(g.units.len(), count - 1);
        assert!(g.units.iter().all(|u| u.id != id));
        assert_eq!(g.disband_armed, None);
        let next = g.selected.expect("selection moves to the next unit");
        assert!(g.is_player_controlled(next));
    }

    #[test]
    fn an_enemy_cannot_be_disbanded() {
        let mut g = GameState::new();
        let enemy = (0..g.units.len())
            .find(|&i| !g.is_player_controlled(i))
            .unwrap();
        let count = g.units.len();
        g.selected = Some(enemy);
        g.disband_selected();
        g.disband_selected();
        assert_eq!(g.units.len(), count);
    }

    #[test]
    fn tab_closes_the_barracks_view_and_its_placement_modes() {
        let mut g = GameState::city_scenario();
        g.cities[0].barracks = Some(Hex::new(-1, 0));
        g.open_barracks(0);
        g.moving_manager = Some((0, 0));
        g.placing_job = Some(JobKind::Road);
        g.select_next_unit();
        assert_eq!(g.selected_barracks, None);
        assert_eq!(g.selected_city, None);
        assert_eq!(g.moving_manager, None);
        assert_eq!(g.placing_job, None);
        assert!(g.selected.is_some(), "Tab still moves on to a unit");
    }

    #[test]
    fn tab_from_the_city_view_stops_placing_a_building() {
        let mut g = GameState::city_scenario();
        g.open_city(0);
        g.queue_selected_city_building(Building::Barracks);
        assert_eq!(g.placing_job, Some(JobKind::Build(Building::Barracks)));
        g.select_next_unit();
        assert_eq!(g.selected_city, None);
        assert_eq!(g.placing_job, None);
    }

    /// Two Blue melee side by side on an open field, and a Red one off to
    /// the east; no cities, fog off.
    fn alert_field(first: UnitType) -> GameState {
        let mut g = GameState::city_scenario();
        g.fog_of_war = false;
        g.units.clear();
        g.cities.clear();
        g.field_workers.clear();
        g.group.clear();
        for (id, pos, team, unit_type) in [
            (100, Hex::new(-4, 0), Team::Blue, first),
            (101, Hex::new(-4, 1), Team::Blue, UnitType::Melee),
            (102, Hex::new(-1, 0), Team::Red, UnitType::Melee),
        ] {
            g.units.push(Unit::new(id, pos, team, unit_type));
        }
        g.selected = Some(0);
        g
    }

    #[test]
    fn alert_drops_this_turns_orders_and_lasts_until_taken_off() {
        let mut g = alert_field(UnitType::Melee);
        g.try_queue_move(0, Hex::new(-3, 0));
        assert!(g.units[0].planned_move.is_some());
        g.toggle_alert();
        let unit = &g.units[0];
        assert!(unit.alert && unit.planned_move.is_none(), "it stays put");
        assert!(!g.needs_orders(0), "skipped in the turn order");
        assert_ne!(g.selected, Some(0), "selection moves on");
        // Hold keeps it (Space on a unit on alert shouldn't end it).
        g.selected = Some(0);
        g.hold_selected_unit();
        assert!(g.units[0].alert);
        // E again takes it off, and it stays selected, needing orders.
        g.selected = Some(0);
        g.units[0].holding = false;
        g.toggle_alert();
        assert!(!g.units[0].alert);
        assert_eq!(g.selected, Some(0));
        assert!(g.needs_orders(0));
    }

    #[test]
    fn any_other_order_ends_an_alert() {
        type Order = fn(&mut GameState);
        let orders: [(&str, Order); 8] = [
            ("move", |g| g.try_queue_move(0, Hex::new(-3, 0))),
            ("attack", |g| g.try_queue_attack(0, Hex::new(-3, 0))),
            ("swap", |g| g.try_queue_swap(0, 1)),
            ("queued move", |g| {
                assert!(g.queue_move(Hex::new(-2, 0)));
            }),
            ("guard", |g| g.toggle_guard()),
            ("ability", |g| g.toggle_selected_ability()),
            ("ctrl-right-click", |g| g.handle_right_click()),
            ("group move", |g| {
                g.set_selection(vec![0, 1]);
                g.group_order(Hex::new(-2, 0), ClickMode::Normal);
            }),
        ];
        for (name, order) in orders {
            let mut g = alert_field(UnitType::Melee);
            g.toggle_alert();
            assert!(g.units[0].alert, "{name}");
            g.selected = Some(0);
            order(&mut g);
            assert!(!g.units[0].alert, "{name} ends the alert");
        }
        // Guard and Alert: going on alert ends a guard too.
        let mut g = alert_field(UnitType::Melee);
        g.toggle_guard();
        g.selected = Some(0);
        g.toggle_alert();
        assert!(g.units[0].alert && !g.units[0].guarding);
    }

    #[test]
    fn only_troops_that_fight_on_land_go_on_alert() {
        for unit_type in [UnitType::Melee, UnitType::Ranged, UnitType::Cavalry] {
            let mut g = alert_field(unit_type);
            g.toggle_alert();
            assert!(g.units[0].alert, "{unit_type:?}");
        }
        for unit_type in [UnitType::Scout, UnitType::Siege] {
            let mut g = alert_field(unit_type);
            g.toggle_alert();
            assert!(!g.units[0].alert, "{unit_type:?}");
            assert_eq!(g.notice, ALERT_NOTICE);
        }
        // A settler (a melee body) can't.
        let mut g = alert_field(UnitType::Melee);
        g.settlers.insert(100);
        g.toggle_alert();
        assert!(!g.units[0].alert);
        // Siege once set up, or setting up this turn, can.
        let mut g = alert_field(UnitType::Siege);
        g.toggle_selected_ability();
        g.selected = Some(0);
        g.toggle_alert();
        assert!(g.units[0].alert && g.units[0].ability_queued);
        let mut g = alert_field(UnitType::Siege);
        g.units[0].deployed = true;
        g.toggle_alert();
        assert!(g.units[0].alert);
        // An enemy selected by hand isn't put on alert.
        let mut g = alert_field(UnitType::Melee);
        g.selected = Some(2);
        g.toggle_alert();
        assert!(!g.units[2].alert);
    }

    #[test]
    fn a_group_goes_on_alert_together_leaving_out_those_that_cannot() {
        let mut g = alert_field(UnitType::Scout);
        g.set_selection(vec![0, 1]);
        g.toggle_alert();
        assert!(!g.units[0].alert, "a scout can't");
        assert!(g.units[1].alert);
        assert!(g.group.is_empty(), "selection moves on");
        g.set_selection(vec![0, 1]);
        g.toggle_alert();
        assert!(!g.units[1].alert, "all that could were: they come off it");
    }

    /// A Blue landing craft offshore at (-1, 0), and Blue melee on the two
    /// shore hexes beside it, (-2, 0) and (-2, 1); no cities, fog off, the
    /// first troop selected.
    fn shore_field() -> GameState {
        let mut g = GameState::naval_scenario();
        g.fog_of_war = false;
        g.units.clear();
        g.cities.clear();
        g.field_workers.clear();
        g.group.clear();
        for (id, pos, unit_type) in [
            (90, Hex::new(-1, 0), UnitType::LandingCraft),
            (91, Hex::new(-2, 0), UnitType::Melee),
            (92, Hex::new(-2, 1), UnitType::Melee),
        ] {
            g.units.push(Unit::new(id, pos, Team::Blue, unit_type));
        }
        g.selected = Some(1);
        g
    }

    #[test]
    fn boarding_replaces_the_troops_other_orders() {
        let mut g = shore_field();
        g.try_queue_swap(1, 2);
        g.units[1].guarding = true;
        g.try_board(1, 0);
        assert_eq!(g.units[1].planned_board, Some(90));
        assert_eq!(g.units[1].planned_move, None);
        assert_eq!(
            g.units[2].planned_move, None,
            "the partner's half of the swap goes too"
        );
        assert!(!g.units[1].guarding, "boarding ends a guard");
        assert!(!g.needs_orders(1), "a troop boarding is done");

        let mut g = shore_field();
        g.go_on_alert(1);
        g.try_board(1, 0);
        assert!(!g.units[1].alert, "boarding ends an alert");
        assert_eq!(g.units[1].planned_board, Some(90));
    }

    #[test]
    fn any_other_order_replaces_boarding() {
        type Give = fn(&mut GameState);
        let orders: [(&str, Give); 6] = [
            ("a move", |g| g.try_queue_move(1, Hex::new(-3, 0))),
            ("an attack", |g| g.try_queue_attack(1, Hex::new(-3, 1))),
            ("a swap", |g| g.try_queue_swap(1, 2)),
            ("a queued move", |g| {
                g.queue_or_unqueue_move(Hex::new(-4, 0));
            }),
            ("a group move", |g| {
                g.set_selection(vec![1, 2]);
                g.group_order(Hex::new(-4, 0), ClickMode::Normal);
            }),
            ("alert", GameState::toggle_alert),
        ];
        for (order, give) in orders {
            let mut g = shore_field();
            g.try_board(1, 0);
            give(&mut g);
            assert_eq!(g.units[1].planned_board, None, "{order}");
            assert!(g.units[1].has_orders(), "{order} was given");
        }
    }

    #[test]
    fn landing_keeps_the_craft_in_place_until_another_order() {
        let mut g = shore_field();
        let passenger = g.units.remove(1);
        g.units[0].cargo.push(passenger);
        g.selected = Some(0);
        let water = Hex::new(0, 0);
        g.try_queue_move(0, water);
        assert_eq!(g.units[0].planned_move, Some(water));
        g.units[0].guarding = true;

        g.try_land(0, Hex::new(-2, 0));
        assert_eq!(g.units[0].planned_unload, Some(Hex::new(-2, 0)));
        assert_eq!(g.units[0].planned_move, None, "it lands from where it is");
        assert!(!g.units[0].guarding, "landing ends a guard");
        assert!(!g.needs_orders(0), "a craft landing a passenger is done");

        g.try_queue_move(0, water);
        assert_eq!(g.units[0].planned_move, Some(water));
        assert_eq!(g.units[0].planned_unload, None, "a move calls it off");

        // An occupied hex, or an empty craft, can't be landed on.
        g.units[0].planned_move = None;
        g.try_land(0, Hex::new(-2, 1));
        assert_eq!(g.units[0].planned_unload, None);
        g.units[0].cargo.clear();
        g.try_land(0, Hex::new(-2, 0));
        assert_eq!(g.units[0].planned_unload, None);
    }

    #[test]
    fn disbanding_a_loaded_craft_warns_of_its_passengers_and_takes_them() {
        let mut g = shore_field();
        let passenger = g.units.remove(1);
        g.units[0].cargo.push(passenger);
        g.settlers.insert(91);
        g.selected = Some(0);
        g.disband_selected();
        assert_eq!(
            g.notice,
            "PRESS DISBAND (OR DELETE) AGAIN TO REMOVE THIS CRAFT AND ITS PASSENGER"
        );
        assert_eq!(g.units.len(), 2, "the first press only asks");
        g.disband_selected();
        assert!(g.units.iter().all(|u| u.id != 90));
        assert!(!g.settlers.contains(&91), "the passenger goes with it");
    }

    /// Ends the turn and plays it out to the next turn's planning.
    fn play_turn(g: &mut GameState) {
        g.end_planning();
        assert!(g.is_resolving(), "{}", g.notice);
        while g.is_resolving() {
            g.update(10.0);
        }
    }

    #[test]
    fn nothing_armed_while_planning_outlives_the_turn() {
        // #161: a Disband armed turns earlier removed the unit in one press.
        let mut g = alert_field(UnitType::Melee);
        let id = g.units[0].id;
        g.disband_selected();
        assert_eq!(g.disband_armed, Some(id));
        g.selected = Some(0);
        g.choose_move_action();
        assert_eq!(g.ui_click_mode, Some(ClickMode::Move));
        play_turn(&mut g);
        assert_eq!(g.disband_armed, None);
        assert_eq!(g.ui_click_mode, None);
        assert_eq!(g.queue_replace_armed, None);
        let idx = g.units.iter().position(|u| u.id == id).unwrap();
        g.selected = Some(idx);
        g.disband_selected();
        assert!(
            g.units.iter().any(|u| u.id == id),
            "one press only asks again"
        );
    }

    #[test]
    fn with_nobody_left_to_order_an_armed_action_is_let_go() {
        let mut g = alert_field(UnitType::Melee);
        g.units[1].holding = true;
        g.choose_attack_action();
        assert_eq!(g.ui_click_mode, Some(ClickMode::Attack));
        g.hold_selected_unit();
        assert_eq!(g.selected, None, "nobody else needs orders");
        assert_eq!(g.ui_click_mode, None, "nothing is left armed");
        assert!(!g.is_resolving(), "the turn waits to be ended");
    }
}
