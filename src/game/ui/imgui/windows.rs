//! The windows a frame draws: each slot's panel, the status bar and the
//! game window's dockspace.

use super::*;

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
        .build(|| unsafe {
            id = ::imgui::sys::igGetID_Str(c"GameDockspace".as_ptr());
            let flags = ::imgui::sys::ImGuiDockNodeFlags_PassthruCentralNode
                | ::imgui::sys::ImGuiDockNodeFlags_NoDockingInCentralNode;
            ::imgui::sys::igDockSpace(
                id,
                ::imgui::sys::ImVec2 { x: 0.0, y: 0.0 },
                flags as i32,
                std::ptr::null(),
            );
        });
    id
}

impl GameState {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_imgui_window(
        &self,
        ui: &Ui,
        layout: &mut ImGuiLayoutState,
        slot: usize,
        title: &str,
        position: Vec2,
        size: Vec2,
        viewport: Vec2,
        panel: &PanelBuilder,
        fonts: &[FontId; 3],
        arranging: bool,
        reset_scroll: bool,
        actions: &mut Vec<Action>,
    ) {
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
            unsafe {
                ::imgui::sys::igSetNextWindowDockID(dock_id, ::imgui::sys::ImGuiCond_Always as i32)
            };
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
            self.render_imgui_panel(ui, panel, fonts, None, self.selection_subject(), actions);
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
