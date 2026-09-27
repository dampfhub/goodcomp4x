//! City and Barracks views: opening and leaving them, map clicks while one is
//! open, and ending planning (which waits on cities with nothing to build).
use super::{Build, MAX_CITY_POPULATION};
use crate::game::hex::Hex;
use crate::game::{GameState, PLAYER_TEAM};

impl GameState {
    /// Y or the Yields button: shows or hides tile yields around the open city.
    pub fn toggle_yields(&mut self) {
        self.show_yields = !self.show_yields;
    }

    /// The open city, if its tile yields are being shown.
    pub(in crate::game) fn yields_city(&self) -> Option<usize> {
        self.selected_city.filter(|_| self.show_yields)
    }

    /// C: opens a city that needs something to build, or else the first of
    /// the player's cities.
    pub fn select_city(&mut self) {
        if self.is_resolving() {
            return;
        }
        let city = (0..self.cities.len())
            .find(|&i| self.city_needs_build(i))
            .or_else(|| self.cities.iter().position(|c| c.team == PLAYER_TEAM));
        if let Some(i) = city {
            self.open_city(i);
        }
    }

    /// Opens city `i`'s view and glides the camera to it.
    pub(in crate::game) fn open_city(&mut self, i: usize) {
        self.close_city_interior();
        if self.selected_city != Some(i) {
            self.city_queue_scroll = 0;
            // Site placement belongs to the open city. A building that
            // finished while the view was closed, with no site chosen yet,
            // picks its site now.
            self.abandon_site_placement();
            self.placing_building = self.cities[i]
                .pending_building
                .filter(|&building| self.needs_site(i, building))
                .map(|building| (i, building));
        }
        self.selected_city = Some(i);
        self.selected_barracks = None;
        self.selected = None;
        self.group.clear();
        self.ui_click_mode = None;
        self.camera.focus_on(self.cities[i].pos.to_world());
        self.notice = if self.city_needs_build(i) {
            format!("CHOOSE WHAT CITY {} BUILDS - 1-4", self.cities[i].id + 1)
        } else {
            "CLICK TILES TO ASSIGN - A AUTO ASSIGN - ESC OR SPACE TO EXIT".into()
        };
    }

    /// One of the player's cities with nothing queued to build. The turn
    /// waits for these, as it does for units without orders.
    pub(in crate::game) fn city_needs_build(&self, i: usize) -> bool {
        let city = &self.cities[i];
        city.team == PLAYER_TEAM && city.queue.is_empty()
    }

    pub(in crate::game) fn leave_city_view(&mut self) {
        self.close_city_interior();
        self.queue_drag = None;
        self.selected_city = None;
        self.selected_barracks = None;
        self.interior_view = None;
        self.interior_selected = None;
        self.moving_manager = None;
        self.abandon_site_placement();
        self.inspected_tile = None;
    }

    /// Escape and Space dismiss city or building management without issuing a
    /// unit order or starting the quit hold. While a building site is being
    /// chosen, the first press only cancels that (and the unsited building)
    /// and keeps the city open.
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
        if self.site_placement().is_some() {
            self.abandon_site_placement();
            return true;
        }
        self.leave_city_view();
        self.notice = "PLANNING - C CITY - SPACE HOLD OR END TURN".into();
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
        self.abandon_site_placement();
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

    pub(in crate::game) fn city_click(&mut self, hex: Hex) -> bool {
        if self.interior_view.is_some() {
            self.interior_click(hex);
            return true;
        }
        // A structure menu owns all map clicks until its explicit exit action.
        // This keeps city assignment, Barracks management, and unit selection
        // on one consistent interaction model.
        if self.selected_barracks.is_some() {
            self.inspected_tile = Some(hex);
            self.notice = "BARRACKS MENU - PRESS ESC OR SPACE TO EXIT".into();
            return true;
        }
        if self.selected_city.is_none() {
            if let Some(i) = self
                .cities
                .iter()
                .position(|c| c.barracks == Some(hex) && c.team == PLAYER_TEAM)
            {
                self.open_barracks(i);
                return true;
            }
            if let Some(i) = self
                .cities
                .iter()
                .position(|c| c.pos == hex && c.team == PLAYER_TEAM)
            {
                self.open_city(i);
                return true;
            }
            return false;
        }
        let i = self.selected_city.unwrap();
        if let Some((_, building)) = self.site_placement() {
            if !self.site_available(i, building, hex) {
                self.notice = format!("{} NEEDS AN OPEN LAND TILE", building.name());
            } else {
                self.cities[i].planned_sites.insert(building, hex);
                self.placing_building = None;
                if self.cities[i].pending_building == Some(building)
                    && self.cities[i].production
                        < self.city_build_cost(i, Build::Building(building))
                {
                    self.cities[i].pending_building = None;
                }
                self.notice = if self.cities[i].pending_building == Some(building)
                    || (self.cities[i].queue.first() == Some(&Build::Building(building))
                        && self.cities[i].production
                            >= self.city_build_cost(i, Build::Building(building)))
                {
                    format!(
                        "{} SITE SELECTED - CLICK CONFIRM IN THE CITY TRAY",
                        building.name()
                    )
                } else {
                    format!("{} SITE SELECTED - CONSTRUCTION CONTINUES", building.name())
                };
            }
            return true;
        }
        if hex == self.cities[i].pos {
            self.open_city_interior(i);
            return true;
        }
        // City management owns map clicks. Dismiss it with Escape or Space
        // before selecting units, so workers may be assigned onto a unit's
        // tile without the unit stealing the click.
        self.inspected_tile = Some(hex);
        if self.moving_manager == Some(i) {
            if self.cities[i].worked.first() == Some(&hex) {
                self.moving_manager = None;
                self.notice = "MANAGER MOVE CANCELLED".into();
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
        // A building left without a site comes out of its queue first, so a
        // city it leaves with nothing to build is asked for something.
        self.abandon_site_placement();
        for i in 0..self.units.len() {
            if self.is_player_controlled(i) && self.needs_orders(i) {
                self.units[i].holding = true;
            }
        }
        if let Some(i) = (0..self.cities.len()).find(|&i| self.city_needs_build(i)) {
            self.open_city(i);
            return;
        }
        for i in 0..self.cities.len() {
            if self.cities[i].team != PLAYER_TEAM {
                self.auto_assign_city(i);
            }
        }
        self.selected_city = None;
        self.notice = "RESOLVING ORDERS".into();
        self.resolve_turn();
    }
}
