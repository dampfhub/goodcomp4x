//! Saving the layout between sessions.

use super::*;

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
        let chosen = |g: &WindowGeometry| g.manual || g.sized || g.docked;
        if chosen(&self.debug_outer_geometry) {
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
                && chosen(geometry)
            {
                lines.push(format!("debug_view {name} {}", geometry_text(geometry)));
            }
            if self.debug_view_overrides.contains(&view) {
                lines.push(format!("debug_override {name}"));
            }
            if let Some(id) = self.debug_view_boxes.get(&view) {
                lines.push(format!("debug_box {name} {id}"));
            }
            // Each view's docking in the game window (#360), Default's always,
            // so a layout saved since is never read as an older one. An older
            // layout never drawn keeps its lines for the session that draws it.
            if let Some(edges) = &self.legacy_docking {
                if let Some(edge) = edges.get(&view) {
                    lines.push(format!(
                        "debug_edge {name} {} {}",
                        edge.side_text(),
                        edge.extent
                    ));
                }
            } else if let Some(docking) = self
                .view_docks
                .get(&view)
                .or((view == ViewScope::Default).then_some(&DockTree::Map))
            {
                let mut words = Vec::new();
                docking.words(&mut words);
                lines.push(format!("dock {name} {}", words.join(" ")));
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
        // Debug's docking in the game window by view, as saved before each
        // view kept all of its docking.
        let mut legacy = std::collections::HashMap::new();
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
                "dock" => {
                    if let Some(view) = values.first().and_then(|v| view_from_text(v))
                        && let Some(docking) = DockTree::from_words(&values[1..])
                    {
                        layout.view_docks.insert(view, docking);
                    }
                }
                "debug_view" | "debug_override" | "debug_box" | "debug_edge" => {
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
                        "debug_edge" => {
                            if let (Some(side), Some(extent)) = (values.get(1), values.get(2))
                                && let Some(edge) = EdgeDock::from_text(side, extent)
                            {
                                legacy.insert(view, edge);
                            }
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
        // Saved before each view kept its docking, or never saved: what
        // ImGui's settings dock in the game window is every view's, with
        // Debug's by view (`adopt_legacy_docking`).
        if layout.view_docks.is_empty() {
            layout.legacy_docking = Some(legacy);
        }
        layout
    }
}

/// A panel's saved geometry: position, size, floating position and size,
/// whether it's placed by the player, docked and collapsed, and whether it's
/// sized by the player (last, so a file from before it still reads).
fn geometry_text(geometry: &WindowGeometry) -> String {
    let g = geometry;
    format!(
        "{} {} {} {} {} {} {} {} {} {} {} {}",
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
        u8::from(g.collapsed),
        u8::from(g.sized)
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
    // Before the sized flag, placing a panel sized it too.
    let sized = match values.get(11) {
        Some(value) => *value != "0",
        None => manual != 0.0,
    };
    Some(WindowGeometry {
        pos: Vec2::new(px, py),
        size: Vec2::new(sx, sy),
        floating_pos: Vec2::new(fpx, fpy),
        floating_size: Vec2::new(fsx, fsy),
        manual: manual != 0.0,
        sized,
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

pub(super) fn view_from_text(text: &str) -> Option<ViewScope> {
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
