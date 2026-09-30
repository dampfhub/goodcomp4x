//! The turn strip ("need orders"): a row of chips for everything the player
//! still has to see to this turn, civilian tasks first, then military ones:
//! cities with nothing to build, cities and Barracks idle because the first
//! item of their queue waits (flagged red: `idle_queues`), settlers, and
//! then the military units.
//! Units needing orders are grouped by kind, one chip per kind with a count.
//! Clicking a chip opens its city, or selects its units, and moves the
//! camera there; Shift-click adds a
//! group's units to the selection, Ctrl-click takes them out. A group of
//! several that's selected opens a second row with each of its units, to
//! pick from or take out one at a time. Selected units and the open city are
//! framed. Both presentations draw it from `roster_panel`, so the chips, their
//! actions and their tooltips (`roster_tooltip`) are the same in each;
//! research joins it once there is any
//! (a `RosterKey` variant and its place in `roster_tasks`).

use super::builder::PanelBuilder;
use super::dock::Zone;
use super::queue::{idle_word, queue_place, wait_text};
use super::{
    ChipIcon, LABEL_TEXT, Layout, Line, ROSTER_CHIP_GAP, ROSTER_PER_ROW, RosterChip, SMALL,
    WAITING_TEXT,
};
use crate::game::GameState;
use crate::game::city::{HeadWait, Lane};
use crate::game::unit::UnitType;

/// What a chip in the turn strip stands for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::game) enum RosterKey {
    /// A city with nothing to build, by its id.
    Production(u32),
    /// A city's queue or its Barracks' that works nothing this turn, its
    /// first item waiting (`idle_queues`): by the city's id.
    Waiting(u32, Lane),
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
    /// build, idle queues, then the unit groups, settlers first,
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
        for wait in self.idle_queues() {
            tasks.push(RosterTask {
                key: RosterKey::Waiting(self.cities[wait.city].id, wait.lane),
                units: Vec::new(),
            });
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
                color: self.local_team.color(),
                selected: open_city(id),
                count: 1,
                warning: false,
            },
            RosterKey::Waiting(id, lane) => RosterChip {
                key: task.key,
                icon: match lane {
                    Lane::City => ChipIcon::City,
                    Lane::Barracks => ChipIcon::Barracks,
                },
                color: self.local_team.color(),
                selected: match lane {
                    Lane::City => open_city(id),
                    Lane::Barracks => self
                        .selected_barracks
                        .is_some_and(|i| self.cities[i].id == id),
                },
                count: 1,
                warning: true,
            },
            RosterKey::Group(..) | RosterKey::Unit(_) => RosterChip {
                key: task.key,
                icon: self.group_icon(&task.units),
                color: self.local_team.color(),
                selected: task.units.iter().any(|i| selection.contains(i)),
                count: task.units.len(),
                warning: false,
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
        let mut panel = PanelBuilder::default();
        panel.text(SMALL, roster_heading(&tasks));
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
            RosterKey::Production(_) | RosterKey::Waiting(..) => Vec::new(),
        }
    }

    /// A click on a chip: opens its city, or selects its units (a group's all
    /// at once, opening its row) and moves the camera to the first.
    pub(super) fn roster_select(&mut self, key: RosterKey) {
        if self.is_resolving() {
            return;
        }
        match key {
            RosterKey::Production(id) | RosterKey::Waiting(id, Lane::City) => {
                if let Some(city) = self.city_index(id) {
                    self.open_city(city);
                }
                return;
            }
            RosterKey::Waiting(id, Lane::Barracks) => {
                if let Some(city) = self.city_index(id) {
                    self.open_barracks(city);
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

    /// What a chip's tooltip says, in both presentations
    /// (`subject_tooltip_lines`): its title, the clicks it takes, and what
    /// they do.
    pub(super) fn roster_tooltip(&self, key: RosterKey) -> (String, &'static str, String) {
        const CLICKS: &str = "CLICK · SHIFT · CTRL";
        match key {
            RosterKey::Production(id) => (
                format!("CITY {}", id + 1),
                "CLICK",
                "HAS NOTHING TO BUILD. CLICK: OPEN IT.".into(),
            ),
            RosterKey::Waiting(id, lane) => {
                let wait = self.idle_queue(id, lane);
                (
                    format!("{} {}", queue_place(id, lane), idle_word(lane)),
                    "CLICK",
                    match wait {
                        Some(wait) => format!(
                            "WORKS NOTHING THIS TURN: {}. CLICK: OPEN IT.",
                            wait_text(&wait)
                        ),
                        None => "CLICK: OPEN IT.".into(),
                    },
                )
            }
            RosterKey::Group(..) => {
                let units = self.roster_key_units(key);
                let role = units
                    .first()
                    .map_or("", |&i| self.unit_role(&self.units[i]));
                (
                    format!("{role} x{}", units.len()),
                    CLICKS,
                    "NEED ORDERS. CLICK: SELECT THEM ALL. SHIFT-CLICK: ADD THEM TO THE \
                     SELECTION. CTRL-CLICK: DESELECT THEM."
                        .into(),
                )
            }
            RosterKey::Unit(id) => {
                let role = self
                    .units
                    .iter()
                    .find(|u| u.id == id)
                    .map_or("", |u| self.unit_role(u));
                (
                    role.into(),
                    CLICKS,
                    "CLICK: SELECT IT. SHIFT-CLICK: ADD IT TO THE SELECTION. CTRL-CLICK: \
                     DESELECT IT."
                        .into(),
                )
            }
        }
    }
}

impl GameState {
    /// The idle queue a waiting chip stands for, as it is now.
    fn idle_queue(&self, id: u32, lane: Lane) -> Option<HeadWait> {
        self.idle_queues()
            .into_iter()
            .find(|wait| wait.lane == lane && self.cities[wait.city].id == id)
    }
}

/// The turn strip's heading: how many chips need orders (a group counts
/// its units), and how many queues wait idle, in the waiting red.
fn roster_heading(tasks: &[RosterTask]) -> Line {
    let (waiting, orders): (Vec<&RosterTask>, Vec<&RosterTask>) = tasks
        .iter()
        .partition(|t| matches!(t.key, RosterKey::Waiting(..)));
    let orders: usize = orders.iter().map(|t| t.units.len().max(1)).sum();
    let mut line = Vec::new();
    if orders > 0 {
        line.push((format!("NEED ORDERS: {orders}"), LABEL_TEXT));
    }
    if !waiting.is_empty() {
        let gap = if line.is_empty() { "" } else { " · " };
        line.push((format!("{gap}WAITING: {}", waiting.len()), WAITING_TEXT));
    }
    line
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
