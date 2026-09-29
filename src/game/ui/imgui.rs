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
use super::text::{end_turn_label, fit_text};
use super::tooltips::Subject;
use super::*;
use crate::game::map_icons;
use crate::game::settings::{Control, Setting};

enum Action {
    Button(Option<PinnedPanel>, Target),
    /// A typed field's new text.
    Text(NetField, String),
    Reorder(Option<PinnedPanel>, QueueKind, usize, usize),
    CreateBox,
    ToggleLayoutLayer,
    RemoveOuterBox(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
enum ViewScope {
    #[default]
    Default,
    City,
    Troop,
}

impl ViewScope {
    fn label(self) -> &'static str {
        match self {
            Self::Default => "DEFAULT",
            Self::City => "CITY / BUILDING",
            Self::Troop => "TROOP",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoxScope {
    Outer,
    View(ViewScope),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PinnedKind {
    City,
    Barracks,
    Unit,
    Group,
    CityQueue,
    BarracksQueue,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct PinnedPanel {
    kind: PinnedKind,
    city_id: u32,
}

impl PinnedPanel {
    fn title(self) -> String {
        match self.kind {
            PinnedKind::City => format!("City {}###PinnedCity-{}", self.city_id + 1, self.city_id),
            PinnedKind::Barracks => format!(
                "Barracks {}###PinnedBarracks-{}",
                self.city_id + 1,
                self.city_id
            ),
            PinnedKind::Unit => format!("Unit {}###PinnedUnit-{}", self.city_id, self.city_id),
            PinnedKind::Group => format!("Group###PinnedGroup-{}", self.city_id),
            PinnedKind::CityQueue => format!(
                "City {} Queue###PinnedCityQueue-{}",
                self.city_id + 1,
                self.city_id
            ),
            PinnedKind::BarracksQueue => format!(
                "Barracks {} Queue###PinnedBarracksQueue-{}",
                self.city_id + 1,
                self.city_id
            ),
        }
    }
}

/// Captured group panels' members, by unit id, keyed by the group's pin id.
type PinnedGroups = std::collections::HashMap<u32, Vec<u32>>;

/// What a captured panel is for this frame, by index (`pin_focus`).
#[derive(Clone, Debug, PartialEq, Eq)]
enum PinFocus {
    /// A unit, or a group's members still alive.
    Units(Vec<usize>),
    City(usize),
    Barracks(usize),
}

#[derive(Clone, Copy)]
struct OuterBox {
    id: u32,
    dock_id: u32,
    scope: BoxScope,
    geometry: WindowGeometry,
}

impl OuterBox {
    fn title(self) -> String {
        let label = match self.scope {
            BoxScope::Outer => "OUTER",
            BoxScope::View(view) => view.label(),
        };
        format!("{label} BOX {}###OuterBox-{}", self.id, self.id)
    }
}

const PANEL_MARGIN: f32 = 14.0;
/// Room kept between the status bar's notice and End Turn after it.
const NOTICE_GAP: f32 = 16.0;
/// Where the tooltip holding a shortened notice in full wraps.
const NOTICE_TOOLTIP_WIDTH: f32 = 480.0;
const PANEL_GAP: f32 = 8.0;
const STATUS_HEIGHT: f32 = 52.0;
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
/// Each slot's native window title, by slot. A new panel that isn't static
/// chrome (like the status bar) gets a slot here, so it can be dragged,
/// docked, resized and put in a box like the rest.
const SLOT_TITLES: [&str; SLOT_COUNT] = [
    "Selection",
    "Production Queue",
    "Debug",
    "Inspect",
    "Units",
    "Settings",
];
/// The order `plan` places automatic panels in: earlier ones get the space
/// nearest their zone's corner. The settings menu isn't docked: it's centered.
const PLAN_ORDER: [usize; SLOT_COUNT] = [SELECTION, QUEUE, SETTINGS, DEBUG, INSPECT, UNITS];

#[derive(Clone, Copy)]
struct QueueDockRelation {
    // Queue's position relative to Selection in their shared dock node.
    direction: ::imgui::sys::ImGuiDir,
    fraction: f32,
    anchor_group: bool,
}

fn native_window(title: &str) -> *mut ::imgui::sys::ImGuiWindow {
    let name = std::ffi::CString::new(title).expect("ImGui window title");
    unsafe { ::imgui::sys::igFindWindowByName(name.as_ptr()) }
}

fn queue_dock_relation() -> Option<QueueDockRelation> {
    dock_relation("Selection", "Production Queue")
}

fn dock_relation(anchor: &str, other: &str) -> Option<QueueDockRelation> {
    let selection = native_window(anchor);
    let queue = native_window(other);
    if selection.is_null() || queue.is_null() {
        return None;
    }
    let (selection, queue) = unsafe { (&*selection, &*queue) };
    if !selection.Active
        || !queue.Active
        || selection.DockNode.is_null()
        || queue.DockNode.is_null()
    {
        return None;
    }
    if selection.DockNode == queue.DockNode {
        return Some(QueueDockRelation {
            direction: ::imgui::sys::ImGuiDir_None,
            fraction: 0.5,
            anchor_group: false,
        });
    }
    // Walk up from both leaves so this also recognizes a panel docked beside
    // an existing Selection+Queue group, rather than only direct siblings.
    let mut selection_branch = selection.DockNode;
    let mut queue_branch = queue.DockNode;
    unsafe {
        while !(*selection_branch).ParentNode.is_null()
            && !node_contains((*selection_branch).ParentNode, queue.DockNode)
        {
            selection_branch = (*selection_branch).ParentNode;
        }
        while !(*queue_branch).ParentNode.is_null()
            && !node_contains((*queue_branch).ParentNode, selection.DockNode)
        {
            queue_branch = (*queue_branch).ParentNode;
        }
    }
    let (selection_node, queue_node) = unsafe { (&*selection_branch, &*queue_branch) };
    if selection_node.ParentNode.is_null() || selection_node.ParentNode != queue_node.ParentNode {
        return None;
    }
    let horizontal = unsafe { (*selection_node.ParentNode).SplitAxis == ::imgui::sys::ImGuiAxis_X };
    let (direction, fraction) = if horizontal {
        (
            if queue_node.Pos.x > selection_node.Pos.x {
                ::imgui::sys::ImGuiDir_Right
            } else {
                ::imgui::sys::ImGuiDir_Left
            },
            queue_node.Size.x / (selection_node.Size.x + queue_node.Size.x).max(1.0),
        )
    } else {
        (
            if queue_node.Pos.y > selection_node.Pos.y {
                ::imgui::sys::ImGuiDir_Down
            } else {
                ::imgui::sys::ImGuiDir_Up
            },
            queue_node.Size.y / (selection_node.Size.y + queue_node.Size.y).max(1.0),
        )
    };
    Some(QueueDockRelation {
        direction,
        fraction: fraction.clamp(0.15, 0.85),
        anchor_group: selection_branch != selection.DockNode,
    })
}

fn shared_dock_node(
    anchor: *mut ::imgui::sys::ImGuiDockNode,
    other: *mut ::imgui::sys::ImGuiDockNode,
) -> *mut ::imgui::sys::ImGuiDockNode {
    let mut node = anchor;
    while !node.is_null() && unsafe { !node_contains(node, other) } {
        node = unsafe { (*node).ParentNode };
    }
    node
}

unsafe fn node_contains(
    ancestor: *mut ::imgui::sys::ImGuiDockNode,
    node: *mut ::imgui::sys::ImGuiDockNode,
) -> bool {
    let mut current = node;
    while !current.is_null() {
        if current == ancestor {
            return true;
        }
        current = unsafe { (*current).ParentNode };
    }
    false
}

fn opposite_dock_direction(direction: ::imgui::sys::ImGuiDir) -> ::imgui::sys::ImGuiDir {
    match direction {
        ::imgui::sys::ImGuiDir_Left => ::imgui::sys::ImGuiDir_Right,
        ::imgui::sys::ImGuiDir_Right => ::imgui::sys::ImGuiDir_Left,
        ::imgui::sys::ImGuiDir_Up => ::imgui::sys::ImGuiDir_Down,
        ::imgui::sys::ImGuiDir_Down => ::imgui::sys::ImGuiDir_Up,
        _ => ::imgui::sys::ImGuiDir_None,
    }
}

/// A panel's window flags: movable and resizable only while arranging (Ctrl
/// held), and titled then or when collapsed. Panels keep ImGui's saved
/// settings, so their docking comes back next session (`app.rs`).
fn panel_chrome(arranging: bool, collapsed: bool) -> WindowFlags {
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
struct WindowGeometry {
    pos: Vec2,
    size: Vec2,
    floating_size: Vec2,
    floating_pos: Vec2,
    manual: bool,
    docked: bool,
    collapsed: bool,
    content_height: f32,
}

/// ImGui windows follow collision-free docks until the player drags a title
/// bar or resize grip. Manual windows become obstacles for the remaining ones.
#[derive(Default)]
pub struct ImGuiLayoutState {
    windows: [WindowGeometry; SLOT_COUNT],
    active_view: ViewScope,
    editing_outer: bool,
    debug_layout_scope: Option<BoxScope>,
    debug_view_geometry: std::collections::HashMap<ViewScope, WindowGeometry>,
    debug_view_overrides: std::collections::HashSet<ViewScope>,
    debug_outer_geometry: WindowGeometry,
    debug_outer_start: Option<(Vec2, Vec2)>,
    debug_view_boxes: std::collections::HashMap<ViewScope, u32>,
    pending_debug_box: Option<u32>,
    pending_view_reset: Option<ViewScope>,
    selection_context: String,
    selection_geometry_context: String,
    selection_geometries: std::collections::HashMap<String, WindowGeometry>,
    selection_geometry_changed: bool,
    defer_geometry: bool,
    dragging_last_frame: bool,
    pair_visible: bool,
    queue_dock_relation: Option<QueueDockRelation>,
    queue_restore_attempts: u8,
    pinned: Vec<PinnedPanel>,
    pinned_geometry: std::collections::HashMap<PinnedPanel, WindowGeometry>,
    pinned_box: std::collections::HashMap<PinnedPanel, u32>,
    pinned_outside_frames: std::collections::HashMap<PinnedPanel, u8>,
    pinned_groups: PinnedGroups,
    pending_pin_dock: Vec<(PinnedPanel, u32)>,
    outer_boxes: Vec<OuterBox>,
    next_outer_box_id: u32,
    last_selection_outer_box: Option<u32>,
    last_queue_outer_box: Option<u32>,
    inspector_snapshot: Option<PanelBuilder>,
    debug_context: Option<String>,
    debug_context_relations: std::collections::HashMap<String, QueueDockRelation>,
    debug_restore_attempts: u8,
    debug_attached: bool,
    debug_reposition: bool,
    debug_outer_relation: Option<(PinnedPanel, QueueDockRelation)>,
    /// The game's `generation` at the last frame (`game_changed`).
    game_generation: Option<u32>,
}

impl ImGuiLayoutState {
    pub fn request_reset_active_view(&mut self) -> bool {
        if self.editing_outer || self.active_view == ViewScope::Default {
            return false;
        }
        self.pending_view_reset = Some(self.active_view);
        true
    }

    fn clear_view_debug_override(&mut self, view: ViewScope) {
        self.debug_view_overrides.remove(&view);
        self.debug_view_geometry.remove(&view);
        self.debug_view_boxes.remove(&view);
        let contexts: &[&str] = match view {
            ViewScope::Default => &[],
            ViewScope::City => &["city", "barracks"],
            ViewScope::Troop => &["unit", "group"],
        };
        for context in contexts {
            self.debug_context_relations.remove(*context);
        }
    }

    fn apply_pending_view_reset(&mut self) {
        let Some(view) = self.pending_view_reset.take() else {
            return;
        };
        self.clear_view_debug_override(view);
        if self.active_view != view || self.debug_layout_scope != Some(BoxScope::View(view)) {
            return;
        }
        let debug = native_window("Debug");
        if !debug.is_null() && unsafe { !(*debug).DockNode.is_null() } {
            unsafe {
                ::imgui::sys::igDockContextQueueUndockWindow(
                    ::imgui::sys::igGetCurrentContext(),
                    debug,
                )
            };
        }
        let mut inherited = self.debug_geometry_for_view(view);
        inherited.docked = false;
        inherited.pos = inherited.floating_pos;
        inherited.size = inherited.floating_size;
        self.windows[DEBUG] = inherited;
        self.pending_debug_box = None;
        self.debug_attached = false;
        self.debug_restore_attempts = 0;
        self.debug_reposition = true;
    }

    fn debug_geometry_for_view(&self, view: ViewScope) -> WindowGeometry {
        if view == ViewScope::Default || self.debug_view_overrides.contains(&view) {
            self.debug_view_geometry
                .get(&view)
                .copied()
                .unwrap_or_default()
        } else {
            self.debug_view_geometry
                .get(&ViewScope::Default)
                .copied()
                .or_else(|| self.debug_view_geometry.get(&view).copied())
                .unwrap_or_default()
        }
    }

    fn box_visible(&self, outer: &OuterBox) -> bool {
        outer.scope == BoxScope::Outer || outer.scope == BoxScope::View(self.active_view)
    }

    fn pin_visible(&self, pin: PinnedPanel) -> bool {
        self.pinned_box.get(&pin).is_some_and(|id| {
            self.outer_boxes
                .iter()
                .any(|outer| outer.id == *id && self.box_visible(outer))
        })
    }

    fn capture_panel(&mut self, pin: PinnedPanel, box_id: u32, title: &str) {
        if self.pinned.contains(&pin) {
            return;
        }
        self.pinned.push(pin);
        self.pinned_box.insert(pin, box_id);
        self.pinned_outside_frames.insert(pin, 0);
        self.pending_pin_dock.push((pin, box_id));
        let window = native_window(title);
        if !window.is_null() {
            unsafe {
                ::imgui::sys::igDockContextQueueUndockWindow(
                    ::imgui::sys::igGetCurrentContext(),
                    window,
                )
            };
        }
    }

    fn track_captured_panels(&mut self) {
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
                self.pinned_groups.remove(&pin.city_id);
            }
        }
    }

    fn remove_outer_box(&mut self, box_id: u32) {
        for title in SLOT_TITLES {
            if self.outer_box_for_window(title) == Some(box_id) {
                let window = native_window(title);
                if !window.is_null() {
                    unsafe {
                        ::imgui::sys::igDockContextQueueUndockWindow(
                            ::imgui::sys::igGetCurrentContext(),
                            window,
                        )
                    };
                }
                if title == "Debug" {
                    self.debug_reposition = true;
                }
                if title == "Inspect" {
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
            let window = native_window(&pin.title());
            if !window.is_null() {
                unsafe {
                    ::imgui::sys::igDockContextQueueUndockWindow(
                        ::imgui::sys::igGetCurrentContext(),
                        window,
                    )
                };
            }
            self.pinned.retain(|item| *item != pin);
            self.pinned_geometry.remove(&pin);
            self.pinned_box.remove(&pin);
            self.pinned_outside_frames.remove(&pin);
            if pin.kind == PinnedKind::Group {
                self.pinned_groups.remove(&pin.city_id);
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
    fn game_changed(&mut self, generation: u32) -> bool {
        self.game_generation
            .replace(generation)
            .is_some_and(|last| last != generation)
    }

    /// Releases every captured panel, keeping the boxes.
    fn clear_captured_panels(&mut self) {
        for pin in &self.pinned {
            let window = native_window(&pin.title());
            if !window.is_null() {
                unsafe {
                    ::imgui::sys::igDockContextQueueUndockWindow(
                        ::imgui::sys::igGetCurrentContext(),
                        window,
                    )
                };
            }
        }
        for title in ["Selection", "Production Queue"] {
            if self.outer_box_for_window(title).is_some() {
                let window = native_window(title);
                if !window.is_null() {
                    unsafe {
                        ::imgui::sys::igDockContextQueueUndockWindow(
                            ::imgui::sys::igGetCurrentContext(),
                            window,
                        )
                    };
                }
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

    fn create_box(&mut self, viewport: Vec2) {
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
                ..Default::default()
            },
        });
    }

    fn switch_layout_scope(&mut self, view: ViewScope) {
        let previous_view = self.active_view;
        let old_debug_box = self.outer_box_for_window("Debug");
        let old_scope = self.debug_layout_scope;
        let desired = if self.editing_outer
            || self
                .outer_box_for_window("Debug")
                .and_then(|id| self.outer_boxes.iter().find(|b| b.id == id))
                .is_some_and(|b| b.scope == BoxScope::Outer)
        {
            BoxScope::Outer
        } else {
            BoxScope::View(view)
        };
        self.active_view = view;
        if previous_view == view && old_scope == Some(BoxScope::View(view)) && !self.defer_geometry
        {
            if let Some(id) = old_debug_box {
                self.debug_view_boxes.insert(view, id);
            } else if self.pending_debug_box.is_none() {
                self.debug_view_boxes.remove(&view);
            }
        }
        if previous_view != view {
            for pin in self.pinned.clone() {
                if let Some(&box_id) = self.pinned_box.get(&pin)
                    && self.pin_visible(pin)
                    && !self.pending_pin_dock.iter().any(|(p, _)| *p == pin)
                {
                    self.pending_pin_dock.push((pin, box_id));
                }
            }
            self.debug_attached = false;
        }
        if old_scope == Some(desired) {
            return;
        }
        if let Some(scope) = old_scope {
            let old = self.windows[DEBUG];
            match scope {
                BoxScope::Outer => self.debug_outer_geometry = old,
                BoxScope::View(view) => {
                    self.debug_view_geometry.insert(view, old);
                    if let Some(id) = old_debug_box
                        && self
                            .outer_boxes
                            .iter()
                            .any(|b| b.id == id && b.scope == BoxScope::View(view))
                    {
                        self.debug_view_boxes.insert(view, id);
                    }
                }
            }
        }
        if let BoxScope::Outer = desired {
            if self.debug_outer_geometry.floating_pos == Vec2::ZERO {
                self.debug_outer_geometry = self.windows[DEBUG];
            }
            self.debug_outer_start = Some((
                self.debug_outer_geometry.floating_pos,
                self.debug_outer_geometry.floating_size,
            ));
        } else if old_scope == Some(BoxScope::Outer)
            && let Some((start_pos, start_size)) = self.debug_outer_start.take()
            && (self.debug_outer_geometry.floating_pos != start_pos
                || self.debug_outer_geometry.floating_size != start_size)
        {
            for view in [ViewScope::Default, ViewScope::City, ViewScope::Troop] {
                let mut geometry = self.debug_outer_geometry;
                geometry.docked = false;
                self.debug_view_geometry.insert(view, geometry);
                self.debug_view_boxes.remove(&view);
            }
            self.debug_view_overrides.clear();
        }
        let debug = native_window("Debug");
        if !debug.is_null() && unsafe { !(*debug).DockNode.is_null() } {
            let shared_outer = self.outer_boxes.iter().any(|b| {
                b.scope == BoxScope::Outer && Some(b.id) == self.outer_box_for_window("Debug")
            });
            if !shared_outer {
                unsafe {
                    ::imgui::sys::igDockContextQueueUndockWindow(
                        ::imgui::sys::igGetCurrentContext(),
                        debug,
                    )
                };
            }
        }
        let mut restored = match desired {
            BoxScope::Outer => self.debug_outer_geometry,
            BoxScope::View(view) => self.debug_geometry_for_view(view),
        };
        restored.docked = false;
        if restored.floating_pos != Vec2::ZERO {
            restored.pos = restored.floating_pos;
            restored.size = restored.floating_size;
            restored.manual = true;
        }
        self.windows[DEBUG] = restored;
        self.debug_reposition = true;
        self.debug_layout_scope = Some(desired);
        self.pending_debug_box = match desired {
            BoxScope::View(view) => self.debug_view_boxes.get(&view).copied(),
            BoxScope::Outer => None,
        };
    }

    fn outer_box_for_window(&self, title: &str) -> Option<u32> {
        let window = native_window(title);
        if window.is_null() {
            return None;
        }
        let node = unsafe { (*window).DockNode };
        if node.is_null() {
            return None;
        }
        self.outer_boxes
            .iter()
            .filter(|outer| self.box_visible(outer))
            .find_map(|outer| {
                if outer.dock_id == 0 {
                    return None;
                }
                let root = unsafe { ::imgui::sys::igDockBuilderGetNode(outer.dock_id) };
                (!root.is_null() && unsafe { node_contains(root, node) }).then_some(outer.id)
            })
    }

    fn release_debug_from_context(&mut self) {
        let debug = native_window("Debug");
        if debug.is_null() || unsafe { (*debug).DockNode.is_null() } {
            return;
        }
        if let Some((pin, relation)) = self.debug_outer_relation
            && self.pinned.contains(&pin)
        {
            let outer = native_window(&pin.title());
            if !outer.is_null() {
                unsafe {
                    ::imgui::sys::igDockContextQueueDock(
                        ::imgui::sys::igGetCurrentContext(),
                        outer,
                        (*outer).DockNode,
                        debug,
                        relation.direction,
                        relation.fraction,
                        false,
                    )
                };
                return;
            }
        }
        unsafe {
            ::imgui::sys::igDockContextQueueUndockWindow(::imgui::sys::igGetCurrentContext(), debug)
        };
        self.debug_reposition = true;
    }

    fn maintain_debug_scope(&mut self, context: Option<&str>) {
        if self.outer_box_for_window("Debug").is_some() {
            self.debug_context_relations.clear();
            self.debug_context = context.map(str::to_owned);
            self.debug_attached = false;
            self.debug_restore_attempts = 0;
            return;
        }
        if self.debug_context.as_deref() != context {
            if self.debug_attached {
                self.release_debug_from_context();
            }
            self.debug_attached = false;
            self.debug_context = context.map(str::to_owned);
            self.debug_restore_attempts = context
                .filter(|kind| self.debug_context_relations.contains_key(*kind))
                .map_or(0, |_| 2);
            return;
        }
        let Some(context) = context else {
            self.debug_outer_relation = self.pinned.iter().find_map(|pin| {
                dock_relation(&pin.title(), "Debug").map(|relation| (*pin, relation))
            });
            return;
        };
        if let Some(relation) = dock_relation("Selection", "Debug") {
            self.debug_context_relations
                .insert(context.into(), relation);
            self.debug_restore_attempts = 0;
            self.debug_attached = true;
            return;
        }
        if self.debug_restore_attempts > 0 {
            let selection = native_window("Selection");
            let debug = native_window("Debug");
            if !selection.is_null() && !debug.is_null() {
                let relation = self.debug_context_relations[context];
                unsafe {
                    let mut target_node = (*selection).DockNode;
                    if relation.anchor_group {
                        let queue = native_window("Production Queue");
                        if !queue.is_null() && !(*queue).DockNode.is_null() {
                            target_node = shared_dock_node(target_node, (*queue).DockNode);
                        }
                    }
                    ::imgui::sys::igDockContextQueueDock(
                        ::imgui::sys::igGetCurrentContext(),
                        selection,
                        target_node,
                        debug,
                        relation.direction,
                        relation.fraction,
                        false,
                    );
                }
                self.debug_restore_attempts -= 1;
            }
        } else if self.debug_attached {
            // An explicit undock while this menu is open replaces its remembered layout.
            self.debug_context_relations.remove(context);
            self.debug_attached = false;
        }
    }

    fn maintain_queue_dock(&mut self, pair_visible: bool) {
        if !pair_visible {
            self.pair_visible = false;
            return;
        }
        if !self.pair_visible && self.queue_dock_relation.is_some() {
            self.queue_restore_attempts = 2;
        }
        self.pair_visible = true;
        if let Some(relation) = queue_dock_relation() {
            self.queue_dock_relation = Some(relation);
            self.queue_restore_attempts = 0;
            return;
        }
        if self.queue_restore_attempts == 0 {
            if !self.defer_geometry {
                self.queue_dock_relation = None;
            }
            return;
        }
        let Some(relation) = self.queue_dock_relation else {
            return;
        };
        let selection = native_window("Selection");
        let queue = native_window("Production Queue");
        if selection.is_null() || queue.is_null() {
            return;
        }
        let queue_has_node = unsafe { !(*queue).DockNode.is_null() };
        let selection_has_node = unsafe { !(*selection).DockNode.is_null() };
        let (target, payload, direction, fraction) = if queue_has_node && !selection_has_node {
            (
                queue,
                selection,
                opposite_dock_direction(relation.direction),
                1.0 - relation.fraction,
            )
        } else {
            (selection, queue, relation.direction, relation.fraction)
        };
        // Use the same request queue as a successful ImGui drag/drop. This
        // restores a split after its windows have temporarily disappeared.
        unsafe {
            ::imgui::sys::igDockContextQueueDock(
                ::imgui::sys::igGetCurrentContext(),
                target,
                (*target).DockNode,
                payload,
                direction,
                fraction,
                false,
            );
        }
        self.queue_restore_attempts -= 1;
    }

    fn sync_native_window(&mut self, slot: usize, title: &str) {
        let name = std::ffi::CString::new(title).expect("ImGui window title");
        let native = unsafe { ::imgui::sys::igFindWindowByName(name.as_ptr()) };
        if native.is_null() {
            return;
        }
        let native = unsafe { &*native };
        let docked = !native.DockNode.is_null();
        if self.windows[slot].docked && !docked {
            self.windows[slot].manual = true;
        }
        self.windows[slot].docked = docked;
        self.windows[slot].collapsed = native.Collapsed;
        self.windows[slot].pos = Vec2::new(native.Pos.x, native.Pos.y);
        if !native.Collapsed {
            self.windows[slot].size = Vec2::new(native.SizeFull.x, native.SizeFull.y);
            if slot == DEBUG && self.debug_reposition && !docked {
                let remembered = self.windows[slot].floating_pos;
                if remembered != Vec2::ZERO {
                    self.windows[slot].pos = remembered;
                    self.windows[slot].size = self.windows[slot].floating_size;
                    self.windows[slot].manual = true;
                }
            } else if !(docked || (slot == DEBUG && self.defer_geometry)) {
                self.windows[slot].floating_size = self.windows[slot].size;
                self.windows[slot].floating_pos = self.windows[slot].pos;
            }
        }
    }

    fn selection_changed(&mut self, context: &str) -> bool {
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

    fn begin_frame(&mut self, ui: &Ui, arranging: bool) {
        let dragging = arranging && ui.io().mouse_down[0];
        self.defer_geometry = dragging || self.dragging_last_frame;
        self.dragging_last_frame = dragging;
        if !arranging || !ui.is_mouse_clicked(ImMouseButton::Left) {
            return;
        }
        let mouse = Vec2::from_array(ui.io().mouse_pos);
        for (slot, window) in self.windows.iter_mut().enumerate() {
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
                window.manual = true;
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

    fn size(&self, slot: usize, measured: Vec2, viewport: Vec2) -> Vec2 {
        let window = self.windows[slot];
        let preferred = if window.docked && window.size != Vec2::ZERO {
            window.size
        } else if window.manual {
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

    fn plan(
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

    fn condition(&self, slot: usize, viewport: Vec2) -> Condition {
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

    fn record(&mut self, slot: usize, ui: &Ui, arranging: bool) {
        let docked = unsafe { ::imgui::sys::igIsWindowDocked() };
        if self.windows[slot].docked && !docked {
            self.windows[slot].manual = true;
        }
        self.windows[slot].pos = Vec2::from_array(ui.window_pos());
        self.windows[slot].size = Vec2::from_array(ui.window_size());
        self.windows[slot].docked = docked;
        if !docked && !(slot == DEBUG && (self.debug_reposition || self.defer_geometry)) {
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

/// Saving the layout between sessions (`persist.rs`): where the player put
/// each panel and box, as lines of text. ImGui keeps its own half, which
/// panels are docked where, in `imgui.ini`; the two are saved together.
impl ImGuiLayoutState {
    /// The layout as text: a line per panel, per box, and per saved
    /// placement of the Debug panel and the Selection panel.
    pub fn to_text(&self) -> String {
        let mut lines = Vec::new();
        for (slot, window) in self.windows.iter().enumerate() {
            lines.push(format!("window {slot} {}", geometry_text(window)));
        }
        for outer in &self.outer_boxes {
            lines.push(format!(
                "box {} {} {}",
                outer.id,
                scope_text(outer.scope),
                geometry_text(&outer.geometry)
            ));
        }
        lines.push(format!("next_box {}", self.next_outer_box_id));
        for (key, id) in [
            ("selection_box", self.last_selection_outer_box),
            ("queue_box", self.last_queue_outer_box),
        ] {
            if let Some(id) = id {
                lines.push(format!("{key} {id}"));
            }
        }
        if self.debug_outer_geometry.manual || self.debug_outer_geometry.docked {
            lines.push(format!(
                "debug_outer {}",
                geometry_text(&self.debug_outer_geometry)
            ));
        }
        for view in [ViewScope::Default, ViewScope::City, ViewScope::Troop] {
            let name = view_text(view);
            // Only a placement the player chose: one the layout made is made
            // again, and restoring it would pin the panel (`switch_layout_scope`).
            if let Some(geometry) = self.debug_view_geometry.get(&view)
                && (geometry.manual || geometry.docked)
            {
                lines.push(format!("debug_view {name} {}", geometry_text(geometry)));
            }
            if self.debug_view_overrides.contains(&view) {
                lines.push(format!("debug_override {name}"));
            }
            if let Some(id) = self.debug_view_boxes.get(&view) {
                lines.push(format!("debug_box {name} {id}"));
            }
        }
        let mut contexts: Vec<_> = self.selection_geometries.iter().collect();
        contexts.sort_by(|a, b| a.0.cmp(b.0));
        for (context, geometry) in contexts {
            if !context.is_empty() && !context.contains(char::is_whitespace) {
                lines.push(format!("selection {context} {}", geometry_text(geometry)));
            }
        }
        lines.iter().map(|line| format!("{line}\n")).collect()
    }

    /// A layout read back from `to_text`'s text. Lines it doesn't understand
    /// are skipped, so an old or damaged file still gives a usable layout.
    pub fn from_text(text: &str) -> Self {
        let mut layout = Self::default();
        for (key, values) in text.lines().filter_map(crate::persist::key_and_values) {
            let number = |i: usize| values.get(i).and_then(|v| v.parse::<u32>().ok());
            let geometry_from = |i: usize| values.get(i..).and_then(geometry_from_text);
            match key {
                "window" => {
                    if let (Some(slot), Some(geometry)) = (number(0), geometry_from(1))
                        && (slot as usize) < SLOT_COUNT
                    {
                        layout.windows[slot as usize] = geometry;
                    }
                }
                "box" => {
                    if let (Some(id), Some(scope), Some(geometry)) = (
                        number(0),
                        values.get(1).and_then(|v| scope_from_text(v)),
                        geometry_from(2),
                    ) {
                        layout.outer_boxes.push(OuterBox {
                            id,
                            dock_id: 0,
                            scope,
                            geometry,
                        });
                    }
                }
                "next_box" => layout.next_outer_box_id = number(0).unwrap_or(0),
                "selection_box" => layout.last_selection_outer_box = number(0),
                "queue_box" => layout.last_queue_outer_box = number(0),
                "debug_outer" => {
                    if let Some(geometry) = geometry_from(0) {
                        layout.debug_outer_geometry = geometry;
                    }
                }
                "debug_view" | "debug_override" | "debug_box" => {
                    let Some(view) = values.first().and_then(|v| view_from_text(v)) else {
                        continue;
                    };
                    match key {
                        "debug_view" => {
                            if let Some(geometry) = geometry_from(1) {
                                layout.debug_view_geometry.insert(view, geometry);
                            }
                        }
                        "debug_override" => {
                            layout.debug_view_overrides.insert(view);
                        }
                        _ => {
                            if let Some(id) = number(1) {
                                layout.debug_view_boxes.insert(view, id);
                            }
                        }
                    }
                }
                "selection" => {
                    if let (Some(context), Some(geometry)) = (values.first(), geometry_from(1)) {
                        layout
                            .selection_geometries
                            .insert((*context).to_string(), geometry);
                    }
                }
                _ => {}
            }
        }
        // Box ids come from the counter: never hand out one already used.
        let highest = layout
            .outer_boxes
            .iter()
            .map(|b| b.id + 1)
            .max()
            .unwrap_or(0);
        layout.next_outer_box_id = layout.next_outer_box_id.max(highest);
        layout
    }
}

/// A panel's saved geometry: position, size, floating position and size,
/// and whether it's placed by the player, docked and collapsed.
fn geometry_text(geometry: &WindowGeometry) -> String {
    let g = geometry;
    format!(
        "{} {} {} {} {} {} {} {} {} {} {}",
        g.pos.x,
        g.pos.y,
        g.size.x,
        g.size.y,
        g.floating_pos.x,
        g.floating_pos.y,
        g.floating_size.x,
        g.floating_size.y,
        u8::from(g.manual),
        u8::from(g.docked),
        u8::from(g.collapsed)
    )
}

fn geometry_from_text(values: &[&str]) -> Option<WindowGeometry> {
    let numbers: Vec<f32> = values
        .iter()
        .take(11)
        .map(|v| v.parse::<f32>().ok().filter(|n| n.is_finite()))
        .collect::<Option<_>>()?;
    let [
        px,
        py,
        sx,
        sy,
        fpx,
        fpy,
        fsx,
        fsy,
        manual,
        docked,
        collapsed,
    ] = numbers[..]
    else {
        return None;
    };
    Some(WindowGeometry {
        pos: Vec2::new(px, py),
        size: Vec2::new(sx, sy),
        floating_pos: Vec2::new(fpx, fpy),
        floating_size: Vec2::new(fsx, fsy),
        manual: manual != 0.0,
        docked: docked != 0.0,
        collapsed: collapsed != 0.0,
        ..WindowGeometry::default()
    })
}

fn view_text(view: ViewScope) -> &'static str {
    match view {
        ViewScope::Default => "default",
        ViewScope::City => "city",
        ViewScope::Troop => "troop",
    }
}

fn view_from_text(text: &str) -> Option<ViewScope> {
    [ViewScope::Default, ViewScope::City, ViewScope::Troop]
        .into_iter()
        .find(|&view| view_text(view) == text)
}

fn scope_text(scope: BoxScope) -> &'static str {
    match scope {
        BoxScope::Outer => "outer",
        BoxScope::View(view) => view_text(view),
    }
}

fn scope_from_text(text: &str) -> Option<BoxScope> {
    if text == "outer" {
        Some(BoxScope::Outer)
    } else {
        view_from_text(text).map(BoxScope::View)
    }
}

fn panel_content_height(cursor_y: f32, padding_y: f32, title_height: Option<f32>) -> f32 {
    cursor_y + padding_y - title_height.unwrap_or(0.0)
}

fn measure_panel(ui: &Ui, panel: &PanelBuilder, fonts: &[FontId; 3], width: f32) -> f32 {
    let inner = (width - 45.0).max(100.0);
    let mut height = 26.0; // border and padding
    for row in flat_rows(&panel.rows) {
        height += match row {
            Row::Text(px, line) => {
                let font = match *px {
                    TITLE => fonts[2],
                    SMALL => fonts[0],
                    _ => fonts[1],
                };
                let _font = ui.push_font(font);
                let text = line.iter().map(|(s, _)| s.as_str()).collect::<String>();
                let measured = ui.calc_text_size(text);
                let lines = if line.len() == 1 {
                    (measured[0] / inner).ceil().max(1.0)
                } else {
                    1.0
                };
                measured[1] * lines + 7.0
            }
            Row::Gap(gap) => gap + 6.0,
            Row::Bar(_) => 18.0,
            Row::Buttons(buttons, compact) => measure_buttons(buttons, *compact, inner),
            Row::Reorder(_, buttons) => measure_buttons(buttons, true, inner),
            Row::QueueItem(_) => 37.0,
            Row::TitleWithButton(..) => ui.frame_height_with_spacing(),
            Row::BuildingCatalog(_, buttons, ..) => {
                (buttons.len().clamp(1, 5) as f32 * 34.0) + 18.0
            }
            Row::Roster(_) => ROSTER_CHIP + 6.0,
            Row::ScrollList(_) => unreachable!("flattened by flat_rows"),
            Row::LabeledButtons(..) => unreachable!("classic only"),
            Row::Heading(_) => {
                let _font = ui.push_font(fonts[0]);
                // The text, the rule under it, and the spacing after each.
                ui.calc_text_size("A")[1] + 2.0 * SPACING_Y + 1.0
            }
            Row::Setting(..) | Row::Field(..) => {
                let _font = ui.push_font(fonts[1]);
                ui.calc_text_size("A")[1] + 2.0 * FRAME_PADDING_Y + SPACING_Y
            }
        };
    }
    height + 12.0
}

/// A row of buttons' height, as `render_buttons` wraps it in `inner` of
/// width.
fn measure_buttons(buttons: &[ButtonSpec], compact: bool, inner: f32) -> f32 {
    if icon_row(buttons) {
        let columns = ((inner + 7.0) / (ICON_BUTTON_SIZE + 7.0)).floor().max(1.0) as usize;
        buttons.len().div_ceil(columns) as f32 * (ICON_BUTTON_SIZE + 6.0)
    } else {
        let columns = ((inner + 7.0) / 135.0).floor().max(1.0) as usize;
        let rows = buttons.len().div_ceil(columns);
        rows as f32 * (if compact { 34.0 } else { 54.0 })
    }
}

/// The style's vertical item spacing and frame padding (`app.rs`), as
/// `measure_panel` counts them for the settings menu's rows.
const SPACING_Y: f32 = 6.0;
const FRAME_PADDING_Y: f32 = 6.0;

/// The width of the settings menu's label column: its longest setting name,
/// and a gap before the controls.
fn setting_label_width(ui: &Ui, panel: &PanelBuilder, font: FontId) -> f32 {
    let _font = ui.push_font(font);
    panel
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Setting(setting, _) => Some(ui.calc_text_size(setting.name())[0]),
            Row::Field(field, ..) => Some(ui.calc_text_size(field.name())[0]),
            _ => None,
        })
        .fold(0.0, f32::max)
        + 18.0
}

/// A setting's name and what it does, for the tooltip on its label and
/// control.
fn setting_tooltip(ui: &Ui, setting: Setting) {
    ui.tooltip(|| {
        ui.text_colored(TEXT, setting.name());
        for line in super::text::wrap(setting.description(), TOOLTIP_WRAP) {
            ui.text_colored([0.72, 0.75, 0.76, 1.0], line);
        }
    });
}

/// A setting's row in the settings menu: its name in the label column, then
/// the control `Setting::control` names across the rest of the width. A
/// change is a `Target::SetSetting` action.
fn render_setting(
    ui: &Ui,
    setting: Setting,
    value: i32,
    label_width: f32,
    scope: Option<PinnedPanel>,
    actions: &mut Vec<Action>,
) {
    let range = setting.range();
    let mut set = |to: i32| {
        if to != value {
            actions.push(Action::Button(scope, Target::SetSetting(setting, to)));
        }
    };
    let tooltip = || {
        if ui.is_item_hovered() {
            setting_tooltip(ui, setting);
        }
    };
    let left = ui.cursor_pos()[0];
    ui.align_text_to_frame_padding();
    ui.text_colored([0.82, 0.84, 0.86, 1.0], setting.name());
    tooltip();
    ui.same_line_with_pos(left + label_width);
    let width = ui.content_region_avail()[0];
    let id = format!("##setting-{setting:?}");
    match setting.control() {
        Control::Toggle => {
            let mut on = value == 1;
            if ui.checkbox(format!("{}{id}", setting.value_text(value)), &mut on) {
                set(on as i32);
            }
            tooltip();
        }
        Control::Slider => {
            let mut to = value;
            ui.set_next_item_width(width);
            // ImGui formats the value itself; the text has no %d, so it
            // shows `value_text` as it is.
            let shown = setting.value_text(value).replace('%', "%%");
            if ui
                .slider_config(&id, *range.start(), *range.end())
                .display_format(shown)
                .flags(SliderFlags::ALWAYS_CLAMP | SliderFlags::NO_INPUT)
                .build(&mut to)
            {
                set(to);
            }
            tooltip();
        }
        Control::Choice if range.clone().count() > Control::MAX_BUTTONS => {
            ui.set_next_item_width(width);
            match ui.begin_combo(&id, setting.value_text(value)) {
                Some(_combo) => {
                    for to in range {
                        let label = format!("{}{id}-{to}", setting.value_text(to));
                        if ui.selectable_config(label).selected(to == value).build() {
                            set(to);
                        }
                    }
                }
                None => tooltip(),
            }
        }
        Control::Choice => {
            let count = range.clone().count() as f32;
            let spacing = ui.clone_style().item_spacing[0];
            let each = ((width - spacing * (count - 1.0)) / count).floor();
            for (index, to) in range.enumerate() {
                if index != 0 {
                    ui.same_line();
                }
                // The current choice is gold, like a queued build.
                let _accent = (to == value).then(|| {
                    (
                        ui.push_style_color(StyleColor::Button, [0.34, 0.30, 0.17, 1.0]),
                        ui.push_style_color(StyleColor::ButtonHovered, [0.42, 0.37, 0.20, 1.0]),
                    )
                });
                let label = format!("{}{id}-{to}", setting.value_text(to));
                if ui.button_with_size(label, [each, 0.0]) {
                    set(to);
                }
                tooltip();
            }
        }
    }
}

/// A typed field's row (the Multiplayer section): its name in the label
/// column, then a text box across the rest; an edit is an `Action::Text`.
fn render_field(ui: &Ui, field: NetField, text: &str, label_width: f32, actions: &mut Vec<Action>) {
    let left = ui.cursor_pos()[0];
    ui.align_text_to_frame_padding();
    ui.text_colored([0.82, 0.84, 0.86, 1.0], field.name());
    ui.same_line_with_pos(left + label_width);
    ui.set_next_item_width(ui.content_region_avail()[0]);
    let mut edited = text.to_string();
    let flags = match field {
        NetField::Port => InputTextFlags::CHARS_DECIMAL,
        NetField::Code => InputTextFlags::CHARS_UPPERCASE | InputTextFlags::CHARS_NO_BLANK,
        NetField::Address => InputTextFlags::CHARS_NO_BLANK,
    };
    // `flags` replaces imgui-rs's own, so it keeps the one that lets the
    // text outgrow the string it started as (without it, an empty box took
    // seven characters). Ctrl+C, X, V and A work through the clipboard
    // `App` gives the context; the box keeps what's typed or pasted as the
    // field does.
    let changed = ui
        .input_text(format!("##field-{field:?}"), &mut edited)
        .flags(flags | InputTextFlags::CALLBACK_RESIZE)
        .callback(InputTextCallback::EDIT, CleanField(field))
        .build();
    // The box is where classic's button to type into the field would be.
    note_drawn_button(ui, Target::EditNetField(field));
    if changed && edited != text {
        actions.push(Action::Text(field, edited));
    }
}

/// Keeps a field's text box to what the field takes (`NetField::clean`)
/// while it's being edited, so a long paste shows cut to the field's length
/// rather than until the box lets go.
struct CleanField(NetField);

impl InputTextCallbackHandler for CleanField {
    fn on_edit(&mut self, mut data: TextCallbackData) {
        let clean = self.0.clean(data.str());
        if clean != data.str() {
            data.clear();
            data.push_str(&clean);
        }
    }
}

/// How wide the unit strip's window wants to be: its widest row of tokens,
/// plus the same allowance for padding and border `measure_panel` takes off.
fn roster_width(panel: &PanelBuilder) -> f32 {
    let widest = panel
        .rows
        .iter()
        .filter_map(|row| match row {
            Row::Roster(chips) => Some(chips.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0) as f32;
    (widest * ROSTER_CHIP + (widest - 1.0).max(0.0) * ROSTER_CHIP_GAP + 45.0).max(210.0)
}

/// A unit strip token drawn with ImGui's draw list: the same token as on the
/// map, in a framed square (bright while selected, white while hovered).
fn draw_roster_chip(ui: &Ui, min: [f32; 2], max: [f32; 2], chip: &RosterChip, hovered: bool) {
    let draw = ui.get_window_draw_list();
    let (bg, border) = if chip.selected {
        (BUTTON_HOVER_BG, ARMED_BORDER_COLOR)
    } else {
        (BUTTON_BG, BORDER_COLOR)
    };
    let edge = if hovered { TEXT } else { border };
    let thickness = if chip.selected { ARMED_BORDER } else { BORDER };
    draw.add_rect(min, max, bg).filled(true).build();
    draw.add_rect(min, max, edge).thickness(thickness).build();
    let mut vertices = Vec::new();
    let radius = (max[0] - min[0]) * ROSTER_TOKEN_SHARE / 2.0;
    super::paint::push_chip_icon(Vec2::ZERO, chip.icon, radius, chip.color, &mut vertices);
    // The token is built Y-up around the origin; ImGui's Y points down.
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let at = |v: &Vertex| [center[0] + v.pos[0], center[1] - v.pos[1]];
    for triangle in vertices.as_chunks::<3>().0 {
        draw.add_triangle(
            at(&triangle[0]),
            at(&triangle[1]),
            at(&triangle[2]),
            triangle[0].color,
        )
        .filled(true)
        .build();
    }
    if chip.count > 1 {
        let text = chip.count.to_string();
        let size = ui.calc_text_size(&text);
        let inset = super::paint::CHIP_COUNT_INSET;
        let origin = [max[0] - size[0] - inset, max[1] - size[1] - inset];
        draw.add_rect(
            [origin[0] - 2.0, origin[1] - 1.0],
            [max[0] - inset + 2.0, max[1] - inset + 1.0],
            PANEL_BG,
        )
        .filled(true)
        .build();
        draw.add_text(origin, TEXT, &text);
    }
}

fn draw_action_icon(
    ui: &Ui,
    min: [f32; 2],
    max: [f32; 2],
    icon: action_icons::ActionIcon,
    color: Color,
    cooldown: Option<&str>,
    small_font: FontId,
) {
    let draw = ui.get_window_draw_list();
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let mut vertices = Vec::new();
    action_icons::push_icon(Vec2::ZERO, 14.0, icon, color, &mut vertices);
    fill_shapes(&draw, center, &vertices);
    if let Some(turns) = cooldown {
        let _font = ui.push_font(small_font);
        let width = ui.calc_text_size(turns)[0];
        let pos = [max[0] - width - 3.0, min[1] + 2.0];
        draw.add_rect(
            [pos[0] - 2.0, pos[1] - 1.0],
            [max[0] - 1.0, pos[1] + 13.0],
            PANEL_BG,
        )
        .filled(true)
        .build();
        draw.add_text(pos, color, turns);
    }
}

fn draw_production_icon(
    ui: &Ui,
    min: [f32; 2],
    max: [f32; 2],
    icon: crate::game::unit_icons::UnitIcon,
) {
    let center = [min[0] + 19.0, (min[1] + max[1]) / 2.0];
    let mut vertices = Vec::new();
    crate::game::unit_icons::push_pictogram(Vec2::ZERO, 12.0, icon, TEXT, &mut vertices);
    fill_shapes(&ui.get_window_draw_list(), center, &vertices);
}

/// Fills shapes built Y-up around the origin (triangle lists, like the
/// map's) into `draw`, the current window's draw list (ImGui allows one
/// handle on it at a time), centered on `center` (ImGui's Y
/// points down). ImGui feathers every filled triangle's edges by a pixel,
/// the inner ones too, which leaves a small silhouette fuzzy and seamed:
/// these are drawn unfeathered, crisp and whole.
fn fill_shapes(draw: &::imgui::DrawListMut, center: [f32; 2], vertices: &[Vertex]) {
    let at = |v: &Vertex| [center[0] + v.pos[0], center[1] - v.pos[1]];
    let list = unsafe { ::imgui::sys::igGetWindowDrawList() };
    let flags = unsafe { (*list).Flags };
    let feathered = ::imgui::sys::ImDrawListFlags_AntiAliasedFill as ::imgui::sys::ImDrawListFlags;
    unsafe { (*list).Flags = flags & !feathered };
    for triangle in vertices.as_chunks::<3>().0 {
        draw.add_triangle(
            at(&triangle[0]),
            at(&triangle[1]),
            at(&triangle[2]),
            triangle[0].color,
        )
        .filled(true)
        .build();
    }
    unsafe { (*list).Flags = flags };
}

fn text_line(ui: &Ui, line: &Line) {
    for (index, (text, color)) in line.iter().enumerate() {
        if index != 0 {
            ui.same_line_with_spacing(0.0, 0.0);
        }
        let readable = if *color == LABEL_TEXT {
            [0.66, 0.70, 0.72, 1.0]
        } else if *color == DIM_TEXT {
            [0.72, 0.75, 0.76, 1.0]
        } else {
            *color
        };
        rich_text(ui, text, readable);
    }
}

/// Whether `text` holds any inline icon characters (`map_icons::inline_icon`).
fn has_icons(text: &str) -> bool {
    text.chars().any(|ch| map_icons::inline_icon(ch).is_some())
}

/// The room one inline icon takes, at the current font.
fn icon_box(ui: &Ui) -> f32 {
    ui.current_font_size() * 1.1
}

/// How wide `text` draws with its icons.
fn rich_width(ui: &Ui, text: &str) -> f32 {
    let mut width = 0.0;
    let mut run = String::new();
    for ch in text.chars() {
        if map_icons::inline_icon(ch).is_some() {
            width += ui.calc_text_size(&run)[0] + icon_box(ui);
            run.clear();
        } else {
            run.push(ch);
        }
    }
    width + ui.calc_text_size(&run)[0]
}

/// Draws `text` into the window's draw list with its top-left at `pos`, each
/// icon character as its icon, the same pictures as the map's yield chips.
/// A dimmed line's icons fade with it.
fn draw_rich(ui: &Ui, pos: [f32; 2], text: &str, color: [f32; 4], dim: bool) {
    let draw = ui.get_window_draw_list();
    let size = icon_box(ui);
    let middle = pos[1] + ui.current_font_size() / 2.0;
    let mut x = pos[0];
    let mut run = String::new();
    let flush = |run: &mut String, x: &mut f32| {
        if !run.is_empty() {
            draw.add_text([*x, pos[1]], color, run.as_str());
            *x += ui.calc_text_size(run.as_str())[0];
            run.clear();
        }
    };
    for ch in text.chars() {
        if map_icons::inline_icon(ch).is_none() {
            run.push(ch);
            continue;
        }
        flush(&mut run, &mut x);
        let mut vertices = Vec::new();
        map_icons::push_inline_icon(Vec2::ZERO, size * 0.85, ch, false, &mut vertices);
        let center = [x + size / 2.0, middle];
        // The icon is built Y-up around the origin; ImGui's Y points down.
        if dim {
            for vertex in &mut vertices {
                vertex.color[3] *= 0.4;
            }
        }
        fill_shapes(&draw, center, &vertices);
        x += size;
    }
    flush(&mut run, &mut x);
}

/// `text` as a widget, with its icons drawn in: plain text when it has none.
fn rich_text(ui: &Ui, text: &str, color: [f32; 4]) {
    if !has_icons(text) {
        ui.text_colored(color, text);
        return;
    }
    let pos = ui.cursor_screen_pos();
    ui.dummy([rich_width(ui, text), ui.text_line_height()]);
    draw_rich(ui, pos, text, color, false);
}

/// Tests only: `imgui` allows one context at a time in the whole process,
/// so a test holds this while it has one.
#[cfg(test)]
pub(super) fn one_context_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A test that failed holding it leaves nothing behind to protect.
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Tests only: a button ImGui drew, and its screen rectangle's corners.
#[cfg(test)]
pub(super) type DrawnButton = (Target, [f32; 2], [f32; 2]);

#[cfg(test)]
thread_local! {
    /// Tests only: each panel button ImGui drew this thread, so a test can
    /// move the mouse onto one and click.
    pub(super) static DRAWN_BUTTONS: std::cell::RefCell<Vec<DrawnButton>> =
        const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
thread_local! {
    /// Tests only: the text of each panel button tooltip ImGui showed this
    /// thread, a line per string.
    pub(super) static SHOWN_TOOLTIPS: std::cell::RefCell<Vec<Vec<String>>> =
        const { std::cell::RefCell::new(Vec::new()) };
    /// Tests only: the status bar's notice as ImGui last drew it, its
    /// rectangle, and how far right it may reach (short of End Turn).
    pub(super) static SHOWN_NOTICE: std::cell::RefCell<ShownNotice> =
        const { std::cell::RefCell::new((String::new(), [0.0; 2], [0.0; 2], 0.0)) };
    /// Tests only: the status bar's second line as ImGui last drew it:
    /// each control's text and rectangle, from Menu on.
    pub(super) static STATUS_CONTROLS: std::cell::RefCell<Vec<DrawnControl>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Tests only: a notice drawn, its rectangle and its limit.
#[cfg(test)]
pub(super) type ShownNotice = (String, [f32; 2], [f32; 2], f32);
/// Tests only: a status bar control's text and rectangle.
#[cfg(test)]
pub(super) type DrawnControl = (String, [f32; 2], [f32; 2]);

/// Tests only: notes the tooltip about to show.
fn note_shown_tooltip(_lines: &[(u32, Line)]) {
    #[cfg(test)]
    SHOWN_TOOLTIPS.with_borrow_mut(|shown| {
        shown.push(
            _lines
                .iter()
                .map(|(_, line)| line.iter().map(|(text, _)| text.as_str()).collect())
                .collect(),
        )
    });
}

/// Tests only: notes the notice just drawn.
fn note_shown_notice(_notice: &str, _min: [f32; 2], _max: [f32; 2], _limit: f32) {
    #[cfg(test)]
    SHOWN_NOTICE.set((_notice.to_string(), _min, _max, _limit));
}

/// Tests only: notes the status bar control just drawn; Menu, the first,
/// starts the list again.
fn note_status_control(_ui: &Ui, _text: &str) {
    #[cfg(test)]
    STATUS_CONTROLS.with_borrow_mut(|drawn| {
        if _text == "MENU" {
            drawn.clear();
        }
        drawn.push((_text.to_string(), _ui.item_rect_min(), _ui.item_rect_max()));
    });
}

/// Tests only: notes where the button for `target` just went.
fn note_drawn_button(_ui: &Ui, _target: Target) {
    #[cfg(test)]
    DRAWN_BUTTONS
        .with_borrow_mut(|drawn| drawn.push((_target, _ui.item_rect_min(), _ui.item_rect_max())));
}

/// A button whose lines may hold icons: ImGui's own button when they
/// don't; otherwise a blank button with the lines drawn over it, centered,
/// or from the left with `left` set.
fn rich_button(ui: &Ui, id: &str, lines: &[String], size: [f32; 2], left: bool) -> bool {
    if !lines.iter().any(|line| has_icons(line)) {
        return ui.button_with_size(format!("{}###{id}", lines.join("\n")), size);
    }
    let clicked = ui.button_with_size(format!("###{id}"), size);
    let (min, max) = (ui.item_rect_min(), ui.item_rect_max());
    let disabled = ui.clone_style().alpha < 1.0;
    let color = ui.style_color(StyleColor::Text);
    let line_height = ui.text_line_height();
    let total = line_height * lines.len() as f32;
    let top = min[1] + (max[1] - min[1] - total) / 2.0;
    for (index, line) in lines.iter().enumerate() {
        let width = rich_width(ui, line);
        let x = if left {
            min[0] + (max[0] - min[0]) * 0.03
        } else {
            min[0] + (max[0] - min[0] - width) / 2.0
        };
        let faded = [
            color[0],
            color[1],
            color[2],
            if disabled { 0.45 } else { 1.0 },
        ];
        draw_rich(
            ui,
            [x, top + line_height * index as f32],
            line,
            faded,
            disabled,
        );
    }
    clicked
}

/// A one-line button with `left` from its left edge and `right` flush
/// with its right edge, so a list of them lines their right parts up (the
/// building catalog's prices).
fn split_button(ui: &Ui, id: &str, left: &str, right: &str, size: [f32; 2]) -> bool {
    let clicked = ui.button_with_size(format!("###{id}"), size);
    let (min, max) = (ui.item_rect_min(), ui.item_rect_max());
    let disabled = ui.clone_style().alpha < 1.0;
    let color = ui.style_color(StyleColor::Text);
    let color = [
        color[0],
        color[1],
        color[2],
        if disabled { 0.45 } else { 1.0 },
    ];
    let pad = (max[0] - min[0]) * 0.03;
    let top = min[1] + (max[1] - min[1] - ui.text_line_height()) / 2.0;
    draw_rich(ui, [min[0] + pad, top], left, color, disabled);
    let right_x = max[0] - pad - rich_width(ui, right);
    draw_rich(ui, [right_x, top], right, color, disabled);
    clicked
}

fn draw_game_dockspace(ui: &Ui, viewport: Vec2) {
    let _padding = ui.push_style_var(StyleVar::WindowPadding([0.0, 0.0]));
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
            let id = ::imgui::sys::igGetID_Str(c"GameDockspace".as_ptr());
            // Keep the viewport host for the status-bar safe area, but don't
            // offer map-wide edge targets. Those compete with the four targets
            // on a panel and can turn a bottom drop into a screen-wide split.
            let flags = ::imgui::sys::ImGuiDockNodeFlags_PassthruCentralNode
                | ::imgui::sys::ImGuiDockNodeFlags_NoDockingInCentralNode
                | ::imgui::sys::ImGuiDockNodeFlags_NoSplit;
            ::imgui::sys::igDockSpace(
                id,
                ::imgui::sys::ImVec2 { x: 0.0, y: 0.0 },
                flags as i32,
                std::ptr::null(),
            );
        });
}

fn draw_outer_boxes(
    ui: &Ui,
    layout: &mut ImGuiLayoutState,
    viewport: Vec2,
    arranging: bool,
    actions: &mut Vec<Action>,
) {
    for outer in &mut layout.outer_boxes {
        if outer.scope != BoxScope::Outer && outer.scope != BoxScope::View(layout.active_view) {
            // Keep native dock nodes alive while their view is inactive.
            if outer.dock_id != 0 {
                unsafe {
                    ::imgui::sys::igDockSpace(
                        outer.dock_id,
                        ::imgui::sys::ImVec2 { x: 0.0, y: 0.0 },
                        ::imgui::sys::ImGuiDockNodeFlags_KeepAliveOnly as i32,
                        std::ptr::null(),
                    )
                };
            }
            continue;
        }
        let title = outer.title();
        let native = native_window(&title);
        if !native.is_null() {
            let native = unsafe { &*native };
            outer.geometry.docked = !native.DockNode.is_null();
            outer.geometry.pos = Vec2::new(native.Pos.x, native.Pos.y);
            outer.geometry.size = Vec2::new(native.SizeFull.x, native.SizeFull.y);
        }
        let mut window = ui.window(&title).flags(
            panel_chrome(arranging, false) | WindowFlags::NO_COLLAPSE | WindowFlags::NO_DOCKING,
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
            ui.text(format!(
                "{} BOX {}",
                match outer.scope {
                    BoxScope::Outer => "OUTER",
                    BoxScope::View(view) => view.label(),
                },
                outer.id
            ));
            ui.same_line();
            if ui.small_button(format!("X##outer-{}", outer.id)) {
                actions.push(Action::RemoveOuterBox(outer.id));
            }
            outer.dock_id = unsafe { ::imgui::sys::igGetID_Str(c"Contents".as_ptr()) };
            unsafe {
                ::imgui::sys::igDockSpace(
                    outer.dock_id,
                    ::imgui::sys::ImVec2 { x: 0.0, y: 0.0 },
                    0,
                    std::ptr::null(),
                )
            };
            outer.geometry.pos = Vec2::from_array(ui.window_pos());
            outer.geometry.size = Vec2::from_array(ui.window_size());
            outer.geometry.docked = unsafe { ::imgui::sys::igIsWindowDocked() };
        });
    }
}

impl GameState {
    fn layout_view(&self) -> ViewScope {
        if self.selected_city.is_some()
            || self.selected_barracks.is_some()
            || self.interior_view.is_some()
        {
            ViewScope::City
        } else if self.selected.is_some() || !self.group.is_empty() {
            ViewScope::Troop
        } else {
            ViewScope::Default
        }
    }

    fn selected_pin(&self) -> Option<PinnedPanel> {
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

    fn selected_queue_pin(&self) -> Option<PinnedPanel> {
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

    fn pinned_city_index(&self, pin: PinnedPanel) -> Option<usize> {
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

    fn valid_pin(&self, pin: PinnedPanel) -> bool {
        self.pinned_city_index(pin).is_some()
            || self.pinned_unit_index(pin).is_some()
            || pin.kind == PinnedKind::Group
    }

    /// What captured panel `pin` is for this frame, found by its stable ids
    /// (a group's members by `groups`, the layout's `pinned_groups`), if
    /// anything of it is left.
    fn pin_focus(&self, pin: PinnedPanel, groups: &PinnedGroups) -> Option<PinFocus> {
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
    fn pin_subject(focus: &PinFocus) -> Subject {
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
    fn activate_pinned(&mut self, pin: PinnedPanel, groups: &PinnedGroups, target: Target) {
        if let Some(focus) = self.pin_focus(pin, groups)
            && self.focus_pin(&focus)
        {
            self.activate_target(target);
        }
    }

    fn reorder_pinned(
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
    fn render_pinned_panel(
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

    #[allow(clippy::too_many_arguments)]
    fn render_imgui_window(
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
        let restoring = context_changed || (slot == DEBUG && layout.debug_reposition);
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
        } else if layout.windows[slot].manual && fits {
            Condition::FirstUseEver
        } else {
            Condition::Always
        };
        let flags = panel_chrome(arranging, layout.windows[slot].collapsed);
        let mut window = ui.window(title).flags(flags);
        if slot == SETTINGS {
            // It opens over the other panels: keep their text from showing
            // through its labels.
            window = window.bg_alpha(1.0);
        }
        // While ImGui is moving a panel or showing docking targets it owns the
        // geometry. Applying our automatic position here makes edge previews
        // oscillate between the two layout systems.
        let resized =
            !layout.windows[slot].docked && !layout.defer_geometry && dock_debug_now.is_none();
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
        let restored_debug = slot == DEBUG
            && layout.debug_reposition
            && !layout.windows[slot].docked
            && !layout.defer_geometry;
        window.build(|| {
            if reset_scroll || ui.is_window_appearing() {
                ui.set_scroll_y(0.0);
            }
            self.render_imgui_panel(ui, panel, fonts, None, self.selection_subject(), actions);
            layout.record(slot, ui, arranging);
        });
        // A collapsed window skips the build closure; native state is still
        // available after Begin/End for the next layout pass.
        if restored_debug {
            layout.debug_reposition = false;
        }
        layout.sync_native_window(slot, title);
        if context_changed && !layout.defer_geometry {
            layout.selection_geometry_changed = false;
        }
    }

    /// A row of buttons (`Row::Buttons`), wrapping to the window's width;
    /// with `reorder`, each can be dragged onto another to reorder that
    /// queue, index by index (`Row::Reorder`), and a click is still its
    /// own target.
    #[allow(clippy::too_many_arguments)]
    fn render_buttons(
        &self,
        ui: &Ui,
        panel: &PanelBuilder,
        buttons: &[ButtonSpec],
        compact: bool,
        reorder: Option<QueueKind>,
        fonts: &[FontId; 3],
        scope: Option<PinnedPanel>,
        subject: Subject,
        actions: &mut Vec<Action>,
    ) {
        if buttons.is_empty() {
            return;
        }
        let available = ui.content_region_avail()[0];
        let spacing = ui.clone_style().item_spacing[0];
        let icons = icon_row(buttons);
        let min_width = if icons { ICON_BUTTON_SIZE } else { 128.0 };
        let columns = (((available + spacing) / (min_width + spacing)).floor() as usize)
            .clamp(1, buttons.len());
        let width = if icons {
            ICON_BUTTON_SIZE
        } else {
            ((available - spacing * (columns - 1) as f32) / columns as f32).max(min_width)
        };
        for (index, spec) in buttons.iter().enumerate() {
            if index % columns != 0 {
                ui.same_line();
            }
            let _accent = match spec.state {
                ButtonState::Queued => {
                    Some(ui.push_style_color(StyleColor::Button, [0.34, 0.30, 0.17, 1.0]))
                }
                _ if spec.armed => {
                    Some(ui.push_style_color(StyleColor::Button, [0.24, 0.32, 0.24, 1.0]))
                }
                _ => None,
            };
            let disabled = spec.state == ButtonState::Disabled;
            let _disabled = ui.begin_disabled(disabled);
            let height = if icons {
                ICON_BUTTON_SIZE
            } else if compact {
                28.0
            } else {
                48.0
            };
            let hint = visible_button_hint(&spec.hint, panel.faded);
            let lines = if icons {
                vec![String::new()]
            } else if hint.is_empty() {
                vec![spec.label.clone()]
            } else if compact {
                vec![format!("{}  {hint}", spec.label)]
            } else {
                vec![spec.label.clone(), hint.to_string()]
            };
            let id = format!("{:?}", spec.target);
            if rich_button(ui, &id, &lines, [width, height], false) {
                actions.push(Action::Button(scope, spec.target));
            }
            note_drawn_button(ui, spec.target);
            if icons {
                let icon = action_icons::for_button(spec.target, &spec.label).expect("icon row");
                let color = match spec.state {
                    ButtonState::Disabled => DIM_TEXT,
                    ButtonState::Queued => GOLD_TEXT,
                    ButtonState::Ready if spec.armed => BOOSTED_TEXT,
                    ButtonState::Ready => TEXT,
                };
                draw_action_icon(
                    ui,
                    ui.item_rect_min(),
                    ui.item_rect_max(),
                    icon,
                    color,
                    action_icons::badge(&spec.label),
                    fonts[0],
                );
            }
            if let Some(kind) = reorder
                && !disabled
                && !self.is_resolving()
            {
                // Scoped to the panel, so one city's chips can't reorder
                // another's.
                let name = format!("{kind:?}-reorder-{scope:?}");
                if let Some(source) = ui.drag_drop_source_config(&name).begin_payload(index) {
                    rich_text(ui, &spec.label, TEXT);
                    source.end();
                }
                if let Some(target) = ui.drag_drop_target() {
                    if let Some(Ok(payload)) =
                        target.accept_payload::<usize, _>(&name, DragDropFlags::empty())
                        && payload.delivery
                    {
                        actions.push(Action::Reorder(scope, kind, payload.data, index));
                    }
                    target.pop();
                }
            }
            self.button_tooltip(ui, spec, subject);
        }
    }

    /// The tooltip of the panel button just drawn, for the unit or city of
    /// its panel (`subject`), while it's hovered (dimmed or not).
    fn button_tooltip(&self, ui: &Ui, spec: &ButtonSpec, subject: Subject) {
        if !ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
            return;
        }
        let lines = self.subject_tooltip_lines(spec.target, &spec.label, subject);
        if lines.is_empty() {
            return;
        }
        note_shown_tooltip(&lines);
        ui.tooltip(|| {
            for (_, line) in &lines {
                text_line(ui, line);
            }
        });
    }

    /// Draws `panel`'s rows. `scope` is the captured panel they're in, if
    /// any, and `subject` the unit or city its buttons act on.
    #[allow(clippy::too_many_arguments)]
    fn render_imgui_panel(
        &self,
        ui: &Ui,
        panel: &PanelBuilder,
        fonts: &[FontId; 3],
        scope: Option<PinnedPanel>,
        subject: Subject,
        actions: &mut Vec<Action>,
    ) {
        // With the plan sent, what would change it shows disabled, as the
        // classic panels do (`Layout::dock_panel`).
        let frozen;
        let panel = if self.plan_frozen() {
            let mut copy = panel.clone();
            copy.freeze_plan();
            frozen = copy;
            &frozen
        } else {
            panel
        };
        let label_width = setting_label_width(ui, panel, fonts[1]);
        for row in flat_rows(&panel.rows) {
            match row {
                Row::Heading(text) => {
                    let _font = ui.push_font(fonts[0]);
                    ui.text_colored(GOLD_TEXT, text);
                    ui.separator();
                }
                Row::Setting(setting, value) => {
                    let _font = ui.push_font(fonts[1]);
                    render_setting(ui, *setting, *value, label_width, scope, actions);
                }
                Row::Field(field, text, _) => {
                    let _font = ui.push_font(fonts[1]);
                    render_field(ui, *field, text, label_width, actions);
                }
                Row::Text(px, line) => {
                    let font = match *px {
                        TITLE => fonts[2],
                        SMALL => fonts[0],
                        _ => fonts[1],
                    };
                    let _font = ui.push_font(font);
                    let _wrap = (line.len() == 1).then(|| ui.push_text_wrap_pos());
                    text_line(ui, line);
                }
                Row::Gap(height) => ui.dummy([0.0, height.max(0.0)]),
                Row::ScrollList(_) => unreachable!("flattened by flat_rows"),
                Row::LabeledButtons(..) => unreachable!("classic only"),
                Row::Roster(chips) => {
                    let io = ui.io();
                    let mode = if io.key_shift {
                        ClickMode::QueueMove
                    } else if io.key_ctrl {
                        ClickMode::Swap
                    } else {
                        ClickMode::Normal
                    };
                    for (index, chip) in chips.iter().enumerate() {
                        if index != 0 {
                            ui.same_line_with_spacing(0.0, ROSTER_CHIP_GAP);
                        }
                        let clicked = ui.invisible_button(
                            format!("##roster-{:?}", chip.key),
                            [ROSTER_CHIP, ROSTER_CHIP],
                        );
                        let hovered = ui.is_item_hovered();
                        draw_roster_chip(ui, ui.item_rect_min(), ui.item_rect_max(), chip, hovered);
                        if clicked {
                            actions.push(Action::Button(scope, roster_target(chip.key, mode)));
                        }
                        if hovered {
                            ui.tooltip_text(self.roster_hint(chip.key));
                        }
                    }
                }
                Row::Bar(fraction) => {
                    // No "0%" overlay: it doesn't fit a 12 px bar, and the row
                    // above already says what the bar counts toward.
                    ProgressBar::new(fraction.clamp(0.0, 1.0))
                        .size([ui.content_region_avail()[0], BAR_HEIGHT])
                        .overlay_text("")
                        .build(ui);
                }
                Row::Buttons(buttons, compact) => self.render_buttons(
                    ui, panel, buttons, *compact, None, fonts, scope, subject, actions,
                ),
                Row::Reorder(kind, buttons) => self.render_buttons(
                    ui,
                    panel,
                    buttons,
                    true,
                    Some(*kind),
                    fonts,
                    scope,
                    subject,
                    actions,
                ),
                Row::TitleWithButton(line, spec) => {
                    ui.align_text_to_frame_padding();
                    {
                        let _font = ui.push_font(fonts[0]);
                        text_line(ui, line);
                    }
                    let hint = visible_button_hint(&spec.hint, panel.faded);
                    let label = if hint.is_empty() {
                        spec.label.clone()
                    } else {
                        format!("{}  {hint}", spec.label)
                    };
                    let width = rich_width(ui, &label) + 2.0 * ui.clone_style().frame_padding[0];
                    // At the right end of the title's line.
                    ui.same_line();
                    let room = ui.content_region_avail()[0] - width;
                    if room > 0.0 {
                        let [x, y] = ui.cursor_pos();
                        ui.set_cursor_pos([x + room, y]);
                    }
                    let _disabled = ui.begin_disabled(spec.state == ButtonState::Disabled);
                    let id = format!("{:?}", spec.target);
                    if rich_button(ui, &id, &[label], [width, 0.0], false) {
                        actions.push(Action::Button(scope, spec.target));
                    }
                    note_drawn_button(ui, spec.target);
                    self.button_tooltip(ui, spec, subject);
                }
                Row::BuildingCatalog(city, buttons, ..) => {
                    let height = buttons.len().clamp(1, 5) as f32 * 34.0 + 18.0;
                    ui.child_window(format!("##building-catalog-{city}-{scope:?}"))
                        .size([0.0, height])
                        .border(true)
                        .build(|| {
                            let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.03, 0.5]));
                            for entry in buttons {
                                let CatalogEntry::Card(spec) = entry else {
                                    if let CatalogEntry::Heading(label) = entry {
                                        ui.text_colored(LABEL_TEXT, *label);
                                        ui.dummy([1.0, 4.0]);
                                    }
                                    continue;
                                };
                                let _accent = match spec.state {
                                    ButtonState::Queued => Some(ui.push_style_color(
                                        StyleColor::Button,
                                        [0.34, 0.30, 0.17, 1.0],
                                    )),
                                    _ => None,
                                };
                                let _disabled =
                                    ui.begin_disabled(spec.state == ButtonState::Disabled);
                                let width = ui.content_region_avail()[0].max(80.0);
                                let hint = visible_button_hint(&spec.hint, false);
                                let has_icon =
                                    action_icons::production_unit_icon(spec.target).is_some();
                                let prefix = if has_icon { "     " } else { "" };
                                let left = format!("{prefix}{}", spec.label);
                                let id = format!("{:?}", spec.target);
                                if split_button(ui, &id, &left, hint, [width, 28.0]) {
                                    actions.push(Action::Button(scope, spec.target));
                                }
                                note_drawn_button(ui, spec.target);
                                if let Some(icon) = action_icons::production_unit_icon(spec.target)
                                {
                                    draw_production_icon(
                                        ui,
                                        ui.item_rect_min(),
                                        ui.item_rect_max(),
                                        icon,
                                    );
                                }
                                self.button_tooltip(ui, spec, subject);
                            }
                        });
                }
                Row::QueueItem(item) => {
                    let width = (ui.content_region_avail()[0] - 39.0).max(50.0);
                    let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.03, 0.5]));
                    let _background = ui.push_style_color(
                        StyleColor::Button,
                        if item.active {
                            [0.26, 0.24, 0.15, 1.0]
                        } else if item.waiting {
                            // Waits for the stockpile: a dull red.
                            [0.24, 0.10, 0.08, 1.0]
                        } else {
                            [0.11, 0.14, 0.16, 1.0]
                        },
                    );
                    if rich_button(
                        ui,
                        &format!("queue-{:?}-{}", item.kind, item.index),
                        &[format!(
                            "::  {}",
                            item.label.trim_start_matches("> ").trim()
                        )],
                        [width, 30.0],
                        true,
                    ) {
                        actions.push(Action::Button(
                            scope,
                            Target::QueueItem(item.kind, item.index),
                        ));
                    }
                    note_drawn_button(ui, Target::QueueItem(item.kind, item.index));
                    drop(_background);
                    drop(_align);
                    if !item.locked && !self.is_resolving() {
                        let name = match item.kind {
                            QueueKind::City => "city-queue",
                            QueueKind::Barracks => "barracks-queue",
                            QueueKind::Workers => "worker-jobs",
                            QueueKind::Priority => "priority",
                        };
                        let name = format!("{name}-{:?}", scope);
                        if let Some(source) =
                            ui.drag_drop_source_config(&name).begin_payload(item.index)
                        {
                            rich_text(ui, &item.label, TEXT);
                            source.end();
                        }
                        if let Some(target) = ui.drag_drop_target() {
                            if let Some(Ok(payload)) =
                                target.accept_payload::<usize, _>(&name, DragDropFlags::empty())
                                && payload.delivery
                            {
                                actions.push(Action::Reorder(
                                    scope,
                                    item.kind,
                                    payload.data,
                                    item.index,
                                ));
                            }
                            target.pop();
                        }
                    }
                    ui.same_line();
                    let _remove_color =
                        ui.push_style_color(StyleColor::Button, [0.23, 0.13, 0.13, 1.0]);
                    let _disabled = ui.begin_disabled(item.locked);
                    let remove = item.kind.remove_target(item.index);
                    if ui.small_button(format!("X##remove-{:?}-{}", item.kind, item.index)) {
                        actions.push(Action::Button(scope, remove));
                    }
                    note_drawn_button(ui, remove);
                }
            }
        }
    }

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
        draw_game_dockspace(ui, viewport);
        // Dear ImGui applies a dock drop in NewFrame, before these windows are
        // submitted. Observe that new dock state now; otherwise our cached
        // floating geometry can undo a valid split on its first frame.
        for (slot, title) in SLOT_TITLES.into_iter().enumerate() {
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
        let pending = self.pending();
        let turn = self.shown_turn();
        let mut stockpile = self.stockpile_line();
        if let Some((first, _)) = stockpile.first_mut() {
            first.insert_str(0, "   ");
        }
        ui.window("Status")
            .flags(STATUS_FLAGS)
            .position([0.0, 0.0], Condition::Always)
            .size([viewport.x, STATUS_HEIGHT], Condition::Always)
            .build(|| {
                let end_width = 220.0;
                let turn_color = self.turn_number_color(ui.style_color(StyleColor::Text));
                rich_text(ui, &format!("TURN {turn}"), turn_color);
                // The player's stockpile, then the notice in what's left.
                for (text, color) in &stockpile {
                    ui.same_line_with_spacing(0.0, 0.0);
                    rich_text(ui, text, *color);
                }
                ui.same_line_with_spacing(0.0, 24.0);
                // The notice has the rest of the line, up to End Turn,
                // shortened to what fits; hovering shows all of it.
                let end_x = (viewport.x - end_width).max(8.0);
                let limit = end_x - NOTICE_GAP;
                let notice = self.shown_notice();
                let room = limit - ui.cursor_pos()[0];
                let shown = fit_text(notice, room, |t| rich_width(ui, t));
                if !shown.is_empty() {
                    rich_text(ui, &shown, NOTICE_TEXT);
                    note_shown_notice(&shown, ui.item_rect_min(), ui.item_rect_max(), limit);
                    if shown != notice && ui.is_item_hovered() {
                        ui.tooltip(|| {
                            let _wrap = ui.push_text_wrap_pos_with_pos(NOTICE_TOOLTIP_WIDTH);
                            rich_text(ui, notice, NOTICE_TEXT);
                        });
                    }
                }
                // The second line: Menu, then the view's layout controls,
                // so the notice above has the width of the bar.
                ui.set_cursor_pos([15.0, 27.0]);
                if ui.small_button("MENU") {
                    actions.push(Action::Button(None, Target::OpenSettings));
                }
                note_status_control(ui, "MENU");
                ui.same_line_with_spacing(0.0, 24.0);
                let view = format!("VIEW: {}", layout.active_view.label());
                ui.text(&view);
                note_status_control(ui, &view);
                ui.same_line();
                let layer = if layout.editing_outer {
                    "EDIT OUTER"
                } else {
                    "EDIT VIEW"
                };
                if ui.small_button(layer) {
                    actions.push(Action::ToggleLayoutLayer);
                }
                note_status_control(ui, layer);
                ui.same_line();
                if ui.small_button("+ BOX") {
                    actions.push(Action::CreateBox);
                }
                note_status_control(ui, "+ BOX");
                if layout.active_view != ViewScope::Default && !layout.editing_outer {
                    ui.same_line();
                    if ui.small_button("RESET") {
                        layout.request_reset_active_view();
                    }
                    note_status_control(ui, "RESET");
                    if ui.is_item_hovered() {
                        ui.tooltip_text(
                            "Ctrl+Shift+R: Restore this view's Debug placement from Default",
                        );
                    }
                }
                ui.set_cursor_pos([end_x, 7.0]);
                let label = if self.is_resolving() {
                    self.resolving_label()
                } else {
                    end_turn_label(pending)
                };
                // Waiting for the others' plans, it takes this side's back.
                let _disabled = ui.begin_disabled(self.is_playing_out());
                let end_size = [end_width - 15.0, 29.0];
                if ui.button_with_size(format!("{label}###EndTurn"), end_size) {
                    actions.push(Action::Button(None, Target::EndTurn));
                }
                note_drawn_button(ui, Target::EndTurn);
                if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                    ui.tooltip_text(if self.waiting_for_peers() {
                        "Click: take back End Turn and change your orders"
                    } else {
                        "Space: End turn or select what still needs orders"
                    });
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
        }
        let selection_context = if selection_is_pinned {
            String::new()
        } else if let Some(city) = self.interior_view {
            format!("interior-{city}")
        } else if let Some(city) = self.selected_city {
            format!("city-{city}")
        } else if let Some(city) = self.selected_barracks {
            format!("barracks-{city}")
        } else if let Some(unit) = self.selected {
            format!("unit-{}", self.units[unit].id)
        } else if !self.group.is_empty() {
            "group".into()
        } else {
            String::new()
        };
        let reset_selection_scroll = layout.selection_changed(&selection_context);
        let mut queue = PanelBuilder::default();
        if selection_is_pinned || queue_is_pinned {
        } else if let Some(city) = self.selected_city {
            self.city_queue_panel(city, usize::MAX, &mut queue);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_queue_panel(city, usize::MAX, &mut queue);
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
        } else if layout.outer_box_for_window("Inspect").is_some() {
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
                "Selection",
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
                "Production Queue",
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
                "Debug",
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
                "Inspect",
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
                SLOT_TITLES[UNITS],
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
                SLOT_TITLES[SETTINGS],
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
        let contextual = selection_context
            .split('-')
            .next()
            .filter(|kind| !kind.is_empty());
        if !layout.editing_outer {
            layout.maintain_debug_scope(contextual);
        }
        layout.track_captured_panels();
        let selection_box = layout.outer_box_for_window("Selection");
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
            layout.capture_panel(pin, box_id, "Selection");
        }
        layout.last_selection_outer_box = selection_box;
        let queue_box = layout.outer_box_for_window("Production Queue");
        if queue_box != layout.last_queue_outer_box
            && !selection_is_pinned
            && !queue_is_pinned
            && !queue.rows.is_empty()
            && let (Some(pin), Some(box_id)) = (selected_queue_pin, queue_box)
        {
            layout.capture_panel(pin, box_id, "Production Queue");
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

#[cfg(test)]
impl ImGuiLayoutState {
    /// Tests only: a new outer box, drawn once (`ImGuiScreen::frame`) before
    /// anything is put in it.
    pub(super) fn add_outer_box(&mut self, viewport: Vec2) {
        let editing = std::mem::replace(&mut self.editing_outer, true);
        self.create_box(viewport);
        self.editing_outer = editing;
    }

    /// Tests only: captures `game`'s selection panel in the newest outer
    /// box, as dragging it there with Ctrl held does.
    pub(super) fn capture_selection(&mut self, game: &GameState) {
        let box_id = self.outer_boxes.last().expect("an outer box").id;
        let pin = game.selected_pin().expect("something selected");
        if pin.kind == PinnedKind::Group {
            self.pinned_groups.insert(
                pin.city_id,
                game.group.iter().map(|&i| game.units[i].id).collect(),
            );
        }
        self.capture_panel(pin, box_id, "Selection");
    }

    /// Tests only: how many panels are captured.
    pub(super) fn captured(&self) -> usize {
        self.pinned.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::PLAYER_TEAM;

    #[test]
    fn hover_text_keeps_the_same_imgui_button_id() {
        let _one = one_context_at_a_time();
        let mut context = ::imgui::Context::create();
        context.io_mut().display_size = [640.0, 480.0];
        context.fonts().build_rgba32_texture();
        let ui = context.frame();
        ui.window("ID test").build(|| {
            assert_eq!(
                ui.new_id_str("MOVE###Unit(Move)"),
                ui.new_id_str("MOVE\nM###Unit(Move)"),
            );
            assert_ne!(
                ui.new_id_str("MOVE##Unit(Move)"),
                ui.new_id_str("MOVE\nM##Unit(Move)"),
            );
        });
    }

    #[test]
    fn default_debug_placement_flows_into_views_until_customized() {
        let mut layout = ImGuiLayoutState::default();
        let mut default = WindowGeometry {
            floating_pos: Vec2::new(900.0, 80.0),
            floating_size: Vec2::new(300.0, 350.0),
            ..Default::default()
        };
        layout
            .debug_view_geometry
            .insert(ViewScope::Default, default);
        assert_eq!(
            layout.debug_geometry_for_view(ViewScope::City).floating_pos,
            default.floating_pos
        );
        default.floating_pos = Vec2::new(700.0, 110.0);
        layout
            .debug_view_geometry
            .insert(ViewScope::Default, default);
        assert_eq!(
            layout
                .debug_geometry_for_view(ViewScope::Troop)
                .floating_pos,
            default.floating_pos
        );
        let mut city = default;
        city.floating_pos = Vec2::new(100.0, 400.0);
        layout.debug_view_geometry.insert(ViewScope::City, city);
        layout.debug_view_overrides.insert(ViewScope::City);
        default.floating_pos = Vec2::new(600.0, 120.0);
        layout
            .debug_view_geometry
            .insert(ViewScope::Default, default);
        assert_eq!(
            layout.debug_geometry_for_view(ViewScope::City).floating_pos,
            city.floating_pos
        );
        assert_eq!(
            layout
                .debug_geometry_for_view(ViewScope::Troop)
                .floating_pos,
            default.floating_pos
        );
        layout.debug_view_boxes.insert(ViewScope::City, 42);
        layout.debug_context_relations.insert(
            "city".into(),
            QueueDockRelation {
                direction: ::imgui::sys::ImGuiDir_Right,
                fraction: 0.5,
                anchor_group: false,
            },
        );
        layout.clear_view_debug_override(ViewScope::City);
        assert_eq!(
            layout.debug_geometry_for_view(ViewScope::City).floating_pos,
            default.floating_pos
        );
        assert!(!layout.debug_view_boxes.contains_key(&ViewScope::City));
        assert!(!layout.debug_context_relations.contains_key("city"));
    }

    #[test]
    fn reset_shortcut_is_only_available_while_editing_a_contextual_view() {
        let mut layout = ImGuiLayoutState::default();
        assert!(!layout.request_reset_active_view());
        layout.active_view = ViewScope::City;
        layout.editing_outer = true;
        assert!(!layout.request_reset_active_view());
        layout.editing_outer = false;
        assert!(layout.request_reset_active_view());
        assert_eq!(layout.pending_view_reset, Some(ViewScope::City));
    }

    #[test]
    fn boxes_follow_their_view_or_the_shared_outer_layer() {
        let mut layout = ImGuiLayoutState::default();
        let viewport = Vec2::new(1280.0, 800.0);
        layout.active_view = ViewScope::City;
        layout.create_box(viewport);
        layout.editing_outer = true;
        layout.create_box(viewport);
        layout.active_view = ViewScope::Troop;
        layout.editing_outer = false;
        layout.create_box(viewport);
        assert_eq!(
            layout
                .outer_boxes
                .iter()
                .filter(|b| layout.box_visible(b))
                .count(),
            2
        );
        layout.active_view = ViewScope::Default;
        assert_eq!(
            layout
                .outer_boxes
                .iter()
                .filter(|b| layout.box_visible(b))
                .count(),
            1
        );
        layout.active_view = ViewScope::City;
        assert_eq!(
            layout
                .outer_boxes
                .iter()
                .filter(|b| layout.box_visible(b))
                .count(),
            2
        );
    }

    #[test]
    fn outer_boxes_keep_distinct_space_without_selection() {
        let mut layout = ImGuiLayoutState::default();
        let viewport = Vec2::new(1280.0, 800.0);
        layout.editing_outer = true;
        layout.create_box(viewport);
        layout.create_box(viewport);
        assert_eq!(layout.outer_boxes.len(), 2);
        assert_ne!(layout.outer_boxes[0].id, layout.outer_boxes[1].id);
        let rect = |outer: &OuterBox| Rect {
            min: Vec2::new(
                outer.geometry.pos.x,
                viewport.y - outer.geometry.pos.y - outer.geometry.size.y,
            ),
            max: Vec2::new(
                outer.geometry.pos.x + outer.geometry.size.x,
                viewport.y - outer.geometry.pos.y,
            ),
        };
        assert!(!rect(&layout.outer_boxes[0]).overlaps(rect(&layout.outer_boxes[1]), 0.0));
    }

    #[test]
    fn captured_unit_action_targets_its_stable_unit_id() {
        let mut game = GameState::new();
        let index = game
            .units
            .iter()
            .position(|unit| unit.team == PLAYER_TEAM)
            .unwrap();
        let pin = PinnedPanel {
            kind: PinnedKind::Unit,
            city_id: game.units[index].id,
        };
        game.selected = None;
        game.activate_pinned(
            pin,
            &PinnedGroups::default(),
            Target::Unit(UnitAction::Hold),
        );
        assert!(game.units[index].holding);
    }

    #[test]
    fn pinned_city_action_uses_its_own_city_after_selection_closes() {
        let mut game = GameState::city_scenario();
        game.select_city();
        let pin = game.selected_pin().unwrap();
        game.leave_city_view();
        assert!(game.selected_city.is_none());
        game.activate_pinned(
            pin,
            &PinnedGroups::default(),
            Target::Build(BuildUnit::Melee),
        );
        assert_eq!(game.selected_city, game.pinned_city_index(pin));
        assert!(!game.cities[game.selected_city.unwrap()].queue.is_empty());
    }

    /// The player's units, by index.
    fn player_units(game: &GameState) -> Vec<usize> {
        (0..game.units.len())
            .filter(|&i| game.units[i].team == PLAYER_TEAM)
            .collect()
    }

    fn unit_pin(game: &GameState, unit: usize) -> PinnedPanel {
        PinnedPanel {
            kind: PinnedKind::Unit,
            city_id: game.units[unit].id,
        }
    }

    #[test]
    fn a_captured_units_tooltips_describe_that_unit() {
        let mut game = GameState::new();
        let units = player_units(&game);
        let (selected, pinned) = (units[0], units[1]);
        game.units[pinned].ability_cooldown = 2;
        let pin = unit_pin(&game, pinned);
        let focus = game.pin_focus(pin, &PinnedGroups::default()).unwrap();
        let tooltip = |game: &GameState, subject| {
            game.subject_tooltip_lines(Target::Unit(UnitAction::Ability), "", subject)
                .into_iter()
                .flat_map(|(_, line)| line.into_iter().map(|(text, _)| text))
                .collect::<String>()
        };
        // Nothing selected: the panel's unit still has a tooltip.
        assert!(tooltip(&game, GameState::pin_subject(&focus)).contains("READY IN"));
        // Another unit selected: still the panel's unit's, not the selection's.
        game.set_selection(vec![selected]);
        assert!(tooltip(&game, GameState::pin_subject(&focus)).contains("READY IN"));
        assert!(!tooltip(&game, game.selection_subject()).contains("READY IN"));
    }

    #[test]
    fn a_captured_unit_is_selected_as_the_map_selects_it() {
        let mut game = GameState::city_scenario();
        let unit = player_units(&game)[0];
        let pin = unit_pin(&game, unit);
        game.select_city();
        game.placing_job = Some(JobKind::Road);
        game.activate_pinned(
            pin,
            &PinnedGroups::default(),
            Target::Unit(UnitAction::Move),
        );
        assert_eq!(game.selected, Some(unit));
        assert!(game.group.is_empty());
        assert_eq!(game.selected_city, None, "the city view closes");
        assert_eq!(game.placing_job, None, "placing belongs to the city");
        assert_eq!(game.ui_click_mode, Some(ClickMode::Move));
    }

    #[test]
    fn a_captured_units_armed_button_toggles_off_while_it_is_selected() {
        let mut game = GameState::new();
        let unit = player_units(&game)[0];
        let pin = unit_pin(&game, unit);
        let groups = PinnedGroups::default();
        game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
        assert_eq!(game.ui_click_mode, Some(ClickMode::Move));
        game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
        assert_eq!(game.ui_click_mode, None, "the second click disarms it");
    }

    #[test]
    fn a_captured_group_down_to_one_member_selects_it_as_one_unit() {
        let mut game = GameState::new();
        let units = player_units(&game);
        let ids: Vec<u32> = units[..2].iter().map(|&i| game.units[i].id).collect();
        let pin = PinnedPanel {
            kind: PinnedKind::Group,
            city_id: 7,
        };
        let mut groups = PinnedGroups::default();
        groups.insert(7, ids.clone());
        game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
        assert_eq!(game.group, units[..2], "both members");
        assert_eq!(game.selected, None);
        // One member dies: the other is selected alone, as `set_selection`
        // keeps it.
        game.units.remove(units[0]);
        let left = game.units.iter().position(|u| u.id == ids[1]).unwrap();
        game.activate_pinned(pin, &groups, Target::Unit(UnitAction::Move));
        assert_eq!(game.selected, Some(left));
        assert!(game.group.is_empty());
    }

    #[test]
    fn a_captured_city_already_open_is_not_opened_again() {
        let mut game = GameState::city_scenario();
        game.fund(crate::game::Team::Blue);
        game.select_city();
        let pin = game.selected_pin().unwrap();
        game.camera.center += Vec2::new(3.0, 0.0);
        let camera = game.camera.center;
        game.notice = "SOMETHING TO KEEP".into();
        game.activate_pinned(
            pin,
            &PinnedGroups::default(),
            Target::Build(BuildUnit::Melee),
        );
        assert_eq!(game.camera.center, camera, "no glide back to the city");
        assert!(!game.notice.starts_with("CHOOSE WHAT"), "{}", game.notice);
        // From a unit's selection it opens the city, as the map does.
        let unit = player_units(&game)[0];
        game.set_selection(vec![unit]);
        game.activate_pinned(pin, &PinnedGroups::default(), Target::ToggleYields);
        assert_eq!(game.selected_city, game.pinned_city_index(pin));
        assert_eq!(game.selected, None);
    }

    #[test]
    fn captured_panels_do_nothing_while_a_turn_plays_out() {
        let mut game = GameState::new();
        let units = player_units(&game);
        game.set_selection(vec![units[0]]);
        game.resolve_turn();
        assert!(game.is_playing_out());
        let pin = unit_pin(&game, units[1]);
        game.activate_pinned(
            pin,
            &PinnedGroups::default(),
            Target::Unit(UnitAction::Hold),
        );
        assert!(!game.units[units[1]].holding);
    }

    #[test]
    fn captured_panels_clear_whenever_the_game_changes() {
        let mut game = GameState::city_scenario();
        let mut layout = ImGuiLayoutState::default();
        assert!(!layout.game_changed(game.generation), "the first frame");
        assert!(!layout.game_changed(game.generation), "the same game");
        // F1-F4 and F12 as the debug panel's buttons do: `switch_scenario`.
        game.switch_scenario(Scenario::Cities);
        assert!(layout.game_changed(game.generation));
        assert!(!layout.game_changed(game.generation));
        // F6 changes nothing; F7 loads another game, even the same one.
        game.save_state();
        assert!(!layout.game_changed(game.generation));
        game.load_state();
        assert!(layout.game_changed(game.generation));
        // A network game (`App` keeps the menus of the one it replaces).
        let mut joined = GameState::city_scenario();
        joined.keep_menus_of(&game);
        assert!(layout.game_changed(joined.generation));
    }

    #[test]
    fn pinned_panel_geometry_reserves_room_for_floating_panels() {
        let mut layout = ImGuiLayoutState::default();
        let pin = PinnedPanel {
            kind: PinnedKind::City,
            city_id: 1,
        };
        layout.pinned_geometry.insert(
            pin,
            WindowGeometry {
                pos: Vec2::new(14.0, 66.0),
                size: Vec2::new(370.0, 450.0),
                ..Default::default()
            },
        );
        let viewport = Vec2::new(1280.0, 800.0);
        let mut sizes = [
            Some(Vec2::new(370.0, 450.0)),
            None,
            Some(Vec2::new(330.0, 300.0)),
            None,
            None,
            None,
        ];
        let positions = layout.plan(viewport, &mut sizes);
        let selection = positions[SELECTION].unwrap();
        let selected = Rect {
            min: Vec2::new(
                selection.x,
                viewport.y - selection.y - sizes[SELECTION].unwrap().y,
            ),
            max: Vec2::new(
                selection.x + sizes[SELECTION].unwrap().x,
                viewport.y - selection.y,
            ),
        };
        let pinned = Rect {
            min: Vec2::new(14.0, 800.0 - 66.0 - 450.0),
            max: Vec2::new(384.0, 800.0 - 66.0),
        };
        assert!(!selected.overlaps(pinned, 0.0));
    }

    #[test]
    fn the_status_bar_never_scrolls() {
        assert!(
            STATUS_FLAGS.contains(WindowFlags::NO_SCROLLBAR | WindowFlags::NO_SCROLL_WITH_MOUSE)
        );
        assert!(STATUS_FLAGS.contains(WindowFlags::NO_TITLE_BAR | WindowFlags::NO_MOVE));
    }

    #[test]
    fn showing_ctrl_chrome_preserves_panel_bounds() {
        let viewport = Vec2::new(1200.0, 800.0);
        let measured = Vec2::new(300.0, 200.0);
        let mut layout = ImGuiLayoutState::default();
        // ImGui moves the content cursor down by one title bar. Normalize
        // that observation before feeding it back into automatic placement.
        let normal = panel_content_height(212.0, 8.0, None);
        let arranging = panel_content_height(238.0, 8.0, Some(26.0));
        assert_eq!(normal, arranging);
        layout.windows[SELECTION].content_height = normal;
        let automatic = layout.size(SELECTION, measured, viewport);
        layout.windows[SELECTION].content_height = arranging;
        assert_eq!(layout.size(SELECTION, measured, viewport), automatic);

        layout.windows[SELECTION].manual = true;
        layout.windows[SELECTION].floating_size = Vec2::new(340.0, 270.0);
        assert_eq!(
            layout.size(SELECTION, measured, viewport),
            Vec2::new(340.0, 270.0)
        );
    }

    #[test]
    fn the_layout_saves_as_text_and_reads_back() {
        let placed = WindowGeometry {
            pos: Vec2::new(120.5, 64.0),
            size: Vec2::new(410.0, 233.0),
            floating_pos: Vec2::new(90.0, 70.0),
            floating_size: Vec2::new(400.0, 220.0),
            manual: true,
            docked: false,
            collapsed: true,
            // Recomputed every frame, so not saved.
            content_height: 180.0,
        };
        let mut layout = ImGuiLayoutState::default();
        layout.windows[UNITS] = placed;
        layout.windows[DEBUG] = WindowGeometry {
            docked: true,
            ..placed
        };
        layout.outer_boxes.push(OuterBox {
            id: 3,
            dock_id: 77,
            scope: BoxScope::View(ViewScope::City),
            geometry: placed,
        });
        layout.next_outer_box_id = 4;
        layout.last_selection_outer_box = Some(3);
        layout.debug_view_geometry.insert(ViewScope::Troop, placed);
        layout.debug_view_overrides.insert(ViewScope::Troop);
        layout.debug_view_boxes.insert(ViewScope::City, 3);
        layout.selection_geometries.insert("unit".into(), placed);

        let read = ImGuiLayoutState::from_text(&layout.to_text());
        let same = |a: &WindowGeometry, b: &WindowGeometry| {
            a.pos == b.pos
                && a.size == b.size
                && a.floating_pos == b.floating_pos
                && a.floating_size == b.floating_size
                && (a.manual, a.docked, a.collapsed) == (b.manual, b.docked, b.collapsed)
        };
        for slot in 0..SLOT_COUNT {
            assert!(
                same(&read.windows[slot], &layout.windows[slot]),
                "slot {slot}"
            );
        }
        assert_eq!(read.windows[UNITS].content_height, 0.0);
        assert_eq!(read.outer_boxes.len(), 1);
        let outer = read.outer_boxes[0];
        assert_eq!(
            (outer.id, outer.scope),
            (3, BoxScope::View(ViewScope::City))
        );
        assert_eq!(outer.dock_id, 0, "ImGui hands out the dock id again");
        assert!(same(&outer.geometry, &placed));
        assert_eq!(read.next_outer_box_id, 4);
        assert_eq!(read.last_selection_outer_box, Some(3));
        assert_eq!(read.last_queue_outer_box, None);
        assert!(same(&read.debug_view_geometry[&ViewScope::Troop], &placed));
        assert!(!read.debug_view_geometry.contains_key(&ViewScope::City));
        assert!(read.debug_view_overrides.contains(&ViewScope::Troop));
        assert_eq!(read.debug_view_boxes.get(&ViewScope::City), Some(&3));
        assert!(same(&read.selection_geometries["unit"], &placed));
    }

    #[test]
    fn a_damaged_layout_file_still_loads() {
        let text = "window 9 1 2 3 4 5 6 7 8 1 0 0\nwindow 1 nope\nbox 2 sideways 0 0 0 0 0 0 0 0 0 0 0\nbox 5 outer 0 0 10 10 0 0 10 10 1 0 0\nnext_box 1\njunk\n";
        let layout = ImGuiLayoutState::from_text(text);
        assert!(
            layout.windows.iter().all(|w| !w.manual),
            "bad windows skipped"
        );
        assert_eq!(layout.outer_boxes.len(), 1);
        assert_eq!(layout.next_outer_box_id, 6, "past every box already there");
        assert_eq!(ImGuiLayoutState::from_text("").outer_boxes.len(), 0);
    }

    #[test]
    fn panel_handles_only_appear_while_arranging() {
        let normal = panel_chrome(false, false);
        assert!(normal.contains(WindowFlags::NO_TITLE_BAR));
        assert!(normal.contains(WindowFlags::NO_RESIZE));
        assert!(normal.contains(WindowFlags::NO_MOVE));
        assert!(!normal.contains(WindowFlags::NO_COLLAPSE));
        let collapsed = panel_chrome(false, true);
        assert!(!collapsed.contains(WindowFlags::NO_TITLE_BAR));
        assert!(collapsed.contains(WindowFlags::NO_MOVE));
        let arranging = panel_chrome(true, false);
        assert!(!arranging.intersects(
            WindowFlags::NO_TITLE_BAR
                | WindowFlags::NO_RESIZE
                | WindowFlags::NO_MOVE
                | WindowFlags::NO_COLLAPSE
        ));
    }

    #[test]
    fn observed_content_sizes_floaters_and_preserves_queue_cap() {
        let mut layout = ImGuiLayoutState::default();
        layout.windows[SELECTION].content_height = 310.0;
        layout.windows[QUEUE].content_height = 310.0;
        let viewport = Vec2::new(1200.0, 800.0);
        assert_eq!(
            layout.size(SELECTION, Vec2::new(500.0, 250.0), viewport).y,
            310.0
        );
        assert_eq!(
            layout.size(QUEUE, Vec2::new(500.0, 225.0), viewport).y,
            225.0
        );
        layout.windows[SELECTION].collapsed = true;
        assert_eq!(
            layout.size(SELECTION, Vec2::new(500.0, 250.0), viewport).y,
            COLLAPSED_HEIGHT
        );
    }

    #[test]
    fn selection_context_resets_scroll_only_when_it_changes() {
        let mut layout = ImGuiLayoutState::default();
        assert!(layout.selection_changed("city-1"));
        assert!(!layout.selection_changed("city-1"));
        assert!(layout.selection_changed("unit-7"));
    }

    #[test]
    fn selection_geometry_is_shared_by_kind_but_not_between_city_and_troops() {
        let mut layout = ImGuiLayoutState::default();
        layout.selection_changed("city-1");
        layout.windows[SELECTION].size = Vec2::new(600.0, 420.0);
        layout.windows[SELECTION].manual = true;
        layout.selection_changed("city-2");
        assert_eq!(layout.windows[SELECTION].size.y, 420.0);
        layout.selection_changed("unit-7");
        assert_eq!(layout.windows[SELECTION].size, Vec2::ZERO);
        layout.windows[SELECTION].size = Vec2::new(350.0, 190.0);
        layout.selection_changed("unit-8");
        assert_eq!(layout.windows[SELECTION].size.y, 190.0);
        layout.selection_changed("city-1");
        assert_eq!(layout.windows[SELECTION].size.y, 420.0);
        assert!(layout.windows[SELECTION].manual);
    }

    #[test]
    fn returning_city_does_not_reuse_a_lost_dock_nodes_position() {
        let mut layout = ImGuiLayoutState::default();
        layout.selection_changed("city-1");
        layout.windows[SELECTION] = WindowGeometry {
            pos: Vec2::new(350.0, 66.0),
            size: Vec2::new(340.0, 700.0),
            floating_size: Vec2::new(560.0, 390.0),
            manual: true,
            docked: true,
            ..WindowGeometry::default()
        };
        layout.selection_changed("");
        layout.windows[SELECTION].docked = false;
        layout.selection_changed("city-1");
        assert!(!layout.windows[SELECTION].docked);
        assert!(!layout.windows[SELECTION].manual);
        assert_eq!(layout.windows[SELECTION].pos, Vec2::ZERO);
        assert_eq!(layout.windows[SELECTION].size.y, 390.0);
    }

    #[test]
    fn floating_windows_above_the_status_bar_are_repositioned() {
        let mut layout = ImGuiLayoutState::default();
        layout.windows[SELECTION] = WindowGeometry {
            pos: Vec2::new(20.0, STATUS_HEIGHT + PANEL_MARGIN - 1.0),
            size: Vec2::new(300.0, 200.0),
            manual: true,
            ..WindowGeometry::default()
        };
        assert_eq!(
            layout.condition(SELECTION, Vec2::new(1200.0, 800.0)),
            Condition::Always
        );
        let mut sizes = [Some(Vec2::new(300.0, 200.0)), None, None, None, None, None];
        let positions = layout.plan(Vec2::new(1200.0, 800.0), &mut sizes);
        assert_eq!(
            positions[SELECTION].unwrap().y,
            STATUS_HEIGHT + PANEL_MARGIN
        );
    }

    fn assert_fits_without_overlap(
        viewport: Vec2,
        sizes: [Option<Vec2>; SLOT_COUNT],
        positions: [Option<Vec2>; SLOT_COUNT],
    ) {
        let mut rects = Vec::new();
        for (size, position) in sizes.into_iter().zip(positions) {
            if let (Some(size), Some(position)) = (size, position) {
                assert!(position.x >= PANEL_MARGIN);
                assert!(position.y >= STATUS_HEIGHT + PANEL_MARGIN);
                assert!(position.x + size.x <= viewport.x - PANEL_MARGIN + 1.0);
                assert!(position.y + size.y <= viewport.y - PANEL_MARGIN + 1.0);
                let rect = Rect {
                    min: position,
                    max: position + size,
                };
                assert!(rects.iter().all(|other| !rect.overlaps(*other, 0.0)));
                rects.push(rect);
            }
        }
    }

    #[test]
    fn logical_viewport_docks_full_panels_without_overlap() {
        let viewport = Vec2::new(2048.0, 1078.0);
        let mut sizes = [
            Some(Vec2::new(560.0, 630.0)),
            Some(Vec2::new(560.0, 220.0)),
            Some(Vec2::new(330.0, 340.0)),
            Some(Vec2::new(345.0, 190.0)),
            Some(Vec2::new(260.0, 90.0)),
            Some(Vec2::new(330.0, 180.0)),
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions.iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
    }

    #[test]
    fn the_settings_menu_opens_in_the_middle_of_the_screen() {
        let viewport = Vec2::new(1600.0, 900.0);
        let mut sizes = [
            Some(Vec2::new(560.0, 300.0)),
            None,
            Some(Vec2::new(330.0, 340.0)),
            None,
            Some(Vec2::new(260.0, 90.0)),
            Some(Vec2::new(330.0, 200.0)),
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        let settings = positions[SETTINGS].expect("settings placed");
        let center = settings + sizes[SETTINGS].unwrap() / 2.0;
        assert!(
            (center - viewport / 2.0).abs().max_element() <= 1.0,
            "{center}"
        );
        // The docked panels keep their places, as if it weren't open.
        let mut without = sizes;
        without[SETTINGS] = None;
        let docked = ImGuiLayoutState::default().plan(viewport, &mut without);
        for slot in [SELECTION, DEBUG, INSPECT, UNITS] {
            assert_eq!(positions[slot], docked[slot], "slot {slot}");
        }
    }

    #[test]
    fn manual_window_is_reserved_for_automatic_panels() {
        let viewport = Vec2::new(1280.0, 720.0);
        let mut sizes = [
            Some(Vec2::new(500.0, 370.0)),
            Some(Vec2::new(500.0, 180.0)),
            Some(Vec2::new(280.0, 280.0)),
            None,
            None,
            None,
        ];
        let mut layout = ImGuiLayoutState::default();
        layout.windows[DEBUG] = WindowGeometry {
            pos: Vec2::new(510.0, 80.0),
            size: sizes[DEBUG].unwrap(),
            floating_size: sizes[DEBUG].unwrap(),
            floating_pos: Vec2::new(510.0, 80.0),
            manual: true,
            docked: false,
            collapsed: false,
            content_height: 0.0,
        };
        let positions = layout.plan(viewport, &mut sizes);
        assert!(positions[..3].iter().all(Option::is_some));
        assert_eq!(sizes[SELECTION].unwrap().y, 370.0);
        assert_fits_without_overlap(viewport, sizes, positions);
    }

    #[test]
    fn native_docked_windows_reserve_their_actual_split_rectangles() {
        let viewport = Vec2::new(1280.0, 720.0);
        let mut layout = ImGuiLayoutState::default();
        layout.windows[SELECTION] = WindowGeometry {
            pos: Vec2::new(14.0, 210.0),
            size: Vec2::new(560.0, 490.0),
            docked: true,
            ..WindowGeometry::default()
        };
        let mut sizes = [
            Some(layout.size(SELECTION, Vec2::new(560.0, 200.0), viewport)),
            Some(Vec2::new(560.0, 180.0)),
            None,
            None,
            None,
            None,
        ];
        assert_eq!(sizes[SELECTION].unwrap().y, 490.0);
        let positions = layout.plan(viewport, &mut sizes);
        let queue = positions[QUEUE].unwrap();
        let docked = Rect {
            min: layout.windows[SELECTION].pos,
            max: layout.windows[SELECTION].pos + layout.windows[SELECTION].size,
        };
        let queue_rect = Rect {
            min: queue,
            max: queue + sizes[QUEUE].unwrap(),
        };
        assert!(!queue_rect.overlaps(docked, 0.0));
    }

    #[test]
    fn docking_does_not_replace_a_panels_floating_size() {
        let viewport = Vec2::new(1280.0, 720.0);
        let mut layout = ImGuiLayoutState::default();
        layout.windows[QUEUE] = WindowGeometry {
            size: Vec2::new(560.0, 120.0),
            floating_size: Vec2::new(560.0, 225.0),
            docked: true,
            manual: true,
            ..WindowGeometry::default()
        };
        assert_eq!(
            layout.size(QUEUE, Vec2::new(560.0, 225.0), viewport).y,
            120.0
        );
        layout.windows[QUEUE].docked = false;
        assert_eq!(
            layout.size(QUEUE, Vec2::new(560.0, 225.0), viewport).y,
            225.0
        );
    }

    #[test]
    fn compact_viewport_keeps_core_panels_onscreen() {
        let viewport = Vec2::new(800.0, 600.0);
        let mut sizes = [
            Some(Vec2::new(420.0, 287.0)),
            Some(Vec2::new(420.0, 225.0)),
            Some(Vec2::new(330.0, 330.0)),
            Some(Vec2::new(345.0, 190.0)),
            None,
            None,
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions[..3].iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
    }

    #[test]
    fn high_dpi_logical_viewport_still_has_room_for_debug() {
        let viewport = Vec2::new(640.0, 480.0);
        let mut sizes = [
            Some(Vec2::new(315.0, 167.0)),
            Some(Vec2::new(315.0, 225.0)),
            Some(Vec2::new(275.0, 400.0)),
            None,
            None,
            None,
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions[..3].iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
    }
}
