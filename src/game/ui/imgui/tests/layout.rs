use super::*;
#[test]
fn the_status_bar_never_scrolls() {
    assert!(STATUS_FLAGS.contains(WindowFlags::NO_SCROLLBAR | WindowFlags::NO_SCROLL_WITH_MOUSE));
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

    layout.windows[SELECTION].place_by_player();
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
        // Put back to the size its content measures (`reset_size`).
        sized: false,
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
    // Each view's docking in the game window (#360): Default's with
    // Debug and Inspect as tabs on the right, City's with the tray over
    // its queue on the left; Troop follows Default's.
    let split = |vertical, sizes, a, b| DockTree::Split {
        vertical,
        sizes,
        parts: Box::new([a, b]),
    };
    let default_docking = split(
        false,
        [0.0, 330.5],
        DockTree::Map,
        DockTree::Panels(vec![DEBUG, INSPECT]),
    );
    let city_docking = split(
        false,
        [420.0, 0.0],
        split(
            true,
            [300.0, 120.0],
            DockTree::Panels(vec![SELECTION]),
            DockTree::Panels(vec![QUEUE]),
        ),
        DockTree::Map,
    );
    layout
        .view_docks
        .insert(ViewScope::Default, default_docking.clone());
    layout
        .view_docks
        .insert(ViewScope::City, city_docking.clone());

    let read = ImGuiLayoutState::from_text(&layout.to_text());
    let same = |a: &WindowGeometry, b: &WindowGeometry| {
        a.pos == b.pos
            && a.size == b.size
            && a.floating_pos == b.floating_pos
            && a.floating_size == b.floating_size
            && (a.manual, a.sized, a.docked, a.collapsed)
                == (b.manual, b.sized, b.docked, b.collapsed)
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
    assert_eq!(
        read.view_docks.get(&ViewScope::Default),
        Some(&default_docking)
    );
    assert_eq!(read.view_docks.get(&ViewScope::City), Some(&city_docking));
    assert!(!read.view_docks.contains_key(&ViewScope::Troop));
    assert_eq!(read.legacy_docking, None, "saved since #360");
    assert_eq!(
        read.docking_for_view(ViewScope::Troop).edge_of(DEBUG),
        Some(EdgeDock {
            side: ::imgui::sys::ImGuiDir_Right,
            extent: 330.5
        })
    );
    // Nothing docked anywhere is still saved as such.
    let empty = ImGuiLayoutState::from_text(&ImGuiLayoutState::default().to_text());
    assert_eq!(empty.legacy_docking, None);
    assert_eq!(empty.docking_for_view(ViewScope::City), DockTree::Map);
}

/// #360: a layout saved when only Debug's docking in the game window was
/// kept by view still loads: what ImGui's settings dock there is every
/// view's, with Debug where each view had it.
#[test]
fn a_layout_saved_before_every_view_kept_its_docking_still_loads() {
    let text = "debug_override city\ndebug_edge default right 300\ndebug_edge city left 250\n";
    let mut layout = ImGuiLayoutState::from_text(text);
    assert!(layout.view_docks.is_empty());
    // Saved again before a frame is drawn, it keeps the older lines.
    assert!(layout.to_text().contains("debug_edge city left 250\n"));
    // ImGui's settings had the turn strip docked at the bottom, and
    // Debug on the left (as City had it when the game closed).
    let settings = DockTree::Split {
        vertical: false,
        sizes: [250.0, 0.0],
        parts: Box::new([
            DockTree::Panels(vec![DEBUG]),
            DockTree::Split {
                vertical: true,
                sizes: [0.0, 90.0],
                parts: Box::new([DockTree::Map, DockTree::Panels(vec![UNITS])]),
            },
        ]),
    };
    let edges = layout.legacy_docking.take().expect("an older layout");
    layout.adopt_legacy_docking(&settings, &edges);
    let edge = |layout: &ImGuiLayoutState, view, slot| {
        layout
            .docking_for_view(view)
            .edge_of(slot)
            .map(|edge| (edge.side_text(), edge.extent))
    };
    for view in [ViewScope::Default, ViewScope::City, ViewScope::Troop] {
        assert_eq!(edge(&layout, view, UNITS), Some(("down", 90.0)), "{view:?}");
    }
    assert_eq!(
        edge(&layout, ViewScope::Default, DEBUG),
        Some(("right", 300.0))
    );
    assert_eq!(edge(&layout, ViewScope::City, DEBUG), Some(("left", 250.0)));
    // Troop followed Default's Debug, and follows Default's docking.
    assert!(!layout.view_docks.contains_key(&ViewScope::Troop));
    assert_eq!(
        edge(&layout, ViewScope::Troop, DEBUG),
        Some(("right", 300.0))
    );
}

#[test]
fn damaged_docking_lines_are_skipped() {
    let deep = format!("dock troop {}map", "split x 0 1 panels 1 0 ".repeat(40));
    for line in [
        "dock default split x 10 0 map map",
        "dock default split x 10 0 panels 1 2 panels 1 2",
        "dock default panels 1 9",
        "dock default split x nope 0 panels 1 2 map",
        "dock default split x 10 0 panels 1 2",
        "dock default map map",
        "dock nowhere map",
        &deep,
    ] {
        let layout = ImGuiLayoutState::from_text(line);
        assert!(layout.view_docks.is_empty(), "{line}");
    }
    let fine = ImGuiLayoutState::from_text("dock city split x 10 0 panels 2 2 3 map");
    assert_eq!(
        fine.view_docks.get(&ViewScope::City).map(|d| d.slots()),
        Some(vec![DEBUG, INSPECT])
    );
}

#[test]
fn a_reset_debug_panel_goes_where_its_view_would_put_it() {
    // No ImGui windows to move: the layout's half only.
    let _one = one_context_at_a_time();
    let moved = WindowGeometry {
        pos: Vec2::new(300.0, 300.0),
        size: Vec2::new(500.0, 500.0),
        floating_pos: Vec2::new(300.0, 300.0),
        floating_size: Vec2::new(500.0, 500.0),
        manual: true,
        sized: true,
        ..WindowGeometry::default()
    };
    // In Default: where and as big as the layout makes it.
    let mut layout = ImGuiLayoutState {
        debug_layout_scope: Some(BoxScope::View(ViewScope::Default)),
        ..ImGuiLayoutState::default()
    };
    layout.windows[DEBUG] = moved;
    layout.reset_position(DEBUG);
    assert!(!layout.windows[DEBUG].manual && layout.windows[DEBUG].sized);
    layout.reset_size(DEBUG);
    assert!(!layout.windows[DEBUG].sized);
    // In City / Building: where RESET would, Default's placement, and
    // once both are back it follows Default again.
    let default = WindowGeometry {
        floating_pos: Vec2::new(900.0, 80.0),
        floating_size: Vec2::new(330.0, 400.0),
        ..moved
    };
    layout
        .debug_view_geometry
        .insert(ViewScope::Default, default);
    layout.active_view = ViewScope::City;
    layout.debug_layout_scope = Some(BoxScope::View(ViewScope::City));
    layout.debug_view_overrides.insert(ViewScope::City);
    layout.windows[DEBUG] = moved;
    layout.reset_position(DEBUG);
    let debug = layout.windows[DEBUG];
    assert!(debug.manual);
    assert_eq!(debug.pos, default.floating_pos);
    assert_eq!(debug.floating_size, moved.floating_size, "size kept");
    assert!(layout.debug_view_overrides.contains(&ViewScope::City));
    layout.reset_size(DEBUG);
    assert_eq!(layout.windows[DEBUG].floating_size, default.floating_size);
    assert!(!layout.debug_view_overrides.contains(&ViewScope::City));
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
    // A file from before the sized flag: a panel the player placed was
    // sized by them too.
    let old = ImGuiLayoutState::from_text("window 0 1 2 3 4 5 6 7 8 1 0 0");
    assert!(old.windows[SELECTION].manual && old.windows[SELECTION].sized);
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
    layout.windows[SELECTION].place_by_player();
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
        sized: true,
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
        sized: true,
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
        sized: true,
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
