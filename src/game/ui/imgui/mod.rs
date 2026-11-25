//! Experimental immediate-mode presentation of the game's existing panel content.
//! Game rules and button actions remain shared with the classic UI.

use ::imgui::{
    Condition, DragDropFlags, FontId, InputTextCallback, InputTextCallbackHandler, InputTextFlags,
    ItemHoveredFlags, MouseButton as ImMouseButton, ProgressBar, SliderFlags, StyleColor, StyleVar,
    TextCallbackData, Ui, WindowFlags,
};

use super::action_icons::{self, ICON_BUTTON_SIZE};
use super::builder::{ButtonSpec, CatalogEntry, Row, flat_rows, icon_row, visible_button_hint};
use super::network_menu::NetField;
use super::text::fit_text;
use super::tooltips::Subject;
use super::*;
use crate::game::keys::Command;
use crate::game::settings::{Control, Setting};
use crate::game::strings::{hover_text, text, tooltip};
use crate::game::{font, map_icons};

mod boxes;
mod dock_tree;
mod layout_state;
mod measure;
mod native;
mod persist;
mod pins;
mod render;
#[cfg(test)]
mod screen;
mod style;
mod test_hooks;
#[cfg(test)]
mod tests;
mod views;
mod widgets;
mod windows;

use boxes::*;
use dock_tree::*;
pub use layout_state::ImGuiLayoutState;
use layout_state::*;
use measure::*;
use native::*;
#[cfg(test)]
use persist::*;
use pins::*;
pub use style::style_imgui;
use test_hooks::*;
use views::*;
use widgets::*;
use windows::*;

#[cfg(test)]
pub(super) use native::forget_docking;
#[cfg(test)]
pub(super) use screen::ImGuiScreen;
#[cfg(test)]
pub(super) use test_hooks::{
    DRAWN_BUTTONS, DRAWN_MARKS, DrawnControl, DrawnMark, SHORT_BUTTONS, SHOWN_NOTICE,
    SHOWN_TOOLTIPS, STATUS_CONTROLS, dock_beside, native_geometry, one_context_at_a_time,
};

enum Action {
    Button(Option<PinnedPanel>, Target),
    /// A typed field's new text.
    Text(NetField, String),
    Reorder(Option<PinnedPanel>, QueueKind, usize, usize),
    CreateBox,
    ToggleLayoutLayer,
    RemoveOuterBox(u32),
}

/// What every window of one frame is drawn with, and the actions its
/// buttons ask for, which `draw_imgui` carries out once all are drawn.
struct ImGuiFrame<'a> {
    ui: &'a Ui,
    /// The screen, in ImGui's logical pixels.
    viewport: Vec2,
    /// The small, body and title fonts (`style_imgui`).
    fonts: &'a [FontId; 3],
    /// Ctrl held: the panels show their title bars and grips.
    arranging: bool,
    actions: Vec<Action>,
}

/// Whom a panel's buttons act for: the captured panel they're in, if any,
/// and the unit or city their tooltips describe.
#[derive(Clone, Copy)]
struct PanelScope {
    pin: Option<PinnedPanel>,
    subject: Subject,
}

const PANEL_MARGIN: f32 = 14.0;
const PANEL_GAP: f32 = 8.0;
pub(super) const STATUS_HEIGHT: f32 = 52.0;
/// Height of a progress bar row (`Row::Bar`).
const BAR_HEIGHT: f32 = 12.0;
/// The settings menu's width, before a narrow screen takes some off.
const SETTINGS_WIDTH: f32 = 460.0;
const COLLAPSED_HEIGHT: f32 = 30.0;
const SLOT_COUNT: usize = 6;
const SELECTION: usize = 0;
const QUEUE: usize = 1;
const DEBUG: usize = 2;
const INSPECT: usize = 3;
const UNITS: usize = 4;
const SETTINGS: usize = 5;
/// Each slot's native window title, by slot (`text/ui.ini`). A new panel
/// that isn't static chrome (like the status bar) gets a slot here, so it
/// can be dragged, docked, resized and put in a box like the rest. ImGui
/// knows a window by its title: code names a slot's window with this, never
/// its words.
fn slot_titles() -> [&'static str; SLOT_COUNT] {
    [
        text!("window_selection"),
        text!("window_queue"),
        text!("window_debug"),
        text!("window_inspect"),
        text!("window_units"),
        text!("window_settings"),
    ]
}
/// The order `plan` places automatic panels in: earlier ones get the space
/// nearest their zone's corner. The settings menu isn't docked: it's centered.
const PLAN_ORDER: [usize; SLOT_COUNT] = [SELECTION, QUEUE, SETTINGS, DEBUG, INSPECT, UNITS];

impl GameState {
    /// Draw native ImGui windows over the Vulkan map and return UI input to the
    /// same game actions used by keyboard shortcuts and the classic panels.
    pub fn draw_imgui(
        &mut self,
        ui: &Ui,
        size: Vec2,
        cursor: Option<Vec2>,
        fonts: &[FontId; 3],
        layout: &mut ImGuiLayoutState,
    ) {
        let viewport = Vec2::from_array(ui.io().display_size);
        let arranging = ui.io().key_ctrl;
        layout.begin_frame(ui, arranging);
        // Pins are by city and unit id, which every new game reuses.
        if layout.game_changed(self.generation) {
            layout.clear_captured_panels();
        }
        // The game window docked as the view has it, before its dockspace
        // is drawn (#360).
        layout.update_game_dock(self.layout_view());
        layout.game_dock_id = draw_game_dockspace(ui, viewport);
        // Dear ImGui applies a dock drop in NewFrame, before these windows are
        // submitted. Observe that new dock state now; otherwise our cached
        // floating geometry can undo a valid split on its first frame.
        for (slot, title) in slot_titles().into_iter().enumerate() {
            layout.sync_native_window(slot, title);
        }
        layout.switch_layout_scope(self.layout_view());
        layout.apply_pending_view_reset();
        let mut frame = ImGuiFrame {
            ui,
            viewport,
            fonts,
            arranging,
            actions: Vec::new(),
        };
        let valid_groups: Vec<_> = layout
            .pinned_groups
            .iter()
            .filter_map(|(id, members)| {
                members
                    .iter()
                    .any(|member| {
                        self.units
                            .iter()
                            .any(|unit| unit.id == *member && unit.team == self.local_team)
                    })
                    .then_some(*id)
            })
            .collect();
        layout.pinned.retain(|pin| {
            self.valid_pin(*pin)
                && (pin.kind != PinnedKind::Group || valid_groups.contains(&pin.id))
        });
        layout
            .pinned_geometry
            .retain(|pin, _| layout.pinned.contains(pin));
        self.draw_status_bar(&mut frame, layout);
        draw_outer_boxes(&mut frame, layout);

        let selected_pin = self.selected_pin();
        let selection_is_pinned = selected_pin.is_some_and(|pin| layout.pinned.contains(&pin));
        let selected_queue_pin = self.selected_queue_pin();
        let queue_is_pinned = selected_queue_pin.is_some_and(|pin| layout.pinned.contains(&pin));
        let mut tray = PanelBuilder::default();
        if selection_is_pinned {
            // Its persistent window owns these controls while the structure is selected.
        } else if let Some(city) = self.interior_view {
            self.interior_tray(city, &mut tray);
        } else if let Some(city) = self.selected_city {
            self.city_tray(city, &mut tray);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_tray(city, &mut tray);
        } else if let Some(idx) = self.selected {
            self.unit_info(idx, &mut tray);
            tray.action_toolbar(self.unit_buttons(idx));
        } else if !self.group.is_empty() {
            self.group_tray(&mut tray);
        } else if self.research_view() {
            self.research_tray(&mut tray);
        }
        // A pinned structure's own window has its controls, not Selection.
        let selection = self.selection_kind().filter(|_| !selection_is_pinned);
        let selection_context = selection
            .as_ref()
            .map_or_else(String::new, |(_, context)| context.clone());
        let reset_selection_scroll = layout.selection_changed(&selection_context);
        let mut queue = PanelBuilder::default();
        if selection_is_pinned || queue_is_pinned {
        } else if let Some(city) = self.selected_city {
            self.city_queue_panel(city, usize::MAX, &mut queue);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_queue_panel(city, usize::MAX, &mut queue);
        } else if self.research_view() {
            self.research_queue_panel(usize::MAX, &mut queue);
        }
        let debug = self.debug_panel_content();
        let mut hover = PanelBuilder::default();
        if let Some(hex) = self.hovered_tile {
            if let Some(panel) = self.structure_inspect_panel(hex) {
                hover = panel;
            } else if let Some(cursor) = cursor
                && let Some(idx) = self.unit_at_screen(cursor, size)
                && (Some(idx) != self.selected || self.selected_city.is_some())
            {
                self.unit_info(idx, &mut hover);
            }
            if self.hover_seconds >= TILE_TOOLTIP_DELAY && !ui.io().want_capture_mouse {
                let lines = self.tile_tooltip_lines(hex);
                ui.tooltip(|| {
                    for (_, line) in &lines {
                        text_line(ui, line);
                    }
                });
            }
        }
        if !hover.rows.is_empty() {
            layout.inspector_snapshot = Some(hover.clone());
        } else if layout
            .outer_box_for_window(slot_titles()[INSPECT])
            .is_some()
        {
            hover = layout.inspector_snapshot.clone().unwrap_or_default();
        } else {
            layout.inspector_snapshot = None;
        }

        // Measure the actual ImGui row geometry in logical pixels. The dock
        // reserves every visible panel before any window is presented.
        let available = (viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN).max(100.0);
        let max_width = (viewport.x - 2.0 * PANEL_MARGIN).max(240.0);
        let (left_width, debug_width) = if viewport.x < 1100.0 {
            let debug_width = (viewport.x * 0.43).clamp(220.0, 330.0).min(max_width);
            let left_width = (viewport.x - debug_width - 3.0 * PANEL_MARGIN - PANEL_GAP)
                .max(210.0)
                .min(max_width);
            (left_width, debug_width)
        } else {
            (560.0_f32.min(max_width), 330.0_f32.min(max_width))
        };
        let units = self.roster_panel();
        let settings = self.settings_open.then(|| self.settings_panel_content());
        let queue_height = measure_panel(ui, &queue, fonts, left_width).min(225.0);
        let mut tray_height = measure_panel(ui, &tray, fonts, left_width).min(available);
        // On a narrow screen the queue cannot wrap into a second column, so
        // reserve its space above the selection before docking either window.
        if !tray.rows.is_empty()
            && !queue.rows.is_empty()
            && viewport.x < 2.0 * left_width + 3.0 * PANEL_MARGIN + PANEL_GAP
        {
            tray_height = tray_height.min((available - queue_height - PANEL_GAP).max(120.0));
        }
        let measured = [
            (!tray.rows.is_empty()).then_some(Vec2::new(left_width, tray_height)),
            (!queue.rows.is_empty()).then_some(Vec2::new(left_width, queue_height)),
            Some(Vec2::new(
                debug_width,
                measure_panel(ui, &debug, fonts, debug_width).min(available),
            )),
            (!hover.rows.is_empty()).then_some(Vec2::new(
                345.0_f32.min(max_width),
                measure_panel(ui, &hover, fonts, 345.0_f32.min(max_width)).min(available),
            )),
            units.as_ref().map(|units| {
                let width = roster_width(units).min(max_width);
                Vec2::new(width, measure_panel(ui, units, fonts, width).min(available))
            }),
            settings.as_ref().map(|settings| {
                // Wide enough for a label column and a control beside it.
                let width = SETTINGS_WIDTH.min(max_width);
                Vec2::new(
                    width,
                    measure_panel(ui, settings, fonts, width).min(available),
                )
            }),
        ];
        let mut sizes = std::array::from_fn(|slot| {
            measured[slot].map(|size| layout.size(slot, size, viewport))
        });
        let pins = layout.pinned.clone();
        for (index, pin) in pins.into_iter().enumerate() {
            if !layout.pin_visible(pin) {
                continue;
            }
            self.render_pinned_panel(&mut frame, layout, pin, index);
        }
        let positions = layout.plan(viewport, &mut sizes);
        let pair_visible = positions[SELECTION].is_some() && positions[QUEUE].is_some();

        let panels = [
            Some(&tray),
            Some(&queue),
            Some(&debug),
            Some(&hover),
            units.as_ref(),
            settings.as_ref(),
        ];
        for slot in 0..SLOT_COUNT {
            if let (Some(position), Some(size), Some(panel)) =
                (positions[slot], sizes[slot], panels[slot])
            {
                let reset_scroll = slot == SELECTION && reset_selection_scroll;
                self.render_imgui_window(
                    &mut frame,
                    layout,
                    slot,
                    (position, size),
                    panel,
                    reset_scroll,
                );
            }
        }
        layout.maintain_queue_dock(pair_visible);
        if !layout.editing_outer {
            layout.maintain_debug_scope(selection.map(|(kind, _)| kind));
        }
        layout.note_floating(&positions);
        layout.track_captured_panels();
        let selection_box = layout.outer_box_for_window(slot_titles()[SELECTION]);
        if selection_box != layout.last_selection_outer_box
            && !selection_is_pinned
            && !tray.rows.is_empty()
            && let (Some(pin), Some(box_id)) = (selected_pin, selection_box)
        {
            if pin.kind == PinnedKind::Group {
                layout.pinned_groups.insert(
                    pin.id,
                    self.group.iter().map(|&i| self.units[i].id).collect(),
                );
            }
            layout.capture_panel(pin, box_id, slot_titles()[SELECTION]);
        }
        layout.last_selection_outer_box = selection_box;
        let queue_box = layout.outer_box_for_window(slot_titles()[QUEUE]);
        if queue_box != layout.last_queue_outer_box
            && !selection_is_pinned
            && !queue_is_pinned
            && !queue.rows.is_empty()
            && let (Some(pin), Some(box_id)) = (selected_queue_pin, queue_box)
        {
            layout.capture_panel(pin, box_id, slot_titles()[QUEUE]);
        }
        layout.last_queue_outer_box = queue_box;
        for action in frame.actions {
            match action {
                Action::Button(Some(pin), target) => {
                    self.activate_pinned(pin, &layout.pinned_groups, target)
                }
                // A new scenario or a load clears the captured panels at the
                // next frame's start (`ImGuiLayoutState::game_changed`).
                Action::Button(None, target) => self.activate_target(target),
                Action::Reorder(Some(pin), kind, source, target) => {
                    self.reorder_pinned(pin, &layout.pinned_groups, kind, source, target)
                }
                Action::Reorder(None, kind, source, target) => {
                    self.reorder_queue(kind, source, target)
                }
                Action::Text(field, text) => self.set_net_field(field, &text),
                Action::CreateBox => layout.create_box(viewport),
                Action::ToggleLayoutLayer => layout.editing_outer = !layout.editing_outer,
                Action::RemoveOuterBox(id) => layout.remove_outer_box(id),
            }
        }
    }
}
