//! Captured panels: a city, Barracks, unit, group or queue panel dropped in
//! a box stays there, for that one, after the selection changes.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum PinnedKind {
    City,
    Barracks,
    Unit,
    Group,
    CityQueue,
    BarracksQueue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct PinnedPanel {
    pub(super) kind: PinnedKind,
    pub(super) city_id: u32,
}

impl PinnedPanel {
    pub(super) fn title(self) -> String {
        match self.kind {
            PinnedKind::City => format!(
                "{}###PinnedCity-{}",
                text!("window_city", city = self.city_id + 1),
                self.city_id
            ),
            PinnedKind::Barracks => format!(
                "{}###PinnedBarracks-{}",
                text!("window_barracks", city = self.city_id + 1),
                self.city_id
            ),
            PinnedKind::Unit => format!(
                "{}###PinnedUnit-{}",
                text!("window_unit", unit = self.city_id),
                self.city_id
            ),
            PinnedKind::Group => {
                format!("{}###PinnedGroup-{}", text!("window_group"), self.city_id)
            }
            PinnedKind::CityQueue => format!(
                "{}###PinnedCityQueue-{}",
                text!("window_city_queue", city = self.city_id + 1),
                self.city_id
            ),
            PinnedKind::BarracksQueue => format!(
                "{}###PinnedBarracksQueue-{}",
                text!("window_barracks_queue", city = self.city_id + 1),
                self.city_id
            ),
        }
    }
}

/// Captured group panels' members, by unit id, keyed by the group's pin id.
pub(super) type PinnedGroups = std::collections::HashMap<u32, Vec<u32>>;

/// What a captured panel is for this frame, by index (`pin_focus`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PinFocus {
    /// A unit, or a group's members still alive.
    Units(Vec<usize>),
    City(usize),
    Barracks(usize),
}

impl GameState {
    pub(super) fn selected_pin(&self) -> Option<PinnedPanel> {
        self.selected_city
            .map(|i| PinnedPanel {
                kind: PinnedKind::City,
                city_id: self.cities[i].id,
            })
            .or_else(|| {
                self.selected_barracks.map(|i| PinnedPanel {
                    kind: PinnedKind::Barracks,
                    city_id: self.cities[i].id,
                })
            })
            .or_else(|| {
                self.selected.map(|i| PinnedPanel {
                    kind: PinnedKind::Unit,
                    city_id: self.units[i].id,
                })
            })
            .or_else(|| {
                (!self.group.is_empty()).then(|| {
                    let mut ids: Vec<_> = self.group.iter().map(|&i| self.units[i].id).collect();
                    ids.sort_unstable();
                    let id = ids.into_iter().fold(2_166_136_261_u32, |hash, id| {
                        (hash ^ id).wrapping_mul(16_777_619)
                    });
                    PinnedPanel {
                        kind: PinnedKind::Group,
                        city_id: id,
                    }
                })
            })
    }

    pub(super) fn selected_queue_pin(&self) -> Option<PinnedPanel> {
        self.selected_city
            .map(|i| PinnedPanel {
                kind: PinnedKind::CityQueue,
                city_id: self.cities[i].id,
            })
            .or_else(|| {
                self.selected_barracks.map(|i| PinnedPanel {
                    kind: PinnedKind::BarracksQueue,
                    city_id: self.cities[i].id,
                })
            })
    }

    pub(super) fn pinned_city_index(&self, pin: PinnedPanel) -> Option<usize> {
        if matches!(pin.kind, PinnedKind::Unit | PinnedKind::Group) {
            return None;
        }
        self.cities.iter().position(|city| {
            city.id == pin.city_id
                && city.team == self.local_team
                && (matches!(pin.kind, PinnedKind::City | PinnedKind::CityQueue)
                    || city.barracks.is_some())
        })
    }

    fn pinned_unit_index(&self, pin: PinnedPanel) -> Option<usize> {
        (pin.kind == PinnedKind::Unit)
            .then(|| {
                self.units
                    .iter()
                    .position(|unit| unit.id == pin.city_id && unit.team == self.local_team)
            })
            .flatten()
    }

    pub(super) fn valid_pin(&self, pin: PinnedPanel) -> bool {
        self.pinned_city_index(pin).is_some()
            || self.pinned_unit_index(pin).is_some()
            || pin.kind == PinnedKind::Group
    }

    /// What captured panel `pin` is for this frame, found by its stable ids
    /// (a group's members by `groups`, the layout's `pinned_groups`), if
    /// anything of it is left.
    pub(super) fn pin_focus(&self, pin: PinnedPanel, groups: &PinnedGroups) -> Option<PinFocus> {
        match pin.kind {
            PinnedKind::Unit => self
                .pinned_unit_index(pin)
                .map(|i| PinFocus::Units(vec![i])),
            PinnedKind::Group => {
                let members: Vec<_> = groups
                    .get(&pin.city_id)?
                    .iter()
                    .filter_map(|id| {
                        self.units
                            .iter()
                            .position(|u| u.id == *id && u.team == self.local_team)
                    })
                    .collect();
                (!members.is_empty()).then_some(PinFocus::Units(members))
            }
            PinnedKind::City | PinnedKind::CityQueue => {
                self.pinned_city_index(pin).map(PinFocus::City)
            }
            PinnedKind::Barracks | PinnedKind::BarracksQueue => {
                self.pinned_city_index(pin).map(PinFocus::Barracks)
            }
        }
    }

    /// What a captured panel's tooltips describe its buttons for.
    pub(super) fn pin_subject(focus: &PinFocus) -> Subject {
        match focus {
            PinFocus::Units(units) => Subject {
                unit: units.first().copied(),
                city: None,
            },
            PinFocus::City(city) | PinFocus::Barracks(city) => Subject {
                unit: None,
                city: Some(*city),
            },
        }
    }

    /// Makes `focus` the selection or the open view, through the same entry
    /// points as the map (`set_selection`, `open_city`, `open_barracks`),
    /// unless it already is, so an armed button stays armed (and toggles
    /// off) and the camera and notice stay put. Refuses, as selecting does,
    /// while a turn plays out.
    fn focus_pin(&mut self, focus: &PinFocus) -> bool {
        if self.is_playing_out() {
            return false;
        }
        let view_open = self.selected_city.is_some()
            || self.selected_barracks.is_some()
            || self.interior_view.is_some();
        match focus {
            PinFocus::Units(units) => {
                let mut selected = self.selection();
                let mut wanted = units.clone();
                selected.sort_unstable();
                wanted.sort_unstable();
                if view_open || selected != wanted {
                    self.set_selection(units.clone());
                }
            }
            PinFocus::City(city) => {
                if self.selected_city != Some(*city) || self.interior_view.is_some() {
                    self.leave_city_view();
                    self.open_city(*city);
                }
            }
            PinFocus::Barracks(city) => {
                if self.selected_barracks != Some(*city) || self.interior_view.is_some() {
                    self.leave_city_view();
                    self.open_barracks(*city);
                }
            }
        }
        true
    }

    /// A button of captured panel `pin`: focuses what the panel is for,
    /// then does what the button does.
    pub(super) fn activate_pinned(
        &mut self,
        pin: PinnedPanel,
        groups: &PinnedGroups,
        target: Target,
    ) {
        if let Some(focus) = self.pin_focus(pin, groups)
            && self.focus_pin(&focus)
        {
            self.activate_target(target);
        }
    }

    pub(super) fn reorder_pinned(
        &mut self,
        pin: PinnedPanel,
        groups: &PinnedGroups,
        kind: QueueKind,
        source: usize,
        target: usize,
    ) {
        if let Some(focus @ (PinFocus::City(_) | PinFocus::Barracks(_))) =
            self.pin_focus(pin, groups)
            && self.focus_pin(&focus)
        {
            self.reorder_queue(kind, source, target);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_pinned_panel(
        &self,
        ui: &Ui,
        layout: &mut ImGuiLayoutState,
        pin: PinnedPanel,
        index: usize,
        viewport: Vec2,
        fonts: &[FontId; 3],
        arranging: bool,
        actions: &mut Vec<Action>,
    ) {
        let Some(focus) = self.pin_focus(pin, &layout.pinned_groups) else {
            return;
        };
        let subject = Self::pin_subject(&focus);
        let mut panel = PanelBuilder::default();
        match (pin.kind, focus) {
            (PinnedKind::City, PinFocus::City(city)) => {
                self.city_tray(city, &mut panel);
                let mut queue = PanelBuilder::default();
                if !layout.pinned.contains(&PinnedPanel {
                    kind: PinnedKind::CityQueue,
                    city_id: pin.city_id,
                }) {
                    self.city_queue_panel(city, usize::MAX, &mut queue);
                }
                if !queue.rows.is_empty() {
                    panel.gap(GAP);
                    panel.rows.extend(queue.rows);
                }
            }
            (PinnedKind::Barracks, PinFocus::Barracks(city)) => {
                self.barracks_tray(city, &mut panel);
                let mut queue = PanelBuilder::default();
                if !layout.pinned.contains(&PinnedPanel {
                    kind: PinnedKind::BarracksQueue,
                    city_id: pin.city_id,
                }) {
                    self.barracks_queue_panel(city, usize::MAX, &mut queue);
                }
                if !queue.rows.is_empty() {
                    panel.gap(GAP);
                    panel.rows.extend(queue.rows);
                }
            }
            (PinnedKind::Unit, PinFocus::Units(units)) => {
                self.unit_info(units[0], &mut panel);
                panel.action_toolbar(self.unit_buttons(units[0]));
            }
            (PinnedKind::Group, PinFocus::Units(members)) => {
                self.group_tray_for(&members, &mut panel);
            }
            (PinnedKind::CityQueue, PinFocus::City(city)) => {
                self.city_queue_panel(city, usize::MAX, &mut panel);
            }
            (PinnedKind::BarracksQueue, PinFocus::Barracks(city)) => {
                self.barracks_queue_panel(city, usize::MAX, &mut panel);
            }
            _ => unreachable!("pin_focus follows the pin's kind"),
        }
        let title = pin.title();
        let width = 370.0_f32.min(viewport.x - 2.0 * PANEL_MARGIN);
        let height = measure_panel(ui, &panel, fonts, width)
            .min(viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN)
            .max(180.0);
        let column = ((viewport.x - 2.0 * PANEL_MARGIN) / (width + PANEL_GAP))
            .floor()
            .max(1.0) as usize;
        let previous_height: f32 = layout
            .pinned
            .iter()
            .take(index)
            .enumerate()
            .filter(|(i, _)| i % column == index % column)
            .map(|(_, prior)| {
                layout
                    .pinned_geometry
                    .get(prior)
                    .map_or(height, |g| g.size.y)
                    + PANEL_GAP
            })
            .sum();
        let default_pos = [
            PANEL_MARGIN + (index % column) as f32 * (width + PANEL_GAP),
            STATUS_HEIGHT + PANEL_MARGIN + previous_height,
        ];
        let mut docking_now = false;
        if let Some(request) = layout
            .pending_pin_dock
            .iter()
            .position(|(item, _)| *item == pin)
        {
            let (_, box_id) = layout.pending_pin_dock[request];
            if let Some(outer) = layout.outer_boxes.iter().find(|b| b.id == box_id)
                && outer.dock_id != 0
            {
                unsafe {
                    ::imgui::sys::igSetNextWindowDockID(
                        outer.dock_id,
                        ::imgui::sys::ImGuiCond_Always as i32,
                    )
                };
                layout.pending_pin_dock.remove(request);
                docking_now = true;
            }
        }
        let geometry = layout.pinned_geometry.entry(pin).or_default();
        let native = native_window(&title);
        if !native.is_null() {
            let native = unsafe { &*native };
            geometry.docked = !native.DockNode.is_null();
            geometry.collapsed = native.Collapsed;
            geometry.pos = Vec2::new(native.Pos.x, native.Pos.y);
            geometry.size = Vec2::new(native.SizeFull.x, native.SizeFull.y);
        }
        let mut window = ui
            .window(&title)
            .flags(panel_chrome(arranging, geometry.collapsed));
        if !geometry.docked && !layout.defer_geometry && !docking_now {
            window = window
                .position(default_pos, Condition::FirstUseEver)
                .size([width, height], Condition::FirstUseEver)
                .size_constraints(
                    [240.0, 130.0],
                    [
                        viewport.x - 2.0 * PANEL_MARGIN,
                        viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN,
                    ],
                );
        }
        window.build(|| {
            self.render_imgui_panel(ui, &panel, fonts, Some(pin), subject, actions);
            geometry.pos = Vec2::from_array(ui.window_pos());
            geometry.size = Vec2::from_array(ui.window_size());
            geometry.docked = unsafe { ::imgui::sys::igIsWindowDocked() };
            if !geometry.docked {
                geometry.floating_pos = geometry.pos;
                geometry.floating_size = geometry.size;
            }
        });
    }
}
