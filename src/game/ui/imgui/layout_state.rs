//! Where each panel goes: the layout's state, read back from ImGui each
//! frame, and `plan`, which docks the visible panels without overlap.

use super::*;

/// A panel's window flags: movable and resizable only while arranging (Ctrl
/// held), and titled then or when collapsed. Panels keep ImGui's saved
/// settings, so their docking comes back next session (`app.rs`).
pub(super) fn panel_chrome(arranging: bool, collapsed: bool) -> WindowFlags {
    if arranging {
        WindowFlags::empty()
    } else {
        let mut flags = WindowFlags::NO_RESIZE | WindowFlags::NO_MOVE;
        if !collapsed {
            flags |= WindowFlags::NO_TITLE_BAR;
        }
        flags
    }
}

#[derive(Clone, Copy, Default)]
pub(super) struct WindowGeometry {
    pub(super) pos: Vec2,
    pub(super) size: Vec2,
    pub(super) floating_size: Vec2,
    pub(super) floating_pos: Vec2,
    /// Placed by the player: the layout keeps it where it is.
    pub(super) manual: bool,
    /// Sized by the player: it keeps `floating_size` rather than the size
    /// its content measures. Set with `manual` when the player drags the
    /// panel; the two part only when one is reset (double-clicking the
    /// title bar or the resize grip, `reset_position`, `reset_size`).
    pub(super) sized: bool,
    pub(super) docked: bool,
    pub(super) collapsed: bool,
    pub(super) content_height: f32,
}

impl WindowGeometry {
    /// The player has moved or resized it: it stays as they left it.
    pub(super) fn place_by_player(&mut self) {
        self.manual = true;
        self.sized = true;
    }
}

/// ImGui windows follow collision-free docks until the player drags a title
/// bar or resize grip. Manual windows become obstacles for the remaining ones.
#[derive(Default)]
pub struct ImGuiLayoutState {
    pub(super) windows: [WindowGeometry; SLOT_COUNT],
    pub(super) active_view: ViewScope,
    pub(super) editing_outer: bool,
    pub(super) debug_layout_scope: Option<BoxScope>,
    pub(super) debug_view_geometry: std::collections::HashMap<ViewScope, WindowGeometry>,
    pub(super) debug_view_overrides: std::collections::HashSet<ViewScope>,
    pub(super) debug_outer_geometry: WindowGeometry,
    pub(super) debug_outer_start: Option<(Vec2, Vec2)>,
    pub(super) debug_view_boxes: std::collections::HashMap<ViewScope, u32>,
    pub(super) pending_debug_box: Option<u32>,
    pub(super) pending_view_reset: Option<ViewScope>,
    pub(super) selection_context: String,
    pub(super) selection_geometry_context: String,
    pub(super) selection_geometries: std::collections::HashMap<String, WindowGeometry>,
    pub(super) selection_geometry_changed: bool,
    pub(super) defer_geometry: bool,
    pub(super) dragging_last_frame: bool,
    pub(super) pair_visible: bool,
    pub(super) queue_dock_relation: Option<QueueDockRelation>,
    pub(super) queue_restore_attempts: u8,
    pub(super) pinned: Vec<PinnedPanel>,
    pub(super) pinned_geometry: std::collections::HashMap<PinnedPanel, WindowGeometry>,
    pub(super) pinned_box: std::collections::HashMap<PinnedPanel, u32>,
    pub(super) pinned_outside_frames: std::collections::HashMap<PinnedPanel, u8>,
    pub(super) pinned_groups: PinnedGroups,
    pub(super) pending_pin_dock: Vec<(PinnedPanel, u32)>,
    pub(super) outer_boxes: Vec<OuterBox>,
    pub(super) next_outer_box_id: u32,
    pub(super) last_selection_outer_box: Option<u32>,
    pub(super) last_queue_outer_box: Option<u32>,
    pub(super) inspector_snapshot: Option<PanelBuilder>,
    pub(super) debug_context: Option<SelectionKind>,
    pub(super) debug_context_relations: std::collections::HashMap<SelectionKind, QueueDockRelation>,
    pub(super) debug_restore_attempts: u8,
    pub(super) debug_attached: bool,
    /// Put each panel back where it floats as it's next drawn: the layout
    /// took it out of a dock, and for Debug, its view changed.
    pub(super) reposition: [bool; SLOT_COUNT],
    pub(super) debug_outer_relation: Option<(PinnedPanel, QueueDockRelation)>,
    /// Whether each panel was placed and sized by the player before the
    /// click on its title bar or grip that began this double-click, if any
    /// (`begin_frame`).
    pub(super) before_click: [Option<(bool, bool)>; SLOT_COUNT],
    /// The game's `generation` at the last frame (`game_changed`).
    pub(super) game_generation: Option<u32>,
    /// The id of the game window's dockspace (`draw_game_dockspace`).
    pub(super) game_dock_id: u32,
    /// How the panels are docked in the game window, by view (#333,
    /// #360). City / Building and Troop, until the player changes theirs,
    /// have none and follow Default's (`docking_for_view`); RESET gives
    /// the view Default's again.
    pub(super) view_docks: std::collections::HashMap<ViewScope, DockTree>,
    /// The view whose docking the game window has (`update_game_dock`).
    pub(super) dock_view: Option<ViewScope>,
    /// The game window's docking as last seen, once ImGui had done what the
    /// layout asked: a change from it is the player's.
    pub(super) dock_now: Option<DockTree>,
    /// Frames to let ImGui settle a docking the layout built before
    /// looking for the player's changes.
    pub(super) dock_settling: u8,
    /// Read from a layout saved before each view kept its docking: Debug's
    /// docking in the game window by view (`debug_edge` lines), to join
    /// with what ImGui's settings dock there once it has them.
    pub(super) legacy_docking: Option<std::collections::HashMap<ViewScope, EdgeDock>>,
    /// Each panel's geometry as it last floated, which it goes back to when
    /// the layout takes it out of the game window.
    pub(super) floating_before: [WindowGeometry; SLOT_COUNT],
}

impl ImGuiLayoutState {
    /// The player double-clicked `slot`'s title bar: it goes back where the
    /// current view's layout puts it, keeping its size. That is the
    /// automatic place, but for Debug in City / Building or Troop, where
    /// RESET would put it: Default's placement (`debug_default`).
    pub(super) fn reset_position(&mut self, slot: usize) {
        let inherited = self
            .debug_default(slot)
            .filter(|default| default.floating_pos != Vec2::ZERO);
        // Sized as before the double-click's first click, which made it the
        // player's (`begin_frame`).
        let sized = self.before_click[slot].map(|(_, sized)| sized);
        let window = &mut self.windows[slot];
        window.sized = sized.unwrap_or(window.sized);
        match inherited {
            Some(default) => {
                window.manual = true;
                window.pos = default.floating_pos;
                window.floating_pos = default.floating_pos;
                set_native_geometry(slot_titles()[slot], Some(default.floating_pos), None);
            }
            None => window.manual = false,
        }
        self.follow_default_debug(slot);
    }

    /// The player double-clicked `slot`'s resize grip: it goes back to the
    /// size the current view's layout gives it, staying where it is. That
    /// is the size its content measures, but for Debug in City / Building
    /// or Troop, the size RESET would give it: Default's (`debug_default`).
    pub(super) fn reset_size(&mut self, slot: usize) {
        let inherited = self
            .debug_default(slot)
            .filter(|default| default.floating_size != Vec2::ZERO);
        // Placed as before the double-click's first click (`begin_frame`).
        let manual = self.before_click[slot].map(|(manual, _)| manual);
        let window = &mut self.windows[slot];
        window.manual = manual.unwrap_or(window.manual);
        match inherited {
            Some(default) => {
                window.sized = true;
                window.size = default.floating_size;
                window.floating_size = default.floating_size;
                set_native_geometry(slot_titles()[slot], None, Some(default.floating_size));
            }
            None => window.sized = false,
        }
        self.follow_default_debug(slot);
    }

    /// Debug's placement in Default, when `slot` is Debug in City /
    /// Building or Troop: what that view's RESET restores
    /// (`apply_pending_view_reset`).
    fn debug_default(&self, slot: usize) -> Option<WindowGeometry> {
        match self.debug_layout_scope {
            Some(BoxScope::View(view)) if slot == DEBUG && view != ViewScope::Default => Some(
                self.debug_view_geometry
                    .get(&ViewScope::Default)
                    .copied()
                    .unwrap_or_default(),
            ),
            _ => None,
        }
    }

    /// After a reset of Debug in City / Building or Troop: once it's where
    /// and as big as Default has it, that view follows Default's Debug
    /// again, as after RESET.
    fn follow_default_debug(&mut self, slot: usize) {
        let (Some(default), Some(BoxScope::View(view))) =
            (self.debug_default(slot), self.debug_layout_scope)
        else {
            return;
        };
        let window = self.windows[DEBUG];
        let placed = |at: Vec2| at != Vec2::ZERO;
        let same_place = window.manual == placed(default.floating_pos)
            && (!window.manual || window.pos == default.floating_pos);
        let same_size = window.sized == placed(default.floating_size)
            && (!window.sized || window.floating_size == default.floating_size);
        // And docked in the game window as Default's is, or neither (#333).
        let same_edge = EdgeDock::matches(
            self.docking_for_view(view).edge_of(DEBUG),
            self.docking_for_view(ViewScope::Default).edge_of(DEBUG),
        );
        if same_place && same_size && same_edge {
            self.debug_view_overrides.remove(&view);
        }
    }
    /// After the panels are drawn: each shown floating, and not being
    /// dragged, keeps where it is to go back to (`floating_before`).
    pub(super) fn note_floating(&mut self, shown: &[Option<Vec2>; SLOT_COUNT]) {
        if self.defer_geometry {
            return;
        }
        for (slot, shown) in shown.iter().enumerate() {
            if shown.is_some() && !self.windows[slot].docked && !self.reposition[slot] {
                self.floating_before[slot] = self.windows[slot];
            }
        }
    }

    pub(super) fn sync_native_window(&mut self, slot: usize, title: &str) {
        let name = std::ffi::CString::new(title).expect("ImGui window title");
        let native = unsafe { ::imgui::sys::igFindWindowByName(name.as_ptr()) };
        if native.is_null() {
            return;
        }
        let native = unsafe { &*native };
        let docked = !native.DockNode.is_null();
        if self.windows[slot].docked && !docked {
            self.windows[slot].place_by_player();
        }
        self.windows[slot].docked = docked;
        self.windows[slot].collapsed = native.Collapsed;
        // Taken out of the game window by the layout (`update_game_dock`):
        // it keeps where it last floated until drawn there.
        let restoring = slot != DEBUG && self.reposition[slot] && !docked;
        if restoring {
            return;
        }
        self.windows[slot].pos = Vec2::new(native.Pos.x, native.Pos.y);
        if !native.Collapsed {
            self.windows[slot].size = Vec2::new(native.SizeFull.x, native.SizeFull.y);
            if slot == DEBUG && self.reposition[DEBUG] && !docked {
                let remembered = self.windows[slot].floating_pos;
                if remembered != Vec2::ZERO {
                    self.windows[slot].pos = remembered;
                    self.windows[slot].size = self.windows[slot].floating_size;
                    self.windows[slot].place_by_player();
                }
            } else if !(docked || (slot == DEBUG && self.defer_geometry)) {
                self.windows[slot].floating_size = self.windows[slot].size;
                self.windows[slot].floating_pos = self.windows[slot].pos;
            }
        }
    }

    pub(super) fn selection_changed(&mut self, context: &str) -> bool {
        let geometry_context = context.split('-').next().unwrap_or("");
        let geometry_changed = self.selection_geometry_context != geometry_context;
        self.selection_geometry_changed |= geometry_changed;
        if geometry_changed {
            if !self.selection_geometry_context.is_empty() {
                self.selection_geometries.insert(
                    self.selection_geometry_context.clone(),
                    self.windows[SELECTION],
                );
            }
            let docked = self.windows[SELECTION].docked;
            let mut restored = self
                .selection_geometries
                .get(geometry_context)
                .copied()
                .unwrap_or_default();
            // This is still one native ImGui window; docking belongs to that
            // window, while floating dimensions belong to the selected kind.
            if restored.docked && !docked {
                // ImGui may have removed the inactive split. Place this as a
                // normal floater for the restoration frame, not over Queue.
                restored.manual = false;
                restored.sized = false;
                restored.pos = Vec2::ZERO;
                restored.size = restored.floating_size;
            }
            restored.docked = docked;
            self.windows[SELECTION] = restored;
            self.selection_geometry_context = geometry_context.into();
        }
        if self.selection_context == context {
            false
        } else {
            self.selection_context = context.into();
            true
        }
    }

    pub(super) fn begin_frame(&mut self, ui: &Ui, arranging: bool) {
        let dragging = arranging && ui.io().mouse_down[0];
        self.defer_geometry = dragging || self.dragging_last_frame;
        self.dragging_last_frame = dragging;
        if !arranging || !ui.is_mouse_clicked(ImMouseButton::Left) {
            return;
        }
        // The second click of a double-click keeps what the first found:
        // how the panel was before it (`reset_position`, `reset_size`).
        let second = ui.is_mouse_double_clicked(ImMouseButton::Left);
        if !second {
            self.before_click = [None; SLOT_COUNT];
        }
        let mouse = Vec2::from_array(ui.io().mouse_pos);
        for slot in 0..SLOT_COUNT {
            let window = &mut self.windows[slot];
            if window.size == Vec2::ZERO || window.docked {
                continue;
            }
            let relative = mouse - window.pos;
            let title = relative.x >= 0.0
                && relative.x < window.size.x
                && relative.y >= 0.0
                && relative.y < 30.0;
            let grip = relative.x >= window.size.x - 20.0
                && relative.x <= window.size.x
                && relative.y >= window.size.y - 20.0
                && relative.y <= window.size.y;
            if title || grip {
                if !second {
                    self.before_click[slot] = Some((window.manual, window.sized));
                }
                window.place_by_player();
                if slot == DEBUG
                    && let Some(BoxScope::View(view)) = self.debug_layout_scope
                    && view != ViewScope::Default
                {
                    self.debug_view_overrides.insert(view);
                }
                break;
            }
        }
    }

    pub(super) fn size(&self, slot: usize, measured: Vec2, viewport: Vec2) -> Vec2 {
        let window = self.windows[slot];
        let preferred = if window.docked && window.size != Vec2::ZERO {
            window.size
        } else if window.sized {
            if window.floating_size != Vec2::ZERO {
                window.floating_size
            } else {
                window.size
            }
        } else if window.content_height > 0.0 {
            let observed = window.content_height.max(60.0);
            Vec2::new(
                measured.x,
                if slot == QUEUE {
                    observed.min(measured.y)
                } else {
                    observed
                },
            )
        } else {
            measured
        };
        let preferred = if window.collapsed {
            Vec2::new(preferred.x, COLLAPSED_HEIGHT)
        } else {
            preferred
        };
        preferred.min(Vec2::new(
            viewport.x - PANEL_MARGIN * 2.0,
            viewport.y - STATUS_HEIGHT - PANEL_MARGIN * 2.0,
        ))
    }

    pub(super) fn plan(
        &self,
        viewport: Vec2,
        sizes: &mut [Option<Vec2>; SLOT_COUNT],
    ) -> [Option<Vec2>; SLOT_COUNT] {
        let mut positions = [None; SLOT_COUNT];
        let mut dock = Dock::new(
            viewport,
            PANEL_MARGIN,
            PANEL_GAP,
            STATUS_HEIGHT + PANEL_MARGIN,
        );
        for geometry in self
            .pinned_geometry
            .iter()
            .filter(|(pin, _)| !self.pinned_box.contains_key(*pin) || self.pin_visible(**pin))
            .map(|(_, geometry)| geometry)
            .chain(
                self.outer_boxes
                    .iter()
                    .filter(|b| self.box_visible(b))
                    .map(|b| &b.geometry),
            )
        {
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
        for (slot, maybe_size) in sizes.iter().enumerate() {
            if let Some(size) = maybe_size
                && self.windows[slot].docked
            {
                let pos = self.windows[slot].pos;
                positions[slot] = Some(pos);
                dock.reserve(Rect {
                    min: Vec2::new(pos.x, viewport.y - pos.y - size.y),
                    max: Vec2::new(pos.x + size.x, viewport.y - pos.y),
                });
                continue;
            }
            if let Some(size) = maybe_size
                && self.windows[slot].manual
            {
                let max =
                    (viewport - *size - Vec2::splat(PANEL_MARGIN)).max(Vec2::splat(PANEL_MARGIN));
                let pos = self.windows[slot]
                    .pos
                    .clamp(Vec2::new(PANEL_MARGIN, STATUS_HEIGHT + PANEL_MARGIN), max);
                positions[slot] = Some(pos);
                dock.reserve(Rect {
                    min: Vec2::new(pos.x, viewport.y - pos.y - size.y),
                    max: Vec2::new(pos.x + size.x, viewport.y - pos.y),
                });
            }
        }
        for slot in PLAN_ORDER {
            if positions[slot].is_some() {
                continue;
            }
            let Some(size) = sizes[slot] else { continue };
            // The settings menu opens in the middle of the screen, over the
            // map, and takes no room from the docked panels.
            if slot == SETTINGS {
                let top = STATUS_HEIGHT + PANEL_MARGIN;
                positions[slot] = Some(
                    ((viewport - size) / 2.0)
                        .max(Vec2::new(PANEL_MARGIN, top))
                        .round(),
                );
                continue;
            }
            let zone = match slot {
                SELECTION | QUEUE | INSPECT => Zone::BottomLeft,
                UNITS => Zone::BottomCenter,
                _ => Zone::TopRight,
            };
            for height_scale in [1.0, 0.85, 0.65, 0.45, 0.25] {
                for width_scale in [1.0, 0.95, 0.85, 0.7, 0.55] {
                    let candidate = Vec2::new(
                        (size.x * width_scale).max(210.0),
                        (size.y * height_scale).max(80.0),
                    );
                    if let Some(rect) = dock.place(candidate, zone) {
                        sizes[slot] = Some(candidate);
                        positions[slot] = Some(Vec2::new(rect.min.x, viewport.y - rect.max.y));
                        break;
                    }
                }
                if positions[slot].is_some() {
                    break;
                }
            }
        }
        positions
    }

    pub(super) fn condition(&self, slot: usize, viewport: Vec2) -> Condition {
        let window = self.windows[slot];
        if !window.manual {
            return Condition::Always;
        }
        let max = viewport - window.size - Vec2::splat(PANEL_MARGIN);
        if window.pos.x < PANEL_MARGIN
            || window.pos.y < STATUS_HEIGHT + PANEL_MARGIN
            || window.pos.x > max.x
            || window.pos.y > max.y
        {
            Condition::Always
        } else {
            Condition::FirstUseEver
        }
    }

    pub(super) fn record(&mut self, slot: usize, ui: &Ui, arranging: bool) {
        let docked = unsafe { ::imgui::sys::igIsWindowDocked() };
        if self.windows[slot].docked && !docked {
            self.windows[slot].place_by_player();
        }
        self.windows[slot].pos = Vec2::from_array(ui.window_pos());
        self.windows[slot].size = Vec2::from_array(ui.window_size());
        self.windows[slot].docked = docked;
        if !docked && !(slot == DEBUG && (self.reposition[DEBUG] || self.defer_geometry)) {
            self.windows[slot].floating_size = self.windows[slot].size;
            self.windows[slot].floating_pos = self.windows[slot].pos;
        }
        // Cursor position includes the title bar when Ctrl shows it. Store
        // content height in the same coordinates in both modes so automatic
        // placement never changes its outer rectangle just for the chrome.
        self.windows[slot].content_height = panel_content_height(
            ui.cursor_pos()[1],
            ui.clone_style().window_padding[1],
            arranging.then(|| ui.frame_height()),
        );
    }
}

pub(super) fn panel_content_height(
    cursor_y: f32,
    padding_y: f32,
    title_height: Option<f32>,
) -> f32 {
    cursor_y + padding_y - title_height.unwrap_or(0.0)
}
