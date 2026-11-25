//! The layout views (Default, City / Building, Troop) and what changes
//! with them: Debug's place, the game window's docking, and the dock
//! relations that put Selection, Queue and Debug back beside each other.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub(super) enum ViewScope {
    #[default]
    Default,
    City,
    Troop,
}

impl ViewScope {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Default => text!("view_default"),
            Self::City => text!("view_city"),
            Self::Troop => text!("view_troop"),
        }
    }
}

/// What the Selection panel is showing, by kind: each remembers its own
/// Debug docking beside Selection, and belongs to one layout view.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum SelectionKind {
    /// A city's interior map, open.
    Interior,
    City,
    Barracks,
    Unit,
    Group,
    /// The research view (`ui/research.rs`).
    Research,
}

impl SelectionKind {
    /// The name that starts its selection context ("city-3"), under which
    /// saved layouts keep its Selection geometry (`selection <name>` lines).
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Interior => "interior",
            Self::City => "city",
            Self::Barracks => "barracks",
            Self::Unit => "unit",
            Self::Group => "group",
            Self::Research => "research",
        }
    }

    /// The layout view it's shown in, whose reset (Ctrl+Shift+R) clears
    /// its Debug docking.
    pub(super) fn view(self) -> ViewScope {
        match self {
            Self::Interior | Self::City | Self::Barracks | Self::Research => ViewScope::City,
            Self::Unit | Self::Group => ViewScope::Troop,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoxScope {
    Outer,
    View(ViewScope),
}

impl BoxScope {
    /// The title of its box `number`: OUTER BOX 2, TROOP BOX 3.
    pub(super) fn box_name(self, number: u32) -> String {
        let view = match self {
            BoxScope::Outer => text!("view_outer"),
            BoxScope::View(view) => view.label(),
        };
        text!("view_box", view = view, number = number)
    }
}

pub(super) fn queue_dock_relation(dockspace: u32) -> Option<QueueDockRelation> {
    dock_relation(slot_titles()[SELECTION], slot_titles()[QUEUE], dockspace)
}

impl ImGuiLayoutState {
    pub fn request_reset_active_view(&mut self) -> bool {
        if self.editing_outer || self.active_view == ViewScope::Default {
            return false;
        }
        self.pending_view_reset = Some(self.active_view);
        true
    }

    pub(super) fn clear_view_debug_override(&mut self, view: ViewScope) {
        self.debug_view_overrides.remove(&view);
        self.debug_view_geometry.remove(&view);
        self.debug_view_boxes.remove(&view);
        // And its docking in the game window, every panel's
        // (`update_game_dock` docks them as Default's).
        self.view_docks.remove(&view);
        // Every kind of selection shown in the view: in City / Building,
        // an open interior as well as a city and a Barracks.
        self.debug_context_relations
            .retain(|kind, _| kind.view() != view);
    }

    pub(super) fn apply_pending_view_reset(&mut self) {
        let Some(view) = self.pending_view_reset.take() else {
            return;
        };
        self.clear_view_debug_override(view);
        if self.active_view != view || self.debug_layout_scope != Some(BoxScope::View(view)) {
            return;
        }
        // Docked in the game window as Default's docking has it (built as
        // the frame began), or out of any dock.
        let kept = self.release_debug_for_view(false);
        let mut inherited = self.debug_geometry_for_view(view);
        inherited.docked = false;
        inherited.pos = inherited.floating_pos;
        inherited.size = inherited.floating_size;
        self.windows[DEBUG] = inherited;
        self.pending_debug_box = None;
        self.debug_attached = false;
        self.debug_restore_attempts = 0;
        self.reposition[DEBUG] = !kept;
    }

    /// How the panels dock in the game window in `view`: its own docking,
    /// or Default's while it follows Default's (City / Building and Troop,
    /// until the player changes theirs); nothing docked there before the
    /// player docks something.
    pub(super) fn docking_for_view(&self, view: ViewScope) -> DockTree {
        self.view_docks
            .get(&view)
            .or_else(|| self.view_docks.get(&ViewScope::Default))
            .cloned()
            .unwrap_or(DockTree::Map)
    }

    /// The docking the game window gets for `view`: the view's, less any
    /// panel a box shown in the view holds, or Debug when it goes in one
    /// of the view's boxes (`pending_debug_box`): switching views takes no
    /// panel out of a box.
    fn docking_to_build(&self, view: ViewScope) -> DockTree {
        let mut docking = self.docking_for_view(view);
        for (slot, title) in slot_titles().iter().enumerate() {
            let boxed = self.in_box_of_view(title, view)
                || (slot == DEBUG
                    && !self.editing_outer
                    && self.debug_view_boxes.contains_key(&view));
            if boxed && let Some(without) = docking.without(slot) {
                docking = without;
            }
        }
        docking
    }

    /// As each frame begins, before the game window's dockspace is drawn:
    /// notes how the panels are docked there, taking a change from how the
    /// layout left them as the player's, for the view shown (while
    /// arranging the boxes every view shares, for every view); and when the
    /// view is another than the one shown, or RESET asks for the view's
    /// default, docks them there as `view`'s layout has them.
    pub(super) fn update_game_dock(&mut self, view: ViewScope) {
        let dockspace = self.game_dock_id;
        let Some(now) = game_dock_tree(dockspace) else {
            return;
        };
        if let Some(edges) = self.legacy_docking.take() {
            self.adopt_legacy_docking(&now, &edges);
        }
        // Docked in the game window: dragged out later, a panel stays where
        // it's dropped.
        for (slot, title) in slot_titles().iter().enumerate() {
            if window_in_dockspace(title, dockspace) {
                self.reposition[slot] = false;
            }
        }
        // While a panel is dragged, ImGui has the docking.
        if self.defer_geometry {
            return;
        }
        if self.dock_settling > 0 {
            self.dock_settling -= 1;
            self.dock_now = Some(now.clone());
        } else if self
            .dock_now
            .as_ref()
            .is_none_or(|seen| !seen.matches(&now))
        {
            if self.dock_now.is_some() {
                self.dock_changed_by_player(&now);
            }
            self.dock_now = Some(now.clone());
        }
        let reset = self.pending_view_reset == Some(view);
        if reset {
            self.view_docks.remove(&view);
        }
        if self.dock_view == Some(view) && !reset {
            return;
        }
        let wanted = self.docking_to_build(view);
        if !wanted.matches(&now) {
            let before = now.slots();
            if !build_game_dock(dockspace, &wanted) {
                return;
            }
            // Taken out of the game window: back where it last floated.
            for slot in before {
                if wanted.contains(slot) {
                    continue;
                }
                let mut geometry = self.floating_before[slot];
                if geometry.size == Vec2::ZERO {
                    geometry = self.windows[slot];
                    geometry.manual = false;
                    geometry.sized = false;
                }
                geometry.docked = false;
                self.windows[slot] = geometry;
                self.reposition[slot] = true;
            }
            self.dock_now = Some(wanted);
            self.dock_settling = 2;
        }
        self.dock_view = Some(view);
    }

    /// The player docked, moved or resized panels in the game window, or
    /// took one out: `now` is the docking of the view shown, or while
    /// arranging the boxes every view shares, of every view. In City /
    /// Building or Troop, Debug docked or taken out there makes the view's
    /// Debug its own, as moving it does.
    fn dock_changed_by_player(&mut self, now: &DockTree) {
        let Some(view) = self.dock_view else {
            return;
        };
        if self.editing_outer {
            self.view_docks.clear();
            self.view_docks.insert(ViewScope::Default, now.clone());
            return;
        }
        let before = self.docking_for_view(view).edge_of(DEBUG);
        if view != ViewScope::Default && !EdgeDock::matches(before, now.edge_of(DEBUG)) {
            self.debug_view_overrides.insert(view);
        }
        self.view_docks.insert(view, now.clone());
    }

    /// A layout saved before each view kept its docking (#360), joined with
    /// what ImGui's settings dock in the game window (`now`): that was
    /// every view's, but for Debug's place, which was kept by view
    /// (`edges`) in Default and in the views with their own Debug.
    pub(super) fn adopt_legacy_docking(
        &mut self,
        now: &DockTree,
        edges: &std::collections::HashMap<ViewScope, EdgeDock>,
    ) {
        let shared = now.without(DEBUG).unwrap_or(DockTree::Map);
        for view in [ViewScope::Default, ViewScope::City, ViewScope::Troop] {
            if view != ViewScope::Default && !self.debug_view_overrides.contains(&view) {
                continue;
            }
            let docking = match edges.get(&view) {
                Some(&edge) => shared.clone().beside_map(DEBUG, edge),
                None => shared.clone(),
            };
            self.view_docks.insert(view, docking);
        }
    }

    /// Takes Debug out of the dock it's in, if any, unless that's the game
    /// window (where the view's docking has it, `update_game_dock`) or,
    /// with `keep_shared`, a box every view shares. Returns whether it
    /// stays docked in the game window.
    fn release_debug_for_view(&self, keep_shared: bool) -> bool {
        if in_game_dock(slot_titles()[DEBUG], self.game_dock_id) {
            return true;
        }
        let shared = keep_shared
            && self.outer_boxes.iter().any(|b| {
                b.scope == BoxScope::Outer
                    && Some(b.id) == self.outer_box_for_window(slot_titles()[DEBUG])
            });
        if !shared && docked(slot_titles()[DEBUG]) {
            undock(slot_titles()[DEBUG]);
        }
        false
    }

    pub(super) fn debug_geometry_for_view(&self, view: ViewScope) -> WindowGeometry {
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

    pub(super) fn switch_layout_scope(&mut self, view: ViewScope) {
        let previous_view = self.active_view;
        let old_debug_box = self.outer_box_for_window(slot_titles()[DEBUG]);
        let old_scope = self.debug_layout_scope;
        let desired = if self.editing_outer
            || self
                .outer_box_for_window(slot_titles()[DEBUG])
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
        let view_box = match desired {
            BoxScope::View(view) => self.debug_view_boxes.get(&view).copied(),
            BoxScope::Outer => None,
        };
        // Docked in the game window where the view's docking has it (#333,
        // #360: `update_game_dock`, which leaves it out when it goes in one
        // of the view's boxes), or else out of the dock it was in, unless
        // that's a box every view shares.
        let kept = self.release_debug_for_view(true);
        let mut restored = match desired {
            BoxScope::Outer => self.debug_outer_geometry,
            BoxScope::View(view) => self.debug_geometry_for_view(view),
        };
        restored.docked = false;
        if restored.floating_pos != Vec2::ZERO {
            restored.pos = restored.floating_pos;
            restored.size = restored.floating_size;
            restored.place_by_player();
        }
        self.windows[DEBUG] = restored;
        self.reposition[DEBUG] = !kept;
        self.debug_layout_scope = Some(desired);
        self.pending_debug_box = view_box;
    }

    fn release_debug_from_context(&mut self) {
        let debug = slot_titles()[DEBUG];
        if !docked(debug) {
            return;
        }
        if let Some((pin, relation)) = self.debug_outer_relation
            && self.pinned.contains(&pin)
            && queue_dock(
                &pin.title(),
                debug,
                None,
                relation.direction,
                split_ratio(relation.direction, relation.fraction),
            )
        {
            return;
        }
        undock(debug);
        self.reposition[DEBUG] = true;
    }

    pub(super) fn maintain_debug_scope(&mut self, context: Option<SelectionKind>) {
        if self.outer_box_for_window(slot_titles()[DEBUG]).is_some() {
            self.debug_context_relations.clear();
            self.debug_context = context;
            self.debug_attached = false;
            self.debug_restore_attempts = 0;
            return;
        }
        if self.debug_context != context {
            if self.debug_attached {
                self.release_debug_from_context();
            }
            self.debug_attached = false;
            self.debug_context = context;
            self.debug_restore_attempts = context
                .filter(|kind| self.debug_context_relations.contains_key(kind))
                .map_or(0, |_| 2);
            return;
        }
        let Some(context) = context else {
            self.debug_outer_relation = self.pinned.iter().find_map(|pin| {
                dock_relation(&pin.title(), slot_titles()[DEBUG], self.game_dock_id)
                    .map(|relation| (*pin, relation))
            });
            return;
        };
        if let Some(relation) = dock_relation(
            slot_titles()[SELECTION],
            slot_titles()[DEBUG],
            self.game_dock_id,
        ) {
            self.debug_context_relations.insert(context, relation);
            self.debug_restore_attempts = 0;
            self.debug_attached = true;
            return;
        }
        if self.debug_restore_attempts > 0 {
            // A view reset (Ctrl+Shift+R) may have forgotten the relation
            // since the restoring began: then there's nothing to restore.
            let Some(relation) = self.debug_context_relations.get(&context).copied() else {
                self.debug_restore_attempts = 0;
                return;
            };
            // Docked in the game window, Debug stays where the view has it
            // there (`update_game_dock`), and a Selection docked there
            // takes no Debug beside it this way.
            let in_game_window = |title: &str| window_in_dockspace(title, self.game_dock_id);
            if in_game_window(slot_titles()[DEBUG]) || in_game_window(slot_titles()[SELECTION]) {
                self.debug_restore_attempts = 0;
                return;
            }
            if queue_dock(
                slot_titles()[SELECTION],
                slot_titles()[DEBUG],
                relation.anchor_group.then_some(slot_titles()[QUEUE]),
                relation.direction,
                split_ratio(relation.direction, relation.fraction),
            ) {
                self.debug_restore_attempts -= 1;
            }
        } else if self.debug_attached {
            // An explicit undock while this menu is open replaces its remembered layout.
            self.debug_context_relations.remove(&context);
            self.debug_attached = false;
        }
    }

    pub(super) fn maintain_queue_dock(&mut self, pair_visible: bool) {
        if !pair_visible {
            self.pair_visible = false;
            return;
        }
        if !self.pair_visible && self.queue_dock_relation.is_some() {
            self.queue_restore_attempts = 2;
        }
        self.pair_visible = true;
        if let Some(relation) = queue_dock_relation(self.game_dock_id) {
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
        let (selection, queue) = (slot_titles()[SELECTION], slot_titles()[QUEUE]);
        let (Some(selection_window), Some(queue_window)) =
            (native_state(selection), native_state(queue))
        else {
            return;
        };
        // Either docked in the game window: ImGui puts them back there.
        if window_in_dockspace(selection, self.game_dock_id)
            || window_in_dockspace(queue, self.game_dock_id)
        {
            self.queue_restore_attempts = 0;
            return;
        }
        let queue_has_node = queue_window.docked;
        let selection_has_node = selection_window.docked;
        let (target, payload, direction, share) = if queue_has_node && !selection_has_node {
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
        queue_dock(
            target,
            payload,
            None,
            direction,
            split_ratio(direction, share),
        );
        self.queue_restore_attempts -= 1;
    }
}

impl GameState {
    /// What the Selection panel shows, if anything: its kind, and its
    /// context, the kind's name and which one ("city-3"; a group has no
    /// number).
    pub(super) fn selection_kind(&self) -> Option<(SelectionKind, String)> {
        let (kind, which) = if let Some(city) = self.interior_view {
            (SelectionKind::Interior, Some(city))
        } else if let Some(city) = self.selected_city {
            (SelectionKind::City, Some(city))
        } else if let Some(city) = self.selected_barracks {
            (SelectionKind::Barracks, Some(city))
        } else if let Some(unit) = self.selected {
            (SelectionKind::Unit, Some(self.units[unit].id as usize))
        } else if !self.group.is_empty() {
            (SelectionKind::Group, None)
        } else if self.research_view() {
            (SelectionKind::Research, None)
        } else {
            return None;
        };
        let context = match which {
            Some(which) => format!("{}-{which}", kind.name()),
            None => kind.name().into(),
        };
        Some((kind, context))
    }

    /// The layout view: the one the selection's kind belongs to, or
    /// Default.
    pub(super) fn layout_view(&self) -> ViewScope {
        self.selection_kind()
            .map_or(ViewScope::Default, |(kind, _)| kind.view())
    }
}

/// Whether `title`'s window is docked in the game window (`dockspace`),
/// or goes back there as it shows again.
fn in_game_dock(title: &str, dockspace: u32) -> bool {
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
    if docked(title) {
        return false;
    }
    in_game_dock_node(panel_dock_id(title), dockspace, true)
}
