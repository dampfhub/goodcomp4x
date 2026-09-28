//! The turn strip ("need orders"): a row of chips for everything the player
//! still has to see to this turn, civilian tasks first, then military ones:
//! cities with nothing to build, cities with idle workers (opening the
//! worker menu), settlers, and then the military units.
//! Units needing orders are grouped by kind, one chip per kind with a count.
//! Clicking a chip opens its city, or selects its units, and moves the
//! camera there; Shift-click adds a
//! group's units to the selection, Ctrl-click takes them out. A group of
//! several that's selected opens a second row with each of its units, to
//! pick from or take out one at a time. Selected units and the open city are
//! framed. Both presentations draw it from `roster_panel`, so the chips and
//! their actions are the same in each; research joins it once there is any
//! (a `RosterKey` variant and its place in `roster_tasks`).

use super::builder::PanelBuilder;
use super::dock::Zone;
use super::{ChipIcon, LABEL_TEXT, Layout, ROSTER_CHIP_GAP, ROSTER_PER_ROW, RosterChip, SMALL};
use crate::game::draw::UnitLook;
use crate::game::unit::UnitType;
use crate::game::unit_icons::UnitIcon;
use crate::game::{GameState, PLAYER_TEAM};

/// What a chip in the turn strip stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::game) enum RosterKey {
    /// A city with nothing to build, by its id.
    Production(u32),
    /// A city with idle workers (`idle_workers`), by its id.
    Workers(u32),
    /// The units of one kind that need orders: a unit type, and whether
    /// they're settlers (which use the melee body).
    Group(UnitType, bool),
    /// One unit, by its id, in an open group's row.
    Unit(u32),
}

/// One chip's worth of the turn strip: what it stands for, and the player's
/// units in it (a group's, in unit order; none for a city).
pub(super) struct RosterTask {
    pub(super) key: RosterKey,
    pub(super) units: Vec<usize>,
}

impl GameState {
    /// The player's units (settlers too) that still need orders, in unit
    /// order. None while a turn plays out or a city interior is open.
    pub(super) fn roster_units(&self) -> Vec<usize> {
        if self.is_resolving() || self.interior_view.is_some() {
            return Vec::new();
        }
        (0..self.units.len())
            .filter(|&i| self.is_player_controlled(i) && self.needs_orders(i))
            .collect()
    }

    /// The city of `id`, by index.
    fn city_index(&self, id: u32) -> Option<usize> {
        self.cities.iter().position(|c| c.id == id)
    }

    /// Everything the turn strip lists, in its order: cities with nothing to
    /// build, then the unit groups, settlers first,
    /// each group where its first unit comes in unit order. Nothing while a
    /// turn plays out or a city interior is open.
    pub(super) fn roster_tasks(&self) -> Vec<RosterTask> {
        if self.is_resolving() || self.interior_view.is_some() {
            return Vec::new();
        }
        let mut tasks: Vec<RosterTask> = Vec::new();
        for (i, city) in self.cities.iter().enumerate() {
            if self.city_needs_build(i) {
                tasks.push(RosterTask {
                    key: RosterKey::Production(city.id),
                    units: Vec::new(),
                });
            }
        }
        for (i, city) in self.cities.iter().enumerate() {
            if self.idle_workers(i) > 0 {
                tasks.push(RosterTask {
                    key: RosterKey::Workers(city.id),
                    units: Vec::new(),
                });
            }
        }
        let mut groups: Vec<RosterTask> = Vec::new();
        for i in self.roster_units() {
            let unit = &self.units[i];
            let key = RosterKey::Group(unit.unit_type, self.settlers.contains(&unit.id));
            match groups.iter_mut().find(|g| g.key == key) {
                Some(group) => group.units.push(i),
                None => groups.push(RosterTask {
                    key,
                    units: vec![i],
                }),
            }
        }
        // Settlers are civilian: they go ahead of the military groups.
        groups.sort_by_key(|g| !matches!(g.key, RosterKey::Group(_, true)));
        tasks.extend(groups);
        tasks
    }

    /// The open group's units, when it has several and one of them is
    /// selected: the turn strip's second row.
    fn open_roster_group(&self, tasks: &[RosterTask]) -> Option<(RosterKey, Vec<usize>)> {
        let key = self.roster_open?;
        let group = tasks.iter().find(|t| t.key == key)?;
        let selection = self.selection();
        (group.units.len() > 1 && group.units.iter().any(|i| selection.contains(i)))
            .then(|| (key, group.units.clone()))
    }

    /// How a group's (or unit's) chip looks: its units' token.
    fn group_icon(&self, units: &[usize]) -> ChipIcon {
        ChipIcon::Unit(self.unit_look(&self.units[units[0]]))
    }

    fn roster_chip(&self, task: &RosterTask, selection: &[usize]) -> RosterChip {
        let open_city = |id: u32| self.selected_city.is_some_and(|i| self.cities[i].id == id);
        match task.key {
            RosterKey::Production(id) => RosterChip {
                key: task.key,
                icon: ChipIcon::City,
                color: PLAYER_TEAM.color(),
                selected: open_city(id),
                count: 1,
            },
            RosterKey::Workers(id) => RosterChip {
                key: task.key,
                icon: ChipIcon::Unit(UnitLook {
                    icon: UnitIcon::Shovel,
                    civilian: true,
                }),
                color: PLAYER_TEAM.color(),
                selected: self.worker_mode
                    && self
                        .worker_menu_city
                        .is_some_and(|i| self.cities[i].id == id),
                count: self
                    .city_index(id)
                    .map_or(1, |i| self.idle_workers(i) as usize),
            },
            RosterKey::Group(..) | RosterKey::Unit(_) => RosterChip {
                key: task.key,
                icon: self.group_icon(&task.units),
                color: PLAYER_TEAM.color(),
                selected: task.units.iter().any(|i| selection.contains(i)),
                count: task.units.len(),
            },
        }
    }

    /// The turn strip's content, or `None` when nothing needs seeing to.
    pub(super) fn roster_panel(&self) -> Option<PanelBuilder> {
        let tasks = self.roster_tasks();
        if tasks.is_empty() {
            return None;
        }
        let selection = self.selection();
        let waiting: usize = tasks.iter().map(|t| t.units.len().max(1)).sum();
        let mut panel = PanelBuilder::default();
        panel.text(SMALL, vec![(format!("NEED ORDERS: {waiting}"), LABEL_TEXT)]);
        let chips: Vec<RosterChip> = tasks
            .iter()
            .map(|task| self.roster_chip(task, &selection))
            .collect();
        push_chip_rows(&mut panel, chips);
        if let Some((key, units)) = self.open_roster_group(&tasks) {
            let role = match key {
                RosterKey::Group(..) => self.unit_role(&self.units[units[0]]),
                _ => "",
            };
            panel.gap(ROSTER_CHIP_GAP);
            panel.text(
                SMALL,
                vec![(format!("{role} x{}", units.len()), LABEL_TEXT)],
            );
            let chips = units
                .iter()
                .map(|&i| {
                    let task = RosterTask {
                        key: RosterKey::Unit(self.units[i].id),
                        units: vec![i],
                    };
                    self.roster_chip(&task, &selection)
                })
                .collect();
            push_chip_rows(&mut panel, chips);
        }
        Some(panel)
    }

    /// Docks the turn strip at the bottom of the classic layout, centered.
    pub(super) fn dock_roster(&self, layout: &mut Layout) {
        if let Some(panel) = self.roster_panel() {
            layout.dock_panel(panel, Zone::BottomCenter);
        }
    }

    /// The indices of the player's units a chip stands for, as they are now.
    fn roster_key_units(&self, key: RosterKey) -> Vec<usize> {
        match key {
            RosterKey::Unit(id) => self
                .units
                .iter()
                .position(|u| u.id == id)
                .filter(|&i| self.is_player_controlled(i))
                .into_iter()
                .collect(),
            RosterKey::Group(..) => self
                .roster_tasks()
                .into_iter()
                .find(|t| t.key == key)
                .map_or_else(Vec::new, |t| t.units),
            RosterKey::Production(_) | RosterKey::Workers(_) => Vec::new(),
        }
    }

    /// A click on a chip: opens its city, or selects its units (a group's all
    /// at once, opening its row) and moves the camera to the first.
    pub(super) fn roster_select(&mut self, key: RosterKey) {
        if self.is_resolving() {
            return;
        }
        match key {
            RosterKey::Production(id) => {
                if let Some(city) = self.city_index(id) {
                    self.open_city(city);
                }
                return;
            }
            RosterKey::Workers(id) => {
                if let Some(city) = self.city_index(id) {
                    self.open_worker_menu(city);
                }
                return;
            }
            RosterKey::Group(..) | RosterKey::Unit(_) => {}
        }
        let units = self.roster_key_units(key);
        let Some(&first) = units.first() else { return };
        self.leave_city_view();
        if matches!(key, RosterKey::Group(..)) {
            self.roster_open = Some(key);
        }
        self.set_selection(units);
        self.camera.focus_on(self.units[first].pos.to_world());
    }

    /// Shift-click on a chip: adds its units to the selection.
    pub(super) fn roster_add(&mut self, key: RosterKey) {
        if self.is_resolving() {
            return;
        }
        for idx in self.roster_key_units(key) {
            self.add_to_selection(idx);
        }
    }

    /// Ctrl-click on a chip: takes its units out of a selection of several,
    /// leaving at least one selected.
    pub(super) fn roster_remove(&mut self, key: RosterKey) {
        if self.is_resolving() {
            return;
        }
        for idx in self.roster_key_units(key) {
            if self.selection().len() > 1 {
                self.remove_from_selection(idx);
            }
        }
    }

    /// What hovering a chip says, for the ImGui tooltip.
    pub(super) fn roster_hint(&self, key: RosterKey) -> String {
        let city_name = |id: u32| format!("CITY {}", id + 1);
        match key {
            RosterKey::Production(id) => {
                format!("{} HAS NOTHING TO BUILD - CLICK: OPEN IT", city_name(id))
            }
            RosterKey::Workers(id) => format!(
                "IDLE WORKERS IN {} - CLICK: THE WORKER MENU, TO GIVE THEM JOBS OR LET THEM SLEEP",
                city_name(id)
            ),
            RosterKey::Group(..) => {
                let units = self.roster_key_units(key);
                let role = units
                    .first()
                    .map_or("", |&i| self.unit_role(&self.units[i]));
                format!(
                    "{role} x{} - CLICK: SELECT ALL · SHIFT: ADD · CTRL: REMOVE",
                    units.len()
                )
            }
            RosterKey::Unit(id) => {
                let role = self
                    .units
                    .iter()
                    .find(|u| u.id == id)
                    .map_or("", |u| self.unit_role(u));
                format!("{role} - CLICK: SELECT · SHIFT: ADD · CTRL: REMOVE")
            }
        }
    }
}

/// Adds `chips` to `panel`, `ROSTER_PER_ROW` to a row.
fn push_chip_rows(panel: &mut PanelBuilder, chips: Vec<RosterChip>) {
    for (row, chunk) in chips.chunks(ROSTER_PER_ROW).enumerate() {
        if row > 0 {
            panel.gap(ROSTER_CHIP_GAP);
        }
        panel.roster(chunk.to_vec());
    }
}
