//! ImGui's own windows and dock nodes, through its C API (`imgui::sys`):
//! every `unsafe` of the presentation is here.

use super::*;

pub(super) fn native_window(title: &str) -> *mut ::imgui::sys::ImGuiWindow {
    let name = std::ffi::CString::new(title).expect("ImGui window title");
    unsafe { ::imgui::sys::igFindWindowByName(name.as_ptr()) }
}
/// Where `other` is docked beside `anchor`, when the two share a dock
/// node outside the game window's dockspace (`dockspace`, its id). Panels
/// docked in the game window stay where they are when they hide and show
/// again (ImGui keeps their empty node), and every view keeps its own
/// docking there (`update_game_dock`), so these relations leave them alone.
pub(super) fn dock_relation(
    anchor: &str,
    other: &str,
    dockspace: u32,
) -> Option<QueueDockRelation> {
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
        || in_dockspace(selection, dockspace)
        || in_dockspace(queue, dockspace)
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

pub(super) fn shared_dock_node(
    anchor: *mut ::imgui::sys::ImGuiDockNode,
    other: *mut ::imgui::sys::ImGuiDockNode,
) -> *mut ::imgui::sys::ImGuiDockNode {
    let mut node = anchor;
    while !node.is_null() && unsafe { !node_contains(node, other) } {
        node = unsafe { (*node).ParentNode };
    }
    node
}

pub(super) unsafe fn node_contains(
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

/// Whether `window` is docked in the dockspace `dockspace` (an id).
pub(super) fn in_dockspace(window: &::imgui::sys::ImGuiWindow, dockspace: u32) -> bool {
    let mut node = window.DockNode;
    if node.is_null() || dockspace == 0 {
        return false;
    }
    unsafe {
        while !(*node).ParentNode.is_null() {
            node = (*node).ParentNode;
        }
        (*node).ID == dockspace
    }
}

/// The id of the dock node `title`'s window is docked in, or goes back to
/// as it shows again (ImGui keeps a hidden window's node), or 0: for a
/// window not shown yet this session, where the saved settings
/// (`imgui.ini`) dock it, as ImGui reads them as it begins.
pub(super) fn panel_dock_id(title: &str) -> u32 {
    if unsafe { ::imgui::sys::igGetCurrentContext() }.is_null() {
        return 0;
    }
    let window = native_window(title);
    unsafe {
        if window.is_null() {
            let name = std::ffi::CString::new(title).expect("ImGui window title");
            let settings =
                ::imgui::sys::igFindWindowSettings(::imgui::sys::igImHashStr(name.as_ptr(), 0, 0));
            if settings.is_null() {
                0
            } else {
                (*settings).DockId
            }
        } else if (*window).DockNode.is_null() {
            (*window).DockId
        } else {
            (*(*window).DockNode).ID
        }
    }
}

/// Whether the dock node `id` is in the game window's dockspace
/// (`dockspace`, its id), and with `leaf`, a leaf of it, one that can hold
/// windows.
fn in_game_dock_node(id: u32, dockspace: u32, leaf: bool) -> bool {
    if id == 0 || dockspace == 0 {
        return false;
    }
    unsafe {
        let node = ::imgui::sys::igDockBuilderGetNode(id);
        if node.is_null() || (leaf && !(*node).ChildNodes[0].is_null()) {
            return false;
        }
        (*::imgui::sys::igDockNodeGetRootNode(node)).ID == dockspace
    }
}

/// Whether `title`'s window is docked in the game window (`dockspace`),
/// or goes back there as it shows again.
pub(super) fn in_game_dock(title: &str, dockspace: u32) -> bool {
    in_game_dock_node(panel_dock_id(title), dockspace, false)
}

/// Whether `title`'s window, hidden a while, goes back to its place docked
/// in the game window (`dockspace`) as it shows again: ImGui keeps its
/// empty node there and docks it back as it begins, unless it's given a
/// position, which would undock it.
pub(super) fn returns_to_game_dock(title: &str, dockspace: u32) -> bool {
    if dockspace == 0 {
        return false;
    }
    let window = native_window(title);
    if !window.is_null() && unsafe { !(*window).DockNode.is_null() } {
        return false;
    }
    in_game_dock_node(panel_dock_id(title), dockspace, true)
}

/// The game window's dockspace `dockspace` and its central node, the map,
/// if ImGui has them.
fn game_dock_nodes(
    dockspace: u32,
) -> Option<(
    *mut ::imgui::sys::ImGuiDockNode,
    *mut ::imgui::sys::ImGuiDockNode,
)> {
    if dockspace == 0 || unsafe { ::imgui::sys::igGetCurrentContext() }.is_null() {
        return None;
    }
    let root = unsafe { ::imgui::sys::igDockBuilderGetNode(dockspace) };
    if root.is_null() {
        return None;
    }
    let central = unsafe { (*root).CentralNode };
    (!central.is_null()).then_some((root, central))
}

/// How the panels are docked in the game window (`dockspace`), shown or
/// not (a hidden one keeps its node there), if ImGui has its dockspace.
pub(super) fn game_dock_tree(dockspace: u32) -> Option<DockTree> {
    let (root, central) = game_dock_nodes(dockspace)?;
    // Where each panel is docked, or goes back to.
    let homes: Vec<(u32, usize)> = slot_titles()
        .iter()
        .enumerate()
        .map(|(slot, title)| (panel_dock_id(title), slot))
        .filter(|&(id, _)| id != 0)
        .collect();
    Some(unsafe { dock_subtree(root, central, &homes) }.unwrap_or(DockTree::Map))
}

/// The docking under `node` (`game_dock_tree`): None where no panel is.
unsafe fn dock_subtree(
    node: *mut ::imgui::sys::ImGuiDockNode,
    central: *mut ::imgui::sys::ImGuiDockNode,
    homes: &[(u32, usize)],
) -> Option<DockTree> {
    let children = unsafe { (*node).ChildNodes };
    if children[0].is_null() || children[1].is_null() {
        if node == central {
            return Some(DockTree::Map);
        }
        let id = unsafe { (*node).ID };
        let slots: Vec<usize> = homes
            .iter()
            .filter(|&&(home, _)| home == id)
            .map(|&(_, slot)| slot)
            .collect();
        return (!slots.is_empty()).then_some(DockTree::Panels(slots));
    }
    let parts = children.map(|child| unsafe { dock_subtree(child, central, homes) });
    match parts {
        [Some(a), Some(b)] => {
            let vertical = unsafe { (*node).SplitAxis } == ::imgui::sys::ImGuiAxis_Y;
            // The size each asks for: a part beside the map keeps its own,
            // and the map takes the rest.
            let size = |child: *mut ::imgui::sys::ImGuiDockNode, part: &DockTree| {
                if part.has_map() {
                    0.0
                } else {
                    let size = unsafe { (*child).SizeRef };
                    if vertical { size.y } else { size.x }.round()
                }
            };
            Some(DockTree::Split {
                vertical,
                sizes: [size(children[0], &a), size(children[1], &b)],
                parts: Box::new([a, b]),
            })
        }
        [a, b] => a.or(b),
    }
}

/// Docks the panels in the game window (`dockspace`) as `tree` has them,
/// and takes any other window docked there out of it: at once for a panel
/// hidden or not shown yet, and as it begins for one shown. Done before
/// the dockspace is drawn, so ImGui lays the new nodes out that frame.
/// Whether it could (ImGui has the dockspace).
pub(super) fn build_game_dock(dockspace: u32, tree: &DockTree) -> bool {
    if game_dock_nodes(dockspace).is_none() {
        return false;
    }
    let always = ::imgui::sys::ImGuiCond_Always as i32;
    unsafe {
        let context = &*::imgui::sys::igGetCurrentContext();
        let windows = &context.Windows;
        let docked: Vec<_> = (0..windows.Size.max(0) as usize)
            .map(|i| *windows.Data.add(i))
            .filter(|&window| {
                let node = (*window).DockNode;
                let id = if node.is_null() {
                    (*window).DockId
                } else {
                    (*node).ID
                };
                in_game_dock_node(id, dockspace, false)
            })
            .collect();
        for window in docked {
            ::imgui::sys::igSetWindowDock(window, 0, always);
        }
        // Panels not shown yet: what the saved settings say.
        for title in slot_titles() {
            if !native_window(title).is_null() {
                continue;
            }
            let name = std::ffi::CString::new(title).expect("ImGui window title");
            let settings =
                ::imgui::sys::igFindWindowSettings(::imgui::sys::igImHashStr(name.as_ptr(), 0, 0));
            if !settings.is_null() && in_game_dock_node((*settings).DockId, dockspace, false) {
                (*settings).DockId = 0;
            }
        }
        ::imgui::sys::igDockBuilderRemoveNodeChildNodes(dockspace);
        // The dockspace is one node now, the map. Taking a panel out above
        // may have merged the map's own node into it, deleting that node
        // while ImGui's root still points at it (it looks again only as the
        // dockspace is drawn), and splitting reads that pointer.
        let root = ::imgui::sys::igDockBuilderGetNode(dockspace);
        let central = ::imgui::sys::ImGuiDockNodeFlags_CentralNode;
        (*root).CentralNode = root;
        (*root).LocalFlags |= central;
        (*root).MergedFlags |= central;
        build_dock_node(dockspace, tree);
        ::imgui::sys::igDockBuilderFinish(dockspace);
    }
    true
}

/// Splits the empty dock node `id` and docks panels in its parts as `tree`
/// has them (`build_game_dock`).
unsafe fn build_dock_node(id: u32, tree: &DockTree) {
    match tree {
        DockTree::Map => {}
        DockTree::Panels(slots) => {
            for &slot in slots {
                let name = std::ffi::CString::new(slot_titles()[slot]).expect("ImGui window title");
                unsafe { ::imgui::sys::igDockBuilderDockWindow(name.as_ptr(), id) };
            }
        }
        DockTree::Split {
            vertical,
            sizes,
            parts,
        } => {
            let node = unsafe { ::imgui::sys::igDockBuilderGetNode(id) };
            if node.is_null() {
                return;
            }
            let size = unsafe { (*node).Size };
            // Less the splitter between the parts (ImGui's
            // `DOCKING_SPLITTER_SIZE`).
            let room = (if *vertical { size.y } else { size.x } - 2.0).max(1.0);
            let map_first = parts[0].has_map();
            let first = if map_first {
                1.0 - sizes[1] / room
            } else if parts[1].has_map() {
                sizes[0] / room
            } else {
                sizes[0] / (sizes[0] + sizes[1]).max(1.0)
            }
            .clamp(0.05, 0.95);
            // The part split off is the one at the direction; the other
            // keeps what the node had, the map (its central node) with it.
            let (direction, share) = match (map_first, vertical) {
                (true, false) => (::imgui::sys::ImGuiDir_Right, 1.0 - first),
                (true, true) => (::imgui::sys::ImGuiDir_Down, 1.0 - first),
                (false, false) => (::imgui::sys::ImGuiDir_Left, first),
                (false, true) => (::imgui::sys::ImGuiDir_Up, first),
            };
            let (mut at_direction, mut opposite) = (0, 0);
            unsafe {
                ::imgui::sys::igDockBuilderSplitNode(
                    id,
                    direction,
                    share,
                    &mut at_direction,
                    &mut opposite,
                );
            }
            let (first_id, second_id) = if map_first {
                (opposite, at_direction)
            } else {
                (at_direction, opposite)
            };
            unsafe {
                build_dock_node(first_id, &parts[0]);
                build_dock_node(second_id, &parts[1]);
            }
        }
    }
}

/// Where `title`'s window is docked in the game window (`dockspace`), if
/// it is: the part of the dockspace holding it that sits beside the map,
/// which may hold other panels too.
#[cfg(test)]
pub(super) fn edge_dock(title: &str, dockspace: u32) -> Option<EdgeDock> {
    let (_, central) = game_dock_nodes(dockspace)?;
    let window = native_window(title);
    if window.is_null() || !in_dockspace(unsafe { &*window }, dockspace) {
        return None;
    }
    unsafe {
        let mut branch = (*window).DockNode;
        while !(*branch).ParentNode.is_null() && !node_contains((*branch).ParentNode, central) {
            branch = (*branch).ParentNode;
        }
        let parent = (*branch).ParentNode;
        if parent.is_null() || node_contains(branch, central) {
            return None;
        }
        let sibling = if (*parent).ChildNodes[0] == branch {
            (*parent).ChildNodes[1]
        } else {
            (*parent).ChildNodes[0]
        };
        if sibling.is_null() {
            return None;
        }
        let (branch, sibling) = (&*branch, &*sibling);
        Some(if (*parent).SplitAxis == ::imgui::sys::ImGuiAxis_X {
            EdgeDock {
                side: if branch.Pos.x < sibling.Pos.x {
                    ::imgui::sys::ImGuiDir_Left
                } else {
                    ::imgui::sys::ImGuiDir_Right
                },
                extent: branch.Size.x,
            }
        } else {
            EdgeDock {
                side: if branch.Pos.y < sibling.Pos.y {
                    ::imgui::sys::ImGuiDir_Up
                } else {
                    ::imgui::sys::ImGuiDir_Down
                },
                extent: branch.Size.y,
            }
        })
    }
}

/// Before `title`'s window begins with `flags`: whether this frame's click
/// is the second of a double-click on its title bar, which puts the panel
/// back where the layout would (`ImGuiLayoutState::reset_position`). ImGui
/// would collapse the window on it, when nothing on it is hovered: this
/// says something is, so the arrow alone collapses it.
pub(super) fn title_double_clicked(ui: &Ui, title: &str, flags: WindowFlags) -> bool {
    if flags.contains(WindowFlags::NO_TITLE_BAR) || !ui.is_mouse_double_clicked(ImMouseButton::Left)
    {
        return false;
    }
    let native = native_window(title);
    if native.is_null() {
        return false;
    }
    let mouse = ui.io().mouse_pos;
    // The title bar, as ImGui measures it (`TitleBarHeight`).
    let height = ui.frame_height();
    unsafe {
        let context = &*::imgui::sys::igGetCurrentContext();
        let window = &*native;
        let on_title = mouse[0] >= window.Pos.x
            && mouse[0] < window.Pos.x + window.Size.x
            && mouse[1] >= window.Pos.y
            && mouse[1] < window.Pos.y + height;
        // As ImGui tests it: docked windows keep their title in the tab bar,
        // and a hovered item (the collapse arrow) takes the click.
        if !window.DockNode.is_null()
            || context.HoveredWindow != native
            || context.HoveredId != 0
            || context.HoveredIdPreviousFrame != 0
            || !on_title
        {
            return false;
        }
        ::imgui::sys::igSetHoveredID(window.MoveId);
    }
    true
}

/// After `title`'s window ends: whether this frame's click is the second of
/// a double-click on its resize grip, which puts the panel back to the size
/// the layout gives it (`ImGuiLayoutState::reset_size`).
pub(super) fn grip_double_clicked(ui: &Ui, title: &str) -> bool {
    if !ui.is_mouse_double_clicked(ImMouseButton::Left) {
        return false;
    }
    let native = native_window(title);
    !native.is_null()
        && unsafe {
            (*native).DockNode.is_null()
                && ::imgui::sys::igGetHoveredID()
                    == ::imgui::sys::igGetWindowResizeCornerID(native, 0)
        }
}

/// Moves and sizes `title`'s window now, as far as given, so the next
/// frame reads it back (`sync_native_window`) where it was put.
pub(super) fn set_native_geometry(title: &str, pos: Option<Vec2>, size: Option<Vec2>) {
    if unsafe { ::imgui::sys::igGetCurrentContext() }.is_null() {
        return;
    }
    let native = native_window(title);
    if native.is_null() {
        return;
    }
    let always = ::imgui::sys::ImGuiCond_Always as i32;
    unsafe {
        if let Some(pos) = pos {
            ::imgui::sys::igSetWindowPos_WindowPtr(
                native,
                ::imgui::sys::ImVec2::new(pos.x, pos.y),
                always,
            );
        }
        if let Some(size) = size {
            ::imgui::sys::igSetWindowSize_WindowPtr(
                native,
                ::imgui::sys::ImVec2::new(size.x, size.y),
                always,
            );
        }
    }
}

/// Whether the item ImGui knows by `id` (a `###` name, as the buttons'
/// labels end: `LABEL###id`) had the cursor on it last frame. A label that
/// changes on hover is chosen before its button is drawn, when ImGui can't
/// yet say whether it's hovered this frame.
pub(super) fn hovered_last_frame(id: &str) -> bool {
    let Ok(name) = std::ffi::CString::new(format!("###{id}")) else {
        return false;
    };
    unsafe {
        let context = ::imgui::sys::igGetCurrentContext();
        let id = ::imgui::sys::igGetID_Str(name.as_ptr());
        !context.is_null() && id != 0 && (*context).HoveredIdPreviousFrame == id
    }
}

/// Runs `draw` with what the window draws clipped to `min`..`max` too.
pub(super) fn clipped_to(min: [f32; 2], max: [f32; 2], draw: impl FnOnce()) {
    unsafe {
        ::imgui::sys::igPushClipRect(
            ::imgui::sys::ImVec2::new(min[0], min[1]),
            ::imgui::sys::ImVec2::new(max[0], max[1]),
            true,
        )
    };
    draw();
    unsafe { ::imgui::sys::igPopClipRect() };
}

/// Tests only: docks the window titled `other` beside the one titled
/// `anchor` ("down": below it), with `share` of the pair's room, as a drop
/// there with the split then dragged would; ImGui does it as the next frame
/// begins. Both must have been drawn, `anchor` floating.
#[cfg(test)]
pub(in crate::game::ui) fn dock_beside(anchor: &str, other: &str, side: &str, share: f32) {
    let direction = match side {
        "left" => ::imgui::sys::ImGuiDir_Left,
        "right" => ::imgui::sys::ImGuiDir_Right,
        "up" => ::imgui::sys::ImGuiDir_Up,
        _ => ::imgui::sys::ImGuiDir_Down,
    };
    let (anchor, other) = (native_window(anchor), native_window(other));
    assert!(!anchor.is_null() && !other.is_null(), "both drawn");
    unsafe {
        ::imgui::sys::igDockContextQueueDock(
            ::imgui::sys::igGetCurrentContext(),
            anchor,
            (*anchor).DockNode,
            other,
            direction,
            split_ratio(direction, share),
            false,
        );
    }
}

/// Tests only: ImGui forgets where the window titled `title` was docked,
/// as it may for a hidden window whose split it drops.
#[cfg(test)]
pub(in crate::game::ui) fn forget_docking(title: &str) {
    let window = native_window(title);
    assert!(!window.is_null(), "{title} has been drawn");
    unsafe { ::imgui::sys::igSetWindowDock(window, 0, ::imgui::sys::ImGuiCond_Always as i32) };
}

/// Tests only: where the window titled `title` is, how big, and whether
/// it's collapsed, as ImGui has it.
#[cfg(test)]
pub(in crate::game::ui) fn native_geometry(title: &str) -> Option<(Vec2, Vec2, bool)> {
    let native = native_window(title);
    (!native.is_null()).then(|| unsafe {
        let window = &*native;
        (
            Vec2::new(window.Pos.x, window.Pos.y),
            Vec2::new(window.SizeFull.x, window.SizeFull.y),
            window.Collapsed,
        )
    })
}
