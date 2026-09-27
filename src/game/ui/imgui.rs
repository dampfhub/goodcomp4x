//! Experimental immediate-mode presentation of the game's existing panel content.
//! Game rules and button actions remain shared with the classic UI.

use ::imgui::{
    Condition, DragDropFlags, FontId, ItemHoveredFlags, MouseButton as ImMouseButton, ProgressBar,
    StyleColor, StyleVar, Ui, WindowFlags,
};

use super::builder::Row;
use super::text::end_turn_label;
use super::*;
use crate::game::PLAYER_TEAM;

enum Action {
    Button(Option<PinnedPanel>, Target),
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
const COLLAPSED_HEIGHT: f32 = 30.0;
const SLOT_COUNT: usize = 5;
const SELECTION: usize = 0;
const QUEUE: usize = 1;
const DEBUG: usize = 2;
const INSPECT: usize = 3;
const UNITS: usize = 4;
/// Each slot's native window title, by slot. A new panel that isn't static
/// chrome (like the status bar) gets a slot here, so it can be dragged,
/// docked, resized and put in a box like the rest.
const SLOT_TITLES: [&str; SLOT_COUNT] =
    ["Selection", "Production Queue", "Debug", "Inspect", "Units"];

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

fn panel_chrome(arranging: bool, collapsed: bool) -> WindowFlags {
    if arranging {
        WindowFlags::NO_SAVED_SETTINGS
    } else {
        let mut flags =
            WindowFlags::NO_SAVED_SETTINGS | WindowFlags::NO_RESIZE | WindowFlags::NO_MOVE;
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
    title_visible: bool,
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
    pinned_groups: std::collections::HashMap<u32, Vec<u32>>,
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

    fn size(&self, slot: usize, measured: Vec2, viewport: Vec2, arranging: bool) -> Vec2 {
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
            let chrome_delta = 30.0 * (arranging as i32 - window.title_visible as i32) as f32;
            let observed = (window.content_height + chrome_delta).max(60.0);
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
        for slot in 0..SLOT_COUNT {
            if positions[slot].is_some() {
                continue;
            }
            let Some(size) = sizes[slot] else { continue };
            let zone = match slot {
                SELECTION | QUEUE | INSPECT => Zone::BottomLeft,
                UNITS => Zone::TopLeft,
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
        self.windows[slot].content_height = ui.cursor_pos()[1] + ui.clone_style().window_padding[1];
        self.windows[slot].title_visible = arranging;
    }
}

fn measure_panel(
    ui: &Ui,
    panel: &PanelBuilder,
    fonts: &[FontId; 3],
    width: f32,
    arranging: bool,
) -> f32 {
    let inner = (width - 45.0).max(100.0);
    let mut height = if arranging { 56.0 } else { 26.0 }; // title, border and padding
    for row in &panel.rows {
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
            Row::Buttons(buttons, compact) => {
                let columns = ((inner + 7.0) / 135.0).floor().max(1.0) as usize;
                let rows = buttons.len().div_ceil(columns);
                rows as f32 * (if *compact { 34.0 } else { 54.0 })
            }
            Row::QueueItem(_) => 37.0,
            Row::Roster(_) => ROSTER_CHIP + 6.0,
        };
    }
    height + 12.0
}

/// For a floating panel the player placed (which keeps its own size), how much
/// taller it must get this frame because Ctrl just showed its title bar
/// (`title_height`), or shorter because it just hid it. `None` when the title
/// bar didn't just change, or the panel is laid out automatically, docked or
/// collapsed (those already account for it).
fn title_bar_change(window: WindowGeometry, arranging: bool, title_height: f32) -> Option<f32> {
    let changed = arranging != window.title_visible;
    (changed && window.manual && !window.docked && !window.collapsed && window.size != Vec2::ZERO)
        .then_some(if arranging {
            title_height
        } else {
            -title_height
        })
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
    super::super::draw::push_unit_token(Vec2::ZERO, chip.look, radius, chip.color, &mut vertices);
    // The token is built Y-up around the origin; ImGui's Y points down.
    let center = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let at = |v: &Vertex| [center[0] + v.pos[0], center[1] - v.pos[1]];
    for triangle in vertices.chunks_exact(3) {
        draw.add_triangle(
            at(&triangle[0]),
            at(&triangle[1]),
            at(&triangle[2]),
            triangle[0].color,
        )
        .filled(true)
        .build();
    }
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
        ui.text_colored(readable, text);
    }
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
                && city.team == PLAYER_TEAM
                && (matches!(pin.kind, PinnedKind::City | PinnedKind::CityQueue)
                    || city.barracks.is_some())
        })
    }

    fn pinned_unit_index(&self, pin: PinnedPanel) -> Option<usize> {
        (pin.kind == PinnedKind::Unit)
            .then(|| {
                self.units
                    .iter()
                    .position(|unit| unit.id == pin.city_id && unit.team == PLAYER_TEAM)
            })
            .flatten()
    }

    fn valid_pin(&self, pin: PinnedPanel) -> bool {
        self.pinned_city_index(pin).is_some()
            || self.pinned_unit_index(pin).is_some()
            || pin.kind == PinnedKind::Group
    }

    fn activate_pinned(&mut self, pin: PinnedPanel, target: Target) {
        if let Some(unit) = self.pinned_unit_index(pin) {
            self.selected = Some(unit);
            self.selected_city = None;
            self.selected_barracks = None;
            self.group.clear();
            self.activate_target(target);
            return;
        }
        let Some(city) = self.pinned_city_index(pin) else {
            return;
        };
        match pin.kind {
            PinnedKind::City | PinnedKind::CityQueue => self.open_city(city),
            PinnedKind::Barracks | PinnedKind::BarracksQueue => self.open_barracks(city),
            PinnedKind::Unit | PinnedKind::Group => unreachable!(),
        }
        self.activate_target(target);
    }

    fn reorder_pinned(&mut self, pin: PinnedPanel, kind: QueueKind, source: usize, target: usize) {
        let Some(city) = self.pinned_city_index(pin) else {
            return;
        };
        match pin.kind {
            PinnedKind::City | PinnedKind::CityQueue => self.open_city(city),
            PinnedKind::Barracks | PinnedKind::BarracksQueue => self.open_barracks(city),
            PinnedKind::Unit | PinnedKind::Group => return,
        }
        self.reorder_queue(kind, source, target);
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
        let mut panel = PanelBuilder::default();
        match pin.kind {
            PinnedKind::City => {
                let Some(city) = self.pinned_city_index(pin) else {
                    return;
                };
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
            PinnedKind::Barracks => {
                let Some(city) = self.pinned_city_index(pin) else {
                    return;
                };
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
            PinnedKind::Unit => {
                let Some(unit) = self.pinned_unit_index(pin) else {
                    return;
                };
                self.unit_info(unit, &mut panel);
                panel.gap(GAP);
                panel.buttons(self.unit_buttons(unit));
            }
            PinnedKind::Group => {
                let Some(ids) = layout.pinned_groups.get(&pin.city_id) else {
                    return;
                };
                let members: Vec<_> = ids
                    .iter()
                    .filter_map(|id| {
                        self.units
                            .iter()
                            .position(|u| u.id == *id && u.team == PLAYER_TEAM)
                    })
                    .collect();
                if members.is_empty() {
                    return;
                }
                self.group_tray_for(&members, &mut panel);
            }
            PinnedKind::CityQueue => {
                let Some(city) = self.pinned_city_index(pin) else {
                    return;
                };
                self.city_queue_panel(city, usize::MAX, &mut panel);
            }
            PinnedKind::BarracksQueue => {
                let Some(city) = self.pinned_city_index(pin) else {
                    return;
                };
                self.barracks_queue_panel(city, usize::MAX, &mut panel);
            }
        }
        let title = pin.title();
        let width = 370.0_f32.min(viewport.x - 2.0 * PANEL_MARGIN);
        let height = measure_panel(ui, &panel, fonts, width, arranging)
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
            self.render_imgui_panel(ui, &panel, fonts, Some(pin), actions);
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
        // A panel the player moved or resized keeps its own size, so when Ctrl
        // shows or hides its title bar it would push the content down and clip
        // it. Grow (or shrink) it by the title bar instead, that one frame.
        let geometry = layout.windows[slot];
        let (condition, size_condition, size) =
            match title_bar_change(geometry, arranging, ui.frame_height()) {
                Some(delta) => (
                    Condition::Always,
                    Condition::Always,
                    (geometry.size + Vec2::new(0.0, delta)).min(Vec2::new(
                        viewport.x - 2.0 * PANEL_MARGIN,
                        viewport.y - STATUS_HEIGHT - 2.0 * PANEL_MARGIN,
                    )),
                ),
                None => (condition, size_condition, size),
            };
        let position = if title_bar_change(geometry, arranging, 0.0).is_some() {
            geometry.pos
        } else {
            position
        };
        let flags = panel_chrome(arranging, layout.windows[slot].collapsed);
        let mut window = ui.window(title).flags(flags);
        // While ImGui is moving a panel or showing docking targets it owns the
        // geometry. Applying our automatic position here makes edge previews
        // oscillate between the two layout systems.
        if !layout.windows[slot].docked && !layout.defer_geometry && dock_debug_now.is_none() {
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
            self.render_imgui_panel(ui, panel, fonts, None, actions);
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

    fn render_imgui_panel(
        &self,
        ui: &Ui,
        panel: &PanelBuilder,
        fonts: &[FontId; 3],
        scope: Option<PinnedPanel>,
        actions: &mut Vec<Action>,
    ) {
        for row in &panel.rows {
            match row {
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
                            format!("##roster-{}", chip.id),
                            [ROSTER_CHIP, ROSTER_CHIP],
                        );
                        let hovered = ui.is_item_hovered();
                        draw_roster_chip(ui, ui.item_rect_min(), ui.item_rect_max(), chip, hovered);
                        if clicked {
                            actions.push(Action::Button(scope, roster_target(chip.id, mode)));
                        }
                        if hovered && let Some(unit) = self.units.iter().find(|u| u.id == chip.id) {
                            ui.tooltip_text(format!(
                                "{} - CLICK: SELECT · SHIFT: ADD · CTRL: REMOVE",
                                self.unit_role(unit)
                            ));
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
                Row::Buttons(buttons, compact) => {
                    if buttons.is_empty() {
                        continue;
                    }
                    let available = ui.content_region_avail()[0];
                    let spacing = ui.clone_style().item_spacing[0];
                    let min_width = 128.0;
                    let columns = (((available + spacing) / (min_width + spacing)).floor()
                        as usize)
                        .clamp(1, buttons.len());
                    let width = ((available - spacing * (columns - 1) as f32) / columns as f32)
                        .max(min_width);
                    for (index, spec) in buttons.iter().enumerate() {
                        if index % columns != 0 {
                            ui.same_line();
                        }
                        let _accent = match spec.state {
                            ButtonState::Queued => Some(
                                ui.push_style_color(StyleColor::Button, [0.34, 0.30, 0.17, 1.0]),
                            ),
                            _ if spec.armed => Some(
                                ui.push_style_color(StyleColor::Button, [0.24, 0.32, 0.24, 1.0]),
                            ),
                            _ => None,
                        };
                        let _disabled = ui.begin_disabled(spec.state == ButtonState::Disabled);
                        let label = if *compact {
                            if spec.hint.is_empty()
                                || matches!(spec.hint.as_str(), "AUTO" | "CLICK")
                            {
                                spec.label.clone()
                            } else {
                                format!("{}  {}", spec.label, spec.hint)
                            }
                        } else if spec.hint.is_empty() {
                            spec.label.clone()
                        } else {
                            format!("{}\n{}", spec.label, spec.hint)
                        };
                        let label = format!("{label}##{:?}", spec.target);
                        if ui.button_with_size(label, [width, if *compact { 28.0 } else { 48.0 }]) {
                            actions.push(Action::Button(scope, spec.target));
                        }
                        if ui.is_item_hovered_with_flags(ItemHoveredFlags::ALLOW_WHEN_DISABLED) {
                            let tooltip = Button {
                                target: spec.target,
                                label: spec.label.clone(),
                                hint: spec.hint.clone(),
                                state: spec.state,
                                armed: spec.armed,
                                faded: false,
                                min: Vec2::ZERO,
                                max: Vec2::ZERO,
                            };
                            ui.tooltip(|| {
                                for (_, line) in self.tooltip_lines(&tooltip) {
                                    text_line(ui, &line);
                                }
                            });
                        }
                    }
                }
                Row::QueueItem(item) => {
                    let width = (ui.content_region_avail()[0] - 39.0).max(50.0);
                    let _align = ui.push_style_var(StyleVar::ButtonTextAlign([0.03, 0.5]));
                    let _background = ui.push_style_color(
                        StyleColor::Button,
                        if item.active {
                            [0.26, 0.24, 0.15, 1.0]
                        } else {
                            [0.11, 0.14, 0.16, 1.0]
                        },
                    );
                    ui.button_with_size(
                        format!(
                            "::  {}##queue-{:?}-{}",
                            item.label.trim_start_matches("> ").trim(),
                            item.kind,
                            item.index
                        ),
                        [width, 30.0],
                    );
                    drop(_background);
                    drop(_align);
                    if !item.locked && !self.is_resolving() {
                        let name = match item.kind {
                            QueueKind::City => "city-queue",
                            QueueKind::Barracks => "barracks-queue",
                            QueueKind::Workers => "worker-jobs",
                        };
                        let name = format!("{name}-{:?}", scope);
                        if let Some(source) =
                            ui.drag_drop_source_config(&name).begin_payload(item.index)
                        {
                            ui.text(&item.label);
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
                    if ui.small_button(format!("X##remove-{:?}-{}", item.kind, item.index)) {
                        actions.push(Action::Button(scope, item.kind.remove_target(item.index)));
                    }
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
                            .any(|unit| unit.id == *member && unit.team == PLAYER_TEAM)
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
        let turn = if self.is_resolving() {
            self.turn
        } else {
            self.turn + 1
        };
        ui.window("Status")
            .flags(STATUS_FLAGS)
            .position([0.0, 0.0], Condition::Always)
            .size([viewport.x, STATUS_HEIGHT], Condition::Always)
            .build(|| {
                let end_width = 220.0;
                ui.text(format!("TURN {turn}"));
                ui.same_line();
                let max_notice = (viewport.x - end_width - 520.0).max(0.0);
                if ui.calc_text_size(self.shown_notice())[0] <= max_notice {
                    ui.text_colored(NOTICE_TEXT, self.shown_notice());
                } else {
                    let mut shortened = self.shown_notice().to_string();
                    while !shortened.is_empty() && ui.calc_text_size(&shortened)[0] > max_notice {
                        shortened.pop();
                    }
                    ui.text_colored(NOTICE_TEXT, shortened);
                }
                ui.set_cursor_pos([(viewport.x - end_width - 365.0).max(8.0), 7.0]);
                ui.text(format!("VIEW: {}", layout.active_view.label()));
                ui.set_cursor_pos([(viewport.x - end_width - 365.0).max(8.0), 27.0]);
                let layer = if layout.editing_outer {
                    "EDIT OUTER"
                } else {
                    "EDIT VIEW"
                };
                if ui.small_button(layer) {
                    actions.push(Action::ToggleLayoutLayer);
                }
                ui.same_line();
                if ui.small_button("+ BOX") {
                    actions.push(Action::CreateBox);
                }
                if layout.active_view != ViewScope::Default && !layout.editing_outer {
                    ui.same_line();
                    if ui.small_button("RESET [CTRL+SHIFT+R]") {
                        layout.request_reset_active_view();
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text("Restore this view's Debug placement from Default");
                    }
                }
                ui.set_cursor_pos([(viewport.x - end_width).max(8.0), 7.0]);
                let label = if self.is_resolving() {
                    "RESOLVING".into()
                } else {
                    end_turn_label(pending)
                };
                let _disabled = ui.begin_disabled(self.is_resolving());
                if ui.button_with_size(format!("{label}  [SPACE]"), [end_width - 15.0, 29.0]) {
                    actions.push(Action::Button(None, Target::EndTurn));
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
            tray.gap(GAP);
            tray.buttons(self.unit_buttons(idx));
        } else if !self.group.is_empty() {
            self.group_tray(&mut tray);
        } else if let Some(hex) = self.inspected_tile {
            self.tile_tray(hex, &mut tray);
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
        } else if let Some(hex) = self.inspected_tile {
            format!("tile-{}-{}", hex.q, hex.r)
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
            if let Some(city) = self.cities.iter().position(|city| city.pos == hex) {
                self.structure_hover_panel(city, false, &mut hover);
            } else if let Some(city) = self
                .cities
                .iter()
                .position(|city| city.barracks == Some(hex))
            {
                self.structure_hover_panel(city, true, &mut hover);
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
        let queue_height = measure_panel(ui, &queue, fonts, left_width, arranging).min(225.0);
        let mut tray_height = measure_panel(ui, &tray, fonts, left_width, arranging).min(available);
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
                measure_panel(ui, &debug, fonts, debug_width, arranging).min(available),
            )),
            (!hover.rows.is_empty()).then_some(Vec2::new(
                345.0_f32.min(max_width),
                measure_panel(ui, &hover, fonts, 345.0_f32.min(max_width), arranging)
                    .min(available),
            )),
            units.as_ref().map(|units| {
                let width = roster_width(units).min(max_width);
                Vec2::new(
                    width,
                    measure_panel(ui, units, fonts, width, arranging).min(available),
                )
            }),
        ];
        let mut sizes = std::array::from_fn(|slot| {
            measured[slot].map(|size| layout.size(slot, size, viewport, arranging))
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
                Action::Button(Some(pin), target) if pin.kind == PinnedKind::Group => {
                    if let Some(ids) = layout.pinned_groups.get(&pin.city_id) {
                        let members: Vec<_> = ids
                            .iter()
                            .filter_map(|id| {
                                self.units
                                    .iter()
                                    .position(|u| u.id == *id && u.team == PLAYER_TEAM)
                            })
                            .collect();
                        if !members.is_empty() {
                            self.group = members;
                            self.selected = None;
                            self.selected_city = None;
                            self.selected_barracks = None;
                            self.activate_target(target);
                        }
                    }
                }
                Action::Button(Some(pin), target) => self.activate_pinned(pin, target),
                Action::Button(None, target) => {
                    if matches!(target, Target::Scenario(_) | Target::LoadState) {
                        layout.clear_captured_panels();
                    }
                    self.activate_target(target)
                }
                Action::Reorder(Some(pin), kind, source, target) => {
                    self.reorder_pinned(pin, kind, source, target)
                }
                Action::Reorder(None, kind, source, target) => {
                    self.reorder_queue(kind, source, target)
                }
                Action::CreateBox => layout.create_box(viewport),
                Action::ToggleLayoutLayer => layout.editing_outer = !layout.editing_outer,
                Action::RemoveOuterBox(id) => layout.remove_outer_box(id),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        game.activate_pinned(pin, Target::Unit(UnitAction::Hold));
        assert!(game.units[index].holding);
    }

    #[test]
    fn pinned_city_action_uses_its_own_city_after_selection_closes() {
        let mut game = GameState::city_scenario();
        game.select_city();
        let pin = game.selected_pin().unwrap();
        game.leave_city_view();
        assert!(game.selected_city.is_none());
        game.activate_pinned(pin, Target::Build(BuildUnit::Melee));
        assert_eq!(game.selected_city, game.pinned_city_index(pin));
        assert!(!game.cities[game.selected_city.unwrap()].queue.is_empty());
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
    fn a_placed_panel_grows_by_its_title_bar_while_ctrl_shows_it() {
        let placed = WindowGeometry {
            size: Vec2::new(300.0, 120.0),
            manual: true,
            ..WindowGeometry::default()
        };
        // Ctrl pressed: the title bar appears, so the panel grows by it.
        assert_eq!(title_bar_change(placed, true, 26.0), Some(26.0));
        // Held: no further change once the title bar is showing.
        let showing = WindowGeometry {
            title_visible: true,
            ..placed
        };
        assert_eq!(title_bar_change(showing, true, 26.0), None);
        // Released: it shrinks back.
        assert_eq!(title_bar_change(showing, false, 26.0), Some(-26.0));
        // Automatically placed, docked or collapsed panels are sized elsewhere.
        for other in [
            WindowGeometry {
                manual: false,
                ..placed
            },
            WindowGeometry {
                docked: true,
                ..placed
            },
            WindowGeometry {
                collapsed: true,
                ..placed
            },
        ] {
            assert_eq!(title_bar_change(other, true, 26.0), None);
        }
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
            layout
                .size(SELECTION, Vec2::new(500.0, 250.0), viewport, false)
                .y,
            310.0
        );
        assert_eq!(
            layout
                .size(SELECTION, Vec2::new(500.0, 250.0), viewport, true)
                .y,
            340.0
        );
        assert_eq!(
            layout
                .size(QUEUE, Vec2::new(500.0, 225.0), viewport, false)
                .y,
            225.0
        );
        layout.windows[SELECTION].collapsed = true;
        assert_eq!(
            layout
                .size(SELECTION, Vec2::new(500.0, 250.0), viewport, false)
                .y,
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
        let mut sizes = [Some(Vec2::new(300.0, 200.0)), None, None, None, None];
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
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions.iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
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
            title_visible: false,
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
            Some(layout.size(SELECTION, Vec2::new(560.0, 200.0), viewport, false)),
            Some(Vec2::new(560.0, 180.0)),
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
            layout
                .size(QUEUE, Vec2::new(560.0, 225.0), viewport, false)
                .y,
            120.0
        );
        layout.windows[QUEUE].docked = false;
        assert_eq!(
            layout
                .size(QUEUE, Vec2::new(560.0, 225.0), viewport, false)
                .y,
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
        ];
        let positions = ImGuiLayoutState::default().plan(viewport, &mut sizes);
        assert!(positions[..3].iter().all(Option::is_some));
        assert_fits_without_overlap(viewport, sizes, positions);
    }
}
