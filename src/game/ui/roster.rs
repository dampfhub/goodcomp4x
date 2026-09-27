//! The unit strip: a row of tokens at the top-left for the player's military
//! units that still need orders, in the order the game selects them as a
//! turn starts (unit order: after the selected one comes the next to its
//! right, wrapping around). Selected units are framed. Click a token to select
//! that unit and move the camera to it, Shift-click to add it to the
//! selection, Ctrl-click to take it out. Both presentations draw it from
//! `roster_panel`, so the tokens and their actions are the same in each.

use super::builder::PanelBuilder;
use super::dock::Zone;
use super::{LABEL_TEXT, Layout, ROSTER_CHIP_GAP, ROSTER_PER_ROW, RosterChip, SMALL};
use crate::game::GameState;

impl GameState {
    /// The player's military units (not settlers) that still need orders, in
    /// unit order. None while a turn plays out or a city interior is open.
    pub(super) fn roster_units(&self) -> Vec<usize> {
        if self.is_resolving() || self.interior_view.is_some() {
            return Vec::new();
        }
        (0..self.units.len())
            .filter(|&i| {
                self.is_player_controlled(i)
                    && !self.settlers.contains(&self.units[i].id)
                    && self.needs_orders(i)
            })
            .collect()
    }

    /// The unit strip's content, or `None` when no unit needs orders.
    pub(super) fn roster_panel(&self) -> Option<PanelBuilder> {
        let units = self.roster_units();
        if units.is_empty() {
            return None;
        }
        let selection = self.selection();
        let mut panel = PanelBuilder::default();
        panel.text(
            SMALL,
            vec![(format!("NEED ORDERS: {}", units.len()), LABEL_TEXT)],
        );
        for (row, chunk) in units.chunks(ROSTER_PER_ROW).enumerate() {
            if row > 0 {
                panel.gap(ROSTER_CHIP_GAP);
            }
            let chips = chunk
                .iter()
                .map(|&i| {
                    let unit = &self.units[i];
                    RosterChip {
                        id: unit.id,
                        look: self.unit_look(unit),
                        color: unit.team.color(),
                        selected: selection.contains(&i),
                    }
                })
                .collect();
            panel.roster(chips);
        }
        Some(panel)
    }

    /// Docks the unit strip in the classic layout's top-left corner.
    pub(super) fn dock_roster(&self, layout: &mut Layout) {
        if let Some(panel) = self.roster_panel() {
            layout.dock_panel(panel, Zone::TopLeft);
        }
    }

    /// The index of the player's unit with this id, if it's still there.
    fn roster_index(&self, id: u32) -> Option<usize> {
        self.units
            .iter()
            .position(|u| u.id == id)
            .filter(|&i| self.is_player_controlled(i))
    }

    /// A click on a token: selects just that unit and moves the camera to it.
    pub(super) fn roster_select(&mut self, id: u32) {
        let Some(idx) = self.roster_index(id).filter(|_| !self.is_resolving()) else {
            return;
        };
        self.set_selection(vec![idx]);
        self.camera.focus_on(self.units[idx].pos.to_world());
    }

    /// Shift-click on a token: adds that unit to the selection.
    pub(super) fn roster_add(&mut self, id: u32) {
        if let Some(idx) = self.roster_index(id).filter(|_| !self.is_resolving()) {
            self.add_to_selection(idx);
        }
    }

    /// Ctrl-click on a token: takes that unit out of a selection of several.
    pub(super) fn roster_remove(&mut self, id: u32) {
        if let Some(idx) = self.roster_index(id).filter(|_| !self.is_resolving())
            && self.selection().len() > 1
        {
            self.remove_from_selection(idx);
        }
    }
}
