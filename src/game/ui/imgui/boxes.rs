//! Boxes: windows the player adds (+ BOX) to hold panels, for one view or
//! for all of them (Outer).

use super::*;

#[derive(Clone, Copy)]
pub(super) struct OuterBox {
    pub(super) id: u32,
    pub(super) dock_id: u32,
    pub(super) scope: BoxScope,
    pub(super) geometry: WindowGeometry,
}

impl OuterBox {
    pub(super) fn title(self) -> String {
        format!("{}###OuterBox-{}", self.scope.box_name(self.id), self.id)
    }
}

impl ImGuiLayoutState {
    pub(super) fn box_visible(&self, outer: &OuterBox) -> bool {
        outer.scope == BoxScope::Outer || outer.scope == BoxScope::View(self.active_view)
    }

    pub(super) fn pin_visible(&self, pin: PinnedPanel) -> bool {
        self.pinned_box.get(&pin).is_some_and(|id| {
            self.outer_boxes
                .iter()
                .any(|outer| outer.id == *id && self.box_visible(outer))
        })
    }

    pub(super) fn capture_panel(&mut self, pin: PinnedPanel, box_id: u32, title: &str) {
        if self.pinned.contains(&pin) {
            return;
        }
        self.pinned.push(pin);
        self.pinned_box.insert(pin, box_id);
        self.pinned_outside_frames.insert(pin, 0);
        self.pending_pin_dock.push((pin, box_id));
        undock(title);
    }

    pub(super) fn track_captured_panels(&mut self) {
        let mut remove = Vec::new();
        for pin in self.pinned.clone() {
            if !self.pin_visible(pin) {
                continue;
            }
            if let Some(box_id) = self.outer_box_for_window(&pin.title()) {
                self.pinned_box.insert(pin, box_id);
                self.pinned_outside_frames.insert(pin, 0);
            } else if !self.defer_geometry {
                let frames = self.pinned_outside_frames.entry(pin).or_default();
                *frames = frames.saturating_add(1);
                if *frames > 3 {
                    remove.push(pin);
                }
            }
        }
        for pin in remove {
            self.pinned.retain(|item| *item != pin);
            self.pinned_geometry.remove(&pin);
            self.pinned_box.remove(&pin);
            self.pinned_outside_frames.remove(&pin);
            if pin.kind == PinnedKind::Group {
                self.pinned_groups.remove(&pin.id);
            }
        }
    }

    pub(super) fn remove_outer_box(&mut self, box_id: u32) {
        for title in slot_titles() {
            if self.outer_box_for_window(title) == Some(box_id) {
                undock(title);
                if title == slot_titles()[DEBUG] {
                    self.reposition[DEBUG] = true;
                }
                if title == slot_titles()[INSPECT] {
                    self.inspector_snapshot = None;
                }
            }
        }
        let captured: Vec<_> = self
            .pinned_box
            .iter()
            .filter_map(|(pin, id)| (*id == box_id).then_some(*pin))
            .collect();
        for pin in captured {
            undock(&pin.title());
            self.pinned.retain(|item| *item != pin);
            self.pinned_geometry.remove(&pin);
            self.pinned_box.remove(&pin);
            self.pinned_outside_frames.remove(&pin);
            if pin.kind == PinnedKind::Group {
                self.pinned_groups.remove(&pin.id);
            }
        }
        self.pending_pin_dock.retain(|(_, id)| *id != box_id);
        self.debug_view_boxes.retain(|_, id| *id != box_id);
        if self.pending_debug_box == Some(box_id) {
            self.pending_debug_box = None;
        }
        self.outer_boxes.retain(|b| b.id != box_id);
        if self.last_selection_outer_box == Some(box_id) {
            self.last_selection_outer_box = None;
        }
        if self.last_queue_outer_box == Some(box_id) {
            self.last_queue_outer_box = None;
        }
    }

    /// Whether the game is another than at the last frame (`generation`:
    /// a scenario switched, by key or button, a load, or a network game),
    /// noting it for the next.
    pub(super) fn game_changed(&mut self, generation: u32) -> bool {
        self.game_generation
            .replace(generation)
            .is_some_and(|last| last != generation)
    }

    /// Releases every captured panel, keeping the boxes.
    pub(super) fn clear_captured_panels(&mut self) {
        for pin in &self.pinned {
            undock(&pin.title());
        }
        for title in [slot_titles()[SELECTION], slot_titles()[QUEUE]] {
            if self.outer_box_for_window(title).is_some() {
                undock(title);
            }
        }
        self.pinned.clear();
        self.pinned_geometry.clear();
        self.pinned_box.clear();
        self.pinned_outside_frames.clear();
        self.pinned_groups.clear();
        self.pending_pin_dock.clear();
        self.inspector_snapshot = None;
    }

    pub(super) fn create_box(&mut self, viewport: Vec2) {
        self.next_outer_box_id += 1;
        let scope = if self.editing_outer {
            BoxScope::Outer
        } else {
            BoxScope::View(self.active_view)
        };
        let size = Vec2::new(380.0, 280.0).min(Vec2::new(
            viewport.x - 2.0 * PANEL_MARGIN,
            viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN,
        ));
        let mut dock = Dock::new(
            viewport,
            PANEL_MARGIN,
            PANEL_GAP,
            STATUS_HEIGHT + PANEL_MARGIN,
        );
        for geometry in self.windows.iter().chain(
            self.outer_boxes
                .iter()
                .filter(|b| self.box_visible(b))
                .map(|b| &b.geometry),
        ) {
            if geometry.size == Vec2::ZERO {
                continue;
            }
            dock.reserve(Rect {
                min: Vec2::new(
                    geometry.pos.x,
                    viewport.y - geometry.pos.y - geometry.size.y,
                ),
                max: Vec2::new(
                    geometry.pos.x + geometry.size.x,
                    viewport.y - geometry.pos.y,
                ),
            });
        }
        let pos = dock
            .place(size, Zone::BottomRight)
            .map(|rect| Vec2::new(rect.min.x, viewport.y - rect.max.y))
            .unwrap_or(Vec2::new(
                (viewport.x - size.x - PANEL_MARGIN).max(PANEL_MARGIN),
                STATUS_HEIGHT + PANEL_MARGIN,
            ));
        self.outer_boxes.push(OuterBox {
            id: self.next_outer_box_id,
            dock_id: 0,
            scope,
            geometry: WindowGeometry {
                pos,
                size,
                floating_pos: pos,
                floating_size: size,
                manual: true,
                sized: true,
                ..Default::default()
            },
        });
    }
    /// Whether `title`'s window is docked in a box shown in `view`, or goes
    /// back to one as it shows again.
    pub(super) fn in_box_of_view(&self, title: &str, view: ViewScope) -> bool {
        self.outer_boxes.iter().any(|outer| {
            (outer.scope == BoxScope::Outer || outer.scope == BoxScope::View(view))
                && outer.dock_id != 0
                && panel_in_dock(title, outer.dock_id)
        })
    }

    pub(super) fn outer_box_for_window(&self, title: &str) -> Option<u32> {
        if !docked(title) {
            return None;
        }
        self.outer_boxes
            .iter()
            .filter(|outer| self.box_visible(outer))
            .find_map(|outer| {
                if outer.dock_id == 0 {
                    return None;
                }
                window_in_dock(title, outer.dock_id).then_some(outer.id)
            })
    }
}

pub(super) fn draw_outer_boxes(frame: &mut ImGuiFrame, layout: &mut ImGuiLayoutState) {
    let (ui, viewport) = (frame.ui, frame.viewport);
    for outer in &mut layout.outer_boxes {
        if outer.scope != BoxScope::Outer && outer.scope != BoxScope::View(layout.active_view) {
            // Keep native dock nodes alive while their view is inactive.
            if outer.dock_id != 0 {
                dockspace(
                    outer.dock_id,
                    ::imgui::sys::ImGuiDockNodeFlags_KeepAliveOnly as i32,
                );
            }
            continue;
        }
        let title = outer.title();
        if let Some(native) = native_state(&title) {
            outer.geometry.docked = native.docked;
            outer.geometry.pos = native.pos;
            outer.geometry.size = native.size;
        }
        let mut window = ui.window(&title).flags(
            panel_chrome(frame.arranging, false)
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_DOCKING,
        );
        if !outer.geometry.docked && !layout.defer_geometry {
            window = window
                .position(outer.geometry.pos.to_array(), Condition::FirstUseEver)
                .size(outer.geometry.size.to_array(), Condition::FirstUseEver)
                .size_constraints(
                    [260.0, 170.0],
                    [
                        viewport.x - 2.0 * PANEL_MARGIN,
                        viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN,
                    ],
                );
        }
        window.build(|| {
            ui.text(outer.scope.box_name(outer.id));
            ui.same_line();
            if ui.small_button(format!("X##outer-{}", outer.id)) {
                frame.actions.push(Action::RemoveOuterBox(outer.id));
            }
            outer.dock_id = item_id(c"Contents");
            dockspace(outer.dock_id, 0);
            outer.geometry.pos = Vec2::from_array(ui.window_pos());
            outer.geometry.size = Vec2::from_array(ui.window_size());
            outer.geometry.docked = current_window_docked();
        });
    }
}
