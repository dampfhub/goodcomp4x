//! Screen-space UI: the top status bar, the bottom-left command tray (the
//! selected unit's stats and actions, or the open city's economy and build
//! options), tooltips for every button, and an info box for a hovered unit.
//!
//! Each frame the UI is laid out once into panels, text and buttons (a
//! `Layout`), which is then either drawn or hit-tested against a click, so
//! what's clickable always matches what's shown. New persistent or hover
//! panels should use `PanelBuilder` and `Layout::dock_panel` with a `Zone`;
//! the reusable geometry is in `ui/dock.rs` and explained in
//! `docs/ui-system.md`. UI geometry is in pixels
//! with the origin at the window's bottom-left and Y pointing up, the same
//! orientation as the world.
//!
//! This file holds the shared constants and types, `Layout`, and the entry
//! points (`build_ui`, `click_ui`, `update_hover`, `layout`). The rest:
//! `builder.rs` (`PanelBuilder`), `paint.rs` (shapes to vertices), `trays.rs`
//! (unit, group, city and Barracks trays), `panels.rs` (top bar, debug panel,
//! structure hover panel), `queue.rs` (queue panels, scrolling and dragging),
//! `tooltips.rs`, `text.rs` (number and text formatting), `tests.rs`.

mod builder;
mod dock;
mod paint;
mod panels;
mod queue;
mod text;
mod tooltips;
mod trays;

use glam::{Mat4, Vec2, Vec3};

use super::city::{BuildUnit, Building, LaborFocus};
use super::hex::Hex;
use super::scenario::Scenario;
use super::{GameState, mesh};
use crate::renderer::Vertex;
use builder::PanelBuilder;
use dock::{Dock, Rect, Zone};
use paint::{draw_button, draw_shape};
use queue::queue_items_that_fit;

type Color = [f32; 4];
/// A line of text made of differently colored spans, drawn left to right.
type Line = Vec<(String, Color)>;

/// Text sizes in pixels, each one of `font::UI_SIZES`.
const SMALL: u32 = 15;
const BODY: u32 = 18;
const TITLE: u32 = 22;

/// Gap between panels and the window's edges.
const MARGIN: f32 = 18.0;
/// Space between a panel's border and its contents.
const PADDING: f32 = 16.0;
/// Extra space between consecutive lines of text.
const LINE_GAP: f32 = 4.0;
/// Space between groups of rows in a panel, and between buttons.
const GAP: f32 = 10.0;
const BORDER: f32 = 2.0;
const ARMED_BORDER: f32 = 3.0;

const TOP_BAR_HEIGHT: f32 = 52.0;
const END_TURN_HEIGHT: f32 = 38.0;
const BUTTON_HEIGHT: f32 = 60.0;
const BUTTON_MIN_WIDTH: f32 = 120.0;
const BUTTON_PADDING: f32 = 16.0;
const GROWTH_BAR_HEIGHT: f32 = 10.0;
const QUEUE_TOP_GAP: f32 = 4.0;
const SCROLLBAR_WIDTH: f32 = 12.0;
const QUEUE_ITEM_HEIGHT: f32 = 48.0;
const QUEUE_ITEM_GAP: f32 = 4.0;
const QUEUE_REMOVE_WIDTH: f32 = 34.0;
/// Tooltip descriptions wrap at this many characters.
const TOOLTIP_WRAP: usize = 50;
const TOOLTIP_GAP: f32 = 8.0;
/// Seconds the cursor rests on a map hex before its tooltip appears.
const TILE_TOOLTIP_DELAY: f32 = 0.75;
/// Where the tile tooltip's top-left corner sits relative to the cursor.
const TILE_TOOLTIP_OFFSET: Vec2 = Vec2::new(20.0, -20.0);

// The swapchain is sRGB, so these linear colors display much lighter than
// their values suggest.
const BORDER_COLOR: Color = [0.30, 0.32, 0.38, 1.0];
const ARMED_BORDER_COLOR: Color = [1.00, 0.92, 0.55, 1.0];
const PANEL_BG: Color = [0.012, 0.014, 0.02, 0.94];
const BUTTON_BG: Color = [0.03, 0.035, 0.05, 0.95];
const BUTTON_HOVER_BG: Color = [0.08, 0.09, 0.13, 0.95];
const QUEUED_BG: Color = [0.78, 0.64, 0.20, 0.95];
const QUEUED_HOVER_BG: Color = [0.90, 0.76, 0.30, 0.95];
const DISABLED_BG: Color = [0.02, 0.02, 0.025, 0.95];
const BAR_BG: Color = [0.01, 0.02, 0.02, 1.0];
const GROWTH_COLOR: Color = [0.30, 0.88, 0.35, 1.0];
const TEXT: Color = [0.95, 0.95, 0.95, 1.0];
const QUEUED_TEXT: Color = [0.08, 0.07, 0.04, 1.0];
const QUEUED_HINT_TEXT: Color = [0.25, 0.20, 0.08, 1.0];
const DISABLED_TEXT: Color = [0.18, 0.18, 0.20, 1.0];
const DIM_TEXT: Color = [0.55, 0.55, 0.60, 1.0];
const LABEL_TEXT: Color = [0.30, 0.32, 0.38, 1.0];
const NOTICE_TEXT: Color = [0.95, 0.85, 0.55, 1.0];
const GOLD_TEXT: Color = [0.95, 0.80, 0.35, 1.0];
const BOOSTED_TEXT: Color = [0.55, 0.92, 0.50, 1.0];
const REDUCED_TEXT: Color = [0.98, 0.52, 0.42, 1.0];
const SELECTION_BOX_FILL: Color = [0.30, 0.55, 0.95, 0.12];
const SELECTION_BOX_EDGE: Color = [0.55, 0.75, 1.00, 0.9];
const SELECTION_BOX_BORDER: f32 = 2.0;
/// Opacity the debug panel is drawn at, so it doesn't read as game UI.
const DEBUG_ALPHA: f32 = 0.55;

/// Maps UI pixels (origin bottom-left, Y up) to clip space.
pub fn ui_projection(screen_size: Vec2) -> Mat4 {
    Mat4::from_translation(Vec3::new(-1.0, 1.0, 0.0))
        * Mat4::from_scale(Vec3::new(2.0 / screen_size.x, -2.0 / screen_size.y, 1.0))
}

/// While Escape is held: a "hold to quit" prompt under the top bar with a
/// bar filling toward `progress` = 1, when the game closes.
pub fn quit_prompt(progress: f32, screen_size: Vec2) -> Vec<Vertex> {
    let mut panel = PanelBuilder::default();
    panel.text(BODY, vec![("HOLD ESC TO QUIT".into(), TEXT)]);
    panel.bar(progress);
    let size = panel.size();
    let top_left = Vec2::new(
        (screen_size.x - size.x) / 2.0,
        screen_size.y - TOP_BAR_HEIGHT - MARGIN,
    );
    let mut layout = Layout::default();
    panel.place_top_left(top_left, &mut layout);
    let mut out = Vec::new();
    for shape in &layout.shapes {
        draw_shape(shape, &mut out);
    }
    out
}

/// While Alt-dragging: the selection rectangle between `a` and `b` (window
/// pixels, origin top-left), a translucent fill with a thin border.
pub fn selection_box(a: Vec2, b: Vec2, screen_size: Vec2) -> Vec<Vertex> {
    let (a, b) = (to_ui(a, screen_size), to_ui(b, screen_size));
    let (min, max) = (a.min(b).round(), a.max(b).round());
    let mut out = Vec::new();
    mesh::quad(min, max, SELECTION_BOX_FILL, &mut out);
    let edge = Vec2::splat(SELECTION_BOX_BORDER);
    let edges = [
        (min, Vec2::new(max.x, min.y) + edge),
        (Vec2::new(min.x, max.y) - edge, max),
        (min, Vec2::new(min.x, max.y) + edge),
        (Vec2::new(max.x, min.y) - edge, max),
    ];
    for (from, to) in edges {
        mesh::quad(from, to, SELECTION_BOX_EDGE, &mut out);
    }
    out
}

/// Something a button does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Target {
    Unit(UnitAction),
    Build(BuildUnit),
    ToggleYields,
    Building(Building),
    BarracksBuild(BuildUnit),
    OpenBarracks,
    OpenCity,
    CityQueueRemove(usize),
    BarracksQueueRemove(usize),
    Focus(LaborFocus),
    ConfirmBuilding(Building),
    EndTurn,
    /// Debug panel: scenario pages, the savestate, playback pacing and fog.
    Scenario(Scenario),
    SaveState,
    LoadState,
    CompleteProduction,
    TogglePlayback,
    ToggleFog,
}

/// An order for the selected unit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum UnitAction {
    Move,
    Attack,
    Swap,
    Ability,
    Hold,
    Guard,
    Settle,
    Road,
    Improve,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ButtonState {
    Ready,
    /// Already queued (or, for End Turn, ready to go).
    Queued,
    /// Not possible right now.
    Disabled,
}

impl ButtonState {
    fn new(queued: bool, disabled: bool) -> Self {
        if disabled {
            ButtonState::Disabled
        } else if queued {
            ButtonState::Queued
        } else {
            ButtonState::Ready
        }
    }
}

struct Button {
    target: Target,
    label: String,
    /// The keyboard shortcut or cost, shown under the label (or beside it on
    /// a single-line button).
    hint: String,
    state: ButtonState,
    /// This button's action is what the next map click will do.
    armed: bool,
    /// Part of the debug panel, drawn see-through so it doesn't read as game UI.
    faded: bool,
    min: Vec2,
    max: Vec2,
}

impl Button {
    fn contains(&self, point: Vec2) -> bool {
        contains(self.min, self.max, point)
    }
}

enum Shape {
    Panel {
        min: Vec2,
        max: Vec2,
        faded: bool,
    },
    Text {
        origin: Vec2,
        px: u32,
        line: Line,
    },
    Bar {
        min: Vec2,
        max: Vec2,
        fraction: f32,
    },
    Scrollbar {
        track_min: Vec2,
        track_max: Vec2,
        thumb_min: Vec2,
        thumb_max: Vec2,
    },
    QueueItem {
        min: Vec2,
        max: Vec2,
        label: String,
        active: bool,
        dragging: bool,
        drop_target: bool,
        locked: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum QueueKind {
    City,
    Barracks,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct QueueDrag {
    kind: QueueKind,
    source: usize,
    target: Option<usize>,
}

struct QueueItemSpec {
    kind: QueueKind,
    index: usize,
    label: String,
    active: bool,
    dragging: bool,
    drop_target: bool,
    locked: bool,
}

struct QueueItemRegion {
    kind: QueueKind,
    index: usize,
    min: Vec2,
    max: Vec2,
    body_max_x: f32,
    locked: bool,
}

struct QueueScrollRegion {
    kind: QueueKind,
    panel_min: Vec2,
    panel_max: Vec2,
    track_min: Vec2,
    track_max: Vec2,
    thumb_height: f32,
    max_offset: usize,
}

/// The UI for one frame. Panels also swallow clicks, so a click on the UI
/// never falls through to the map.
#[derive(Default)]
struct Layout {
    shapes: Vec<Shape>,
    buttons: Vec<Button>,
    panels: Vec<(Vec2, Vec2)>,
    queue_scrollbars: Vec<QueueScrollRegion>,
    queue_items: Vec<QueueItemRegion>,
    dock: Option<Dock>,
}

impl Layout {
    fn for_screen(screen: Vec2) -> Self {
        Self {
            dock: Some(Dock::new(
                screen,
                MARGIN,
                GAP,
                TOP_BAR_HEIGHT + QUEUE_TOP_GAP,
            )),
            ..Self::default()
        }
    }

    /// Place a measured panel in a screen zone. All docked panels compose with
    /// one another and keep their rendering and hit boxes at the same rect.
    fn dock_panel(&mut self, panel: PanelBuilder, zone: Zone) -> Option<Rect> {
        let rect = self.dock.as_mut()?.place(panel.size(), zone)?;
        panel.place_bottom_left(rect.min, self);
        Some(rect)
    }

    fn remaining_height(&self, zone: Zone, width: f32) -> f32 {
        self.dock
            .as_ref()
            .map_or(0.0, |dock| dock.remaining_height(zone, width))
    }
    fn button_at(&self, point: Vec2) -> Option<&Button> {
        self.buttons.iter().find(|b| b.contains(point))
    }

    fn covers(&self, point: Vec2) -> bool {
        self.panels
            .iter()
            .any(|&(min, max)| contains(min, max, point))
    }

    fn panel(&mut self, min: Vec2, max: Vec2, faded: bool) {
        self.shapes.push(Shape::Panel { min, max, faded });
        self.panels.push((min, max));
    }
}

impl GameState {
    /// The UI as a triangle list in UI pixels. `cursor` is in window pixels
    /// with the origin at the top-left.
    pub fn build_ui(&self, screen_size: Vec2, cursor: Option<Vec2>) -> Vec<Vertex> {
        let (layout, over_ui) = self.layout_with_hover(screen_size, cursor);
        let point = cursor.map(|c| to_ui(c, screen_size));
        let hovered = point.and_then(|p| layout.button_at(p)).map(|b| b.target);

        let mut out = Vec::new();
        for shape in &layout.shapes {
            draw_shape(shape, &mut out);
        }
        for button in &layout.buttons {
            draw_button(button, hovered == Some(button.target), &mut out);
        }
        if let Some(button) = hovered.and_then(|t| layout.buttons.iter().find(|b| b.target == t)) {
            self.draw_tooltip(button, &layout, screen_size, &mut out);
        }
        if let (Some(point), Some(hex)) = (point, self.hovered_tile)
            && self.hover_seconds >= TILE_TOOLTIP_DELAY
            && !over_ui
        {
            self.draw_tile_tooltip(hex, point, screen_size, &mut out);
        }
        out
    }

    /// Build persistent and hover panels through the same dock, so visible
    /// hover cards also participate in hit testing and collision placement.
    fn layout_with_hover(&self, screen_size: Vec2, cursor: Option<Vec2>) -> (Layout, bool) {
        let mut layout = self.layout(screen_size);
        let point = cursor.map(|c| to_ui(c, screen_size));

        // Describe a unit under the cursor, unless it's the selected one
        // (already in the tray) or the cursor is over the UI.
        let over_ui = point.is_some_and(|p| layout.covers(p));
        if let Some(idx) = cursor
            .filter(|_| !over_ui)
            .and_then(|c| self.unit_at_screen(c, screen_size))
            .filter(|&idx| Some(idx) != self.selected || self.selected_city.is_some())
            .filter(|_| {
                !self.hovered_tile.is_some_and(|hex| {
                    self.cities
                        .iter()
                        .any(|city| city.pos == hex || city.barracks == Some(hex))
                })
            })
        {
            let mut panel = PanelBuilder::default();
            self.unit_info(idx, &mut panel);
            layout.dock_panel(panel, Zone::TopRight);
        }
        // A structure's live panel is only for one the player can see now:
        // their own, or one in sight.
        let fog = self.fog();
        let known =
            |city: &super::city::City, hex: Hex| city.team == super::PLAYER_TEAM || fog.sees(hex);
        if !over_ui && let Some(hex) = self.hovered_tile {
            if let Some(city) = self
                .cities
                .iter()
                .position(|city| city.pos == hex && known(city, hex))
            {
                let mut panel = PanelBuilder::default();
                self.structure_hover_panel(city, false, &mut panel);
                layout.dock_panel(panel, Zone::BottomLeft);
            } else if let Some(city) = self
                .cities
                .iter()
                .position(|city| city.barracks == Some(hex) && known(city, hex))
            {
                let mut panel = PanelBuilder::default();
                self.structure_hover_panel(city, true, &mut panel);
                layout.dock_panel(panel, Zone::BottomLeft);
            }
        }

        (layout, over_ui)
    }

    /// Handles a click on the UI, returning whether it hit anything (in which
    /// case it shouldn't also count as a click on the map). `cursor` is in
    /// window pixels with the origin at the top-left.
    pub(super) fn click_ui(&mut self, cursor: Vec2, screen_size: Vec2) -> bool {
        let point = to_ui(cursor, screen_size);
        let layout = self.layout_with_hover(screen_size, Some(cursor)).0;
        if self.drag_queue_scrollbar_at(cursor, screen_size, false) {
            return true;
        }
        let Some(button) = layout.button_at(point) else {
            return layout.covers(point);
        };
        if button.state == ButtonState::Disabled {
            return true;
        }
        match button.target {
            Target::Unit(action) => match action {
                UnitAction::Move => self.choose_move_action(),
                UnitAction::Attack => self.choose_attack_action(),
                UnitAction::Swap => self.choose_swap_action(),
                UnitAction::Ability => self.toggle_selected_ability(),
                UnitAction::Hold => self.hold_selected_unit(),
                UnitAction::Guard => self.toggle_guard(),
                UnitAction::Settle => self.found_city_selected(),
                UnitAction::Road => self.build_worker_road_selected(),
                UnitAction::Improve => self.improve_worker_tile_selected(),
            },
            Target::Build(build) => self.queue_selected_city_unit(build),
            Target::ToggleYields => self.toggle_yields(),
            Target::Building(building) => self.queue_selected_city_building(building),
            Target::BarracksBuild(build) => self.queue_selected_barracks_unit(build),
            Target::OpenBarracks => {
                if let Some(city) = self.selected_city {
                    self.open_barracks(city)
                }
            }
            Target::OpenCity => self.open_selected_city_from_barracks(),
            Target::CityQueueRemove(index) => self.remove_selected_city_queue_item(index),
            Target::BarracksQueueRemove(index) => self.remove_selected_barracks_queue_item(index),
            Target::Focus(focus) => self.set_selected_city_focus(focus),
            Target::ConfirmBuilding(building) => self.confirm_building(building),
            Target::EndTurn => self.end_planning(),
            Target::Scenario(scenario) => self.switch_scenario(scenario),
            Target::SaveState => self.save_state(),
            Target::LoadState => self.load_state(),
            Target::CompleteProduction => self.debug_complete_current_production(),
            Target::TogglePlayback => self.toggle_instant_playback(),
            Target::ToggleFog => self.toggle_fog(),
        }
        true
    }

    /// Tracks which map hex the cursor is over (ignoring the UI) and how long
    /// it has rested there, for city hover outlines and the tile tooltip.
    /// `cursor` is in window pixels with the origin at the top-left.
    pub fn update_hover(&mut self, cursor: Option<Vec2>, screen_size: Vec2, dt: f32) {
        let layout = self.layout(screen_size);
        let hex = cursor
            .filter(|&c| !layout.covers(to_ui(c, screen_size)))
            .and_then(|c| self.hex_at_screen(c, screen_size));
        if hex == self.hovered_tile {
            self.hover_seconds += dt;
        } else {
            self.hovered_tile = hex;
            self.hover_seconds = 0.0;
        }
        self.hovered_city = hex.and_then(|h| self.cities.iter().position(|c| c.pos == h));
    }

    fn layout(&self, screen_size: Vec2) -> Layout {
        let mut layout = Layout::for_screen(screen_size);
        self.top_bar(screen_size, &mut layout);

        let mut tray = PanelBuilder::default();
        if let Some(city) = self.selected_city {
            self.city_tray(city, &mut tray);
        } else if let Some(city) = self.selected_barracks {
            self.barracks_tray(city, &mut tray);
        } else if let Some(idx) = self.selected {
            self.unit_info(idx, &mut tray);
            tray.gap(GAP);
            tray.buttons(self.unit_buttons(idx));
        } else if !self.group.is_empty() {
            self.group_tray(&mut tray);
        } else {
            self.debug_panel(&mut layout);
            return layout;
        }
        let tray_size = tray.size();
        layout.dock_panel(tray, Zone::BottomLeft);
        let queue_visible =
            queue_items_that_fit(layout.remaining_height(Zone::BottomLeft, tray_size.x));
        if let Some(city) = self.selected_city {
            let mut queue = PanelBuilder::default();
            self.city_queue_panel(city, queue_visible, &mut queue);
            if !queue.rows.is_empty() {
                layout.dock_panel(queue, Zone::BottomLeft);
            }
        } else if let Some(city) = self.selected_barracks {
            let mut queue = PanelBuilder::default();
            self.barracks_queue_panel(city, queue_visible, &mut queue);
            if !queue.rows.is_empty() {
                layout.dock_panel(queue, Zone::BottomLeft);
            }
        }
        self.debug_panel(&mut layout);
        layout
    }

    /// The unit drawn under `cursor` (window pixels, origin top-left). In a
    /// contested hex, whichever of the two is nearer the cursor.
    fn unit_at_screen(&self, cursor: Vec2, screen_size: Vec2) -> Option<usize> {
        let hex = self.hex_at_screen(cursor, screen_size)?;
        let point = self.camera.screen_to_world(cursor, screen_size);
        let fog = self.fog();
        let distance = |idx: usize| self.unit_layout(idx).0.distance(point);
        self.units_at(hex)
            .filter(|&i| fog.shows(&self.units[i]))
            .min_by(|&a, &b| distance(a).total_cmp(&distance(b)))
    }
}

fn contains(min: Vec2, max: Vec2, point: Vec2) -> bool {
    point.cmpge(min).all() && point.cmple(max).all()
}

/// Window pixels (origin top-left, Y down) to UI pixels (origin bottom-left, Y up).
fn to_ui(cursor: Vec2, screen_size: Vec2) -> Vec2 {
    Vec2::new(cursor.x, screen_size.y - cursor.y)
}

#[cfg(test)]
mod tests;
