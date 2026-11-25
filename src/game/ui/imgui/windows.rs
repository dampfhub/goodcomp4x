//! The windows a frame draws: each slot's panel, the status bar and the
//! game window's dockspace.

use super::*;

/// Room kept between the status bar's notice and End Turn after it.
const NOTICE_GAP: f32 = 16.0;
/// Where the tooltip holding a shortened notice in full wraps.
const NOTICE_TOOLTIP_WIDTH: f32 = 480.0;
/// Where the status bar's second line starts (Menu), and the room after
/// Menu, before the view's layout controls.
const STATUS_MENU_X: f32 = 15.0;
const STATUS_MENU_GAP: f32 = 24.0;
/// Room before the notice, after the stockpile.
const STATUS_NOTICE_GAP: f32 = 24.0;
/// The top status bar is a fixed strip: its second row of small buttons
/// reaches a few pixels past `STATUS_HEIGHT` with the window padding, which
/// must not turn into a scrollbar.
pub(super) const STATUS_FLAGS: WindowFlags = WindowFlags::NO_TITLE_BAR
    .union(WindowFlags::NO_RESIZE)
    .union(WindowFlags::NO_MOVE)
    .union(WindowFlags::NO_DOCKING)
    .union(WindowFlags::NO_SAVED_SETTINGS)
    .union(WindowFlags::NO_SCROLLBAR)
    .union(WindowFlags::NO_SCROLL_WITH_MOUSE);

/// The game window below the status bar as a dockspace (#333): a panel
/// dragged to one of its edges docks there, and to the other panels
/// docked there. The rest of it, the map, is the central node, which
/// takes no docking and passes the mouse through
/// (`PassthruCentralNode`: ImGui leaves a hole in the host window where it
/// is, so clicks, drags, the wheel and hovering there reach the game).
/// Returns the dockspace's id.
pub(super) fn draw_game_dockspace(ui: &Ui, viewport: Vec2) -> u32 {
    let _padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
    let mut id = 0;
    ui.window("Game dockspace")
        .flags(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_COLLAPSE
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_DOCKING
                | WindowFlags::NO_BACKGROUND
                | WindowFlags::NO_BRING_TO_FRONT_ON_FOCUS
                | WindowFlags::NO_NAV_FOCUS
                | WindowFlags::NO_SCROLLBAR
                | WindowFlags::NO_SAVED_SETTINGS,
        )
        .position([0.0, STATUS_HEIGHT], Condition::Always)
        .size(
            [viewport.x, (viewport.y - STATUS_HEIGHT).max(1.0)],
            Condition::Always,
        )
        .build(|| {
            id = item_id(c"GameDockspace");
            let flags = ::imgui::sys::ImGuiDockNodeFlags_PassthruCentralNode
                | ::imgui::sys::ImGuiDockNodeFlags_NoDockingInCentralNode;
            dockspace(id, flags as i32);
        });
    id
}

impl GameState {
    /// The status bar across the top: the turn, the stockpile and the latest
    /// notice, then Menu and the view's layout controls; End Turn at the
    /// right.
    pub(super) fn draw_status_bar(&self, frame: &mut ImGuiFrame, layout: &mut ImGuiLayoutState) {
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
        let (ui, viewport) = (frame.ui, frame.viewport);
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
                    frame
                        .actions
                        .push(Action::Button(None, Target::OpenSettings));
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
                // The research view hides the panels these arrange.
                if !self.research_view() {
                    ui.same_line_with_spacing(0.0, STATUS_MENU_GAP);
                    if let Some(view) = &view {
                        ui.text(view);
                        note_status_control(ui, view);
                        ui.same_line();
                    }
                    let (layer_shown, layer_id) = shown(layer, layer_hover, "Layer");
                    if ui.small_button(layer_id) {
                        frame.actions.push(Action::ToggleLayoutLayer);
                    }
                    note_status_control(ui, layer_shown);
                    if ui.is_item_hovered() {
                        ui.tooltip_text(layer_tip);
                    }
                    ui.same_line();
                    let (box_shown, box_id) = shown(add_box, hover_text!("view_add_box"), "AddBox");
                    if ui.small_button(box_id) {
                        frame.actions.push(Action::CreateBox);
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
                }
                ui.set_cursor_pos([end_x, 7.0]);
                // Waiting for the others' plans, it takes this side's back.
                let end_turn = self.end_turn_button();
                let _disabled = ui.begin_disabled(end_turn.unavailable.is_some());
                let end_size = [end_width - 15.0, 29.0];
                let end_shown =
                    hovered_spec(&end_turn, "EndTurn").map_or(end_turn.label.clone(), |s| s.label);
                if ui.button_with_size(format!("{end_shown}###EndTurn"), end_size) {
                    frame.actions.push(Action::Button(None, Target::EndTurn));
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
    }

    /// Draws `slot`'s window, showing `panel`, where the layout planned it
    /// (its position and size), unless the player or ImGui's dock has it;
    /// with `reset_scroll`, scrolled back to its top.
    pub(super) fn render_imgui_window(
        &self,
        frame: &mut ImGuiFrame,
        layout: &mut ImGuiLayoutState,
        slot: usize,
        (position, size): (Vec2, Vec2),
        panel: &PanelBuilder,
        reset_scroll: bool,
    ) {
        let (ui, viewport, arranging) = (frame.ui, frame.viewport, frame.arranging);
        let title = slot_titles()[slot];
        let dock_debug_now = if slot == DEBUG {
            layout.pending_debug_box.and_then(|id| {
                layout
                    .outer_boxes
                    .iter()
                    .find(|b| b.id == id && layout.box_visible(b) && b.dock_id != 0)
                    .map(|b| b.dock_id)
            })
        } else {
            None
        };
        if let Some(dock_id) = dock_debug_now {
            set_next_dock_id(dock_id);
            layout.pending_debug_box = None;
        }
        let context_changed = slot == SELECTION && layout.selection_geometry_changed;
        let restoring = context_changed || layout.reposition[slot];
        let condition = if restoring {
            Condition::Always
        } else {
            layout.condition(slot, viewport)
        };
        let old_size = layout.windows[slot].size;
        let fits = old_size.x <= viewport.x - 2.0 * PANEL_MARGIN
            && old_size.y <= viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN;
        let size_condition = if restoring {
            Condition::Always
        } else if layout.windows[slot].sized && fits {
            Condition::FirstUseEver
        } else {
            Condition::Always
        };
        let flags = panel_chrome(arranging, layout.windows[slot].collapsed);
        let title_reset = title_double_clicked(ui, title, flags);
        let mut window = ui.window(title).flags(flags);
        if slot == SETTINGS {
            // It opens over the other panels: keep their text from showing
            // through its labels.
            window = window.bg_alpha(1.0);
        }
        // While ImGui is moving a panel or showing docking targets it owns the
        // geometry. Applying our automatic position here makes edge previews
        // oscillate between the two layout systems.
        let resized = !layout.windows[slot].docked
            && !layout.defer_geometry
            && dock_debug_now.is_none()
            && !returns_to_game_dock(title, layout.game_dock_id);
        if resized {
            window = window
                .position(position.to_array(), condition)
                .size(size.to_array(), size_condition)
                .size_constraints(
                    [size.x.min(240.0), size.y.min(100.0)],
                    [
                        viewport.x - 2.0 * PANEL_MARGIN,
                        viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN,
                    ],
                );
        }
        let restored =
            layout.reposition[slot] && !layout.windows[slot].docked && !layout.defer_geometry;
        window.build(|| {
            if reset_scroll || ui.is_window_appearing() {
                ui.set_scroll_y(0.0);
            }
            let scope = PanelScope {
                pin: None,
                subject: self.selection_subject(),
            };
            self.render_imgui_panel(frame, panel, scope);
            layout.record(slot, ui, arranging);
        });
        // A collapsed window skips the build closure; native state is still
        // available after Begin/End for the next layout pass.
        if restored {
            layout.reposition[slot] = false;
        }
        layout.sync_native_window(slot, title);
        // Double-clicked: the title bar puts the panel back
        // where the layout would, the grip back to the size it would give.
        if title_reset {
            layout.reset_position(slot);
        }
        if grip_double_clicked(ui, title) {
            layout.reset_size(slot);
        }
        if context_changed && !layout.defer_geometry {
            layout.selection_geometry_changed = false;
        }
    }
}
