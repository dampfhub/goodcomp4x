//! City and Barracks views: opening and leaving them, map clicks while one is
//! open, and ending planning (which waits on cities with nothing to build).
use super::MAX_CITY_POPULATION;
use crate::game::GameState;
use crate::game::hex::Hex;
use glam::Vec2;

impl GameState {
    /// Y or the Yields button: shows or hides tile yields around the open city.
    pub fn toggle_yields(&mut self) {
        self.show_yields = !self.show_yields;
    }

    /// The open city, if its tile yields are being shown.
    pub(in crate::game) fn yields_city(&self) -> Option<usize> {
        self.selected_city.filter(|_| self.show_yields)
    }

    /// The city whose delivery shares (the percentages) the map shows:
    /// `yields_city`, or with yields off, the open city's while something
    /// is being placed for its workers and Alt is held.
    pub(in crate::game) fn shares_city(&self) -> Option<usize> {
        self.yields_city().or(self
            .selected_city
            .filter(|_| self.placing_job.is_some() && self.show_details))
    }

    /// C: opens a city that needs something to build, or else the first of
    /// the player's cities.
    pub fn select_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        let city = (0..self.cities.len())
            .find(|&i| self.city_needs_build(i))
            .or_else(|| self.cities.iter().position(|c| c.team == self.local_team));
        if let Some(i) = city {
            self.open_city(i);
        }
    }

    /// Opens city `i`'s view and glides the camera to it.
    pub(in crate::game) fn open_city(&mut self, i: usize) {
        self.close_city_interior();
        if self.selected_city != Some(i) {
            self.city_queue_scroll = 0;
            // Placing belongs to the open city.
            self.placing_job = None;
            self.hovered_job = None;
        }
        self.selected_city = Some(i);
        self.selected_barracks = None;
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera.focus_on(self.cities[i].pos.to_world());
        self.notice = if self.city_needs_build(i) {
            format!("CHOOSE WHAT CITY {} BUILDS - 0-9", self.cities[i].id + 1)
        } else {
            "CLICK TILES TO ASSIGN - A AUTO ASSIGN - ESC OR SPACE TO EXIT".into()
        };
    }

    /// One of the player's cities with nothing queued to build. The turn
    /// waits for these, as it does for units without orders.
    /// A player city with an empty queue holds up the turn: it can always
    /// gather (`Build::Gather`), even when its side can't pay for anything.
    pub(in crate::game) fn city_needs_build(&self, i: usize) -> bool {
        let city = &self.cities[i];
        city.team == self.local_team && city.queue.is_empty()
    }

    pub(in crate::game) fn leave_city_view(&mut self) {
        self.close_city_interior();
        self.queue_drag = None;
        self.selected_city = None;
        self.selected_barracks = None;
        self.interior_view = None;
        self.interior_selected = None;
        self.moving_manager = None;
        self.placing_job = None;
        self.hovered_job = None;
    }

    /// Escape and Space dismiss city or building management without issuing a
    /// unit order or opening the settings menu. While something is being
    /// placed for the city's workers, the first press only stops that and
    /// keeps the city open (`stop_placing`).
    pub fn exit_structure_menu(&mut self) -> bool {
        if self.interior_view.is_some() {
            self.close_city_interior();
            return true;
        }
        if self.selected_city.is_none()
            && self.selected_barracks.is_none()
            && self.interior_view.is_none()
        {
            return false;
        }
        if self.stop_placing() {
            return true;
        }
        self.leave_city_view();
        self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
        true
    }

    /// The city tray's Cancel Placing button, and the first Escape while
    /// placing: stops placing what the open city was placing for its
    /// workers, with nothing placed or paid. Returns whether anything was
    /// being placed.
    pub(in crate::game) fn stop_placing(&mut self) -> bool {
        let Some(kind) = self.placing_job.take() else {
            return false;
        };
        self.hovered_job = None;
        self.notice = format!("STOPPED PLACING {}", kind.name());
        true
    }

    pub(in crate::game) fn open_barracks(&mut self, city: usize) {
        self.close_city_interior();
        if self.cities[city].barracks.is_none() {
            return;
        }
        if self.selected_barracks != Some(city) {
            self.barracks_queue_scroll = 0;
        }
        self.selected_city = None;
        self.placing_job = None;
        self.hovered_job = None;
        self.selected_barracks = Some(city);
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera
            .focus_on(self.cities[city].barracks.unwrap().to_world());
        self.notice = "BARRACKS - QUEUE TROOPS OR CLICK CITY TO RETURN".into();
    }

    pub fn open_selected_city_from_barracks(&mut self) {
        if let Some(city) = self.selected_barracks {
            self.open_city(city);
        }
    }

    /// A map click at `point` (world space) on `hex`. With a city or
    /// Barracks view open, a click on one of the player's unit tokens (as
    /// drawn: `unit_token_contains`) selects that unit and closes the view;
    /// anywhere else on the hex it's the view's click (`city_click`), so a
    /// citizen can still be put to work on a tile a unit stands on. Moving
    /// the manager and placing worker jobs keep every click.
    pub(in crate::game) fn city_click_at(&mut self, hex: Hex, point: Vec2) -> bool {
        let view_open = self.selected_city.is_some() || self.selected_barracks.is_some();
        if view_open
            && self.interior_view.is_none()
            && self.moving_manager.is_none()
            && self.placing_job.is_none()
            && let Some(unit) = self.unit_token_at(hex, point)
        {
            // Local view state only: nothing a network game's plan carries.
            self.set_selection(vec![unit]);
            self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
            return true;
        }
        self.city_click(hex)
    }

    /// One of the player's units on `hex` whose token, as drawn, is under
    /// `point`.
    fn unit_token_at(&self, hex: Hex, point: Vec2) -> Option<usize> {
        let fog = self.fog();
        self.units_at(hex).find(|&i| {
            self.is_player_controlled(i)
                && fog.shows(&self.units[i])
                && self.unit_token_contains(i, point)
        })
    }

    /// A click on `hex` while a view may be open, off any unit's token.
    pub(in crate::game) fn city_click(&mut self, hex: Hex) -> bool {
        if self.interior_view.is_some() {
            self.interior_click(hex);
            return true;
        }
        // A structure menu owns map clicks off a unit's token
        // (`city_click_at`) until its explicit exit action.
        if self.selected_barracks.is_some() {
            self.notice = "BARRACKS MENU - PRESS ESC OR SPACE TO EXIT".into();
            return true;
        }
        if self.selected_city.is_none() {
            if let Some(i) = self
                .cities
                .iter()
                .position(|c| c.barracks == Some(hex) && c.team == self.local_team)
            {
                self.open_barracks(i);
                return true;
            }
            if let Some(i) = self
                .cities
                .iter()
                .position(|c| c.pos == hex && c.team == self.local_team)
            {
                self.open_city(i);
                return true;
            }
            return false;
        }
        let i = self.selected_city.unwrap();
        if hex == self.cities[i].pos {
            self.open_city_interior(i);
            return true;
        }
        // City management owns map clicks off a unit's token, so citizens
        // may be assigned onto a unit's tile. A tile never seen can't be
        // worked.
        if !self.is_explored(hex) {
            self.notice = "UNEXPLORED - SCOUT IT FIRST".into();
            return true;
        }
        if self.moving_manager == Some(i) {
            if self.cities[i].worked.first() == Some(&hex) {
                self.moving_manager = None;
                self.notice = "MANAGER MOVE CANCELLED".into();
            } else if self.closed_to_citizens(hex) {
                self.notice = "A BUILDING STANDS THERE - NO CITIZEN CAN WORK IT".into();
            } else if self.may_be_manager(i, hex) {
                self.moving_manager = None;
                self.move_manager(i, hex);
            } else {
                self.notice = "MANAGER NEEDS A REACHABLE, UNCLAIMED TILE".into();
            }
            return true;
        }
        if let Some(at) = self.cities[i].worked.iter().position(|h| *h == hex) {
            if at == 0 {
                self.moving_manager = Some(i);
                self.notice = "MANAGER PICKED UP - CLICK A DESTINATION".into();
                return true;
            }
            self.cities[i].worked.remove(at);
            self.cities[i].remembered_worked.retain(|h| *h != hex);
            self.notice = "CITIZEN UNASSIGNED".into();
        } else if self.closed_to_citizens(hex) {
            self.notice = "A BUILDING STANDS THERE - NO CITIZEN CAN WORK IT".into();
        } else if !self.may_assign(i, hex) {
            self.notice = "CLICK A WORKED TILE TO MOVE OR RELEASE A CITIZEN; CLICK AN OPEN ADJACENT TILE TO ASSIGN".into();
        } else if !self.routes(i).costs.contains_key(&hex) {
            self.notice = "NO OPEN ROUTE WITHIN LOGISTICS BUDGET".into();
        } else if !self.may_manage_or_work(i, hex) {
            self.notice = "THE FIRST CITIZEN MANAGES THE OTHERS AND MUST WORK LAND".into();
        } else if self.cities[i].worked.len() >= self.cities[i].population.min(MAX_CITY_POPULATION)
        {
            self.notice = "ALL CITIZENS BUSY - RELEASE A WORKED TILE FIRST".into();
        } else {
            self.cities[i].worked.push(hex);
            self.cities[i].remembered_worked = self.cities[i].worked.clone();
            self.notice = "CITIZEN ASSIGNED".into();
        }
        true
    }

    /// Space (once nothing needs orders) or the End Turn button: resolves the
    /// turn, unless a unit still needs orders or a city needs something to
    /// build, in which case that's selected instead.
    pub fn end_planning(&mut self) {
        if self.is_resolving() {
            return;
        }
        for i in 0..self.units.len() {
            if self.is_player_controlled(i) && self.needs_orders(i) {
                self.units[i].holding = true;
            }
        }
        self.placing_job = None;
        self.hovered_job = None;
        if let Some(i) = (0..self.cities.len()).find(|&i| self.city_needs_build(i)) {
            self.open_city(i);
            return;
        }
        // A networked game resolves once every side's plan is in
        // (`multiplayer.rs`).
        if self.is_networked() {
            self.submit_plan();
            return;
        }
        for i in 0..self.cities.len() {
            if !self.is_human(self.cities[i].team) {
                self.auto_assign_city(i);
            }
        }
        self.selected_city = None;
        self.notice = "RESOLVING ORDERS".into();
        self.resolve_turn();
    }
}
