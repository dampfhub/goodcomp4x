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
pub(super) use native::{dock_beside, forget_docking, native_geometry};
#[cfg(test)]
pub(super) use test_hooks::{
    DRAWN_BUTTONS, DRAWN_MARKS, DrawnControl, DrawnMark, SHORT_BUTTONS, SHOWN_NOTICE,
    SHOWN_TOOLTIPS, STATUS_CONTROLS, one_context_at_a_time,
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

const PANEL_MARGIN: f32 = 14.0;
/// Room kept between the status bar's notice and End Turn after it.
const NOTICE_GAP: f32 = 16.0;
/// Where the tooltip holding a shortened notice in full wraps.
const NOTICE_TOOLTIP_WIDTH: f32 = 480.0;
const PANEL_GAP: f32 = 8.0;
pub(super) const STATUS_HEIGHT: f32 = 52.0;
/// Where the status bar's second line starts (Menu), and the room after
/// Menu, before the view's layout controls.
const STATUS_MENU_X: f32 = 15.0;
const STATUS_MENU_GAP: f32 = 24.0;
/// Room before the notice, after the stockpile.
const STATUS_NOTICE_GAP: f32 = 24.0;
/// The top status bar is a fixed strip: its second row of small buttons
/// reaches a few pixels past `STATUS_HEIGHT` with the window padding, which
/// must not turn into a scrollbar.
const STATUS_FLAGS: WindowFlags = WindowFlags::NO_TITLE_BAR
    .union(WindowFlags::NO_RESIZE)
    .union(WindowFlags::NO_MOVE)
    .union(WindowFlags::NO_DOCKING)
    .union(WindowFlags::NO_SAVED_SETTINGS)
    .union(WindowFlags::NO_SCROLLBAR)
    .union(WindowFlags::NO_SCROLL_WITH_MOUSE);
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
        let mut actions = Vec::new();
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
                && (pin.kind != PinnedKind::Group || valid_groups.contains(&pin.city_id))
        });
        layout
            .pinned_geometry
            .retain(|pin, _| layout.pinned.contains(pin));
        let turn = self.shown_turn();
        // The stockpile and supply beside the turn number: all of it, or
        // without the stockpile's change a turn when that won't fit.
        let status_lines = [self.status_line(), self.brief_status_line()].map(|mut line| {
            if let Some((first, _)) = line.first_mut() {
                first.insert_str(0, "   ");
            }
            line
        });
        // The status bar's buttons act on no unit or city.
        let no_subject = Subject::default();
        ui.window("Status")
            .flags(STATUS_FLAGS)
            .position([0.0, 0.0], Condition::Always)
            .size([viewport.x, STATUS_HEIGHT], Condition::Always)
            .build(|| {
                let end_width = 220.0;
                // Everything else on either line stops short of End Turn.
                let end_x = (viewport.x - end_width).max(8.0);
                let limit = end_x - NOTICE_GAP;
                let turn_color = self.turn_number_color(ui.style_color(StyleColor::Text));
                rich_text(ui, &text!("status_turn", turn = turn), turn_color);
                let mut first_bottom = ui.item_rect_max()[1];
                // The player's stockpile and supply, as much as fits (all
                // of it in any window the game allows), then the notice in
                // what's left. Each goes on the line only if drawn: a
                // `same_line` with nothing after it would take Menu's line
                // up to this one.
                let line_width = |line: &Line| -> f32 {
                    line.iter().map(|(text, _)| rich_width(ui, text)).sum()
                };
                let left = ui.window_pos()[0];
                let mut x = ui.item_rect_max()[0] - left;
                let stockpile = status_lines
                    .iter()
                    .find(|line| x + line_width(line) <= limit)
                    .unwrap_or(&status_lines[1]);
                let mut cut = false;
                for (text, color) in stockpile {
                    if x + rich_width(ui, text) > limit {
                        cut = true;
                        break;
                    }
                    ui.same_line_with_spacing(0.0, 0.0);
                    rich_text(ui, text, *color);
                    first_bottom = first_bottom.max(ui.item_rect_max()[1]);
                    x = ui.item_rect_max()[0] - left;
                }
                // The notice has the rest of the line, up to End Turn,
                // shortened to what fits (none if the stockpile was cut);
                // hovering shows all of it.
                let notice = self.shown_notice();
                let room = if cut {
                    0.0
                } else {
                    limit - x - STATUS_NOTICE_GAP
                };
                let shown = fit_text(notice, room, |t| rich_width(ui, t));
                if !shown.is_empty() {
                    ui.same_line_with_spacing(0.0, STATUS_NOTICE_GAP);
                    rich_text(ui, &shown, NOTICE_TEXT);
                    first_bottom = first_bottom.max(ui.item_rect_max()[1]);
                    note_shown_notice(&shown, ui.item_rect_min(), ui.item_rect_max(), limit);
                    if shown != notice && ui.is_item_hovered() {
                        ui.tooltip(|| {
                            let _wrap = ui.push_text_wrap_pos_with_pos(NOTICE_TOOLTIP_WIDTH);
                            rich_text(ui, notice, NOTICE_TEXT);
                        });
                    }
                }
                // The second line, below the first: Menu, then the view's
                // layout controls, so the notice above has the width of
                // the bar. The view's name goes first when there's room,
                // shortened or left to the layer button's tooltip when
                // there isn't.
                let spacing = ui.clone_style().item_spacing[0];
                let pad = ui.clone_style().frame_padding[0];
                let button_width = |label: &str| ui.calc_text_size(label)[0] + 2.0 * pad;
                let name = layout.active_view.label();
                let (layer, layer_hover, layer_tip) = if layout.editing_outer {
                    (
                        text!("view_edit_outer"),
                        hover_text!("view_edit_outer"),
                        tooltip!("view_edit_outer", view = name),
                    )
                } else {
                    (
                        text!("view_edit_view"),
                        hover_text!("view_edit_view"),
                        tooltip!("view_edit_view", view = name),
                    )
                };
                let (add_box, reset_label) = (text!("view_add_box"), text!("view_reset"));
                let menu = self.menu_button();
                let reset = layout.active_view != ViewScope::Default && !layout.editing_outer;
                let mut controls = STATUS_MENU_X + button_width(&menu.label) + STATUS_MENU_GAP;
                controls += button_width(layer) + spacing + button_width(add_box);
                if reset {
                    controls += spacing + button_width(reset_label);
                }
                let view = [text!("view_name", view = name), name.to_string()]
                    .into_iter()
                    .find(|view| controls + ui.calc_text_size(view)[0] + spacing <= limit);
                // A control's label: its hover text while the cursor is on
                // it, if a text file gives it one; ImGui knows it by `id`.
                let shown = |label: &'static str, hover: Option<&'static str>, id: &str| {
                    let shown = hover.filter(|_| hovered_last_frame(id)).unwrap_or(label);
                    (shown, format!("{shown}###{id}"))
                };
                let second_line = (first_bottom - ui.window_pos()[1] + 2.0).ceil();
                ui.set_cursor_pos([STATUS_MENU_X, second_line]);
                let menu_shown =
                    hovered_spec(&menu, "Menu").map_or(menu.label.clone(), |s| s.label);
                clear_status_controls();
                if ui.small_button(format!("{menu_shown}###Menu")) {
                    actions.push(Action::Button(None, Target::OpenSettings));
                }
                note_status_control(ui, &menu_shown);
                note_drawn_button(ui, Target::OpenSettings);
                if ui.is_item_hovered() {
                    let lines = self.subject_tooltip_lines(
                        Target::OpenSettings,
                        &menu.label,
                        None,
                        no_subject,
                    );
                    show_tooltip(ui, &lines);
                }
                ui.same_line_with_spacing(0.0, STATUS_MENU_GAP);
                if let Some(view) = &view {
                    ui.text(view);
                    note_status_control(ui, view);
                    ui.same_line();
                }
                let (layer_shown, layer_id) = shown(layer, layer_hover, "Layer");
                if ui.small_button(layer_id) {
                    actions.push(Action::ToggleLayoutLayer);
                }
                note_status_control(ui, layer_shown);
                if ui.is_item_hovered() {
                    ui.tooltip_text(layer_tip);
                }
                ui.same_line();
                let (box_shown, box_id) = shown(add_box, hover_text!("view_add_box"), "AddBox");
                if ui.small_button(box_id) {
                    actions.push(Action::CreateBox);
                }
                note_status_control(ui, box_shown);
                if reset {
                    ui.same_line();
                    let (reset_shown, reset_id) =
                        shown(reset_label, hover_text!("view_reset"), "Reset");
                    if ui.small_button(reset_id) {
                        layout.request_reset_active_view();
                    }
                    note_status_control(ui, reset_shown);
                    if ui.is_item_hovered() {
                        let key = Command::ResetLayout.key_in_words();
                        ui.tooltip_text(tooltip!("view_reset", key = key));
                    }
                }
                ui.set_cursor_pos([end_x, 7.0]);
                // Waiting for the others' plans, it takes this side's back.
                let end_turn = self.end_turn_button();
                let _disabled = ui.begin_disabled(end_turn.unavailable.is_some());
                let end_size = [end_width - 15.0, 29.0];
                let end_shown =
                    hovered_spec(&end_turn, "EndTurn").map_or(end_turn.label.clone(), |s| s.label);
                if ui.button_with_size(format!("{end_shown}###EndTurn"), end_size) {
                    actions.push(Action::Button(None, Target::EndTurn));
                }
                note_drawn_button(ui, Target::EndTurn);
                if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                    let lines = self.subject_tooltip_lines(
                        Target::EndTurn,
                        &end_turn.label,
                        end_turn.unavailable.as_deref(),
                        no_subject,
                    );
                    show_tooltip(ui, &lines);
                }
            });

        draw_outer_boxes(ui, layout, viewport, arranging, &mut actions);

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
            self.render_pinned_panel(
                ui,
                layout,
                pin,
                index,
                viewport,
                fonts,
                arranging,
                &mut actions,
            );
        }
        let positions = layout.plan(viewport, &mut sizes);
        let pair_visible = positions[SELECTION].is_some() && positions[QUEUE].is_some();

        if let (Some(position), Some(size)) = (positions[SELECTION], sizes[SELECTION]) {
            self.render_imgui_window(
                ui,
                layout,
                SELECTION,
                slot_titles()[SELECTION],
                position,
                size,
                viewport,
                &tray,
                fonts,
                arranging,
                reset_selection_scroll,
                &mut actions,
            );
        }
        if let (Some(position), Some(size)) = (positions[QUEUE], sizes[QUEUE]) {
            self.render_imgui_window(
                ui,
                layout,
                QUEUE,
                slot_titles()[QUEUE],
                position,
                size,
                viewport,
                &queue,
                fonts,
                arranging,
                false,
                &mut actions,
            );
        }
        if let (Some(position), Some(size)) = (positions[DEBUG], sizes[DEBUG]) {
            self.render_imgui_window(
                ui,
                layout,
                DEBUG,
                slot_titles()[DEBUG],
                position,
                size,
                viewport,
                &debug,
                fonts,
                arranging,
                false,
                &mut actions,
            );
        }
        if let (Some(position), Some(size)) = (positions[INSPECT], sizes[INSPECT]) {
            self.render_imgui_window(
                ui,
                layout,
                INSPECT,
                slot_titles()[INSPECT],
                position,
                size,
                viewport,
                &hover,
                fonts,
                arranging,
                false,
                &mut actions,
            );
        }
        if let (Some(position), Some(size), Some(units)) =
            (positions[UNITS], sizes[UNITS], units.as_ref())
        {
            self.render_imgui_window(
                ui,
                layout,
                UNITS,
                slot_titles()[UNITS],
                position,
                size,
                viewport,
                units,
                fonts,
                arranging,
                false,
                &mut actions,
            );
        }
        if let (Some(position), Some(size), Some(settings)) =
            (positions[SETTINGS], sizes[SETTINGS], settings.as_ref())
        {
            self.render_imgui_window(
                ui,
                layout,
                SETTINGS,
                slot_titles()[SETTINGS],
                position,
                size,
                viewport,
                settings,
                fonts,
                arranging,
                false,
                &mut actions,
            );
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
                    pin.city_id,
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
        for action in actions {
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
