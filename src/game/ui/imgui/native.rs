//! ImGui's own windows and dock nodes, through its C API (`imgui::sys`):
//! every `unsafe` of the presentation is here.

use super::*;

fn native_window(title: &str) -> *mut ::imgui::sys::ImGuiWindow {
    let name = std::ffi::CString::new(title).expect("ImGui window title");
    unsafe { ::imgui::sys::igFindWindowByName(name.as_ptr()) }
}

/// A window as ImGui has it (`native_state`).
#[derive(Clone, Copy)]
pub(super) struct NativeState {
    /// In a dock node.
    pub(super) docked: bool,
    pub(super) collapsed: bool,
    pub(super) pos: Vec2,
    /// Its size uncollapsed.
    pub(super) size: Vec2,
}

/// `title`'s window as ImGui has it, if it has been drawn.
pub(super) fn native_state(title: &str) -> Option<NativeState> {
    let native = native_window(title);
    (!native.is_null()).then(|| {
        let native = unsafe { &*native };
        NativeState {
            docked: !native.DockNode.is_null(),
            collapsed: native.Collapsed,
            pos: Vec2::new(native.Pos.x, native.Pos.y),
            size: Vec2::new(native.SizeFull.x, native.SizeFull.y),
        }
    })
}

/// Whether `title`'s window has been drawn and is in a dock node.
pub(super) fn docked(title: &str) -> bool {
    native_state(title).is_some_and(|window| window.docked)
}

/// Whether `title`'s window is docked in the dockspace `dockspace` (an id).
pub(super) fn window_in_dockspace(title: &str, dockspace: u32) -> bool {
    let window = native_window(title);
    !window.is_null() && in_dockspace(unsafe { &*window }, dockspace)
}

/// Whether `title`'s window is docked in the dock node `root` (an id) or a
/// node under it.
pub(super) fn window_in_dock(title: &str, root: u32) -> bool {
    let window = native_window(title);
    if window.is_null() {
        return false;
    }
    unsafe {
        let node = (*window).DockNode;
        let root = ::imgui::sys::igDockBuilderGetNode(root);
        !node.is_null() && !root.is_null() && node_contains(root, node)
    }
}

/// Whether the dock node `title`'s window is docked in, or goes back to
/// (`panel_dock_id`), is the node `root` (an id) or under it.
pub(super) fn panel_in_dock(title: &str, root: u32) -> bool {
    unsafe {
        let node = ::imgui::sys::igDockBuilderGetNode(panel_dock_id(title));
        let root = ::imgui::sys::igDockBuilderGetNode(root);
        !node.is_null() && !root.is_null() && node_contains(root, node)
    }
}

/// Asks ImGui to take `title`'s window out of its dock as the next frame
/// begins, if it has been drawn.
pub(super) fn undock(title: &str) {
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

/// Asks ImGui to dock `payload`'s window beside `target`'s at `direction`,
/// the split's first part taking `ratio` (`split_ratio`), as a drop there
/// would, as the next frame begins: beside `target`'s dock node, or with
/// `group`, beside the node `target` shares with `group`'s window. Whether
/// both windows have been drawn, so it could.
pub(super) fn queue_dock(
    target: &str,
    payload: &str,
    group: Option<&str>,
    direction: ::imgui::sys::ImGuiDir,
    ratio: f32,
) -> bool {
    let (target, payload) = (native_window(target), native_window(payload));
    if target.is_null() || payload.is_null() {
        return false;
    }
    unsafe {
        let mut node = (*target).DockNode;
        if let Some(group) = group {
            let other = native_window(group);
            if !other.is_null() && !(*other).DockNode.is_null() {
                node = shared_dock_node(node, (*other).DockNode);
            }
        }
        ::imgui::sys::igDockContextQueueDock(
            ::imgui::sys::igGetCurrentContext(),
            target,
            node,
            payload,
            direction,
            ratio,
            false,
        );
    }
    true
}

/// Docks the next window begun in the dock node `id`.
pub(super) fn set_next_dock_id(id: u32) {
    unsafe { ::imgui::sys::igSetNextWindowDockID(id, ::imgui::sys::ImGuiCond_Always as i32) };
}

/// Whether the window being drawn is docked.
pub(super) fn current_window_docked() -> bool {
    unsafe { ::imgui::sys::igIsWindowDocked() }
}

/// The id ImGui gives `name` in the window being drawn.
pub(super) fn item_id(name: &std::ffi::CStr) -> u32 {
    unsafe { ::imgui::sys::igGetID_Str(name.as_ptr()) }
}

/// A dockspace, the dock node `id`, filling the rest of the window being
/// drawn, with `flags` (`ImGuiDockNodeFlags`).
pub(super) fn dockspace(id: u32, flags: i32) {
    unsafe {
        ::imgui::sys::igDockSpace(
            id,
            ::imgui::sys::ImVec2 { x: 0.0, y: 0.0 },
            flags,
            std::ptr::null(),
        )
    };
}

/// Runs `draw` with the window's draw list filling shapes unfeathered
/// (`fill_shapes`).
pub(super) fn unfeathered(draw: impl FnOnce()) {
    let list = unsafe { ::imgui::sys::igGetWindowDrawList() };
    let flags = unsafe { (*list).Flags };
    let feathered = ::imgui::sys::ImDrawListFlags_AntiAliasedFill as ::imgui::sys::ImDrawListFlags;
    unsafe { (*list).Flags = flags & !feathered };
    draw();
    unsafe { (*list).Flags = flags };
}

/// Tests only: the panel (root window) being drawn, and the corners of its
/// draw list's clip rectangle now (`note_mark`).
#[cfg(test)]
pub(super) fn current_clip() -> (String, [f32; 2], [f32; 2]) {
    unsafe {
        let window = ::imgui::sys::igGetCurrentWindow();
        let root = (*window).RootWindow;
        let name = std::ffi::CStr::from_ptr((*root).Name)
            .to_string_lossy()
            .into_owned();
        let list = ::imgui::sys::igGetWindowDrawList();
        let mut min = ::imgui::sys::ImVec2::zero();
        let mut max = ::imgui::sys::ImVec2::zero();
        ::imgui::sys::ImDrawList_GetClipRectMin(&mut min, list);
        ::imgui::sys::ImDrawList_GetClipRectMax(&mut max, list);
        (name, [min.x, min.y], [max.x, max.y])
    }
}

/// Tests only: sizes `title`'s window `size` at once, as dragging its grip
/// would (`resize_panel`). It must have been drawn.
#[cfg(test)]
pub(super) fn set_full_size(title: &str, size: Vec2) {
    let native = native_window(title);
    assert!(!native.is_null(), "{title} has been drawn");
    let full = ::imgui::sys::ImVec2::new(size.x, size.y);
    unsafe {
        (*native).SizeFull = full;
        (*native).Size = full;
    }
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

/// Whether `window` is docked in the dockspace `dockspace` (an id).
fn in_dockspace(window: &::imgui::sys::ImGuiWindow, dockspace: u32) -> bool {
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
pub(super) fn in_game_dock_node(id: u32, dockspace: u32, leaf: bool) -> bool {
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

/// Tests only: ImGui forgets where the window titled `title` was docked,
/// as it may for a hidden window whose split it drops.
#[cfg(test)]
pub(in crate::game::ui) fn forget_docking(title: &str) {
    let window = native_window(title);
    assert!(!window.is_null(), "{title} has been drawn");
    unsafe { ::imgui::sys::igSetWindowDock(window, 0, ::imgui::sys::ImGuiCond_Always as i32) };
}
